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
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
    pub const fn from_u128(v: u128) -> Self {
        Self(Uuid::from_u128(v))
    }
}

impl Default for LookId {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct TemplateId(pub Uuid);

impl TemplateId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
    pub const fn from_u128(v: u128) -> Self {
        Self(Uuid::from_u128(v))
    }
}

impl Default for TemplateId {
    fn default() -> Self {
        Self::new()
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

/// How local image/video media scales inside the `1920×1080` virtual output canvas.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum MediaFitMode {
    /// Scale proportionally to cover the entire canvas (crop edges if aspect differs).
    #[default]
    Fill = 0,
    /// Scale proportionally to fit completely inside the canvas (letterbox/pillarbox).
    Contain = 1,
    /// Stretch to fill the exact canvas dimensions.
    Stretch = 2,
}

/// Live playback transport state for active motion/video media.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MediaTransportState {
    pub playing: bool,
    pub looping: bool,
    pub muted: bool,
    pub volume: f32,
    pub fit_mode: MediaFitMode,
    pub position_secs: f32,
    pub duration_secs: f32,
}

impl Default for MediaTransportState {
    fn default() -> Self {
        Self {
            playing: true,
            looping: true,
            muted: true,
            volume: 1.0,
            fit_mode: MediaFitMode::Fill,
            position_secs: 0.0,
            duration_secs: 0.0,
        }
    }
}

/// Active audio bus playback state (walk-in music, tracks, audio cues).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AudioPlaybackState {
    pub title: String,
    pub media_path: String,
    pub playing: bool,
    pub looping: bool,
    pub volume: f32,
}

/// Non-destructive song arrangement specifying the playback order of slide groups
/// (e.g. `["Verse 1", "Chorus", "Verse 2", "Chorus", "Bridge", "Chorus"]`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Arrangement {
    pub id: String,
    pub name: String,
    pub group_order: Vec<String>,
}

/// Expands a master slide list according to an optional [`Arrangement`], preserving
/// slide content while allowing groups (e.g. Chorus) to repeat cleanly without duplicating data.
pub fn expand_deck_by_arrangement(master: &[Slide], arrangement: Option<&Arrangement>) -> Vec<Slide> {
    let Some(arr) = arrangement else {
        return master.to_vec();
    };
    if arr.group_order.is_empty() {
        return master.to_vec();
    }
    let mut expanded = Vec::new();
    for (occ_idx, group_name) in arr.group_order.iter().enumerate() {
        let matches: Vec<&Slide> = master
            .iter()
            .filter(|s| s.group.eq_ignore_ascii_case(group_name))
            .collect();
        for (slide_idx, orig) in matches.into_iter().enumerate() {
            let mut copy = orig.clone();
            // Ensure repeated occurrences of the same group have distinct, deterministic SlideIds
            // so the UI and cue deck track which chorus occurrence is live.
            if occ_idx > 0 {
                let raw = orig.id.0.as_u128()
                    ^ ((occ_idx as u128).wrapping_mul(0x9E37_79B9_7F4A_7C15))
                    ^ (slide_idx as u128);
                copy.id = SlideId::from_u128(raw);
            }
            expanded.push(copy);
        }
    }
    if expanded.is_empty() {
        master.to_vec()
    } else {
        expanded
    }
}

/// Role of a media cue attached to a slide or triggered onto a composition layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum MediaCueRole {
    /// Persistent Layer 1 background that stays active across subsequent slides until replaced or cleared.
    #[default]
    Background,
    /// Foreground image/video placed on the Slide layer that clears as soon as the next slide is cued.
    Foreground,
    /// Foreground image/video routed to the independent Announcements layer.
    Announcement,
    /// Audio track cue routed to the audio bus.
    Audio,
}

/// Media cue attached to an individual slide (e.g., via dragging from the Media Bin onto a slide card).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SlideMediaCue {
    pub label: String,
    pub media_path: String,
    pub kind: String,
    pub role: MediaCueRole,
    pub motion_style: ShaderMotionStyle,
    pub primary_color: ColorRgba,
    pub secondary_color: ColorRgba,
    pub accent_color: ColorRgba,
}

/// Persistent active Layer 1 (`MediaBackground`) state — survives slide transitions until
/// another background is triggered or Layer 1 is cleared.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActiveBackground {
    pub label: String,
    pub media_path: String,
    pub motion_style: ShaderMotionStyle,
    pub primary_color: ColorRgba,
    pub secondary_color: ColorRgba,
    pub accent_color: ColorRgba,
}

/// Active foreground image/video state placed on `LayerKind::Slide` or `LayerKind::Announcements`.
/// Foreground items on `LayerKind::Slide` automatically clear when the next slide is cued.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ForegroundMedia {
    pub label: String,
    pub media_path: String,
    pub kind: String,
    pub target_layer: LayerKind,
}

/// Reusable Layer 4 Prop / Overlay item (e.g., speaker nameplate, church logo pill, Give Online bug).
/// Multiple overlays can be active simultaneously; `locked` overlays survive `ClearAll`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OverlayItem {
    pub id: String,
    pub name: String,
    pub text: String,
    pub bounds: RectF,
    pub style: TextStyle,
    #[serde(default)]
    pub accent_color: Option<ColorRgba>,
    #[serde(default)]
    pub locked: bool,
    #[serde(default)]
    pub auto_clear_secs: Option<u32>,
}

/// Reusable Layer 6 tokenized Message template (e.g., `"Parent #{code}, please check Nursery {room}"`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MessageTemplate {
    pub id: String,
    pub name: String,
    pub template_text: String,
    pub show_on_audience: bool,
    pub show_on_stage: bool,
    #[serde(default)]
    pub auto_clear_secs: Option<u32>,
}

/// Live rendered Layer 6 Message displayed on Audience and/or Stage outputs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActiveMessage {
    pub template_id: Option<String>,
    pub rendered_text: String,
    pub show_on_audience: bool,
    pub show_on_stage: bool,
}

/// Automated action attached to an individual slide and fired automatically when the slide goes live.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SlideAction {
    SetOutputLook { output_id: OutputId, look_id: LookId },
    ShowOverlay(String),
    HideOverlay(String),
    SetVariable { key: String, value: String },
    SetMask(MaskStyle),
}

/// Replaces `{token}` placeholders inside a [`MessageTemplate`] with operator-supplied values.
pub fn render_message_template(template: &str, tokens: &BTreeMap<String, String>) -> String {
    let mut out = template.to_string();
    for (k, v) in tokens {
        let placeholder = format!("{{{k}}}");
        out = out.replace(&placeholder, v);
    }
    out
}

/// Expands live dynamic tokens (`{{var:Name}}`, `{{timer}}`, `{{clock}}`, `{{ccli}}`) inside slide text.
pub fn resolve_dynamic_tokens(
    text: &str,
    vars: &BTreeMap<String, String>,
    timer: Option<&str>,
    ccli: Option<&str>,
    clock_str: &str,
) -> String {
    if !text.contains("{{") {
        return text.to_string();
    }
    let mut out = text
        .replace("{{timer}}", timer.unwrap_or("00:00"))
        .replace("{{clock}}", clock_str)
        .replace("{{ccli}}", ccli.unwrap_or(""));
    for (k, v) in vars {
        let pat = format!("{{{{var:{k}}}}}");
        out = out.replace(&pat, v);
    }
    out
}

/// Built-in starter Overlays shared by `SceneState` and `core-library`.
pub fn default_overlays() -> Vec<OverlayItem> {
    vec![
        OverlayItem {
            id: "overlay-speaker-lt".to_string(),
            name: "Speaker Nameplate (Lower-Left)".to_string(),
            text: "Pastor David Chen · Senior Pastor".to_string(),
            bounds: RectF::new(90.0, 860.0, 780.0, 120.0),
            style: TextStyle {
                font_size: 40.0,
                bold: true,
                backdrop_color: ColorRgba::rgba(0.06, 0.10, 0.18, 0.90),
                backdrop_radius: 14.0,
                backdrop_padding: 20.0,
                stroke_width: 1.5,
                h_align: TextAlign::Left,
                v_align: VerticalAlign::Middle,
                ..TextStyle::default()
            },
            accent_color: Some(ColorRgba::rgba(0.22, 0.62, 1.0, 0.96)),
            locked: false,
            auto_clear_secs: Some(12),
        },
        OverlayItem {
            id: "overlay-church-bug".to_string(),
            name: "Sanctuary Live Bug (Locked)".to_string(),
            text: "LIVE · GRACE FELLOWSHIP".to_string(),
            bounds: RectF::new(1460.0, 44.0, 390.0, 74.0),
            style: TextStyle {
                font_size: 26.0,
                bold: true,
                backdrop_color: ColorRgba::rgba(0.75, 0.12, 0.16, 0.88),
                backdrop_radius: 10.0,
                backdrop_padding: 12.0,
                stroke_width: 0.0,
                h_align: TextAlign::Center,
                v_align: VerticalAlign::Middle,
                ..TextStyle::default()
            },
            accent_color: None,
            locked: true,
            auto_clear_secs: None,
        },
    ]
}

