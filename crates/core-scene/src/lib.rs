//! Canonical, resolution-independent scene graph for OpenPresenter.
//!
//! This crate intentionally contains zero OS, GPU, or async I/O dependencies so both
//! the real-time compositor (`openpresenter-engine`) and operator UI (`apps/desktop`)
//! operate on an identical single source of truth.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use uuid::Uuid;

/// Unique identifier for a logical output (physical screen, Stage, NDI, OMT, SDI, etc.).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct OutputId(pub Uuid);

impl OutputId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    pub const fn from_u128(v: u128) -> Self {
        Self(Uuid::from_u128(v))
    }
}

impl Default for OutputId {
    fn default() -> Self {
        Self::new()
    }
}

/// Unique identifier for a per-output Look configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct LookId(pub Uuid);

impl LookId {
    pub const fn from_u128(v: u128) -> Self {
        Self(Uuid::from_u128(v))
    }
}

/// Unique identifier for a slide.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct SlideId(pub Uuid);

impl SlideId {
    pub const fn from_u128(v: u128) -> Self {
        Self(Uuid::from_u128(v))
    }
}

/// Seven canonical presentation layers composited bottom-to-top on the GPU.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum LayerKind {
    VideoInput = 0,
    MediaBackground = 1,
    Slide = 2,
    Announcements = 3,
    PropsAndOverlays = 4,
    Masks = 5,
    MessagesAndTimers = 6,
}

impl LayerKind {
    pub const ALL: [LayerKind; 7] = [
        LayerKind::VideoInput,
        LayerKind::MediaBackground,
        LayerKind::Slide,
        LayerKind::Announcements,
        LayerKind::PropsAndOverlays,
        LayerKind::Masks,
        LayerKind::MessagesAndTimers,
    ];
}

/// Normalized RGBA color in [0.0, 1.0].
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ColorRgba {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl ColorRgba {
    pub const BLACK: Self = Self::rgb(0.0, 0.0, 0.0);
    pub const WHITE: Self = Self::rgb(1.0, 1.0, 1.0);
    pub const TRANSPARENT: Self = Self {
        r: 0.0,
        g: 0.0,
        b: 0.0,
        a: 0.0,
    };

    pub const fn rgb(r: f32, g: f32, b: f32) -> Self {
        Self { r, g, b, a: 1.0 }
    }

    pub const fn rgba(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self { r, g, b, a }
    }

    pub const fn to_array(self) -> [f32; 4] {
        [self.r, self.g, self.b, self.a]
    }
}

/// Procedural/static background style used by the M0 GPU compositor (extended to video/media in M1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ShaderMotionStyle {
    Solid = 0,
    AuroraWave = 1,
    SanctuaryGlow = 2,
    BroadcastGrid = 3,
    LowerThirdAlpha = 4,
}

/// Transition specification between cues.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TransitionSpec {
    Cut,
    Dissolve { duration_ms: u32 },
}

impl Default for TransitionSpec {
    fn default() -> Self {
        Self::Dissolve { duration_ms: 450 }
    }
}

/// Canonical slide representation (M0 foundation; extended with rich text/vector items in M1).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Slide {
    pub id: SlideId,
    pub label: String,
    pub group: String,
    pub headline: String,
    pub body: String,
    pub primary_color: ColorRgba,
    pub secondary_color: ColorRgba,
    pub accent_color: ColorRgba,
    pub motion_style: ShaderMotionStyle,
    pub transition: TransitionSpec,
}

/// Physical or logical output classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OutputKind {
    Audience,
    Stage,
}

/// Per-output Look defining layer visibility and broadcast keying behavior.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LookConfig {
    pub id: LookId,
    pub name: String,
    pub enabled_layers: Vec<LayerKind>,
    pub lower_third_mode: bool,
    pub transparent_background: bool,
}

impl LookConfig {
    pub fn is_layer_enabled(&self, layer: LayerKind) -> bool {
        self.enabled_layers.contains(&layer)
    }
}

