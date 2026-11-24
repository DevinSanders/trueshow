//! Broadcast 2D typography and vector layer rasterizer for OpenPresenter.
//!
//! All consumers interact exclusively through [`Vector2DRasterizer`], keeping the active
//! rendering backend (`cosmic-text` + `tiny-skia` CPU rasterizer by default, with an optional
//! `skia-safe` backend boundary) decoupled from `core-scene` and `render-wgpu`.
//!
//! Slides rasterize on-demand into tightly packed **premultiplied RGBA8** buffers cached by
//! content hash so the `wgpu` compositor runs 60 fps transitions and scaling at 0% CPU.

use core_scene::{
    ColorRgba, RectF, Slide, SlideElement, SlideElementKind, StageLayoutConfig, TextAlign,
    TextStyle, VIRTUAL_CANVAS_HEIGHT, VIRTUAL_CANVAS_WIDTH, VerticalAlign,
};
use cosmic_text::{Attrs, Buffer, Family, FontSystem, Metrics, Shaping, Style, SwashCache, Weight};
use std::collections::{HashMap, VecDeque};
use tiny_skia::{Color, FillRule, Paint, PathBuilder, Pixmap, Stroke, Transform};

/// Tightly packed premultiplied RGBA8 raster output ready for `wgpu::Queue::write_texture`.
#[derive(Debug, Clone)]
pub struct RasterizedLayer {
    pub width: u32,
    pub height: u32,
    pub content_hash: [u8; 32],
    pub rgba_premul: Vec<u8>,
}

/// Caret hit-test rectangle in `1920×1080` virtual canvas coordinates for zero-drift slide editing.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CaretGeometry {
    pub x: f32,
    pub y: f32,
    pub height: f32,
    pub byte_index: usize,
}

/// Abstract 2D slide/typography rasterizer trait (ADR-004).
pub trait Vector2DRasterizer {
    fn rasterize_slide(
        &mut self,
        slide: &Slide,
        target_width: u32,
        target_height: u32,
        lower_third_override: bool,
    ) -> RasterizedLayer;

    #[allow(clippy::too_many_arguments)]
    fn rasterize_stage_confidence(
        &mut self,
        current: Option<&Slide>,
        next: Option<&Slide>,
        stage_message: Option<&str>,
        timer_readout: Option<&str>,
        layout: &StageLayoutConfig,
        target_width: u32,
        target_height: u32,
    ) -> RasterizedLayer;

    fn hit_test_caret(&mut self, element: &SlideElement, canvas_x: f32, canvas_y: f32) -> Option<CaretGeometry>;
}

/// Cached broadcast rasterizer using `cosmic-text` (Unicode/RTL/CJK shaping & fallback)
/// + `tiny-skia` (antialiased shapes, rounded pill backdrops, text outlines, and drop shadows).
pub struct BroadcastRasterizer {
    font_system: FontSystem,
    swash_cache: SwashCache,
    cache: HashMap<[u8; 32], RasterizedLayer>,
    lru_order: VecDeque<[u8; 32]>,
    max_cached_layers: usize,
}

impl Default for BroadcastRasterizer {
    fn default() -> Self {
        Self::new(64)
    }
}

impl BroadcastRasterizer {
    pub fn new(max_cached_layers: usize) -> Self {
        Self {
            font_system: FontSystem::new(),
            swash_cache: SwashCache::new(),
            cache: HashMap::new(),
            lru_order: VecDeque::new(),
            max_cached_layers: max_cached_layers.max(4),
        }
    }

    pub fn cached_count(&self) -> usize {
        self.cache.len()
    }

