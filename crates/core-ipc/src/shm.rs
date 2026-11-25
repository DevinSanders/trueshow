//! Lock-free triple-buffered shared-memory ring for GPU preview & editor frames.
//!
//! Layout of the memory-mapped file:
//! - Bytes `0..64`: [`ShmHeader`] (`#[repr(C, align(64))]`)
//! - Slot 0: `64 .. 64 + slot_bytes`
//! - Slot 1: `64 + slot_bytes .. 64 + 2 * slot_bytes`
//! - Slot 2: `64 + 2 * slot_bytes .. 64 + 3 * slot_bytes`
//!
//! Because there are 3 slots and a single writer only advances into `(active + 1) % 3`,
//! the reader verifies the atomic `write_seq` before and after copying `active_slot`,
//! yielding tear-free RGBA8 frames across OS process boundaries with zero mutexes.

use memmap2::{Mmap, MmapMut};
use std::fs::{File, OpenOptions};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering, fence};
use thiserror::Error;

const SHM_MAGIC: [u8; 8] = *b"OPSHM001";
const SHM_VERSION: u32 = 1;
const SLOT_COUNT: usize = 3;
const HEADER_SIZE: usize = std::mem::size_of::<ShmHeader>();

#[repr(C, align(64))]
struct ShmHeader {
    magic: [u8; 8],
    version: u32,
    width: u32,
    height: u32,
    stride: u32,
    slot_bytes: u32,
    active_slot: AtomicU32,
    write_seq: AtomicU64,
    timestamp_us: AtomicU64,
    _reserved: [u8; 16],
}

const _: () = assert!(HEADER_SIZE == 64);

/// Metadata describing a preview frame read from shared memory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PreviewFrameMeta {
    pub width: u32,
    pub height: u32,
    pub sequence: u64,
    pub timestamp_us: u64,
}

#[derive(Debug, Error)]
pub enum ShmError {
    #[error("I/O error accessing preview ring: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid shared-memory header magic or version")]
    InvalidHeader,
    #[error("shared-memory file truncated (expected at least {expected} bytes, got {actual})")]
    Truncated { expected: usize, actual: usize },
    #[error("pixel buffer length mismatch (expected {expected} bytes, got {actual})")]
    BufferSizeMismatch { expected: usize, actual: usize },
}

/// Single-producer shared-memory preview writer owned by `trueshow-engine`.
pub struct ShmPreviewWriter {
    path: PathBuf,
    _file: File,
    mmap: MmapMut,
    width: u32,
    height: u32,
    slot_bytes: usize,
}

impl ShmPreviewWriter {
    /// Creates (or truncates) a triple-buffered RGBA8 preview ring at `path`.
    pub fn create(path: impl AsRef<Path>, width: u32, height: u32) -> Result<Self, ShmError> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let stride = width
            .checked_mul(4)
            .ok_or_else(|| ShmError::Io(std::io::Error::other("preview stride overflow")))?;
        let slot_bytes = (stride as usize)
            .checked_mul(height as usize)
            .ok_or_else(|| ShmError::Io(std::io::Error::other("preview buffer overflow")))?;
        let total_len = HEADER_SIZE + slot_bytes * SLOT_COUNT;

        let file = OpenOptions::new()
            .create(true)
            .truncate(true)
            .read(true)
            .write(true)
            .open(&path)?;
        file.set_len(total_len as u64)?;

        #[allow(unsafe_code)]
        // SAFETY: `file` is freshly created/sized exclusively by this writer and mapped for its lifetime.
        let mut mmap = unsafe { MmapMut::map_mut(&file)? };

        #[allow(unsafe_code)]
        // SAFETY: `mmap` starts at page alignment (>= 64) and has size >= `HEADER_SIZE` (64 bytes).
        unsafe {
            let hdr = &mut *(mmap.as_mut_ptr() as *mut ShmHeader);
            hdr.magic = SHM_MAGIC;
            hdr.version = SHM_VERSION;
            hdr.width = width;
            hdr.height = height;
            hdr.stride = stride;
            hdr.slot_bytes = slot_bytes as u32;
            hdr.active_slot = AtomicU32::new(0);
            hdr.write_seq = AtomicU64::new(0);
            hdr.timestamp_us = AtomicU64::new(0);
            hdr._reserved = [0u8; 16];
        }
        fence(Ordering::Release);

