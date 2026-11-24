// OpenPresenter M1 7-Layer GPU Compositor Shader
// Composites motion backgrounds, Dissolve/Push/Wipe transitions, real rasterized
// premultiplied RGBA8 slide/stage text textures, blend modes, masks, and overlays.

struct CompositorUniforms {
    time_sec: f32,
    blend_t: f32,
    aspect: f32,
    flags: u32,
    curr_primary: vec4<f32>,
    curr_secondary: vec4<f32>,
    curr_accent: vec4<f32>,
    prev_primary: vec4<f32>,
    prev_secondary: vec4<f32>,
    prev_accent: vec4<f32>,
    curr_style: u32,
    prev_style: u32,
    transition_kind: u32,
    mask_style: u32,
    blend_mode: u32,
    _pad0: u32,
    _pad1: u32,
    _pad2: u32,
};

@group(0) @binding(0)
var<uniform> u: CompositorUniforms;

@group(0) @binding(1)
var slide_tex: texture_2d<f32>;

@group(0) @binding(2)
var slide_sampler: sampler;

struct VertexOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) idx: u32) -> VertexOut {
    var positions = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -3.0),
        vec2<f32>(3.0, 1.0),
        vec2<f32>(-1.0, 1.0),
    );
    let p = positions[idx];
    var out: VertexOut;
    out.pos = vec4<f32>(p, 0.0, 1.0);
    out.uv = vec2<f32>(p.x * 0.5 + 0.5, 0.5 - p.y * 0.5);
    return out;
}

fn eval_style(
    uv: vec2<f32>,
    t: f32,
    style: u32,
    c1: vec4<f32>,
    c2: vec4<f32>,
    accent: vec4<f32>,
) -> vec4<f32> {
    if style == 0u {
        return c1;
    }

    if style == 1u {
        let wave1 = sin(uv.x * 3.8 + t * 0.75 + sin(uv.y * 2.5 - t * 0.45));
        let wave2 = cos(uv.y * 4.4 - t * 0.55 + cos(uv.x * 2.2 + t * 0.35));
        let blend = clamp(0.5 + 0.32 * wave1 + 0.22 * wave2, 0.0, 1.0);
        let ribbon = smoothstep(0.72, 0.98, sin((uv.x * 2.4 + uv.y * 1.6) * 3.14159 + t * 0.9) * 0.5 + 0.5);
        var col = mix(c1.rgb, c2.rgb, blend);
        col = mix(col, accent.rgb, ribbon * 0.22);
        let d = distance(uv, vec2<f32>(0.5, 0.5));
        col *= 1.0 - smoothstep(0.35, 0.95, d) * 0.28;
        return vec4<f32>(col, 1.0);
    }

    if style == 2u {
        let center1 = vec2<f32>(0.5 + 0.16 * sin(t * 0.45), 0.46 + 0.10 * cos(t * 0.35));
        let center2 = vec2<f32>(0.5 - 0.18 * cos(t * 0.30), 0.58 + 0.12 * sin(t * 0.50));
        let g1 = exp(-distance(uv * vec2<f32>(u.aspect, 1.0), center1 * vec2<f32>(u.aspect, 1.0)) * 2.4);
        let g2 = exp(-distance(uv * vec2<f32>(u.aspect, 1.0), center2 * vec2<f32>(u.aspect, 1.0)) * 3.2);
        var col = mix(c1.rgb, c2.rgb, clamp(g1 * 1.15, 0.0, 1.0));
        col += accent.rgb * g2 * 0.42;
        return vec4<f32>(clamp(col, vec3<f32>(0.0), vec3<f32>(1.0)), 1.0);
    }

    if style == 3u {
        var col = mix(c1.rgb, c2.rgb, uv.y);
        let gx = abs(fract(uv.x * 16.0) - 0.5);
        let gy = abs(fract(uv.y * 9.0) - 0.5);
        if gx > 0.485 || gy > 0.485 {
            col = mix(col, vec3<f32>(0.45, 0.52, 0.60), 0.35);
        }
        let in_title_border =
            (abs(uv.x - 0.10) < 0.0025 || abs(uv.x - 0.90) < 0.0025) && uv.y >= 0.10 && uv.y <= 0.90 ||
            (abs(uv.y - 0.10) < 0.004 || abs(uv.y - 0.90) < 0.004) && uv.x >= 0.10 && uv.x <= 0.90;
        if in_title_border {
            col = accent.rgb;
        }
        let sweep_x = fract(t * 0.25);
        let bar = exp(-abs(uv.x - sweep_x) * 90.0);
        col = mix(col, accent.rgb, bar * 0.7);
        return vec4<f32>(col, 1.0);
    }

    let in_band = uv.y >= 0.74 && uv.y <= 0.94 && uv.x >= 0.06 && uv.x <= 0.94;
    if in_band {
        let edge = smoothstep(0.06, 0.075, uv.x) * (1.0 - smoothstep(0.925, 0.94, uv.x));
        let accent_bar = select(0.0, 1.0, uv.x < 0.072);
        let col = mix(c2.rgb, accent.rgb, accent_bar);
        return vec4<f32>(col, c2.a * edge);
    }
    return vec4<f32>(0.0, 0.0, 0.0, 0.0);
}

