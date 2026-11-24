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
use core_library::{
    BibleVerseRecord, DEFAULT_LIBRARY_ID, LibraryStore, LookPresetRecord, MacroRecord,
    MediaAssetRecord, MediaBinRecord, PlaylistRecord, PresentationLibraryRecord,
    ScheduledEventRecord, SongRecord, build_scripture_slides, infer_media_kind, parse_quick_lyrics,
    slide_from_text,
};
use core_scene::{
    ActiveBackground, ActiveMessage, Arrangement, AudioPlaybackState, AutoPlayTrack, ColorRgba,
    ForegroundMedia, LOOK_AUDIENCE_FULL, LOOK_STAGE_CONFIDENCE, LayerKind, LookConfig, LookId,
    MaskStyle, MediaCueRole, MediaTransportState, MessageTemplate, MonitorRegion,
    OutputCalibration, OutputConfig, OutputId, OutputKind, OutputPlacement, OverlayItem, SceneDiff,
    SceneState, ShaderMotionStyle, Slide, SlideAction, SlideElement, SlideId, SlideMediaCue,
    StageLayoutConfig, TemplateConfig, TemplateId, TransitionSpec, render_message_template,
    split_monitor_regions,
};
use interprocess::local_socket::tokio::{Stream, prelude::*};
use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;
use tauri::{AppHandle, Manager, State};
use tauri::ipc::Response;
use tokio::sync::Mutex;
use tracing::{error, info, warn};
use uuid::Uuid;

#[derive(Clone)]
struct BroadcastProfileState {
    source_name: String,
    alpha_mode: String,
    decklink_mode: String,
    streaming_active: bool,
}

impl Default for BroadcastProfileState {
    fn default() -> Self {
        Self {
            source_name: "OpenPresenter — Main Broadcast".to_string(),
            alpha_mode: "PremultipliedAlpha".to_string(),
            decklink_mode: "ExternalKeyAndFill".to_string(),
            streaming_active: false,
        }
    }
}

struct EngineBridgeState {
    socket_name: String,
    conn: Mutex<Option<Stream>>,
    preview_buses: Mutex<HashMap<OutputId, PreviewBusDescriptor>>,
    preview_readers: StdMutex<HashMap<OutputId, (String, ShmPreviewReader)>>,
    library: StdMutex<LibraryStore>,
    timers: StdMutex<show_control::TimerEngine>,
    broadcast_profile: StdMutex<BroadcastProfileState>,
    active_song_id: StdMutex<Option<String>>,
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
            broadcast_profile: StdMutex::new(BroadcastProfileState::default()),
            active_song_id: StdMutex::new(None),
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
        EngineEvent::Snapshot(mut snap) => {
            // Hydrate persisted SQLite custom Looks, Templates, Overlays & CCLI display config on attach.
            let persisted = bridge.library.lock().ok().and_then(|lib| {
                let looks = lib.list_looks().ok()?;
                let templates = lib.list_templates().ok()?;
                let overlays = lib.list_overlays().ok().unwrap_or_default();
                let ccli_cfg = lib.get_ccli_display_config().ok().unwrap_or_default();
                Some((looks, templates, overlays, ccli_cfg))
            });
            if let Some((looks, templates, overlays, ccli_cfg)) = persisted {
                if (!looks.is_empty() || !templates.is_empty())
                    && let Ok(updated) = apply_scene_diff(
                        &bridge,
                        SceneDiff::SyncLooksAndTemplates { looks, templates },
                    )
                    .await
                {
                    snap.state = updated;
                }
                if !overlays.is_empty()
                    && let Ok(updated) = apply_scene_diff(&bridge, SceneDiff::SyncOverlays(overlays)).await
                {
                    snap.state = updated;
                }
                if let Ok(updated) =
                    apply_scene_diff(&bridge, SceneDiff::SetCcliDisplayConfig(ccli_cfg)).await
                {
                    snap.state = updated;
                }
            }
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
async fn delete_slide_element(
    bridge: State<'_, Arc<EngineBridgeState>>,
    slide_id: String,
    element_id: String,
) -> Result<SceneState, String> {
    let sid = SlideId(Uuid::parse_str(&slide_id).map_err(|e| e.to_string())?);
    let eid = core_scene::ElementId(Uuid::parse_str(&element_id).map_err(|e| e.to_string())?);
    apply_scene_diff(
        &bridge,
        SceneDiff::DeleteSlideElement {
            slide_id: sid,
            element_id: eid,
        },
    )
    .await
}

#[tauri::command]
async fn set_output_stage_layout(
    bridge: State<'_, Arc<EngineBridgeState>>,
    output_id: String,
    layout: StageLayoutConfig,
) -> Result<SceneState, String> {
    let oid = OutputId(Uuid::parse_str(&output_id).map_err(|e| e.to_string())?);
    apply_scene_diff(&bridge, SceneDiff::SetOutputStageLayout { output_id: oid, layout }).await
}

#[tauri::command]
async fn set_pointer(
    bridge: State<'_, Arc<EngineBridgeState>>,
    pointer: Option<core_scene::TelestratorPointer>,
) -> Result<SceneState, String> {
    apply_scene_diff(&bridge, SceneDiff::SetPointer(pointer)).await
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
async fn set_freeze(
    bridge: State<'_, Arc<EngineBridgeState>>,
    enabled: bool,
) -> Result<SceneState, String> {
    apply_scene_diff(&bridge, SceneDiff::SetFreeze(enabled)).await
}

#[tauri::command]
async fn undo_scene(bridge: State<'_, Arc<EngineBridgeState>>) -> Result<SceneState, String> {
    apply_scene_diff(&bridge, SceneDiff::Undo).await
}

#[tauri::command]
async fn redo_scene(bridge: State<'_, Arc<EngineBridgeState>>) -> Result<SceneState, String> {
    apply_scene_diff(&bridge, SceneDiff::Redo).await
}

#[tauri::command]
async fn set_media_transport(
    bridge: State<'_, Arc<EngineBridgeState>>,
    transport: MediaTransportState,
) -> Result<SceneState, String> {
    apply_scene_diff(&bridge, SceneDiff::SetMediaTransport(transport)).await
}

#[tauri::command]
async fn set_audio_playback(
    bridge: State<'_, Arc<EngineBridgeState>>,
    audio: Option<AudioPlaybackState>,
) -> Result<SceneState, String> {
    apply_scene_diff(&bridge, SceneDiff::SetAudioPlayback(audio)).await
}

#[tauri::command]
async fn select_arrangement(
    bridge: State<'_, Arc<EngineBridgeState>>,
    arrangement_id: Option<String>,
) -> Result<SceneState, String> {
    apply_scene_diff(&bridge, SceneDiff::SelectArrangement(arrangement_id)).await
}

#[tauri::command]
async fn save_arrangements(
    bridge: State<'_, Arc<EngineBridgeState>>,
    arrangements: Vec<Arrangement>,
    active_id: Option<String>,
) -> Result<SceneState, String> {
    let state = apply_scene_diff(
        &bridge,
        SceneDiff::SetArrangements {
            arrangements: arrangements.clone(),
            active_id,
        },
    )
    .await?;
    if let Ok(guard) = bridge.active_song_id.lock()
        && let Some(song_id) = guard.as_deref()
        && let Ok(lib) = bridge.library.lock()
    {
        let _ = lib.update_song_arrangements(song_id, &arrangements);
    }
    Ok(state)
}

#[tauri::command]
fn list_bible_translations(
    bridge: State<'_, Arc<EngineBridgeState>>,
) -> Result<Vec<String>, String> {
    let lib = bridge.library.lock().map_err(|e| e.to_string())?;
    lib.list_bible_translations().map_err(|e| e.to_string())
}

#[derive(serde::Serialize)]
struct ScriptureLookupResult {
    primary: Vec<BibleVerseRecord>,
    secondary: Vec<BibleVerseRecord>,
}

#[tauri::command]
fn lookup_scripture(
    bridge: State<'_, Arc<EngineBridgeState>>,
    translation: Option<String>,
    secondary_translation: Option<String>,
    query: String,
    limit: Option<usize>,
) -> Result<ScriptureLookupResult, String> {
    let lib = bridge.library.lock().map_err(|e| e.to_string())?;
    let max_hits = limit.unwrap_or(24).clamp(1, 100);
    let primary = lib
        .lookup_scripture(
            translation.as_deref().filter(|s| !s.is_empty()),
            &query,
            max_hits,
        )
        .map_err(|e| e.to_string())?;
    let secondary = match secondary_translation.as_deref().filter(|s| !s.is_empty()) {
        Some(sec_tr) => lib
            .lookup_scripture(Some(sec_tr), &query, max_hits)
            .unwrap_or_default(),
        None => Vec::new(),
    };
    Ok(ScriptureLookupResult { primary, secondary })
}

#[tauri::command]
async fn present_scripture(
    bridge: State<'_, Arc<EngineBridgeState>>,
    primary: Vec<BibleVerseRecord>,
    secondary: Vec<BibleVerseRecord>,
    verses_per_slide: usize,
    show_verse_numbers: bool,
    save_to_library: bool,
) -> Result<SceneState, String> {
    if primary.is_empty() {
        return Err("Select at least one Bible verse to present".to_string());
    }
    let slides = build_scripture_slides(&primary, &secondary, verses_per_slide, show_verse_numbers);
    if slides.is_empty() {
        return Err("No scripture slides could be generated".to_string());
    }
    if save_to_library {
        let title = slides
            .first()
            .map(|s| s.headline.clone())
            .unwrap_or_else(|| "Scripture Reading".to_string());
        let rec = SongRecord {
            id: Uuid::new_v4().to_string(),
            library_id: DEFAULT_LIBRARY_ID.to_string(),
            title,
            author: "Scripture".to_string(),
            copyright: "Public Domain".to_string(),
            ccli_number: None,
            key_signature: None,
            bpm: None,
            lyrics_plain: slides
                .iter()
                .map(|s| s.plain_text())
                .collect::<Vec<_>>()
                .join("\n"),
            slides: slides.clone(),
            arrangements: Vec::new(),
            cue_tracks: Vec::new(),
            chord_chart: None,
            sheet_music: None,
            updated_at: chrono::Utc::now(),
        };
        let lib = bridge.library.lock().map_err(|e| e.to_string())?;
        let _ = lib.upsert_song(&rec);
    }
    apply_scene_diff(&bridge, SceneDiff::ReplaceDeck(slides)).await
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
async fn parse_and_save_quick_lyrics(
    bridge: State<'_, Arc<EngineBridgeState>>,
    library_id: String,
    title: String,
    author: Option<String>,
    ccli_number: Option<String>,
    raw_lyrics: String,
    max_lines_per_slide: usize,
    load_to_deck: bool,
) -> Result<Option<SceneState>, String> {
    let trimmed = title.trim();
    if trimmed.is_empty() {
        return Err("Song title cannot be empty".to_string());
    }
    let (slides, arrangements) =
        parse_quick_lyrics(trimmed, &raw_lyrics, max_lines_per_slide, ccli_number.clone());
    let rec = SongRecord {
        id: Uuid::new_v4().to_string(),
        library_id: if library_id.trim().is_empty() || library_id == "all" {
            DEFAULT_LIBRARY_ID.to_string()
        } else {
            library_id
        },
        title: trimmed.to_string(),
        author: author.unwrap_or_default(),
        copyright: String::new(),
        ccli_number,
        key_signature: None,
        bpm: None,
        lyrics_plain: slides
            .iter()
            .map(|s| s.plain_text())
            .collect::<Vec<_>>()
            .join("\n"),
        slides: slides.clone(),
        arrangements: arrangements.clone(),
        cue_tracks: Vec::new(),
        chord_chart: None,
        sheet_music: None,
        updated_at: chrono::Utc::now(),
    };
    {
        let lib = bridge.library.lock().map_err(|e| e.to_string())?;
        lib.upsert_song(&rec).map_err(|e| e.to_string())?;
    }
    if load_to_deck {
        if let Ok(mut g) = bridge.active_song_id.lock() {
            *g = Some(rec.id.clone());
        }
        let active_id = arrangements.first().map(|a| a.id.clone());
        let _ = apply_scene_diff(
            &bridge,
            SceneDiff::LoadPresentation {
                slides,
                arrangements,
                active_arrangement_id: active_id,
            },
        )
        .await?;
        let _ = apply_scene_diff(&bridge, SceneDiff::SetCueTracks(Vec::new())).await;
        let state = apply_scene_diff(
            &bridge,
            SceneDiff::SetActivePresentationMeta {
                id: Some(rec.id),
                title: Some(rec.title),
                author: (!rec.author.is_empty()).then_some(rec.author),
                copyright: (!rec.copyright.is_empty()).then_some(rec.copyright),
                ccli_number: rec.ccli_number,
            },
        )
        .await?;
        Ok(Some(state))
    } else {
        Ok(None)
    }
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
        calibration: OutputCalibration::default(),
    };

    let state = apply_scene_diff(&bridge, SceneDiff::UpsertOutput(cfg)).await?;
    refresh_snapshot_and_buses(&bridge, Some(state.outputs.len())).await
}

#[tauri::command]
async fn set_output_calibration(
    bridge: State<'_, Arc<EngineBridgeState>>,
    output_id: String,
    calibration: OutputCalibration,
) -> Result<SceneState, String> {
    let oid = OutputId(Uuid::parse_str(&output_id).map_err(|e| e.to_string())?);
    apply_scene_diff(
        &bridge,
        SceneDiff::SetOutputCalibration {
            output_id: oid,
            calibration,
        },
    )
    .await
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
            calibration: OutputCalibration::default(),
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
fn list_looks(bridge: State<'_, Arc<EngineBridgeState>>) -> Result<Vec<LookConfig>, String> {
    let lib = bridge.library.lock().map_err(|e| e.to_string())?;
    lib.list_looks().map_err(|e| e.to_string())
}

#[tauri::command]
async fn upsert_look(
    bridge: State<'_, Arc<EngineBridgeState>>,
    mut look: LookConfig,
) -> Result<SceneState, String> {
    look.name = look.name.trim().to_string();
    if look.name.is_empty() {
        return Err("Look name cannot be empty".to_string());
    }
    let state = apply_scene_diff(&bridge, SceneDiff::UpsertLook(look.clone())).await?;
    if let Some(canonical) = state.looks.get(&look.id) {
        let lib = bridge.library.lock().map_err(|e| e.to_string())?;
        lib.upsert_look(canonical).map_err(|e| e.to_string())?;
    }
    Ok(state)
}

#[tauri::command]
async fn delete_look(
    bridge: State<'_, Arc<EngineBridgeState>>,
    look_id: String,
) -> Result<SceneState, String> {
    let lid = LookId(Uuid::parse_str(&look_id).map_err(|e| e.to_string())?);
    if lid == LOOK_AUDIENCE_FULL || lid == LOOK_STAGE_CONFIDENCE {
        return Err("Cannot delete built-in Audience Full or Stage Confidence Looks".to_string());
    }
    let state = apply_scene_diff(&bridge, SceneDiff::RemoveLook(lid)).await?;
    let lib = bridge.library.lock().map_err(|e| e.to_string())?;
    let _ = lib.delete_look(lid).map_err(|e| e.to_string())?;
    Ok(state)
}

#[tauri::command]
fn list_templates(
    bridge: State<'_, Arc<EngineBridgeState>>,
) -> Result<Vec<TemplateConfig>, String> {
    let lib = bridge.library.lock().map_err(|e| e.to_string())?;
    lib.list_templates().map_err(|e| e.to_string())
}

#[tauri::command]
async fn upsert_template(
    bridge: State<'_, Arc<EngineBridgeState>>,
    mut template: TemplateConfig,
) -> Result<SceneState, String> {
    template.name = template.name.trim().to_string();
    if template.name.is_empty() {
        return Err("Template name cannot be empty".to_string());
    }
    let state = apply_scene_diff(&bridge, SceneDiff::UpsertTemplate(template.clone())).await?;
    if let Some(canonical) = state.templates.get(&template.id) {
        let lib = bridge.library.lock().map_err(|e| e.to_string())?;
        lib.upsert_template(canonical).map_err(|e| e.to_string())?;
    }
    Ok(state)
}

#[tauri::command]
async fn delete_template(
    bridge: State<'_, Arc<EngineBridgeState>>,
    template_id: String,
) -> Result<SceneState, String> {
    let tid = TemplateId(Uuid::parse_str(&template_id).map_err(|e| e.to_string())?);
    let state = apply_scene_diff(&bridge, SceneDiff::RemoveTemplate(tid)).await?;
    let lib = bridge.library.lock().map_err(|e| e.to_string())?;
    let _ = lib.delete_template(tid).map_err(|e| e.to_string())?;
    // Persist any looks whose template_id was cleared by removing this template.
    for look in state.looks.values() {
        let _ = lib.upsert_look(look);
    }
    Ok(state)
}

#[tauri::command]
async fn apply_template_to_slide(
    bridge: State<'_, Arc<EngineBridgeState>>,
    slide_id: String,
    template_id: String,
) -> Result<SceneState, String> {
    let sid = SlideId(Uuid::parse_str(&slide_id).map_err(|e| e.to_string())?);
    let tid = TemplateId(Uuid::parse_str(&template_id).map_err(|e| e.to_string())?);
    apply_scene_diff(
        &bridge,
        SceneDiff::ApplyTemplateToSlide {
            slide_id: sid,
            template_id: tid,
        },
    )
    .await
}

#[tauri::command]
async fn apply_template_to_deck(
    bridge: State<'_, Arc<EngineBridgeState>>,
    template_id: String,
) -> Result<SceneState, String> {
    let tid = TemplateId(Uuid::parse_str(&template_id).map_err(|e| e.to_string())?);
    apply_scene_diff(&bridge, SceneDiff::ApplyTemplateToDeck { template_id: tid }).await
}

#[tauri::command]
fn list_look_presets(
    bridge: State<'_, Arc<EngineBridgeState>>,
) -> Result<Vec<LookPresetRecord>, String> {
    let lib = bridge.library.lock().map_err(|e| e.to_string())?;
    lib.list_look_presets().map_err(|e| e.to_string())
}

#[tauri::command]
fn save_look_preset(
    bridge: State<'_, Arc<EngineBridgeState>>,
    id: Option<String>,
    name: String,
    assignments: BTreeMap<String, String>,
) -> Result<Vec<LookPresetRecord>, String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err("Look Preset name cannot be empty".to_string());
    }
    let rec = LookPresetRecord {
        id: id
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| Uuid::new_v4().to_string()),
        name: trimmed.to_string(),
        assignments,
        updated_at: chrono::Utc::now(),
    };
    let lib = bridge.library.lock().map_err(|e| e.to_string())?;
    lib.upsert_look_preset(&rec).map_err(|e| e.to_string())?;
    lib.list_look_presets().map_err(|e| e.to_string())
}

#[tauri::command]
fn delete_look_preset(
    bridge: State<'_, Arc<EngineBridgeState>>,
    preset_id: String,
) -> Result<Vec<LookPresetRecord>, String> {
    let lib = bridge.library.lock().map_err(|e| e.to_string())?;
    let _ = lib
        .delete_look_preset(&preset_id)
        .map_err(|e| e.to_string())?;
    lib.list_look_presets().map_err(|e| e.to_string())
}

#[tauri::command]
async fn apply_look_preset(
    bridge: State<'_, Arc<EngineBridgeState>>,
    preset_id: String,
) -> Result<SceneState, String> {
    let preset = {
        let lib = bridge.library.lock().map_err(|e| e.to_string())?;
        lib.list_look_presets()
            .map_err(|e| e.to_string())?
            .into_iter()
            .find(|p| p.id == preset_id)
            .ok_or_else(|| format!("Look Preset '{preset_id}' not found"))?
    };
    let mut pairs = Vec::with_capacity(preset.assignments.len());
    for (oid_str, lid_str) in preset.assignments {
        if let (Ok(oid), Ok(lid)) = (Uuid::parse_str(&oid_str), Uuid::parse_str(&lid_str)) {
            pairs.push((OutputId(oid), LookId(lid)));
        }
    }
    if pairs.is_empty() {
        return Err("Preset contains no valid output-to-look assignments".to_string());
    }
    apply_scene_diff(&bridge, SceneDiff::ApplyLookRouting { assignments: pairs }).await
}

#[tauri::command]
async fn apply_look_routing(
    bridge: State<'_, Arc<EngineBridgeState>>,
    assignments: BTreeMap<String, String>,
) -> Result<SceneState, String> {
    let mut pairs = Vec::with_capacity(assignments.len());
    for (oid_str, lid_str) in assignments {
        let oid = OutputId(Uuid::parse_str(&oid_str).map_err(|e| e.to_string())?);
        let lid = LookId(Uuid::parse_str(&lid_str).map_err(|e| e.to_string())?);
        pairs.push((oid, lid));
    }
    apply_scene_diff(&bridge, SceneDiff::ApplyLookRouting { assignments: pairs }).await
}

#[tauri::command]
fn list_libraries(
    bridge: State<'_, Arc<EngineBridgeState>>,
) -> Result<Vec<PresentationLibraryRecord>, String> {
    let lib = bridge.library.lock().map_err(|e| e.to_string())?;
    lib.list_libraries().map_err(|e| e.to_string())
}

#[tauri::command]
fn create_library(
    bridge: State<'_, Arc<EngineBridgeState>>,
    name: String,
) -> Result<Vec<PresentationLibraryRecord>, String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err("Library name cannot be empty".to_string());
    }
    let lib = bridge.library.lock().map_err(|e| e.to_string())?;
    lib.upsert_library(&PresentationLibraryRecord {
        id: Uuid::new_v4().to_string(),
        name: trimmed.to_string(),
        updated_at: chrono::Utc::now(),
    })
    .map_err(|e| e.to_string())?;
    lib.list_libraries().map_err(|e| e.to_string())
}

