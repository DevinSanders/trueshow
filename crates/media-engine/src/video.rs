//! Video frame types, zero-copy GPU surface descriptors, and decoder trait.

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum VideoError {
    #[error("media source not found: {0}")]
    NotFound(String),
    #[error("unsupported or corrupt container/codec: {0}")]
    UnsupportedCodec(String),
    #[error("seek target {pts_us} us out of range (duration {duration_us} us)")]
    SeekOutOfRange { pts_us: u64, duration_us: u64 },
    #[error("decoder stream reached end-of-file")]
    EndOfStream,
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

/// Supported pixel formats spanning hardware NV12/P010, ProRes 4444 alpha, and VP9+alpha.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PixelFormat {
    Rgba8,
    Bgra8,
    Nv12,
    P010,
    /// WebM VP9 + Alpha (`yuva420p` dual-stream)
    Yuva420p,
    /// Apple ProRes 4444 (`yuva444p10le`)
    Yuva444p10,
}

impl PixelFormat {
    pub const fn has_alpha(self) -> bool {
        matches!(self, Self::Rgba8 | Self::Bgra8 | Self::Yuva420p | Self::Yuva444p10)
    }

    pub const fn bytes_per_frame_rgba_equivalent(self, width: u32, height: u32) -> usize {
        (width as usize) * (height as usize) * 4
    }
}

/// Hardware acceleration backend selected at runtime.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HwAccelBackend {
    VideoToolboxMetal,
    D3D12Va,
    D3D11VaShared,
    VaApiDmaBuf,
    CpuStagingFallback,
}

impl HwAccelBackend {
    /// Returns the preferred zero-copy backend for the current OS target.
    pub const fn preferred_for_host() -> Self {
        #[cfg(target_os = "macos")]
        {
            Self::VideoToolboxMetal
        }
        #[cfg(target_os = "windows")]
        {
            Self::D3D12Va
        }
        #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
        {
            Self::VaApiDmaBuf
        }
    }

    /// Selects the runtime decode path for a specific codec & alpha requirement.
    ///
    /// On Apple Silicon, VideoToolbox hardware-decodes ProRes 4444 with alpha; on Windows/Linux,
    /// ProRes 4444 and WebM VP9+alpha automatically use the LGPL CPU decode + GPU YUVA upload path.
    pub fn select_for_codec(codec_name: &str, has_alpha: bool, hw_available: bool) -> Self {
        let lower = codec_name.to_ascii_lowercase();
        if !hw_available {
            return Self::CpuStagingFallback;
        }
        if lower.contains("vp9") && has_alpha {
            return Self::CpuStagingFallback;
        }
        if lower.contains("prores") {
            #[cfg(target_os = "macos")]
            {
                return Self::VideoToolboxMetal;
            }
            #[cfg(not(target_os = "macos"))]
            {
                return Self::CpuStagingFallback;
            }
        }
        Self::preferred_for_host()
    }
}

/// Zero-copy native OS GPU texture descriptor for `wgpu-hal` import (ADR-005).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NativeGpuSurfaceHandle {
    /// macOS `IOSurfaceID` from `CVPixelBufferGetIOSurface`.
    MacOsIoSurface { iosurface_id: u32 },
    /// Windows D3D12 shared NT handle or device-local resource ID.
    WindowsDxgiSharedHandle { nt_handle: usize, fence_value: u64 },
    /// Linux DRM PRIME DMA-BUF file descriptor & format modifier.
    LinuxDmaBuf { fd: i32, drm_modifier: u64 },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VideoMetadata {
    pub width: u32,
    pub height: u32,
    pub fps_num: u32,
    pub fps_den: u32,
    pub duration_us: u64,
    pub codec: String,
    pub pixel_format: PixelFormat,
    pub backend: HwAccelBackend,
}

impl VideoMetadata {
    pub fn frame_duration_us(&self) -> u64 {
        let num = u64::from(self.fps_num.max(1));
        let den = u64::from(self.fps_den.max(1));
        (1_000_000 * den) / num
    }
}

/// Decoded video frame carrying either a zero-copy GPU handle or tightly packed CPU staging planes.
#[derive(Debug, Clone)]
pub struct DecodedVideoFrame {
    pub frame_index: u64,
    pub pts_us: u64,
    pub duration_us: u64,
    pub width: u32,
    pub height: u32,
    pub pixel_format: PixelFormat,
    pub rgba_staging: Option<Vec<u8>>,
    pub native_handle: Option<NativeGpuSurfaceHandle>,
}

/// Unified video decoder trait implemented by dynamic LGPL FFmpeg and procedural/test decoders.
pub trait VideoDecoder: Send {
    fn metadata(&self) -> &VideoMetadata;
    fn decode_next(&mut self) -> Result<DecodedVideoFrame, VideoError>;
    fn seek_us(&mut self, pts_us: u64) -> Result<(), VideoError>;
    fn clone_reader(&self) -> Result<Box<dyn VideoDecoder>, VideoError>;
}

