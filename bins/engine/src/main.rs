//! `openpresenter-engine` — decoupled sidecar rendering & compositing process.
//!
//! Owns the OS main thread (`winit` + `wgpu`) so physical sanctuary displays remain live at
//! 60 fps even if the Tauri 2 operator UI crashes, reloads, or restarts.

use clap::Parser;
use core_diagnostics::{
    DiagnosticsPaths, LogLevel, LoggingController, init_logging, install_crash_hook,
};
use core_ipc::{
    EngineCommand, EngineEvent, default_preview_dir, default_socket_name,
    protocol::cleanup_stale_socket, read_frame, to_local_socket_name, write_frame,
};
use core_scene::SceneState;
use interprocess::local_socket::{
    ListenerOptions,
    tokio::{Stream, prelude::*},
};
use render_wgpu::{
    EngineWakeEvent, EventLoopProxy, SharedEngineState, create_event_loop, run_compositor,
};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::io::AsyncWriteExt;
use tracing::{error, info, warn};

#[derive(Debug, Parser)]
#[command(
    name = "openpresenter-engine",
    about = "OpenPresenter decoupled real-time GPU compositing sidecar engine"
)]
struct Cli {
    /// Local socket name (Named Pipe on Windows, Unix socket filename on macOS/Linux).
    #[arg(long, default_value_t = default_socket_name())]
    socket: String,

    /// Directory where triple-buffered shared-memory preview rings are created.
    #[arg(long, default_value_os_t = default_preview_dir())]
    preview_dir: PathBuf,

    /// Number of physical output windows to open for M0 (1 = Audience, 2 = Audience + Stage).
    #[arg(long, default_value_t = 2)]
    outputs: usize,

    /// Run IPC control server without opening physical OS windows (for headless CI tests).
    #[arg(long, default_value_t = false)]
    headless: bool,

    /// Initial logging verbosity (`error`, `warn`, `info`, `debug`, `trace`).
    #[arg(long, default_value = "info")]
    log_level: String,
}

fn main() {
    let cli = Cli::parse();
    let diag_paths = DiagnosticsPaths::default_system();
    let initial_level = LogLevel::parse(&cli.log_level).unwrap_or(LogLevel::Info);
    install_crash_hook(
        "openpresenter-engine",
        env!("CARGO_PKG_VERSION"),
        diag_paths.clone(),
    );
    let logging = init_logging("engine", &diag_paths, initial_level).ok();
    info!(
        log_dir = %diag_paths.log_dir.display(),
        crash_dir = %diag_paths.crash_dir.display(),
        level = initial_level.as_str(),
        "initialized persistent diagnostics logging and crash reporter"
    );

    let ffmpeg_status = media_engine::probe_dynamic_lgpl_ffmpeg(&[]);
    let ndi_status = broadcast_io::probe_ndi_runtime(&[]);
    let decklink_status = broadcast_io::probe_decklink_runtime(&[]);
    info!("media-engine LGPL FFmpeg status: {ffmpeg_status:?}");
    info!("broadcast-io NDI 6 dynamic runtime status: {ndi_status:?}");
    info!("broadcast-io DeckLink dynamic driver status: {decklink_status:?}");
    let shared = Arc::new(SharedEngineState::new(SceneState::demo_m0(
        cli.outputs.clamp(1, 2),
    )));

    if cli.headless {
        shared.mark_headless_ready();
        info!(
            "starting openpresenter-engine in headless IPC mode on socket '{}'",
            cli.socket
        );
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("create tokio runtime");
        rt.block_on(run_ipc_server(cli.socket, shared, None, logging));
        return;
    }

    // Best-effort cleanup of stale SHM files from exited engine instances (locked files are skipped).
    if let Ok(entries) = std::fs::read_dir(&cli.preview_dir) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.extension().and_then(|e| e.to_str()) == Some("op_shm") {
                let _ = std::fs::remove_file(&p);
            }
        }
    }

    let (event_loop, proxy) = create_event_loop().expect("failed to initialize winit event loop");

    let ipc_socket = cli.socket.clone();
    let ipc_shared = Arc::clone(&shared);
    let ipc_proxy = proxy.clone();
    let ipc_logging = logging.clone();

    std::thread::Builder::new()
        .name("ipc-control-server".to_string())
        .spawn(move || {
            let rt = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .expect("create tokio runtime");
            rt.block_on(run_ipc_server(
                ipc_socket,
                ipc_shared,
                Some(ipc_proxy),
                ipc_logging,
            ));
        })
        .expect("spawn IPC server thread");

    info!(
        "starting openpresenter-engine GPU compositor (pid={}, outputs={}, socket='{}')",
        std::process::id(),
        cli.outputs.clamp(1, 2),
        cli.socket
    );

    if let Err(err) = run_compositor(event_loop, shared, cli.preview_dir) {
        error!("compositor exited with error: {err}");
        let _ = core_diagnostics::record_runtime_error(
            &diag_paths,
            "openpresenter-engine",
            "compositor-fatal-error",
            env!("CARGO_PKG_VERSION"),
            &err,
            Some("bins/engine/src/main.rs"),
            None,
        );
        std::process::exit(1);
    }
}

