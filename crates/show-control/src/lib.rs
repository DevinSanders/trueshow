//! Show control and Stage Display networking for OpenPresenter.
//!
//! M2 delivers the optional low-latency LAN WebSocket Browser Stage Display server (`web_stage`),
//! while keeping primary Stage Displays rendered natively via `wgpu`. M3 adds concurrent Timers,
//! Macros, MIDI, OSC, SMPTE LTC/MTC, and Bitfocus Companion APIs.

pub mod web_stage;

pub use web_stage::{StageViewPayload, WebStageServer, build_stage_router, payload_from_scene};
