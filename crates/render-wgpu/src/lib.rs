//! Multi-surface `wgpu` 30 + `winit` 30 real-time 7-layer GPU compositor for OpenPresenter.
//!
//! Integrates `render-text` (`BroadcastRasterizer`) to upload cached premultiplied RGBA8
//! typography/vector textures to the GPU on slide mutation while compositing motion
//! backgrounds, transitions, masks, and multiple Looks at locked 60 fps.

pub mod staging;

use bytemuck::{Pod, Zeroable};
use core_ipc::{
    DisplayInfo, EngineSnapshot, EngineTelemetry, OutputTelemetry, PROTOCOL_VERSION,
    PreviewBusDescriptor, ShmPreviewWriter,
};
use core_scene::{
    ActiveBackground, BlendMode, ColorRgba, LayerKind, LookConfig, MaskStyle, OutputConfig,
    OutputId, OutputKind, OutputPlacement, SceneDiff, SceneState, Slide, StageLayoutConfig,
};
use render_text::{BroadcastRasterizer, Vector2DRasterizer, decode_local_image_premul_rgba};
use staging::PreviewReadbackRing;
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};
use tracing::{error, info, warn};
use winit::application::ApplicationHandler;
use winit::dpi::{LogicalSize, PhysicalPosition, PhysicalSize};
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Fullscreen, Window, WindowAttributes, WindowId, WindowLevel};

pub use winit::event_loop::EventLoopProxy;

pub const PREVIEW_WIDTH: u32 = 640;
pub const PREVIEW_HEIGHT: u32 = 360;
const LAYER_TEX_WIDTH: u32 = 1280;
const LAYER_TEX_HEIGHT: u32 = 720;

/// Wakeup events delivered from the IPC control thread to the `winit` main thread.
#[derive(Debug, Clone)]
pub enum EngineWakeEvent {
    SceneChanged,
    Shutdown,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
struct CompositorUniforms {
    time_sec: f32,
    blend_t: f32,
    aspect: f32,
    flags: u32,
    curr_primary: [f32; 4],
    curr_secondary: [f32; 4],
    curr_accent: [f32; 4],
    prev_primary: [f32; 4],
    prev_secondary: [f32; 4],
    prev_accent: [f32; 4],
    curr_style: u32,
    prev_style: u32,
    transition_kind: u32,
    mask_style: u32,
    blend_mode: u32,
    _pad0: u32,
    _pad1: u32,
    _pad2: u32,
}

/// Thread-safe engine state shared between the GPU compositor thread and the IPC server.
pub struct SharedEngineState {
    inner: RwLock<EngineRuntimeState>,
}

struct EngineRuntimeState {
    scene: SceneState,
    undo_stack: VecDeque<SceneState>,
    redo_stack: VecDeque<SceneState>,
    prev_slide: Option<Slide>,
    prev_background: Option<ActiveBackground>,
    transition_start: Instant,
    transition_duration: Duration,
    transition_kind: u32,
    hardware_ready: bool,
    gpu_adapter_name: String,
    gpu_backend: String,
    displays: Vec<DisplayInfo>,
    preview_buses: Vec<PreviewBusDescriptor>,
    telemetry: EngineTelemetry,
}

impl SharedEngineState {
    pub fn new(scene: SceneState) -> Self {
        let initial_slide = scene.active_slide.clone();
        let initial_bg = scene.active_background.clone();
        Self {
            inner: RwLock::new(EngineRuntimeState {
                scene,
                undo_stack: VecDeque::with_capacity(32),
                redo_stack: VecDeque::with_capacity(32),
                prev_slide: initial_slide,
                prev_background: initial_bg,
                transition_start: Instant::now(),
                transition_duration: Duration::from_millis(1),
                transition_kind: 0,
                hardware_ready: false,
                gpu_adapter_name: "Initializing...".to_string(),
                gpu_backend: "Pending".to_string(),
                displays: Vec::new(),
                preview_buses: Vec::new(),
                telemetry: EngineTelemetry::default(),
            }),
        }
    }

    pub fn is_hardware_ready(&self) -> bool {
        self.inner.read().map(|g| g.hardware_ready).unwrap_or(false)
    }

    pub fn mark_headless_ready(&self) {
        if let Ok(mut guard) = self.inner.write() {
            guard.hardware_ready = true;
            guard.gpu_adapter_name = "Headless Test Engine".to_string();
            guard.gpu_backend = "Headless".to_string();
        }
    }

    pub fn apply_diff(&self, diff: SceneDiff) -> Option<SceneState> {
        let mut guard = self.inner.write().ok()?;
        let old_slide = guard.scene.active_slide.clone();
        let old_bg = guard.scene.active_background.clone();

        match diff {
            SceneDiff::Undo => {
                let mut prev = guard.undo_stack.pop_back()?;
                let current = guard.scene.clone();
                if guard.redo_stack.len() >= 32 {
                    guard.redo_stack.pop_front();
                }
                guard.redo_stack.push_back(current);
                prev.revision = guard.scene.revision + 1;
                guard.scene = prev;
                return Some(guard.scene.clone());
            }
            SceneDiff::Redo => {
                let mut next = guard.redo_stack.pop_back()?;
                let current = guard.scene.clone();
                if guard.undo_stack.len() >= 32 {
                    guard.undo_stack.pop_front();
                }
                guard.undo_stack.push_back(current);
                next.revision = guard.scene.revision + 1;
                guard.scene = next;
                return Some(guard.scene.clone());
            }
            other => {
                let record_history = !matches!(
                    other,
                    SceneDiff::SyncLooksAndTemplates { .. } | SceneDiff::SetTimerReadout(_)
                );
                let snapshot_before = record_history.then(|| guard.scene.clone());
                let changed = guard.scene.apply(other);
                if !changed {
                    return None;
                }
                if let Some(prev) = snapshot_before {
                    if guard.undo_stack.len() >= 32 {
                        guard.undo_stack.pop_front();
                    }
                    guard.undo_stack.push_back(prev);
                    guard.redo_stack.clear();
                }
            }
        }
        let bg_changed = guard.scene.active_background != old_bg;
        if bg_changed {
            guard.prev_background = old_bg;
            guard.transition_start = Instant::now();
            guard.transition_kind = 0; // Smooth Dissolve for Layer 1 background swap
            guard.transition_duration = Duration::from_millis(450);
        }
        if guard.scene.active_slide != old_slide {
            let old_id = old_slide.as_ref().map(|s| s.id);
            let new_id = guard.scene.active_slide.as_ref().map(|s| s.id);
            if old_id != new_id {
                guard.prev_slide = old_slide;
                if !bg_changed {
                    guard.prev_background = guard.scene.active_background.clone();
                }
                guard.transition_start = Instant::now();
                let spec = guard
                    .scene
                    .active_slide
                    .as_ref()
                    .map(|s| s.transition)
                    .unwrap_or_default();
                guard.transition_kind = spec.kind_code();
                guard.transition_duration = Duration::from_millis(u64::from(spec.duration_ms().max(1)));
            }
        }
        Some(guard.scene.clone())
    }

