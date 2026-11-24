//! Canonical, resolution-independent scene graph for OpenPresenter.
//!
//! Reference virtual canvas coordinate space is `1920.0 × 1080.0`. Both the native GPU
//! compositor (`openpresenter-engine`) and the operator editor (`apps/desktop`) share these
//! exact structures so WYSIWYG previews match sanctuary displays pixel-for-pixel.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use uuid::Uuid;

pub const VIRTUAL_CANVAS_WIDTH: f32 = 1920.0;
pub const VIRTUAL_CANVAS_HEIGHT: f32 = 1080.0;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct LookId(pub Uuid);

impl LookId {
    pub const fn from_u128(v: u128) -> Self {
        Self(Uuid::from_u128(v))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct SlideId(pub Uuid);

impl SlideId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
    pub const fn from_u128(v: u128) -> Self {
        Self(Uuid::from_u128(v))
    }
}

impl Default for SlideId {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ElementId(pub Uuid);

impl ElementId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
    pub const fn from_u128(v: u128) -> Self {
        Self(Uuid::from_u128(v))
    }
}

impl Default for ElementId {
    fn default() -> Self {
        Self::new()
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

/// Blend modes supported by the GPU layer compositor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum BlendMode {
    #[default]
    Normal = 0,
    Multiply = 1,
    Screen = 2,
    Additive = 3,
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

    pub fn to_u8_rgba(self) -> [u8; 4] {
        [
            (self.r.clamp(0.0, 1.0) * 255.0).round() as u8,
            (self.g.clamp(0.0, 1.0) * 255.0).round() as u8,
            (self.b.clamp(0.0, 1.0) * 255.0).round() as u8,
            (self.a.clamp(0.0, 1.0) * 255.0).round() as u8,
        ]
    }
}

/// Rectangle in `1920×1080` virtual canvas coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct RectF {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl RectF {
    pub const fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum TextAlign {
    Left,
    #[default]
    Center,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum VerticalAlign {
    Top,
    #[default]
    Middle,
    Bottom,
}

/// Broadcast typography style for slide text elements.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TextStyle {
    pub font_family: String,
    pub font_size: f32,
    pub bold: bool,
    pub italic: bool,
    pub line_height: f32,
    pub color: ColorRgba,
    pub stroke_color: ColorRgba,
    pub stroke_width: f32,
    pub shadow_color: ColorRgba,
    pub shadow_offset_x: f32,
    pub shadow_offset_y: f32,
    pub shadow_blur: f32,
    pub backdrop_color: ColorRgba,
    pub backdrop_radius: f32,
    pub backdrop_padding: f32,
    pub h_align: TextAlign,
    pub v_align: VerticalAlign,
    pub auto_shrink: bool,
}

impl Default for TextStyle {
    fn default() -> Self {
        Self {
            font_family: "sans-serif".to_string(),
            font_size: 86.0,
            bold: true,
            italic: false,
            line_height: 1.22,
            color: ColorRgba::WHITE,
            stroke_color: ColorRgba::rgba(0.0, 0.0, 0.0, 0.92),
            stroke_width: 4.0,
            shadow_color: ColorRgba::rgba(0.0, 0.0, 0.0, 0.72),
            shadow_offset_x: 0.0,
            shadow_offset_y: 6.0,
            shadow_blur: 10.0,
            backdrop_color: ColorRgba::TRANSPARENT,
            backdrop_radius: 18.0,
            backdrop_padding: 24.0,
            h_align: TextAlign::Center,
            v_align: VerticalAlign::Middle,
            auto_shrink: true,
        }
    }
}

/// Individual visual element inside a slide.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SlideElementKind {
    TextBox {
        text: String,
        style: TextStyle,
    },
    ShapeRect {
        fill: ColorRgba,
        stroke: ColorRgba,
        stroke_width: f32,
        corner_radius: f32,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SlideElement {
    pub id: ElementId,
    pub name: String,
    pub bounds: RectF,
    pub opacity: f32,
    pub z_index: i32,
    pub kind: SlideElementKind,
}

/// Procedural or media motion background style.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ShaderMotionStyle {
    Solid = 0,
    AuroraWave = 1,
    SanctuaryGlow = 2,
    BroadcastGrid = 3,
    LowerThirdAlpha = 4,
}

/// GPU-composited mask styles applied to Layer 5 (`Masks`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum MaskStyle {
    #[default]
    None = 0,
    SoftVignette = 1,
    RoundedFrame = 2,
    LowerThirdBand = 3,
}

/// GPU transition specifications between cues.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TransitionSpec {
    Cut,
    Dissolve { duration_ms: u32 },
    PushLeft { duration_ms: u32 },
    WipeRight { duration_ms: u32 },
}

