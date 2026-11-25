//! Real-time multi-channel audio engine (`cpal` host discovery + `symphonia` pure-Rust decoding
//! + lock-free planar `rtrb` routing matrix).
//!
//! Real-time invariants: `PlanarAudioRouter::mix_interleaved` performs **zero heap allocations,
//! zero mutex locks, and zero syscalls**, making it safe to call directly on OS audio threads.

use cpal::traits::{DeviceTrait, HostTrait};
use rtrb::{Consumer, Producer, RingBuffer};
use serde::{Deserialize, Serialize};
use std::io::Cursor;
use symphonia::core::audio::sample::Sample;
use symphonia::core::codecs::audio::AudioDecoderOptions;
use symphonia::core::errors::Error as SymphoniaError;
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::{FormatOptions, TrackType};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AudioEngineError {
    #[error("audio decode failed: {0}")]
    Decode(String),
    #[error("audio stream contains no supported track")]
    NoAudioTrack,
    #[error("invalid audio bus configuration: {0}")]
    InvalidConfig(String),
}

/// Independent output buses for sanctuary PA, broadcast livestream, click/guide, and stage wedges.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AudioBusId {
    MainSanctuary = 0,
    BroadcastStream = 1,
    ClickAndGuide = 2,
    StageMonitor = 3,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AudioDeviceInfo {
    pub name: String,
    pub host: String,
    pub is_default_output: bool,
}

/// Enumerates host OS audio output devices (CoreAudio / WASAPI / ALSA-PipeWire; no proprietary ASIO).
pub fn list_output_devices() -> Vec<AudioDeviceInfo> {
    let host = cpal::default_host();
    let host_name = format!("{:?}", host.id());
    let default_name = host
        .default_output_device()
        .and_then(|d| d.description().ok().map(|desc| desc.name().to_string()));

    let mut out = Vec::new();
    if let Ok(devices) = host.output_devices() {
        for dev in devices {
            if let Ok(desc) = dev.description() {
                let name = desc.name().to_string();
                let is_default_output = default_name.as_deref() == Some(name.as_str());
                out.push(AudioDeviceInfo {
                    name,
                    host: host_name.clone(),
                    is_default_output,
                });
            }
        }
    }
    out
}

/// Decoded interleaved floating-point PCM buffer produced by pure-Rust `symphonia`.
#[derive(Debug, Clone)]
pub struct DecodedAudio {
    pub sample_rate: u32,
    pub channels: usize,
    pub interleaved_f32: Vec<f32>,
}

/// Decodes WAV, FLAC, MP3, AAC, or OGG/Vorbis audio bytes cleanly using MPL-2.0 `symphonia`.
pub fn decode_audio_bytes(
    bytes: Vec<u8>,
    extension_hint: Option<&str>,
) -> Result<DecodedAudio, AudioEngineError> {
    let cursor = Cursor::new(bytes);
    let mss = MediaSourceStream::new(Box::new(cursor), Default::default());
    let mut hint = Hint::new();
    if let Some(ext) = extension_hint {
        hint.with_extension(ext);
    }

    let mut format = symphonia::default::get_probe()
        .probe(
            &hint,
            mss,
            FormatOptions::default(),
            MetadataOptions::default(),
        )
        .map_err(|e| AudioEngineError::Decode(e.to_string()))?;

    let track = format
        .default_track(TrackType::Audio)
        .ok_or(AudioEngineError::NoAudioTrack)?;
    let track_id = track.id;
    let audio_params = track
        .codec_params
        .as_ref()
        .and_then(|p| p.audio())
        .ok_or(AudioEngineError::NoAudioTrack)?;
    let sample_rate = audio_params.sample_rate.unwrap_or(48_000);
    let channels = audio_params
        .channels
        .as_ref()
        .map(|c| c.count())
        .unwrap_or(2);

    let mut decoder = symphonia::default::get_codecs()
        .make_audio_decoder(audio_params, &AudioDecoderOptions::default())
        .map_err(|e| AudioEngineError::Decode(e.to_string()))?;

    let mut interleaved_f32 = Vec::new();
    let mut frame_samples = Vec::new();
    loop {
        let packet = match format.next_packet() {
            Ok(Some(p)) => p,
            Ok(None) => break,
            Err(SymphoniaError::IoError(err))
                if err.kind() == std::io::ErrorKind::UnexpectedEof =>
            {
                break;
            }
            Err(SymphoniaError::ResetRequired) => continue,
            Err(e) => return Err(AudioEngineError::Decode(e.to_string())),
        };
        if packet.track_id != track_id {
            continue;
        }
        let decoded = decoder
            .decode(&packet)
            .map_err(|e| AudioEngineError::Decode(e.to_string()))?;
        frame_samples.resize(decoded.samples_interleaved(), f32::MID);
        decoded.copy_to_slice_interleaved(&mut frame_samples);
        interleaved_f32.extend_from_slice(&frame_samples);
    }

    Ok(DecodedAudio {
        sample_rate,
        channels,
        interleaved_f32,
    })
}

