//! Tauri 2 Rust bridge for the OpenPresenter operator control surface.
//!
//! Strictly acts as an IPC client to the decoupled `openpresenter-engine` sidecar process:
//! - Sends framed binary `EngineCommand` requests over local sockets (`interprocess`).
//! - Reads downscaled RGBA8 preview frames from lock-free shared memory (`ShmPreviewReader`)
//!   and streams them to Svelte 5 via Tauri 2 raw binary `Channel<Response>` (`ArrayBuffer`).

use core_ipc::{
    EngineCommand, EngineEvent, EngineSnapshot, EngineTelemetry, PreviewBusDescriptor,
    ShmPreviewReader, default_socket_name, read_frame, to_local_socket_name, write_frame,
};
use core_scene::{LayerKind, LookId, OutputId, SceneDiff, SceneState, SlideId, TransitionSpec};
use interprocess::local_socket::tokio::{Stream, prelude::*};
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::Duration;
use tauri::State;
use tauri::ipc::{Channel, Response};
use tokio::sync::Mutex;
use tracing::{info, warn};
use uuid::Uuid;

struct EngineBridgeState {
    socket_name: String,
    conn: Mutex<Option<Stream>>,
    preview_buses: Mutex<HashMap<OutputId, PreviewBusDescriptor>>,
}

impl EngineBridgeState {
    fn new() -> Self {
        Self {
            socket_name: default_socket_name(),
            conn: Mutex::new(None),
            preview_buses: Mutex::new(HashMap::new()),
        }
    }

    async fn try_connect_once(&self) -> Result<Stream, String> {
        let name = to_local_socket_name(&self.socket_name).map_err(|e| e.to_string())?;
        Stream::connect(name).await.map_err(|e| e.to_string())
    }

    async fn ensure_connected(&self, auto_spawn: bool) -> Result<(), String> {
        let mut guard = self.conn.lock().await;
        if guard.is_some() {
            return Ok(());
        }

        if let Ok(stream) = self.try_connect_once().await {
            *guard = Some(stream);
            return Ok(());
        }

        if !auto_spawn {
            return Err("openpresenter-engine is not running".to_string());
        }

        spawn_engine_sidecar(&self.socket_name)?;

        for _ in 0..40 {
            tokio::time::sleep(Duration::from_millis(125)).await;
            if let Ok(stream) = self.try_connect_once().await {
                *guard = Some(stream);
                return Ok(());
            }
        }

        Err(format!(
            "timed out waiting for openpresenter-engine on socket '{}'",
            self.socket_name
        ))
    }

    async fn request(&self, cmd: EngineCommand, auto_spawn: bool) -> Result<EngineEvent, String> {
        self.ensure_connected(auto_spawn).await?;
        let mut guard = self.conn.lock().await;
        let stream = guard
            .as_mut()
            .ok_or_else(|| "engine stream unavailable".to_string())?;

        if let Err(err) = write_frame(stream, &cmd).await {
            *guard = None;
            return Err(format!("IPC write failed: {err}"));
        }

        match read_frame::<_, EngineEvent>(stream).await {
            Ok(evt) => Ok(evt),
            Err(err) => {
                *guard = None;
                Err(format!("IPC read failed: {err}"))
            }
        }
    }
}

fn find_engine_binary() -> Option<PathBuf> {
    let bin_name = if cfg!(windows) {
        "openpresenter-engine.exe"
    } else {
        "openpresenter-engine"
    };

    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let sibling = dir.join(bin_name);
            if sibling.exists() {
                return Some(sibling);
            }
            // During `cargo` / `tauri dev`, workspace engine binary lives in `<root>/target/debug/`
            for ancestor in dir.ancestors().take(6) {
                let candidate = ancestor.join("target").join("debug").join(bin_name);
                if candidate.exists() {
                    return Some(candidate);
                }
                let release_candidate = ancestor.join("target").join("release").join(bin_name);
                if release_candidate.exists() {
                    return Some(release_candidate);
                }
            }
        }
    }

    if let Ok(cwd) = std::env::current_dir() {
        for ancestor in cwd.ancestors().take(5) {
            let candidate = ancestor.join("target").join("debug").join(bin_name);
            if candidate.exists() {
                return Some(candidate);
            }
        }
    }

    None
}

fn spawn_engine_sidecar(socket_name: &str) -> Result<(), String> {
    let bin = find_engine_binary().ok_or_else(|| {
        "could not locate `openpresenter-engine` binary; run `cargo build --bin openpresenter-engine` first"
            .to_string()
    })?;

    info!("spawning decoupled engine sidecar: {}", bin.display());
    Command::new(&bin)
        .arg("--socket")
        .arg(socket_name)
        .arg("--outputs")
        .arg("2")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|e| format!("failed to spawn {}: {e}", bin.display()))?;

    Ok(())
}

#[tauri::command]
async fn connect_engine(
    bridge: State<'_, Arc<EngineBridgeState>>,
) -> Result<EngineSnapshot, String> {
    match bridge.request(EngineCommand::GetSnapshot, true).await? {
        EngineEvent::Snapshot(snap) => {
            let mut map = bridge.preview_buses.lock().await;
            map.clear();
            for bus in &snap.preview_buses {
                map.insert(bus.output_id, bus.clone());
            }
            Ok(*snap)
        }
        other => Err(format!("unexpected response to GetSnapshot: {other:?}")),
    }
}