impl TransitionSpec {
    pub fn duration_ms(self) -> u32 {
        match self {
            Self::Cut => 0,
            Self::Dissolve { duration_ms }
            | Self::PushLeft { duration_ms }
            | Self::WipeRight { duration_ms } => duration_ms,
        }
    }

    pub fn kind_code(self) -> u32 {
        match self {
            Self::Cut | Self::Dissolve { .. } => 0,
            Self::PushLeft { .. } => 1,
            Self::WipeRight { .. } => 2,
        }
    }
}

impl Default for TransitionSpec {
    fn default() -> Self {
        Self::Dissolve { duration_ms: 450 }
    }
}

/// Canonical slide representation with vector elements and optional ChordPro notes for Stage displays.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Slide {
    pub id: SlideId,
    pub label: String,
    pub group: String,
    pub headline: String,
    pub body: String,
    pub chords_line: Option<String>,
    pub ccli_number: Option<String>,
    pub media_path: Option<String>,
    pub primary_color: ColorRgba,
    pub secondary_color: ColorRgba,
    pub accent_color: ColorRgba,
    pub motion_style: ShaderMotionStyle,
    pub transition: TransitionSpec,
    pub elements: Vec<SlideElement>,
}

impl Slide {
    /// Returns the combined lyric/scripture text for stage confidence & search indexing.
    pub fn plain_text(&self) -> String {
        let mut parts = Vec::new();
        for el in &self.elements {
            if let SlideElementKind::TextBox { text, .. } = &el.kind
                && !text.trim().is_empty()
            {
                parts.push(text.trim().to_string());
            }
        }
        if parts.is_empty() {
            format!("{}\n{}", self.headline, self.body)
        } else {
            parts.join("\n")
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OutputKind {
    Audience,
    Stage,
}

/// Per-output Look defining layer visibility, blend modes, and broadcast keying behavior.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LookConfig {
    pub id: LookId,
    pub name: String,
    pub enabled_layers: Vec<LayerKind>,
    pub lower_third_mode: bool,
    pub transparent_background: bool,
    pub slide_blend_mode: BlendMode,
}

impl LookConfig {
    pub fn is_layer_enabled(&self, layer: LayerKind) -> bool {
        self.enabled_layers.contains(&layer)
    }
}

/// Customizable Stage Display layout configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StageLayoutConfig {
    pub show_current_slide: bool,
    pub show_next_slide: bool,
    pub show_chords: bool,
    pub show_clock: bool,
    pub show_timer: bool,
    pub show_stage_message: bool,
}

impl Default for StageLayoutConfig {
    fn default() -> Self {
        Self {
            show_current_slide: true,
            show_next_slide: true,
            show_chords: true,
            show_clock: true,
            show_timer: true,
            show_stage_message: true,
        }
    }
}

/// Pixel rectangle relative to the assigned physical monitor's top-left origin
/// (or virtual desktop origin when `monitor_index` is `None`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct MonitorRegion {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

/// How an output screen is positioned on a physical OS display.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum OutputPlacement {
    /// Standard resizable window.
    #[default]
    Windowed,
    /// Borderless fullscreen occupying the entire selected physical monitor.
    Fullscreen,
    /// Independent borderless sub-screen occupying a pixel region of a physical monitor
    /// (e.g., split-screen halves, quadrants, or LED wall slices).
    Region(MonitorRegion),
}