#[tauri::command]
fn delete_library(
    bridge: State<'_, Arc<EngineBridgeState>>,
    library_id: String,
) -> Result<Vec<PresentationLibraryRecord>, String> {
    let lib = bridge.library.lock().map_err(|e| e.to_string())?;
    if !lib.delete_library(&library_id).map_err(|e| e.to_string())? {
        return Err("Cannot delete the default library or library not found".to_string());
    }
    lib.list_libraries().map_err(|e| e.to_string())
}

#[tauri::command]
fn search_library_songs(
    bridge: State<'_, Arc<EngineBridgeState>>,
    query: String,
    library_id: Option<String>,
) -> Result<Vec<SongRecord>, String> {
    let lib = bridge.library.lock().map_err(|e| e.to_string())?;
    let lib_filter = library_id
        .as_deref()
        .filter(|id| !id.is_empty() && *id != "all");
    lib.search_songs_in_library(lib_filter, &query, 100)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn create_presentation(
    bridge: State<'_, Arc<EngineBridgeState>>,
    library_id: String,
    title: String,
    body_text: String,
    author: Option<String>,
    ccli_number: Option<String>,
) -> Result<SongRecord, String> {
    let title = title.trim();
    if title.is_empty() {
        return Err("Presentation title cannot be empty".to_string());
    }
    let stanzas: Vec<&str> = body_text
        .split("\n\n")
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect();
    let slides: Vec<Slide> = if stanzas.is_empty() {
        vec![slide_from_text(title, "Slide 1", title, ccli_number.clone())]
    } else {
        stanzas
            .into_iter()
            .enumerate()
            .map(|(idx, stanza)| {
                slide_from_text(
                    &format!("{title} ({})", idx + 1),
                    &format!("Slide {}", idx + 1),
                    stanza,
                    ccli_number.clone(),
                )
            })
            .collect()
    };
    let rec = SongRecord {
        id: Uuid::new_v4().to_string(),
        library_id: if library_id.trim().is_empty() || library_id == "all" {
            DEFAULT_LIBRARY_ID.to_string()
        } else {
            library_id
        },
        title: title.to_string(),
        author: author.unwrap_or_default(),
        copyright: String::new(),
        ccli_number,
        key_signature: None,
        bpm: None,
        lyrics_plain: slides
            .iter()
            .map(|s| s.plain_text())
            .collect::<Vec<_>>()
            .join("\n"),
        slides,
        arrangements: Vec::new(),
        cue_tracks: Vec::new(),
        chord_chart: None,
        sheet_music: None,
        updated_at: chrono::Utc::now(),
    };
    let lib = bridge.library.lock().map_err(|e| e.to_string())?;
    lib.upsert_song(&rec).map_err(|e| e.to_string())?;
    Ok(rec)
}

#[tauri::command]
fn delete_presentation(
    bridge: State<'_, Arc<EngineBridgeState>>,
    song_id: String,
) -> Result<bool, String> {
    let lib = bridge.library.lock().map_err(|e| e.to_string())?;
    lib.delete_song(&song_id).map_err(|e| e.to_string())
}

#[tauri::command]
async fn load_presentation_to_deck(
    bridge: State<'_, Arc<EngineBridgeState>>,
    song_id: String,
) -> Result<SceneState, String> {
    let (
        title,
        author,
        copyright,
        ccli_number,
        slides,
        arrangements,
        cue_tracks,
        chord_chart,
        sheet_music,
    ) = {
        let lib = bridge.library.lock().map_err(|e| e.to_string())?;
        let song = lib
            .get_song(&song_id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("presentation '{song_id}' not found"))?;
        let s = if song.slides.is_empty() {
            vec![slide_from_text(
                &song.title,
                "Slide 1",
                &song.lyrics_plain,
                song.ccli_number.clone(),
            )]
        } else {
            song.slides
        };
        (
            song.title,
            (!song.author.is_empty()).then_some(song.author),
            (!song.copyright.is_empty()).then_some(song.copyright),
            song.ccli_number,
            s,
            song.arrangements,
            song.cue_tracks,
            song.chord_chart,
            song.sheet_music,
        )
    };
    if let Ok(mut g) = bridge.active_song_id.lock() {
        *g = Some(song_id.clone());
    }
    let active_arrangement_id = arrangements.first().map(|a| a.id.clone());
    let _ = apply_scene_diff(
        &bridge,
        SceneDiff::LoadPresentation {
            slides,
            arrangements,
            active_arrangement_id,
        },
    )
    .await?;
    let _ = apply_scene_diff(&bridge, SceneDiff::SetCueTracks(cue_tracks)).await;
    let _ = apply_scene_diff(
        &bridge,
        SceneDiff::SetActiveMusicSheets {
            chord_chart,
            sheet_music,
        },
    )
    .await;
    apply_scene_diff(
        &bridge,
        SceneDiff::SetActivePresentationMeta {
            id: Some(song_id),
            title: Some(title),
            author,
            copyright,
            ccli_number,
        },
    )
    .await
}

#[tauri::command]
fn list_playlists(
    bridge: State<'_, Arc<EngineBridgeState>>,
) -> Result<Vec<PlaylistRecord>, String> {
    let lib = bridge.library.lock().map_err(|e| e.to_string())?;
    lib.list_playlists().map_err(|e| e.to_string())
}

#[tauri::command]
fn upsert_playlist(
    bridge: State<'_, Arc<EngineBridgeState>>,
    id: Option<String>,
    name: String,
    item_song_ids: Vec<String>,
) -> Result<Vec<PlaylistRecord>, String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err("Playlist name cannot be empty".to_string());
    }
    let rec = PlaylistRecord {
        id: id
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| Uuid::new_v4().to_string()),
        name: trimmed.to_string(),
        item_song_ids,
        updated_at: chrono::Utc::now(),
    };
    let lib = bridge.library.lock().map_err(|e| e.to_string())?;
    lib.upsert_playlist(&rec).map_err(|e| e.to_string())?;
    lib.list_playlists().map_err(|e| e.to_string())
}