async fn run_ipc_server(
    socket_id: String,
    shared: Arc<SharedEngineState>,
    proxy: Option<EventLoopProxy<EngineWakeEvent>>,
    logging: Option<LoggingController>,
) {
    cleanup_stale_socket(&socket_id);
    let name = match to_local_socket_name(&socket_id) {
        Ok(n) => n,
        Err(err) => {
            error!("invalid local socket name '{socket_id}': {err}");
            return;
        }
    };

    let listener = match ListenerOptions::new().name(name).create_tokio() {
        Ok(l) => l,
        Err(err) => {
            error!("failed to bind local IPC listener '{socket_id}': {err}");
            return;
        }
    };

    info!("IPC control server listening on '{socket_id}' (survives UI disconnects)");

    let remote_shared = Arc::clone(&shared);
    let remote_proxy = proxy.clone();
    let remote_handler: show_control::web_stage::RemoteCommandHandler = Arc::new(move |cmd| {
        let diff = match cmd {
            show_control::web_stage::RemoteCommand::NextSlide => {
                let snap = remote_shared.snapshot().state;
                let cur = snap
                    .active_slide
                    .as_ref()
                    .and_then(|a| snap.deck.iter().position(|s| s.id == a.id))
                    .unwrap_or(0);
                let next_idx = (cur + 1).min(snap.deck.len().saturating_sub(1));
                snap.deck.get(next_idx).map(|s| core_scene::SceneDiff::TriggerSlide {
                    slide_id: s.id,
                    transition_override: None,
                })
            }
            show_control::web_stage::RemoteCommand::PrevSlide => {
                let snap = remote_shared.snapshot().state;
                let cur = snap
                    .active_slide
                    .as_ref()
                    .and_then(|a| snap.deck.iter().position(|s| s.id == a.id))
                    .unwrap_or(0);
                snap.deck
                    .get(cur.saturating_sub(1))
                    .map(|s| core_scene::SceneDiff::TriggerSlide {
                        slide_id: s.id,
                        transition_override: None,
                    })
            }
            show_control::web_stage::RemoteCommand::TriggerSlide { slide_id } => {
                uuid::Uuid::parse_str(&slide_id)
                    .ok()
                    .map(|u| core_scene::SceneDiff::TriggerSlide {
                        slide_id: core_scene::SlideId(u),
                        transition_override: None,
                    })
            }
            show_control::web_stage::RemoteCommand::SetBlackout { enabled } => {
                Some(core_scene::SceneDiff::SetBlackout(enabled))
            }
            show_control::web_stage::RemoteCommand::SetFreeze { enabled } => {
                Some(core_scene::SceneDiff::SetFreeze(enabled))
            }
            show_control::web_stage::RemoteCommand::ClearAll => {
                Some(core_scene::SceneDiff::ClearAll)
            }
            show_control::web_stage::RemoteCommand::SetStageMessage { message } => {
                Some(core_scene::SceneDiff::SetStageMessage(message))
            }
            show_control::web_stage::RemoteCommand::SetPointer { x, y, active } => {
                Some(core_scene::SceneDiff::SetPointer(if active {
                    Some(core_scene::TelestratorPointer {
                        x,
                        y,
                        active: true,
                        color: core_scene::ColorRgba::rgba(1.0, 0.22, 0.22, 0.92),
                        radius: 16.0,
                    })
                } else {
                    None
                }))
            }
        };
        if let Some(d) = diff
            && remote_shared.apply_diff(d).is_some()
            && let Some(p) = remote_proxy.as_ref()
        {
            let _ = p.send_event(EngineWakeEvent::SceneChanged);
        }
    });

    let stage_payload = show_control::payload_from_scene(&shared.snapshot().state);
    let stage_bind = match show_control::WebStageServer::bind_with_remote(
        "0.0.0.0:5505",
        stage_payload.clone(),
        "2026".to_string(),
        Some(Arc::clone(&remote_handler)),
    )
    .await
    {
        Ok(srv) => Ok(srv),
        Err(_) => {
            show_control::WebStageServer::bind_with_remote(
                "0.0.0.0:0",
                stage_payload,
                "2026".to_string(),
                Some(remote_handler),
            )
            .await
        }
    };
    let stage_server = match stage_bind {
        Ok(srv) => {
            let port_dir = std::env::temp_dir().join("openpresenter-data");
            let _ = std::fs::create_dir_all(&port_dir);
            let _ = std::fs::write(
                port_dir.join("web_stage_port.txt"),
                srv.local_addr().port().to_string(),
            );
            info!(
                stage_url = %srv.url(),
                remote_url = %srv.remote_url(),
                pin = %srv.remote_pin(),
                "LAN Web Stage Display & Mobile Remote Controller listening"
            );
            Some(Arc::new(srv))
        }
        Err(err) => {
            warn!("could not start LAN Web Stage server: {err}");
            None
        }
    };

    loop {
        match listener.accept().await {
            Ok(stream) => {
                info!("operator control client attached");
                let shared_c = Arc::clone(&shared);
                let proxy_c = proxy.clone();
                let stage_c = stage_server.clone();
                let logging_c = logging.clone();
                tokio::spawn(async move {
                    if let Err(err) =
                        handle_client(stream, shared_c, proxy_c, stage_c, logging_c).await
                    {
                        warn!("operator control client detached ({err}); live outputs remain active");
                    } else {
                        info!("operator control client detached cleanly; live outputs remain active");
                    }
                });
            }
            Err(err) => {
                warn!("IPC accept error: {err}");
            }
        }
    }
}

