//! TrueShow Media Engine:
//! - Hardware video decode & zero-copy GPU import descriptors (`VideoToolbox`, `D3D12VA`, `D3D11VA`, `VA-API`)
//! - Dynamic LGPL-2.1+ FFmpeg runtime discovery (zero static GPL taint)
//! - Dual-reader pre-roll seamless looping for motion backgrounds (`SeamlessLooper`)
//! - Real-time lock-free planar multi-channel audio routing (`cpal` + `symphonia` + `rtrb`)

pub mod audio;
pub mod ffmpeg_dyn;
pub mod looper;
pub mod video;

pub use audio::{
    AudioBusId, AudioDeviceInfo, AudioEngineError, PlanarAudioRouter, decode_audio_bytes,
};
pub use ffmpeg_dyn::{FfmpegLibraryInfo, FfmpegProbeStatus, probe_dynamic_lgpl_ffmpeg};
pub use looper::{LoopTransitionReport, SeamlessLooper};
pub use video::{
    DecodedVideoFrame, HwAccelBackend, NativeGpuSurfaceHandle, PixelFormat,
    ProceduralMotionDecoder, VideoDecoder, VideoError, VideoMetadata,
};