/// Pure-Rust procedural RGBA/Alpha motion decoder for offline testing, demos, and fallback loops.
#[derive(Debug, Clone)]
pub struct ProceduralMotionDecoder {
    meta: VideoMetadata,
    total_frames: u64,
    cursor: u64,
}

impl ProceduralMotionDecoder {
    pub fn new(width: u32, height: u32, fps: u32, total_frames: u64, with_alpha: bool) -> Result<Self, VideoError> {
        if width == 0 || height == 0 || fps == 0 || total_frames == 0 {
            return Err(VideoError::UnsupportedCodec(
                "dimensions, fps, and total_frames must be non-zero".into(),
            ));
        }
        let frame_us = 1_000_000 / u64::from(fps);
        let meta = VideoMetadata {
            width,
            height,
            fps_num: fps,
            fps_den: 1,
            duration_us: frame_us * total_frames,
            codec: if with_alpha { "prores4444-sim".into() } else { "h264-sim".into() },
            pixel_format: PixelFormat::Rgba8,
            backend: HwAccelBackend::CpuStagingFallback,
        };
        Ok(Self {
            meta,
            total_frames,
            cursor: 0,
        })
    }
}

impl VideoDecoder for ProceduralMotionDecoder {
    fn metadata(&self) -> &VideoMetadata {
        &self.meta
    }

    fn decode_next(&mut self) -> Result<DecodedVideoFrame, VideoError> {
        if self.cursor >= self.total_frames {
            return Err(VideoError::EndOfStream);
        }
        let frame_idx = self.cursor;
        self.cursor += 1;

        let frame_dur = self.meta.frame_duration_us();
        let w = self.meta.width as usize;
        let h = self.meta.height as usize;
        let mut rgba = vec![0u8; w * h * 4];
        let phase = (frame_idx as f32 / self.total_frames as f32) * std::f32::consts::TAU;
        let r = ((phase.sin() * 0.5 + 0.5) * 255.0) as u8;
        let g = (((phase + 2.0).sin() * 0.5 + 0.5) * 255.0) as u8;
        let b = (((phase + 4.0).sin() * 0.5 + 0.5) * 255.0) as u8;
        let a = if self.meta.codec.contains("4444") { 200 } else { 255 };

        for px in rgba.as_chunks_mut::<4>().0 {
            px[0] = r;
            px[1] = g;
            px[2] = b;
            px[3] = a;
        }

        Ok(DecodedVideoFrame {
            frame_index: frame_idx,
            pts_us: frame_idx * frame_dur,
            duration_us: frame_dur,
            width: self.meta.width,
            height: self.meta.height,
            pixel_format: PixelFormat::Rgba8,
            rgba_staging: Some(rgba),
            native_handle: None,
        })
    }

    fn seek_us(&mut self, pts_us: u64) -> Result<(), VideoError> {
        if pts_us > self.meta.duration_us {
            return Err(VideoError::SeekOutOfRange {
                pts_us,
                duration_us: self.meta.duration_us,
            });
        }
        let frame_dur = self.meta.frame_duration_us().max(1);
        self.cursor = (pts_us / frame_dur).min(self.total_frames);
        Ok(())
    }

    fn clone_reader(&self) -> Result<Box<dyn VideoDecoder>, VideoError> {
        Ok(Box::new(Self {
            meta: self.meta.clone(),
            total_frames: self.total_frames,
            cursor: 0,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pixel_format_alpha_and_backend_selection_rules() {
        assert!(PixelFormat::Yuva444p10.has_alpha());
        assert!(PixelFormat::Yuva420p.has_alpha());
        assert!(!PixelFormat::Nv12.has_alpha());
        assert!(!PixelFormat::P010.has_alpha());
        assert_eq!(PixelFormat::Rgba8.bytes_per_frame_rgba_equivalent(10, 10), 400);

        assert_eq!(
            HwAccelBackend::select_for_codec("vp9", true, true),
            HwAccelBackend::CpuStagingFallback
        );
        assert_eq!(
            HwAccelBackend::select_for_codec("h264", false, false),
            HwAccelBackend::CpuStagingFallback
        );
        assert_eq!(
            HwAccelBackend::select_for_codec("hevc", false, true),
            HwAccelBackend::preferred_for_host()
        );
    }

    #[test]
    fn procedural_decoder_handles_happy_and_failure_paths() {
        assert!(ProceduralMotionDecoder::new(0, 16, 60, 10, false).is_err());

        let mut dec = ProceduralMotionDecoder::new(8, 4, 60, 2, true).expect("valid decoder");
        let f0 = dec.decode_next().expect("frame 0");
        assert_eq!(f0.frame_index, 0);
        assert_eq!(f0.rgba_staging.as_ref().expect("rgba")[3], 200);

        let f1 = dec.decode_next().expect("frame 1");
        assert_eq!(f1.frame_index, 1);

        assert!(matches!(dec.decode_next(), Err(VideoError::EndOfStream)));
        assert!(matches!(
            dec.seek_us(999_999_999),
            Err(VideoError::SeekOutOfRange { .. })
        ));
        dec.seek_us(0).expect("seek to start");
        assert_eq!(dec.decode_next().expect("frame 0 again").frame_index, 0);
    }
}