async fn handle_client(
    mut stream: Stream,
    shared: Arc<SharedEngineState>,
    proxy: Option<EventLoopProxy<EngineWakeEvent>>,
    stage_server: Option<Arc<show_control::WebStageServer>>,
    logging: Option<LoggingController>,
) -> std::io::Result<()> {
    loop {
        let cmd: EngineCommand = match read_frame(&mut stream).await {
            Ok(c) => c,
            Err(err) if err.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(()),
            Err(err) => return Err(err),
        };

        let response = match cmd {
            EngineCommand::GetSnapshot => {
                // Wait briefly for GPU surfaces and SHM preview rings to finish initializing
                // so newly attached control surfaces never observe empty preview_buses.
                for _ in 0..200 {
                    if shared.is_hardware_ready() {
                        break;
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(25)).await;
                }
                EngineEvent::Snapshot(Box::new(shared.snapshot()))
            }
            EngineCommand::ApplyDiff(diff) => match shared.apply_diff(*diff) {
                Some(new_state) => {
                    if let Some(srv) = stage_server.as_ref() {
                        srv.publish(show_control::payload_from_scene(&new_state));
                    }
                    if let Some(p) = proxy.as_ref() {
                        let _ = p.send_event(EngineWakeEvent::SceneChanged);
                    }
                    EngineEvent::StateUpdated(Box::new(new_state))
                }
                None => EngineEvent::Error("scene diff did not match an existing target".into()),
            },
            EngineCommand::ListDisplays => EngineEvent::Displays(shared.displays()),
            EngineCommand::GetTelemetry => EngineEvent::Telemetry(shared.telemetry()),
            EngineCommand::SetLogLevel(level_str) => match LogLevel::parse(&level_str) {
                Ok(lvl) => {
                    if let Some(ctrl) = logging.as_ref() {
                        match ctrl.set_level(lvl) {
                            Ok(()) => EngineEvent::LogLevelUpdated(lvl.as_str().to_string()),
                            Err(err) => EngineEvent::Error(err.to_string()),
                        }
                    } else {
                        EngineEvent::LogLevelUpdated(lvl.as_str().to_string())
                    }
                }
                Err(err) => EngineEvent::Error(err.to_string()),
            },
            EngineCommand::Ping(seq) => EngineEvent::Pong(seq),
            EngineCommand::Shutdown => {
                let _ = write_frame(&mut stream, &EngineEvent::ShuttingDown).await;
                let _ = stream.shutdown().await;
                if let Some(p) = proxy.as_ref() {
                    let _ = p.send_event(EngineWakeEvent::Shutdown);
                } else {
                    std::process::exit(0);
                }
                return Ok(());
            }
        };

        write_frame(&mut stream, &response).await?;
    }
}
