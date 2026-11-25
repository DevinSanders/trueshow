//! Complete Show Control & Stage Display engine for TrueShow.
//!
//! - [`timers`]: Concurrent Countdown, Count-Up, and Elapsed timers with overrun formatting.
//! - [`macros`]: Reusable multi-action automations with cycle guards.
//! - [`midi`]: Bidirectional MIDI (`midir`) + MultiTracks Playback / Ableton Live cue routing.
//! - [`osc`]: Bidirectional OSC (`rosc`) command routing.
//! - [`timecode`]: Pure-Rust SMPTE LTC 80-bit frame codec + MIDI Timecode (MTC) assembler.
//! - [`companion`]: Bitfocus Companion Satellite protocol & REST show-control API.
//! - [`web_stage`]: Embedded LAN WebSocket Browser Stage Display server.

pub mod companion;
pub mod macros;
pub mod midi;
pub mod osc;
pub mod timecode;
pub mod timers;
pub mod web_stage;

pub use companion::{CompanionCommand, CompanionFeedback, parse_satellite_line};
pub use macros::{MacroAction, MacroDefinition, MacroError, MacroExecutionPlan, MacroRegistry};
pub use midi::{MidiCueBinding, MidiCueRouter, MidiMessage, MidiTrigger, list_midi_ports};
pub use osc::{OscControlEvent, OscError, decode_osc_packet, encode_osc_event};
pub use timecode::{FrameRate, MtcAssembler, SmpteTimecode, decode_ltc_bits, encode_ltc_bits};
pub use timers::{TimerEngine, TimerId, TimerKind, TimerSnapshot};
pub use web_stage::{
    StageHub, StageViewPayload, WebStageServer, build_stage_router, payload_from_scene,
};