/// Configuration for an individual compositor output window / render target.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutputConfig {
    pub id: OutputId,
    pub name: String,
    pub kind: OutputKind,
    pub monitor_index: Option<usize>,
    pub width: u32,
    pub height: u32,
    pub fullscreen: bool,
    pub look_id: LookId,
}

/// Complete live compositor state synchronized between control surface and sidecar engine.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SceneState {
    pub revision: u64,
    pub blackout: bool,
    pub active_slide: Option<Slide>,
    pub media_layer_active: bool,
    pub slide_layer_active: bool,
    pub overlay_layer_active: bool,
    pub looks: BTreeMap<LookId, LookConfig>,
    pub outputs: Vec<OutputConfig>,
    pub deck: Vec<Slide>,
}

/// Deterministic mutation commands applied to `SceneState`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SceneDiff {
    TriggerSlide {
        slide_id: SlideId,
        transition_override: Option<TransitionSpec>,
    },
    TriggerCustomSlide {
        slide: Slide,
    },
    ClearLayer(LayerKind),
    ClearAll,
    SetBlackout(bool),
    SetOutputLook {
        output_id: OutputId,
        look_id: LookId,
    },
    ToggleOverlayLayer(bool),
}

// Well-known deterministic M0 IDs for out-of-the-box demo sessions.
pub const LOOK_AUDIENCE_FULL: LookId = LookId::from_u128(1);
pub const LOOK_STAGE_CONFIDENCE: LookId = LookId::from_u128(2);
pub const LOOK_BROADCAST_LOWER_THIRD: LookId = LookId::from_u128(3);

pub const OUTPUT_AUDIENCE_MAIN: OutputId = OutputId::from_u128(101);
pub const OUTPUT_STAGE_MAIN: OutputId = OutputId::from_u128(102);

impl SceneState {
    /// Creates a ready-to-run M0 demonstration state with 2 outputs, 3 Looks, and 5 worship test cues.
    pub fn demo_m0(output_count: usize) -> Self {
        let mut looks = BTreeMap::new();
        looks.insert(
            LOOK_AUDIENCE_FULL,
            LookConfig {
                id: LOOK_AUDIENCE_FULL,
                name: "Audience Full".to_string(),
                enabled_layers: LayerKind::ALL.to_vec(),
                lower_third_mode: false,
                transparent_background: false,
            },
        );
        looks.insert(
            LOOK_STAGE_CONFIDENCE,
            LookConfig {
                id: LOOK_STAGE_CONFIDENCE,
                name: "Stage Confidence".to_string(),
                enabled_layers: vec![LayerKind::Slide, LayerKind::MessagesAndTimers],
                lower_third_mode: false,
                transparent_background: false,
            },
        );
        looks.insert(
            LOOK_BROADCAST_LOWER_THIRD,
            LookConfig {
                id: LOOK_BROADCAST_LOWER_THIRD,
                name: "Broadcast Lower Third (Alpha)".to_string(),
                enabled_layers: vec![LayerKind::Slide, LayerKind::PropsAndOverlays],
                lower_third_mode: true,
                transparent_background: true,
            },
        );

        let mut outputs = vec![OutputConfig {
            id: OUTPUT_AUDIENCE_MAIN,
            name: "Audience 1 (Main Sanctuary)".to_string(),
            kind: OutputKind::Audience,
            monitor_index: None,
            width: 1280,
            height: 720,
            fullscreen: false,
            look_id: LOOK_AUDIENCE_FULL,
        }];

        if output_count >= 2 {
            outputs.push(OutputConfig {
                id: OUTPUT_STAGE_MAIN,
                name: "Stage 1 (Confidence Monitor)".to_string(),
                kind: OutputKind::Stage,
                monitor_index: None,
                width: 960,
                height: 540,
                fullscreen: false,
                look_id: LOOK_STAGE_CONFIDENCE,
            });
        }

        let deck = demo_deck();
        let first_slide = deck.first().cloned();

        Self {
            revision: 1,
            blackout: false,
            active_slide: first_slide,
            media_layer_active: true,
            slide_layer_active: true,
            overlay_layer_active: true,
            looks,
            outputs,
            deck,
        }
    }