        Ok(Self {
            path,
            _file: file,
            mmap,
            width,
            height,
            slot_bytes,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn dimensions(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    /// Writes a tightly packed `RGBA8` frame into the next inactive slot and atomically publishes it.
    pub fn publish_rgba(&mut self, rgba_pixels: &[u8], timestamp_us: u64) -> Result<u64, ShmError> {
        if rgba_pixels.len() != self.slot_bytes {
            return Err(ShmError::BufferSizeMismatch {
                expected: self.slot_bytes,
                actual: rgba_pixels.len(),
            });
        }

        let current_slot = self.header().active_slot.load(Ordering::Relaxed) as usize;
        let next_slot = (current_slot + 1) % SLOT_COUNT;
        let offset = HEADER_SIZE + next_slot * self.slot_bytes;

        self.mmap[offset..offset + self.slot_bytes].copy_from_slice(rgba_pixels);

        fence(Ordering::Release);
        let hdr = self.header();
        hdr.timestamp_us.store(timestamp_us, Ordering::Relaxed);
        hdr.active_slot.store(next_slot as u32, Ordering::Release);
        let seq = hdr.write_seq.fetch_add(1, Ordering::AcqRel) + 1;
        Ok(seq)
    }

    fn header(&self) -> &ShmHeader {
        #[allow(unsafe_code)]
        // SAFETY: `self.mmap` is page-aligned and initialized with a valid `ShmHeader`.
        unsafe {
            &*(self.mmap.as_ptr() as *const ShmHeader)
        }
    }
}

/// Lock-free shared-memory preview reader used by `apps/desktop` (Tauri 2 shell).
pub struct ShmPreviewReader {
    _file: File,
    mmap: Mmap,
    width: u32,
    height: u32,
    slot_bytes: usize,
    last_seq: u64,
}

impl ShmPreviewReader {
    /// Opens an existing preview ring created by `trueshow-engine`.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, ShmError> {
        let file = OpenOptions::new().read(true).open(path)?;
        #[allow(unsafe_code)]
        // SAFETY: Read-only mapping of the writer's triple-buffered file; header/slot access uses atomic fences.
        let mmap = unsafe { Mmap::map(&file)? };

        if mmap.len() < HEADER_SIZE {
            return Err(ShmError::Truncated {
                expected: HEADER_SIZE,
                actual: mmap.len(),
            });
        }

        #[allow(unsafe_code)]
        // SAFETY: `mmap` is page-aligned and has length >= `HEADER_SIZE`.
        let hdr = unsafe { &*(mmap.as_ptr() as *const ShmHeader) };
        fence(Ordering::Acquire);

        if hdr.magic != SHM_MAGIC || hdr.version != SHM_VERSION {
            return Err(ShmError::InvalidHeader);
        }

        let width = hdr.width;
        let height = hdr.height;
        let slot_bytes = hdr.slot_bytes as usize;
        let expected_total = HEADER_SIZE + slot_bytes * SLOT_COUNT;
        if mmap.len() < expected_total {
            return Err(ShmError::Truncated {
                expected: expected_total,
                actual: mmap.len(),
            });
        }

        Ok(Self {
            _file: file,
            mmap,
            width,
            height,
            slot_bytes,
            last_seq: 0,
        })
    }

    pub fn dimensions(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    /// Reads the newest unpublished frame into `out_rgba` if one is available.
    ///
    /// Returns `None` when no newer frame has been published since the previous call.
    pub fn try_read_latest(&mut self, out_rgba: &mut Vec<u8>) -> Option<PreviewFrameMeta> {
        let hdr = self.header();
        let seq_before = hdr.write_seq.load(Ordering::Acquire);
        if seq_before == 0 || seq_before == self.last_seq {
            return None;
        }

        out_rgba.resize(self.slot_bytes, 0);

        for _ in 0..3 {
            let s1 = hdr.write_seq.load(Ordering::Acquire);
            let slot = (hdr.active_slot.load(Ordering::Acquire) as usize) % SLOT_COUNT;
            let ts = hdr.timestamp_us.load(Ordering::Relaxed);
            let offset = HEADER_SIZE + slot * self.slot_bytes;

            out_rgba.copy_from_slice(&self.mmap[offset..offset + self.slot_bytes]);
            fence(Ordering::Acquire);

            let s2 = hdr.write_seq.load(Ordering::Acquire);
            // With 3 slots, a slot can only be overwritten if writer advanced by >= 2 frames during copy.
            if s2.saturating_sub(s1) < 2 {
                self.last_seq = s2;
                return Some(PreviewFrameMeta {
                    width: self.width,
                    height: self.height,
                    sequence: s2,
                    timestamp_us: ts,
                });
            }
        }

        None
    }

    fn header(&self) -> &ShmHeader {
        #[allow(unsafe_code)]
        // SAFETY: Validated during `open`.
        unsafe {
            &*(self.mmap.as_ptr() as *const ShmHeader)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn publishes_and_reads_frames_without_tearing() {
        let path =
            std::env::temp_dir().join(format!("trueshow-test-{}.op_shm", std::process::id()));
        let mut writer = ShmPreviewWriter::create(&path, 16, 16).expect("create writer");
        let mut reader = ShmPreviewReader::open(&path).expect("open reader");

        let mut out = Vec::new();
        assert!(reader.try_read_latest(&mut out).is_none());

        let frame1 = vec![0xAAu8; 16 * 16 * 4];
        let seq1 = writer.publish_rgba(&frame1, 1000).expect("publish 1");
        assert_eq!(seq1, 1);

        let meta1 = reader.try_read_latest(&mut out).expect("read 1");
        assert_eq!(meta1.sequence, 1);
        assert_eq!(meta1.timestamp_us, 1000);
        assert_eq!(out, frame1);

        // No new frame yet.
        assert!(reader.try_read_latest(&mut out).is_none());

        let frame2 = vec![0x55u8; 16 * 16 * 4];
        writer.publish_rgba(&frame2, 2000).expect("publish 2");
        let meta2 = reader.try_read_latest(&mut out).expect("read 2");
        assert_eq!(meta2.sequence, 2);
        assert_eq!(out, frame2);

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn rejects_invalid_buffer_size_header_and_truncated_files() {
        let base =
            std::env::temp_dir().join(format!("trueshow-shm-err-{}.op_shm", std::process::id()));
        let mut writer = ShmPreviewWriter::create(&base, 8, 8).expect("create");
        assert_eq!(writer.path(), base.as_path());
        assert_eq!(writer.dimensions(), (8, 8));
        assert!(matches!(
            writer.publish_rgba(&[0u8; 4], 10),
            Err(ShmError::BufferSizeMismatch { .. })
        ));

        let reader = ShmPreviewReader::open(&base).expect("open");
        assert_eq!(reader.dimensions(), (8, 8));
        drop(reader);
        drop(writer);
        let _ = std::fs::remove_file(&base);

        // Missing file -> Io error
        let missing = std::env::temp_dir().join("trueshow-definitely-missing.op_shm");
        assert!(matches!(
            ShmPreviewReader::open(&missing),
            Err(ShmError::Io(_))
        ));

        // Truncated file (< 64 bytes)
        let trunc =
            std::env::temp_dir().join(format!("trueshow-trunc-{}.op_shm", std::process::id()));
        std::fs::write(&trunc, b"tiny").expect("write trunc");
        assert!(matches!(
            ShmPreviewReader::open(&trunc),
            Err(ShmError::Truncated { .. })
        ));
        let _ = std::fs::remove_file(&trunc);

        // Invalid magic header
        let bad_hdr =
            std::env::temp_dir().join(format!("trueshow-badhdr-{}.op_shm", std::process::id()));
        std::fs::write(&bad_hdr, vec![0x11u8; 128]).expect("write bad hdr");
        assert!(matches!(
            ShmPreviewReader::open(&bad_hdr),
            Err(ShmError::InvalidHeader)
        ));
        let _ = std::fs::remove_file(&bad_hdr);
    }
}