    pub fn snapshot(&self) -> EngineSnapshot {
        let guard = self.inner.read().expect("engine state lock poisoned");
        EngineSnapshot {
            protocol_version: PROTOCOL_VERSION,
            engine_pid: std::process::id(),
            gpu_adapter_name: guard.gpu_adapter_name.clone(),
            gpu_backend: guard.gpu_backend.clone(),
            state: guard.scene.clone(),
            displays: guard.displays.clone(),
            preview_buses: guard.preview_buses.clone(),
            telemetry: guard.telemetry.clone(),
        }
    }

    pub fn displays(&self) -> Vec<DisplayInfo> {
        self.inner
            .read()
            .map(|g| g.displays.clone())
            .unwrap_or_default()
    }

    pub fn telemetry(&self) -> EngineTelemetry {
        self.inner
            .read()
            .map(|g| g.telemetry.clone())
            .unwrap_or_default()
    }

    pub fn set_hardware_metadata(
        &self,
        adapter: String,
        backend: String,
        displays: Vec<DisplayInfo>,
        preview_buses: Vec<PreviewBusDescriptor>,
    ) {
        if let Ok(mut guard) = self.inner.write() {
            guard.hardware_ready = true;
            guard.gpu_adapter_name = adapter;
            guard.gpu_backend = backend;
            guard.displays = displays;
            guard.preview_buses = preview_buses;
        }
    }

    pub fn update_outputs_metadata(
        &self,
        displays: Vec<DisplayInfo>,
        preview_buses: Vec<PreviewBusDescriptor>,
    ) {
        if let Ok(mut guard) = self.inner.write() {
            guard.displays = displays;
            guard.preview_buses = preview_buses;
        }
    }

    fn update_telemetry(&self, telemetry: EngineTelemetry) {
        if let Ok(mut guard) = self.inner.write() {
            guard.telemetry = telemetry;
        }
    }

    fn scene_revision(&self) -> u64 {
        self.inner.read().map(|g| g.scene.revision).unwrap_or(0)
    }

    fn uniforms_for_output(
        &self,
        output_id: OutputId,
        output_kind: OutputKind,
        aspect: f32,
        elapsed_sec: f32,
        has_bg_image: bool,
    ) -> CompositorUniforms {
        let guard = self.inner.read().expect("engine state lock poisoned");
        let look = guard.scene.look_for_output(output_id).cloned().unwrap_or(
            LookConfig {
                id: core_scene::LOOK_AUDIENCE_FULL,
                name: "Fallback".to_string(),
                description: String::new(),
                enabled_layers: LayerKind::ALL.to_vec(),
                lower_third_mode: false,
                transparent_background: false,
                slide_blend_mode: BlendMode::Normal,
                template_id: None,
                mask_override: None,
            },
        );

        let blend_t = (guard.transition_start.elapsed().as_secs_f32()
            / guard.transition_duration.as_secs_f32())
        .clamp(0.0, 1.0);

        let curr = guard.scene.active_slide.as_ref();
        let prev = guard.prev_slide.as_ref().or(curr);

        let mut flags = 0u32;
        if guard.scene.blackout {
            flags |= 1 << 0;
        }
        if guard.scene.media_layer_active
            && (guard.scene.active_background.is_some() || curr.is_some())
            && look.is_layer_enabled(LayerKind::MediaBackground)
        {
            flags |= 1 << 1;
        }
        let fg_on_announcements = guard
            .scene
            .active_foreground_media
            .as_ref()
            .is_some_and(|fg| {
                fg.target_layer == LayerKind::Announcements
                    && guard.scene.announcements_layer_active
                    && look.is_layer_enabled(LayerKind::Announcements)
            });
        if (guard.scene.slide_layer_active && look.is_layer_enabled(LayerKind::Slide))
            || fg_on_announcements
        {
            flags |= 1 << 2;
        }
        if guard.scene.overlay_layer_active && look.is_layer_enabled(LayerKind::PropsAndOverlays) {
            flags |= 1 << 3;
        }
        if look.lower_third_mode {
            flags |= 1 << 4;
        }
        if output_kind == OutputKind::Stage {
            flags |= 1 << 5;
        }
        if look.transparent_background {
            flags |= 1 << 6;
        }
        if has_bg_image {
            flags |= 1 << 7;
        }

        let (c1, c2, ca, cs) = background_colors(guard.scene.active_background.as_ref(), curr);
        let (p1, p2, pa, ps) = background_colors(
            guard.prev_background.as_ref().or(guard.scene.active_background.as_ref()),
            prev,
        );

        let mask_style = if look.is_layer_enabled(LayerKind::Masks) {
            look.mask_override.unwrap_or(guard.scene.mask_style) as u32
        } else {
            MaskStyle::None as u32
        };

        CompositorUniforms {
            time_sec: elapsed_sec,
            blend_t,
            aspect,
            flags,
            curr_primary: c1.to_array(),
            curr_secondary: c2.to_array(),
            curr_accent: ca.to_array(),
            prev_primary: p1.to_array(),
            prev_secondary: p2.to_array(),
            prev_accent: pa.to_array(),
            curr_style: cs,
            prev_style: ps,
            transition_kind: guard.transition_kind,
            mask_style,
            blend_mode: look.slide_blend_mode as u32,
            _pad0: 0,
            _pad1: 0,
            _pad2: 0,
        }
    }
}

fn background_colors(
    bg: Option<&ActiveBackground>,
    fallback_slide: Option<&Slide>,
) -> (ColorRgba, ColorRgba, ColorRgba, u32) {
    if let Some(b) = bg {
        return (
            b.primary_color,
            b.secondary_color,
            b.accent_color,
            b.motion_style as u32,
        );
    }
    match fallback_slide {
        Some(s) => (
            s.primary_color,
            s.secondary_color,
            s.accent_color,
            s.motion_style as u32,
        ),
        None => (ColorRgba::BLACK, ColorRgba::BLACK, ColorRgba::BLACK, 0),
    }
}

struct OutputSurfaceState {
    config_meta: OutputConfig,
    window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    surface_config: wgpu::SurfaceConfiguration,
    pipeline: wgpu::RenderPipeline,
    uniform_buffer: wgpu::Buffer,
    slide_texture: wgpu::Texture,
    bg_texture: wgpu::Texture,
    has_bg_image: bool,
    uploaded_bg_path: Option<String>,
    uploaded_revision: u64,
    bind_group: wgpu::BindGroup,
    preview_bus: Option<PreviewBusDescriptor>,
    preview_ring: Option<PreviewReadbackRing>,
    preview_pipeline: Option<wgpu::RenderPipeline>,
    frames_presented: u64,
}

struct GpuContext {
    instance: wgpu::Instance,
    adapter: wgpu::Adapter,
    device: wgpu::Device,
    queue: wgpu::Queue,
    shader: wgpu::ShaderModule,
    bind_group_layout: wgpu::BindGroupLayout,
    pipeline_layout: wgpu::PipelineLayout,
    sampler: wgpu::Sampler,
}

struct FrameStats {
    window_ms: VecDeque<f32>,
    last_frame: Instant,
    total_frames: u64,
    dropped_frames: u64,
    last_telemetry_publish: Instant,
}

impl FrameStats {
    fn new() -> Self {
        Self {
            window_ms: VecDeque::with_capacity(120),
            last_frame: Instant::now(),
            total_frames: 0,
            dropped_frames: 0,
            last_telemetry_publish: Instant::now(),
        }
    }

