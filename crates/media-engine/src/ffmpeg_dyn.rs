//! Dynamic runtime loader & LGPL compliance inspector for FFmpeg shared libraries.
//!
//! Never statically links FFmpeg (preventing accidental GPL taint). Inspects `avcodec_license()`
//! at runtime when FFmpeg shared libraries are present and flags any `--enable-gpl` / `--enable-nonfree`
//! builds so TrueShow stays cleanly LGPL-2.1+ hygienic.

use serde::{Deserialize, Serialize};
use std::ffi::CStr;
use std::os::raw::c_char;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FfmpegLibraryInfo {
    pub library_path: String,
    pub license_string: String,
    pub is_lgpl_compliant: bool,
    pub avcodec_version: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FfmpegProbeStatus {
    Loaded(FfmpegLibraryInfo),
    NotInstalled { searched_names: Vec<String> },
}

/// Evaluates an `avcodec_license()` string to confirm LGPL compliance (no GPL/nonfree flags).
pub fn is_lgpl_license_string(license: &str) -> bool {
    let lower = license.to_ascii_lowercase();
    lower.contains("lgpl") && !lower.contains("nonfree") && !lower.contains("unredistributable")
}

/// Candidate shared library names for dynamic `avcodec` loading across Windows, macOS, and Linux.
pub fn candidate_avcodec_names() -> Vec<&'static str> {
    #[cfg(target_os = "windows")]
    {
        vec!["avcodec-62.dll", "avcodec-61.dll", "avcodec-60.dll"]
    }
    #[cfg(target_os = "macos")]
    {
        vec![
            "libavcodec.62.dylib",
            "libavcodec.61.dylib",
            "libavcodec.60.dylib",
            "/opt/homebrew/lib/libavcodec.dylib",
            "/usr/local/lib/libavcodec.dylib",
        ]
    }
    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    {
        vec![
            "libavcodec.so.62",
            "libavcodec.so.61",
            "libavcodec.so.60",
            "libavcodec.so",
        ]
    }
}

/// Dynamically probes for LGPL FFmpeg shared libraries at runtime without failing if absent.
pub fn probe_dynamic_lgpl_ffmpeg(extra_dirs: &[std::path::PathBuf]) -> FfmpegProbeStatus {
    let mut searched = Vec::new();
    let names = candidate_avcodec_names();

    let mut candidates = Vec::new();
    for dir in extra_dirs {
        for name in &names {
            candidates.push(dir.join(name).to_string_lossy().into_owned());
        }
    }
    for name in &names {
        candidates.push((*name).to_string());
    }

    for candidate in candidates {
        searched.push(candidate.clone());
        #[allow(unsafe_code)]
        // SAFETY: Dynamic probe of standard FFmpeg C ABI functions (`avcodec_version`, `avcodec_license`).
        let loaded = unsafe { libloading::Library::new(&candidate) };
        if let Ok(lib) = loaded {
            #[allow(unsafe_code)]
            let info = unsafe {
                let version_fn: Result<libloading::Symbol<'_, unsafe extern "C" fn() -> u32>, _> =
                    lib.get(b"avcodec_version\0");
                let license_fn: Result<
                    libloading::Symbol<'_, unsafe extern "C" fn() -> *const c_char>,
                    _,
                > = lib.get(b"avcodec_license\0");

                if let (Ok(ver), Ok(lic)) = (version_fn, license_fn) {
                    let version = ver();
                    let ptr = lic();
                    let license_string = if ptr.is_null() {
                        "unknown".to_string()
                    } else {
                        CStr::from_ptr(ptr).to_string_lossy().into_owned()
                    };
                    let is_lgpl_compliant = is_lgpl_license_string(&license_string);
                    Some(FfmpegLibraryInfo {
                        library_path: candidate,
                        license_string,
                        is_lgpl_compliant,
                        avcodec_version: version,
                    })
                } else {
                    None
                }
            };
            if let Some(meta) = info {
                return FfmpegProbeStatus::Loaded(meta);
            }
        }
    }

    FfmpegProbeStatus::NotInstalled {
        searched_names: searched,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_lgpl_vs_gpl_nonfree_license_strings() {
        assert!(is_lgpl_license_string("LGPL version 2.1 or later"));
        assert!(is_lgpl_license_string("LGPL version 3 or later"));
        assert!(!is_lgpl_license_string("GPL version 2 or later"));
        assert!(!is_lgpl_license_string("nonfree and unredistributable"));
        assert!(!is_lgpl_license_string("LGPL version 2.1, nonfree"));
    }

    #[test]
    fn dynamic_probe_handles_missing_libraries_gracefully() {
        assert!(!candidate_avcodec_names().is_empty());
        let status =
            probe_dynamic_lgpl_ffmpeg(&[std::path::PathBuf::from("__nonexistent_ffmpeg_dir__")]);
        match status {
            FfmpegProbeStatus::Loaded(info) => assert!(info.avcodec_version > 0),
            FfmpegProbeStatus::NotInstalled { searched_names } => {
                assert!(!searched_names.is_empty());
            }
        }
    }
}
