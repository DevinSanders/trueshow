//! Tauri 2 Rust bridge for the OpenPresenter operator control surface (M1).
//!
//! Strictly acts as an IPC client to `openpresenter-engine` + local offline SQLite library host:
//! - Control plane: framed binary `EngineCommand` over `interprocess` local sockets.
//! - Preview plane: lock-free shared-memory RGBA8 ring streamed via Tauri `Channel<Response>`.
//! - Local library: offline SQLite WAL + FTS5 persistence (`core-library`).

use core_diagnostics::{
    CrashReport, DiagnosticsPaths, ExportedBundle, LogLevel, LoggingController,
};
use core_ipc::{
    DisplayInfo, EngineCommand, EngineEvent, EngineSnapshot, EngineTelemetry, PreviewBusDescriptor,
    ShmPreviewReader, default_socket_name, read_frame, to_local_socket_name, write_frame,
};
use core_library::{LibraryStore, PlaylistRecord, SongRecord};
use core_scene::{
    LOOK_AUDIENCE_FULL, LOOK_STAGE_CONFIDENCE, LayerKind, LookId, MaskStyle, MonitorRegion,
    OutputConfig, OutputId, OutputKind, OutputPlacement, SceneDiff, SceneState, Slide,
    SlideElement, SlideId, StageLayoutConfig, TransitionSpec, split_monitor_regions,
};
use interprocess::local_socket::tokio::{Stream, prelude::*};
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;
use tauri::State;
use tauri::ipc::Response;
use tokio::sync::Mutex;
use tracing::{error, info, warn};
use uuid::Uuid;

struct EngineBridgeState {
    socket_name: String,
    conn: Mutex<Option<Stream>>,
    preview_buses: Mutex<HashMap<OutputId, PreviewBusDescriptor>>,
    preview_readers: StdMutex<HashMap<OutputId, (String, ShmPreviewReader)>>,
    library: StdMutex<LibraryStore>,
    timers: StdMutex<show_control::TimerEngine>,
    diagnostics: DiagnosticsPaths,
    logging: Option<LoggingController>,
    last_gpu_summary: StdMutex<Option<String>>,
}

