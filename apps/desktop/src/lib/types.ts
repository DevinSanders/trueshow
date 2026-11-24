export interface ColorRgba {
  r: number;
  g: number;
  b: number;
  a: number;
}

export interface RectF {
  x: number;
  y: number;
  width: number;
  height: number;
}

export type TextAlign = "Left" | "Center" | "Right";
export type VerticalAlign = "Top" | "Middle" | "Bottom";
export type MaskStyle = "None" | "SoftVignette" | "RoundedFrame" | "LowerThirdBand";

export interface TextStyle {
  font_family: string;
  font_size: number;
  bold: boolean;
  italic: boolean;
  line_height: number;
  color: ColorRgba;
  stroke_color: ColorRgba;
  stroke_width: number;
  shadow_color: ColorRgba;
  shadow_offset_x: number;
  shadow_offset_y: number;
  shadow_blur: number;
  backdrop_color: ColorRgba;
  backdrop_radius: number;
  backdrop_padding: number;
  h_align: TextAlign;
  v_align: VerticalAlign;
  auto_shrink: boolean;
}

export type SlideElementKind =
  | { TextBox: { text: string; style: TextStyle } }
  | {
      ShapeRect: {
        fill: ColorRgba;
        stroke: ColorRgba;
        stroke_width: number;
        corner_radius: number;
      };
    };

export interface SlideElement {
  id: string;
  name: string;
  bounds: RectF;
  opacity: number;
  z_index: number;
  kind: SlideElementKind;
}

export type TransitionSpec =
  | "Cut"
  | { Dissolve: { duration_ms: number } }
  | { PushLeft: { duration_ms: number } }
  | { WipeRight: { duration_ms: number } };

export type LayerKind =
  | "VideoInput"
  | "MediaBackground"
  | "Slide"
  | "Announcements"
  | "PropsAndOverlays"
  | "Masks"
  | "MessagesAndTimers";

export type MediaCueRole = "Background" | "Foreground" | "Announcement" | "Audio";

export interface SlideMediaCue {
  label: string;
  media_path: string;
  kind: string;
  role: MediaCueRole;
  motion_style: string;
  primary_color: ColorRgba;
  secondary_color: ColorRgba;
  accent_color: ColorRgba;
}

export interface ActiveBackground {
  label: string;
  media_path: string;
  motion_style: string;
  primary_color: ColorRgba;
  secondary_color: ColorRgba;
  accent_color: ColorRgba;
}

export interface ForegroundMedia {
  label: string;
  media_path: string;
  kind: string;
  target_layer: LayerKind;
}

export interface Slide {
  id: string;
  label: string;
  group: string;
  headline: string;
  body: string;
  chords_line: string | null;
  ccli_number: string | null;
  media_path: string | null;
  media_cue?: SlideMediaCue | null;
  primary_color: ColorRgba;
  secondary_color: ColorRgba;
  accent_color: ColorRgba;
  motion_style: string;
  transition: TransitionSpec;
  elements: SlideElement[];
}

export interface LookConfig {
  id: string;
  name: string;
  enabled_layers: LayerKind[];
  lower_third_mode: boolean;
  transparent_background: boolean;
  slide_blend_mode: string;
}

export interface MonitorRegion {
  x: number;
  y: number;
  width: number;
  height: number;
}

export type OutputPlacement =
  | "Windowed"
  | "Fullscreen"
  | { Region: MonitorRegion };

export interface OutputConfig {
  id: string;
  name: string;
  kind: "Audience" | "Stage";
  monitor_index: number | null;
  width: number;
  height: number;
  fullscreen: boolean;
  placement?: OutputPlacement;
  look_id: string;
}

export interface SceneState {
  revision: number;
  blackout: boolean;
  active_slide: Slide | null;
  next_slide_preview: Slide | null;
  active_background?: ActiveBackground | null;
  active_foreground_media?: ForegroundMedia | null;
  video_input_active: boolean;
  media_layer_active: boolean;
  slide_layer_active: boolean;
  announcements_layer_active: boolean;
  overlay_layer_active: boolean;
  mask_style: MaskStyle;
  announcement_text: string | null;
  stage_message: string | null;
  timer_readout: string | null;
  looks: Record<string, LookConfig>;
  outputs: OutputConfig[];
  deck: Slide[];
}

export interface DisplayInfo {
  index: number;
  name: string;
  width: number;
  height: number;
  scale_factor: number;
  refresh_millihertz: number | null;
  position_x: number;
  position_y: number;
}

export interface PreviewBusDescriptor {
  output_id: string;
  shm_path: string;
  width: number;
  height: number;
}

export interface OutputTelemetry {
  output_id: string;
  width: number;
  height: number;
  frames_presented: number;
}

export interface EngineTelemetry {
  fps: number;
  avg_frame_ms: number;
  p99_frame_ms: number;
  total_frames: number;
  dropped_frames: number;
  outputs: OutputTelemetry[];
}

export interface EngineSnapshot {
  protocol_version: number;
  engine_pid: number;
  gpu_adapter_name: string;
  gpu_backend: string;
  state: SceneState;
  displays: DisplayInfo[];
  preview_buses: PreviewBusDescriptor[];
  telemetry: EngineTelemetry;
}

export interface PresentationLibraryRecord {
  id: string;
  name: string;
  updated_at: string;
}

export interface MediaBinRecord {
  id: string;
  name: string;
  category: string;
  updated_at: string;
}

export interface MediaAssetRecord {
  blake3_hash: string;
  bin_id: string;
  file_name: string;
  local_path: string;
  kind: string;
  width: number | null;
  height: number | null;
  duration_ms: number | null;
  has_alpha: boolean;
  size_bytes: number;
}

export interface SongRecord {
  id: string;
  library_id?: string;
  title: string;
  author: string;
  copyright: string;
  ccli_number: string | null;
  key_signature: string | null;
  bpm: number | null;
  lyrics_plain: string;
  slides: Slide[];
  updated_at: string;
}

export interface PlaylistRecord {
  id: string;
  name: string;
  item_song_ids: string[];
  updated_at: string;
}

export interface DiagnosticsStatus {
  log_dir: string;
  crash_dir: string;
  engine_log_path: string;
  desktop_log_path: string;
  active_log_level: string;
  crash_count: number;
}

export interface CrashReport {
  id: string;
  timestamp_utc: string;
  component: string;
  kind: string;
  app_version: string;
  pid: number;
  os: string;
  arch: string;
  thread: string;
  message: string;
  location: string | null;
  backtrace: string;
  recent_logs: string[];
}

export interface ExportedBundle {
  file_path: string;
  crash_count: number;
  markdown: string;
}
