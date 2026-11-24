// OpenPresenter M0 GPU Compositor Shader
// Renders animated motion backgrounds, smooth crossfade transitions,
// broadcast lower-third alpha bands, and Stage confidence layouts.

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
    _pad0: u32,
    _pad1: u32,
};

@group(0) @binding(0)
var<uniform> u: CompositorUniforms;

struct VertexOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) idx: u32) -> VertexOut {
    // Oversized full-screen triangle (no vertex buffer required).
    var positions = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -3.0),
        vec2<f32>(3.0, 1.0),
        vec2<f32>(-1.0, 1.0),
    );
    let p = positions[idx];
    var out: VertexOut;
    out.pos = vec4<f32>(p, 0.0, 1.0);
    // Flip Y so uv=(0,0) is top-left and uv=(1,1) is bottom-right.
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
        // Solid fill
        return c1;
    }

    if style == 1u {
        // AuroraWave: layered drifting sine ribbons
        let wave1 = sin(uv.x * 3.8 + t * 0.75 + sin(uv.y * 2.5 - t * 0.45));
        let wave2 = cos(uv.y * 4.4 - t * 0.55 + cos(uv.x * 2.2 + t * 0.35));
        let blend = clamp(0.5 + 0.32 * wave1 + 0.22 * wave2, 0.0, 1.0);
        let ribbon = smoothstep(0.72, 0.98, sin((uv.x * 2.4 + uv.y * 1.6) * 3.14159 + t * 0.9) * 0.5 + 0.5);
        var col = mix(c1.rgb, c2.rgb, blend);
        col = mix(col, accent.rgb, ribbon * 0.22);
        // Subtle vignette for lyric legibility
        let d = distance(uv, vec2<f32>(0.5, 0.5));
        col *= 1.0 - smoothstep(0.35, 0.95, d) * 0.28;
        return vec4<f32>(col, 1.0);
    }

    if style == 2u {
        // SanctuaryGlow: warm breathing radial light fields
        let center1 = vec2<f32>(0.5 + 0.16 * sin(t * 0.45), 0.46 + 0.10 * cos(t * 0.35));
        let center2 = vec2<f32>(0.5 - 0.18 * cos(t * 0.30), 0.58 + 0.12 * sin(t * 0.50));
        let g1 = exp(-distance(uv * vec2<f32>(u.aspect, 1.0), center1 * vec2<f32>(u.aspect, 1.0)) * 2.4);
        let g2 = exp(-distance(uv * vec2<f32>(u.aspect, 1.0), center2 * vec2<f32>(u.aspect, 1.0)) * 3.2);
        var col = mix(c1.rgb, c2.rgb, clamp(g1 * 1.15, 0.0, 1.0));
        col += accent.rgb * g2 * 0.42;
        return vec4<f32>(clamp(col, vec3<f32>(0.0), vec3<f32>(1.0)), 1.0);
    }

    if style == 3u {
        // BroadcastGrid: calibration grid + action/title safe frames + moving scanline
        var col = mix(c1.rgb, c2.rgb, uv.y);
        let gx = abs(fract(uv.x * 16.0) - 0.5);
        let gy = abs(fract(uv.y * 9.0) - 0.5);
        if gx > 0.485 || gy > 0.485 {
            col = mix(col, vec3<f32>(0.45, 0.52, 0.60), 0.35);
        }
        // Title-safe box (10% margin)
        let in_title_border =
            (abs(uv.x - 0.10) < 0.0025 || abs(uv.x - 0.90) < 0.0025) && uv.y >= 0.10 && uv.y <= 0.90 ||
            (abs(uv.y - 0.10) < 0.004 || abs(uv.y - 0.90) < 0.004) && uv.x >= 0.10 && uv.x <= 0.90;
        if in_title_border {
            col = accent.rgb;
        }
        // Sweeping 60 fps verification bar
        let sweep_x = fract(t * 0.25);
        let bar = exp(-abs(uv.x - sweep_x) * 90.0);
        col = mix(col, accent.rgb, bar * 0.7);
        return vec4<f32>(col, 1.0);
    }

    // Style 4: LowerThirdAlpha — transparent upper frame with crisp lower-third graphic band
    let in_band = uv.y >= 0.74 && uv.y <= 0.92 && uv.x >= 0.06 && uv.x <= 0.94;
    if in_band {
        let edge = smoothstep(0.06, 0.075, uv.x) * (1.0 - smoothstep(0.925, 0.94, uv.x));
        let accent_bar = select(0.0, 1.0, uv.x < 0.072);
        let col = mix(c2.rgb, accent.rgb, accent_bar);
        return vec4<f32>(col, c2.a * edge);
    }
    return vec4<f32>(0.0, 0.0, 0.0, 0.0);
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
    let lower_third_mode = (u.flags & 16u) != 0u;
    let stage_mode = (u.flags & 32u) != 0u;
    let transparent_bg = (u.flags & 64u) != 0u;

    let smooth_blend = smoothstep(0.0, 1.0, clamp(u.blend_t, 0.0, 1.0));

    // Stage confidence layout: high-contrast dark background + live cue accent strip + progress pulse
    if stage_mode {
        var col = vec3<f32>(0.04, 0.05, 0.07);
        let curr_tint = mix(u.prev_accent.rgb, u.curr_accent.rgb, smooth_blend);
        // Top header status bar
        if in.uv.y < 0.09 {
            col = vec3<f32>(0.09, 0.11, 0.15);
            if in.uv.y > 0.082 {
                col = curr_tint;
            }
        }
        // Main current-slide confidence box
        let in_curr_box = in.uv.x > 0.05 && in.uv.x < 0.95 && in.uv.y > 0.14 && in.uv.y < 0.64;
        if in_curr_box && slide_active {
            let box_bg = mix(u.prev_primary.rgb, u.curr_primary.rgb, smooth_blend);
            col = mix(vec3<f32>(0.10, 0.12, 0.16), box_bg, 0.65);
            // Simulated centered lyric bars (replaced by Skia text texture in M1)
            let lyric_band = (abs(in.uv.y - 0.34) < 0.025 || abs(in.uv.y - 0.44) < 0.025)
                && in.uv.x > 0.18 && in.uv.x < 0.82;
            if lyric_band {
                col = vec3<f32>(0.94, 0.96, 1.0);
            }
        }
        // Next-slide confidence box
        let in_next_box = in.uv.x > 0.05 && in.uv.x < 0.95 && in.uv.y > 0.70 && in.uv.y < 0.94;
        if in_next_box {
            col = vec3<f32>(0.08, 0.09, 0.12);
            let next_band = abs(in.uv.y - 0.82) < 0.018 && in.uv.x > 0.22 && in.uv.x < 0.72;
            if next_band {
                col = vec3<f32>(0.55, 0.62, 0.70);
            }
        }
        // Live 60 fps heartbeat ticker dot in top-right
        let pulse = 0.5 + 0.5 * sin(u.time_sec * 4.0);
        let dot_d = distance(in.uv * vec2<f32>(u.aspect, 1.0), vec2<f32>(0.95 * u.aspect, 0.045));
        if dot_d < 0.016 {
            col = mix(vec3<f32>(0.1, 0.6, 0.3), vec3<f32>(0.2, 1.0, 0.55), pulse);
        }
        return vec4<f32>(col, 1.0);
    }

    // Audience / Broadcast Look composition
    var out_col = vec4<f32>(0.0, 0.0, 0.0, select(1.0, 0.0, transparent_bg));

    if media_active && !transparent_bg {
        let prev_px = eval_style(in.uv, u.time_sec, u.prev_style, u.prev_primary, u.prev_secondary, u.prev_accent);
        let curr_px = eval_style(in.uv, u.time_sec, u.curr_style, u.curr_primary, u.curr_secondary, u.curr_accent);
        out_col = mix(prev_px, curr_px, smooth_blend);
    }

    let accent = mix(u.prev_accent.rgb, u.curr_accent.rgb, smooth_blend);

    // Slide layer placeholder geometry (replaced by Skia cached RGBA8 layer texture in M1)
    if slide_active {
        if lower_third_mode {
            let in_lt = in.uv.y >= 0.76 && in.uv.y <= 0.92 && in.uv.x >= 0.08 && in.uv.x <= 0.92;
            if in_lt {
                let lt_bg = vec4<f32>(0.06, 0.08, 0.13, 0.88);
                let line_mask = (abs(in.uv.y - 0.82) < 0.014 && in.uv.x > 0.14 && in.uv.x < 0.76)
                    || (abs(in.uv.y - 0.87) < 0.010 && in.uv.x > 0.14 && in.uv.x < 0.58);
                let fg = select(lt_bg.rgb, vec3<f32>(0.97, 0.98, 1.0), line_mask);
                let strip = select(fg, accent, in.uv.x < 0.092);
                out_col = vec4<f32>(mix(out_col.rgb, strip, lt_bg.a), max(out_col.a, lt_bg.a));
            }
        } else {
            // Centered lyric placeholder card with subtle drop-shadow feel
            let card_d = max(abs(in.uv.x - 0.5) - 0.34, abs(in.uv.y - 0.5) - 0.16);
            if card_d < 0.0 {
                let card_bg = vec3<f32>(0.02, 0.03, 0.06);
                out_col = vec4<f32>(mix(out_col.rgb, card_bg, 0.42), 1.0);
                // Top accent rule
                if abs(in.uv.y - 0.365) < 0.005 && abs(in.uv.x - 0.5) < 0.18 {
                    out_col = vec4<f32>(accent, 1.0);
                }
                // Simulated headline & subline bars
                let h1 = abs(in.uv.y - 0.46) < 0.024 && abs(in.uv.x - 0.5) < 0.25;
                let h2 = abs(in.uv.y - 0.55) < 0.016 && abs(in.uv.x - 0.5) < 0.19;
                if h1 {
                    out_col = vec4<f32>(0.98, 0.99, 1.0, 1.0);
                } else if h2 {
                    out_col = vec4<f32>(0.82, 0.88, 0.95, 1.0);
                }
            }
        }
    }

    // Props & Overlays layer (corner motion bug verifying layer toggling)
    if overlay_active {
        let bug_d = distance(in.uv * vec2<f32>(u.aspect, 1.0), vec2<f32>(0.93 * u.aspect, 0.09));
        if bug_d < 0.028 {
            let ring = smoothstep(0.020, 0.026, bug_d);
            let col = mix(accent, vec3<f32>(1.0, 1.0, 1.0), ring * 0.7);
            out_col = vec4<f32>(col, 1.0);
        }
    }

    return out_col;
}
