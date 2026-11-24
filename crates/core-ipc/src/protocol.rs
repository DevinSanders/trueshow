//! Wire protocol definitions and cross-platform local socket naming.

use core_scene::{OutputId, SceneDiff, SceneState};
#[cfg(not(windows))]
use interprocess::local_socket::{GenericFilePath, ToFsName};
#[cfg(windows)]
use interprocess::local_socket::{GenericNamespaced, ToNsName};
use interprocess::local_socket::Name;
use serde::{Deserialize, Serialize};
use std::io;
use std::path::PathBuf;

/// Bumped whenever breaking changes are made to control-plane or SHM layouts.
pub const PROTOCOL_VERSION: u32 = 1;

/// Default local socket identifier shared by `openpresenter-engine` and `apps/desktop`.
pub fn default_socket_name() -> String {
    "openpresenter-engine-v1.sock".to_string()
}

/// Default directory for memory-mapped preview ring files.
pub fn default_preview_dir() -> PathBuf {
    std::env::temp_dir().join("openpresenter-preview-v1")
}

/// Converts a logical socket identifier into a platform-appropriate `interprocess` socket `Name`.
///
/// - On Windows: Named Pipe (`GenericNamespaced`, e.g., `\\.\pipe\openpresenter-engine-v1.sock`).
/// - On macOS / Linux: Unix Domain Socket path inside `std::env::temp_dir()`.
pub fn to_local_socket_name(socket_id: &str) -> io::Result<Name<'static>> {
    #[cfg(windows)]
    {
        socket_id.to_string().to_ns_name::<GenericNamespaced>()
    }
    #[cfg(not(windows))]
    {
        let path = if socket_id.contains(std::path::MAIN_SEPARATOR) {
            PathBuf::from(socket_id)
        } else {
            std::env::temp_dir().join(socket_id)
        };
        path.to_fs_name::<GenericFilePath>()
    }
}

/// Removes a stale Unix domain socket file if present (no-op on Windows Named Pipes).
pub fn cleanup_stale_socket(_socket_id: &str) {
    #[cfg(not(windows))]
    {
        let path = if _socket_id.contains(std::path::MAIN_SEPARATOR) {
            PathBuf::from(_socket_id)
        } else {
            std::env::temp_dir().join(_socket_id)
        };
        let _ = std::fs::remove_file(path);
    }
}

/// Physical monitor metadata reported by `winit`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DisplayInfo {
    pub index: usize,
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub scale_factor: f64,
    pub refresh_millihertz: Option<u32>,
    pub position_x: i32,
    pub position_y: i32,
}

/// Descriptor locating a memory-mapped preview ring buffer for a specific output.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PreviewBusDescriptor {
    pub output_id: OutputId,
    pub shm_path: String,
    pub width: u32,
    pub height: u32,
}

/// Per-output rendering performance counters.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OutputTelemetry {
    pub output_id: OutputId,
    pub width: u32,
    pub height: u32,
    pub frames_presented: u64,
}

/// Live engine telemetry snapshot.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EngineTelemetry {
    pub fps: f32,
    pub avg_frame_ms: f32,
    pub p99_frame_ms: f32,
    pub total_frames: u64,
    pub dropped_frames: u64,
    pub outputs: Vec<OutputTelemetry>,
}

impl Default for EngineTelemetry {
    fn default() -> Self {
        Self {
            fps: 0.0,
            avg_frame_ms: 0.0,
            p99_frame_ms: 0.0,
            total_frames: 0,
            dropped_frames: 0,
            outputs: Vec::new(),
        }
    }
}

/// Full engine state delivered immediately upon UI connection or reconnection.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EngineSnapshot {
    pub protocol_version: u32,
    pub engine_pid: u32,
    pub gpu_adapter_name: String,
    pub gpu_backend: String,
    pub state: SceneState,
    pub displays: Vec<DisplayInfo>,
    pub preview_buses: Vec<PreviewBusDescriptor>,
    pub telemetry: EngineTelemetry,
}

/// Commands sent from the operator control surface to `openpresenter-engine`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum EngineCommand {
    GetSnapshot,
    ApplyDiff(Box<SceneDiff>),
    ListDisplays,
    GetTelemetry,
    SetLogLevel(String),
    Ping(u64),
    Shutdown,
}

/// Responses and unsolicited events sent from `openpresenter-engine` to the control surface.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum EngineEvent {
    Snapshot(Box<EngineSnapshot>),
    StateUpdated(Box<SceneState>),
    Displays(Vec<DisplayInfo>),
    Telemetry(EngineTelemetry),
    LogLevelUpdated(String),
    Pong(u64),
    ShuttingDown,
    Error(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn socket_names_and_defaults_are_valid() {
        let sock = default_socket_name();
        assert!(sock.contains("openpresenter"));
        assert!(to_local_socket_name(&sock).is_ok());
        cleanup_stale_socket(&sock);

        let dir = default_preview_dir();
        assert!(dir.to_string_lossy().contains("openpresenter-preview"));

        let tel = EngineTelemetry::default();
        assert_eq!(tel.total_frames, 0);
        assert_eq!(tel.dropped_frames, 0);
        assert!(tel.outputs.is_empty());
    }
}
