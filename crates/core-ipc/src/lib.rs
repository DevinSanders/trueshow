//! Dual-plane local IPC between `trueshow-engine` and the Tauri 2 operator UI.
//!
//! - **Control Plane:** Framed `postcard` binary messages over cross-platform local sockets
//!   (Windows Named Pipes / Unix Domain Sockets via `interprocess`).
//! - **Preview Data Plane:** Lock-free triple-buffered shared-memory ring (`memmap2`) carrying
//!   downscaled RGBA8 preview frames without stalling the 60 fps render thread.

pub mod framing;
pub mod protocol;
pub mod shm;

pub use framing::{MAX_FRAME_BYTES, read_frame, write_frame};
pub use protocol::{
    DisplayInfo, EngineCommand, EngineEvent, EngineSnapshot, EngineTelemetry, OutputTelemetry,
    PROTOCOL_VERSION, PreviewBusDescriptor, default_preview_dir, default_socket_name,
    to_local_socket_name,
};
pub use shm::{PreviewFrameMeta, ShmError, ShmPreviewReader, ShmPreviewWriter};
