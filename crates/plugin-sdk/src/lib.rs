//! Permissively licensed (`MIT OR Apache-2.0`) SDK contract for OpenPresenter plugins.

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum PluginSdkError {
    #[error("manifest TOML parse error: {0}")]
    Toml(#[from] toml::de::Error),
    #[error("invalid plugin manifest: {0}")]
    InvalidManifest(String),
    #[error("JSON-RPC error: {0}")]
    Json(#[from] serde_json::Error),
}

/// Two-tier plugin execution boundary (ADR / 02-workspace-and-plugin-design.md).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginTier {
    /// Out-of-process native/Python/ML sidecar (Whisper STT, Art-Net/sACN DMX, custom hardware).
    SidecarProcess,
    /// Sandboxed WebAssembly component (`wasmtime` + WIT) for importers, widgets, and macros.
    WasmComponent,
}

/// Capability-based permissions declared in `plugin.toml` and explicitly approved by the operator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginCapability {
    AudioTap,
    OverlayPublish,
    CueTrigger,
    FileImport,
    NetworkLan,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginManifest {
    pub id: String,
    pub name: String,
    pub version: String,
    pub tier: PluginTier,
    pub entrypoint: String,
    #[serde(default)]
    pub capabilities: Vec<PluginCapability>,
}

impl PluginManifest {
    pub fn from_toml_str(src: &str) -> Result<Self, PluginSdkError> {
        let manifest: Self = toml::from_str(src)?;
        manifest.validate()?;
        Ok(manifest)
    }

    pub fn validate(&self) -> Result<(), PluginSdkError> {
        if self.id.trim().is_empty()
            || !self
                .id
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
        {
            return Err(PluginSdkError::InvalidManifest(
                "plugin id must be non-empty ASCII alphanumeric/./-/_".into(),
            ));
        }
        if self.name.trim().is_empty() || self.entrypoint.trim().is_empty() {
            return Err(PluginSdkError::InvalidManifest(
                "plugin name and entrypoint are required".into(),
            ));
        }
        Ok(())
    }

    pub fn has_capability(&self, cap: PluginCapability) -> bool {
        self.capabilities.contains(&cap)
    }
}

/// JSON-RPC 2.0 request frame exchanged with Tier 1 sidecar plugins over local sockets.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PluginRpcRequest {
    pub jsonrpc: String,
    pub id: u64,
    pub method: String,
    pub params: serde_json::Value,
}

impl PluginRpcRequest {
    pub fn new(id: u64, method: impl Into<String>, params: serde_json::Value) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            method: method.into(),
            params,
        }
    }
}

/// JSON-RPC 2.0 response frame returned by Tier 1 sidecar plugins.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PluginRpcResponse {
    pub jsonrpc: String,
    pub id: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl PluginRpcResponse {
    pub fn ok(id: u64, result: serde_json::Value) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: Some(result),
            error: None,
        }
    }

    pub fn err(id: u64, message: impl Into<String>) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: None,
            error: Some(message.into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_valid_manifests_and_rejects_invalid_ids_and_rpc_roundtrip() {
        let toml_src = r#"
            id = "org.openpresenter.whisper-stt"
            name = "Local Whisper Auto-Scripture"
            version = "0.1.0"
            tier = "sidecar_process"
            entrypoint = "whisper-sidecar"
            capabilities = ["audio_tap", "cue_trigger"]
        "#;
        let m = PluginManifest::from_toml_str(toml_src).expect("parse manifest");
        assert_eq!(m.tier, PluginTier::SidecarProcess);
        assert!(m.has_capability(PluginCapability::AudioTap));
        assert!(!m.has_capability(PluginCapability::NetworkLan));

        // Failure paths
        assert!(
            PluginManifest::from_toml_str(
                r#"id = "bad id!"
                name = "X"
                version = "0.1.0"
                tier = "wasm_component"
                entrypoint = "p.wasm""#
            )
            .is_err()
        );
        assert!(
            PluginManifest::from_toml_str(
                r#"id = "ok.id"
                name = "   "
                version = "0.1.0"
                tier = "wasm_component"
                entrypoint = "p.wasm""#
            )
            .is_err()
        );

        let req = PluginRpcRequest::new(1, "on_cue", serde_json::json!({"slide": "s1"}));
        let raw = serde_json::to_string(&req).expect("serialize");
        let back: PluginRpcRequest = serde_json::from_str(&raw).expect("deserialize");
        assert_eq!(back, req);

        let ok = PluginRpcResponse::ok(1, serde_json::json!({"accepted": true}));
        let err = PluginRpcResponse::err(2, "permission denied");
        assert!(ok.error.is_none());
        assert_eq!(err.error.as_deref(), Some("permission denied"));
    }
}