/// Divides a physical monitor of size `monitor_width × monitor_height` into `cols × rows`
/// pixel-accurate, non-overlapping [`MonitorRegion`] tiles (absorbing any remainder pixels on
/// the final column/row so there are zero 1px gaps).
pub fn split_monitor_regions(
    monitor_width: u32,
    monitor_height: u32,
    cols: u32,
    rows: u32,
) -> Vec<MonitorRegion> {
    if monitor_width == 0 || monitor_height == 0 || cols == 0 || rows == 0 {
        return Vec::new();
    }
    let cols = cols.min(8);
    let rows = rows.min(8);
    let base_w = (monitor_width / cols).max(1);
    let base_h = (monitor_height / rows).max(1);
    let mut regions = Vec::with_capacity((cols * rows) as usize);
    for r in 0..rows {
        let y = r * base_h;
        let h = if r + 1 == rows {
            monitor_height.saturating_sub(y).max(1)
        } else {
            base_h
        };
        for c in 0..cols {
            let x = c * base_w;
            let w = if c + 1 == cols {
                monitor_width.saturating_sub(x).max(1)
            } else {
                base_w
            };
            regions.push(MonitorRegion {
                x: x as i32,
                y: y as i32,
                width: w,
                height: h,
            });
        }
    }
    regions
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
    #[serde(default)]
    pub placement: OutputPlacement,
    pub look_id: LookId,
    pub stage_layout: Option<StageLayoutConfig>,
}

impl OutputConfig {
    /// Resolves effective placement, keeping legacy `fullscreen: true` flags consistent.
    pub fn effective_placement(&self) -> OutputPlacement {
        match self.placement {
            OutputPlacement::Windowed if self.fullscreen => OutputPlacement::Fullscreen,
            other => other,
        }
    }
}

/// Complete live compositor state synchronized between control surface and sidecar engine.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SceneState {
    pub revision: u64,
    pub blackout: bool,
    pub active_slide: Option<Slide>,
    pub next_slide_preview: Option<Slide>,
    pub video_input_active: bool,
    pub media_layer_active: bool,
    pub slide_layer_active: bool,
    pub announcements_layer_active: bool,
    pub overlay_layer_active: bool,
    pub mask_style: MaskStyle,
    pub announcement_text: Option<String>,
    pub stage_message: Option<String>,
    pub timer_readout: Option<String>,
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
    UpsertSlide {
        slide: Slide,
    },
    UpdateElement {
        slide_id: SlideId,
        element: SlideElement,
    },
    ClearLayer(LayerKind),
    ClearAll,
    SetBlackout(bool),
    SetOutputLook {
        output_id: OutputId,
        look_id: LookId,
    },
    UpsertOutput(OutputConfig),
    RemoveOutput(OutputId),
    SetOutputStageLayout {
        output_id: OutputId,
        layout: StageLayoutConfig,
    },
    ToggleOverlayLayer(bool),
    SetMask(MaskStyle),
    SetAnnouncement(Option<String>),
    SetStageMessage(Option<String>),
    SetTimerReadout(Option<String>),
    ReplaceDeck(Vec<Slide>),
}

pub const LOOK_AUDIENCE_FULL: LookId = LookId::from_u128(1);
pub const LOOK_STAGE_CONFIDENCE: LookId = LookId::from_u128(2);
pub const LOOK_BROADCAST_LOWER_THIRD: LookId = LookId::from_u128(3);

pub const OUTPUT_AUDIENCE_MAIN: OutputId = OutputId::from_u128(101);
pub const OUTPUT_STAGE_MAIN: OutputId = OutputId::from_u128(102);