fn composite_premul(dst: vec4<f32>, src: vec4<f32>, mode: u32) -> vec4<f32> {
    var blended_rgb = src.rgb + dst.rgb * (1.0 - src.a);
    if mode == 1u {
        // Multiply
        blended_rgb = mix(dst.rgb, dst.rgb * src.rgb, src.a);
    } else if mode == 2u {
        // Screen
        let scr = vec3<f32>(1.0) - (vec3<f32>(1.0) - dst.rgb) * (vec3<f32>(1.0) - src.rgb);
        blended_rgb = mix(dst.rgb, scr, src.a);
    } else if mode == 3u {
        // Additive
        blended_rgb = clamp(dst.rgb + src.rgb, vec3<f32>(0.0), vec3<f32>(1.0));
    }
    let out_a = src.a + dst.a * (1.0 - src.a);
    return vec4<f32>(blended_rgb, out_a);
}

fn eval_mask(uv: vec2<f32>, mask_style: u32) -> f32 {
    if mask_style == 0u {
        return 1.0;
    }
    if mask_style == 1u {
        // SoftVignette
        let d = distance(uv, vec2<f32>(0.5, 0.5));
        return clamp(1.0 - smoothstep(0.42, 0.78, d), 0.0, 1.0);
    }
    if mask_style == 2u {
        // RoundedFrame
        let q = abs(uv - vec2<f32>(0.5, 0.5)) - vec2<f32>(0.42, 0.40);
        let dist = length(max(q, vec2<f32>(0.0))) - 0.04;
        return 1.0 - smoothstep(-0.005, 0.008, dist);
    }
    // LowerThirdBand mask
    return smoothstep(0.70, 0.74, uv.y);
}