#[tauri::command]
async fn get_telemetry(
    bridge: State<'_, Arc<EngineBridgeState>>,
) -> Result<EngineTelemetry, String> {
    match bridge.request(EngineCommand::GetTelemetry, false).await? {
        EngineEvent::Telemetry(t) => Ok(t),
        other => Err(format!("unexpected response to GetTelemetry: {other:?}")),
    }
}

async fn apply_scene_diff(
    bridge: &EngineBridgeState,
    diff: SceneDiff,
) -> Result<SceneState, String> {
    match bridge.request(EngineCommand::ApplyDiff(diff), false).await? {
        EngineEvent::StateUpdated(state) => Ok(*state),
        EngineEvent::Error(msg) => Err(msg),
        other => Err(format!("unexpected response to ApplyDiff: {other:?}")),
    }
}

#[tauri::command]
async fn trigger_slide(
    bridge: State<'_, Arc<EngineBridgeState>>,
    slide_id: String,
    duration_ms: u32,
) -> Result<SceneState, String> {
    let id = SlideId(Uuid::parse_str(&slide_id).map_err(|e| e.to_string())?);
    let transition = if duration_ms == 0 {
        TransitionSpec::Cut
    } else {
        TransitionSpec::Dissolve { duration_ms }
    };
    apply_scene_diff(
        &bridge,
        SceneDiff::TriggerSlide {
            slide_id: id,
            transition_override: Some(transition),
        },
    )
    .await
}

#[tauri::command]
async fn clear_layer(
    bridge: State<'_, Arc<EngineBridgeState>>,
    layer: LayerKind,
) -> Result<SceneState, String> {
    apply_scene_diff(&bridge, SceneDiff::ClearLayer(layer)).await
}

#[tauri::command]
async fn clear_all(bridge: State<'_, Arc<EngineBridgeState>>) -> Result<SceneState, String> {
    apply_scene_diff(&bridge, SceneDiff::ClearAll).await
}

#[tauri::command]
async fn set_blackout(
    bridge: State<'_, Arc<EngineBridgeState>>,
    enabled: bool,
) -> Result<SceneState, String> {
    apply_scene_diff(&bridge, SceneDiff::SetBlackout(enabled)).await
}

#[tauri::command]
async fn toggle_overlay(
    bridge: State<'_, Arc<EngineBridgeState>>,
    active: bool,
) -> Result<SceneState, String> {
    apply_scene_diff(&bridge, SceneDiff::ToggleOverlayLayer(active)).await
}

#[tauri::command]
async fn set_output_look(
    bridge: State<'_, Arc<EngineBridgeState>>,
    output_id: String,
    look_id: String,
) -> Result<SceneState, String> {
    let oid = OutputId(Uuid::parse_str(&output_id).map_err(|e| e.to_string())?);
    let lid = LookId(Uuid::parse_str(&look_id).map_err(|e| e.to_string())?);
    apply_scene_diff(
        &bridge,
        SceneDiff::SetOutputLook {
            output_id: oid,
            look_id: lid,
        },
    )
    .await
}

#[tauri::command]
async fn shutdown_engine(bridge: State<'_, Arc<EngineBridgeState>>) -> Result<(), String> {
    let _ = bridge.request(EngineCommand::Shutdown, false).await?;
    let mut guard = bridge.conn.lock().await;
    *guard = None;
    Ok(())
}

/// Streams raw binary RGBA8 preview frames from the engine's shared-memory ring into
/// the Svelte 5 `<canvas>` via a Tauri 2 binary `Channel<Response>` (`ArrayBuffer`).
#[tauri::command]
async fn start_preview_stream(
    bridge: State<'_, Arc<EngineBridgeState>>,
    output_id: String,
    on_frame: Channel<Response>,
) -> Result<(), String> {
    let oid = OutputId(Uuid::parse_str(&output_id).map_err(|e| e.to_string())?);
    let bus = {
        let map = bridge.preview_buses.lock().await;
        map.get(&oid).cloned()
    }
    .ok_or_else(|| format!("no preview bus registered for output {output_id}"))?;

    tokio::spawn(async move {
        let mut reader = match ShmPreviewReader::open(&bus.shm_path) {
            Ok(r) => r,
            Err(err) => {
                warn!("failed to open SHM preview ring '{}': {err}", bus.shm_path);
                return;
            }
        };

        let mut rgba = Vec::new();
        let mut tick = tokio::time::interval(Duration::from_millis(33));

        loop {
            tick.tick().await;
            if let Some(meta) = reader.try_read_latest(&mut rgba) {
                let mut packet = Vec::with_capacity(8 + rgba.len());
                packet.extend_from_slice(&meta.width.to_le_bytes());
                packet.extend_from_slice(&meta.height.to_le_bytes());
                packet.extend_from_slice(&rgba);
                if on_frame.send(Response::new(packet)).is_err() {
                    break;
                }
            }
        }
    });

    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .init();

    let bridge = Arc::new(EngineBridgeState::new());

    tauri::Builder::default()
        .manage(bridge)
        .invoke_handler(tauri::generate_handler![
            connect_engine,
            get_telemetry,
            trigger_slide,
            clear_layer,
            clear_all,
            set_blackout,
            toggle_overlay,
            set_output_look,
            shutdown_engine,
            start_preview_stream,
        ])
        .run(tauri::generate_context!())
        .expect("error while running OpenPresenter desktop control surface");
}