impl SceneState {
    /// Creates a ready-to-run demonstration state with 2 outputs, 3 Looks, and rich worship slides.
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
                slide_blend_mode: BlendMode::Normal,
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
                slide_blend_mode: BlendMode::Normal,
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
                slide_blend_mode: BlendMode::Normal,
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
            placement: OutputPlacement::Windowed,
            look_id: LOOK_AUDIENCE_FULL,
            stage_layout: None,
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
                placement: OutputPlacement::Windowed,
                look_id: LOOK_STAGE_CONFIDENCE,
                stage_layout: Some(StageLayoutConfig::default()),
            });
        }

        let deck = demo_deck();
        let first_slide = deck.first().cloned();
        let next_slide = deck.get(1).cloned();

        Self {
            revision: 1,
            blackout: false,
            active_slide: first_slide,
            next_slide_preview: next_slide,
            video_input_active: false,
            media_layer_active: true,
            slide_layer_active: true,
            announcements_layer_active: false,
            overlay_layer_active: true,
            mask_style: MaskStyle::None,
            announcement_text: None,
            stage_message: None,
            timer_readout: Some("05:00".to_string()),
            looks,
            outputs,
            deck,
        }
    }

    pub fn apply(&mut self, diff: SceneDiff) -> bool {
        match diff {
            SceneDiff::TriggerSlide {
                slide_id,
                transition_override,
            } => {
                if let Some(idx) = self.deck.iter().position(|s| s.id == slide_id) {
                    let mut slide = self.deck[idx].clone();
                    if let Some(t) = transition_override {
                        slide.transition = t;
                    }
                    self.active_slide = Some(slide);
                    self.next_slide_preview = self.deck.get(idx + 1).cloned();
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
            SceneDiff::UpsertSlide { slide } => {
                if let Some(existing) = self.deck.iter_mut().find(|s| s.id == slide.id) {
                    *existing = slide.clone();
                } else {
                    self.deck.push(slide.clone());
                }
                if self.active_slide.as_ref().map(|s| s.id) == Some(slide.id) {
                    self.active_slide = Some(slide);
                }
                self.revision += 1;
                true
            }
            SceneDiff::UpdateElement { slide_id, element } => {
                let mut updated = false;
                if let Some(slide) = self.deck.iter_mut().find(|s| s.id == slide_id) {
                    if let Some(el) = slide.elements.iter_mut().find(|e| e.id == element.id) {
                        *el = element.clone();
                    } else {
                        slide.elements.push(element.clone());
                    }
                    if let SlideElementKind::TextBox { text, .. } = &element.kind
                        && slide.elements.first().map(|e| e.id) == Some(element.id)
                    {
                        slide.headline = text.lines().next().unwrap_or_default().to_string();
                    }
                    if self.active_slide.as_ref().map(|s| s.id) == Some(slide_id) {
                        self.active_slide = Some(slide.clone());
                    }
                    updated = true;
                }
                if updated {
                    self.revision += 1;
                }
                updated
            }
            SceneDiff::ClearLayer(layer) => {
                match layer {
                    LayerKind::VideoInput => self.video_input_active = false,
                    LayerKind::MediaBackground => self.media_layer_active = false,
                    LayerKind::Slide => self.slide_layer_active = false,
                    LayerKind::Announcements => {
                        self.announcements_layer_active = false;
                        self.announcement_text = None;
                    }
                    LayerKind::PropsAndOverlays => self.overlay_layer_active = false,
                    LayerKind::Masks => self.mask_style = MaskStyle::None,
                    LayerKind::MessagesAndTimers => {
                        self.stage_message = None;
                        self.timer_readout = None;
                    }
                }
                self.revision += 1;
                true
            }
            SceneDiff::ClearAll => {
                self.video_input_active = false;
                self.media_layer_active = false;
                self.slide_layer_active = false;
                self.announcements_layer_active = false;
                self.overlay_layer_active = false;
                self.announcement_text = None;
                self.mask_style = MaskStyle::None;
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
            SceneDiff::UpsertOutput(mut cfg) => {
                if cfg.name.trim().is_empty() || cfg.width == 0 || cfg.height == 0 {
                    return false;
                }
                if let OutputPlacement::Region(r) = cfg.placement {
                    if r.width == 0 || r.height == 0 {
                        return false;
                    }
                    cfg.width = r.width;
                    cfg.height = r.height;
                    cfg.fullscreen = false;
                } else if cfg.placement == OutputPlacement::Fullscreen {
                    cfg.fullscreen = true;
                } else if cfg.fullscreen {
                    cfg.placement = OutputPlacement::Fullscreen;
                }
                if !self.looks.contains_key(&cfg.look_id) {
                    cfg.look_id = match cfg.kind {
                        OutputKind::Audience => LOOK_AUDIENCE_FULL,
                        OutputKind::Stage => LOOK_STAGE_CONFIDENCE,
                    };
                }
                if cfg.kind == OutputKind::Stage && cfg.stage_layout.is_none() {
                    cfg.stage_layout = Some(StageLayoutConfig::default());
                }
                if let Some(existing) = self.outputs.iter_mut().find(|o| o.id == cfg.id) {
                    *existing = cfg;
                } else {
                    self.outputs.push(cfg);
                }
                self.revision += 1;
                true
            }
            SceneDiff::RemoveOutput(output_id) => {
                let before = self.outputs.len();
                self.outputs.retain(|o| o.id != output_id);
                if self.outputs.len() < before {
                    self.revision += 1;
                    true
                } else {
                    false
                }
            }
            SceneDiff::SetOutputStageLayout { output_id, layout } => {
                if let Some(output) = self.outputs.iter_mut().find(|o| o.id == output_id) {
                    output.stage_layout = Some(layout);
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
            SceneDiff::SetMask(style) => {
                self.mask_style = style;
                self.revision += 1;
                true
            }
            SceneDiff::SetAnnouncement(text) => {
                self.announcements_layer_active = text.is_some();
                self.announcement_text = text;
                self.revision += 1;
                true
            }
            SceneDiff::SetStageMessage(msg) => {
                self.stage_message = msg;
                self.revision += 1;
                true
            }
            SceneDiff::SetTimerReadout(readout) => {
                self.timer_readout = readout;
                self.revision += 1;
                true
            }
            SceneDiff::ReplaceDeck(deck) => {
                self.deck = deck;
                self.active_slide = self.deck.first().cloned();
                self.next_slide_preview = self.deck.get(1).cloned();
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

fn default_lyric_elements(base_id: u128, primary_text: &str, sub_text: Option<&str>) -> Vec<SlideElement> {
    let mut els = vec![SlideElement {
        id: ElementId::from_u128(base_id),
        name: "Primary Lyrics".to_string(),
        bounds: RectF::new(160.0, 250.0, 1600.0, 480.0),
        opacity: 1.0,
        z_index: 10,
        kind: SlideElementKind::TextBox {
            text: primary_text.to_string(),
            style: TextStyle::default(),
        },
    }];

    if let Some(sub) = sub_text {
        els.push(SlideElement {
            id: ElementId::from_u128(base_id + 1),
            name: "Reference / Subtitle".to_string(),
            bounds: RectF::new(260.0, 770.0, 1400.0, 140.0),
            opacity: 0.92,
            z_index: 11,
            kind: SlideElementKind::TextBox {
                text: sub.to_string(),
                style: TextStyle {
                    font_size: 46.0,
                    bold: false,
                    stroke_width: 2.5,
                    color: ColorRgba::rgba(0.90, 0.94, 1.0, 0.95),
                    ..TextStyle::default()
                },
            },
        });
    }
    els
}

/// Built-in demonstration deck with real worship lyrics, Scripture, chords, and broadcast cues.
pub fn demo_deck() -> Vec<Slide> {
    vec![
        Slide {
            id: SlideId::from_u128(1001),
            label: "01 — Prelude Welcome".to_string(),
            group: "Pre-Service".to_string(),
            headline: "WELCOME TO WORSHIP".to_string(),
            body: "Sunday Morning Gathering".to_string(),
            chords_line: None,
            ccli_number: None,
            media_path: None,
            primary_color: ColorRgba::rgb(0.06, 0.14, 0.28),
            secondary_color: ColorRgba::rgb(0.08, 0.42, 0.56),
            accent_color: ColorRgba::rgb(0.95, 0.78, 0.35),
            motion_style: ShaderMotionStyle::AuroraWave,
            transition: TransitionSpec::Dissolve { duration_ms: 500 },
            elements: default_lyric_elements(
                5001,
                "WELCOME TO WORSHIP",
                Some("Sunday Morning Gathering • OpenPresenter Native GPU"),
            ),
        },
        Slide {
            id: SlideId::from_u128(1002),
            label: "02 — Amazing Grace (V1)".to_string(),
            group: "Verse 1".to_string(),
            headline: "Amazing grace! How sweet the sound".to_string(),
            body: "That saved a wretch like me".to_string(),
            chords_line: Some(
                "[G]Amazing [C]grace! How [G]sweet the sound / That saved a [D7]wretch like me"
                    .to_string(),
            ),
            ccli_number: Some("22025".to_string()),
            media_path: None,
            primary_color: ColorRgba::rgb(0.24, 0.08, 0.05),
            secondary_color: ColorRgba::rgb(0.76, 0.32, 0.10),
            accent_color: ColorRgba::rgb(1.0, 0.88, 0.62),
            motion_style: ShaderMotionStyle::SanctuaryGlow,
            transition: TransitionSpec::Dissolve { duration_ms: 600 },
            elements: default_lyric_elements(
                5010,
                "Amazing grace! How sweet the sound\nThat saved a wretch like me\nI once was lost, but now am found\nWas blind, but now I see",
                Some("CCLI #22025 • Public Domain"),
            ),
        },
        Slide {
            id: SlideId::from_u128(1003),
            label: "03 — Psalm 100:1-2 (KJV)".to_string(),
            group: "Scripture".to_string(),
            headline: "Make a joyful noise unto the LORD".to_string(),
            body: "Psalm 100:1-2 (KJV)".to_string(),
            chords_line: None,
            ccli_number: None,
            media_path: None,
            primary_color: ColorRgba::rgb(0.07, 0.09, 0.19),
            secondary_color: ColorRgba::rgb(0.22, 0.18, 0.44),
            accent_color: ColorRgba::rgb(0.68, 0.82, 1.0),
            motion_style: ShaderMotionStyle::AuroraWave,
            transition: TransitionSpec::PushLeft { duration_ms: 450 },
            elements: default_lyric_elements(
                5020,
                "Make a joyful noise unto the LORD, all ye lands.\nServe the LORD with gladness:\ncome before his presence with singing.",
                Some("PSALM 100:1-2 (KJV)"),
            ),
        },
        Slide {
            id: SlideId::from_u128(1004),
            label: "04 — Broadcast Alignment Grid".to_string(),
            group: "Engineering".to_string(),
            headline: "SIGNAL & SAFE-AREA TEST".to_string(),
            body: "16:9 / Action-Safe / Title-Safe".to_string(),
            chords_line: None,
            ccli_number: None,
            media_path: None,
            primary_color: ColorRgba::rgb(0.08, 0.10, 0.12),
            secondary_color: ColorRgba::rgb(0.18, 0.24, 0.30),
            accent_color: ColorRgba::rgb(0.20, 0.90, 0.55),
            motion_style: ShaderMotionStyle::BroadcastGrid,
            transition: TransitionSpec::Cut,
            elements: default_lyric_elements(
                5030,
                "BROADCAST SAFE-AREA TEST",
                Some("1920×1080 Reference • 60 fps GPU Compositor"),
            ),
        },
        Slide {
            id: SlideId::from_u128(1005),
            label: "05 — Lower-Third Key".to_string(),
            group: "Broadcast".to_string(),
            headline: "Pastor David Miller — Lead Pastor".to_string(),
            body: "Lower-Third Alpha Key/Fill Band".to_string(),
            chords_line: None,
            ccli_number: None,
            media_path: None,
            primary_color: ColorRgba::rgba(0.05, 0.07, 0.11, 0.0),
            secondary_color: ColorRgba::rgba(0.12, 0.44, 0.88, 0.92),
            accent_color: ColorRgba::rgb(1.0, 1.0, 1.0),
            motion_style: ShaderMotionStyle::LowerThirdAlpha,
            transition: TransitionSpec::WipeRight { duration_ms: 350 },
            elements: vec![SlideElement {
                id: ElementId::from_u128(5040),
                name: "Lower Third Pill".to_string(),
                bounds: RectF::new(140.0, 810.0, 1640.0, 170.0),
                opacity: 1.0,
                z_index: 12,
                kind: SlideElementKind::TextBox {
                    text: "Make a joyful noise unto the LORD, all ye lands".to_string(),
                    style: TextStyle {
                        font_size: 56.0,
                        backdrop_color: ColorRgba::rgba(0.06, 0.10, 0.18, 0.86),
                        backdrop_radius: 20.0,
                        backdrop_padding: 28.0,
                        h_align: TextAlign::Left,
                        ..TextStyle::default()
                    },
                },
            }],
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
        assert!(state.next_slide_preview.is_some());
        assert!(!state.blackout);
    }

    #[test]
    fn apply_diffs_updates_state_elements_and_masks_deterministically() {
        let mut state = SceneState::demo_m0(2);
        let initial_rev = state.revision;

        assert!(state.apply(SceneDiff::SetBlackout(true)));
        assert!(state.blackout);
        assert_eq!(state.revision, initial_rev + 1);

        let target_id = state.deck[1].id;
        assert!(state.apply(SceneDiff::TriggerSlide {
            slide_id: target_id,
            transition_override: Some(TransitionSpec::PushLeft { duration_ms: 300 }),
        }));
        assert!(!state.blackout);
        assert_eq!(state.active_slide.as_ref().map(|s| s.id), Some(target_id));

        assert!(state.apply(SceneDiff::SetMask(MaskStyle::RoundedFrame)));
        assert_eq!(state.mask_style, MaskStyle::RoundedFrame);

        let mut el = state.deck[1].elements[0].clone();
        if let SlideElementKind::TextBox { ref mut text, .. } = el.kind {
            *text = "Updated Lyric Line".to_string();
        }
        assert!(state.apply(SceneDiff::UpdateElement {
            slide_id: target_id,
            element: el,
        }));
        assert!(
            state
                .active_slide
                .as_ref()
                .expect("active slide")
                .plain_text()
                .contains("Updated Lyric Line")
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

    #[test]
    fn covers_all_layers_transitions_ids_and_diffs() {
        let _ = OutputId::default();
        let _ = SlideId::default();
        let _ = ElementId::default();
        assert_eq!(ColorRgba::WHITE.to_array(), [1.0, 1.0, 1.0, 1.0]);
        assert_eq!(ColorRgba::rgba(1.0, 0.5, 0.0, 0.5).to_u8_rgba(), [255, 128, 0, 128]);
        assert_eq!(TransitionSpec::Cut.duration_ms(), 0);
        assert_eq!(TransitionSpec::Cut.kind_code(), 0);
        assert_eq!(TransitionSpec::Dissolve { duration_ms: 250 }.duration_ms(), 250);
        assert_eq!(TransitionSpec::PushLeft { duration_ms: 300 }.kind_code(), 1);
        assert_eq!(TransitionSpec::WipeRight { duration_ms: 350 }.kind_code(), 2);

        let mut state = SceneState::demo_m0(2);
        let mut custom = state.deck[0].clone();
        custom.id = SlideId::new();
        custom.elements.clear();
        assert!(custom.plain_text().contains("WELCOME"));

        assert!(state.apply(SceneDiff::TriggerCustomSlide { slide: custom.clone() }));
        assert!(state.apply(SceneDiff::UpsertSlide { slide: custom.clone() }));
        // Update existing slide via UpsertSlide
        custom.label = "Updated Label".into();
        assert!(state.apply(SceneDiff::UpsertSlide { slide: custom.clone() }));

        // Add new element via UpdateElement
        let new_el = SlideElement {
            id: ElementId::new(),
            name: "Shape".into(),
            bounds: RectF::new(10.0, 10.0, 100.0, 50.0),
            opacity: 1.0,
            z_index: 1,
            kind: SlideElementKind::ShapeRect {
                fill: ColorRgba::BLACK,
                stroke: ColorRgba::WHITE,
                stroke_width: 2.0,
                corner_radius: 8.0,
            },
        };
        assert!(state.apply(SceneDiff::UpdateElement {
            slide_id: custom.id,
            element: new_el,
        }));

        assert!(state.apply(SceneDiff::ToggleOverlayLayer(false)));
        assert!(state.apply(SceneDiff::SetAnnouncement(Some("Welcome guests".into()))));
        assert!(state.apply(SceneDiff::SetStageMessage(Some("Wrap up".into()))));
        assert!(state.apply(SceneDiff::SetTimerReadout(Some("02:00".into()))));
        for layer in LayerKind::ALL {
            assert!(state.apply(SceneDiff::ClearLayer(layer)));
        }
        assert!(state.apply(SceneDiff::ClearAll));
        assert!(state.apply(SceneDiff::ReplaceDeck(demo_deck())));
        assert!(state.look_for_output(OutputId::new()).is_none());
    }

    #[test]
    fn manages_dynamic_outputs_and_split_monitor_regions() {
        let regions = split_monitor_regions(3840, 2160, 2, 2);
        assert_eq!(regions.len(), 4);
        assert_eq!(regions[0], MonitorRegion { x: 0, y: 0, width: 1920, height: 1080 });
        assert_eq!(regions[3], MonitorRegion { x: 1920, y: 1080, width: 1920, height: 1080 });

        // Remainder pixels on odd resolution absorbed cleanly on last column/row.
        let odd = split_monitor_regions(1921, 1081, 3, 2);
        assert_eq!(odd.len(), 6);
        assert_eq!(odd[2].x as u32 + odd[2].width, 1921);
        assert_eq!(odd[5].y as u32 + odd[5].height, 1081);
        assert!(split_monitor_regions(0, 1080, 2, 1).is_empty());
        assert!(split_monitor_regions(1920, 1080, 0, 1).is_empty());

        let mut state = SceneState::demo_m0(1);
        let stage_id = OutputId::new();
        let region_out = OutputConfig {
            id: stage_id,
            name: "Split Monitor Right (Stage)".into(),
            kind: OutputKind::Stage,
            monitor_index: Some(1),
            width: 960,
            height: 1080,
            fullscreen: false,
            placement: OutputPlacement::Region(MonitorRegion {
                x: 960,
                y: 0,
                width: 960,
                height: 1080,
            }),
            look_id: LookId::from_u128(9999), // unknown look falls back to Stage default
            stage_layout: None,
        };
        assert!(state.apply(SceneDiff::UpsertOutput(region_out)));
        let stored = state.outputs.iter().find(|o| o.id == stage_id).expect("stored");
        assert_eq!(stored.look_id, LOOK_STAGE_CONFIDENCE);
        assert!(stored.stage_layout.is_some());
        assert_eq!(
            stored.effective_placement(),
            OutputPlacement::Region(MonitorRegion { x: 960, y: 0, width: 960, height: 1080 })
        );

        // Update existing output to Fullscreen + update StageLayout
        let mut updated = stored.clone();
        updated.placement = OutputPlacement::Fullscreen;
        assert!(state.apply(SceneDiff::UpsertOutput(updated)));
        assert!(state.outputs.iter().find(|o| o.id == stage_id).expect("updated").fullscreen);

        let mut legacy_fs = state.outputs[0].clone();
        legacy_fs.placement = OutputPlacement::Windowed;
        legacy_fs.fullscreen = true;
        assert_eq!(legacy_fs.effective_placement(), OutputPlacement::Fullscreen);
        assert!(state.apply(SceneDiff::UpsertOutput(legacy_fs)));

        assert!(state.apply(SceneDiff::SetOutputStageLayout {
            output_id: stage_id,
            layout: StageLayoutConfig {
                show_chords: false,
                ..StageLayoutConfig::default()
            },
        }));
        assert!(!state.apply(SceneDiff::SetOutputStageLayout {
            output_id: OutputId::new(),
            layout: StageLayoutConfig::default(),
        }));

        // Failure paths: empty name, zero dimensions, zero region, removing unknown output
        let bad_name = OutputConfig {
            id: OutputId::new(),
            name: "   ".into(),
            kind: OutputKind::Audience,
            monitor_index: None,
            width: 1280,
            height: 720,
            fullscreen: false,
            placement: OutputPlacement::Windowed,
            look_id: LOOK_AUDIENCE_FULL,
            stage_layout: None,
        };
        assert!(!state.apply(SceneDiff::UpsertOutput(bad_name)));
        let bad_region = OutputConfig {
            id: OutputId::new(),
            name: "Bad Region".into(),
            kind: OutputKind::Audience,
            monitor_index: Some(0),
            width: 1280,
            height: 720,
            fullscreen: false,
            placement: OutputPlacement::Region(MonitorRegion { x: 0, y: 0, width: 0, height: 500 }),
            look_id: LOOK_AUDIENCE_FULL,
            stage_layout: None,
        };
        assert!(!state.apply(SceneDiff::UpsertOutput(bad_region)));
        assert!(!state.apply(SceneDiff::RemoveOutput(OutputId::new())));

        // Remove output cleanly
        assert!(state.apply(SceneDiff::RemoveOutput(stage_id)));
        assert!(state.outputs.iter().all(|o| o.id != stage_id));
    }
}