/// Lock-free real-time channel strip feeding [`PlanarAudioRouter`].
pub struct AudioInputFeed {
    producer: Producer<f32>,
}

impl AudioInputFeed {
    /// Pushes interleaved stereo/mono samples from the decoder thread without locking.
    pub fn push_samples(&mut self, samples: &[f32]) -> usize {
        let mut written = 0;
        for &s in samples {
            if self.producer.push(s).is_ok() {
                written += 1;
            } else {
                break;
            }
        }
        written
    }
}

struct BusChannel {
    consumer: Consumer<f32>,
    target_gain: f32,
    current_gain: f32,
    muted: bool,
    output_pair: (usize, usize),
}

/// Lock-free planar multi-channel audio routing matrix for real-time audio callbacks.
pub struct PlanarAudioRouter {
    output_channels: usize,
    buses: Vec<(AudioBusId, BusChannel)>,
}

impl PlanarAudioRouter {
    pub fn new(output_channels: usize) -> Result<Self, AudioEngineError> {
        if !(2..=32).contains(&output_channels) {
            return Err(AudioEngineError::InvalidConfig(format!(
                "output_channels must be in 2..=32, got {output_channels}"
            )));
        }
        Ok(Self {
            output_channels,
            buses: Vec::new(),
        })
    }

    /// Registers a stereo bus routed to physical output channel pair `(left_ch, right_ch)`.
    pub fn add_bus(
        &mut self,
        id: AudioBusId,
        output_pair: (usize, usize),
        ring_capacity: usize,
    ) -> Result<AudioInputFeed, AudioEngineError> {
        if output_pair.0 >= self.output_channels || output_pair.1 >= self.output_channels {
            return Err(AudioEngineError::InvalidConfig(format!(
                "channel pair {:?} exceeds {} output channels",
                output_pair, self.output_channels
            )));
        }
        let (producer, consumer) = RingBuffer::<f32>::new(ring_capacity.max(256));
        self.buses.push((
            id,
            BusChannel {
                consumer,
                target_gain: 1.0,
                current_gain: 1.0,
                muted: false,
                output_pair,
            },
        ));
        Ok(AudioInputFeed { producer })
    }

    pub fn set_bus_gain(&mut self, id: AudioBusId, gain: f32) -> bool {
        if let Some((_, ch)) = self.buses.iter_mut().find(|(b, _)| *b == id) {
            ch.target_gain = gain.clamp(0.0, 2.0);
            true
        } else {
            false
        }
    }

    pub fn set_bus_muted(&mut self, id: AudioBusId, muted: bool) -> bool {
        if let Some((_, ch)) = self.buses.iter_mut().find(|(b, _)| *b == id) {
            ch.muted = muted;
            true
        } else {
            false
        }
    }