    fn insert_cache(&mut self, key: [u8; 32], layer: RasterizedLayer) {
        if self.cache.contains_key(&key) {
            return;
        }
        while self.lru_order.len() >= self.max_cached_layers {
            if let Some(evict) = self.lru_order.pop_front() {
                self.cache.remove(&evict);
            }
        }
        self.lru_order.push_back(key);
        self.cache.insert(key, layer);
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_rounded_rect(
        pixmap: &mut Pixmap,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        radius: f32,
        fill: ColorRgba,
        stroke: Option<(ColorRgba, f32)>,
    ) {
        if w <= 1.0 || h <= 1.0 {
            return;
        }
        let r = radius.min(w * 0.5).min(h * 0.5).max(0.0);
        let mut pb = PathBuilder::new();
        if r <= 0.5 {
            if let Some(rect) = tiny_skia::Rect::from_xywh(x, y, w, h) {
                pb.push_rect(rect);
            }
        } else {
            let k = r * 0.552_284_8;
            pb.move_to(x + r, y);
            pb.line_to(x + w - r, y);
            pb.cubic_to(x + w - r + k, y, x + w, y + r - k, x + w, y + r);
            pb.line_to(x + w, y + h - r);
            pb.cubic_to(x + w, y + h - r + k, x + w - r + k, y + h, x + w - r, y + h);
            pb.line_to(x + r, y + h);
            pb.cubic_to(x + r - k, y + h, x, y + h - r + k, x, y + h - r);
            pb.line_to(x, y + r);
            pb.cubic_to(x, y + r - k, x + r - k, y, x + r, y);
            pb.close();
        }

        let Some(path) = pb.finish() else {
            return;
        };

        if fill.a > 0.001 {
            let mut paint = Paint::default();
            paint.set_color(Color::from_rgba(fill.r, fill.g, fill.b, fill.a).unwrap_or(Color::TRANSPARENT));
            paint.anti_alias = true;
            pixmap.fill_path(&path, &paint, FillRule::Winding, Transform::identity(), None);
        }

        if let Some((sc, sw)) = stroke
            && sc.a > 0.001
            && sw > 0.1
        {
            let mut paint = Paint::default();
            paint.set_color(Color::from_rgba(sc.r, sc.g, sc.b, sc.a).unwrap_or(Color::TRANSPARENT));
            paint.anti_alias = true;
            let st = Stroke {
                width: sw,
                ..Stroke::default()
            };
            pixmap.stroke_path(&path, &paint, &st, Transform::identity(), None);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn render_text_box(
        &mut self,
        pixmap: &mut Pixmap,
        bounds: RectF,
        text: &str,
        style: &TextStyle,
        opacity: f32,
        scale_x: f32,
        scale_y: f32,
    ) {
        if text.trim().is_empty() || opacity <= 0.001 {
            return;
        }

        let px_x = bounds.x * scale_x;
        let px_y = bounds.y * scale_y;
        let px_w = (bounds.width * scale_x).max(8.0);
        let px_h = (bounds.height * scale_y).max(8.0);
        let font_scale = scale_x.min(scale_y);

        if style.backdrop_color.a > 0.001 {
            let mut bg = style.backdrop_color;
            bg.a *= opacity;
            Self::draw_rounded_rect(
                pixmap,
                px_x,
                px_y,
                px_w,
                px_h,
                style.backdrop_radius * font_scale,
                bg,
                None,
            );
        }

        let pad = if style.backdrop_color.a > 0.001 {
            style.backdrop_padding * font_scale
        } else {
            4.0 * font_scale
        };
        let inner_w = (px_w - pad * 2.0).max(8.0);
        let inner_h = (px_h - pad * 2.0).max(8.0);

        let mut font_size = (style.font_size * font_scale).max(10.0);
        let family = match style.font_family.to_lowercase().as_str() {
            "serif" => Family::Serif,
            "monospace" | "mono" => Family::Monospace,
            "sans-serif" | "sans" | "" => Family::SansSerif,
            _ => Family::Name(style.font_family.as_str()),
        };

        let attrs = Attrs::new()
            .family(family)
            .weight(if style.bold { Weight::BOLD } else { Weight::NORMAL })
            .style(if style.italic { Style::Italic } else { Style::Normal });

        let cosmic_align = match style.h_align {
            TextAlign::Left => cosmic_text::Align::Left,
            TextAlign::Center => cosmic_text::Align::Center,
            TextAlign::Right => cosmic_text::Align::Right,
        };

        // Auto-shrink loop so long worship stanzas never overflow the safe bounding box.
        let mut buffer = Buffer::new(
            &mut self.font_system,
            Metrics::new(font_size, (font_size * style.line_height).max(font_size + 2.0)),
        );
        let mut total_height = 0.0f32;
        let max_attempts = if style.auto_shrink { 6 } else { 1 };
        for _ in 0..max_attempts {
            let line_h = (font_size * style.line_height).max(font_size + 2.0);
            buffer.set_metrics(&mut self.font_system, Metrics::new(font_size, line_h));
            buffer.set_size(&mut self.font_system, Some(inner_w), Some(inner_h * 2.0));
            buffer.set_text(&mut self.font_system, text, &attrs, Shaping::Advanced);
            for line in buffer.lines.iter_mut() {
                line.set_align(Some(cosmic_align));
            }
            buffer.shape_until_scroll(&mut self.font_system, false);
            let run_count = buffer.layout_runs().count().max(1) as f32;
            total_height = run_count * line_h;
            if total_height <= inner_h || font_size <= 14.0 {
                break;
            }
            font_size = (font_size * 0.86).max(12.0);
        }

        let offset_y = match style.v_align {
            VerticalAlign::Top => px_y + pad,
            VerticalAlign::Middle => px_y + pad + ((inner_h - total_height) * 0.5).max(0.0),
            VerticalAlign::Bottom => px_y + pad + (inner_h - total_height).max(0.0),
        };
        let offset_x = px_x + pad;

        let width = pixmap.width() as i32;
        let height = pixmap.height() as i32;
        let data = pixmap.data_mut();

        // Collect rasterized glyph masks once so shadow, outline stroke, and main fill share work.
        struct PlacedGlyph {
            x: i32,
            y: i32,
            w: i32,
            h: i32,
            mask: Vec<u8>,
        }
        let mut glyphs = Vec::new();

        for run in buffer.layout_runs() {
            for glyph in run.glyphs.iter() {
                let physical = glyph.physical((offset_x, offset_y), 1.0);
                let Some(image) = self
                    .swash_cache
                    .get_image(&mut self.font_system, physical.cache_key)
                else {
                    continue;
                };
                if image.placement.width == 0 || image.placement.height == 0 {
                    continue;
                }
                let gx = physical.x + image.placement.left;
                let gy = physical.y + run.line_y.round() as i32 - image.placement.top;
                let gw = image.placement.width as i32;
                let gh = image.placement.height as i32;
                // Extract alpha mask (swash AlphaMask has 1 byte/px; Color has 4 bytes/px).
                let mask = if image.data.len() == (gw * gh) as usize {
                    image.data.clone()
                } else {
                    image
                        .data
                        .as_chunks::<4>()
                        .0
                        .iter()
                        .map(|c| c[3])
                        .collect::<Vec<u8>>()
                };
                glyphs.push(PlacedGlyph {
                    x: gx,
                    y: gy,
                    w: gw,
                    h: gh,
                    mask,
                });
            }
        }

        // Pass 1: Soft drop shadow (multi-tap offset approximation for crisp broadcast contrast)
        if style.shadow_color.a > 0.01 {
            let sx = (style.shadow_offset_x * font_scale).round() as i32;
            let sy = (style.shadow_offset_y * font_scale).round() as i32;
            let blur_r = (style.shadow_blur * font_scale * 0.25).round().clamp(0.0, 3.0) as i32;
            let mut sh = style.shadow_color;
            sh.a *= opacity;
            let taps: &[(i32, i32, f32)] = if blur_r >= 2 {
                &[
                    (0, 0, 0.45),
                    (-blur_r, 0, 0.20),
                    (blur_r, 0, 0.20),
                    (0, -blur_r, 0.20),
                    (0, blur_r, 0.20),
                ]
            } else {
                &[(0, 0, 1.0)]
            };
            for g in &glyphs {
                for &(dx, dy, weight) in taps {
                    let mut c = sh;
                    c.a *= weight;
                    blit_glyph_mask(data, width, height, g.x + sx + dx, g.y + sy + dy, g.w, g.h, &g.mask, c);
                }
            }
        }

        // Pass 2: Outside text outline / stroke (8-direction radial dilation around each glyph)
        let stroke_px = (style.stroke_width * font_scale).round().clamp(0.0, 6.0) as i32;
        if stroke_px > 0 && style.stroke_color.a > 0.01 {
            let mut sc = style.stroke_color;
            sc.a *= opacity;
            let offsets = [
                (-stroke_px, 0),
                (stroke_px, 0),
                (0, -stroke_px),
                (0, stroke_px),
                (-stroke_px, -stroke_px),
                (stroke_px, -stroke_px),
                (-stroke_px, stroke_px),
                (stroke_px, stroke_px),
            ];
            for g in &glyphs {
                for (dx, dy) in offsets {
                    blit_glyph_mask(data, width, height, g.x + dx, g.y + dy, g.w, g.h, &g.mask, sc);
                }
            }
        }

        // Pass 3: Foreground glyph fill
        let mut fg = style.color;
        fg.a *= opacity;
        for g in &glyphs {
            blit_glyph_mask(data, width, height, g.x, g.y, g.w, g.h, &g.mask, fg);
        }
    }
}

/// Premultiplied Porter-Duff `source-over` glyph mask blitter.
#[allow(clippy::too_many_arguments)]
fn blit_glyph_mask(
    dst_rgba: &mut [u8],
    dst_w: i32,
    dst_h: i32,
    gx: i32,
    gy: i32,
    gw: i32,
    gh: i32,
    mask: &[u8],
    color: ColorRgba,
) {
    let base_a = color.a.clamp(0.0, 1.0);
    if base_a <= 0.001 {
        return;
    }
    let cr = color.r.clamp(0.0, 1.0);
    let cg = color.g.clamp(0.0, 1.0);
    let cb = color.b.clamp(0.0, 1.0);

    for row in 0..gh {
        let py = gy + row;
        if py < 0 || py >= dst_h {
            continue;
        }
        for col in 0..gw {
            let px = gx + col;
            if px < 0 || px >= dst_w {
                continue;
            }
            let m = mask[(row * gw + col) as usize];
            if m == 0 {
                continue;
            }
            let sa = (f32::from(m) / 255.0) * base_a;
            let inv_sa = 1.0 - sa;
            let sr = cr * sa;
            let sg = cg * sa;
            let sb = cb * sa;

            let idx = ((py * dst_w + px) * 4) as usize;
            let dr = f32::from(dst_rgba[idx]) / 255.0;
            let dg = f32::from(dst_rgba[idx + 1]) / 255.0;
            let db = f32::from(dst_rgba[idx + 2]) / 255.0;
            let da = f32::from(dst_rgba[idx + 3]) / 255.0;

            dst_rgba[idx] = ((sr + dr * inv_sa).clamp(0.0, 1.0) * 255.0) as u8;
            dst_rgba[idx + 1] = ((sg + dg * inv_sa).clamp(0.0, 1.0) * 255.0) as u8;
            dst_rgba[idx + 2] = ((sb + db * inv_sa).clamp(0.0, 1.0) * 255.0) as u8;
            dst_rgba[idx + 3] = ((sa + da * inv_sa).clamp(0.0, 1.0) * 255.0) as u8;
        }
    }
}

fn hash_slide_key(slide: &Slide, w: u32, h: u32, lower_third: bool) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"slide-v1");
    hasher.update(&w.to_le_bytes());
    hasher.update(&h.to_le_bytes());
    hasher.update(&[u8::from(lower_third)]);
    hasher.update(slide.id.0.as_bytes());
    hasher.update(slide.headline.as_bytes());
    hasher.update(slide.body.as_bytes());
    for el in &slide.elements {
        hasher.update(el.id.0.as_bytes());
        hasher.update(&el.bounds.x.to_le_bytes());
        hasher.update(&el.bounds.y.to_le_bytes());
        hasher.update(&el.bounds.width.to_le_bytes());
        hasher.update(&el.bounds.height.to_le_bytes());
        hasher.update(&el.opacity.to_le_bytes());
        hasher.update(&el.z_index.to_le_bytes());
        match &el.kind {
            SlideElementKind::TextBox { text, style } => {
                hasher.update(b"T");
                hasher.update(text.as_bytes());
                hasher.update(style.font_family.as_bytes());
                hasher.update(&style.font_size.to_le_bytes());
                hasher.update(&[u8::from(style.bold), u8::from(style.italic)]);
                hasher.update(&style.color.to_u8_rgba());
                hasher.update(&style.stroke_color.to_u8_rgba());
                hasher.update(&style.stroke_width.to_le_bytes());
                hasher.update(&style.shadow_blur.to_le_bytes());
                hasher.update(&style.backdrop_color.to_u8_rgba());
                hasher.update(&[style.h_align as u8, style.v_align as u8]);
            }
            SlideElementKind::ShapeRect {
                fill,
                stroke,
                stroke_width,
                corner_radius,
            } => {
                hasher.update(b"R");
                hasher.update(&fill.to_u8_rgba());
                hasher.update(&stroke.to_u8_rgba());
                hasher.update(&stroke_width.to_le_bytes());
                hasher.update(&corner_radius.to_le_bytes());
            }
        }
    }
    *hasher.finalize().as_bytes()
}

impl Vector2DRasterizer for BroadcastRasterizer {
    fn rasterize_slide(
        &mut self,
        slide: &Slide,
        target_width: u32,
        target_height: u32,
        lower_third_override: bool,
    ) -> RasterizedLayer {
        let w = target_width.max(16);
        let h = target_height.max(16);
        let key = hash_slide_key(slide, w, h, lower_third_override);

        if let Some(hit) = self.cache.get(&key) {
            return hit.clone();
        }

        let mut pixmap = Pixmap::new(w, h).unwrap_or_else(|| {
            Pixmap::new(16, 16).expect("fallback pixmap")
        });

        let scale_x = w as f32 / VIRTUAL_CANVAS_WIDTH;
        let scale_y = h as f32 / VIRTUAL_CANVAS_HEIGHT;

        if lower_third_override {
            let text = slide.plain_text();
            let lt_bounds = RectF::new(120.0, 790.0, 1680.0, 200.0);
            let lt_style = TextStyle {
                font_size: 54.0,
                backdrop_color: ColorRgba::rgba(0.05, 0.08, 0.14, 0.88),
                backdrop_radius: 18.0,
                backdrop_padding: 26.0,
                stroke_width: 2.0,
                h_align: TextAlign::Center,
                v_align: VerticalAlign::Middle,
                ..TextStyle::default()
            };
            self.render_text_box(&mut pixmap, lt_bounds, &text, &lt_style, 1.0, scale_x, scale_y);
        } else {
            let mut sorted: Vec<&SlideElement> = slide.elements.iter().collect();
            sorted.sort_by_key(|e| e.z_index);

            for el in sorted {
                match &el.kind {
                    SlideElementKind::ShapeRect {
                        fill,
                        stroke,
                        stroke_width,
                        corner_radius,
                    } => {
                        let mut f = *fill;
                        f.a *= el.opacity;
                        let mut s = *stroke;
                        s.a *= el.opacity;
                        Self::draw_rounded_rect(
                            &mut pixmap,
                            el.bounds.x * scale_x,
                            el.bounds.y * scale_y,
                            el.bounds.width * scale_x,
                            el.bounds.height * scale_y,
                            corner_radius * scale_x.min(scale_y),
                            f,
                            Some((s, stroke_width * scale_x.min(scale_y))),
                        );
                    }
                    SlideElementKind::TextBox { text, style } => {
                        self.render_text_box(
                            &mut pixmap,
                            el.bounds,
                            text,
                            style,
                            el.opacity,
                            scale_x,
                            scale_y,
                        );
                    }
                }
            }
        }

        let layer = RasterizedLayer {
            width: pixmap.width(),
            height: pixmap.height(),
            content_hash: key,
            rgba_premul: pixmap.take(),
        };
        self.insert_cache(key, layer.clone());
        layer
    }

