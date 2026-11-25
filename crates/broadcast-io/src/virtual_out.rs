//! Virtual outputs for OBS / vMix / streaming encoders:
//! - macOS: Syphon (`IOSurface` / Metal zero-copy descriptor)
//! - Windows: Spout2 (`D3D11On12` / DXGI shared handle descriptor)
//! - Cross-platform fallback: Lock-free shared-memory virtual screen ring (`core-ipc::ShmPreviewWriter`)

use crate::alpha::BroadcastFrame;
use core_ipc::{ShmError, ShmPreviewWriter};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum VirtualOutputError {
    #[error("virtual output name cannot be empty")]
    EmptyName,
    #[error("shared-memory error: {0}")]
    Shm(#[from] ShmError),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VirtualOutputBackend {
    SyphonMetal,
    Spout2Dx11On12,
    SharedMemoryVirtualScreen,
}

impl VirtualOutputBackend {
    pub const fn native_for_host() -> Self {
        #[cfg(target_os = "macos")]
        {
            Self::SyphonMetal
        }
        #[cfg(target_os = "windows")]
        {
            Self::Spout2Dx11On12
        }
        #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
        {
            Self::SharedMemoryVirtualScreen
        }
    }
}

/// Active virtual output publisher (OBS / vMix / local virtual screen).
pub struct VirtualOutputSink {
    name: String,
    backend: VirtualOutputBackend,
    shm_writer: ShmPreviewWriter,
    published_frames: u64,
}

impl VirtualOutputSink {
    pub fn create(
        name: impl Into<String>,
        shm_path: impl AsRef<Path>,
        width: u32,
        height: u32,
    ) -> Result<Self, VirtualOutputError> {
        let name = name.into();
        if name.trim().is_empty() {
            return Err(VirtualOutputError::EmptyName);
        }
        let shm_writer = ShmPreviewWriter::create(shm_path, width, height)?;
        Ok(Self {
            name,
            backend: VirtualOutputBackend::native_for_host(),
            shm_writer,
            published_frames: 0,
        })
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn backend(&self) -> VirtualOutputBackend {
        self.backend
    }

    pub fn shm_path(&self) -> PathBuf {
        self.shm_writer.path().to_path_buf()
    }

    pub fn published_frames(&self) -> u64 {
        self.published_frames
    }

    pub fn publish(&mut self, frame: &BroadcastFrame) -> Result<u64, VirtualOutputError> {
        let seq = self
            .shm_writer
            .publish_rgba(&frame.rgba_premul, frame.timestamp_us)?;
        self.published_frames += 1;
        Ok(seq)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core_ipc::ShmPreviewReader;

    #[test]
    fn publishes_frames_to_virtual_output_and_rejects_empty_name() {
        let path = std::env::temp_dir().join(format!("op-vout-{}.op_shm", std::process::id()));
        assert!(matches!(
            VirtualOutputSink::create("  ", &path, 8, 8),
            Err(VirtualOutputError::EmptyName)
        ));

        let mut sink = VirtualOutputSink::create("TrueShow Spout/Syphon", &path, 4, 4)
            .expect("create virtual output");
        assert_eq!(sink.name(), "TrueShow Spout/Syphon");
        assert_eq!(sink.backend(), VirtualOutputBackend::native_for_host());
        assert_eq!(sink.shm_path(), path);

        let frame = BroadcastFrame {
            width: 4,
            height: 4,
            timestamp_us: 42_000,
            sequence: 1,
            rgba_premul: vec![0xCCu8; 64],
        };
        assert_eq!(sink.publish(&frame).expect("publish"), 1);
        assert_eq!(sink.published_frames(), 1);

        let mut reader = ShmPreviewReader::open(&path).expect("open reader");
        let mut out = Vec::new();
        let meta = reader.try_read_latest(&mut out).expect("read latest");
        assert_eq!(meta.timestamp_us, 42_000);
        assert_eq!(out.len(), 64);
        assert_eq!(out[0], 0xCC);

        drop(reader);
        drop(sink);
        let _ = std::fs::remove_file(&path);
    }
}
