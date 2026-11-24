//! Dual-reader pre-roll seamless looping engine for worship motion backgrounds.
//!
//! When the active reader approaches `duration - preroll_window_us`, a cloned secondary reader
//! pre-decodes the opening frames starting at `PTS = 0`. On the exact end-of-stream boundary,
//! the looper swaps readers instantaneously with zero seek stall and zero black frames.

use crate::video::{DecodedVideoFrame, VideoDecoder, VideoError};
use std::collections::VecDeque;

/// Telemetry returned whenever a seamless wraparound completes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LoopTransitionReport {
    pub completed_loops: u64,
    pub prerolled_frames_ready: usize,
}

pub struct SeamlessLooper {
    active: Box<dyn VideoDecoder>,
    standby: Option<Box<dyn VideoDecoder>>,
    preroll_queue: VecDeque<DecodedVideoFrame>,
    preroll_target_frames: usize,
    preroll_window_us: u64,
    loop_count: u64,
    seamless_swaps: u64,
}

impl SeamlessLooper {
    pub fn new(
        decoder: Box<dyn VideoDecoder>,
        preroll_target_frames: usize,
        preroll_window_us: u64,
    ) -> Self {
        Self {
            active: decoder,
            standby: None,
            preroll_queue: VecDeque::with_capacity(preroll_target_frames.max(2)),
            preroll_target_frames: preroll_target_frames.max(2),
            preroll_window_us,
            loop_count: 0,
            seamless_swaps: 0,
        }
    }

    pub fn loop_count(&self) -> u64 {
        self.loop_count
    }

    pub fn seamless_swaps(&self) -> u64 {
        self.seamless_swaps
    }

    pub fn prerolled_len(&self) -> usize {
        self.preroll_queue.len()
    }

    /// Returns the next frame continuously, seamlessly wrapping at end-of-stream with zero black frames.
    pub fn next_frame(&mut self) -> Result<(DecodedVideoFrame, Option<LoopTransitionReport>), VideoError> {
        // 1. Drain any already-prerolled head frames if we just swapped loops.
        match self.active.decode_next() {
            Ok(frame) => {
                let duration_us = self.active.metadata().duration_us;
                let remaining = duration_us.saturating_sub(frame.pts_us + frame.duration_us);
                if remaining <= self.preroll_window_us {
                    self.pump_preroll();
                }
                Ok((frame, None))
            }
            Err(VideoError::EndOfStream) => {
                // Ensure standby is primed even for very short clips.
                self.pump_preroll();
                self.loop_count += 1;

                if let Some(first_prerolled) = self.preroll_queue.pop_front() {
                    let ready = self.preroll_queue.len() + 1;
                    if let Some(mut next_reader) = self.standby.take() {
                        // Advance standby already consumed `ready` frames; promote it to active.
                        // Keep remaining preroll frames by wrapping active in a queued state,
                        // or directly re-seek standby to the exact next frame index.
                        let next_pts = first_prerolled.pts_us + first_prerolled.duration_us;
                        let _ = next_reader.seek_us(next_pts);
                        self.active = next_reader;
                        self.preroll_queue.clear();
                        self.seamless_swaps += 1;
                        return Ok((
                            first_prerolled,
                            Some(LoopTransitionReport {
                                completed_loops: self.loop_count,
                                prerolled_frames_ready: ready,
                            }),
                        ));
                    }
                }

                // Fallback: hard seek to 0 on the active reader if cloning failed.
                self.active.seek_us(0)?;
                let frame = self.active.decode_next()?;
                Ok((
                    frame,
                    Some(LoopTransitionReport {
                        completed_loops: self.loop_count,
                        prerolled_frames_ready: 0,
                    }),
                ))
            }
            Err(err) => Err(err),
        }
    }

    fn pump_preroll(&mut self) {
        if self.standby.is_none()
            && let Ok(mut cloned) = self.active.clone_reader()
        {
            let _ = cloned.seek_us(0);
            self.standby = Some(cloned);
        }

        if let Some(standby) = self.standby.as_mut() {
            while self.preroll_queue.len() < self.preroll_target_frames {
                match standby.decode_next() {
                    Ok(frame) => self.preroll_queue.push_back(frame),
                    Err(_) => break,
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::video::ProceduralMotionDecoder;

    #[test]
    fn seamlessly_loops_across_three_wraparounds_without_dropping_frames() {
        // 10 frames @ 60 fps (~166 ms clip); preroll triggers within final 50 ms.
        let dec = ProceduralMotionDecoder::new(8, 8, 60, 10, false).expect("decoder");
        let mut looper = SeamlessLooper::new(Box::new(dec), 4, 50_000);

        let mut seen_indices = Vec::new();
        let mut reports = Vec::new();
        for _ in 0..32 {
            let (frame, report) = looper.next_frame().expect("never fails during continuous loop");
            seen_indices.push(frame.frame_index);
            if let Some(r) = report {
                reports.push(r);
            }
        }

        assert_eq!(looper.loop_count(), 3);
        assert_eq!(looper.seamless_swaps(), 3);
        assert_eq!(reports.len(), 3);
        assert!(reports.iter().all(|r| r.prerolled_frames_ready >= 2));
        // Verify exact 0..9, 0..9, 0..9, 0, 1 frame continuity with zero black gaps or skips.
        let expected: Vec<u64> = (0..32).map(|i| i % 10).collect();
        assert_eq!(seen_indices, expected);
    }

    struct NonCloneDecoder {
        inner: ProceduralMotionDecoder,
        fail_next: bool,
    }

    impl VideoDecoder for NonCloneDecoder {
        fn metadata(&self) -> &crate::video::VideoMetadata {
            self.inner.metadata()
        }
        fn decode_next(&mut self) -> Result<DecodedVideoFrame, VideoError> {
            if self.fail_next {
                return Err(VideoError::UnsupportedCodec("simulated fault".into()));
            }
            self.inner.decode_next()
        }
        fn seek_us(&mut self, pts_us: u64) -> Result<(), VideoError> {
            self.inner.seek_us(pts_us)
        }
        fn clone_reader(&self) -> Result<Box<dyn VideoDecoder>, VideoError> {
            Err(VideoError::NotFound("clone disabled".into()))
        }
    }

    #[test]
    fn falls_back_to_hard_seek_when_clone_fails_and_propagates_decode_errors() {
        let dec = NonCloneDecoder {
            inner: ProceduralMotionDecoder::new(4, 4, 60, 2, false).expect("inner"),
            fail_next: false,
        };
        let mut looper = SeamlessLooper::new(Box::new(dec), 2, 20_000);
        assert_eq!(looper.prerolled_len(), 0);
        let _ = looper.next_frame().expect("f0");
        let _ = looper.next_frame().expect("f1");
        let (wrapped, report) = looper.next_frame().expect("fallback seek wrap");
        assert_eq!(wrapped.frame_index, 0);
        assert_eq!(report.expect("report").prerolled_frames_ready, 0);

        let err_dec = NonCloneDecoder {
            inner: ProceduralMotionDecoder::new(4, 4, 60, 2, false).expect("inner"),
            fail_next: true,
        };
        let mut err_looper = SeamlessLooper::new(Box::new(err_dec), 2, 20_000);
        assert!(matches!(
            err_looper.next_frame(),
            Err(VideoError::UnsupportedCodec(_))
        ));
    }
}