@fragment
fn fs_main(in: VertexOut) -> @location(0) vec4<f32> {
    let blackout = (u.flags & 1u) != 0u;
    if blackout {
        return vec4<f32>(0.0, 0.0, 0.0, 1.0);
    }

    let media_active = (u.flags & 2u) != 0u;
    let slide_active = (u.flags & 4u) != 0u;
    let overlay_active = (u.flags & 8u) != 0u;
    let stage_mode = (u.flags & 32u) != 0u;
    let transparent_bg = (u.flags & 64u) != 0u;

    let smooth_blend = smoothstep(0.0, 1.0, clamp(u.blend_t, 0.0, 1.0));
    let slide_sample = textureSample(slide_tex, slide_sampler, in.uv);

    // Stage Confidence Mode: crisp dark backdrop + live rasterized Stage Confidence typography
    if stage_mode {
        var bg = vec3<f32>(0.04, 0.05, 0.08);
        let accent = mix(u.prev_accent.rgb, u.curr_accent.rgb, smooth_blend);
        if in.uv.y < 0.11 {
            bg = vec3<f32>(0.08, 0.10, 0.14);
            if in.uv.y > 0.102 {
                bg = accent;
            }
        } else if in.uv.y > 0.68 && in.uv.y < 0.92 {
            bg = vec3<f32>(0.07, 0.08, 0.12);
        }
        let pulse = 0.5 + 0.5 * sin(u.time_sec * 4.0);
        let dot_d = distance(in.uv * vec2<f32>(u.aspect, 1.0), vec2<f32>(0.04 * u.aspect, 0.055));
        if dot_d < 0.016 {
            bg = mix(vec3<f32>(0.1, 0.65, 0.32), vec3<f32>(0.25, 1.0, 0.55), pulse);
        }
        let composed = composite_premul(vec4<f32>(bg, 1.0), slide_sample, 0u);
        return vec4<f32>(composed.rgb, 1.0);
    }

    // Layer 1: Media / Motion Background with Cut / Dissolve / PushLeft / WipeRight transitions
    var out_col = vec4<f32>(0.0, 0.0, 0.0, select(1.0, 0.0, transparent_bg));
    if media_active && !transparent_bg {
        if u.transition_kind == 1u && smooth_blend < 0.999 {
            // PushLeft
            let split = 1.0 - smooth_blend;
            if in.uv.x < split {
                let p_uv = vec2<f32>(in.uv.x + smooth_blend, in.uv.y);
                out_col = eval_style(p_uv, u.time_sec, u.prev_style, u.prev_primary, u.prev_secondary, u.prev_accent);
            } else {
                let c_uv = vec2<f32>(in.uv.x - split, in.uv.y);
                out_col = eval_style(c_uv, u.time_sec, u.curr_style, u.curr_primary, u.curr_secondary, u.curr_accent);
            }
        } else if u.transition_kind == 2u && smooth_blend < 0.999 {
            // WipeRight
            let edge = smoothstep(smooth_blend - 0.04, smooth_blend + 0.04, in.uv.x);
            let prev_px = eval_style(in.uv, u.time_sec, u.prev_style, u.prev_primary, u.prev_secondary, u.prev_accent);
            let curr_px = eval_style(in.uv, u.time_sec, u.curr_style, u.curr_primary, u.curr_secondary, u.curr_accent);
            out_col = mix(curr_px, prev_px, edge);
        } else {
            let prev_px = eval_style(in.uv, u.time_sec, u.prev_style, u.prev_primary, u.prev_secondary, u.prev_accent);
            let curr_px = eval_style(in.uv, u.time_sec, u.curr_style, u.curr_primary, u.curr_secondary, u.curr_accent);
            out_col = mix(prev_px, curr_px, smooth_blend);
        }
    }

    // Layer 2: Slide Typography & Vector Elements (real premultiplied RGBA8 texture from render-text)
    if slide_active {
        let fade_slide = vec4<f32>(slide_sample.rgb * smooth_blend, slide_sample.a * smooth_blend);
        out_col = composite_premul(out_col, fade_slide, u.blend_mode);
    }

    // Layer 4: Props & Overlays (live corner bug indicator)
    if overlay_active {
        let accent = mix(u.prev_accent.rgb, u.curr_accent.rgb, smooth_blend);
        let bug_d = distance(in.uv * vec2<f32>(u.aspect, 1.0), vec2<f32>(0.93 * u.aspect, 0.09));
        if bug_d < 0.024 {
            let ring = smoothstep(0.017, 0.023, bug_d);
            let col = mix(accent, vec3<f32>(1.0, 1.0, 1.0), ring * 0.7);
            out_col = vec4<f32>(col, 1.0);
        }
    }

    // Layer 5: Masks
    let m = eval_mask(in.uv, u.mask_style);
    if transparent_bg {
        out_col = vec4<f32>(out_col.rgb * m, out_col.a * m);
    } else {
        out_col = vec4<f32>(out_col.rgb * m, 1.0);
    }

    return out_col;
}