    /// Applies a `SceneDiff` deterministically and increments `revision`.
    pub fn apply(&mut self, diff: SceneDiff) -> bool {
        match diff {
            SceneDiff::TriggerSlide {
                slide_id,
                transition_override,
            } => {
                if let Some(mut slide) = self.deck.iter().find(|s| s.id == slide_id).cloned() {
                    if let Some(t) = transition_override {
                        slide.transition = t;
                    }
                    self.active_slide = Some(slide);
                    self.media_layer_active = true;
                    self.slide_layer_active = true;
                    self.blackout = false;
                    self.revision += 1;
                    true
                } else {
                    false
                }
            }
            SceneDiff::TriggerCustomSlide { slide } => {
                self.active_slide = Some(slide);
                self.media_layer_active = true;
                self.slide_layer_active = true;
                self.blackout = false;
                self.revision += 1;
                true
            }
            SceneDiff::ClearLayer(layer) => {
                match layer {
                    LayerKind::MediaBackground | LayerKind::VideoInput => {
                        self.media_layer_active = false;
                    }
                    LayerKind::Slide | LayerKind::Announcements => {
                        self.slide_layer_active = false;
                    }
                    LayerKind::PropsAndOverlays
                    | LayerKind::Masks
                    | LayerKind::MessagesAndTimers => {
                        self.overlay_layer_active = false;
                    }
                }
                self.revision += 1;
                true
            }
            SceneDiff::ClearAll => {
                self.media_layer_active = false;
                self.slide_layer_active = false;
                self.overlay_layer_active = false;
                self.revision += 1;
                true
            }
            SceneDiff::SetBlackout(enabled) => {
                self.blackout = enabled;
                self.revision += 1;
                true
            }
            SceneDiff::SetOutputLook { output_id, look_id } => {
                if !self.looks.contains_key(&look_id) {
                    return false;
                }
                if let Some(output) = self.outputs.iter_mut().find(|o| o.id == output_id) {
                    output.look_id = look_id;
                    self.revision += 1;
                    true
                } else {
                    false
                }
            }
            SceneDiff::ToggleOverlayLayer(active) => {
                self.overlay_layer_active = active;
                self.revision += 1;
                true
            }
        }
    }

    pub fn look_for_output(&self, output_id: OutputId) -> Option<&LookConfig> {
        let output = self.outputs.iter().find(|o| o.id == output_id)?;
        self.looks.get(&output.look_id)
    }
}