    fn record_frame(&mut self) -> Option<EngineTelemetry> {
        let now = Instant::now();
        let dt_ms = now.duration_since(self.last_frame).as_secs_f32() * 1000.0;
        self.last_frame = now;
        self.total_frames += 1;

        if self.total_frames > 5 && dt_ms > 22.0 {
            self.dropped_frames += 1;
        }
        if self.window_ms.len() >= 120 {
            self.window_ms.pop_front();
        }
        self.window_ms.push_back(dt_ms);

        if now.duration_since(self.last_telemetry_publish) >= Duration::from_millis(250) {
            self.last_telemetry_publish = now;
            let mut sorted: Vec<f32> = self.window_ms.iter().copied().collect();
            sorted.sort_by(f32::total_cmp);
            let avg = if sorted.is_empty() {
                0.0
            } else {
                sorted.iter().sum::<f32>() / sorted.len() as f32
            };
            let p99 = if sorted.is_empty() {
                0.0
            } else {
                let idx = ((sorted.len() as f32 * 0.99).ceil() as usize).saturating_sub(1);
                sorted[idx.min(sorted.len() - 1)]
            };
            let fps = if avg > 0.01 { 1000.0 / avg } else { 0.0 };
            Some(EngineTelemetry {
                fps,
                avg_frame_ms: avg,
                p99_frame_ms: p99,
                total_frames: self.total_frames,
                dropped_frames: self.dropped_frames,
                outputs: Vec::new(),
            })
        } else {
            None
        }
    }
}

struct CompositorApp {
    shared: Arc<SharedEngineState>,
    preview_dir: PathBuf,
    start_time: Instant,
    next_frame_at: Instant,
    scene_dirty: bool,
    gpu: Option<GpuContext>,
    rasterizer: BroadcastRasterizer,
    outputs: HashMap<WindowId, OutputSurfaceState>,
    ordered_windows: Vec<WindowId>,
    stats: FrameStats,
    preview_tick_counter: u64,
}

impl CompositorApp {
    fn new(shared: Arc<SharedEngineState>, preview_dir: PathBuf) -> Self {
        let now = Instant::now();
        Self {
            shared,
            preview_dir,
            start_time: now,
            next_frame_at: now,
            scene_dirty: true,
            gpu: None,
            rasterizer: BroadcastRasterizer::new(64),
            outputs: HashMap::new(),
            ordered_windows: Vec::new(),
            stats: FrameStats::new(),
            preview_tick_counter: 0,
        }
    }

    fn create_pipeline(
        device: &wgpu::Device,
        layout: &wgpu::PipelineLayout,
        shader: &wgpu::ShaderModule,
        format: wgpu::TextureFormat,
        label: &str,
    ) -> wgpu::RenderPipeline {
        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some(label),
            layout: Some(layout),
            vertex: wgpu::VertexState {
                module: shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        })
    }

    fn sync_output_slide_texture(&mut self, window_id: WindowId) {
        let rev = self.shared.scene_revision();
        let Some(gpu) = self.gpu.as_ref() else {
            return;
        };
        let Some(out) = self.outputs.get_mut(&window_id) else {
            return;
        };
        if out.uploaded_revision == rev {
            return;
        }

        let snap = self.shared.snapshot();
        // When Freeze Output is enabled, hold the frozen GPU textures without uploading mutations.
        if snap.state.freeze_output && out.uploaded_revision > 0 {
            return;
        }
        let look = snap.state.look_for_output(out.config_meta.id);
        let lower_third = look.map(|l| l.lower_third_mode).unwrap_or(false);
        let template_override = look.and_then(|l| snap.state.template_for_look(l));
        let fit_mode = snap.state.media_transport.fit_mode;

        // Upload real local PNG/BMP image texture for Layer 1 background when configured.
        let desired_bg_path = snap
            .state
            .active_background
            .as_ref()
            .map(|b| b.media_path.clone())
            .filter(|p| !p.starts_with("builtin://") && !p.is_empty());
        if desired_bg_path != out.uploaded_bg_path {
            out.uploaded_bg_path = desired_bg_path.clone();
            if let Some(bg_path) = desired_bg_path
                && let Some(rgba) = decode_local_image_premul_rgba(
                    Path::new(&bg_path),
                    LAYER_TEX_WIDTH,
                    LAYER_TEX_HEIGHT,
                    fit_mode,
                )
            {
                gpu.queue.write_texture(
                    wgpu::TexelCopyTextureInfo {
                        texture: &out.bg_texture,
                        mip_level: 0,
                        origin: wgpu::Origin3d::ZERO,
                        aspect: wgpu::TextureAspect::All,
                    },
                    &rgba,
                    wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(LAYER_TEX_WIDTH * 4),
                        rows_per_image: Some(LAYER_TEX_HEIGHT),
                    },
                    wgpu::Extent3d {
                        width: LAYER_TEX_WIDTH,
                        height: LAYER_TEX_HEIGHT,
                        depth_or_array_layers: 1,
                    },
                );
                out.has_bg_image = true;
            } else {
                out.has_bg_image = false;
            }
        }

