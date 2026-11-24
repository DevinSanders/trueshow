//! Tauri 2 Rust bridge for the OpenPresenter operator control surface (M1).
//!
//! Strictly acts as an IPC client to `openpresenter-engine` + local offline SQLite library host:
//! - Control plane: framed binary `EngineCommand` over `interprocess` local sockets.
//! - Preview plane: lock-free shared-memory RGBA8 ring streamed via Tauri `Channel<Response>`.
//! - Local library: offline SQLite WAL + FTS5 persistence (`core-library`).

use core_ipc::{
    EngineCommand, EngineEvent, EngineSnapshot, EngineTelemetry, PreviewBusDescriptor,
    ShmPreviewReader, default_socket_name, read_frame, to_local_socket_name, write_frame,
};
use core_library::{LibraryStore, PlaylistRecord, SongRecord};
use core_scene::{
    LayerKind, LookId, MaskStyle, OutputId, SceneDiff, SceneState, Slide, SlideElement, SlideId,
    TransitionSpec,
};
use interprocess::local_socket::tokio::{Stream, prelude::*};
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex as StdMutex};
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
    library: StdMutex<LibraryStore>,
}

impl EngineBridgeState {
    fn new() -> Self {
        let db_path = std::env::temp_dir()
            .join("openpresenter-data")
            .join("library.sqlite3");
        let store = LibraryStore::open(&db_path)
            .or_else(|_| LibraryStore::open_in_memory())
            .expect("open local SQLite library");
        let _ = store.seed_demo_if_empty();

        Self {
            socket_name: default_socket_name(),
            conn: Mutex::new(None),
            preview_buses: Mutex::new(HashMap::new()),
            library: StdMutex::new(store),
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

    if let Ok(exe) = std::env::current_exe()
        && let Some(dir) = exe.parent()
    {
        let sibling = dir.join(bin_name);
        if sibling.exists() {
            return Some(sibling);
        }
        for ancestor in dir.ancestors().take(6) {
            let debug_bin = ancestor.join("target").join("debug").join(bin_name);
            if debug_bin.exists() {
                return Some(debug_bin);
            }
            let release_bin = ancestor.join("target").join("release").join(bin_name);
            if release_bin.exists() {
                return Some(release_bin);
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
    match bridge
        .request(EngineCommand::ApplyDiff(Box::new(diff)), false)
        .await?
    {
        EngineEvent::StateUpdated(state) => Ok(*state),
        EngineEvent::Error(msg) => Err(msg),
        other => Err(format!("unexpected response to ApplyDiff: {other:?}")),
    }
}

#[tauri::command]
async fn trigger_slide(
    bridge: State<'_, Arc<EngineBridgeState>>,
    slide_id: String,
    transition_kind: Option<String>,
    duration_ms: u32,
) -> Result<SceneState, String> {
    let id = SlideId(Uuid::parse_str(&slide_id).map_err(|e| e.to_string())?);
    let transition = match transition_kind.as_deref().unwrap_or("Dissolve") {
        "Cut" => TransitionSpec::Cut,
        "PushLeft" => TransitionSpec::PushLeft { duration_ms },
        "WipeRight" => TransitionSpec::WipeRight { duration_ms },
        _ if duration_ms == 0 => TransitionSpec::Cut,
        _ => TransitionSpec::Dissolve { duration_ms },
    };
    let state = apply_scene_diff(
        &bridge,
        SceneDiff::TriggerSlide {
            slide_id: id,
            transition_override: Some(transition),
        },
    )
    .await?;

    // Automatic CCLI projection audit logging when a song slide with a CCLI number goes live.
    if let Some(slide) = &state.active_slide
        && let Some(ccli) = &slide.ccli_number
        && let Ok(lib) = bridge.library.lock()
    {
        let _ = lib.record_ccli_usage("Default Service", &slide.label, ccli, "", 60);
    }

    Ok(state)
}

#[tauri::command]
async fn update_slide_element(
    bridge: State<'_, Arc<EngineBridgeState>>,
    slide_id: String,
    element: SlideElement,
) -> Result<SceneState, String> {
    let id = SlideId(Uuid::parse_str(&slide_id).map_err(|e| e.to_string())?);
    apply_scene_diff(
        &bridge,
        SceneDiff::UpdateElement {
            slide_id: id,
            element,
        },
    )
    .await
}

#[tauri::command]
async fn upsert_slide(
    bridge: State<'_, Arc<EngineBridgeState>>,
    slide: Slide,
) -> Result<SceneState, String> {
    apply_scene_diff(&bridge, SceneDiff::UpsertSlide { slide }).await
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
async fn set_mask(
    bridge: State<'_, Arc<EngineBridgeState>>,
    mask: MaskStyle,
) -> Result<SceneState, String> {
    apply_scene_diff(&bridge, SceneDiff::SetMask(mask)).await
}

#[tauri::command]
async fn set_stage_message(
    bridge: State<'_, Arc<EngineBridgeState>>,
    message: Option<String>,
) -> Result<SceneState, String> {
    apply_scene_diff(&bridge, SceneDiff::SetStageMessage(message)).await
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
fn search_library_songs(
    bridge: State<'_, Arc<EngineBridgeState>>,
    query: String,
) -> Result<Vec<SongRecord>, String> {
    let lib = bridge.library.lock().map_err(|e| e.to_string())?;
    lib.search_songs(&query, 50).map_err(|e| e.to_string())
}

#[tauri::command]
fn list_playlists(
    bridge: State<'_, Arc<EngineBridgeState>>,
) -> Result<Vec<PlaylistRecord>, String> {
    let lib = bridge.library.lock().map_err(|e| e.to_string())?;
    lib.list_playlists().map_err(|e| e.to_string())
}

#[tauri::command]
async fn shutdown_engine(bridge: State<'_, Arc<EngineBridgeState>>) -> Result<(), String> {
    let _ = bridge.request(EngineCommand::Shutdown, false).await?;
    let mut guard = bridge.conn.lock().await;
    *guard = None;
    Ok(())
}

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
            update_slide_element,
            upsert_slide,
            clear_layer,
            clear_all,
            set_blackout,
            toggle_overlay,
            set_mask,
            set_stage_message,
            set_output_look,
            search_library_songs,
            list_playlists,
            shutdown_engine,
            start_preview_stream,
        ])
        .run(tauri::generate_context!())
        .expect("error while running OpenPresenter desktop control surface");
}