    #[allow(clippy::too_many_arguments)]
    fn rasterize_stage_confidence(
        &mut self,
        current: Option<&Slide>,
        next: Option<&Slide>,
        stage_message: Option<&str>,
        timer_readout: Option<&str>,
        layout: &StageLayoutConfig,
        target_width: u32,
        target_height: u32,
    ) -> RasterizedLayer {
        let w = target_width.max(16);
        let h = target_height.max(16);

        let mut hasher = blake3::Hasher::new();
        hasher.update(b"stage-v1");
        hasher.update(&w.to_le_bytes());
        hasher.update(&h.to_le_bytes());
        if let Some(c) = current {
            hasher.update(c.plain_text().as_bytes());
            if let Some(ch) = &c.chords_line {
                hasher.update(ch.as_bytes());
            }
        }
        if let Some(n) = next {
            hasher.update(n.plain_text().as_bytes());
        }
        if let Some(m) = stage_message {
            hasher.update(m.as_bytes());
        }
        if let Some(t) = timer_readout {
            hasher.update(t.as_bytes());
        }
        let key = *hasher.finalize().as_bytes();
        if let Some(hit) = self.cache.get(&key) {
            return hit.clone();
        }

        let mut pixmap = Pixmap::new(w, h).unwrap_or_else(|| {
            Pixmap::new(16, 16).expect("fallback pixmap")
        });
        let scale_x = w as f32 / VIRTUAL_CANVAS_WIDTH;
        let scale_y = h as f32 / VIRTUAL_CANVAS_HEIGHT;

        // Top timer / status readout
        if layout.show_timer
            && let Some(timer) = timer_readout
        {
            let style = TextStyle {
                font_size: 62.0,
                color: ColorRgba::rgb(0.28, 0.94, 0.58),
                stroke_width: 0.0,
                shadow_color: ColorRgba::TRANSPARENT,
                h_align: TextAlign::Right,
                v_align: VerticalAlign::Middle,
                ..TextStyle::default()
            };
            self.render_text_box(
                &mut pixmap,
                RectF::new(1380.0, 12.0, 480.0, 96.0),
                timer,
                &style,
                1.0,
                scale_x,
                scale_y,
            );
        }

        // Current slide label & chords
        if layout.show_current_slide
            && let Some(curr) = current
        {
            if layout.show_chords
                && let Some(chords) = &curr.chords_line
            {
                let chord_style = TextStyle {
                    font_family: "monospace".to_string(),
                    font_size: 40.0,
                    color: ColorRgba::rgb(0.98, 0.82, 0.32),
                    stroke_width: 0.0,
                    shadow_color: ColorRgba::TRANSPARENT,
                    h_align: TextAlign::Center,
                    v_align: VerticalAlign::Middle,
                    ..TextStyle::default()
                };
                self.render_text_box(
                    &mut pixmap,
                    RectF::new(120.0, 150.0, 1680.0, 84.0),
                    chords,
                    &chord_style,
                    1.0,
                    scale_x,
                    scale_y,
                );
            }

            let curr_style = TextStyle {
                font_size: 72.0,
                color: ColorRgba::WHITE,
                stroke_width: 1.5,
                shadow_color: ColorRgba::TRANSPARENT,
                h_align: TextAlign::Center,
                v_align: VerticalAlign::Middle,
                ..TextStyle::default()
            };
            self.render_text_box(
                &mut pixmap,
                RectF::new(120.0, 230.0, 1680.0, 430.0),
                &curr.plain_text(),
                &curr_style,
                1.0,
                scale_x,
                scale_y,
            );
        }

        // Next slide preview band
        if layout.show_next_slide
            && let Some(nxt) = next
        {
            let next_text = format!("NEXT: {}", nxt.plain_text().replace('\n', " / "));
            let next_style = TextStyle {
                font_size: 44.0,
                bold: false,
                color: ColorRgba::rgb(0.68, 0.78, 0.90),
                stroke_width: 0.0,
                shadow_color: ColorRgba::TRANSPARENT,
                h_align: TextAlign::Left,
                v_align: VerticalAlign::Middle,
                ..TextStyle::default()
            };
            self.render_text_box(
                &mut pixmap,
                RectF::new(120.0, 760.0, 1680.0, 190.0),
                &next_text,
                &next_style,
                1.0,
                scale_x,
                scale_y,
            );
        }

        // Urgent Stage Message banner (e.g. "Nursery #14" or "Wrap up in 2 min")
        if layout.show_stage_message
            && let Some(msg) = stage_message
            && !msg.trim().is_empty()
        {
            let msg_style = TextStyle {
                font_size: 52.0,
                color: ColorRgba::WHITE,
                backdrop_color: ColorRgba::rgba(0.84, 0.16, 0.16, 0.95),
                backdrop_radius: 14.0,
                backdrop_padding: 18.0,
                stroke_width: 0.0,
                shadow_color: ColorRgba::TRANSPARENT,
                ..TextStyle::default()
            };
            self.render_text_box(
                &mut pixmap,
                RectF::new(160.0, 930.0, 1600.0, 120.0),
                msg,
                &msg_style,
                1.0,
                scale_x,
                scale_y,
            );
        }

        let layer = RasterizedLayer {
            width: pixmap.width(),
            height: pixmap.height(),
            content_hash: key,
            rgba_premul: pixmap.take(),
        };
        self.insert_cache(key, layer.clone());
        layer
    }