        let fg_active = snap.state.active_foreground_media.as_ref().filter(|fg| {
            look.map(|l| l.is_layer_enabled(fg.target_layer))
                .unwrap_or(true)
        });

        let raster = if out.config_meta.kind == OutputKind::Stage {
            let layout = out
                .config_meta
                .stage_layout
                .clone()
                .unwrap_or_else(StageLayoutConfig::default);
            self.rasterizer.rasterize_stage_confidence(
                snap.state.active_slide.as_ref(),
                snap.state.next_slide_preview.as_ref(),
                snap.state.stage_message.as_deref(),
                snap.state.timer_readout.as_deref(),
                &layout,
                LAYER_TEX_WIDTH,
                LAYER_TEX_HEIGHT,
            )
        } else if let Some(fg) = fg_active {
            // Decode real local foreground PNG/BMP image directly onto Layer 2/3 when available.
            if let Some(rgba) = decode_local_image_premul_rgba(
                Path::new(&fg.media_path),
                LAYER_TEX_WIDTH,
                LAYER_TEX_HEIGHT,
                fit_mode,
            ) {
                render_text::RasterizedLayer {
                    width: LAYER_TEX_WIDTH,
                    height: LAYER_TEX_HEIGHT,
                    content_hash: [0u8; 32],
                    rgba_premul: rgba,
                }
            } else {
                // Fallback card for videos or virtual/missing media paths.
                let mut fg_slide = core_scene::demo_deck()[0].clone();
                fg_slide.id = core_scene::SlideId::new();
                fg_slide.label = fg.label.clone();
                fg_slide.headline = fg.label.clone();
                fg_slide.body = format!("{} LAYER · {}", fg.kind.to_uppercase(), fg.media_path);
                fg_slide.elements = vec![
                    core_scene::SlideElement {
                        id: core_scene::ElementId::new(),
                        name: "Foreground Media Frame".to_string(),
                        bounds: core_scene::RectF::new(140.0, 110.0, 1640.0, 860.0),
                        opacity: 0.94,
                        z_index: 5,
                        kind: core_scene::SlideElementKind::ShapeRect {
                            fill: ColorRgba::rgba(0.06, 0.11, 0.19, 0.92),
                            stroke: ColorRgba::rgba(0.28, 0.62, 0.98, 0.95),
                            stroke_width: 4.0,
                            corner_radius: 24.0,
                        },
                    },
                    core_scene::SlideElement {
                        id: core_scene::ElementId::new(),
                        name: "Foreground Media Label".to_string(),
                        bounds: core_scene::RectF::new(200.0, 340.0, 1520.0, 380.0),
                        opacity: 1.0,
                        z_index: 10,
                        kind: core_scene::SlideElementKind::TextBox {
                            text: format!("[FOREGROUND {}]\n{}", fg.kind.to_uppercase(), fg.label),
                            style: core_scene::TextStyle {
                                font_size: 68.0,
                                ..core_scene::TextStyle::default()
                            },
                        },
                    },
                ];
                self.rasterizer
                    .rasterize_slide(&fg_slide, LAYER_TEX_WIDTH, LAYER_TEX_HEIGHT, lower_third)
            }
        } else if let Some(slide) = snap.state.active_slide.as_ref() {
            self.rasterizer.rasterize_slide_with_template(
                slide,
                LAYER_TEX_WIDTH,
                LAYER_TEX_HEIGHT,
                lower_third,
                template_override,
            )
        } else {
            render_text::RasterizedLayer {
                width: LAYER_TEX_WIDTH,
                height: LAYER_TEX_HEIGHT,
                content_hash: [0u8; 32],
                rgba_premul: vec![0u8; (LAYER_TEX_WIDTH * LAYER_TEX_HEIGHT * 4) as usize],
            }
        };