/// Built-in M0 cue deck demonstrating GPU motion shaders, crossfades, and alpha lower-third modes.
pub fn demo_deck() -> Vec<Slide> {
    vec![
        Slide {
            id: SlideId::from_u128(1001),
            label: "01 — Prelude Aurora".to_string(),
            group: "Pre-Service".to_string(),
            headline: "WELCOME TO WORSHIP".to_string(),
            body: "M0 GPU Compositor — 60 fps Aurora Motion Shader".to_string(),
            primary_color: ColorRgba::rgb(0.06, 0.14, 0.28),
            secondary_color: ColorRgba::rgb(0.08, 0.42, 0.56),
            accent_color: ColorRgba::rgb(0.95, 0.78, 0.35),
            motion_style: ShaderMotionStyle::AuroraWave,
            transition: TransitionSpec::Dissolve { duration_ms: 500 },
        },
        Slide {
            id: SlideId::from_u128(1002),
            label: "02 — Worship Warm Glow".to_string(),
            group: "Chorus 1".to_string(),
            headline: "SANCTUARY WARM MOTION".to_string(),
            body: "Smooth GPU crossfade across Audience & Stage swapchains".to_string(),
            primary_color: ColorRgba::rgb(0.24, 0.08, 0.05),
            secondary_color: ColorRgba::rgb(0.76, 0.32, 0.10),
            accent_color: ColorRgba::rgb(1.0, 0.88, 0.62),
            motion_style: ShaderMotionStyle::SanctuaryGlow,
            transition: TransitionSpec::Dissolve { duration_ms: 600 },
        },
        Slide {
            id: SlideId::from_u128(1003),
            label: "03 — Scripture Indigo".to_string(),
            group: "Scripture".to_string(),
            headline: "PSALM 100:1-5".to_string(),
            body: "Clean high-contrast readability backdrop".to_string(),
            primary_color: ColorRgba::rgb(0.07, 0.09, 0.19),
            secondary_color: ColorRgba::rgb(0.22, 0.18, 0.44),
            accent_color: ColorRgba::rgb(0.68, 0.82, 1.0),
            motion_style: ShaderMotionStyle::AuroraWave,
            transition: TransitionSpec::Dissolve { duration_ms: 350 },
        },
        Slide {
            id: SlideId::from_u128(1004),
            label: "04 — Broadcast Alignment Grid".to_string(),
            group: "Engineering".to_string(),
            headline: "SIGNAL & SAFE-AREA TEST".to_string(),
            body: "16:9 / Action-Safe / Title-Safe calibration pattern".to_string(),
            primary_color: ColorRgba::rgb(0.08, 0.10, 0.12),
            secondary_color: ColorRgba::rgb(0.18, 0.24, 0.30),
            accent_color: ColorRgba::rgb(0.20, 0.90, 0.55),
            motion_style: ShaderMotionStyle::BroadcastGrid,
            transition: TransitionSpec::Cut,
        },
        Slide {
            id: SlideId::from_u128(1005),
            label: "05 — Lower-Third Key Preview".to_string(),
            group: "Broadcast".to_string(),
            headline: "LOWER THIRD ALPHA BAND".to_string(),
            body: "Key/Fill transparent region validation".to_string(),
            primary_color: ColorRgba::rgba(0.05, 0.07, 0.11, 0.0),
            secondary_color: ColorRgba::rgba(0.12, 0.44, 0.88, 0.92),
            accent_color: ColorRgba::rgb(1.0, 1.0, 1.0),
            motion_style: ShaderMotionStyle::LowerThirdAlpha,
            transition: TransitionSpec::Dissolve { duration_ms: 300 },
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn demo_scene_initializes_two_outputs_and_looks() {
        let state = SceneState::demo_m0(2);
        assert_eq!(state.outputs.len(), 2);
        assert_eq!(state.looks.len(), 3);
        assert!(state.active_slide.is_some());
        assert!(!state.blackout);
    }

    #[test]
    fn apply_diffs_updates_state_and_revision_deterministically() {
        let mut state = SceneState::demo_m0(2);
        let initial_rev = state.revision;

        assert!(state.apply(SceneDiff::SetBlackout(true)));
        assert!(state.blackout);
        assert_eq!(state.revision, initial_rev + 1);

        let target_id = state.deck[1].id;
        assert!(state.apply(SceneDiff::TriggerSlide {
            slide_id: target_id,
            transition_override: Some(TransitionSpec::Cut),
        }));
        assert!(!state.blackout);
        assert_eq!(state.active_slide.as_ref().map(|s| s.id), Some(target_id));
        assert_eq!(
            state.active_slide.as_ref().map(|s| s.transition),
            Some(TransitionSpec::Cut)
        );

        assert!(state.apply(SceneDiff::ClearLayer(LayerKind::MediaBackground)));
        assert!(!state.media_layer_active);
        assert!(state.slide_layer_active);

        assert!(state.apply(SceneDiff::SetOutputLook {
            output_id: OUTPUT_AUDIENCE_MAIN,
            look_id: LOOK_BROADCAST_LOWER_THIRD,
        }));
        assert!(
            state
                .look_for_output(OUTPUT_AUDIENCE_MAIN)
                .map(|l| l.lower_third_mode)
                .unwrap_or(false)
        );
    }

    #[test]
    fn invalid_slide_or_look_returns_false_without_bumping_revision() {
        let mut state = SceneState::demo_m0(1);
        let rev = state.revision;
        assert!(!state.apply(SceneDiff::TriggerSlide {
            slide_id: SlideId::from_u128(999_999),
            transition_override: None,
        }));
        assert!(!state.apply(SceneDiff::SetOutputLook {
            output_id: OUTPUT_AUDIENCE_MAIN,
            look_id: LookId::from_u128(999_999),
        }));
        assert_eq!(state.revision, rev);
    }
}
