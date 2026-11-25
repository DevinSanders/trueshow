//! Bidirectional MIDI protocol codec and MultiTracks Playback / Ableton Live cue router.
//!
//! Uses `midir` (CoreMIDI / Windows MIDI / ALSA) with zero AI-agent or MCP gating.

use core_scene::{LayerKind, SceneDiff, SlideId};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MidiMessage {
    NoteOn { channel: u8, note: u8, velocity: u8 },
    NoteOff { channel: u8, note: u8, velocity: u8 },
    ControlChange { channel: u8, controller: u8, value: u8 },
    ProgramChange { channel: u8, program: u8 },
}

impl MidiMessage {
    pub fn parse(raw: &[u8]) -> Option<Self> {
        let status = *raw.first()?;
        let kind = status & 0xF0;
        let channel = (status & 0x0F) + 1;
        match kind {
            0x90 if raw.len() >= 3 => {
                let note = raw[1] & 0x7F;
                let velocity = raw[2] & 0x7F;
                if velocity == 0 {
                    Some(Self::NoteOff { channel, note, velocity: 0 })
                } else {
                    Some(Self::NoteOn { channel, note, velocity })
                }
            }
            0x80 if raw.len() >= 3 => Some(Self::NoteOff {
                channel,
                note: raw[1] & 0x7F,
                velocity: raw[2] & 0x7F,
            }),
            0xB0 if raw.len() >= 3 => Some(Self::ControlChange {
                channel,
                controller: raw[1] & 0x7F,
                value: raw[2] & 0x7F,
            }),
            0xC0 if raw.len() >= 2 => Some(Self::ProgramChange {
                channel,
                program: raw[1] & 0x7F,
            }),
            _ => None,
        }
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        match *self {
            Self::NoteOn { channel, note, velocity } => {
                vec![0x90 | ((channel.saturating_sub(1)) & 0x0F), note & 0x7F, velocity & 0x7F]
            }
            Self::NoteOff { channel, note, velocity } => {
                vec![0x80 | ((channel.saturating_sub(1)) & 0x0F), note & 0x7F, velocity & 0x7F]
            }
            Self::ControlChange { channel, controller, value } => {
                vec![0xB0 | ((channel.saturating_sub(1)) & 0x0F), controller & 0x7F, value & 0x7F]
            }
            Self::ProgramChange { channel, program } => {
                vec![0xC0 | ((channel.saturating_sub(1)) & 0x0F), program & 0x7F]
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MidiTrigger {
    NoteOn { channel: u8, note: u8 },
    ControlChange { channel: u8, controller: u8, min_value: u8 },
    ProgramChange { channel: u8, program: u8 },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum MidiCueBinding {
    Scene(Box<SceneDiff>),
    RunMacro(Uuid),
}

/// Maps inbound MultiTracks Playback / Ableton Live MIDI cues to TrueShow actions.
#[derive(Default)]
pub struct MidiCueRouter {
    note_bindings: HashMap<(u8, u8), MidiCueBinding>,
    pc_bindings: HashMap<(u8, u8), MidiCueBinding>,
    cc_bindings: Vec<(u8, u8, u8, MidiCueBinding)>,
}

impl MidiCueRouter {
    pub fn new() -> Self {
        Self::default()
    }

    /// Standard MultiTracks Playback MIDI cue preset (Channel 16 standard worship automation profile).
    pub fn multitracks_playback_preset(deck_slide_ids: &[SlideId]) -> Self {
        let mut router = Self::new();
        for (idx, &slide_id) in deck_slide_ids.iter().enumerate() {
            if let Ok(note) = u8::try_from(idx) {
                router.bind(
                    MidiTrigger::NoteOn { channel: 16, note },
                    MidiCueBinding::Scene(Box::new(SceneDiff::TriggerSlide {
                        slide_id,
                        transition_override: None,
                    })),
                );
            }
        }
        router.bind(
            MidiTrigger::NoteOn { channel: 16, note: 120 },
            MidiCueBinding::Scene(Box::new(SceneDiff::ClearLayer(LayerKind::Slide))),
        );
        router.bind(
            MidiTrigger::NoteOn { channel: 16, note: 127 },
            MidiCueBinding::Scene(Box::new(SceneDiff::SetBlackout(true))),
        );
        router
    }

    pub fn bind(&mut self, trigger: MidiTrigger, binding: MidiCueBinding) {
        match trigger {
            MidiTrigger::NoteOn { channel, note } => {
                self.note_bindings.insert((channel, note), binding);
            }
            MidiTrigger::ProgramChange { channel, program } => {
                self.pc_bindings.insert((channel, program), binding);
            }
            MidiTrigger::ControlChange {
                channel,
                controller,
                min_value,
            } => {
                self.cc_bindings.push((channel, controller, min_value, binding));
            }
        }
    }

    pub fn route(&self, msg: &MidiMessage) -> Option<MidiCueBinding> {
        match *msg {
            MidiMessage::NoteOn { channel, note, velocity } if velocity > 0 => {
                self.note_bindings.get(&(channel, note)).cloned()
            }
            MidiMessage::ProgramChange { channel, program } => {
                self.pc_bindings.get(&(channel, program)).cloned()
            }
            MidiMessage::ControlChange {
                channel,
                controller,
                value,
            } => self
                .cc_bindings
                .iter()
                .find(|(ch, cc, min_v, _)| *ch == channel && *cc == controller && value >= *min_v)
                .map(|(_, _, _, b)| b.clone()),
            _ => None,
        }
    }
}

/// Lists available OS MIDI input and output port names via `midir`.
pub fn list_midi_ports() -> (Vec<String>, Vec<String>) {
    let inputs = midir::MidiInput::new("trueshow-probe")
        .ok()
        .map(|mi| {
            mi.ports()
                .iter()
                .filter_map(|p| mi.port_name(p).ok())
                .collect()
        })
        .unwrap_or_default();

    let outputs = midir::MidiOutput::new("trueshow-probe")
        .ok()
        .map(|mo| {
            mo.ports()
                .iter()
                .filter_map(|p| mo.port_name(p).ok())
                .collect()
        })
        .unwrap_or_default();

    (inputs, outputs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_encodes_and_routes_multitracks_and_ableton_midi_cues() {
        let on = MidiMessage::NoteOn { channel: 16, note: 1, velocity: 100 };
        assert_eq!(MidiMessage::parse(&on.to_bytes()), Some(on.clone()));

        let off_vel0 = MidiMessage::parse(&[0x9F, 1, 0]).expect("vel0 -> NoteOff");
        assert_eq!(off_vel0, MidiMessage::NoteOff { channel: 16, note: 1, velocity: 0 });

        let off = MidiMessage::NoteOff { channel: 2, note: 60, velocity: 64 };
        assert_eq!(MidiMessage::parse(&off.to_bytes()), Some(off));

        let cc = MidiMessage::ControlChange { channel: 1, controller: 64, value: 127 };
        assert_eq!(MidiMessage::parse(&cc.to_bytes()), Some(cc.clone()));

        let pc = MidiMessage::ProgramChange { channel: 3, program: 12 };
        assert_eq!(MidiMessage::parse(&pc.to_bytes()), Some(pc.clone()));

        // Failure paths: empty or truncated bytes return None
        assert!(MidiMessage::parse(&[]).is_none());
        assert!(MidiMessage::parse(&[0x90, 60]).is_none());
        assert!(MidiMessage::parse(&[0xF0, 0x01]).is_none());

        let s0 = SlideId::from_u128(1001);
        let s1 = SlideId::from_u128(1002);
        let mut router = MidiCueRouter::multitracks_playback_preset(&[s0, s1]);
        let macro_id = Uuid::from_u128(55);
        router.bind(
            MidiTrigger::ControlChange { channel: 1, controller: 64, min_value: 64 },
            MidiCueBinding::RunMacro(macro_id),
        );
        router.bind(
            MidiTrigger::ProgramChange { channel: 3, program: 12 },
            MidiCueBinding::Scene(Box::new(SceneDiff::ClearAll)),
        );

        assert_eq!(
            router.route(&on),
            Some(MidiCueBinding::Scene(Box::new(SceneDiff::TriggerSlide {
                slide_id: s1,
                transition_override: None,
            })))
        );
        assert_eq!(router.route(&cc), Some(MidiCueBinding::RunMacro(macro_id)));
        assert_eq!(
            router.route(&pc),
            Some(MidiCueBinding::Scene(Box::new(SceneDiff::ClearAll)))
        );
        assert!(router.route(&off_vel0).is_none());

        let _ports = list_midi_ports();
    }
}