        gpu.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &out.slide_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &raster.rgba_premul,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(LAYER_TEX_WIDTH * 4),
                rows_per_image: Some(LAYER_TEX_HEIGHT),
            },
            wgpu::Extent3d {
                width: LAYER_TEX_WIDTH,
                height: LAYER_TEX_HEIGHT,
                depth_or_array_layers: 1,
            },
        );
        out.uploaded_revision = rev;
    }

    fn render_output(&mut self, window_id: WindowId, force_preview: bool) {
        self.sync_output_slide_texture(window_id);

        let Some(gpu) = self.gpu.as_ref() else {
            return;
        };
        let Some(out) = self.outputs.get_mut(&window_id) else {
            return;
        };

        let phys = out.window.inner_size();
        let minimized = out.window.is_minimized().unwrap_or(false) || phys.width == 0 || phys.height == 0;

        if !minimized
            && (phys.width != out.surface_config.width || phys.height != out.surface_config.height)
        {
            out.surface_config.width = phys.width;
            out.surface_config.height = phys.height;
            out.surface.configure(&gpu.device, &out.surface_config);
        }

        let is_frozen = self
            .shared
            .inner
            .read()
            .map(|g| g.scene.freeze_output)
            .unwrap_or(false);
        if !is_frozen || out.frames_presented == 0 {
            let elapsed_sec = self.start_time.elapsed().as_secs_f32();
            let aspect = (out.surface_config.width as f32) / (out.surface_config.height.max(1) as f32);
            let uniforms = self.shared.uniforms_for_output(
                out.config_meta.id,
                out.config_meta.kind,
                aspect,
                elapsed_sec,
                out.has_bg_image,
            );
            gpu.queue
                .write_buffer(&out.uniform_buffer, 0, bytemuck::bytes_of(&uniforms));
        }

        let mut surface_frame = None;
        if !minimized {
            match out.surface.get_current_texture() {
                wgpu::CurrentSurfaceTexture::Success(frame)
                | wgpu::CurrentSurfaceTexture::Suboptimal(frame) => {
                    surface_frame = Some(frame);
                }
                wgpu::CurrentSurfaceTexture::Lost | wgpu::CurrentSurfaceTexture::Outdated => {
                    let cur = out.window.inner_size();
                    if cur.width > 0 && cur.height > 0 {
                        out.surface_config.width = cur.width;
                        out.surface_config.height = cur.height;
                        out.surface.configure(&gpu.device, &out.surface_config);
                    }
                }
                wgpu::CurrentSurfaceTexture::Timeout
                | wgpu::CurrentSurfaceTexture::Occluded
                | wgpu::CurrentSurfaceTexture::Validation => {}
            }
        }

        let want_preview = force_preview || self.preview_tick_counter.is_multiple_of(2);
        if surface_frame.is_none() && !want_preview {
            if let Some(ring) = out.preview_ring.as_mut() {
                ring.drain_ready(&gpu.device);
            }
            return;
        }

        let mut encoder = gpu
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("compositor_frame_encoder"),
            });

        let mut encoded_any = false;

        if let Some(frame) = surface_frame.as_ref() {
            let surface_view = frame
                .texture
                .create_view(&wgpu::TextureViewDescriptor::default());
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("output_surface_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &surface_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&out.pipeline);
            pass.set_bind_group(0, &out.bind_group, &[]);
            pass.draw(0..3, 0..1);
            encoded_any = true;
        }

        let mut submitted_slot = None;
        if let (Some(ring), Some(preview_pipe)) =
            (out.preview_ring.as_mut(), out.preview_pipeline.as_ref())
        {
            ring.drain_ready(&gpu.device);
            if want_preview {
                {
                    let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: Some("preview_downsample_pass"),
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view: ring.view(),
                            resolve_target: None,
                            ops: wgpu::Operations {
                                load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                                store: wgpu::StoreOp::Store,
                            },
                            depth_slice: None,
                        })],
                        depth_stencil_attachment: None,
                        timestamp_writes: None,
                        occlusion_query_set: None,
                        multiview_mask: None,
                    });
                    pass.set_pipeline(preview_pipe);
                    pass.set_bind_group(0, &out.bind_group, &[]);
                    pass.draw(0..3, 0..1);
                }
                submitted_slot = ring.encode_copy(&mut encoder);
                encoded_any = true;
            }
        }

        if encoded_any {
            gpu.queue.submit(std::iter::once(encoder.finish()));
        }
        if let (Some(slot), Some(ring)) = (submitted_slot, out.preview_ring.as_mut()) {
            ring.begin_map(slot);
        }

        if let Some(frame) = surface_frame {
            gpu.queue.present(frame);
            out.frames_presented += 1;
        }
    }

    fn render_all_outputs(&mut self, force_preview: bool) {
        if self.gpu.is_none() || self.ordered_windows.is_empty() {
            return;
        }
        let ids: Vec<WindowId> = self.ordered_windows.clone();
        for wid in ids {
            self.render_output(wid, force_preview);
        }
        self.preview_tick_counter += 1;
        if let Some(mut telemetry) = self.stats.record_frame() {
            telemetry.outputs = self
                .ordered_windows
                .iter()
                .filter_map(|id| self.outputs.get(id))
                .map(|o| OutputTelemetry {
                    output_id: o.config_meta.id,
                    width: o.surface_config.width,
                    height: o.surface_config.height,
                    frames_presented: o.frames_presented,
                })
                .collect();
            self.shared.update_telemetry(telemetry);
        }
    }
    fn enumerate_displays(event_loop: &ActiveEventLoop) -> Vec<DisplayInfo> {
        event_loop
            .available_monitors()
            .enumerate()
            .map(|(idx, mon)| {
                let size = mon.size();
                let pos = mon.position();
                DisplayInfo {
                    index: idx,
                    name: mon
                        .name()
                        .unwrap_or_else(|| format!("Display {}", idx + 1)),
                    width: size.width,
                    height: size.height,
                    scale_factor: mon.scale_factor(),
                    refresh_millihertz: mon.refresh_rate_millihertz(),
                    position_x: pos.x,
                    position_y: pos.y,
                }
            })
            .collect()
    }

    fn build_window_attributes(
        event_loop: &ActiveEventLoop,
        cfg: &OutputConfig,
        stagger_idx: usize,
    ) -> WindowAttributes {
        let mut attrs = WindowAttributes::default()
            .with_title(format!("OpenPresenter Output — {}", cfg.name));

        match cfg.effective_placement() {
            OutputPlacement::Fullscreen => {
                let target_mon = cfg
                    .monitor_index
                    .and_then(|m| event_loop.available_monitors().nth(m));
                attrs = attrs
                    .with_inner_size(LogicalSize::new(cfg.width.max(320) as f64, cfg.height.max(180) as f64))
                    .with_fullscreen(Some(Fullscreen::Borderless(target_mon)));
            }
            OutputPlacement::Region(reg) => {
                let mon_origin = cfg
                    .monitor_index
                    .and_then(|m| event_loop.available_monitors().nth(m))
                    .map(|m| m.position())
                    .unwrap_or(PhysicalPosition::new(0, 0));
                attrs = attrs
                    .with_decorations(false)
                    .with_window_level(WindowLevel::AlwaysOnTop)
                    .with_inner_size(PhysicalSize::new(reg.width.max(64), reg.height.max(64)))
                    .with_position(PhysicalPosition::new(
                        mon_origin.x + reg.x,
                        mon_origin.y + reg.y,
                    ));
            }
            OutputPlacement::Windowed => {
                let pos = if let Some(mon) = cfg
                    .monitor_index
                    .and_then(|m| event_loop.available_monitors().nth(m))
                {
                    let origin = mon.position();
                    PhysicalPosition::new(
                        origin.x + 40 + (stagger_idx as i32) * 60,
                        origin.y + 40 + (stagger_idx as i32) * 50,
                    )
                } else {
                    PhysicalPosition::new(
                        60 + (stagger_idx as i32) * 90,
                        60 + (stagger_idx as i32) * 70,
                    )
                };
                attrs = attrs
                    .with_decorations(true)
                    .with_window_level(WindowLevel::Normal)
                    .with_inner_size(LogicalSize::new(
                        cfg.width.max(320) as f64,
                        cfg.height.max(180) as f64,
                    ))
                    .with_position(pos);
            }
        }
        attrs
    }

    fn apply_live_window_placement(
        event_loop: &ActiveEventLoop,
        window: &Window,
        cfg: &OutputConfig,
    ) {
        window.set_title(&format!("OpenPresenter Output — {}", cfg.name));
        match cfg.effective_placement() {
            OutputPlacement::Fullscreen => {
                let target_mon = cfg
                    .monitor_index
                    .and_then(|m| event_loop.available_monitors().nth(m));
                window.set_window_level(WindowLevel::Normal);
                window.set_fullscreen(Some(Fullscreen::Borderless(target_mon)));
            }
            OutputPlacement::Region(reg) => {
                let mon_origin = cfg
                    .monitor_index
                    .and_then(|m| event_loop.available_monitors().nth(m))
                    .map(|m| m.position())
                    .unwrap_or(PhysicalPosition::new(0, 0));
                window.set_fullscreen(None);
                window.set_decorations(false);
                window.set_window_level(WindowLevel::AlwaysOnTop);
                let _ = window.request_inner_size(PhysicalSize::new(
                    reg.width.max(64),
                    reg.height.max(64),
                ));
                window.set_outer_position(PhysicalPosition::new(
                    mon_origin.x + reg.x,
                    mon_origin.y + reg.y,
                ));
            }
            OutputPlacement::Windowed => {
                window.set_fullscreen(None);
                window.set_decorations(true);
                window.set_window_level(WindowLevel::Normal);
                let _ = window.request_inner_size(LogicalSize::new(
                    cfg.width.max(320) as f64,
                    cfg.height.max(180) as f64,
                ));
                if let Some(mon) = cfg
                    .monitor_index
                    .and_then(|m| event_loop.available_monitors().nth(m))
                {
                    let origin = mon.position();
                    window.set_outer_position(PhysicalPosition::new(origin.x + 60, origin.y + 60));
                }
            }
        }
    }

    fn build_output_surface_state(
        gpu: &GpuContext,
        preview_dir: &Path,
        cfg: OutputConfig,
        window: Arc<Window>,
        surface: wgpu::Surface<'static>,
        is_primary: bool,
    ) -> OutputSurfaceState {
        let caps = surface.get_capabilities(&gpu.adapter);
        let surface_format = caps
            .formats
            .iter()
            .copied()
            .find(|f| f.is_srgb())
            .unwrap_or(caps.formats[0]);

        let present_mode = if is_primary {
            wgpu::PresentMode::AutoVsync
        } else if caps.present_modes.contains(&wgpu::PresentMode::Mailbox) {
            wgpu::PresentMode::Mailbox
        } else if caps.present_modes.contains(&wgpu::PresentMode::Immediate) {
            wgpu::PresentMode::Immediate
        } else {
            wgpu::PresentMode::AutoNoVsync
        };

        let phys = window.inner_size();
        let surface_config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width: phys.width.max(1),
            height: phys.height.max(1),
            present_mode,
            alpha_mode: caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
            color_space: wgpu::SurfaceColorSpace::Auto,
        };
        surface.configure(&gpu.device, &surface_config);

        let pipeline = Self::create_pipeline(
            &gpu.device,
            &gpu.pipeline_layout,
            &gpu.shader,
            surface_format,
            &format!("pipeline_{}", cfg.name),
        );

        let uniform_buffer = gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("compositor_uniform_buf"),
            size: std::mem::size_of::<CompositorUniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let slide_texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("slide_layer_rgba8"),
            size: wgpu::Extent3d {
                width: LAYER_TEX_WIDTH,
                height: LAYER_TEX_HEIGHT,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let slide_view = slide_texture.create_view(&wgpu::TextureViewDescriptor::default());

        let bg_texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("bg_layer_rgba8"),
            size: wgpu::Extent3d {
                width: LAYER_TEX_WIDTH,
                height: LAYER_TEX_HEIGHT,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let bg_view = bg_texture.create_view(&wgpu::TextureViewDescriptor::default());

        let bind_group = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("compositor_bg"),
            layout: &gpu.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&slide_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&gpu.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(&bg_view),
                },
            ],
        });

        let pid = std::process::id();
        let shm_path = preview_dir.join(format!("preview-{pid}-{}.op_shm", cfg.id.0));
        let (preview_bus, preview_ring, preview_pipeline) =
            match ShmPreviewWriter::create(&shm_path, PREVIEW_WIDTH, PREVIEW_HEIGHT) {
                Ok(writer) => {
                    let bus = PreviewBusDescriptor {
                        output_id: cfg.id,
                        shm_path: shm_path.to_string_lossy().into_owned(),
                        width: PREVIEW_WIDTH,
                        height: PREVIEW_HEIGHT,
                    };
                    let ring =
                        PreviewReadbackRing::new(&gpu.device, PREVIEW_WIDTH, PREVIEW_HEIGHT, writer);
                    let ppipe = Self::create_pipeline(
                        &gpu.device,
                        &gpu.pipeline_layout,
                        &gpu.shader,
                        wgpu::TextureFormat::Rgba8UnormSrgb,
                        "preview_rgba8_pipeline",
                    );
                    (Some(bus), Some(ring), Some(ppipe))
                }
                Err(err) => {
                    warn!("failed to create SHM preview ring for {}: {err}", cfg.name);
                    (None, None, None)
                }
            };

        OutputSurfaceState {
            config_meta: cfg,
            window,
            surface,
            surface_config,
            pipeline,
            uniform_buffer,
            slide_texture,
            bg_texture,
            has_bg_image: false,
            uploaded_bg_path: None,
            uploaded_revision: 0,
            bind_group,
            preview_bus,
            preview_ring,
            preview_pipeline,
            frames_presented: 0,
        }
    }

    fn current_preview_buses(&self) -> Vec<PreviewBusDescriptor> {
        self.ordered_windows
            .iter()
            .filter_map(|wid| self.outputs.get(wid))
            .filter_map(|o| o.preview_bus.clone())
            .collect()
    }

    /// Reconciles live `winit` + `wgpu` output windows with `SceneState.outputs` in real time:
    /// adds newly configured Audience/Stage/Split-Monitor screens, updates placement/monitor/Look
    /// in-place, and closes removed screens without interrupting remaining live displays.
    fn sync_outputs_with_scene(&mut self, event_loop: &ActiveEventLoop) {
        if self.gpu.is_none() {
            return;
        }
        let desired = self.shared.snapshot().state.outputs;
        let desired_ids: HashSet<OutputId> = desired.iter().map(|o| o.id).collect();

        // 1. Remove closed/deleted outputs
        let to_remove: Vec<WindowId> = self
            .outputs
            .iter()
            .filter(|(_, state)| !desired_ids.contains(&state.config_meta.id))
            .map(|(wid, _)| *wid)
            .collect();

        let mut topology_changed = !to_remove.is_empty();
        for wid in to_remove {
            if let Some(removed) = self.outputs.remove(&wid) {
                info!(
                    output = %removed.config_meta.name,
                    "removed GPU output window dynamically"
                );
            }
            self.ordered_windows.retain(|&id| id != wid);
        }

        // 2. Update existing outputs in-place or spawn newly added outputs
        for (idx, cfg) in desired.into_iter().enumerate() {
            let existing_wid = self
                .outputs
                .iter()
                .find(|(_, state)| state.config_meta.id == cfg.id)
                .map(|(wid, _)| *wid);

            if let Some(wid) = existing_wid {
                if let Some(state) = self.outputs.get_mut(&wid)
                    && state.config_meta != cfg
                {
                    let placement_changed = state.config_meta.monitor_index != cfg.monitor_index
                        || state.config_meta.effective_placement() != cfg.effective_placement()
                        || state.config_meta.width != cfg.width
                        || state.config_meta.height != cfg.height
                        || state.config_meta.name != cfg.name;
                    if placement_changed {
                        Self::apply_live_window_placement(event_loop, &state.window, &cfg);
                    }
                    state.config_meta = cfg;
                    state.uploaded_revision = 0;
                    topology_changed = true;
                }
            } else if let Some(gpu) = self.gpu.as_ref() {
                let attrs = Self::build_window_attributes(event_loop, &cfg, idx);
                let window = match event_loop.create_window(attrs) {
                    Ok(w) => Arc::new(w),
                    Err(err) => {
                        error!("failed to spawn live output window '{}': {err}", cfg.name);
                        continue;
                    }
                };
                let surface = match gpu.instance.create_surface(Arc::clone(&window)) {
                    Ok(s) => s,
                    Err(err) => {
                        error!("failed to create wgpu surface for '{}': {err}", cfg.name);
                        continue;
                    }
                };
                let is_primary = self.ordered_windows.is_empty();
                let out_state = Self::build_output_surface_state(
                    gpu,
                    &self.preview_dir,
                    cfg,
                    window,
                    surface,
                    is_primary,
                );
                let wid = out_state.window.id();
                info!(
                    output = %out_state.config_meta.name,
                    kind = ?out_state.config_meta.kind,
                    placement = ?out_state.config_meta.effective_placement(),
                    "created live GPU output screen"
                );
                self.ordered_windows.push(wid);
                self.outputs.insert(wid, out_state);
                topology_changed = true;
            }
        }

        if topology_changed {
            let displays = Self::enumerate_displays(event_loop);
            let buses = self.current_preview_buses();
            self.shared.update_outputs_metadata(displays, buses);
        }
    }
}

