//! Broadcast video I/O subsystem for TrueShow (M2).
//!
//! - **Alpha / Key & Fill (`alpha`):** Premultiplied RGBA -> straight BGRA conversion and
//!   dual-stream Fill + Luma-Key extraction for SDI external keyers.
//! - **NDI 6 (`ndi`):** Zero-bundled dynamic runtime loader (`NDI_RUNTIME_DIR_V6`).
//! - **Open Media Transport (`omt`):** MIT-licensed LAN IP video + alpha transport with mDNS metadata.
//! - **Blackmagic DeckLink (`decklink`):** Synchronized SDI Key & Fill scheduler + dynamic driver probe.
//! - **Virtual Outputs (`virtual_out`):** Syphon (macOS), Spout2 (Windows), and shared-memory loopback.

pub mod alpha;
pub mod decklink;
pub mod ndi;
pub mod omt;
pub mod virtual_out;

pub use alpha::{
    AlphaMode, BroadcastFrame, BroadcastQueue, premul_rgba_to_straight_bgra, split_key_fill_bgra,
};
pub use decklink::{
    DeckLinkError, DeckLinkKeyFillPair, DeckLinkKeyMode, DeckLinkScheduler, DeckLinkStandard,
    DeckLinkStatus, probe_decklink_runtime,
};
pub use ndi::{NdiConfig, NdiRuntimeStatus, NdiSenderState, NdiSourceInfo, probe_ndi_runtime};
pub use omt::{OmtError, OmtFrameHeader, OmtReceiver, OmtSender, OmtServiceDescriptor};
pub use virtual_out::{VirtualOutputBackend, VirtualOutputError, VirtualOutputSink};