#[tauri::command]
fn delete_playlist(
    bridge: State<'_, Arc<EngineBridgeState>>,
    playlist_id: String,
) -> Result<Vec<PlaylistRecord>, String> {
    let lib = bridge.library.lock().map_err(|e| e.to_string())?;
    let _ = lib.delete_playlist(&playlist_id).map_err(|e| e.to_string())?;
    lib.list_playlists().map_err(|e| e.to_string())
}

#[tauri::command]
fn list_media_bins(
    bridge: State<'_, Arc<EngineBridgeState>>,
) -> Result<Vec<MediaBinRecord>, String> {
    let lib = bridge.library.lock().map_err(|e| e.to_string())?;
    lib.list_media_bins().map_err(|e| e.to_string())
}

#[tauri::command]
fn create_media_bin(
    bridge: State<'_, Arc<EngineBridgeState>>,
    name: String,
    category: String,
) -> Result<Vec<MediaBinRecord>, String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err("Media bin name cannot be empty".to_string());
    }
    let lib = bridge.library.lock().map_err(|e| e.to_string())?;
    lib.upsert_media_bin(&MediaBinRecord {
        id: Uuid::new_v4().to_string(),
        name: trimmed.to_string(),
        category,
        updated_at: chrono::Utc::now(),
    })
    .map_err(|e| e.to_string())?;
    lib.list_media_bins().map_err(|e| e.to_string())
}

#[tauri::command]
fn delete_media_bin(
    bridge: State<'_, Arc<EngineBridgeState>>,
    bin_id: String,
) -> Result<Vec<MediaBinRecord>, String> {
    let lib = bridge.library.lock().map_err(|e| e.to_string())?;
    if !lib.delete_media_bin(&bin_id).map_err(|e| e.to_string())? {
        return Err("Cannot delete the default Motion Backgrounds bin".to_string());
    }
    lib.list_media_bins().map_err(|e| e.to_string())
}

