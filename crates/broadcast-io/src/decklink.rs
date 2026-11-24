//! Blackmagic DeckLink SDI Key & Fill output scheduler and dynamic driver probe.
//!
//! Strictly avoids bundling Blackmagic Desktop Video drivers or proprietary SDK headers
//! in default open-source builds; probes system-installed Desktop Video libraries dynamically.

use crate::alpha::{BroadcastFrame, split_key_fill_bgra};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum DeckLinkError {
    #[error("invalid DeckLink key/fill sub-device pairing: fill ({fill}) and key ({key}) must be distinct")]
    InvalidSubDevicePair { fill: u32, key: u32 },
    #[error("frame buffer dimensions mismatch ({width}x{height})")]
    InvalidFrameDimensions { width: u32, height: u32 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeckLinkStandard {
    Hd1080p5994,
    Hd1080p60,
    Uhd2160p5994,
    Uhd2160p60,
}

impl DeckLinkStandard {
    pub const fn dimensions(self) -> (u32, u32) {
        match self {
            Self::Hd1080p5994 | Self::Hd1080p60 => (1920, 1080),
            Self::Uhd2160p5994 | Self::Uhd2160p60 => (3840, 2160),
        }
    }

    pub const fn timescale_and_duration(self) -> (u64, u64) {
        match self {
            Self::Hd1080p5994 | Self::Uhd2160p5994 => (60_000, 1001),
            Self::Hd1080p60 | Self::Uhd2160p60 => (60_000, 1000),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeckLinkKeyMode {
    /// Paired SDI outputs (e.g., DeckLink Duo 2 SDI 1 = Fill, SDI 2 = Key) for ATEM/Ross external keyers.
    ExternalDualSdiKeyFill { fill_subdevice: u32, key_subdevice: u32 },
    /// Single-device internal alpha keying (`IDeckLinkKeyer`).
    InternalKeyer { subdevice: u32, level: u8 },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeckLinkStatus {
    DriverDetected { library_path: String },
    DriverNotInstalled { install_hint: String },
}

/// Dynamically probes for user-installed Blackmagic Desktop Video runtime without bundling drivers.
pub fn probe_decklink_runtime(extra_paths: &[String]) -> DeckLinkStatus {
    let mut candidates = extra_paths.to_vec();
    #[cfg(target_os = "windows")]
    candidates.push("DeckLinkAPI64.dll".to_string());
    #[cfg(target_os = "macos")]
    candidates.push("/Library/Frameworks/DeckLinkAPI.framework/DeckLinkAPI".to_string());
    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    candidates.push("libDeckLinkAPI.so".to_string());

    for path in candidates {
        #[allow(unsafe_code)]
        if unsafe { libloading::Library::new(&path) }.is_ok() {
            return DeckLinkStatus::DriverDetected { library_path: path };
        }
    }

    DeckLinkStatus::DriverNotInstalled {
        install_hint: "Install Blackmagic Desktop Video drivers from blackmagicdesign.com to enable SDI Key & Fill."
            .to_string(),
    }
}

/// Synchronized Fill + Key SDI frame buffers scheduled onto paired hardware sub-devices.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeckLinkKeyFillPair {
    pub frame_number: u64,
    pub display_time: u64,
    pub frame_duration: u64,
    pub timescale: u64,
    pub fill_bgra: Vec<u8>,
    pub key_luma_bgra: Vec<u8>,
}

/// Frame-accurate SDI Key & Fill scheduler.
#[derive(Debug)]
pub struct DeckLinkScheduler {
    standard: DeckLinkStandard,
    mode: DeckLinkKeyMode,
    scheduled_frames: u64,
}

impl DeckLinkScheduler {
    pub fn new(standard: DeckLinkStandard, mode: DeckLinkKeyMode) -> Result<Self, DeckLinkError> {
        if let DeckLinkKeyMode::ExternalDualSdiKeyFill {
            fill_subdevice,
            key_subdevice,
        } = mode
            && fill_subdevice == key_subdevice
        {
            return Err(DeckLinkError::InvalidSubDevicePair {
                fill: fill_subdevice,
                key: key_subdevice,
            });
        }
        Ok(Self {
            standard,
            mode,
            scheduled_frames: 0,
        })
    }

    pub fn standard(&self) -> DeckLinkStandard {
        self.standard
    }

    pub fn mode(&self) -> DeckLinkKeyMode {
        self.mode
    }

    pub fn scheduled_frames(&self) -> u64 {
        self.scheduled_frames
    }

    /// Splits a composited RGBA8 frame into a genlocked Fill + Key pair with exact broadcast timing.
    pub fn schedule_frame(&mut self, frame: &BroadcastFrame) -> Result<DeckLinkKeyFillPair, DeckLinkError> {
        let expected = (frame.width as usize) * (frame.height as usize) * 4;
        if expected == 0 || frame.rgba_premul.len() != expected {
            return Err(DeckLinkError::InvalidFrameDimensions {
                width: frame.width,
                height: frame.height,
            });
        }

        let mut fill_bgra = Vec::new();
        let mut key_luma_bgra = Vec::new();
        split_key_fill_bgra(&frame.rgba_premul, &mut fill_bgra, &mut key_luma_bgra);

        let (timescale, frame_duration) = self.standard.timescale_and_duration();
        let frame_number = self.scheduled_frames;
        self.scheduled_frames += 1;

        Ok(DeckLinkKeyFillPair {
            frame_number,
            display_time: frame_number * frame_duration,
            frame_duration,
            timescale,
            fill_bgra,
            key_luma_bgra,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_key_fill_pairing_and_schedules_5994_timing() {
        assert!(matches!(
            DeckLinkScheduler::new(
                DeckLinkStandard::Hd1080p5994,
                DeckLinkKeyMode::ExternalDualSdiKeyFill {
                    fill_subdevice: 1,
                    key_subdevice: 1,
                }
            ),
            Err(DeckLinkError::InvalidSubDevicePair { .. })
        ));

        let mut sched = DeckLinkScheduler::new(
            DeckLinkStandard::Hd1080p5994,
            DeckLinkKeyMode::ExternalDualSdiKeyFill {
                fill_subdevice: 1,
                key_subdevice: 2,
            },
        )
        .expect("valid scheduler");

        assert_eq!(sched.standard().dimensions(), (1920, 1080));
        assert_eq!(DeckLinkStandard::Uhd2160p60.dimensions(), (3840, 2160));
        assert_eq!(DeckLinkStandard::Uhd2160p60.timescale_and_duration(), (60_000, 1000));
        assert!(matches!(
            sched.mode(),
            DeckLinkKeyMode::ExternalDualSdiKeyFill { .. }
        ));

        let f0 = BroadcastFrame {
            width: 2,
            height: 1,
            timestamp_us: 0,
            sequence: 1,
            rgba_premul: vec![0, 0, 0, 0, 200, 100, 50, 255],
        };
        let p0 = sched.schedule_frame(&f0).expect("schedule 0");
        assert_eq!(p0.display_time, 0);
        assert_eq!(p0.frame_duration, 1001);
        assert_eq!(&p0.key_luma_bgra[0..4], &[0, 0, 0, 255]);
        assert_eq!(&p0.key_luma_bgra[4..8], &[255, 255, 255, 255]);

        let p1 = sched.schedule_frame(&f0).expect("schedule 1");
        assert_eq!(p1.display_time, 1001);
        assert_eq!(sched.scheduled_frames(), 2);

        // Failure path: bad buffer dimensions
        let bad = BroadcastFrame {
            width: 4,
            height: 4,
            timestamp_us: 0,
            sequence: 3,
            rgba_premul: vec![0; 4],
        };
        assert!(matches!(
            sched.schedule_frame(&bad),
            Err(DeckLinkError::InvalidFrameDimensions { .. })
        ));

        let _status = probe_decklink_runtime(&["__nonexistent_decklink__.dll".to_string()]);
    }
}