/// Built-in starter tokenized Message templates (e.g. Nursery parent paging).
pub fn default_message_templates() -> Vec<MessageTemplate> {
    vec![
        MessageTemplate {
            id: "msg-nursery".to_string(),
            name: "Nursery Parent Paging".to_string(),
            template_text: "Parent with Tag #{code}, please check Nursery Room {room}.".to_string(),
            show_on_audience: true,
            show_on_stage: true,
            auto_clear_secs: Some(45),
        },
        MessageTemplate {
            id: "msg-parking".to_string(),
            name: "Parking Lot Alert".to_string(),
            template_text: "Owner of {vehicle} (Plate {plate}): lights are on in the {lot} lot.".to_string(),
            show_on_audience: true,
            show_on_stage: false,
            auto_clear_secs: Some(30),
        },
    ]
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
    #[serde(default)]
    pub media_cue: Option<SlideMediaCue>,
    #[serde(default)]
    pub auto_advance_secs: Option<f32>,
    #[serde(default)]
    pub loop_to_first: bool,
    #[serde(default)]
    pub actions: Vec<SlideAction>,
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

/// Reusable slide/output visual Template defining typography, safe-area bounds, backdrop pill,
/// optional subtitle band, accent bar, and default mask.
///
/// Templates can be applied directly to slides/decks OR attached to a [`LookConfig`] so each
/// physical/broadcast output renders the same live slide deck with a different layout simultaneously.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TemplateConfig {
    pub id: TemplateId,
    pub name: String,
    #[serde(default)]
    pub category: String,
    pub text_bounds: RectF,
    #[serde(default)]
    pub subtitle_bounds: Option<RectF>,
    pub text_style: TextStyle,
    #[serde(default)]
    pub subtitle_style: Option<TextStyle>,
    #[serde(default)]
    pub accent_bar_color: Option<ColorRgba>,
    #[serde(default)]
    pub default_mask: MaskStyle,
}

/// Per-output Look defining layer visibility, optional template override, blend modes, and broadcast keying behavior.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LookConfig {
    pub id: LookId,
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub enabled_layers: Vec<LayerKind>,
    pub lower_third_mode: bool,
    pub transparent_background: bool,
    pub slide_blend_mode: BlendMode,
    #[serde(default)]
    pub template_id: Option<TemplateId>,
    #[serde(default)]
    pub mask_override: Option<MaskStyle>,
}

impl LookConfig {
    pub fn is_layer_enabled(&self, layer: LayerKind) -> bool {
        self.enabled_layers.contains(&layer)
    }
}

/// Live laser pointer / telestrator highlight overlay coordinates in normalized `[0.0, 1.0]` UV space.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct TelestratorPointer {
    pub x: f32,
    pub y: f32,
    pub active: bool,
    pub color: ColorRgba,
    pub radius: f32,
}

fn default_stage_layout_name() -> String {
    "Band Confidence (Default)".to_string()
}

fn default_stage_current_rect() -> RectF {
    RectF::new(120.0, 230.0, 1680.0, 430.0)
}

fn default_stage_next_rect() -> RectF {
    RectF::new(120.0, 760.0, 1680.0, 190.0)
}

fn default_stage_chords_rect() -> RectF {
    RectF::new(120.0, 150.0, 1680.0, 84.0)
}

fn default_stage_timer_rect() -> RectF {
    RectF::new(1380.0, 12.0, 480.0, 96.0)
}

fn default_stage_message_rect() -> RectF {
    RectF::new(160.0, 930.0, 1600.0, 120.0)
}

/// Customizable Stage Display layout configuration with toggleable, movable/resizable zones.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StageLayoutConfig {
    #[serde(default = "default_stage_layout_name")]
    pub name: String,
    pub show_current_slide: bool,
    pub show_next_slide: bool,
    pub show_chords: bool,
    pub show_clock: bool,
    pub show_timer: bool,
    pub show_stage_message: bool,
    #[serde(default = "default_stage_current_rect")]
    pub current_rect: RectF,
    #[serde(default = "default_stage_next_rect")]
    pub next_rect: RectF,
    #[serde(default = "default_stage_chords_rect")]
    pub chords_rect: RectF,
    #[serde(default = "default_stage_timer_rect")]
    pub timer_rect: RectF,
    #[serde(default = "default_stage_message_rect")]
    pub message_rect: RectF,
}

