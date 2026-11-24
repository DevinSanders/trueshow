//! Alpha channel conversions, Key & Fill matte splitting, and non-blocking frame queues.

use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

/// Alpha encoding modes expected by external switchers and IP video receivers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AlphaMode {
    PremultipliedBgra,
    StraightBgra,
    KeyFillSplit,
}

/// Video frame handed from the GPU async staging ring to a broadcast sink worker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BroadcastFrame {
    pub width: u32,
    pub height: u32,
    pub timestamp_us: u64,
    pub sequence: u64,
    pub rgba_premul: Vec<u8>,
}

/// Converts tightly packed premultiplied RGBA8 (GPU compositor output) into straight-alpha BGRA8
/// expected by NDI / OMT broadcast sinks without dark halo fringing.
pub fn premul_rgba_to_straight_bgra(src_rgba: &[u8], dst_bgra: &mut Vec<u8>) -> bool {
    if !src_rgba.len().is_multiple_of(4) {
        return false;
    }
    dst_bgra.resize(src_rgba.len(), 0);
    for (src, dst) in src_rgba
        .as_chunks::<4>()
        .0
        .iter()
        .zip(dst_bgra.as_chunks_mut::<4>().0.iter_mut())
    {
        let a = src[3];
        if a == 0 {
            dst[0] = 0;
            dst[1] = 0;
            dst[2] = 0;
            dst[3] = 0;
        } else if a == 255 {
            dst[0] = src[2]; // B
            dst[1] = src[1]; // G
            dst[2] = src[0]; // R
            dst[3] = 255;
        } else {
            let a_u32 = u32::from(a);
            let r = ((u32::from(src[0]) * 255 + a_u32 / 2) / a_u32).min(255) as u8;
            let g = ((u32::from(src[1]) * 255 + a_u32 / 2) / a_u32).min(255) as u8;
            let b = ((u32::from(src[2]) * 255 + a_u32 / 2) / a_u32).min(255) as u8;
            dst[0] = b;
            dst[1] = g;
            dst[2] = r;
            dst[3] = a;
        }
    }
    true
}

/// Splits premultiplied RGBA8 into synchronized **Fill** (straight RGB, opaque alpha) and
/// **Key** (grayscale alpha matte `B=G=R=alpha, A=255`) frames for dual-SDI external keyers.
pub fn split_key_fill_bgra(
    src_rgba: &[u8],
    out_fill_bgra: &mut Vec<u8>,
    out_key_luma_bgra: &mut Vec<u8>,
) -> bool {
    if !src_rgba.len().is_multiple_of(4) {
        return false;
    }
    out_fill_bgra.resize(src_rgba.len(), 0);
    out_key_luma_bgra.resize(src_rgba.len(), 0);

    for ((src, fill), key) in src_rgba
        .as_chunks::<4>()
        .0
        .iter()
        .zip(out_fill_bgra.as_chunks_mut::<4>().0.iter_mut())
        .zip(out_key_luma_bgra.as_chunks_mut::<4>().0.iter_mut())
    {
        let a = src[3];
        if a == 0 {
            *fill = [0, 0, 0, 255];
        } else if a == 255 {
            *fill = [src[2], src[1], src[0], 255];
        } else {
            let a_u32 = u32::from(a);
            let r = ((u32::from(src[0]) * 255 + a_u32 / 2) / a_u32).min(255) as u8;
            let g = ((u32::from(src[1]) * 255 + a_u32 / 2) / a_u32).min(255) as u8;
            let b = ((u32::from(src[2]) * 255 + a_u32 / 2) / a_u32).min(255) as u8;
            *fill = [b, g, r, 255];
        }
        *key = [a, a, a, 255];
    }
    true
}

/// Bounded drop-oldest broadcast frame queue isolating the 60 fps render thread from slow sinks.
#[derive(Debug)]
pub struct BroadcastQueue {
    capacity: usize,
    frames: VecDeque<BroadcastFrame>,
    dropped_count: u64,
}

impl BroadcastQueue {
    pub fn new(capacity: usize) -> Self {
        let cap = capacity.max(1);
        Self {
            capacity: cap,
            frames: VecDeque::with_capacity(cap),
            dropped_count: 0,
        }
    }

    pub fn push(&mut self, frame: BroadcastFrame) {
        if self.frames.len() >= self.capacity {
            let _ = self.frames.pop_front();
            self.dropped_count += 1;
        }
        self.frames.push_back(frame);
    }

    pub fn pop(&mut self) -> Option<BroadcastFrame> {
        self.frames.pop_front()
    }

    pub fn len(&self) -> usize {
        self.frames.len()
    }

    pub fn is_empty(&self) -> bool {
        self.frames.is_empty()
    }

    pub fn dropped_count(&self) -> u64 {
        self.dropped_count
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_premul_rgba_to_straight_bgra_without_dark_halos() {
        // Pixel 0: transparent, Pixel 1: opaque red, Pixel 2: 50% white premultiplied (128,128,128,128)
        let src = [0, 0, 0, 0, 255, 0, 0, 255, 128, 128, 128, 128];
        let mut dst = Vec::new();
        assert!(premul_rgba_to_straight_bgra(&src, &mut dst));
        assert_eq!(&dst[0..4], &[0, 0, 0, 0]);
        assert_eq!(&dst[4..8], &[0, 0, 255, 255]); // BGRA red
        assert_eq!(&dst[8..12], &[255, 255, 255, 128]); // un-premultiplied pure white @ 50% alpha

        // Failure path: non-multiple-of-4 input rejected cleanly
        assert!(!premul_rgba_to_straight_bgra(&[1, 2, 3], &mut dst));
    }

    #[test]
    fn splits_synchronized_key_and_fill_streams() {
        let src = [0, 0, 0, 0, 10, 20, 30, 255, 64, 128, 0, 128];
        let mut fill = Vec::new();
        let mut key = Vec::new();
        assert!(split_key_fill_bgra(&src, &mut fill, &mut key));

        assert_eq!(&fill[0..4], &[0, 0, 0, 255]);
        assert_eq!(&key[0..4], &[0, 0, 0, 255]);
        assert_eq!(&fill[4..8], &[30, 20, 10, 255]);
        assert_eq!(&key[4..8], &[255, 255, 255, 255]);
        assert_eq!(&fill[8..12], &[0, 255, 128, 255]);
        assert_eq!(&key[8..12], &[128, 128, 128, 255]);

        assert!(!split_key_fill_bgra(&[1, 2], &mut fill, &mut key));
    }

    #[test]
    fn bounded_queue_drops_oldest_frame_without_blocking_render_thread() {
        let mut q = BroadcastQueue::new(2);
        assert!(q.is_empty());
        for seq in 1..=4u64 {
            q.push(BroadcastFrame {
                width: 2,
                height: 2,
                timestamp_us: seq * 16_666,
                sequence: seq,
                rgba_premul: vec![0u8; 16],
            });
        }
        assert_eq!(q.len(), 2);
        assert_eq!(q.dropped_count(), 2);
        assert_eq!(q.pop().expect("f3").sequence, 3);
        assert_eq!(q.pop().expect("f4").sequence, 4);
        assert!(q.pop().is_none());
    }
}