impl EngineBridgeState {
    fn new(diagnostics: DiagnosticsPaths, logging: Option<LoggingController>) -> Self {
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
            preview_readers: StdMutex::new(HashMap::new()),
            library: StdMutex::new(store),
            timers: StdMutex::new(show_control::TimerEngine::new()),
            diagnostics,
            logging,
            last_gpu_summary: StdMutex::new(None),
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
        let log_level = self
            .logging
            .as_ref()
            .map(|l| l.current_level())
            .unwrap_or(LogLevel::Info);
        spawn_engine_sidecar(&self.socket_name, log_level, self.diagnostics.clone())?;
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

fn spawn_engine_sidecar(
    socket_name: &str,
    log_level: LogLevel,
    diag_paths: DiagnosticsPaths,
) -> Result<(), String> {
    let bin = find_engine_binary().ok_or_else(|| {
        "could not locate `openpresenter-engine` binary; run `cargo build --bin openpresenter-engine` first"
            .to_string()
    })?;
    info!(
        path = %bin.display(),
        level = log_level.as_str(),
        "spawning decoupled engine sidecar"
    );
    let mut child = Command::new(&bin)
        .arg("--socket")
        .arg(socket_name)
        .arg("--outputs")
        .arg("2")
        .arg("--log-level")
        .arg(log_level.as_str())
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|e| format!("failed to spawn {}: {e}", bin.display()))?;

    let child_pid = child.id();
    std::thread::Builder::new()
        .name(format!("engine-exit-watcher-{child_pid}"))
        .spawn(move || match child.wait() {
            Ok(status) if status.success() => {
                info!(pid = child_pid, "openpresenter-engine exited cleanly");
            }
            Ok(status) => {
                let msg = format!(
                    "openpresenter-engine (pid={child_pid}) exited abnormally with status: {status}"
                );
                error!("{msg}");
                let _ = core_diagnostics::record_runtime_error(
                    &diag_paths,
                    "openpresenter-engine",
                    "sidecar-abnormal-exit",
                    env!("CARGO_PKG_VERSION"),
                    &msg,
                    Some("apps/desktop/src-tauri/src/lib.rs"),
                    Some("Process terminated unexpectedly; inspect recent_logs for engine stderr/trace."),
                );
            }
            Err(err) => {
                warn!(pid = child_pid, "failed waiting on engine sidecar: {err}");
            }
        })
        .ok();
    Ok(())
}

#[tauri::command]
async fn connect_engine(
    bridge: State<'_, Arc<EngineBridgeState>>,
) -> Result<EngineSnapshot, String> {
    let event = match bridge.request(EngineCommand::GetSnapshot, true).await {
        Ok(evt) => evt,
        Err(_) => bridge.request(EngineCommand::GetSnapshot, true).await?,
    };
    match event {
        EngineEvent::Snapshot(snap) => {
            if let Ok(mut gpu) = bridge.last_gpu_summary.lock() {
                *gpu = Some(format!(
                    "{} ({}) · PID {} · {} display(s)",
                    snap.gpu_adapter_name,
                    snap.gpu_backend,
                    snap.engine_pid,
                    snap.displays.len()
                ));
            }
            let mut map = bridge.preview_buses.lock().await;
            map.clear();
            for bus in &snap.preview_buses {
                map.insert(bus.output_id, bus.clone());
            }
            if let Ok(mut readers) = bridge.preview_readers.lock() {
                readers.clear();
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

async fn refresh_snapshot_and_buses(
    bridge: &EngineBridgeState,
    expected_output_count: Option<usize>,
) -> Result<EngineSnapshot, String> {
    // Allow the compositor main thread a short window to spawn/close OS windows and SHM rings.
    let mut last_snap = None;
    for attempt in 0..12 {
        match bridge.request(EngineCommand::GetSnapshot, false).await? {
            EngineEvent::Snapshot(snap) => {
                let synced = match expected_output_count {
                    Some(n) => snap.preview_buses.len() == n,
                    None => true,
                };
                last_snap = Some(*snap);
                if synced || attempt == 11 {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(35)).await;
            }
            other => return Err(format!("unexpected response to GetSnapshot: {other:?}")),
        }
    }
    let snap = last_snap.ok_or_else(|| "engine snapshot unavailable".to_string())?;
    let mut map = bridge.preview_buses.lock().await;
    map.clear();
    for bus in &snap.preview_buses {
        map.insert(bus.output_id, bus.clone());
    }
    if let Ok(mut readers) = bridge.preview_readers.lock() {
        readers.retain(|oid, _| map.contains_key(oid));
    }
    Ok(snap)
}

#[derive(serde::Deserialize)]
struct OutputScreenInput {
    id: Option<String>,
    name: String,
    kind: OutputKind,
    monitor_index: Option<usize>,
    width: u32,
    height: u32,
    placement_mode: String, // "windowed" | "fullscreen" | "region"
    region_x: Option<i32>,
    region_y: Option<i32>,
    region_width: Option<u32>,
    region_height: Option<u32>,
    look_id: Option<String>,
}

#[tauri::command]
async fn list_physical_displays(
    bridge: State<'_, Arc<EngineBridgeState>>,
) -> Result<Vec<DisplayInfo>, String> {
    match bridge.request(EngineCommand::ListDisplays, false).await? {
        EngineEvent::Displays(d) => Ok(d),
        other => Err(format!("unexpected response to ListDisplays: {other:?}")),
    }
}

#[tauri::command]
async fn upsert_output_screen(
    bridge: State<'_, Arc<EngineBridgeState>>,
    screen: OutputScreenInput,
) -> Result<EngineSnapshot, String> {
    let id = match screen.id.as_deref().map(str::trim) {
        Some(raw) if !raw.is_empty() => OutputId(Uuid::parse_str(raw).map_err(|e| e.to_string())?),
        _ => OutputId::new(),
    };
    let look_id = match screen.look_id.as_deref().map(str::trim) {
        Some(raw) if !raw.is_empty() => LookId(Uuid::parse_str(raw).map_err(|e| e.to_string())?),
        _ => match screen.kind {
            OutputKind::Audience => LOOK_AUDIENCE_FULL,
            OutputKind::Stage => LOOK_STAGE_CONFIDENCE,
        },
    };
    let (placement, fullscreen, width, height) = match screen.placement_mode.as_str() {
        "fullscreen" => (
            OutputPlacement::Fullscreen,
            true,
            screen.width.max(320),
            screen.height.max(180),
        ),
        "region" => {
            let rw = screen.region_width.unwrap_or(screen.width).max(64);
            let rh = screen.region_height.unwrap_or(screen.height).max(64);
            let reg = MonitorRegion {
                x: screen.region_x.unwrap_or(0),
                y: screen.region_y.unwrap_or(0),
                width: rw,
                height: rh,
            };
            (OutputPlacement::Region(reg), false, rw, rh)
        }
        _ => (
            OutputPlacement::Windowed,
            false,
            screen.width.max(320),
            screen.height.max(180),
        ),
    };

    let cfg = OutputConfig {
        id,
        name: screen.name.trim().to_string(),
        kind: screen.kind,
        monitor_index: screen.monitor_index,
        width,
        height,
        fullscreen,
        placement,
        look_id,
        stage_layout: (screen.kind == OutputKind::Stage).then(StageLayoutConfig::default),
    };

    let state = apply_scene_diff(&bridge, SceneDiff::UpsertOutput(cfg)).await?;
    refresh_snapshot_and_buses(&bridge, Some(state.outputs.len())).await
}

#[tauri::command]
async fn remove_output_screen(
    bridge: State<'_, Arc<EngineBridgeState>>,
    output_id: String,
) -> Result<EngineSnapshot, String> {
    let oid = OutputId(Uuid::parse_str(&output_id).map_err(|e| e.to_string())?);
    let state = apply_scene_diff(&bridge, SceneDiff::RemoveOutput(oid)).await?;
    refresh_snapshot_and_buses(&bridge, Some(state.outputs.len())).await
}

#[tauri::command]
async fn split_monitor_into_screens(
    bridge: State<'_, Arc<EngineBridgeState>>,
    monitor_index: usize,
    cols: u32,
    rows: u32,
    default_kind: OutputKind,
    replace_existing_on_monitor: bool,
) -> Result<EngineSnapshot, String> {
    let snap = refresh_snapshot_and_buses(&bridge, None).await?;
    let display = snap
        .displays
        .get(monitor_index)
        .ok_or_else(|| format!("physical display #{monitor_index} not found"))?
        .clone();

    let regions = split_monitor_regions(display.width, display.height, cols, rows);
    if regions.is_empty() {
        return Err("cols, rows, and display dimensions must be greater than zero".to_string());
    }

    if replace_existing_on_monitor {
        for out in &snap.state.outputs {
            if out.monitor_index == Some(monitor_index) {
                let _ = apply_scene_diff(&bridge, SceneDiff::RemoveOutput(out.id)).await;
            }
        }
    }

    let look_id = match default_kind {
        OutputKind::Audience => LOOK_AUDIENCE_FULL,
        OutputKind::Stage => LOOK_STAGE_CONFIDENCE,
    };
    let kind_label = match default_kind {
        OutputKind::Audience => "Audience",
        OutputKind::Stage => "Stage",
    };

    let mut final_count = snap.state.outputs.len();
    for (idx, reg) in regions.into_iter().enumerate() {
        let cfg = OutputConfig {
            id: OutputId::new(),
            name: format!(
                "Display {} — Region {} ({kind_label})",
                monitor_index + 1,
                idx + 1
            ),
            kind: default_kind,
            monitor_index: Some(monitor_index),
            width: reg.width,
            height: reg.height,
            fullscreen: false,
            placement: OutputPlacement::Region(reg),
            look_id,
            stage_layout: (default_kind == OutputKind::Stage).then(StageLayoutConfig::default),
        };
        let st = apply_scene_diff(&bridge, SceneDiff::UpsertOutput(cfg)).await?;
        final_count = st.outputs.len();
    }

    refresh_snapshot_and_buses(&bridge, Some(final_count)).await
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

#[derive(serde::Serialize)]
struct BroadcastCapabilitiesDto {
    ndi_status: String,
    omt_ready: bool,
    decklink_status: String,
    virtual_output_backend: String,
    web_stage_url: String,
}

#[tauri::command]
fn get_broadcast_capabilities() -> BroadcastCapabilitiesDto {
    let ndi_status = match broadcast_io::probe_ndi_runtime(&[]) {
        broadcast_io::NdiRuntimeStatus::Available { library_path } => {
            format!("Available ({library_path})")
        }
        broadcast_io::NdiRuntimeStatus::NotInstalled { install_hint, .. } => install_hint,
    };
    let decklink_status = match broadcast_io::probe_decklink_runtime(&[]) {
        broadcast_io::DeckLinkStatus::DriverDetected { library_path } => {
            format!("Driver Detected ({library_path})")
        }
        broadcast_io::DeckLinkStatus::DriverNotInstalled { install_hint } => install_hint,
    };
    BroadcastCapabilitiesDto {
        ndi_status,
        omt_ready: true,
        decklink_status,
        virtual_output_backend: format!("{:?}", broadcast_io::VirtualOutputBackend::native_for_host()),
        web_stage_url: "http://127.0.0.1:<ephemeral-stage-port>".to_string(),
    }
}

#[derive(serde::Serialize)]
struct TimerStatusDto {
    id: String,
    name: String,
    formatted: String,
    running: bool,
    overrun: bool,
}

#[tauri::command]
fn create_countdown_timer(
    bridge: State<'_, Arc<EngineBridgeState>>,
    name: String,
    initial_secs: i64,
) -> Result<Vec<TimerStatusDto>, String> {
    let mut timers = bridge.timers.lock().map_err(|e| e.to_string())?;
    let _ = timers.create(
        name,
        show_control::TimerKind::Countdown {
            initial_secs,
            allow_overrun: true,
        },
    );
    Ok(timers
        .list()
        .into_iter()
        .map(|t| TimerStatusDto {
            id: t.id.0.to_string(),
            name: t.name,
            formatted: t.formatted,
            running: t.running,
            overrun: t.overrun,
        })
        .collect())
}

#[derive(serde::Serialize)]
struct ImportSummaryDto {
    title: String,
    slide_count: usize,
    ccli_number: Option<String>,
    source_format: String,
}

#[tauri::command]
fn import_worship_document(
    bridge: State<'_, Arc<EngineBridgeState>>,
    format: String,
    content: String,
) -> Result<ImportSummaryDto, String> {
    let mut lib = bridge.library.lock().map_err(|e| e.to_string())?;
    match format.as_str() {
        "chordpro" => {
            let song = integrations::parse_chordpro(&content)?;
            let slide_count = song.slides.len();
            let rec = SongRecord {
                id: Uuid::new_v4().to_string(),
                title: song.title.clone(),
                author: song.author,
                copyright: song.copyright,
                ccli_number: song.ccli_number.clone(),
                key_signature: song.key,
                bpm: None,
                lyrics_plain: song.slides.iter().map(|s| s.plain_text()).collect::<Vec<_>>().join("\n"),
                slides: song.slides,
                updated_at: chrono::Utc::now(),
            };
            lib.upsert_song(&rec).map_err(|e| e.to_string())?;
            Ok(ImportSummaryDto {
                title: song.title,
                slide_count,
                ccli_number: song.ccli_number,
                source_format: "ChordPro".into(),
            })
        }
        "songselect_usr" => {
            let song = integrations::parse_songselect_usr(&content)?;
            let slide_count = song.slides.len();
            let rec = SongRecord {
                id: Uuid::new_v4().to_string(),
                title: song.title.clone(),
                author: song.author,
                copyright: song.copyright,
                ccli_number: song.ccli_number.clone(),
                key_signature: None,
                bpm: None,
                lyrics_plain: song.slides.iter().map(|s| s.plain_text()).collect::<Vec<_>>().join("\n"),
                slides: song.slides,
                updated_at: chrono::Utc::now(),
            };
            lib.upsert_song(&rec).map_err(|e| e.to_string())?;
            Ok(ImportSummaryDto {
                title: song.title,
                slide_count,
                ccli_number: song.ccli_number,
                source_format: "SongSelect (.usr)".into(),
            })
        }
        "plain_text" => {
            let imported = integrations::import_plain_text_song("Imported Song", &content)
                .map_err(|e| e.to_string())?;
            Ok(ImportSummaryDto {
                title: imported.title,
                slide_count: imported.slides.len(),
                ccli_number: imported.ccli_number,
                source_format: imported.source_format,
            })
        }
        "bible_osis" => {
            let verses = integrations::parse_bible_xml(
                &content,
                "IMPORTED",
                "Public Domain",
                integrations::BibleXmlFormat::Osis,
            )
            .map_err(|e| e.to_string())?;
            let count = lib.insert_bible_verses(&verses).map_err(|e| e.to_string())?;
            Ok(ImportSummaryDto {
                title: "OSIS Bible".into(),
                slide_count: count,
                ccli_number: None,
                source_format: "OSIS XML".into(),
            })
        }
        other => Err(format!("unsupported import format '{other}'")),
    }
}

#[tauri::command]
fn export_ccli_csv_report(bridge: State<'_, Arc<EngineBridgeState>>) -> Result<String, String> {
    let lib = bridge.library.lock().map_err(|e| e.to_string())?;
    let logs = lib.list_ccli_usage().map_err(|e| e.to_string())?;
    Ok(integrations::export_ccli_usage_csv(&logs))
}

#[tauri::command]
fn list_plugin_tiers() -> Vec<plugin_host::RegisteredPluginInfo> {
    let mut supervisor = plugin_host::SidecarSupervisor::default();
    let _ = supervisor.register(
        plugin_sdk::PluginManifest {
            id: "org.openpresenter.whisper-stt".to_string(),
            name: "Local Whisper STT (Optional Sidecar)".to_string(),
            version: "0.1.0".to_string(),
            tier: plugin_sdk::PluginTier::SidecarProcess,
            entrypoint: "openpresenter-whisper-sidecar".to_string(),
            capabilities: vec![
                plugin_sdk::PluginCapability::AudioTap,
                plugin_sdk::PluginCapability::CueTrigger,
            ],
        },
        &[plugin_sdk::PluginCapability::AudioTap],
    );
    supervisor.list()
}

#[tauri::command]
async fn shutdown_engine(bridge: State<'_, Arc<EngineBridgeState>>) -> Result<(), String> {
    let _ = bridge.request(EngineCommand::Shutdown, false).await?;
    let mut guard = bridge.conn.lock().await;
    *guard = None;
    Ok(())
}

/// Fast 2x nearest-neighbor RGBA8 downsample so ~310px sidebar monitors transfer 4x fewer bytes
/// (230 KB instead of 921 KB) without saturating WebView2 IPC or the JS main thread.
fn pack_preview_packet(src_w: u32, src_h: u32, rgba: &[u8]) -> Vec<u8> {
    if src_w >= 640 && src_h >= 360 {
        let dst_w = src_w / 2;
        let dst_h = src_h / 2;
        let mut packet = Vec::with_capacity(8 + (dst_w * dst_h * 4) as usize);
        packet.extend_from_slice(&dst_w.to_le_bytes());
        packet.extend_from_slice(&dst_h.to_le_bytes());
        let src_stride = (src_w * 4) as usize;
        for y in 0..dst_h as usize {
            let row_off = (y * 2) * src_stride;
            for x in 0..dst_w as usize {
                let px = row_off + (x * 2) * 4;
                if px + 4 <= rgba.len() {
                    packet.extend_from_slice(&rgba[px..px + 4]);
                }
            }
        }
        packet
    } else {
        let mut packet = Vec::with_capacity(8 + rgba.len());
        packet.extend_from_slice(&src_w.to_le_bytes());
        packet.extend_from_slice(&src_h.to_le_bytes());
        packet.extend_from_slice(rgba);
        packet
    }
}

/// Pull-based, naturally backpressured binary preview reader.
///
/// Unlike push-based Tauri `Channel::send` (which triggers unbounded `webview.eval()` +
/// `ChannelDataIpcQueue` backlog under WebView2), pulling frames sequentially from the UI
/// guarantees zero queue buildup, zero leaked background tasks, and instant UI responsiveness.
#[tauri::command]
async fn poll_preview_frame(
    bridge: State<'_, Arc<EngineBridgeState>>,
    output_id: String,
) -> Result<Response, String> {
    let oid = OutputId(Uuid::parse_str(&output_id).map_err(|e| e.to_string())?);
    let bus = {
        let map = bridge.preview_buses.lock().await;
        map.get(&oid).cloned()
    }
    .ok_or_else(|| format!("no preview bus registered for output {output_id}"))?;

    let mut readers = bridge.preview_readers.lock().map_err(|e| e.to_string())?;
    let needs_open = match readers.get(&oid) {
        Some((path, _)) => path != &bus.shm_path,
        None => true,
    };
    if needs_open {
        let reader = ShmPreviewReader::open(&bus.shm_path)
            .map_err(|e| format!("failed to open SHM preview ring '{}': {e}", bus.shm_path))?;
        readers.insert(oid, (bus.shm_path.clone(), reader));
    }

    let Some((_, reader)) = readers.get_mut(&oid) else {
        return Ok(Response::new(Vec::new()));
    };

    let mut rgba = Vec::new();
    match reader.try_read_latest(&mut rgba) {
        Some(meta) => Ok(Response::new(pack_preview_packet(
            meta.width,
            meta.height,
            &rgba,
        ))),
        None => Ok(Response::new(Vec::new())),
    }
}

#[derive(serde::Serialize)]
struct DiagnosticsStatusDto {
    log_dir: String,
    crash_dir: String,
    engine_log_path: String,
    desktop_log_path: String,
    active_log_level: String,
    crash_count: usize,
}

#[tauri::command]
fn get_diagnostics_status(
    bridge: State<'_, Arc<EngineBridgeState>>,
) -> Result<DiagnosticsStatusDto, String> {
    let crashes = core_diagnostics::list_crash_reports(&bridge.diagnostics)
        .map_err(|e| e.to_string())?;
    let active_level = bridge
        .logging
        .as_ref()
        .map(|l| l.current_level())
        .unwrap_or(LogLevel::Info);
    Ok(DiagnosticsStatusDto {
        log_dir: bridge.diagnostics.log_dir.to_string_lossy().into_owned(),
        crash_dir: bridge.diagnostics.crash_dir.to_string_lossy().into_owned(),
        engine_log_path: bridge
            .diagnostics
            .log_file_for("engine")
            .to_string_lossy()
            .into_owned(),
        desktop_log_path: bridge
            .diagnostics
            .log_file_for("desktop")
            .to_string_lossy()
            .into_owned(),
        active_log_level: active_level.as_str().to_string(),
        crash_count: crashes.len(),
    })
}

#[tauri::command]
async fn set_debug_logging(
    bridge: State<'_, Arc<EngineBridgeState>>,
    level: String,
) -> Result<String, String> {
    let parsed = LogLevel::parse(&level).map_err(|e| e.to_string())?;
    if let Some(ctrl) = bridge.logging.as_ref() {
        ctrl.set_level(parsed).map_err(|e| e.to_string())?;
    }
    // Forward live log level change to the decoupled engine sidecar if connected.
    let _ = bridge
        .request(EngineCommand::SetLogLevel(parsed.as_str().to_string()), false)
        .await;
    Ok(parsed.as_str().to_string())
}

#[tauri::command]
fn read_diagnostic_logs(
    bridge: State<'_, Arc<EngineBridgeState>>,
    source: String,
    max_lines: Option<usize>,
) -> Result<Vec<String>, String> {
    core_diagnostics::read_recent_logs(&bridge.diagnostics, &source, max_lines.unwrap_or(200))
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn list_crash_reports(
    bridge: State<'_, Arc<EngineBridgeState>>,
) -> Result<Vec<CrashReport>, String> {
    core_diagnostics::list_crash_reports(&bridge.diagnostics).map_err(|e| e.to_string())
}

#[tauri::command]
fn clear_crash_reports(bridge: State<'_, Arc<EngineBridgeState>>) -> Result<usize, String> {
    core_diagnostics::clear_crash_reports(&bridge.diagnostics).map_err(|e| e.to_string())
}

#[tauri::command]
fn report_frontend_crash(
    bridge: State<'_, Arc<EngineBridgeState>>,
    kind: String,
    message: String,
    stack: Option<String>,
) -> Result<CrashReport, String> {
    error!(kind = %kind, message = %message, "recorded frontend UI crash report");
    core_diagnostics::record_runtime_error(
        &bridge.diagnostics,
        "openpresenter-ui",
        &kind,
        env!("CARGO_PKG_VERSION"),
        &message,
        None,
        stack.as_deref(),
    )
    .map_err(|e| e.to_string())
}

#[tauri::command]
fn export_diagnostics_bundle(
    bridge: State<'_, Arc<EngineBridgeState>>,
) -> Result<ExportedBundle, String> {
    let level = bridge
        .logging
        .as_ref()
        .map(|l| l.current_level())
        .unwrap_or(LogLevel::Info);
    let gpu = bridge
        .last_gpu_summary
        .lock()
        .ok()
        .and_then(|g| g.clone());
    core_diagnostics::export_diagnostic_bundle(
        &bridge.diagnostics,
        env!("CARGO_PKG_VERSION"),
        level,
        gpu.as_deref(),
    )
    .map_err(|e| e.to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let diag_paths = DiagnosticsPaths::default_system();
    core_diagnostics::install_crash_hook(
        "openpresenter-desktop",
        env!("CARGO_PKG_VERSION"),
        diag_paths.clone(),
    );
    let logging = core_diagnostics::init_logging("desktop", &diag_paths, LogLevel::Info).ok();
    info!(
        log_dir = %diag_paths.log_dir.display(),
        crash_dir = %diag_paths.crash_dir.display(),
        "OpenPresenter desktop control surface starting with persistent diagnostics"
    );

    let bridge = Arc::new(EngineBridgeState::new(diag_paths, logging));

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
            list_physical_displays,
            upsert_output_screen,
            remove_output_screen,
            split_monitor_into_screens,
            search_library_songs,
            list_playlists,
            get_broadcast_capabilities,
            create_countdown_timer,
            import_worship_document,
            export_ccli_csv_report,
            list_plugin_tiers,
            shutdown_engine,
            poll_preview_frame,
            get_diagnostics_status,
            set_debug_logging,
            read_diagnostic_logs,
            list_crash_reports,
            clear_crash_reports,
            report_frontend_crash,
            export_diagnostics_bundle,
        ])
        .run(tauri::generate_context!())
        .expect("error while running OpenPresenter desktop control surface");
}
