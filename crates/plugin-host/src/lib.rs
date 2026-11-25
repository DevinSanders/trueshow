//! Two-Tier Plugin Host for TrueShow:
//! - **Tier 1 (`SidecarSupervisor`):** Capability-enforced registry & crash-restart governor for
//!   out-of-process native/ML sidecars (Whisper STT, Art-Net/sACN lighting, custom hardware).
//! - **Tier 2 (`WasmSandboxHost`):** Embedded `wasmtime` sandbox with strict **fuel metering**
//!   so an infinite loop or trap inside a community `.wasm` plugin can never hang live outputs.

use plugin_sdk::{PluginCapability, PluginManifest, PluginTier};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};
use thiserror::Error;
use wasmtime::{Config, Engine, Instance, Module, Store};

#[derive(Debug, Error)]
pub enum PluginHostError {
    #[error("plugin '{0}' is not registered")]
    NotFound(String),
    #[error("plugin '{plugin_id}' lacks required capability {capability:?}")]
    PermissionDenied {
        plugin_id: String,
        capability: PluginCapability,
    },
    #[error("plugin '{0}' exceeded maximum restart budget ({1} crashes)")]
    CrashBudgetExhausted(String, u32),
    #[error("wrong plugin tier for host: expected {expected:?}, got {actual:?}")]
    WrongTier {
        expected: PluginTier,
        actual: PluginTier,
    },
    #[error("Wasmtime execution error or fuel trap: {0}")]
    WasmRuntime(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegisteredPluginInfo {
    pub manifest: PluginManifest,
    pub granted_capabilities: Vec<PluginCapability>,
    pub restart_count: u32,
    pub healthy: bool,
}

struct SidecarEntry {
    manifest: PluginManifest,
    granted: HashSet<PluginCapability>,
    restart_count: u32,
    max_restarts: u32,
    healthy: bool,
}

/// Tier 1 Sidecar supervisor enforcing operator capability grants and crash-loop protection.
pub struct SidecarSupervisor {
    entries: BTreeMap<String, SidecarEntry>,
    default_max_restarts: u32,
}

impl Default for SidecarSupervisor {
    fn default() -> Self {
        Self::new(3)
    }
}

impl SidecarSupervisor {
    pub fn new(default_max_restarts: u32) -> Self {
        Self {
            entries: BTreeMap::new(),
            default_max_restarts: default_max_restarts.max(1),
        }
    }

    pub fn register(
        &mut self,
        manifest: PluginManifest,
        operator_approved: &[PluginCapability],
    ) -> Result<(), PluginHostError> {
        if manifest.tier != PluginTier::SidecarProcess {
            return Err(PluginHostError::WrongTier {
                expected: PluginTier::SidecarProcess,
                actual: manifest.tier,
            });
        }
        // Effective permissions = intersection(manifest.capabilities, operator_approved)
        let granted: HashSet<PluginCapability> = operator_approved
            .iter()
            .copied()
            .filter(|c| manifest.has_capability(*c))
            .collect();

        self.entries.insert(
            manifest.id.clone(),
            SidecarEntry {
                manifest,
                granted,
                restart_count: 0,
                max_restarts: self.default_max_restarts,
                healthy: true,
            },
        );
        Ok(())
    }

    pub fn require_capability(
        &self,
        plugin_id: &str,
        capability: PluginCapability,
    ) -> Result<(), PluginHostError> {
        let entry = self
            .entries
            .get(plugin_id)
            .ok_or_else(|| PluginHostError::NotFound(plugin_id.to_string()))?;
        if entry.granted.contains(&capability) {
            Ok(())
        } else {
            Err(PluginHostError::PermissionDenied {
                plugin_id: plugin_id.to_string(),
                capability,
            })
        }
    }

    /// Records a sidecar crash and decides whether the supervisor should auto-restart it.
    pub fn record_crash(&mut self, plugin_id: &str) -> Result<u32, PluginHostError> {
        let entry = self
            .entries
            .get_mut(plugin_id)
            .ok_or_else(|| PluginHostError::NotFound(plugin_id.to_string()))?;
        entry.restart_count += 1;
        if entry.restart_count > entry.max_restarts {
            entry.healthy = false;
            return Err(PluginHostError::CrashBudgetExhausted(
                plugin_id.to_string(),
                entry.max_restarts,
            ));
        }
        Ok(entry.restart_count)
    }

    pub fn list(&self) -> Vec<RegisteredPluginInfo> {
        self.entries
            .values()
            .map(|e| {
                let mut caps: Vec<PluginCapability> = e.granted.iter().copied().collect();
                caps.sort();
                RegisteredPluginInfo {
                    manifest: e.manifest.clone(),
                    granted_capabilities: caps,
                    restart_count: e.restart_count,
                    healthy: e.healthy,
                }
            })
            .collect()
    }
}

/// Tier 2 Sandboxed WebAssembly host powered by `wasmtime` with deterministic fuel limits.
pub struct WasmSandboxHost {
    engine: Engine,
    fuel_budget: u64,
}

impl WasmSandboxHost {
    pub fn new(fuel_budget: u64) -> Result<Self, PluginHostError> {
        let mut config = Config::new();
        config.consume_fuel(true);
        let engine =
            Engine::new(&config).map_err(|e| PluginHostError::WasmRuntime(e.to_string()))?;
        Ok(Self {
            engine,
            fuel_budget: fuel_budget.max(1_000),
        })
    }

    /// Executes an exported `(i32, i32) -> i32` plugin hook inside an isolated fuel-metered store.
    /// Zero host filesystem, env, or network imports are exposed to the sandbox.
    pub fn run_i32_hook(
        &self,
        wasm_or_wat: &[u8],
        export_name: &str,
        arg0: i32,
        arg1: i32,
    ) -> Result<i32, PluginHostError> {
        let module = Module::new(&self.engine, wasm_or_wat)
            .map_err(|e| PluginHostError::WasmRuntime(e.to_string()))?;
        let mut store = Store::new(&self.engine, ());
        store
            .set_fuel(self.fuel_budget)
            .map_err(|e| PluginHostError::WasmRuntime(e.to_string()))?;

        let instance = Instance::new(&mut store, &module, &[])
            .map_err(|e| PluginHostError::WasmRuntime(e.to_string()))?;
        let func = instance
            .get_typed_func::<(i32, i32), i32>(&mut store, export_name)
            .map_err(|e| PluginHostError::WasmRuntime(e.to_string()))?;

        func.call(&mut store, (arg0, arg1))
            .map_err(|e| PluginHostError::WasmRuntime(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sidecar_supervisor_enforces_capability_intersection_and_crash_budget() {
        let mut sup = SidecarSupervisor::new(2);
        let manifest = PluginManifest {
            id: "org.trueshow.whisper".into(),
            name: "Local Whisper STT".into(),
            version: "0.1.0".into(),
            tier: PluginTier::SidecarProcess,
            entrypoint: "whisper-stt".into(),
            capabilities: vec![PluginCapability::AudioTap, PluginCapability::CueTrigger],
        };

        // Wrong tier rejected
        let mut bad_tier = manifest.clone();
        bad_tier.tier = PluginTier::WasmComponent;
        assert!(matches!(
            sup.register(bad_tier, &[PluginCapability::AudioTap]),
            Err(PluginHostError::WrongTier { .. })
        ));

        // Grant only AudioTap (not CueTrigger, and NetworkLan wasn't even declared)
        sup.register(
            manifest,
            &[PluginCapability::AudioTap, PluginCapability::NetworkLan],
        )
        .expect("register");

        assert!(
            sup.require_capability("org.trueshow.whisper", PluginCapability::AudioTap)
                .is_ok()
        );
        assert!(matches!(
            sup.require_capability("org.trueshow.whisper", PluginCapability::CueTrigger),
            Err(PluginHostError::PermissionDenied { .. })
        ));
        assert!(matches!(
            sup.require_capability("missing.plugin", PluginCapability::AudioTap),
            Err(PluginHostError::NotFound(_))
        ));

        assert_eq!(sup.record_crash("org.trueshow.whisper").expect("c1"), 1);
        assert_eq!(sup.record_crash("org.trueshow.whisper").expect("c2"), 2);
        assert!(matches!(
            sup.record_crash("org.trueshow.whisper"),
            Err(PluginHostError::CrashBudgetExhausted(_, 2))
        ));

        let list = sup.list();
        assert_eq!(list.len(), 1);
        assert!(!list[0].healthy);
    }

    #[test]
    fn wasmtime_sandbox_runs_valid_plugin_and_traps_infinite_loop_via_fuel() {
        let host = WasmSandboxHost::new(10_000).expect("wasmtime host");

        // Happy path: valid sandboxed plugin hook
        let valid_wat = br#"
            (module
              (func (export "on_cue") (param i32 i32) (result i32)
                local.get 0
                local.get 1
                i32.add))
        "#;
        let sum = host
            .run_i32_hook(valid_wat, "on_cue", 19, 23)
            .expect("valid hook succeeds");
        assert_eq!(sum, 42);

        // Failure path 1: infinite loop trapped cleanly by fuel exhaustion (never hangs host!)
        let rogue_wat = br#"
            (module
              (func (export "on_cue") (param i32 i32) (result i32)
                (loop $spin
                  br $spin)
                i32.const 0))
        "#;
        let err = host
            .run_i32_hook(rogue_wat, "on_cue", 1, 2)
            .expect_err("infinite loop must trap on fuel");
        assert!(err.to_string().to_lowercase().contains("fuel"));

        // Failure path 2: missing export rejected cleanly
        assert!(
            host.run_i32_hook(valid_wat, "does_not_exist", 0, 0)
                .is_err()
        );
    }
}
