//! Dynamic NDI 6.x runtime loader and sender/receiver management (zero bundled proprietary DLLs).
//!
//! Strictly adheres to TrueShow's proprietary SDK hygiene mandate: loads only the
//! user-installed NDI Runtime via `NDI_RUNTIME_DIR_V6` / standard system paths at runtime.

use crate::alpha::{AlphaMode, BroadcastFrame, premul_rgba_to_straight_bgra};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NdiConfig {
    pub source_name: String,
    pub groups: Option<String>,
    pub clock_video: bool,
    pub alpha_mode: AlphaMode,
}

impl Default for NdiConfig {
    fn default() -> Self {
        Self {
            source_name: "TrueShow — Main Output".to_string(),
            groups: None,
            clock_video: true,
            alpha_mode: AlphaMode::StraightBgra,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NdiSourceInfo {
    pub name: String,
    pub url_address: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum NdiRuntimeStatus {
    Available {
        library_path: String,
    },
    NotInstalled {
        install_hint: String,
        searched_paths: Vec<String>,
    },
}

/// Candidate NDI 6 dynamic library locations (respects `NDI_RUNTIME_DIR_V6` / `NDI_RUNTIME_DIR_V5`).
pub fn ndi_candidate_paths() -> Vec<String> {
    let mut paths = Vec::new();
    let lib_name = if cfg!(target_os = "windows") {
        "Processing.NDI.Lib.x64.dll"
    } else if cfg!(target_os = "macos") {
        "libndi.dylib"
    } else {
        "libndi.so.6"
    };

    for env_var in ["NDI_RUNTIME_DIR_V6", "NDI_RUNTIME_DIR_V5"] {
        if let Ok(dir) = std::env::var(env_var) {
            paths.push(
                PathBuf::from(dir)
                    .join(lib_name)
                    .to_string_lossy()
                    .into_owned(),
            );
        }
    }

    #[cfg(target_os = "windows")]
    {
        paths.push(format!(r"C:\Program Files\NDI\NDI 6 Runtime\v6\{lib_name}"));
        paths.push(lib_name.to_string());
    }
    #[cfg(target_os = "macos")]
    {
        paths.push("/usr/local/lib/libndi.dylib".to_string());
        paths.push("/Library/NDI SDK for Apple/lib/macOS/libndi.dylib".to_string());
        paths.push(lib_name.to_string());
    }
    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    {
        paths.push("/usr/lib/libndi.so.6".to_string());
        paths.push("/usr/local/lib/libndi.so.6".to_string());
        paths.push(lib_name.to_string());
    }
    paths
}

/// Dynamically probes for a user-installed NDI 6 runtime without bundling proprietary libraries.
pub fn probe_ndi_runtime(extra_paths: &[String]) -> NdiRuntimeStatus {
    let mut searched = extra_paths.to_vec();
    searched.extend(ndi_candidate_paths());

    for candidate in &searched {
        #[allow(unsafe_code)]
        // SAFETY: Dynamic probe of user-installed NDI runtime library; verifies presence only.
        if unsafe { libloading::Library::new(candidate) }.is_ok() {
            return NdiRuntimeStatus::Available {
                library_path: candidate.clone(),
            };
        }
    }

    NdiRuntimeStatus::NotInstalled {
        install_hint: "Install the free NDI 6 Runtime from ndi.video to enable NDI send/receive."
            .to_string(),
        searched_paths: searched,
    }
}

/// Stateful NDI output sender preparing straight/premultiplied BGRA frames for transmission.
#[derive(Debug)]
pub struct NdiSenderState {
    config: NdiConfig,
    frames_sent: u64,
    bgra_scratch: Vec<u8>,
}

impl NdiSenderState {
    pub fn new(config: NdiConfig) -> Result<Self, String> {
        if config.source_name.trim().is_empty() {
            return Err("NDI source_name cannot be empty".to_string());
        }
        Ok(Self {
            config,
            frames_sent: 0,
            bgra_scratch: Vec::new(),
        })
    }

    pub fn config(&self) -> &NdiConfig {
        &self.config
    }

    pub fn frames_sent(&self) -> u64 {
        self.frames_sent
    }

    /// Formats a composited GPU RGBA8 frame into an NDI-ready BGRA8 payload.
    pub fn prepare_frame(&mut self, frame: &BroadcastFrame) -> Result<&[u8], String> {
        let expected = (frame.width as usize) * (frame.height as usize) * 4;
        if frame.rgba_premul.len() != expected || expected == 0 {
            return Err("NDI frame dimensions do not match RGBA buffer length".to_string());
        }
        if !premul_rgba_to_straight_bgra(&frame.rgba_premul, &mut self.bgra_scratch) {
            return Err("failed to convert RGBA to straight BGRA".to_string());
        }
        self.frames_sent += 1;
        Ok(&self.bgra_scratch)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn probes_ndi_runtime_gracefully_and_validates_sender_frames() {
        let status = probe_ndi_runtime(&["__nonexistent_ndi_runtime__.dll".to_string()]);
        match status {
            NdiRuntimeStatus::Available { library_path } => assert!(!library_path.is_empty()),
            NdiRuntimeStatus::NotInstalled {
                install_hint,
                searched_paths,
            } => {
                assert!(install_hint.contains("NDI"));
                assert!(!searched_paths.is_empty());
            }
        }

        // Failure path: empty source name rejected
        let bad_cfg = NdiConfig {
            source_name: "   ".into(),
            ..Default::default()
        };
        assert!(NdiSenderState::new(bad_cfg).is_err());

        // Happy path + bad buffer size failure path
        let mut sender = NdiSenderState::new(NdiConfig::default()).expect("sender");
        assert_eq!(sender.config().alpha_mode, AlphaMode::StraightBgra);
        let good_frame = BroadcastFrame {
            width: 2,
            height: 1,
            timestamp_us: 100,
            sequence: 1,
            rgba_premul: vec![255, 0, 0, 255, 0, 0, 0, 0],
        };
        let bgra = sender.prepare_frame(&good_frame).expect("prepare");
        assert_eq!(&bgra[0..4], &[0, 0, 255, 255]);
        assert_eq!(sender.frames_sent(), 1);

        let bad_frame = BroadcastFrame {
            width: 4,
            height: 4,
            timestamp_us: 200,
            sequence: 2,
            rgba_premul: vec![0u8; 4],
        };
        assert!(sender.prepare_frame(&bad_frame).is_err());
    }
}