#[tauri::command]
fn list_media_assets(
    bridge: State<'_, Arc<EngineBridgeState>>,
    kind: Option<String>,
    bin_id: Option<String>,
) -> Result<Vec<MediaAssetRecord>, String> {
    let lib = bridge.library.lock().map_err(|e| e.to_string())?;
    lib.list_media_assets_filtered(kind.as_deref(), bin_id.as_deref())
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn add_media_file(
    bridge: State<'_, Arc<EngineBridgeState>>,
    file_path: String,
    bin_id: Option<String>,
    kind: Option<String>,
    has_alpha: Option<bool>,
) -> Result<MediaAssetRecord, String> {
    let trimmed = file_path.trim();
    if trimmed.is_empty() {
        return Err("Media file path cannot be empty".to_string());
    }
    let path = std::path::Path::new(trimmed);
    if !path.exists() {
        return Err(format!("Media file not found: {trimmed}"));
    }
    let resolved_kind = kind
        .filter(|k| !k.trim().is_empty() && k != "auto")
        .unwrap_or_else(|| infer_media_kind(path).to_string());
    let resolved_bin = bin_id.unwrap_or_else(|| match resolved_kind.as_str() {
        "image" => "images".to_string(),
        "video" => "videos".to_string(),
        "audio" => "audio".to_string(),
        _ => "backgrounds".to_string(),
    });
    let lib = bridge.library.lock().map_err(|e| e.to_string())?;
    lib.register_media_file_in_bin(
        path,
        &resolved_bin,
        &resolved_kind,
        None,
        None,
        None,
        has_alpha.unwrap_or(false),
    )
    .map_err(|e| e.to_string())
}

#[tauri::command]
fn delete_media_asset(
    bridge: State<'_, Arc<EngineBridgeState>>,
    blake3_hash: String,
) -> Result<bool, String> {
    let lib = bridge.library.lock().map_err(|e| e.to_string())?;
    lib.delete_media_asset(&blake3_hash)
        .map_err(|e| e.to_string())
}

fn asset_visual_preset(
    asset: &MediaAssetRecord,
) -> (ShaderMotionStyle, ColorRgba, ColorRgba, ColorRgba) {
    match asset.local_path.as_str() {
        "builtin://motion/sanctuary-glow" => (
            ShaderMotionStyle::SanctuaryGlow,
            ColorRgba::rgb(0.24, 0.08, 0.05),
            ColorRgba::rgb(0.76, 0.32, 0.10),
            ColorRgba::rgb(1.0, 0.88, 0.62),
        ),
        "builtin://motion/broadcast-grid" => (
            ShaderMotionStyle::BroadcastGrid,
            ColorRgba::rgb(0.08, 0.10, 0.12),
            ColorRgba::rgb(0.18, 0.24, 0.30),
            ColorRgba::rgb(0.20, 0.90, 0.55),
        ),
        "builtin://motion/lower-third-alpha" => (
            ShaderMotionStyle::LowerThirdAlpha,
            ColorRgba::rgba(0.05, 0.07, 0.11, 0.0),
            ColorRgba::rgba(0.12, 0.44, 0.88, 0.92),
            ColorRgba::rgb(1.0, 1.0, 1.0),
        ),
        _ if asset.kind == "video" => (
            ShaderMotionStyle::SanctuaryGlow,
            ColorRgba::rgb(0.07, 0.11, 0.24),
            ColorRgba::rgb(0.18, 0.36, 0.68),
            ColorRgba::rgb(0.68, 0.85, 1.0),
        ),
        _ => (
            ShaderMotionStyle::AuroraWave,
            ColorRgba::rgb(0.06, 0.14, 0.28),
            ColorRgba::rgb(0.08, 0.42, 0.56),
            ColorRgba::rgb(0.95, 0.78, 0.35),
        ),
    }
}

#[tauri::command]
fn move_presentation_to_library(
    bridge: State<'_, Arc<EngineBridgeState>>,
    song_id: String,
    library_id: String,
) -> Result<bool, String> {
    let lib = bridge.library.lock().map_err(|e| e.to_string())?;
    lib.move_song_to_library(&song_id, &library_id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn move_media_to_bin(
    bridge: State<'_, Arc<EngineBridgeState>>,
    blake3_hash: String,
    bin_id: String,
) -> Result<bool, String> {
    let lib = bridge.library.lock().map_err(|e| e.to_string())?;
    lib.move_media_to_bin(&blake3_hash, &bin_id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn toggle_video_input(
    bridge: State<'_, Arc<EngineBridgeState>>,
    active: bool,
) -> Result<SceneState, String> {
    apply_scene_diff(&bridge, SceneDiff::ToggleVideoInput(active)).await
}

#[tauri::command]
async fn attach_media_to_slide(
    bridge: State<'_, Arc<EngineBridgeState>>,
    slide_id: String,
    blake3_hash: Option<String>,
    role: Option<String>,
) -> Result<SceneState, String> {
    let sid = SlideId(Uuid::parse_str(&slide_id).map_err(|e| e.to_string())?);
    let cue = match blake3_hash.as_deref().map(str::trim) {
        Some(hash) if !hash.is_empty() => {
            let asset = {
                let lib = bridge.library.lock().map_err(|e| e.to_string())?;
                lib.list_media_assets()
                    .map_err(|e| e.to_string())?
                    .into_iter()
                    .find(|a| a.blake3_hash == hash)
                    .ok_or_else(|| format!("media asset '{hash}' not found"))?
            };
            let parsed_role = match role.as_deref().unwrap_or("auto") {
                "background" => MediaCueRole::Background,
                "foreground" => MediaCueRole::Foreground,
                "announcement" => MediaCueRole::Announcement,
                "audio" => MediaCueRole::Audio,
                _ => match asset.kind.as_str() {
                    "background" => MediaCueRole::Background,
                    "audio" => MediaCueRole::Audio,
                    _ => MediaCueRole::Foreground,
                },
            };
            let (motion_style, primary_color, secondary_color, accent_color) =
                asset_visual_preset(&asset);
            Some(SlideMediaCue {
                label: asset.file_name,
                media_path: asset.local_path,
                kind: asset.kind,
                role: parsed_role,
                motion_style,
                primary_color,
                secondary_color,
                accent_color,
            })
        }
        _ => None,
    };
    apply_scene_diff(&bridge, SceneDiff::AttachMediaToSlide { slide_id: sid, cue }).await
}

#[tauri::command]
async fn trigger_media_asset(
    bridge: State<'_, Arc<EngineBridgeState>>,
    blake3_hash: String,
    role: Option<String>,
) -> Result<SceneState, String> {
    let asset = {
        let lib = bridge.library.lock().map_err(|e| e.to_string())?;
        lib.list_media_assets()
            .map_err(|e| e.to_string())?
            .into_iter()
            .find(|a| a.blake3_hash == blake3_hash)
            .ok_or_else(|| format!("media asset '{blake3_hash}' not found"))?
    };

    let resolved_role = match role.as_deref().unwrap_or("auto") {
        "background" => "background",
        "foreground" => "foreground",
        "announcement" => "announcement",
        "audio" => "audio",
        _ => match asset.kind.as_str() {
            "background" => "background",
            "audio" => "audio",
            _ => "foreground",
        },
    };

    match resolved_role {
        "audio" => {
            apply_scene_diff(
                &bridge,
                SceneDiff::SetAudioPlayback(Some(AudioPlaybackState {
                    title: asset.file_name,
                    media_path: asset.local_path,
                    playing: true,
                    looping: true,
                    volume: 0.85,
                })),
            )
            .await
        }
        "background" => {
            let (motion_style, primary_color, secondary_color, accent_color) =
                asset_visual_preset(&asset);
            apply_scene_diff(
                &bridge,
                SceneDiff::SetBackground(ActiveBackground {
                    label: asset.file_name,
                    media_path: asset.local_path,
                    motion_style,
                    primary_color,
                    secondary_color,
                    accent_color,
                }),
            )
            .await
        }
        "announcement" => {
            apply_scene_diff(
                &bridge,
                SceneDiff::SetForegroundMedia(ForegroundMedia {
                    label: asset.file_name,
                    media_path: asset.local_path,
                    kind: asset.kind,
                    target_layer: LayerKind::Announcements,
                }),
            )
            .await
        }
        _ => {
            apply_scene_diff(
                &bridge,
                SceneDiff::SetForegroundMedia(ForegroundMedia {
                    label: asset.file_name,
                    media_path: asset.local_path,
                    kind: asset.kind,
                    target_layer: LayerKind::Slide,
                }),
            )
            .await
        }
    }
}

#[derive(serde::Serialize)]
struct BroadcastCapabilitiesDto {
    ndi_status: String,
    omt_ready: bool,
    decklink_status: String,
    virtual_output_backend: String,
    web_stage_url: String,
    web_musician_url: String,
    web_remote_url: String,
    remote_pin: String,
    active_source_name: String,
    alpha_mode: String,
    decklink_mode: String,
    streaming_active: bool,
}

fn detect_lan_ipv4() -> String {
    std::net::UdpSocket::bind("0.0.0.0:0")
        .and_then(|sock| {
            sock.connect("8.8.8.8:80")?;
            sock.local_addr()
        })
        .map(|addr| addr.ip().to_string())
        .unwrap_or_else(|_| "127.0.0.1".to_string())
}

fn read_web_stage_port() -> u16 {
    std::fs::read_to_string(
        std::env::temp_dir()
            .join("openpresenter-data")
            .join("web_stage_port.txt"),
    )
    .ok()
    .and_then(|s| s.trim().parse::<u16>().ok())
    .unwrap_or(5505)
}

#[tauri::command]
fn get_broadcast_capabilities(
    bridge: State<'_, Arc<EngineBridgeState>>,
) -> BroadcastCapabilitiesDto {
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
    let lan_ip = detect_lan_ipv4();
    let port = read_web_stage_port();
    let profile = bridge
        .broadcast_profile
        .lock()
        .map(|g| g.clone())
        .unwrap_or_default();
    BroadcastCapabilitiesDto {
        ndi_status,
        omt_ready: true,
        decklink_status,
        virtual_output_backend: format!("{:?}", broadcast_io::VirtualOutputBackend::native_for_host()),
        web_stage_url: format!("http://{lan_ip}:{port}/"),
        web_musician_url: format!("http://{lan_ip}:{port}/musician"),
        web_remote_url: format!("http://{lan_ip}:{port}/remote"),
        remote_pin: "2026".to_string(),
        active_source_name: profile.source_name,
        alpha_mode: profile.alpha_mode,
        decklink_mode: profile.decklink_mode,
        streaming_active: profile.streaming_active,
    }
}

#[tauri::command]
fn apply_broadcast_profile(
    bridge: State<'_, Arc<EngineBridgeState>>,
    source_name: String,
    alpha_mode: String,
    decklink_mode: String,
    streaming_active: bool,
) -> Result<BroadcastCapabilitiesDto, String> {
    let trimmed = source_name.trim();
    if trimmed.is_empty() {
        return Err("Broadcast source name cannot be empty".to_string());
    }
    let resolved_alpha = match alpha_mode.as_str() {
        "StraightBgra" | "StraightAlpha" => broadcast_io::AlphaMode::StraightBgra,
        "KeyFillSplit" => broadcast_io::AlphaMode::KeyFillSplit,
        _ => broadcast_io::AlphaMode::PremultipliedBgra,
    };
    let _ndi_cfg = broadcast_io::NdiConfig {
        source_name: trimmed.to_string(),
        groups: None,
        clock_video: true,
        alpha_mode: resolved_alpha,
    };
    let _dl_mode = match decklink_mode.as_str() {
        "InternalKeyer" | "InternalKey" => broadcast_io::DeckLinkKeyMode::InternalKeyer {
            subdevice: 1,
            level: 255,
        },
        _ => broadcast_io::DeckLinkKeyMode::ExternalDualSdiKeyFill {
            fill_subdevice: 1,
            key_subdevice: 2,
        },
    };
    {
        let mut guard = bridge.broadcast_profile.lock().map_err(|e| e.to_string())?;
        guard.source_name = trimmed.to_string();
        guard.alpha_mode = alpha_mode;
        guard.decklink_mode = decklink_mode;
        guard.streaming_active = streaming_active;
    }
    Ok(get_broadcast_capabilities(bridge))
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
fn list_timers(bridge: State<'_, Arc<EngineBridgeState>>) -> Result<Vec<TimerStatusDto>, String> {
    let timers = bridge.timers.lock().map_err(|e| e.to_string())?;
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

#[tauri::command]
async fn control_timer(
    bridge: State<'_, Arc<EngineBridgeState>>,
    timer_id: String,
    action: String,
) -> Result<Vec<TimerStatusDto>, String> {
    let tid = show_control::TimerId(Uuid::parse_str(&timer_id).map_err(|e| e.to_string())?);
    let (list, push_text) = {
        let mut timers = bridge.timers.lock().map_err(|e| e.to_string())?;
        match action.as_str() {
            "start" => {
                let _ = timers.start(tid);
            }
            "pause" => {
                let _ = timers.pause(tid);
            }
            "reset" => {
                let _ = timers.reset(tid);
            }
            "add_minute" => {
                let _ = timers.add_seconds(tid, 60);
            }
            _ => {}
        }
        let pushed = timers.get(tid).map(|t| t.formatted);
        let all = timers
            .list()
            .into_iter()
            .map(|t| TimerStatusDto {
                id: t.id.0.to_string(),
                name: t.name,
                formatted: t.formatted,
                running: t.running,
                overrun: t.overrun,
            })
            .collect::<Vec<_>>();
        (all, pushed)
    };
    if let Some(txt) = push_text {
        let _ = apply_scene_diff(&bridge, SceneDiff::SetTimerReadout(Some(txt))).await;
    }
    Ok(list)
}

#[tauri::command]
async fn tick_timers(
    bridge: State<'_, Arc<EngineBridgeState>>,
    elapsed_ms: u64,
) -> Result<Vec<TimerStatusDto>, String> {
    let (list, running_readout) = {
        let mut timers = bridge.timers.lock().map_err(|e| e.to_string())?;
        timers.tick_ms(elapsed_ms.min(10_000));
        let all = timers.list();
        let active_fmt = all.iter().find(|t| t.running).map(|t| t.formatted.clone());
        let dtos = all
            .into_iter()
            .map(|t| TimerStatusDto {
                id: t.id.0.to_string(),
                name: t.name,
                formatted: t.formatted,
                running: t.running,
                overrun: t.overrun,
            })
            .collect::<Vec<_>>();
        (dtos, active_fmt)
    };
    if let Some(fmt) = running_readout {
        let _ = apply_scene_diff(&bridge, SceneDiff::SetTimerReadout(Some(fmt))).await;
    }
    Ok(list)
}

#[tauri::command]
fn list_overlays(bridge: State<'_, Arc<EngineBridgeState>>) -> Result<Vec<OverlayItem>, String> {
    let lib = bridge.library.lock().map_err(|e| e.to_string())?;
    lib.list_overlays().map_err(|e| e.to_string())
}

#[tauri::command]
async fn upsert_overlay(
    bridge: State<'_, Arc<EngineBridgeState>>,
    overlay: OverlayItem,
) -> Result<SceneState, String> {
    let state = apply_scene_diff(&bridge, SceneDiff::UpsertOverlay(overlay.clone())).await?;
    let lib = bridge.library.lock().map_err(|e| e.to_string())?;
    lib.upsert_overlay(&overlay).map_err(|e| e.to_string())?;
    Ok(state)
}

#[tauri::command]
async fn delete_overlay(
    bridge: State<'_, Arc<EngineBridgeState>>,
    overlay_id: String,
) -> Result<SceneState, String> {
    let state = apply_scene_diff(&bridge, SceneDiff::RemoveOverlay(overlay_id.clone())).await?;
    let lib = bridge.library.lock().map_err(|e| e.to_string())?;
    let _ = lib.delete_overlay(&overlay_id).map_err(|e| e.to_string())?;
    Ok(state)
}

#[tauri::command]
async fn toggle_overlay_item(
    bridge: State<'_, Arc<EngineBridgeState>>,
    overlay_id: String,
    active: bool,
) -> Result<SceneState, String> {
    apply_scene_diff(
        &bridge,
        SceneDiff::ToggleOverlayItem {
            id: overlay_id,
            active,
        },
    )
    .await
}

#[tauri::command]
fn list_message_templates(
    bridge: State<'_, Arc<EngineBridgeState>>,
) -> Result<Vec<MessageTemplate>, String> {
    let lib = bridge.library.lock().map_err(|e| e.to_string())?;
    lib.list_message_templates().map_err(|e| e.to_string())
}

#[tauri::command]
fn upsert_message_template(
    bridge: State<'_, Arc<EngineBridgeState>>,
    template: MessageTemplate,
) -> Result<Vec<MessageTemplate>, String> {
    if template.name.trim().is_empty() || template.template_text.trim().is_empty() {
        return Err("Message template name and text cannot be empty".to_string());
    }
    let lib = bridge.library.lock().map_err(|e| e.to_string())?;
    lib.upsert_message_template(&template)
        .map_err(|e| e.to_string())?;
    lib.list_message_templates().map_err(|e| e.to_string())
}

#[tauri::command]
fn delete_message_template(
    bridge: State<'_, Arc<EngineBridgeState>>,
    template_id: String,
) -> Result<Vec<MessageTemplate>, String> {
    let lib = bridge.library.lock().map_err(|e| e.to_string())?;
    let _ = lib
        .delete_message_template(&template_id)
        .map_err(|e| e.to_string())?;
    lib.list_message_templates().map_err(|e| e.to_string())
}

#[tauri::command]
async fn trigger_message(
    bridge: State<'_, Arc<EngineBridgeState>>,
    template_id: Option<String>,
    template_text: String,
    tokens: BTreeMap<String, String>,
    show_on_audience: bool,
    show_on_stage: bool,
) -> Result<SceneState, String> {
    let rendered = render_message_template(&template_text, &tokens);
    if rendered.trim().is_empty() {
        return Err("Message text cannot be empty".to_string());
    }
    apply_scene_diff(
        &bridge,
        SceneDiff::SetActiveMessage(Some(ActiveMessage {
            template_id,
            rendered_text: rendered,
            show_on_audience,
            show_on_stage,
        })),
    )
    .await
}

#[tauri::command]
async fn clear_message(bridge: State<'_, Arc<EngineBridgeState>>) -> Result<SceneState, String> {
    apply_scene_diff(&bridge, SceneDiff::SetActiveMessage(None)).await
}

#[tauri::command]
async fn set_variable(
    bridge: State<'_, Arc<EngineBridgeState>>,
    key: String,
    value: Option<String>,
) -> Result<SceneState, String> {
    apply_scene_diff(&bridge, SceneDiff::SetVariable { key, value }).await
}

#[tauri::command]
async fn set_slide_automation(
    bridge: State<'_, Arc<EngineBridgeState>>,
    slide_id: String,
    auto_advance_secs: Option<f32>,
    loop_to_first: bool,
    actions: Vec<SlideAction>,
) -> Result<SceneState, String> {
    let sid = SlideId(Uuid::parse_str(&slide_id).map_err(|e| e.to_string())?);
    let state = apply_scene_diff(
        &bridge,
        SceneDiff::SetSlideAutomation {
            slide_id: sid,
            auto_advance_secs,
            loop_to_first,
            actions,
        },
    )
    .await?;
    if let Ok(guard) = bridge.active_song_id.lock()
        && let Some(song_id) = guard.as_deref()
        && let Ok(lib) = bridge.library.lock()
    {
        let slides_to_save = if state.master_deck.is_empty() {
            &state.deck
        } else {
            &state.master_deck
        };
        let _ = lib.update_song_slides_and_cues(song_id, Some(slides_to_save), None);
    }
    Ok(state)
}

#[derive(serde::Deserialize)]
struct SlideTimingInput {
    slide_id: String,
    auto_advance_secs: Option<f32>,
    loop_to_first: bool,
}

#[tauri::command]
async fn apply_recorded_cues(
    bridge: State<'_, Arc<EngineBridgeState>>,
    timings: Vec<SlideTimingInput>,
    cue_track: Option<AutoPlayTrack>,
) -> Result<SceneState, String> {
    let mut parsed = Vec::with_capacity(timings.len());
    for t in timings {
        let sid = SlideId(Uuid::parse_str(&t.slide_id).map_err(|e| e.to_string())?);
        parsed.push((sid, t.auto_advance_secs, t.loop_to_first));
    }
    let mut state = if !parsed.is_empty() {
        apply_scene_diff(&bridge, SceneDiff::BatchSetSlideTimings { timings: parsed }).await?
    } else {
        refresh_snapshot_and_buses(&bridge, None).await?.state
    };
    if let Some(track) = cue_track {
        let mut tracks = state.active_cue_tracks.clone();
        if let Some(existing) = tracks.iter_mut().find(|x| x.id == track.id) {
            *existing = track;
        } else {
            tracks.push(track);
        }
        state = apply_scene_diff(&bridge, SceneDiff::SetCueTracks(tracks)).await?;
    }
    if let Ok(guard) = bridge.active_song_id.lock()
        && let Some(song_id) = guard.as_deref()
        && let Ok(lib) = bridge.library.lock()
    {
        let slides_to_save = if state.master_deck.is_empty() {
            &state.deck
        } else {
            &state.master_deck
        };
        let _ = lib.update_song_slides_and_cues(
            song_id,
            Some(slides_to_save),
            Some(&state.active_cue_tracks),
        );
    }
    Ok(state)
}

#[tauri::command]
async fn delete_cue_track(
    bridge: State<'_, Arc<EngineBridgeState>>,
    track_id: String,
) -> Result<SceneState, String> {
    let cur = refresh_snapshot_and_buses(&bridge, None).await?.state;
    let mut tracks = cur.active_cue_tracks;
    tracks.retain(|t| t.id != track_id);
    let state = apply_scene_diff(&bridge, SceneDiff::SetCueTracks(tracks)).await?;
    if let Ok(guard) = bridge.active_song_id.lock()
        && let Some(song_id) = guard.as_deref()
        && let Ok(lib) = bridge.library.lock()
    {
        let _ = lib.update_song_slides_and_cues(song_id, None, Some(&state.active_cue_tracks));
    }
    Ok(state)
}

#[tauri::command]
fn list_macros(bridge: State<'_, Arc<EngineBridgeState>>) -> Result<Vec<MacroRecord>, String> {
    let lib = bridge.library.lock().map_err(|e| e.to_string())?;
    lib.list_macros().map_err(|e| e.to_string())
}

#[tauri::command]
fn save_macro(
    bridge: State<'_, Arc<EngineBridgeState>>,
    id: Option<String>,
    name: String,
    steps: Vec<String>,
) -> Result<Vec<MacroRecord>, String> {
    if name.trim().is_empty() || steps.is_empty() {
        return Err("Macro name and at least one step are required".to_string());
    }
    let rec = MacroRecord {
        id: id
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| Uuid::new_v4().to_string()),
        name: name.trim().to_string(),
        steps,
        updated_at: chrono::Utc::now(),
    };
    let lib = bridge.library.lock().map_err(|e| e.to_string())?;
    lib.upsert_macro(&rec).map_err(|e| e.to_string())?;
    lib.list_macros().map_err(|e| e.to_string())
}

#[tauri::command]
fn delete_macro(
    bridge: State<'_, Arc<EngineBridgeState>>,
    macro_id: String,
) -> Result<Vec<MacroRecord>, String> {
    let lib = bridge.library.lock().map_err(|e| e.to_string())?;
    let _ = lib.delete_macro(&macro_id).map_err(|e| e.to_string())?;
    lib.list_macros().map_err(|e| e.to_string())
}

async fn execute_macro_step(
    bridge: &EngineBridgeState,
    step: &str,
) -> Result<Option<SceneState>, String> {
    let trimmed = step.trim();
    if trimmed.eq_ignore_ascii_case("clear:all") {
        return Ok(Some(apply_scene_diff(bridge, SceneDiff::ClearAll).await?));
    }
    if trimmed.eq_ignore_ascii_case("clear:slide") {
        return Ok(Some(
            apply_scene_diff(bridge, SceneDiff::ClearLayer(LayerKind::Slide)).await?,
        ));
    }
    if trimmed.eq_ignore_ascii_case("blackout:on") {
        return Ok(Some(apply_scene_diff(bridge, SceneDiff::SetBlackout(true)).await?));
    }
    if trimmed.eq_ignore_ascii_case("blackout:off") {
        return Ok(Some(apply_scene_diff(bridge, SceneDiff::SetBlackout(false)).await?));
    }
    if let Some(ov_id) = trimmed.strip_prefix("overlay:show:") {
        return Ok(Some(
            apply_scene_diff(
                bridge,
                SceneDiff::ToggleOverlayItem {
                    id: ov_id.trim().to_string(),
                    active: true,
                },
            )
            .await?,
        ));
    }
    if let Some(ov_id) = trimmed.strip_prefix("overlay:hide:") {
        return Ok(Some(
            apply_scene_diff(
                bridge,
                SceneDiff::ToggleOverlayItem {
                    id: ov_id.trim().to_string(),
                    active: false,
                },
            )
            .await?,
        ));
    }
    if let Some(mask_name) = trimmed.strip_prefix("mask:") {
        let mask = match mask_name.trim() {
            "SoftVignette" => MaskStyle::SoftVignette,
            "RoundedFrame" => MaskStyle::RoundedFrame,
            "LowerThirdBand" => MaskStyle::LowerThirdBand,
            _ => MaskStyle::None,
        };
        return Ok(Some(apply_scene_diff(bridge, SceneDiff::SetMask(mask)).await?));
    }
    if let Some(rest) = trimmed.strip_prefix("var:")
        && let Some((k, v)) = rest.split_once('=')
    {
        return Ok(Some(
            apply_scene_diff(
                bridge,
                SceneDiff::SetVariable {
                    key: k.trim().to_string(),
                    value: Some(v.trim().to_string()),
                },
            )
            .await?,
        ));
    }
    Ok(None)
}

#[tauri::command]
async fn run_macro(
    bridge: State<'_, Arc<EngineBridgeState>>,
    macro_id: String,
) -> Result<Option<SceneState>, String> {
    let steps = {
        let lib = bridge.library.lock().map_err(|e| e.to_string())?;
        lib.list_macros()
            .map_err(|e| e.to_string())?
            .into_iter()
            .find(|m| m.id == macro_id)
            .ok_or_else(|| format!("Macro '{macro_id}' not found"))?
            .steps
    };
    let mut last_state = None;
    for step in &steps {
        if let Ok(Some(st)) = execute_macro_step(&bridge, step).await {
            last_state = Some(st);
        }
    }
    Ok(last_state)
}

#[tauri::command]
fn list_scheduled_events(
    bridge: State<'_, Arc<EngineBridgeState>>,
) -> Result<Vec<ScheduledEventRecord>, String> {
    let lib = bridge.library.lock().map_err(|e| e.to_string())?;
    lib.list_scheduled_events().map_err(|e| e.to_string())
}

#[tauri::command]
fn save_scheduled_event(
    bridge: State<'_, Arc<EngineBridgeState>>,
    id: Option<String>,
    name: String,
    trigger_hhmm: String,
    macro_id: String,
    enabled: bool,
) -> Result<Vec<ScheduledEventRecord>, String> {
    if name.trim().is_empty() || trigger_hhmm.trim().len() != 5 {
        return Err("Provide event name and HH:MM trigger time".to_string());
    }
    let rec = ScheduledEventRecord {
        id: id
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| Uuid::new_v4().to_string()),
        name: name.trim().to_string(),
        trigger_hhmm: trigger_hhmm.trim().to_string(),
        macro_id,
        enabled,
        updated_at: chrono::Utc::now(),
    };
    let lib = bridge.library.lock().map_err(|e| e.to_string())?;
    lib.upsert_scheduled_event(&rec).map_err(|e| e.to_string())?;
    lib.list_scheduled_events().map_err(|e| e.to_string())
}

#[tauri::command]
fn delete_scheduled_event(
    bridge: State<'_, Arc<EngineBridgeState>>,
    event_id: String,
) -> Result<Vec<ScheduledEventRecord>, String> {
    let lib = bridge.library.lock().map_err(|e| e.to_string())?;
    let _ = lib
        .delete_scheduled_event(&event_id)
        .map_err(|e| e.to_string())?;
    lib.list_scheduled_events().map_err(|e| e.to_string())
}

#[tauri::command]
async fn dispatch_control_command(
    bridge: State<'_, Arc<EngineBridgeState>>,
    protocol: String,
    command: String,
) -> Result<String, String> {
    let trimmed = command.trim();
    match protocol.as_str() {
        "companion" => {
            let parsed = show_control::parse_satellite_line(trimmed).map_err(|e| e.to_string())?;
            Ok(format!("Companion Satellite parsed: {parsed:?}"))
        }
        "osc" => {
            let evt = show_control::osc::parse_osc_text_command(trimmed).map_err(|e| e.to_string())?;
            if let show_control::osc::OscControlEvent::Scene(diff) = evt.clone() {
                let _ = apply_scene_diff(&bridge, *diff).await?;
            }
            Ok(format!("OSC executed: {evt:?}"))
        }
        _ => {
            let _ = execute_macro_step(&bridge, trimmed).await?;
            Ok(format!("Executed control step: {trimmed}"))
        }
    }
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

fn persist_imported_presentation(
    lib: &LibraryStore,
    imported: integrations::PresentationImportResult,
) -> Result<ImportSummaryDto, String> {
    let slide_count = imported.slides.len();
    let rec = SongRecord {
        id: Uuid::new_v4().to_string(),
        library_id: DEFAULT_LIBRARY_ID.to_string(),
        title: imported.title.clone(),
        author: String::new(),
        copyright: String::new(),
        ccli_number: imported.ccli_number.clone(),
        key_signature: None,
        bpm: None,
        lyrics_plain: imported
            .slides
            .iter()
            .map(|s| s.plain_text())
            .collect::<Vec<_>>()
            .join("\n"),
        slides: imported.slides,
        arrangements: Vec::new(),
        cue_tracks: Vec::new(),
        chord_chart: None,
        sheet_music: None,
        updated_at: chrono::Utc::now(),
    };
    lib.upsert_song(&rec).map_err(|e| e.to_string())?;
    Ok(ImportSummaryDto {
        title: imported.title,
        slide_count,
        ccli_number: imported.ccli_number,
        source_format: imported.source_format,
    })
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
            let chord_chart = Some(integrations::parse_chordpro_to_chart(&content, Some(&song.title)));
            let slide_count = song.slides.len();
            let rec = SongRecord {
                id: Uuid::new_v4().to_string(),
                library_id: DEFAULT_LIBRARY_ID.to_string(),
                title: song.title.clone(),
                author: song.author,
                copyright: song.copyright,
                ccli_number: song.ccli_number.clone(),
                key_signature: song.key,
                bpm: chord_chart.as_ref().and_then(|c| c.tempo_bpm),
                lyrics_plain: song.slides.iter().map(|s| s.plain_text()).collect::<Vec<_>>().join("\n"),
                slides: song.slides,
                arrangements: Vec::new(),
                cue_tracks: Vec::new(),
                chord_chart,
                sheet_music: None,
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
                library_id: DEFAULT_LIBRARY_ID.to_string(),
                title: song.title.clone(),
                author: song.author,
                copyright: song.copyright,
                ccli_number: song.ccli_number.clone(),
                key_signature: None,
                bpm: None,
                lyrics_plain: song.slides.iter().map(|s| s.plain_text()).collect::<Vec<_>>().join("\n"),
                slides: song.slides,
                arrangements: Vec::new(),
                cue_tracks: Vec::new(),
                chord_chart: None,
                sheet_music: None,
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
            persist_imported_presentation(&lib, imported)
        }
        "propresenter_xml" => {
            let imported = integrations::import_pro6_xml(&content).map_err(|e| e.to_string())?;
            persist_imported_presentation(&lib, imported)
        }
        "openlyrics_xml" => {
            let imported =
                integrations::import_openlyrics_xml(&content).map_err(|e| e.to_string())?;
            persist_imported_presentation(&lib, imported)
        }
        "opensong_xml" => {
            let imported =
                integrations::import_opensong_xml(&content).map_err(|e| e.to_string())?;
            persist_imported_presentation(&lib, imported)
        }
        "freeshow_json" => {
            let imported =
                integrations::import_freeshow_json(&content).map_err(|e| e.to_string())?;
            persist_imported_presentation(&lib, imported)
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
fn import_worship_file(
    bridge: State<'_, Arc<EngineBridgeState>>,
    format: String,
    file_path: String,
) -> Result<ImportSummaryDto, String> {
    let trimmed = file_path.trim();
    if trimmed.is_empty() {
        return Err("Provide a local file path to import".to_string());
    }
    let path = PathBuf::from(trimmed);
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Imported Deck")
        .to_string();

    if format == "pdf"
        || path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("pdf"))
    {
        let bytes = std::fs::read(&path)
            .map_err(|e| format!("failed to read '{}': {e}", path.display()))?;
        let parsed = integrations::import_lyric_sheet_pdf(&bytes, Some(&stem))
            .map_err(|e| e.to_string())?;
        let lib = bridge.library.lock().map_err(|e| e.to_string())?;
        let slide_count = parsed.slides.len();
        let rec = SongRecord {
            id: Uuid::new_v4().to_string(),
            library_id: DEFAULT_LIBRARY_ID.to_string(),
            title: parsed.title.clone(),
            author: parsed.author,
            copyright: parsed.copyright,
            ccli_number: parsed.ccli_number.clone(),
            key_signature: parsed.key_signature,
            bpm: parsed.tempo_bpm,
            lyrics_plain: parsed
                .slides
                .iter()
                .map(|s| s.plain_text())
                .collect::<Vec<_>>()
                .join("\n"),
            slides: parsed.slides,
            arrangements: parsed.arrangements,
            cue_tracks: Vec::new(),
            chord_chart: parsed.chord_chart,
            sheet_music: None,
            updated_at: chrono::Utc::now(),
        };
        lib.upsert_song(&rec).map_err(|e| e.to_string())?;
        return Ok(ImportSummaryDto {
            title: parsed.title,
            slide_count,
            ccli_number: parsed.ccli_number,
            source_format: "Lyric Sheet PDF".into(),
        });
    }

    if format == "pptx" || path.extension().and_then(|e| e.to_str()).is_some_and(|e| e.eq_ignore_ascii_case("pptx")) {
        let bytes = std::fs::read(&path).map_err(|e| format!("failed to read '{}': {e}", path.display()))?;
        let imported = integrations::import_pptx_bytes(&stem, &bytes).map_err(|e| e.to_string())?;
        let lib = bridge.library.lock().map_err(|e| e.to_string())?;
        return persist_imported_presentation(&lib, imported);
    }

    let content = std::fs::read_to_string(&path)
        .map_err(|e| format!("failed to read '{}': {e}", path.display()))?;
    let auto_format = if format == "auto" {
        match path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase()
            .as_str()
        {
            "show" | "json" => "freeshow_json".to_string(),
            "pro6" | "pro5" => "propresenter_xml".to_string(),
            "cho" | "chopro" => "chordpro".to_string(),
            "usr" => "songselect_usr".to_string(),
            "xml" => {
                if content.contains("<RVPresentationDocument") {
                    "propresenter_xml".to_string()
                } else if content.contains("<lyrics>") && content.contains("<ccli>") {
                    "opensong_xml".to_string()
                } else {
                    "openlyrics_xml".to_string()
                }
            }
            _ => "plain_text".to_string(),
        }
    } else {
        format
    };
    import_worship_document(bridge, auto_format, content)
}

#[derive(serde::Serialize)]
struct PdfImportResultDto {
    song_id: String,
    library_id: String,
    title: String,
    author: String,
    ccli_number: Option<String>,
    slide_count: usize,
    arrangement_count: usize,
    scene: Option<SceneState>,
}

fn managed_imported_media_dir() -> PathBuf {
    std::env::temp_dir()
        .join("openpresenter-data")
        .join("imported-media")
}

fn resolve_target_library_id(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed == "all" {
        DEFAULT_LIBRARY_ID.to_string()
    } else {
        trimmed.to_string()
    }
}

fn resolve_target_bin_id(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed == "all" {
        core_library::DEFAULT_MEDIA_BIN_ID.to_string()
    } else {
        trimmed.to_string()
    }
}

async fn persist_parsed_pdf_song(
    bridge: &EngineBridgeState,
    parsed: integrations::ParsedLyricSheet,
    library_id: &str,
    load_to_deck: bool,
) -> Result<PdfImportResultDto, String> {
    let resolved_lib = resolve_target_library_id(library_id);
    let song_id = Uuid::new_v4().to_string();
    let slide_count = parsed.slides.len();
    let arrangement_count = parsed.arrangements.len();
    let rec = SongRecord {
        id: song_id.clone(),
        library_id: resolved_lib.clone(),
        title: parsed.title.clone(),
        author: parsed.author.clone(),
        copyright: parsed.copyright,
        ccli_number: parsed.ccli_number.clone(),
        key_signature: parsed.key_signature,
        bpm: parsed.tempo_bpm,
        lyrics_plain: parsed
            .slides
            .iter()
            .map(|s| s.plain_text())
            .collect::<Vec<_>>()
            .join("\n"),
        slides: parsed.slides.clone(),
        arrangements: parsed.arrangements.clone(),
        cue_tracks: Vec::new(),
        chord_chart: parsed.chord_chart.clone(),
        sheet_music: None,
        updated_at: chrono::Utc::now(),
    };
    {
        let lib = bridge.library.lock().map_err(|e| e.to_string())?;
        lib.upsert_song(&rec).map_err(|e| e.to_string())?;
    }

    let scene = if load_to_deck {
        if let Ok(mut g) = bridge.active_song_id.lock() {
            *g = Some(song_id.clone());
        }
        let active_id = parsed.arrangements.first().map(|a| a.id.clone());
        let _ = apply_scene_diff(
            bridge,
            SceneDiff::LoadPresentation {
                slides: parsed.slides,
                arrangements: parsed.arrangements,
                active_arrangement_id: active_id,
            },
        )
        .await?;
        let _ = apply_scene_diff(bridge, SceneDiff::SetCueTracks(Vec::new())).await;
        let _ = apply_scene_diff(
            bridge,
            SceneDiff::SetActiveMusicSheets {
                chord_chart: parsed.chord_chart,
                sheet_music: None,
            },
        )
        .await;
        let st = apply_scene_diff(
            bridge,
            SceneDiff::SetActivePresentationMeta {
                id: Some(song_id.clone()),
                title: Some(parsed.title.clone()),
                author: (!rec.author.is_empty()).then_some(rec.author),
                copyright: (!rec.copyright.is_empty()).then_some(rec.copyright),
                ccli_number: rec.ccli_number,
            },
        )
        .await?;
        Some(st)
    } else {
        None
    };

    Ok(PdfImportResultDto {
        song_id,
        library_id: resolved_lib,
        title: parsed.title,
        author: parsed.author,
        ccli_number: parsed.ccli_number,
        slide_count,
        arrangement_count,
        scene,
    })
}

#[tauri::command]
async fn import_lyric_sheet_pdf_file(
    bridge: State<'_, Arc<EngineBridgeState>>,
    file_path: String,
    library_id: String,
    load_to_deck: bool,
) -> Result<PdfImportResultDto, String> {
    let trimmed = file_path.trim();
    if trimmed.is_empty() {
        return Err("Provide a local PDF lyric sheet path".to_string());
    }
    let path = PathBuf::from(trimmed);
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Imported Lyric Sheet");
    let bytes = std::fs::read(&path)
        .map_err(|e| format!("failed to read PDF '{}': {e}", path.display()))?;
    let parsed = integrations::import_lyric_sheet_pdf(&bytes, Some(stem))
        .map_err(|e| e.to_string())?;
    persist_parsed_pdf_song(&bridge, parsed, &library_id, load_to_deck).await
}

#[derive(serde::Serialize)]
struct SafeMediaImportDto {
    asset: MediaAssetRecord,
    safe_file_name: String,
    deduplicated: bool,
}

fn persist_staged_media(
    bridge: &EngineBridgeState,
    context_label: &str,
    original_filename: &str,
    bytes: &[u8],
    target_bin_id: &str,
) -> Result<SafeMediaImportDto, String> {
    let staged = integrations::stage_imported_media_bytes(
        &managed_imported_media_dir(),
        context_label,
        original_filename,
        bytes,
    )
    .map_err(|e| e.to_string())?;
    let resolved_bin = resolve_target_bin_id(target_bin_id);
    let record = MediaAssetRecord {
        blake3_hash: staged.blake3_hash,
        bin_id: resolved_bin,
        file_name: staged.display_name,
        local_path: staged.disk_path.to_string_lossy().into_owned(),
        kind: staged.media_kind,
        width: None,
        height: None,
        duration_ms: None,
        has_alpha: original_filename.to_ascii_lowercase().ends_with(".png"),
        size_bytes: staged.size_bytes,
    };
    let lib = bridge.library.lock().map_err(|e| e.to_string())?;
    lib.upsert_media_asset(&record).map_err(|e| e.to_string())?;
    Ok(SafeMediaImportDto {
        asset: record,
        safe_file_name: staged.safe_file_name,
        deduplicated: staged.deduplicated,
    })
}

#[tauri::command]
fn import_media_file_safe(
    bridge: State<'_, Arc<EngineBridgeState>>,
    file_path: String,
    context_label: String,
    target_bin_id: String,
) -> Result<SafeMediaImportDto, String> {
    let trimmed = file_path.trim();
    if trimmed.is_empty() {
        return Err("Provide a local media file path".to_string());
    }
    let path = PathBuf::from(trimmed);
    let orig_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("media.bin");
    let bytes = std::fs::read(&path)
        .map_err(|e| format!("failed to read '{}': {e}", path.display()))?;
    persist_staged_media(&bridge, &context_label, orig_name, &bytes, &target_bin_id)
}

fn pco_curl_request(
    method: &str,
    url: &str,
    app_id_or_bearer: &str,
    secret: Option<&str>,
) -> Result<Vec<u8>, String> {
    let auth_id = app_id_or_bearer.trim();
    if auth_id.is_empty() {
        return Err(
            "Enter your Planning Center App ID + Secret (or OAuth Bearer token)".to_string(),
        );
    }
    let curl_bin = if cfg!(windows) { "curl.exe" } else { "curl" };
    let mut cmd = Command::new(curl_bin);
    cmd.args(["-sS", "-L", "-f", "--max-time", "30", "-X", method]);
    if let Some(sec) = secret.map(str::trim).filter(|s| !s.is_empty()) {
        cmd.arg("-u").arg(format!("{auth_id}:{sec}"));
    } else {
        cmd.arg("-H").arg(format!("Authorization: Bearer {auth_id}"));
    }
    cmd.arg("-H").arg("Accept: application/json");
    cmd.arg(url);

    let out = cmd
        .output()
        .map_err(|e| format!("failed to invoke {curl_bin} for Planning Center request: {e}"))?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        return Err(format!("Planning Center API error: {}", stderr.trim()));
    }
    Ok(out.stdout)
}

fn pco_curl_download_signed_url(url: &str) -> Result<Vec<u8>, String> {
    let curl_bin = if cfg!(windows) { "curl.exe" } else { "curl" };
    let out = Command::new(curl_bin)
        .args(["-sS", "-L", "-f", "--max-time", "60", url])
        .output()
        .map_err(|e| format!("failed to download attachment via {curl_bin}: {e}"))?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        return Err(format!("Attachment download failed: {}", stderr.trim()));
    }
    Ok(out.stdout)
}

#[tauri::command]
fn pco_list_plans(
    app_id_or_bearer: String,
    secret: Option<String>,
    service_type_id: String,
) -> Result<Vec<integrations::PcoPlanSummary>, String> {
    let st_id = service_type_id.trim();
    if st_id.is_empty() {
        return Err("Provide a Planning Center Service Type ID".to_string());
    }
    let url = integrations::build_pco_plans_url(st_id);
    let body = pco_curl_request("GET", &url, &app_id_or_bearer, secret.as_deref())?;
    let json_str = String::from_utf8_lossy(&body);
    integrations::parse_pco_plans_json_api(&json_str).map_err(|e| e.to_string())
}

#[tauri::command]
fn pco_list_plan_attachments(
    app_id_or_bearer: String,
    secret: Option<String>,
    service_type_id: String,
    plan_id: String,
    plan_title: Option<String>,
) -> Result<Vec<integrations::PcoAttachment>, String> {
    let st_id = service_type_id.trim();
    let pid = plan_id.trim();
    if st_id.is_empty() || pid.is_empty() {
        return Err("Provide Service Type ID and Plan ID".to_string());
    }
    let url = integrations::build_pco_plan_attachments_url(st_id, pid);
    let body = pco_curl_request("GET", &url, &app_id_or_bearer, secret.as_deref())?;
    let json_str = String::from_utf8_lossy(&body);
    integrations::parse_pco_attachments_json_api(&json_str, plan_title.as_deref())
        .map_err(|e| e.to_string())
}

#[derive(serde::Serialize)]
struct PcoAttachmentImportDto {
    kind: String,
    summary: String,
    scene: Option<SceneState>,
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
async fn pco_import_attachment(
    bridge: State<'_, Arc<EngineBridgeState>>,
    app_id_or_bearer: String,
    secret: Option<String>,
    attachment: integrations::PcoAttachment,
    target_library_id: String,
    target_media_bin_id: String,
    load_to_deck: bool,
    import_as: Option<String>,
) -> Result<PcoAttachmentImportDto, String> {
    let download_url = if let Some(url) = attachment.download_url.clone() {
        url
    } else {
        let open_body = pco_curl_request(
            "POST",
            &attachment.open_endpoint,
            &app_id_or_bearer,
            secret.as_deref(),
        )?;
        let open_json = String::from_utf8_lossy(&open_body);
        integrations::parse_pco_open_attachment_url(&open_json).map_err(|e| e.to_string())?
    };

    let bytes = pco_curl_download_signed_url(&download_url)?;
    let fallback = PathBuf::from(&attachment.filename)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(&attachment.display_name)
        .to_string();

    let effective_mode = import_as
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());

    if effective_mode == Some("sheet_music")
        || (effective_mode.is_none()
            && attachment.kind == integrations::PcoAttachmentKind::SheetMusicPdf)
    {
        let sheet = integrations::import_sheet_music_pdf(
            &bytes,
            &attachment.display_name,
            Some(&attachment.filename),
            &managed_imported_media_dir(),
        )
        .map_err(|e| e.to_string())?;
        let page_count = sheet.pages.len();
        let scene = attach_music_sheet_to_active(&bridge, None, Some(sheet)).await?;
        return Ok(PcoAttachmentImportDto {
            kind: "SheetMusicPdf".into(),
            summary: format!(
                "Attached Sheet Music \"{}\" ({page_count} full-clef page(s)) to active song & Musician Stage",
                attachment.display_name
            ),
            scene: Some(scene),
        });
    }

    if effective_mode == Some("chord_chart")
        || (effective_mode.is_none()
            && attachment.kind == integrations::PcoAttachmentKind::ChordChartPdf)
    {
        let parsed = integrations::import_lyric_sheet_pdf(&bytes, Some(&fallback))
            .map_err(|e| e.to_string())?;
        if let Some(chart) = parsed.chord_chart.clone()
            && !load_to_deck
        {
            let sec_count = chart.sections.len();
            let scene = attach_music_sheet_to_active(&bridge, Some(chart), None).await?;
            return Ok(PcoAttachmentImportDto {
                kind: "ChordChartPdf".into(),
                summary: format!(
                    "Attached Chord Chart \"{}\" ({sec_count} sections) to active song & Musician Stage",
                    parsed.title
                ),
                scene: Some(scene),
            });
        }
        let sec_count = parsed
            .chord_chart
            .as_ref()
            .map(|c| c.sections.len())
            .unwrap_or(0);
        let res = persist_parsed_pdf_song(&bridge, parsed, &target_library_id, load_to_deck).await?;
        return Ok(PcoAttachmentImportDto {
            kind: "ChordChartPdf".into(),
            summary: format!(
                "Imported Chord Chart PDF \"{}\" ({} slides, {sec_count} chart sections) into '{}'",
                res.title, res.slide_count, res.library_id
            ),
            scene: res.scene,
        });
    }

    match attachment.kind {
        integrations::PcoAttachmentKind::LyricPdf
        | integrations::PcoAttachmentKind::ChordChartPdf
        | integrations::PcoAttachmentKind::SheetMusicPdf => {
            let parsed = integrations::import_lyric_sheet_pdf(&bytes, Some(&fallback))
                .map_err(|e| e.to_string())?;
            let res = persist_parsed_pdf_song(&bridge, parsed, &target_library_id, load_to_deck).await?;
            Ok(PcoAttachmentImportDto {
                kind: "LyricPdf".into(),
                summary: format!(
                    "Imported lyric PDF \"{}\" ({} slides{}) into library '{}'",
                    res.title,
                    res.slide_count,
                    res.ccli_number
                        .as_deref()
                        .map(|c| format!(", CCLI #{c}"))
                        .unwrap_or_default(),
                    res.library_id
                ),
                scene: res.scene,
            })
        }
        integrations::PcoAttachmentKind::Image
        | integrations::PcoAttachmentKind::Video
        | integrations::PcoAttachmentKind::Audio => {
            let staged = persist_staged_media(
                &bridge,
                &attachment.context_label,
                &attachment.filename,
                &bytes,
                &target_media_bin_id,
            )?;
            Ok(PcoAttachmentImportDto {
                kind: format!("{:?}", attachment.kind),
                summary: format!(
                    "Imported media \"{}\" as {} into bin '{}'{}",
                    staged.asset.file_name,
                    staged.safe_file_name,
                    staged.asset.bin_id,
                    if staged.deduplicated {
                        " (deduplicated)"
                    } else {
                        ""
                    }
                ),
                scene: None,
            })
        }
        integrations::PcoAttachmentKind::Other => Err(format!(
            "Unsupported Planning Center attachment type for '{}'",
            attachment.filename
        )),
    }
}

async fn attach_music_sheet_to_active(
    bridge: &EngineBridgeState,
    new_chart: Option<core_scene::ChordChartDocument>,
    new_sheet: Option<core_scene::SheetMusicDocument>,
) -> Result<SceneState, String> {
    let snap = refresh_snapshot_and_buses(bridge, None).await?;
    let merged_chart = new_chart.or(snap.state.active_chord_chart);
    let merged_sheet = new_sheet.or(snap.state.active_sheet_music);

    if let Ok(guard) = bridge.active_song_id.lock()
        && let Some(song_id) = guard.as_deref()
        && let Ok(lib) = bridge.library.lock()
    {
        let _ = lib.update_song_music_sheets(song_id, Some(&merged_chart), Some(&merged_sheet));
    }

    apply_scene_diff(
        bridge,
        SceneDiff::SetActiveMusicSheets {
            chord_chart: merged_chart,
            sheet_music: merged_sheet,
        },
    )
    .await
}

#[derive(serde::Serialize)]
struct MusicSheetImportDto {
    kind: String,
    title: String,
    count: usize,
    summary: String,
    scene: SceneState,
}

#[tauri::command]
async fn import_chord_chart_file(
    bridge: State<'_, Arc<EngineBridgeState>>,
    file_path: String,
    attach_to_active_song: bool,
    library_id: String,
    load_to_deck: bool,
) -> Result<MusicSheetImportDto, String> {
    let trimmed = file_path.trim();
    if trimmed.is_empty() {
        return Err("Provide a local Chord Chart PDF or ChordPro (.cho/.crd) file path".to_string());
    }
    let path = PathBuf::from(trimmed);
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Imported Chord Chart")
        .to_string();
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_ascii_lowercase())
        .unwrap_or_default();

    let (parsed_opt, chart) = if ext == "pdf" {
        let bytes = std::fs::read(&path)
            .map_err(|e| format!("failed to read '{}': {e}", path.display()))?;
        let parsed = integrations::import_lyric_sheet_pdf(&bytes, Some(&stem))
            .map_err(|e| e.to_string())?;
        let chart = parsed
            .chord_chart
            .clone()
            .ok_or_else(|| "No chord chart sections found in PDF".to_string())?;
        (Some(parsed), chart)
    } else {
        let content = std::fs::read_to_string(&path)
            .map_err(|e| format!("failed to read '{}': {e}", path.display()))?;
        let chart = integrations::parse_chordpro_to_chart(&content, Some(&stem));
        (None, chart)
    };

    let sec_count = chart.sections.len();
    let title = chart.title.clone();
    if attach_to_active_song {
        let scene = attach_music_sheet_to_active(&bridge, Some(chart), None).await?;
        return Ok(MusicSheetImportDto {
            kind: "ChordChart".into(),
            title: title.clone(),
            count: sec_count,
            summary: format!("Attached Chord Chart \"{title}\" ({sec_count} sections) to active song & Musician Stage"),
            scene,
        });
    }

    if let Some(parsed) = parsed_opt {
        let res = persist_parsed_pdf_song(&bridge, parsed, &library_id, load_to_deck).await?;
        let scene = match res.scene {
            Some(s) => s,
            None => refresh_snapshot_and_buses(&bridge, None).await?.state,
        };
        return Ok(MusicSheetImportDto {
            kind: "ChordChart".into(),
            title: res.title.clone(),
            count: sec_count,
            summary: format!(
                "Imported Chord Chart \"{}\" ({} slides, {sec_count} sections) into '{}'",
                res.title, res.slide_count, res.library_id
            ),
            scene,
        });
    }

    let scene = attach_music_sheet_to_active(&bridge, Some(chart), None).await?;
    Ok(MusicSheetImportDto {
        kind: "ChordChart".into(),
        title: title.clone(),
        count: sec_count,
        summary: format!("Loaded Chord Chart \"{title}\" ({sec_count} sections) to Musician Stage"),
        scene,
    })
}

#[tauri::command]
async fn import_sheet_music_file(
    bridge: State<'_, Arc<EngineBridgeState>>,
    file_path: String,
) -> Result<MusicSheetImportDto, String> {
    let trimmed = file_path.trim();
    if trimmed.is_empty() {
        return Err("Provide a Sheet Music PDF, image file, or folder of page images".to_string());
    }
    let raw_paths: Vec<PathBuf> = trimmed
        .split(['\n', ';'])
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .collect();
    let first = raw_paths
        .first()
        .ok_or_else(|| "Provide a valid file path".to_string())?;
    let stem = first
        .file_stem()
        .or_else(|| first.file_name())
        .and_then(|s| s.to_str())
        .unwrap_or("Imported Sheet Music")
        .to_string();
    let ext = first
        .extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_ascii_lowercase())
        .unwrap_or_default();

    let sheet = if raw_paths.len() == 1 && ext == "pdf" {
        let bytes = std::fs::read(first)
            .map_err(|e| format!("failed to read '{}': {e}", first.display()))?;
        integrations::import_sheet_music_pdf(
            &bytes,
            &stem,
            Some(trimmed),
            &managed_imported_media_dir(),
        )
        .map_err(|e| e.to_string())?
    } else {
        let mut collected: Vec<PathBuf> = Vec::new();
        for p in &raw_paths {
            if p.is_dir() {
                if let Ok(rd) = std::fs::read_dir(p) {
                    for entry in rd.flatten() {
                        let ep = entry.path();
                        if ep
                            .extension()
                            .and_then(|e| e.to_str())
                            .is_some_and(|e| {
                                matches!(
                                    e.to_ascii_lowercase().as_str(),
                                    "png" | "jpg" | "jpeg" | "bmp" | "webp"
                                )
                            })
                        {
                            collected.push(ep);
                        }
                    }
                }
            } else if p.is_file() {
                collected.push(p.clone());
            }
        }
        if collected.is_empty() {
            return Err("No sheet music image pages (.png/.jpg/.bmp) found at that path".to_string());
        }
        integrations::build_sheet_music_from_images(&collected, &stem)
    };

    let page_count = sheet.pages.len();
    let title = sheet.title.clone();
    let scene = attach_music_sheet_to_active(&bridge, None, Some(sheet)).await?;
    Ok(MusicSheetImportDto {
        kind: "SheetMusic".into(),
        title: title.clone(),
        count: page_count,
        summary: format!(
            "Attached Sheet Music \"{title}\" ({page_count} full-clef page(s)) to active song & Musician Stage"
        ),
        scene,
    })
}

#[tauri::command]
async fn update_sheet_music_page_map(
    bridge: State<'_, Arc<EngineBridgeState>>,
    group_to_page: std::collections::BTreeMap<String, u32>,
    slide_to_page: std::collections::BTreeMap<String, u32>,
) -> Result<SceneState, String> {
    let snap = refresh_snapshot_and_buses(&bridge, None).await?;
    let mut sheet = snap
        .state
        .active_sheet_music
        .ok_or_else(|| "No Sheet Music is attached to the active presentation".to_string())?;
    sheet.group_to_page = group_to_page;
    sheet.slide_to_page = slide_to_page;
    attach_music_sheet_to_active(&bridge, None, Some(sheet)).await
}

#[tauri::command]
fn get_ccli_display_config(
    bridge: State<'_, Arc<EngineBridgeState>>,
) -> Result<core_scene::CcliDisplayConfig, String> {
    let lib = bridge.library.lock().map_err(|e| e.to_string())?;
    lib.get_ccli_display_config().map_err(|e| e.to_string())
}

#[tauri::command]
async fn save_ccli_display_config(
    bridge: State<'_, Arc<EngineBridgeState>>,
    config: core_scene::CcliDisplayConfig,
) -> Result<SceneState, String> {
    {
        let lib = bridge.library.lock().map_err(|e| e.to_string())?;
        lib.save_ccli_display_config(&config)
            .map_err(|e| e.to_string())?;
    }
    apply_scene_diff(&bridge, SceneDiff::SetCcliDisplayConfig(config)).await
}

#[tauri::command]
async fn import_local_presentation_file(
    bridge: State<'_, Arc<EngineBridgeState>>,
    file_path: String,
    library_id: String,
    load_to_deck: bool,
) -> Result<PdfImportResultDto, String> {
    let trimmed = file_path.trim();
    if trimmed.is_empty() {
        return Err(
            "Provide a local file or folder path (.pdf, .pptx, .key, images, .show, .pro6, .xml, .cho, .usr)"
                .to_string(),
        );
    }

    // Support semicolon or newline separated list of image files as well as a single file or folder.
    let raw_paths: Vec<PathBuf> = trimmed
        .split(['\n', ';'])
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .collect();
    let first = raw_paths
        .first()
        .ok_or_else(|| "Provide a valid file path".to_string())?;
    let stem = first
        .file_stem()
        .or_else(|| first.file_name())
        .and_then(|s| s.to_str())
        .unwrap_or("Imported Presentation")
        .to_string();
    let ext = first
        .extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_ascii_lowercase())
        .unwrap_or_default();

    // 1. PDF Lyric Sheet / Presentation
    if raw_paths.len() == 1 && ext == "pdf" {
        let bytes = std::fs::read(first)
            .map_err(|e| format!("failed to read PDF '{}': {e}", first.display()))?;
        let parsed = integrations::import_lyric_sheet_pdf(&bytes, Some(&stem))
            .map_err(|e| e.to_string())?;
        return persist_parsed_pdf_song(&bridge, parsed, &library_id, load_to_deck).await;
    }

    // 2. Images (single image, multiple image paths, or a folder of slide images)
    let is_image_import = first.is_dir()
        || raw_paths.len() > 1
        || matches!(ext.as_str(), "png" | "jpg" | "jpeg" | "bmp" | "webp" | "gif");
    let imported = if is_image_import {
        integrations::import_images_as_presentation(&stem, &raw_paths, &managed_imported_media_dir())
            .map_err(|e| e.to_string())?
    } else if ext == "pptx" {
        // 3. PowerPoint (.pptx)
        let bytes = std::fs::read(first)
            .map_err(|e| format!("failed to read '{}': {e}", first.display()))?;
        integrations::import_pptx_bytes(&stem, &bytes).map_err(|e| e.to_string())?
    } else if ext == "key" {
        // 4. Apple Keynote (.key)
        let bytes = std::fs::read(first)
            .map_err(|e| format!("failed to read '{}': {e}", first.display()))?;
        integrations::import_keynote_bytes(&stem, &bytes, &managed_imported_media_dir())
            .map_err(|e| e.to_string())?
    } else {
        // 5. Text / XML / JSON worship formats
        let content = std::fs::read_to_string(first)
            .map_err(|e| format!("failed to read '{}': {e}", first.display()))?;
        match ext.as_str() {
            "show" | "json" => integrations::import_freeshow_json(&content).map_err(|e| e.to_string())?,
            "pro6" | "pro5" => integrations::import_pro6_xml(&content).map_err(|e| e.to_string())?,
            "cho" | "chopro" => {
                let s = integrations::parse_chordpro(&content)?;
                integrations::PresentationImportResult {
                    title: s.title,
                    ccli_number: s.ccli_number,
                    source_format: "ChordPro".into(),
                    slides: s.slides,
                }
            }
            "usr" => {
                let s = integrations::parse_songselect_usr(&content)?;
                integrations::PresentationImportResult {
                    title: s.title,
                    ccli_number: s.ccli_number,
                    source_format: "SongSelect (.usr)".into(),
                    slides: s.slides,
                }
            }
            "xml" => {
                if content.contains("<RVPresentationDocument") {
                    integrations::import_pro6_xml(&content).map_err(|e| e.to_string())?
                } else if content.contains("<lyrics>") && content.contains("<ccli>") {
                    integrations::import_opensong_xml(&content).map_err(|e| e.to_string())?
                } else {
                    integrations::import_openlyrics_xml(&content).map_err(|e| e.to_string())?
                }
            }
            _ => integrations::import_plain_text_song(&stem, &content).map_err(|e| e.to_string())?,
        }
    };

    let parsed = integrations::ParsedLyricSheet {
        title: imported.title,
        author: String::new(),
        copyright: String::new(),
        ccli_number: imported.ccli_number,
        key_signature: None,
        tempo_bpm: None,
        time_signature: None,
        slides: imported.slides,
        arrangements: Vec::new(),
        chord_chart: None,
        raw_text_preview: String::new(),
    };
    persist_parsed_pdf_song(&bridge, parsed, &library_id, load_to_deck).await
}

#[derive(serde::Serialize)]
struct BackupSummaryDto {
    bundle_path: String,
    item_count: usize,
}

#[tauri::command]
fn export_library_backup(
    bridge: State<'_, Arc<EngineBridgeState>>,
) -> Result<BackupSummaryDto, String> {
    let dir = std::env::temp_dir()
        .join("openpresenter-data")
        .join("backups");
    let ts = chrono::Utc::now().format("%Y%m%d-%H%M%S");
    let dest = dir.join(format!("openpresenter-backup-{ts}.opbundle"));
    let lib = bridge.library.lock().map_err(|e| e.to_string())?;
    let item_count = lib.export_backup_bundle(&dest).map_err(|e| e.to_string())?;
    Ok(BackupSummaryDto {
        bundle_path: dest.to_string_lossy().into_owned(),
        item_count,
    })
}

#[tauri::command]
fn import_library_backup(
    bridge: State<'_, Arc<EngineBridgeState>>,
    file_path: String,
) -> Result<BackupSummaryDto, String> {
    let trimmed = file_path.trim();
    if trimmed.is_empty() {
        return Err("Provide a .opbundle file path to restore".to_string());
    }
    let src = PathBuf::from(trimmed);
    let lib = bridge.library.lock().map_err(|e| e.to_string())?;
    let item_count = lib.import_backup_bundle(&src).map_err(|e| e.to_string())?;
    Ok(BackupSummaryDto {
        bundle_path: src.to_string_lossy().into_owned(),
        item_count,
    })
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

/// Gracefully stops the decoupled `openpresenter-engine` sidecar (closing all native GPU
/// Audience/Stage/Broadcast windows), releases SHM readers, closes all Tauri windows, and exits.
#[tauri::command]
async fn shutdown_all(
    app: AppHandle,
    bridge: State<'_, Arc<EngineBridgeState>>,
) -> Result<(), String> {
    info!("received Shutdown All request; stopping engine sidecar and closing all windows");
    let _ = tokio::time::timeout(
        Duration::from_secs(2),
        bridge.request(EngineCommand::Shutdown, false),
    )
    .await;
    {
        let mut guard = bridge.conn.lock().await;
        *guard = None;
    }
    {
        let mut buses = bridge.preview_buses.lock().await;
        buses.clear();
    }
    if let Ok(mut readers) = bridge.preview_readers.lock() {
        readers.clear();
    }
    for (_, window) in app.webview_windows() {
        let _ = window.close();
    }
    app.exit(0);
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
            delete_slide_element,
            set_output_stage_layout,
            set_pointer,
            clear_layer,
            clear_all,
            set_blackout,
            set_freeze,
            undo_scene,
            redo_scene,
            set_media_transport,
            set_audio_playback,
            select_arrangement,
            save_arrangements,
            list_bible_translations,
            lookup_scripture,
            present_scripture,
            parse_and_save_quick_lyrics,
            toggle_overlay,
            set_mask,
            set_stage_message,
            set_output_look,
            list_looks,
            upsert_look,
            delete_look,
            list_templates,
            upsert_template,
            delete_template,
            apply_template_to_slide,
            apply_template_to_deck,
            list_look_presets,
            save_look_preset,
            delete_look_preset,
            apply_look_preset,
            apply_look_routing,
            list_physical_displays,
            upsert_output_screen,
            set_output_calibration,
            remove_output_screen,
            split_monitor_into_screens,
            list_libraries,
            create_library,
            delete_library,
            search_library_songs,
            create_presentation,
            delete_presentation,
            load_presentation_to_deck,
            list_playlists,
            upsert_playlist,
            delete_playlist,
            list_media_bins,
            create_media_bin,
            delete_media_bin,
            list_media_assets,
            add_media_file,
            delete_media_asset,
            trigger_media_asset,
            move_presentation_to_library,
            move_media_to_bin,
            attach_media_to_slide,
            toggle_video_input,
            get_broadcast_capabilities,
            apply_broadcast_profile,
            create_countdown_timer,
            list_timers,
            control_timer,
            tick_timers,
            list_overlays,
            upsert_overlay,
            delete_overlay,
            toggle_overlay_item,
            list_message_templates,
            upsert_message_template,
            delete_message_template,
            trigger_message,
            clear_message,
            set_variable,
            set_slide_automation,
            apply_recorded_cues,
            delete_cue_track,
            list_macros,
            save_macro,
            delete_macro,
            run_macro,
            list_scheduled_events,
            save_scheduled_event,
            delete_scheduled_event,
            dispatch_control_command,
            import_worship_document,
            import_worship_file,
            import_local_presentation_file,
            import_lyric_sheet_pdf_file,
            import_chord_chart_file,
            import_sheet_music_file,
            update_sheet_music_page_map,
            import_media_file_safe,
            get_ccli_display_config,
            save_ccli_display_config,
            pco_list_plans,
            pco_list_plan_attachments,
            pco_import_attachment,
            export_library_backup,
            import_library_backup,
            export_ccli_csv_report,
            list_plugin_tiers,
            shutdown_engine,
            shutdown_all,
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