    fn hit_test_caret(
        &mut self,
        element: &SlideElement,
        canvas_x: f32,
        canvas_y: f32,
    ) -> Option<CaretGeometry> {
        let SlideElementKind::TextBox { text, style } = &element.kind else {
            return None;
        };
        let b = element.bounds;
        if canvas_x < b.x || canvas_x > b.x + b.width || canvas_y < b.y || canvas_y > b.y + b.height {
            return None;
        }
        let rel_x = (canvas_x - b.x).clamp(0.0, b.width);
        let rel_y = (canvas_y - b.y).clamp(0.0, b.height);
        let line_h = (style.font_size * style.line_height).max(16.0);

        let mut buffer = Buffer::new(&mut self.font_system, Metrics::new(style.font_size, line_h));
        buffer.set_size(&mut self.font_system, Some(b.width), Some(b.height));
        buffer.set_text(&mut self.font_system, text, &Attrs::new(), Shaping::Advanced);
        buffer.shape_until_scroll(&mut self.font_system, false);

        if let Some(cursor) = buffer.hit(rel_x, rel_y) {
            let line_offset: usize = text
                .lines()
                .take(cursor.line)
                .map(|l| l.len() + 1)
                .sum();
            let byte_index = (line_offset + cursor.index).min(text.len());
            return Some(CaretGeometry {
                x: canvas_x,
                y: b.y + (cursor.line as f32) * line_h,
                height: line_h,
                byte_index,
            });
        }

        Some(CaretGeometry {
            x: b.x + rel_x,
            y: b.y,
            height: line_h,
            byte_index: text.len(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core_scene::demo_deck;

    #[test]
    fn rasterizes_slide_and_hits_lru_cache() {
        let mut r = BroadcastRasterizer::new(8);
        let deck = demo_deck();
        let slide = &deck[1];

        let first = r.rasterize_slide(slide, 640, 360, false);
        assert_eq!(first.width, 640);
        assert_eq!(first.height, 360);
        assert_eq!(first.rgba_premul.len(), 640 * 360 * 4);
        assert_eq!(r.cached_count(), 1);

        let second = r.rasterize_slide(slide, 640, 360, false);
        assert_eq!(first.content_hash, second.content_hash);
        assert_eq!(r.cached_count(), 1);

        // Lower-third override produces a distinct cached hash and non-empty backdrop pixels.
        let lt = r.rasterize_slide(slide, 640, 360, true);
        assert_ne!(first.content_hash, lt.content_hash);
        assert!(lt.rgba_premul.iter().any(|&b| b > 0));
        assert_eq!(r.cached_count(), 2);
    }

    #[test]
    fn rasterizes_stage_confidence_with_message_and_timer() {
        let mut r = BroadcastRasterizer::default();
        let deck = demo_deck();
        let out = r.rasterize_stage_confidence(
            Some(&deck[1]),
            Some(&deck[2]),
            Some("Nursery #14 needed"),
            Some("04:32"),
            &StageLayoutConfig::default(),
            640,
            360,
        );
        assert_eq!(out.rgba_premul.len(), 640 * 360 * 4);
        assert!(out.rgba_premul.iter().any(|&b| b > 0));
    }

    #[test]
    fn hit_tests_caret_inside_text_element_bounds() {
        let mut r = BroadcastRasterizer::default();
        let deck = demo_deck();
        let el = &deck[1].elements[0];
        assert!(r.hit_test_caret(el, 10.0, 10.0).is_none());
        let hit = r
            .hit_test_caret(el, el.bounds.x + 120.0, el.bounds.y + 40.0)
            .expect("point inside bounds should hit");
        assert!(hit.height > 10.0);
    }

    #[test]
    fn covers_shapes_lru_eviction_alignments_auto_shrink_and_empty_stage() {
        let mut r = BroadcastRasterizer::new(4);
        let mut slide = demo_deck()[0].clone();
        slide.elements = vec![
            SlideElement {
                id: core_scene::ElementId::new(),
                name: "Rounded Box".into(),
                bounds: RectF::new(100.0, 100.0, 400.0, 200.0),
                opacity: 1.0,
                z_index: 1,
                kind: SlideElementKind::ShapeRect {
                    fill: ColorRgba::rgba(0.1, 0.2, 0.4, 0.9),
                    stroke: ColorRgba::WHITE,
                    stroke_width: 4.0,
                    corner_radius: 16.0,
                },
            },
            SlideElement {
                id: core_scene::ElementId::new(),
                name: "Sharp Box".into(),
                bounds: RectF::new(520.0, 100.0, 120.0, 80.0),
                opacity: 1.0,
                z_index: 2,
                kind: SlideElementKind::ShapeRect {
                    fill: ColorRgba::rgb(0.2, 0.5, 0.2),
                    stroke: ColorRgba::TRANSPARENT,
                    stroke_width: 0.0,
                    corner_radius: 0.0,
                },
            },
            SlideElement {
                id: core_scene::ElementId::new(),
                name: "Long Italic Serif".into(),
                bounds: RectF::new(100.0, 340.0, 420.0, 120.0),
                opacity: 1.0,
                z_index: 3,
                kind: SlideElementKind::TextBox {
                    text: "A very long lyric paragraph that forces auto-shrink across multiple wrapped lines inside a compact box.".into(),
                    style: TextStyle {
                        font_family: "serif".into(),
                        font_size: 72.0,
                        bold: false,
                        italic: true,
                        h_align: TextAlign::Right,
                        v_align: VerticalAlign::Bottom,
                        shadow_blur: 2.0,
                        backdrop_color: ColorRgba::rgba(0.0, 0.0, 0.0, 0.5),
                        auto_shrink: true,
                        ..TextStyle::default()
                    },
                },
            },
        ];

        assert!(r.hit_test_caret(&slide.elements[0], 150.0, 150.0).is_none());

        // Trigger LRU cache eviction (> 4 unique slides)
        for i in 0..6u32 {
            slide.headline = format!("Slide {i}");
            let out = r.rasterize_slide(&slide, 320, 180, false);
            assert!(out.rgba_premul.iter().any(|&b| b > 0));
        }
        assert!(r.cached_count() <= 4);

        // Stage confidence with empty current/next slides still succeeds
        let empty_stage = r.rasterize_stage_confidence(
            None,
            None,
            None,
            None,
            &StageLayoutConfig::default(),
            320,
            180,
        );
        assert_eq!(empty_stage.rgba_premul.len(), 320 * 180 * 4);
    }
}