impl Default for StageLayoutConfig {
    fn default() -> Self {
        Self {
            name: default_stage_layout_name(),
            show_current_slide: true,
            show_next_slide: true,
            show_chords: true,
            show_clock: true,
            show_timer: true,
            show_stage_message: true,
            current_rect: default_stage_current_rect(),
            next_rect: default_stage_next_rect(),
            chords_rect: default_stage_chords_rect(),
            timer_rect: default_stage_timer_rect(),
            message_rect: default_stage_message_rect(),
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
    #[serde(default)]
    pub freeze_output: bool,
    pub active_slide: Option<Slide>,
    pub next_slide_preview: Option<Slide>,
    #[serde(default)]
    pub active_background: Option<ActiveBackground>,
    #[serde(default)]
    pub active_foreground_media: Option<ForegroundMedia>,
    #[serde(default)]
    pub media_transport: MediaTransportState,
    #[serde(default)]
    pub active_audio: Option<AudioPlaybackState>,
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
    #[serde(default)]
    pub templates: BTreeMap<TemplateId, TemplateConfig>,
    #[serde(default)]
    pub overlays: BTreeMap<String, OverlayItem>,
    #[serde(default)]
    pub active_overlay_ids: Vec<String>,
    #[serde(default)]
    pub active_message: Option<ActiveMessage>,
    #[serde(default)]
    pub variables: BTreeMap<String, String>,
    #[serde(default)]
    pub pointer: Option<TelestratorPointer>,
    pub outputs: Vec<OutputConfig>,
    pub deck: Vec<Slide>,
    #[serde(default)]
    pub master_deck: Vec<Slide>,
    #[serde(default)]
    pub active_arrangements: Vec<Arrangement>,
    #[serde(default)]
    pub active_arrangement_id: Option<String>,
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
    SetBackground(ActiveBackground),
    SetForegroundMedia(ForegroundMedia),
    AttachMediaToSlide {
        slide_id: SlideId,
        cue: Option<SlideMediaCue>,
    },
    ToggleVideoInput(bool),
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
    SetFreeze(bool),
    SetMediaTransport(MediaTransportState),
    SetAudioPlayback(Option<AudioPlaybackState>),
    SetArrangements {
        arrangements: Vec<Arrangement>,
        active_id: Option<String>,
    },
    SelectArrangement(Option<String>),
    LoadPresentation {
        slides: Vec<Slide>,
        arrangements: Vec<Arrangement>,
        active_arrangement_id: Option<String>,
    },
    Undo,
    Redo,
    SetOutputLook {
        output_id: OutputId,
        look_id: LookId,
    },
    UpsertLook(LookConfig),
    RemoveLook(LookId),
    UpsertTemplate(TemplateConfig),
    RemoveTemplate(TemplateId),
    ApplyTemplateToSlide {
        slide_id: SlideId,
        template_id: TemplateId,
    },
    ApplyTemplateToDeck {
        template_id: TemplateId,
    },
    ApplyLookRouting {
        assignments: Vec<(OutputId, LookId)>,
    },
    SyncLooksAndTemplates {
        looks: Vec<LookConfig>,
        templates: Vec<TemplateConfig>,
    },
    UpsertOutput(OutputConfig),
    RemoveOutput(OutputId),
    SetOutputStageLayout {
        output_id: OutputId,
        layout: StageLayoutConfig,
    },
    ToggleOverlayLayer(bool),
    UpsertOverlay(OverlayItem),
    RemoveOverlay(String),
    ToggleOverlayItem {
        id: String,
        active: bool,
    },
    SetActiveMessage(Option<ActiveMessage>),
    SetVariable {
        key: String,
        value: Option<String>,
    },
    SetSlideAutomation {
        slide_id: SlideId,
        auto_advance_secs: Option<f32>,
        loop_to_first: bool,
        actions: Vec<SlideAction>,
    },
    SyncOverlays(Vec<OverlayItem>),
    SetPointer(Option<TelestratorPointer>),
    DeleteSlideElement {
        slide_id: SlideId,
        element_id: ElementId,
    },
    SetMask(MaskStyle),
    SetAnnouncement(Option<String>),
    SetStageMessage(Option<String>),
    SetTimerReadout(Option<String>),
    ReplaceDeck(Vec<Slide>),
}

pub const LOOK_AUDIENCE_FULL: LookId = LookId::from_u128(1);
pub const LOOK_STAGE_CONFIDENCE: LookId = LookId::from_u128(2);
pub const LOOK_BROADCAST_LOWER_THIRD: LookId = LookId::from_u128(3);

pub const TEMPLATE_SANCTUARY_CENTER: TemplateId = TemplateId::from_u128(1);
pub const TEMPLATE_LOWER_THIRD_BAR: TemplateId = TemplateId::from_u128(2);
pub const TEMPLATE_SCRIPTURE_CARD: TemplateId = TemplateId::from_u128(3);
pub const TEMPLATE_BOLD_MODERN: TemplateId = TemplateId::from_u128(4);

pub const OUTPUT_AUDIENCE_MAIN: OutputId = OutputId::from_u128(101);
pub const OUTPUT_STAGE_MAIN: OutputId = OutputId::from_u128(102);

/// Built-in default slide/output templates shared by `SceneState` and `core-library`.
pub fn default_templates() -> Vec<TemplateConfig> {
    vec![
        TemplateConfig {
            id: TEMPLATE_SANCTUARY_CENTER,
            name: "Sanctuary Full Center".to_string(),
            category: "Lyrics".to_string(),
            text_bounds: RectF::new(160.0, 230.0, 1600.0, 500.0),
            subtitle_bounds: Some(RectF::new(260.0, 770.0, 1400.0, 140.0)),
            text_style: TextStyle::default(),
            subtitle_style: Some(TextStyle {
                font_size: 46.0,
                bold: false,
                stroke_width: 2.5,
                color: ColorRgba::rgba(0.90, 0.94, 1.0, 0.95),
                ..TextStyle::default()
            }),
            accent_bar_color: None,
            default_mask: MaskStyle::None,
        },
        TemplateConfig {
            id: TEMPLATE_LOWER_THIRD_BAR,
            name: "Broadcast Lower Third (Bar)".to_string(),
            category: "Lower Third".to_string(),
            text_bounds: RectF::new(120.0, 790.0, 1680.0, 200.0),
            subtitle_bounds: None,
            text_style: TextStyle {
                font_size: 54.0,
                bold: true,
                backdrop_color: ColorRgba::rgba(0.05, 0.08, 0.14, 0.88),
                backdrop_radius: 18.0,
                backdrop_padding: 26.0,
                stroke_width: 2.0,
                h_align: TextAlign::Center,
                v_align: VerticalAlign::Middle,
                ..TextStyle::default()
            },
            subtitle_style: None,
            accent_bar_color: Some(ColorRgba::rgba(0.22, 0.62, 1.0, 0.96)),
            default_mask: MaskStyle::None,
        },
        TemplateConfig {
            id: TEMPLATE_SCRIPTURE_CARD,
            name: "Scripture Serif Card".to_string(),
            category: "Scripture".to_string(),
            text_bounds: RectF::new(180.0, 220.0, 1560.0, 520.0),
            subtitle_bounds: Some(RectF::new(180.0, 760.0, 1560.0, 110.0)),
            text_style: TextStyle {
                font_family: "serif".to_string(),
                font_size: 64.0,
                bold: false,
                italic: false,
                line_height: 1.32,
                color: ColorRgba::rgba(0.98, 0.97, 0.93, 1.0),
                stroke_width: 1.5,
                backdrop_color: ColorRgba::rgba(0.05, 0.07, 0.12, 0.78),
                backdrop_radius: 22.0,
                backdrop_padding: 36.0,
                h_align: TextAlign::Left,
                v_align: VerticalAlign::Middle,
                ..TextStyle::default()
            },
            subtitle_style: Some(TextStyle {
                font_family: "sans-serif".to_string(),
                font_size: 40.0,
                bold: true,
                color: ColorRgba::rgba(0.96, 0.80, 0.36, 0.98),
                stroke_width: 1.5,
                h_align: TextAlign::Right,
                v_align: VerticalAlign::Middle,
                ..TextStyle::default()
            }),
            accent_bar_color: Some(ColorRgba::rgba(0.95, 0.78, 0.35, 0.95)),
            default_mask: MaskStyle::SoftVignette,
        },
        TemplateConfig {
            id: TEMPLATE_BOLD_MODERN,
            name: "Modern Left-Aligned Bold".to_string(),
            category: "Lyrics".to_string(),
            text_bounds: RectF::new(140.0, 180.0, 1280.0, 620.0),
            subtitle_bounds: Some(RectF::new(140.0, 820.0, 1280.0, 100.0)),
            text_style: TextStyle {
                font_family: "sans-serif".to_string(),
                font_size: 92.0,
                bold: true,
                line_height: 1.15,
                color: ColorRgba::WHITE,
                stroke_width: 3.0,
                h_align: TextAlign::Left,
                v_align: VerticalAlign::Middle,
                ..TextStyle::default()
            },
            subtitle_style: Some(TextStyle {
                font_size: 42.0,
                bold: false,
                color: ColorRgba::rgba(0.75, 0.87, 1.0, 0.92),
                stroke_width: 2.0,
                h_align: TextAlign::Left,
                v_align: VerticalAlign::Middle,
                ..TextStyle::default()
            }),
            accent_bar_color: None,
            default_mask: MaskStyle::None,
        },
    ]
}

/// Built-in default Looks shared by `SceneState` and `core-library`.
pub fn default_looks() -> Vec<LookConfig> {
    vec![
        LookConfig {
            id: LOOK_AUDIENCE_FULL,
            name: "Audience Full".to_string(),
            description: "Full 7-layer sanctuary presentation output".to_string(),
            enabled_layers: LayerKind::ALL.to_vec(),
            lower_third_mode: false,
            transparent_background: false,
            slide_blend_mode: BlendMode::Normal,
            template_id: None,
            mask_override: None,
        },
        LookConfig {
            id: LOOK_STAGE_CONFIDENCE,
            name: "Stage Confidence".to_string(),
            description: "High-contrast lyrics, chords, next-slide cue, and timer for stage".to_string(),
            enabled_layers: vec![LayerKind::Slide, LayerKind::MessagesAndTimers],
            lower_third_mode: false,
            transparent_background: false,
            slide_blend_mode: BlendMode::Normal,
            template_id: None,
            mask_override: None,
        },
        LookConfig {
            id: LOOK_BROADCAST_LOWER_THIRD,
            name: "Broadcast Lower Third (Alpha)".to_string(),
            description: "Transparent alpha lower-third lyric bar for livestream / NDI / SDI key+fill".to_string(),
            enabled_layers: vec![LayerKind::Slide, LayerKind::PropsAndOverlays],
            lower_third_mode: true,
            transparent_background: true,
            slide_blend_mode: BlendMode::Normal,
            template_id: Some(TEMPLATE_LOWER_THIRD_BAR),
            mask_override: None,
        },
    ]
}

/// Applies a [`TemplateConfig`]'s layout, typography, optional subtitle styling, and optional
/// accent bar directly onto a [`Slide`]'s elements.
pub fn apply_template_to_slide_mut(slide: &mut Slide, template: &TemplateConfig) {
    let mut text_indices = Vec::new();
    for (idx, el) in slide.elements.iter().enumerate() {
        if matches!(el.kind, SlideElementKind::TextBox { .. }) {
            text_indices.push(idx);
        }
    }

    let primary_text = text_indices
        .first()
        .and_then(|&i| match &slide.elements[i].kind {
            SlideElementKind::TextBox { text, .. } => Some(text.clone()),
            _ => None,
        })
        .unwrap_or_else(|| {
            if slide.body.trim().is_empty() {
                slide.headline.clone()
            } else {
                format!("{}\n{}", slide.headline, slide.body)
            }
        });

    let secondary_text = text_indices
        .get(1)
        .and_then(|&i| match &slide.elements[i].kind {
            SlideElementKind::TextBox { text, .. } => Some(text.clone()),
            _ => None,
        });

    let mut new_elements = Vec::new();

    if let Some(accent) = template.accent_bar_color {
        let b = template.text_bounds;
        new_elements.push(SlideElement {
            id: ElementId::from_u128(template.id.0.as_u128() ^ 0xA55E_7BA8_0000_0001),
            name: "Template Accent Bar".to_string(),
            bounds: RectF::new(b.x, b.y, 12.0, b.height),
            opacity: 1.0,
            z_index: 8,
            kind: SlideElementKind::ShapeRect {
                fill: accent,
                stroke: ColorRgba::TRANSPARENT,
                stroke_width: 0.0,
                corner_radius: 6.0,
            },
        });
    }

    let primary_id = text_indices
        .first()
        .map(|&i| slide.elements[i].id)
        .unwrap_or_else(ElementId::new);
    new_elements.push(SlideElement {
        id: primary_id,
        name: "Primary Lyrics".to_string(),
        bounds: template.text_bounds,
        opacity: 1.0,
        z_index: 10,
        kind: SlideElementKind::TextBox {
            text: primary_text,
            style: template.text_style.clone(),
        },
    });

    if let (Some(sub_bounds), Some(sub_text)) = (template.subtitle_bounds, secondary_text) {
        let sub_id = text_indices
            .get(1)
            .map(|&i| slide.elements[i].id)
            .unwrap_or_else(ElementId::new);
        let sub_style = template.subtitle_style.clone().unwrap_or_else(|| TextStyle {
            font_size: (template.text_style.font_size * 0.55).max(28.0),
            bold: false,
            ..template.text_style.clone()
        });
        new_elements.push(SlideElement {
            id: sub_id,
            name: "Reference / Subtitle".to_string(),
            bounds: sub_bounds,
            opacity: 0.92,
            z_index: 11,
            kind: SlideElementKind::TextBox {
                text: sub_text,
                style: sub_style,
            },
        });
    }

    slide.elements = new_elements;
}

impl SceneState {
    /// Creates a ready-to-run demonstration state with 2 outputs, 3 Looks, 4 Templates, and rich worship slides.
    pub fn demo_m0(output_count: usize) -> Self {
        let mut looks = BTreeMap::new();
        for look in default_looks() {
            looks.insert(look.id, look);
        }
        let mut templates = BTreeMap::new();
        for tpl in default_templates() {
            templates.insert(tpl.id, tpl);
        }
        let mut overlays = BTreeMap::new();
        for ov in default_overlays() {
            overlays.insert(ov.id.clone(), ov);
        }
        let mut variables = BTreeMap::new();
        variables.insert("Service".to_string(), "Sunday 10:00 AM".to_string());
        variables.insert("TeamA".to_string(), "0".to_string());
        variables.insert("TeamB".to_string(), "0".to_string());

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

        let initial_bg = first_slide.as_ref().map(|s| ActiveBackground {
            label: "Aurora Wave (GPU Motion)".to_string(),
            media_path: "builtin://motion/aurora-wave".to_string(),
            motion_style: s.motion_style,
            primary_color: s.primary_color,
            secondary_color: s.secondary_color,
            accent_color: s.accent_color,
        });

        Self {
            revision: 1,
            blackout: false,
            freeze_output: false,
            active_slide: first_slide,
            next_slide_preview: next_slide,
            active_background: initial_bg,
            active_foreground_media: None,
            media_transport: MediaTransportState::default(),
            active_audio: None,
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
            templates,
            overlays,
            active_overlay_ids: Vec::new(),
            active_message: None,
            variables,
            pointer: None,
            outputs,
            master_deck: deck.clone(),
            deck,
            active_arrangements: Vec::new(),
            active_arrangement_id: None,
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
                    // Foreground images/videos on the Slide layer always clear when the next slide is cued.
                    if self
                        .active_foreground_media
                        .as_ref()
                        .is_some_and(|fg| fg.target_layer == LayerKind::Slide)
                    {
                        self.active_foreground_media = None;
                    }
                    // Apply any media cue explicitly attached to this slide (Background stays persistent!).
                    if let Some(cue) = slide.media_cue.clone() {
                        match cue.role {
                            MediaCueRole::Background => {
                                self.active_background = Some(ActiveBackground {
                                    label: cue.label,
                                    media_path: cue.media_path,
                                    motion_style: cue.motion_style,
                                    primary_color: cue.primary_color,
                                    secondary_color: cue.secondary_color,
                                    accent_color: cue.accent_color,
                                });
                                self.media_layer_active = true;
                            }
                            MediaCueRole::Foreground => {
                                self.active_foreground_media = Some(ForegroundMedia {
                                    label: cue.label,
                                    media_path: cue.media_path,
                                    kind: cue.kind,
                                    target_layer: LayerKind::Slide,
                                });
                            }
                            MediaCueRole::Announcement => {
                                self.active_foreground_media = Some(ForegroundMedia {
                                    label: cue.label.clone(),
                                    media_path: cue.media_path,
                                    kind: cue.kind,
                                    target_layer: LayerKind::Announcements,
                                });
                                self.announcements_layer_active = true;
                                self.announcement_text = Some(cue.label);
                            }
                            MediaCueRole::Audio => {}
                        }
                    }
                    // Execute any automated SlideActions attached to this slide.
                    for action in slide.actions.clone() {
                        match action {
                            SlideAction::SetOutputLook { output_id, look_id } => {
                                if self.looks.contains_key(&look_id)
                                    && let Some(out) = self.outputs.iter_mut().find(|o| o.id == output_id)
                                {
                                    out.look_id = look_id;
                                }
                            }
                            SlideAction::ShowOverlay(oid) => {
                                if self.overlays.contains_key(&oid) && !self.active_overlay_ids.contains(&oid) {
                                    self.active_overlay_ids.push(oid);
                                    self.overlay_layer_active = true;
                                }
                            }
                            SlideAction::HideOverlay(oid) => {
                                self.active_overlay_ids.retain(|id| id != &oid);
                            }
                            SlideAction::SetVariable { key, value } => {
                                self.variables.insert(key, value);
                            }
                            SlideAction::SetMask(mask) => {
                                self.mask_style = mask;
                            }
                        }
                    }
                    self.active_slide = Some(slide);
                    self.next_slide_preview = self.deck.get(idx + 1).cloned();
                    self.slide_layer_active = true;
                    self.blackout = false;
                    self.revision += 1;
                    true
                } else {
                    false
                }
            }
            SceneDiff::TriggerCustomSlide { slide } => {
                if self
                    .active_foreground_media
                    .as_ref()
                    .is_some_and(|fg| fg.target_layer == LayerKind::Slide)
                {
                    self.active_foreground_media = None;
                }
                self.active_slide = Some(slide);
                self.slide_layer_active = true;
                self.blackout = false;
                self.revision += 1;
                true
            }
            SceneDiff::SetBackground(bg) => {
                // Background sits on Layer 1 independently and NEVER clears the active slide!
                self.active_background = Some(bg);
                self.media_layer_active = true;
                self.blackout = false;
                self.revision += 1;
                true
            }
            SceneDiff::SetForegroundMedia(fg) => {
                match fg.target_layer {
                    LayerKind::Announcements => {
                        self.announcements_layer_active = true;
                        self.announcement_text = Some(fg.label.clone());
                    }
                    _ => {
                        self.slide_layer_active = true;
                    }
                }
                self.active_foreground_media = Some(fg);
                self.blackout = false;
                self.revision += 1;
                true
            }
            SceneDiff::AttachMediaToSlide { slide_id, cue } => {
                if let Some(slide) = self.deck.iter_mut().find(|s| s.id == slide_id) {
                    slide.media_path = cue.as_ref().map(|c| c.media_path.clone());
                    slide.media_cue = cue;
                    if self.active_slide.as_ref().map(|s| s.id) == Some(slide_id) {
                        self.active_slide = Some(slide.clone());
                    }
                    self.revision += 1;
                    true
                } else {
                    false
                }
            }
            SceneDiff::ToggleVideoInput(active) => {
                self.video_input_active = active;
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
                    // Execute any automated SlideActions attached to this slide.
                    for action in slide.actions.clone() {
                        match action {
                            SlideAction::SetOutputLook { output_id, look_id } => {
                                if self.looks.contains_key(&look_id)
                                    && let Some(out) = self.outputs.iter_mut().find(|o| o.id == output_id)
                                {
                                    out.look_id = look_id;
                                }
                            }
                            SlideAction::ShowOverlay(oid) => {
                                if self.overlays.contains_key(&oid) && !self.active_overlay_ids.contains(&oid) {
                                    self.active_overlay_ids.push(oid);
                                    self.overlay_layer_active = true;
                                }
                            }
                            SlideAction::HideOverlay(oid) => {
                                self.active_overlay_ids.retain(|id| id != &oid);
                            }
                            SlideAction::SetVariable { key, value } => {
                                self.variables.insert(key, value);
                            }
                            SlideAction::SetMask(mask) => {
                                self.mask_style = mask;
                            }
                        }
                    }
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
                    LayerKind::MediaBackground => {
                        self.media_layer_active = false;
                        self.active_background = None;
                    }
                    LayerKind::Slide => {
                        self.slide_layer_active = false;
                        if self
                            .active_foreground_media
                            .as_ref()
                            .is_some_and(|fg| fg.target_layer == LayerKind::Slide)
                        {
                            self.active_foreground_media = None;
                        }
                    }
                    LayerKind::Announcements => {
                        self.announcements_layer_active = false;
                        self.announcement_text = None;
                        if self
                            .active_foreground_media
                            .as_ref()
                            .is_some_and(|fg| fg.target_layer == LayerKind::Announcements)
                        {
                            self.active_foreground_media = None;
                        }
                    }
                    LayerKind::PropsAndOverlays => {
                        self.active_overlay_ids.retain(|id| {
                            self.overlays.get(id).is_some_and(|ov| ov.locked)
                        });
                        self.overlay_layer_active = !self.active_overlay_ids.is_empty();
                    }
                    LayerKind::Masks => self.mask_style = MaskStyle::None,
                    LayerKind::MessagesAndTimers => {
                        self.stage_message = None;
                        self.active_message = None;
                        self.timer_readout = None;
                    }
                }
                self.revision += 1;
                true
            }
            SceneDiff::ClearAll => {
                self.video_input_active = false;
                self.media_layer_active = false;
                self.active_background = None;
                self.slide_layer_active = false;
                self.active_foreground_media = None;
                self.announcements_layer_active = false;
                self.active_overlay_ids.retain(|id| {
                    self.overlays.get(id).is_some_and(|ov| ov.locked)
                });
                self.overlay_layer_active = !self.active_overlay_ids.is_empty();
                self.announcement_text = None;
                self.mask_style = MaskStyle::None;
                self.stage_message = None;
                self.active_message = None;
                self.timer_readout = None;
                self.revision += 1;
                true
            }
            SceneDiff::SetBlackout(enabled) => {
                self.blackout = enabled;
                self.revision += 1;
                true
            }
            SceneDiff::SetFreeze(enabled) => {
                self.freeze_output = enabled;
                self.revision += 1;
                true
            }
            SceneDiff::SetMediaTransport(transport) => {
                self.media_transport = transport;
                self.revision += 1;
                true
            }
            SceneDiff::SetAudioPlayback(audio) => {
                self.active_audio = audio;
                self.revision += 1;
                true
            }
            SceneDiff::SetArrangements {
                arrangements,
                active_id,
            } => {
                if self.master_deck.is_empty() {
                    self.master_deck = self.deck.clone();
                }
                self.active_arrangements = arrangements;
                self.active_arrangement_id = active_id;
                let selected = self.active_arrangement_id.as_deref().and_then(|id| {
                    self.active_arrangements.iter().find(|a| a.id == id)
                });
                self.deck = expand_deck_by_arrangement(&self.master_deck, selected);
                self.active_slide = self.deck.first().cloned();
                self.next_slide_preview = self.deck.get(1).cloned();
                self.revision += 1;
                true
            }
            SceneDiff::SelectArrangement(active_id) => {
                if self.master_deck.is_empty() {
                    self.master_deck = self.deck.clone();
                }
                if let Some(ref id) = active_id
                    && !self.active_arrangements.iter().any(|a| a.id == *id)
                {
                    return false;
                }
                self.active_arrangement_id = active_id;
                let selected = self.active_arrangement_id.as_deref().and_then(|id| {
                    self.active_arrangements.iter().find(|a| a.id == id)
                });
                self.deck = expand_deck_by_arrangement(&self.master_deck, selected);
                self.active_slide = self.deck.first().cloned();
                self.next_slide_preview = self.deck.get(1).cloned();
                self.revision += 1;
                true
            }
            SceneDiff::LoadPresentation {
                slides,
                arrangements,
                active_arrangement_id,
            } => {
                self.master_deck = slides;
                self.active_arrangements = arrangements;
                self.active_arrangement_id = active_arrangement_id;
                let selected = self.active_arrangement_id.as_deref().and_then(|id| {
                    self.active_arrangements.iter().find(|a| a.id == id)
                });
                self.deck = expand_deck_by_arrangement(&self.master_deck, selected);
                self.active_slide = self.deck.first().cloned();
                self.next_slide_preview = self.deck.get(1).cloned();
                self.revision += 1;
                true
            }
            SceneDiff::Undo | SceneDiff::Redo => false,
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
            SceneDiff::UpsertLook(mut look) => {
                look.name = look.name.trim().to_string();
                if look.name.is_empty() {
                    return false;
                }
                if let Some(tid) = look.template_id
                    && !self.templates.contains_key(&tid)
                {
                    look.template_id = None;
                }
                self.looks.insert(look.id, look);
                self.revision += 1;
                true
            }
            SceneDiff::RemoveLook(look_id) => {
                // Always preserve core Audience & Stage fallback Looks.
                if look_id == LOOK_AUDIENCE_FULL || look_id == LOOK_STAGE_CONFIDENCE {
                    return false;
                }
                if self.looks.remove(&look_id).is_some() {
                    for out in &mut self.outputs {
                        if out.look_id == look_id {
                            out.look_id = match out.kind {
                                OutputKind::Audience => LOOK_AUDIENCE_FULL,
                                OutputKind::Stage => LOOK_STAGE_CONFIDENCE,
                            };
                        }
                    }
                    self.revision += 1;
                    true
                } else {
                    false
                }
            }
            SceneDiff::UpsertTemplate(mut tpl) => {
                tpl.name = tpl.name.trim().to_string();
                if tpl.name.is_empty()
                    || tpl.text_bounds.width <= 0.0
                    || tpl.text_bounds.height <= 0.0
                {
                    return false;
                }
                if tpl.category.trim().is_empty() {
                    tpl.category = "Custom".to_string();
                }
                self.templates.insert(tpl.id, tpl);
                self.revision += 1;
                true
            }
            SceneDiff::RemoveTemplate(template_id) => {
                if self.templates.remove(&template_id).is_some() {
                    for look in self.looks.values_mut() {
                        if look.template_id == Some(template_id) {
                            look.template_id = None;
                        }
                    }
                    self.revision += 1;
                    true
                } else {
                    false
                }
            }
            SceneDiff::ApplyTemplateToSlide {
                slide_id,
                template_id,
            } => {
                let Some(tpl) = self.templates.get(&template_id).cloned() else {
                    return false;
                };
                if let Some(slide) = self.deck.iter_mut().find(|s| s.id == slide_id) {
                    apply_template_to_slide_mut(slide, &tpl);
                    if self.active_slide.as_ref().map(|s| s.id) == Some(slide_id) {
                        self.active_slide = Some(slide.clone());
                    }
                    if self.next_slide_preview.as_ref().map(|s| s.id) == Some(slide_id) {
                        self.next_slide_preview = Some(slide.clone());
                    }
                    self.revision += 1;
                    true
                } else {
                    false
                }
            }
            SceneDiff::ApplyTemplateToDeck { template_id } => {
                let Some(tpl) = self.templates.get(&template_id).cloned() else {
                    return false;
                };
                for slide in &mut self.deck {
                    apply_template_to_slide_mut(slide, &tpl);
                }
                if let Some(active_id) = self.active_slide.as_ref().map(|s| s.id) {
                    if let Some(updated) = self.deck.iter().find(|s| s.id == active_id).cloned() {
                        self.active_slide = Some(updated);
                    } else if let Some(active) = self.active_slide.as_mut() {
                        apply_template_to_slide_mut(active, &tpl);
                    }
                }
                if let Some(next_id) = self.next_slide_preview.as_ref().map(|s| s.id)
                    && let Some(updated) = self.deck.iter().find(|s| s.id == next_id).cloned()
                {
                    self.next_slide_preview = Some(updated);
                }
                self.revision += 1;
                true
            }
            SceneDiff::ApplyLookRouting { assignments } => {
                let mut changed = false;
                for (oid, lid) in assignments {
                    if self.looks.contains_key(&lid)
                        && let Some(out) = self.outputs.iter_mut().find(|o| o.id == oid)
                    {
                        out.look_id = lid;
                        changed = true;
                    }
                }
                if changed {
                    self.revision += 1;
                }
                changed
            }
            SceneDiff::SyncLooksAndTemplates { looks, templates } => {
                for tpl in templates {
                    if !tpl.name.trim().is_empty() {
                        self.templates.insert(tpl.id, tpl);
                    }
                }
                for look in looks {
                    if !look.name.trim().is_empty() {
                        self.looks.insert(look.id, look);
                    }
                }
                self.revision += 1;
                true
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
            SceneDiff::UpsertOverlay(mut item) => {
                item.name = item.name.trim().to_string();
                if item.id.trim().is_empty() || item.name.is_empty() {
                    return false;
                }
                self.overlays.insert(item.id.clone(), item);
                self.revision += 1;
                true
            }
            SceneDiff::RemoveOverlay(id) => {
                if self.overlays.remove(&id).is_some() {
                    self.active_overlay_ids.retain(|x| x != &id);
                    self.revision += 1;
                    true
                } else {
                    false
                }
            }
            SceneDiff::ToggleOverlayItem { id, active } => {
                if !self.overlays.contains_key(&id) {
                    return false;
                }
                if active {
                    if !self.active_overlay_ids.contains(&id) {
                        self.active_overlay_ids.push(id);
                    }
                    self.overlay_layer_active = true;
                } else {
                    self.active_overlay_ids.retain(|x| x != &id);
                }
                self.revision += 1;
                true
            }
            SceneDiff::SetActiveMessage(msg) => {
                if let Some(ref m) = msg
                    && m.show_on_stage
                {
                    self.stage_message = Some(m.rendered_text.clone());
                } else if msg.is_none() {
                    self.stage_message = None;
                }
                self.active_message = msg;
                self.revision += 1;
                true
            }
            SceneDiff::SetVariable { key, value } => {
                let clean = key.trim().to_string();
                if clean.is_empty() {
                    return false;
                }
                match value {
                    Some(v) => {
                        self.variables.insert(clean, v);
                    }
                    None => {
                        self.variables.remove(&clean);
                    }
                }
                self.revision += 1;
                true
            }
            SceneDiff::SetSlideAutomation {
                slide_id,
                auto_advance_secs,
                loop_to_first,
                actions,
            } => {
                let mut found = false;
                for list in [&mut self.deck, &mut self.master_deck] {
                    if let Some(s) = list.iter_mut().find(|s| s.id == slide_id) {
                        s.auto_advance_secs = auto_advance_secs;
                        s.loop_to_first = loop_to_first;
                        s.actions = actions.clone();
                        found = true;
                    }
                }
                if self.active_slide.as_ref().map(|s| s.id) == Some(slide_id)
                    && let Some(active) = self.active_slide.as_mut()
                {
                    active.auto_advance_secs = auto_advance_secs;
                    active.loop_to_first = loop_to_first;
                    active.actions = actions;
                }
                if found {
                    self.revision += 1;
                }
                found
            }
            SceneDiff::SyncOverlays(items) => {
                for item in items {
                    if !item.id.trim().is_empty() && !item.name.trim().is_empty() {
                        self.overlays.insert(item.id.clone(), item);
                    }
                }
                self.revision += 1;
                true
            }
            SceneDiff::SetPointer(ptr) => {
                self.pointer = ptr;
                self.revision += 1;
                true
            }
            SceneDiff::DeleteSlideElement {
                slide_id,
                element_id,
            } => {
                let mut removed = false;
                if let Some(slide) = self.deck.iter_mut().find(|s| s.id == slide_id) {
                    let before = slide.elements.len();
                    slide.elements.retain(|e| e.id != element_id);
                    if slide.elements.len() < before {
                        if self.active_slide.as_ref().map(|s| s.id) == Some(slide_id) {
                            self.active_slide = Some(slide.clone());
                        }
                        removed = true;
                    }
                }
                if removed {
                    self.revision += 1;
                }
                removed
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
                self.master_deck = deck.clone();
                self.deck = deck;
                self.active_arrangements.clear();
                self.active_arrangement_id = None;
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

    pub fn template_for_look(&self, look: &LookConfig) -> Option<&TemplateConfig> {
        let tid = look.template_id?;
        self.templates.get(&tid)
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
            media_cue: None,
            auto_advance_secs: None,
            loop_to_first: false,
            actions: Vec::new(),
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
            media_cue: None,
            auto_advance_secs: None,
            loop_to_first: false,
            actions: Vec::new(),
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
            media_cue: None,
            auto_advance_secs: None,
            loop_to_first: false,
            actions: Vec::new(),
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
            media_cue: None,
            auto_advance_secs: None,
            loop_to_first: false,
            actions: Vec::new(),
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
            label: "05 — Closing Benediction".to_string(),
            group: "Benediction".to_string(),
            headline: "The LORD bless thee, and keep thee".to_string(),
            body: "Numbers 6:24-26 (KJV)".to_string(),
            chords_line: None,
            ccli_number: None,
            media_path: None,
            media_cue: None,
            auto_advance_secs: None,
            loop_to_first: false,
            actions: Vec::new(),
            primary_color: ColorRgba::rgb(0.06, 0.12, 0.24),
            secondary_color: ColorRgba::rgb(0.14, 0.30, 0.52),
            accent_color: ColorRgba::rgb(0.95, 0.82, 0.45),
            motion_style: ShaderMotionStyle::SanctuaryGlow,
            transition: TransitionSpec::Dissolve { duration_ms: 500 },
            elements: default_lyric_elements(
                5040,
                "The LORD bless thee, and keep thee:\nThe LORD make his face shine upon thee,\nand be gracious unto thee.",
                Some("NUMBERS 6:24-25 (KJV)"),
            ),
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

    #[test]
    fn separates_persistent_background_layer_from_ephemeral_foreground_media() {
        let mut state = SceneState::demo_m0(2);
        let original_slide_id = state.active_slide.as_ref().expect("slide 0").id;

        // 1. Triggering a background updates Layer 1 WITHOUT clearing the active slide!
        let glow_bg = ActiveBackground {
            label: "Sanctuary Glow".into(),
            media_path: "builtin://motion/sanctuary-glow".into(),
            motion_style: ShaderMotionStyle::SanctuaryGlow,
            primary_color: ColorRgba::rgb(0.24, 0.08, 0.05),
            secondary_color: ColorRgba::rgb(0.76, 0.32, 0.10),
            accent_color: ColorRgba::rgb(1.0, 0.88, 0.62),
        };
        assert!(state.apply(SceneDiff::SetBackground(glow_bg.clone())));
        assert_eq!(state.active_slide.as_ref().map(|s| s.id), Some(original_slide_id));
        assert_eq!(state.active_background.as_ref(), Some(&glow_bg));
        assert!(state.media_layer_active);

        // 2. Switching slides PRESERVES the active background!
        let slide2_id = state.deck[1].id;
        assert!(state.apply(SceneDiff::TriggerSlide {
            slide_id: slide2_id,
            transition_override: None,
        }));
        assert_eq!(state.active_background.as_ref(), Some(&glow_bg));

        // 3. Foreground image on Slide layer clears as soon as the next slide is cued!
        assert!(state.apply(SceneDiff::SetForegroundMedia(ForegroundMedia {
            label: "SermonGraphic.png".into(),
            media_path: "C:\\Media\\SermonGraphic.png".into(),
            kind: "image".into(),
            target_layer: LayerKind::Slide,
        })));
        assert!(state.active_foreground_media.is_some());
        assert_eq!(state.active_background.as_ref(), Some(&glow_bg));

        let slide3_id = state.deck[2].id;
        assert!(state.apply(SceneDiff::TriggerSlide {
            slide_id: slide3_id,
            transition_override: None,
        }));
        assert!(state.active_foreground_media.is_none());
        assert_eq!(state.active_background.as_ref(), Some(&glow_bg));

        // 4. Foreground image on Announcements layer persists across slide switches until cleared!
        assert!(state.apply(SceneDiff::SetForegroundMedia(ForegroundMedia {
            label: "AnnouncementCard.png".into(),
            media_path: "C:\\Media\\AnnouncementCard.png".into(),
            kind: "image".into(),
            target_layer: LayerKind::Announcements,
        })));
        assert!(state.apply(SceneDiff::TriggerSlide {
            slide_id: slide2_id,
            transition_override: None,
        }));
        assert!(state.active_foreground_media.is_some());
        assert!(state.apply(SceneDiff::ClearLayer(LayerKind::Announcements)));
        assert!(state.active_foreground_media.is_none());

        // 5. Attaching a Background media cue to a slide triggers that background when cued.
        let cue = SlideMediaCue {
            label: "Broadcast Grid".into(),
            media_path: "builtin://motion/broadcast-grid".into(),
            kind: "background".into(),
            role: MediaCueRole::Background,
            motion_style: ShaderMotionStyle::BroadcastGrid,
            primary_color: ColorRgba::rgb(0.08, 0.10, 0.12),
            secondary_color: ColorRgba::rgb(0.18, 0.24, 0.30),
            accent_color: ColorRgba::rgb(0.20, 0.90, 0.55),
        };
        assert!(state.apply(SceneDiff::AttachMediaToSlide {
            slide_id: slide3_id,
            cue: Some(cue),
        }));
        assert!(!state.apply(SceneDiff::AttachMediaToSlide {
            slide_id: SlideId::new(),
            cue: None,
        }));
        assert!(state.apply(SceneDiff::TriggerSlide {
            slide_id: slide3_id,
            transition_override: None,
        }));
        assert_eq!(
            state.active_background.as_ref().map(|b| b.label.as_str()),
            Some("Broadcast Grid")
        );

        // 6. Toggle Video Input + clear individual layers independently
        assert!(state.apply(SceneDiff::ToggleVideoInput(true)));
        assert!(state.video_input_active);
        assert!(state.apply(SceneDiff::ClearLayer(LayerKind::MediaBackground)));
        assert!(state.active_background.is_none());
        assert!(state.slide_layer_active);
    }

    #[test]
    fn manages_custom_looks_templates_routing_and_slide_application() {
        let mut state = SceneState::demo_m0(2);
        assert_eq!(state.templates.len(), 4);
        assert_eq!(state.looks.len(), 3);
        let _ = LookId::default();
        let _ = TemplateId::default();

        // 1. Upsert custom Template (happy path + validation failures)
        let custom_tpl_id = TemplateId::new();
        let custom_tpl = TemplateConfig {
            id: custom_tpl_id,
            name: "  Livestream Lower Bar  ".to_string(),
            category: "   ".to_string(),
            text_bounds: RectF::new(100.0, 800.0, 1720.0, 180.0),
            subtitle_bounds: Some(RectF::new(100.0, 990.0, 1720.0, 60.0)),
            text_style: TextStyle {
                font_family: "serif".to_string(),
                font_size: 52.0,
                backdrop_color: ColorRgba::rgba(0.05, 0.08, 0.14, 0.88),
                ..TextStyle::default()
            },
            subtitle_style: None,
            accent_bar_color: Some(ColorRgba::rgb(0.2, 0.6, 1.0)),
            default_mask: MaskStyle::LowerThirdBand,
        };
        assert!(state.apply(SceneDiff::UpsertTemplate(custom_tpl.clone())));
        let stored_tpl = state.templates.get(&custom_tpl_id).expect("stored tpl");
        assert_eq!(stored_tpl.name, "Livestream Lower Bar");
        assert_eq!(stored_tpl.category, "Custom");

        // Failure paths for UpsertTemplate
        let mut bad_tpl = custom_tpl.clone();
        bad_tpl.name = "  ".into();
        assert!(!state.apply(SceneDiff::UpsertTemplate(bad_tpl)));
        let mut bad_w_tpl = custom_tpl.clone();
        bad_w_tpl.text_bounds.width = 0.0;
        assert!(!state.apply(SceneDiff::UpsertTemplate(bad_w_tpl)));

        // 2. Upsert custom Look referencing custom Template
        let custom_look_id = LookId::new();
        let custom_look = LookConfig {
            id: custom_look_id,
            name: "  Livestream Custom  ".to_string(),
            description: "Key + Fill with serif bar".to_string(),
            enabled_layers: vec![LayerKind::Slide, LayerKind::Masks],
            lower_third_mode: true,
            transparent_background: true,
            slide_blend_mode: BlendMode::Screen,
            template_id: Some(custom_tpl_id),
            mask_override: Some(MaskStyle::LowerThirdBand),
        };
        assert!(state.apply(SceneDiff::UpsertLook(custom_look.clone())));
        assert_eq!(
            state
                .template_for_look(state.looks.get(&custom_look_id).expect("look"))
                .map(|t| t.id),
            Some(custom_tpl_id)
        );

        // Failure path: blank look name & unknown template_id sanitized to None
        let mut bad_look = custom_look.clone();
        bad_look.name = " ".into();
        assert!(!state.apply(SceneDiff::UpsertLook(bad_look)));
        let mut unknown_tpl_look = custom_look.clone();
        unknown_tpl_look.id = LookId::new();
        unknown_tpl_look.template_id = Some(TemplateId::new());
        assert!(state.apply(SceneDiff::UpsertLook(unknown_tpl_look.clone())));
        assert!(state.looks.get(&unknown_tpl_look.id).expect("sanitized").template_id.is_none());

        // 3. Apply Look Routing atomically across outputs
        assert!(state.apply(SceneDiff::ApplyLookRouting {
            assignments: vec![
                (OUTPUT_AUDIENCE_MAIN, custom_look_id),
                (OUTPUT_STAGE_MAIN, LOOK_STAGE_CONFIDENCE),
            ],
        }));
        assert_eq!(
            state.look_for_output(OUTPUT_AUDIENCE_MAIN).map(|l| l.id),
            Some(custom_look_id)
        );
        assert!(!state.apply(SceneDiff::ApplyLookRouting {
            assignments: vec![(OutputId::new(), LookId::new())],
        }));

        // 4. Apply Template directly to a single slide & to the entire deck
        let first_id = state.deck[0].id;
        assert!(state.apply(SceneDiff::ApplyTemplateToSlide {
            slide_id: first_id,
            template_id: custom_tpl_id,
        }));
        assert!(
            state
                .active_slide
                .as_ref()
                .expect("active")
                .elements
                .iter()
                .any(|e| e.name == "Template Accent Bar")
        );
        assert!(!state.apply(SceneDiff::ApplyTemplateToSlide {
            slide_id: SlideId::new(),
            template_id: custom_tpl_id,
        }));
        assert!(!state.apply(SceneDiff::ApplyTemplateToSlide {
            slide_id: first_id,
            template_id: TemplateId::new(),
        }));

        assert!(state.apply(SceneDiff::ApplyTemplateToDeck {
            template_id: TEMPLATE_SCRIPTURE_CARD,
        }));
        assert!(state.deck.iter().all(|s| {
            s.elements
                .iter()
                .any(|e| matches!(&e.kind, SlideElementKind::TextBox { style, .. } if style.font_family == "serif"))
        }));
        assert!(!state.apply(SceneDiff::ApplyTemplateToDeck {
            template_id: TemplateId::new(),
        }));

        // 5. Removing Template clears look.template_id; Removing Look resets output fallback; Built-ins protected
        assert!(state.apply(SceneDiff::RemoveTemplate(custom_tpl_id)));
        assert!(!state.apply(SceneDiff::RemoveTemplate(custom_tpl_id)));
        assert!(state.looks.get(&custom_look_id).expect("look").template_id.is_none());

        assert!(!state.apply(SceneDiff::RemoveLook(LOOK_AUDIENCE_FULL)));
        assert!(!state.apply(SceneDiff::RemoveLook(LOOK_STAGE_CONFIDENCE)));
        assert!(state.apply(SceneDiff::RemoveLook(custom_look_id)));
        assert!(!state.apply(SceneDiff::RemoveLook(custom_look_id)));
        assert_eq!(
            state.look_for_output(OUTPUT_AUDIENCE_MAIN).map(|l| l.id),
            Some(LOOK_AUDIENCE_FULL)
        );

        // 6. SyncLooksAndTemplates hydrates persisted records
        assert!(state.apply(SceneDiff::SyncLooksAndTemplates {
            looks: vec![custom_look],
            templates: vec![custom_tpl],
        }));
        assert!(state.looks.contains_key(&custom_look_id));
        assert!(state.templates.contains_key(&custom_tpl_id));
    }

    #[test]
    fn manages_freeze_media_transport_audio_and_non_destructive_arrangements() {
        let mut state = SceneState::demo_m0(2);
        assert!(!state.freeze_output);
        assert!(state.apply(SceneDiff::SetFreeze(true)));
        assert!(state.freeze_output);

        assert!(state.apply(SceneDiff::SetMediaTransport(MediaTransportState {
            playing: false,
            looping: true,
            muted: false,
            volume: 0.75,
            fit_mode: MediaFitMode::Contain,
            position_secs: 12.5,
            duration_secs: 60.0,
        })));
        assert_eq!(state.media_transport.fit_mode, MediaFitMode::Contain);

        assert!(state.apply(SceneDiff::SetAudioPlayback(Some(AudioPlaybackState {
            title: "Walk-In Piano".into(),
            media_path: "C:\\Audio\\walkin.mp3".into(),
            playing: true,
            looping: true,
            volume: 0.8,
        }))));
        assert!(state.active_audio.is_some());

        let mut s_v1 = demo_deck()[0].clone();
        s_v1.id = SlideId::from_u128(2001);
        s_v1.group = "Verse 1".into();
        let mut s_ch = demo_deck()[1].clone();
        s_ch.id = SlideId::from_u128(2002);
        s_ch.group = "Chorus".into();
        let mut s_v2 = demo_deck()[2].clone();
        s_v2.id = SlideId::from_u128(2003);
        s_v2.group = "Verse 2".into();

        let arr = Arrangement {
            id: "arr-full".into(),
            name: "Full Band".into(),
            group_order: vec!["Verse 1".into(), "Chorus".into(), "Verse 2".into(), "Chorus".into()],
        };
        assert!(state.apply(SceneDiff::LoadPresentation {
            slides: vec![s_v1, s_ch, s_v2],
            arrangements: vec![arr.clone()],
            active_arrangement_id: Some("arr-full".into()),
        }));
        assert_eq!(state.master_deck.len(), 3);
        assert_eq!(state.deck.len(), 4);
        assert_ne!(state.deck[1].id, state.deck[3].id);

        assert!(!state.apply(SceneDiff::SelectArrangement(Some("missing".into()))));
        assert!(state.apply(SceneDiff::SelectArrangement(None)));
        assert_eq!(state.deck.len(), 3);
        assert!(state.apply(SceneDiff::SetArrangements {
            arrangements: vec![arr],
            active_id: Some("arr-full".into()),
        }));
        assert_eq!(state.deck.len(), 4);
        assert!(!state.apply(SceneDiff::Undo));
        assert!(!state.apply(SceneDiff::Redo));
    }

    #[test]
    fn manages_multi_overlays_locked_clear_messages_variables_and_slide_actions() {
        let mut state = SceneState::demo_m0(2);
        assert_eq!(state.overlays.len(), 2);
        assert_eq!(default_message_templates().len(), 2);

        // Toggle both unlocked & locked overlays active
        assert!(state.apply(SceneDiff::ToggleOverlayItem {
            id: "overlay-speaker-lt".into(),
            active: true,
        }));
        assert!(state.apply(SceneDiff::ToggleOverlayItem {
            id: "overlay-church-bug".into(),
            active: true,
        }));
        assert!(!state.apply(SceneDiff::ToggleOverlayItem {
            id: "missing".into(),
            active: true,
        }));
        assert_eq!(state.active_overlay_ids.len(), 2);

        // ClearAll clears unlocked overlays but keeps locked overlay active!
        assert!(state.apply(SceneDiff::ClearAll));
        assert_eq!(state.active_overlay_ids, vec!["overlay-church-bug".to_string()]);
        assert!(state.overlay_layer_active);

        // Upsert & Remove custom overlay (+ validation)
        let mut custom_ov = default_overlays()[0].clone();
        custom_ov.id = "ov-custom".into();
        custom_ov.name = "Give Online QR".into();
        assert!(state.apply(SceneDiff::UpsertOverlay(custom_ov.clone())));
        let mut bad_ov = custom_ov.clone();
        bad_ov.name = "  ".into();
        assert!(!state.apply(SceneDiff::UpsertOverlay(bad_ov)));
        assert!(state.apply(SceneDiff::RemoveOverlay("ov-custom".into())));
        assert!(!state.apply(SceneDiff::RemoveOverlay("ov-custom".into())));
        assert!(state.apply(SceneDiff::SyncOverlays(vec![custom_ov])));

        // Tokenized Message rendering & Layer 6 activation
        let tokens = BTreeMap::from([
            ("code".into(), "42".into()),
            ("room".into(), "B".into()),
        ]);
        let rendered = render_message_template(
            &default_message_templates()[0].template_text,
            &tokens,
        );
        assert_eq!(rendered, "Parent with Tag #42, please check Nursery Room B.");
        assert!(state.apply(SceneDiff::SetActiveMessage(Some(ActiveMessage {
            template_id: Some("msg-nursery".into()),
            rendered_text: rendered.clone(),
            show_on_audience: true,
            show_on_stage: true,
        }))));
        assert_eq!(state.stage_message.as_deref(), Some(rendered.as_str()));
        assert!(state.apply(SceneDiff::SetActiveMessage(None)));
        assert!(state.stage_message.is_none());

        // Dynamic Variables + token expansion
        assert!(state.apply(SceneDiff::SetVariable {
            key: "TeamA".into(),
            value: Some("14".into()),
        }));
        assert!(!state.apply(SceneDiff::SetVariable {
            key: "  ".into(),
            value: Some("x".into()),
        }));
        let expanded = resolve_dynamic_tokens(
            "Score: {{var:TeamA}} | {{timer}} | {{clock}} | {{ccli}}",
            &state.variables,
            Some("04:30"),
            Some("22025"),
            "10:15",
        );
        assert_eq!(expanded, "Score: 14 | 04:30 | 10:15 | 22025");
        assert!(state.apply(SceneDiff::SetVariable {
            key: "TeamA".into(),
            value: None,
        }));

        // Slide Automation + Actions triggered automatically on TriggerSlide
        let target_slide = state.deck[1].id;
        assert!(state.apply(SceneDiff::SetSlideAutomation {
            slide_id: target_slide,
            auto_advance_secs: Some(8.0),
            loop_to_first: true,
            actions: vec![
                SlideAction::ShowOverlay("overlay-speaker-lt".into()),
                SlideAction::SetVariable {
                    key: "Status".into(),
                    value: "Live".into(),
                },
                SlideAction::SetMask(MaskStyle::RoundedFrame),
                SlideAction::SetOutputLook {
                    output_id: OUTPUT_AUDIENCE_MAIN,
                    look_id: LOOK_BROADCAST_LOWER_THIRD,
                },
            ],
        }));
        assert!(!state.apply(SceneDiff::SetSlideAutomation {
            slide_id: SlideId::new(),
            auto_advance_secs: None,
            loop_to_first: false,
            actions: vec![],
        }));
        assert!(state.apply(SceneDiff::TriggerSlide {
            slide_id: target_slide,
            transition_override: None,
        }));
        assert!(state.active_overlay_ids.contains(&"overlay-speaker-lt".to_string()));
        assert_eq!(state.variables.get("Status").map(String::as_str), Some("Live"));
        assert_eq!(state.mask_style, MaskStyle::RoundedFrame);
        assert_eq!(
            state.look_for_output(OUTPUT_AUDIENCE_MAIN).map(|l| l.id),
            Some(LOOK_BROADCAST_LOWER_THIRD)
        );
    }
}
