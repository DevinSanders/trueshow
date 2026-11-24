//! Pure-Rust SMPTE LTC (80-bit frame codec) and MIDI Timecode (MTC) quarter-frame assembler.

use serde::{Deserialize, Serialize};
use thiserror::Error;

const LTC_SYNC_WORD: u16 = 0xBFFC;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum TimecodeError {
    #[error("invalid timecode field: {0}")]
    InvalidField(String),
    #[error("SMPTE LTC sync word mismatch (expected 0xBFFC)")]
    InvalidSyncWord,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FrameRate {
    Fps24,
    Fps25,
    Fps2997DropFrame,
    Fps30,
}

impl FrameRate {
    pub const fn max_frame(self) -> u8 {
        match self {
            Self::Fps24 => 23,
            Self::Fps25 => 24,
            Self::Fps2997DropFrame | Self::Fps30 => 29,
        }
    }

    pub const fn is_drop_frame(self) -> bool {
        matches!(self, Self::Fps2997DropFrame)
    }

    pub const fn mtc_rate_code(self) -> u8 {
        match self {
            Self::Fps24 => 0,
            Self::Fps25 => 1,
            Self::Fps2997DropFrame => 2,
            Self::Fps30 => 3,
        }
    }

    pub const fn from_mtc_rate_code(code: u8) -> Self {
        match code & 0x03 {
            0 => Self::Fps24,
            1 => Self::Fps25,
            2 => Self::Fps2997DropFrame,
            _ => Self::Fps30,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SmpteTimecode {
    pub hours: u8,
    pub minutes: u8,
    pub seconds: u8,
    pub frames: u8,
    pub rate: FrameRate,
}

impl SmpteTimecode {
    pub fn new(
        hours: u8,
        minutes: u8,
        seconds: u8,
        frames: u8,
        rate: FrameRate,
    ) -> Result<Self, TimecodeError> {
        if hours > 23 {
            return Err(TimecodeError::InvalidField(format!("hours {hours} > 23")));
        }
        if minutes > 59 {
            return Err(TimecodeError::InvalidField(format!("minutes {minutes} > 59")));
        }
        if seconds > 59 {
            return Err(TimecodeError::InvalidField(format!("seconds {seconds} > 59")));
        }
        if frames > rate.max_frame() {
            return Err(TimecodeError::InvalidField(format!(
                "frames {frames} > {}",
                rate.max_frame()
            )));
        }
        Ok(Self {
            hours,
            minutes,
            seconds,
            frames,
            rate,
        })
    }

    pub fn format_string(&self) -> String {
        let sep = if self.rate.is_drop_frame() { ';' } else { ':' };
        format!(
            "{:02}:{:02}:{:02}{}{:02}",
            self.hours, self.minutes, self.seconds, sep, self.frames
        )
    }
}

fn write_bits(bits: &mut [u8; 80], start: usize, count: usize, value: u8) {
    for i in 0..count {
        bits[start + i] = (value >> i) & 1;
    }
}

fn read_bits(bits: &[u8; 80], start: usize, count: usize) -> u8 {
    let mut val = 0u8;
    for i in 0..count {
        val |= (bits[start + i] & 1) << i;
    }
    val
}

/// Encodes a [`SmpteTimecode`] into the standard 80-bit SMPTE 12M LTC bit frame.
pub fn encode_ltc_bits(tc: SmpteTimecode) -> [u8; 80] {
    let mut bits = [0u8; 80];
    write_bits(&mut bits, 0, 4, tc.frames % 10);
    write_bits(&mut bits, 8, 2, tc.frames / 10);
    bits[10] = u8::from(tc.rate.is_drop_frame());
    write_bits(&mut bits, 16, 4, tc.seconds % 10);
    write_bits(&mut bits, 24, 3, tc.seconds / 10);
    write_bits(&mut bits, 32, 4, tc.minutes % 10);
    write_bits(&mut bits, 40, 3, tc.minutes / 10);
    write_bits(&mut bits, 48, 4, tc.hours % 10);
    write_bits(&mut bits, 56, 2, tc.hours / 10);
    for i in 0..16 {
        bits[64 + i] = ((LTC_SYNC_WORD >> i) & 1) as u8;
    }
    bits
}

/// Decodes an 80-bit SMPTE 12M LTC bit frame, verifying the `0xBFFC` sync word.
pub fn decode_ltc_bits(bits: &[u8; 80], default_non_drop_rate: FrameRate) -> Result<SmpteTimecode, TimecodeError> {
    let mut sync = 0u16;
    for i in 0..16 {
        sync |= u16::from(bits[64 + i] & 1) << i;
    }
    if sync != LTC_SYNC_WORD {
        return Err(TimecodeError::InvalidSyncWord);
    }

    let frames = read_bits(bits, 0, 4) + read_bits(bits, 8, 2) * 10;
    let drop_flag = bits[10] != 0;
    let seconds = read_bits(bits, 16, 4) + read_bits(bits, 24, 3) * 10;
    let minutes = read_bits(bits, 32, 4) + read_bits(bits, 40, 3) * 10;
    let hours = read_bits(bits, 48, 4) + read_bits(bits, 56, 2) * 10;
    let rate = if drop_flag {
        FrameRate::Fps2997DropFrame
    } else {
        default_non_drop_rate
    };
    SmpteTimecode::new(hours, minutes, seconds, frames, rate)
}

/// Assembles 8 incoming MIDI Timecode (MTC `0xF1`) quarter-frame bytes into full timecode.
#[derive(Default)]
pub struct MtcAssembler {
    pieces: [u8; 8],
    received_mask: u8,
}

impl MtcAssembler {
    pub fn new() -> Self {
        Self::default()
    }

    /// Feeds one MTC quarter-frame data byte (`(piece_idx << 4) | nibble`).
    /// Returns `Some(SmpteTimecode)` once all 8 pieces (0..=7) have been assembled.
    pub fn push_quarter_frame(&mut self, data_byte: u8) -> Option<Result<SmpteTimecode, TimecodeError>> {
        let idx = usize::from((data_byte >> 4) & 0x07);
        let nibble = data_byte & 0x0F;
        self.pieces[idx] = nibble;
        self.received_mask |= 1 << idx;

        if self.received_mask == 0xFF && idx == 7 {
            let frames = self.pieces[0] | ((self.pieces[1] & 0x01) << 4);
            let seconds = self.pieces[2] | ((self.pieces[3] & 0x03) << 4);
            let minutes = self.pieces[4] | ((self.pieces[5] & 0x03) << 4);
            let hours = self.pieces[6] | ((self.pieces[7] & 0x01) << 4);
            let rate = FrameRate::from_mtc_rate_code((self.pieces[7] >> 1) & 0x03);
            self.received_mask = 0;
            return Some(SmpteTimecode::new(hours, minutes, seconds, frames, rate));
        }
        None
    }

    /// Encodes a timecode into its 8 MTC quarter-frame data bytes.
    pub fn encode_quarter_frames(tc: SmpteTimecode) -> [u8; 8] {
        [
            tc.frames & 0x0F,
            (1 << 4) | ((tc.frames >> 4) & 0x01),
            (2 << 4) | (tc.seconds & 0x0F),
            (3 << 4) | ((tc.seconds >> 4) & 0x03),
            (4 << 4) | (tc.minutes & 0x0F),
            (5 << 4) | ((tc.minutes >> 4) & 0x03),
            (6 << 4) | (tc.hours & 0x0F),
            (7 << 4) | ((tc.hours >> 4) & 0x01) | (tc.rate.mtc_rate_code() << 1),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_ltc_80_bit_frames_and_mtc_quarter_frames() {
        let tc = SmpteTimecode::new(1, 23, 45, 19, FrameRate::Fps2997DropFrame).expect("tc");
        assert_eq!(tc.format_string(), "01:23:45;19");

        let bits = encode_ltc_bits(tc);
        let decoded = decode_ltc_bits(&bits, FrameRate::Fps30).expect("decode ltc");
        assert_eq!(decoded, tc);

        let tc24 = SmpteTimecode::new(0, 5, 10, 23, FrameRate::Fps24).expect("tc24");
        assert_eq!(tc24.format_string(), "00:05:10:23");
        let qframes = MtcAssembler::encode_quarter_frames(tc24);
        let mut asm = MtcAssembler::new();
        let mut assembled = None;
        for b in qframes {
            assembled = asm.push_quarter_frame(b);
        }
        assert_eq!(assembled.expect("completed").expect("valid"), tc24);

        // Failure paths: out-of-range fields & corrupt sync word
        assert!(SmpteTimecode::new(25, 0, 0, 0, FrameRate::Fps30).is_err());
        assert!(SmpteTimecode::new(0, 60, 0, 0, FrameRate::Fps30).is_err());
        assert!(SmpteTimecode::new(0, 0, 60, 0, FrameRate::Fps30).is_err());
        assert!(SmpteTimecode::new(0, 0, 0, 25, FrameRate::Fps25).is_err());

        let mut bad_bits = bits;
        bad_bits[79] ^= 1;
        assert_eq!(
            decode_ltc_bits(&bad_bits, FrameRate::Fps30),
            Err(TimecodeError::InvalidSyncWord)
        );
    }
}
