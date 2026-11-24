//! `openpresenter-engine` — decoupled sidecar rendering & compositing process.
//!
//! Owns the OS main thread (`winit` + `wgpu`) so physical sanctuary displays remain live at
//! 60 fps even if the Tauri 2 operator UI crashes, reloads, or restarts.

use clap::Parser;
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
}

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,wgpu=warn".into()),
        )
        .init();

    let cli = Cli::parse();
    let shared = Arc::new(SharedEngineState::new(SceneState::demo_m0(
        cli.outputs.clamp(1, 2),
    )));

    if cli.headless {
        info!(
            "starting openpresenter-engine in headless IPC mode on socket '{}'",
            cli.socket
        );
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("create tokio runtime");
        rt.block_on(run_ipc_server(cli.socket, shared, None));
        return;
    }

    let (event_loop, proxy) = create_event_loop().expect("failed to initialize winit event loop");

    let ipc_socket = cli.socket.clone();
    let ipc_shared = Arc::clone(&shared);
    let ipc_proxy = proxy.clone();

    std::thread::Builder::new()
        .name("ipc-control-server".to_string())
        .spawn(move || {
            let rt = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .expect("create tokio runtime");
            rt.block_on(run_ipc_server(ipc_socket, ipc_shared, Some(ipc_proxy)));
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
        std::process::exit(1);
    }
}

async fn run_ipc_server(
    socket_id: String,
    shared: Arc<SharedEngineState>,
    proxy: Option<EventLoopProxy<EngineWakeEvent>>,
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

    loop {
        match listener.accept().await {
            Ok(stream) => {
                info!("operator control client attached");
                let shared_c = Arc::clone(&shared);
                let proxy_c = proxy.clone();
                tokio::spawn(async move {
                    if let Err(err) = handle_client(stream, shared_c, proxy_c).await {
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
) -> std::io::Result<()> {
    loop {
        let cmd: EngineCommand = match read_frame(&mut stream).await {
            Ok(c) => c,
            Err(err) if err.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(()),
            Err(err) => return Err(err),
        };

        let response = match cmd {
            EngineCommand::GetSnapshot => EngineEvent::Snapshot(Box::new(shared.snapshot())),
            EngineCommand::ApplyDiff(diff) => match shared.apply_diff(diff) {
                Some(new_state) => {
                    if let Some(p) = proxy.as_ref() {
                        let _ = p.send_event(EngineWakeEvent::SceneChanged);
                    }
                    EngineEvent::StateUpdated(Box::new(new_state))
                }
                None => EngineEvent::Error("scene diff did not match an existing target".into()),
            },
            EngineCommand::ListDisplays => EngineEvent::Displays(shared.displays()),
            EngineCommand::GetTelemetry => EngineEvent::Telemetry(shared.telemetry()),
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