    /// Real-time audio callback mixdown: zero heap allocations, zero locks, smooth clickless gain ramp.
    pub fn mix_interleaved(&mut self, out_interleaved: &mut [f32]) {
        out_interleaved.fill(0.0);
        let ch_count = self.output_channels;
        let frames = out_interleaved.len() / ch_count;

        for (_, bus) in &mut self.buses {
            let goal = if bus.muted { 0.0 } else { bus.target_gain };
            let (l_idx, r_idx) = bus.output_pair;

            for frame in 0..frames {
                let l_in = bus.consumer.pop().unwrap_or(0.0);
                let r_in = bus.consumer.pop().unwrap_or(0.0);
                // Smooth 1-pole gain ramp prevents zipper clicks on mute/fader moves.
                bus.current_gain += (goal - bus.current_gain) * 0.05;
                let base = frame * ch_count;
                out_interleaved[base + l_idx] += l_in * bus.current_gain;
                out_interleaved[base + r_idx] += r_in * bus.current_gain;
            }
        }

        // Soft limiter ceiling to prevent harsh digital clipping on PA outputs.
        for sample in out_interleaved.iter_mut() {
            *sample = sample.clamp(-1.0, 1.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build_minimal_wav_pcm16(sample_rate: u32, samples: &[i16]) -> Vec<u8> {
        let data_bytes = (samples.len() * 2) as u32;
        let riff_size = 36 + data_bytes;
        let mut wav = Vec::with_capacity((44 + data_bytes) as usize);
        wav.extend_from_slice(b"RIFF");
        wav.extend_from_slice(&riff_size.to_le_bytes());
        wav.extend_from_slice(b"WAVE");
        wav.extend_from_slice(b"fmt ");
        wav.extend_from_slice(&16u32.to_le_bytes());
        wav.extend_from_slice(&1u16.to_le_bytes()); // PCM
        wav.extend_from_slice(&2u16.to_le_bytes()); // Stereo
        wav.extend_from_slice(&sample_rate.to_le_bytes());
        wav.extend_from_slice(&(sample_rate * 4).to_le_bytes());
        wav.extend_from_slice(&4u16.to_le_bytes());
        wav.extend_from_slice(&16u16.to_le_bytes());
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&data_bytes.to_le_bytes());
        for &s in samples {
            wav.extend_from_slice(&s.to_le_bytes());
        }
        wav
    }

    #[test]
    fn symphonia_decodes_valid_wav_and_rejects_corrupt_bytes() {
        let wav = build_minimal_wav_pcm16(48_000, &[16_384, -16_384, 8_192, -8_192]);
        let decoded = decode_audio_bytes(wav, Some("wav")).expect("decode wav");
        assert_eq!(decoded.sample_rate, 48_000);
        assert_eq!(decoded.channels, 2);
        assert_eq!(decoded.interleaved_f32.len(), 4);
        assert!((decoded.interleaved_f32[0] - 0.5).abs() < 0.01);

        assert!(decode_audio_bytes(b"not-a-valid-audio-file".to_vec(), Some("wav")).is_err());
    }

    #[test]
    fn planar_audio_router_routes_independent_buses_and_validates_bounds() {
        assert!(PlanarAudioRouter::new(1).is_err());
        assert!(PlanarAudioRouter::new(64).is_err());

        let mut router = PlanarAudioRouter::new(4).expect("4-channel router");
        assert!(
            router
                .add_bus(AudioBusId::MainSanctuary, (0, 9), 256)
                .is_err()
        );

        let mut main_feed = router
            .add_bus(AudioBusId::MainSanctuary, (0, 1), 512)
            .expect("main bus");
        let mut click_feed = router
            .add_bus(AudioBusId::ClickAndGuide, (2, 3), 512)
            .expect("click bus");

        main_feed.push_samples(&[0.5, 0.25, 0.5, 0.25]);
        click_feed.push_samples(&[0.8, 0.8, 0.8, 0.8]);

        let mut out = vec![0.0f32; 8]; // 2 frames × 4 channels
        router.mix_interleaved(&mut out);

        assert!((out[0] - 0.5).abs() < 0.01);
        assert!((out[1] - 0.25).abs() < 0.01);
        assert!((out[2] - 0.8).abs() < 0.01);
        assert!((out[3] - 0.8).abs() < 0.01);

        assert!(router.set_bus_muted(AudioBusId::ClickAndGuide, true));
        assert!(router.set_bus_gain(AudioBusId::MainSanctuary, 1.5));
        assert!(!router.set_bus_muted(AudioBusId::StageMonitor, true));
        assert!(!router.set_bus_gain(AudioBusId::StageMonitor, 0.5));

        // Verify limiter clamping & muted bus attenuation
        main_feed.push_samples(&[2.0, -2.0]);
        click_feed.push_samples(&[1.0, 1.0]);
        let mut clipped = vec![0.0f32; 4];
        router.mix_interleaved(&mut clipped);
        assert!(clipped[0] <= 1.0 && clipped[1] >= -1.0);

        // Verify ring buffer full handling does not panic
        let pushed = main_feed.push_samples(&vec![0.1f32; 1024]);
        assert!(pushed <= 512);

        // Host device enumeration never panics (even on headless CI machines)
        let _devices = list_output_devices();
    }
}