impl ApplicationHandler<EngineWakeEvent> for CompositorApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.gpu.is_some() {
            return;
        }

        let displays = Self::enumerate_displays(event_loop);
        info!("enumerated {} physical display(s)", displays.len());

        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let output_configs = self.shared.snapshot().state.outputs;
        if output_configs.is_empty() {
            warn!("no outputs configured in SceneState");
            return;
        }

        let mut created_windows: Vec<(OutputConfig, Arc<Window>, wgpu::Surface<'static>)> =
            Vec::with_capacity(output_configs.len());

        for (idx, cfg) in output_configs.into_iter().enumerate() {
            let attrs = Self::build_window_attributes(event_loop, &cfg, idx);
            let window = match event_loop.create_window(attrs) {
                Ok(w) => Arc::new(w),
                Err(err) => {
                    error!("failed to create output window #{idx}: {err}");
                    continue;
                }
            };

            let surface = match instance.create_surface(Arc::clone(&window)) {
                Ok(s) => s,
                Err(err) => {
                    error!("failed to create wgpu surface #{idx}: {err}");
                    continue;
                }
            };

            created_windows.push((cfg, window, surface));
        }

        let Some((_, _, first_surface)) = created_windows.first() else {
            error!("no output surfaces could be created");
            event_loop.exit();
            return;
        };

        let adapter = match pollster::block_on(instance.request_adapter(
            &wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(first_surface),
                force_fallback_adapter: false,
                ..Default::default()
            },
        )) {
            Ok(a) => a,
            Err(err) => {
                error!("failed to acquire wgpu adapter: {err}");
                event_loop.exit();
                return;
            }
        };

        let adapter_info = adapter.get_info();
        info!(
            "initialized GPU adapter '{}' via {:?}",
            adapter_info.name, adapter_info.backend
        );

        let (device, queue) = match pollster::block_on(adapter.request_device(
            &wgpu::DeviceDescriptor {
                label: Some("openpresenter_compositor_device"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                ..Default::default()
            },
        )) {
            Ok(pair) => pair,
            Err(err) => {
                error!("failed to create wgpu device: {err}");
                event_loop.exit();
                return;
            }
        };

        device.on_uncaptured_error(Arc::new(|err| {
            error!("wgpu uncaptured device error (suppressed to keep live outputs alive): {err}");
            let paths = core_diagnostics::DiagnosticsPaths::default_system();
            let _ = core_diagnostics::record_runtime_error(
                &paths,
                "openpresenter-engine",
                "wgpu-device-error",
                env!("CARGO_PKG_VERSION"),
                &err.to_string(),
                Some("crates/render-wgpu/src/lib.rs"),
                None,
            );
        }));

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("openpresenter_compositor_wgsl"),
            source: wgpu::ShaderSource::Wgsl(include_str!("compositor.wgsl").into()),
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("compositor_bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    },
                    count: None,
                },
            ],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("compositor_pipeline_layout"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("slide_layer_sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        let gpu = GpuContext {
            instance,
            adapter,
            device,
            queue,
            shader,
            bind_group_layout,
            pipeline_layout,
            sampler,
        };

        for (idx, (cfg, window, surface)) in created_windows.into_iter().enumerate() {
            let out_state = Self::build_output_surface_state(
                &gpu,
                &self.preview_dir,
                cfg,
                window,
                surface,
                idx == 0,
            );
            let wid = out_state.window.id();
            self.ordered_windows.push(wid);
            self.outputs.insert(wid, out_state);
        }

        let preview_buses = self.current_preview_buses();
        self.shared.set_hardware_metadata(
            adapter_info.name,
            format!("{:?}", adapter_info.backend),
            displays,
            preview_buses,
        );

        self.gpu = Some(gpu);
        self.scene_dirty = true;
        self.next_frame_at = Instant::now();
        // Render initial frame immediately so both Audience and Stage surfaces + SHM rings populate right away.
        self.render_all_outputs(true);
        // Reset frame timing baseline after one-time GPU/font startup so telemetry reflects steady-state FPS.
        self.stats = FrameStats::new();
        event_loop.set_control_flow(ControlFlow::WaitUntil(
            Instant::now() + Duration::from_micros(16_000),
        ));
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: EngineWakeEvent) {
        match event {
            EngineWakeEvent::SceneChanged => {
                self.sync_outputs_with_scene(event_loop);
                self.scene_dirty = true;
                self.next_frame_at = Instant::now();
            }
            EngineWakeEvent::Shutdown => {
                info!("received explicit Shutdown event; closing compositor outputs");
                event_loop.exit();
            }
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::CloseRequested => {
                if let Some(removed) = self.outputs.remove(&window_id) {
                    info!(
                        "output window '{}' closed by user ({} remaining)",
                        removed.config_meta.name,
                        self.outputs.len()
                    );
                    let _ = self
                        .shared
                        .apply_diff(SceneDiff::RemoveOutput(removed.config_meta.id));
                }
                self.ordered_windows.retain(|&id| id != window_id);
                let displays = Self::enumerate_displays(event_loop);
                let buses = self.current_preview_buses();
                self.shared.update_outputs_metadata(displays, buses);
                if self.ordered_windows.is_empty() {
                    info!("all output windows closed; shutting down engine");
                    event_loop.exit();
                }
            }
            WindowEvent::Resized(new_size) => {
                if let (Some(gpu), Some(out)) =
                    (self.gpu.as_ref(), self.outputs.get_mut(&window_id))
                    && new_size.width > 0
                    && new_size.height > 0
                {
                    out.surface_config.width = new_size.width;
                    out.surface_config.height = new_size.height;
                    out.surface.configure(&gpu.device, &out.surface_config);
                    self.scene_dirty = true;
                }
            }
            WindowEvent::RedrawRequested => {
                self.render_output(window_id, false);
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let now = Instant::now();
        if self.scene_dirty || now >= self.next_frame_at {
            let force_preview = self.scene_dirty;
            self.scene_dirty = false;
            self.render_all_outputs(force_preview);
            // Anchor next deadline to frame start so VSync-paced frames run at full 60 fps
            // while minimized/occluded frames still sleep instead of busy-spinning.
            self.next_frame_at = now + Duration::from_micros(16_000);
        }
        event_loop.set_control_flow(ControlFlow::WaitUntil(self.next_frame_at));
    }
}

pub fn create_event_loop() -> Result<(EventLoop<EngineWakeEvent>, EventLoopProxy<EngineWakeEvent>), String> {
    let event_loop = EventLoop::<EngineWakeEvent>::with_user_event()
        .build()
        .map_err(|e| e.to_string())?;
    event_loop.set_control_flow(ControlFlow::WaitUntil(
        Instant::now() + Duration::from_micros(16_000),
    ));
    let proxy = event_loop.create_proxy();
    Ok((event_loop, proxy))
}

pub fn run_compositor(
    event_loop: EventLoop<EngineWakeEvent>,
    shared: Arc<SharedEngineState>,
    preview_dir: PathBuf,
) -> Result<(), String> {
    let mut app = CompositorApp::new(shared, preview_dir);
    event_loop.run_app(&mut app).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compositor_uniforms_are_16_byte_aligned_for_wgsl() {
        assert_eq!(std::mem::size_of::<CompositorUniforms>() % 16, 0);
    }

    #[test]
    fn shared_state_tracks_transitions_masks_and_looks() {
        let state = SharedEngineState::new(SceneState::demo_m0(2));
        let target_id = state.snapshot().state.deck[2].id;

        let updated = state
            .apply_diff(SceneDiff::TriggerSlide {
                slide_id: target_id,
                transition_override: Some(core_scene::TransitionSpec::PushLeft { duration_ms: 400 }),
            })
            .expect("trigger slide");
        assert_eq!(updated.active_slide.as_ref().map(|s| s.id), Some(target_id));

        state
            .apply_diff(SceneDiff::SetMask(MaskStyle::SoftVignette))
            .expect("set mask");

        let u_aud = state.uniforms_for_output(
            core_scene::OUTPUT_AUDIENCE_MAIN,
            OutputKind::Audience,
            16.0 / 9.0,
            1.5,
            true,
        );
        let u_stage = state.uniforms_for_output(
            core_scene::OUTPUT_STAGE_MAIN,
            OutputKind::Stage,
            16.0 / 9.0,
            1.5,
            false,
        );
        assert_eq!(u_aud.flags & (1 << 5), 0);
        assert_ne!(u_aud.flags & (1 << 7), 0);
        assert_eq!(u_aud.transition_kind, 1);
        assert_eq!(u_aud.mask_style, MaskStyle::SoftVignette as u32);
        assert_ne!(u_stage.flags & (1 << 5), 0);
        assert_eq!(u_stage.mask_style, MaskStyle::None as u32);

        // Undo/Redo restores mask cleanly
        let undone = state.apply_diff(SceneDiff::Undo).expect("undo mask");
        assert_eq!(undone.mask_style, MaskStyle::None);
        let redone = state.apply_diff(SceneDiff::Redo).expect("redo mask");
        assert_eq!(redone.mask_style, MaskStyle::SoftVignette);
    }
}
