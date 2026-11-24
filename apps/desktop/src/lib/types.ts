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

export interface OverlayItem {
  id: string;
  name: string;
  text: string;
  bounds: RectF;
  style: TextStyle;
  accent_color?: ColorRgba | null;
  locked: boolean;
  auto_clear_secs?: number | null;
}

export interface MessageTemplate {
  id: string;
  name: string;
  template_text: string;
  show_on_audience: boolean;
  show_on_stage: boolean;
  auto_clear_secs?: number | null;
}

export interface ActiveMessage {
  template_id: string | null;
  rendered_text: string;
  show_on_audience: boolean;
  show_on_stage: boolean;
}

export interface MacroRecord {
  id: string;
  name: string;
  steps: string[];
  updated_at: string;
}

export interface ScheduledEventRecord {
  id: string;
  name: string;
  trigger_hhmm: string;
  macro_id: string;
  enabled: boolean;
  updated_at: string;
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
  auto_advance_secs?: number | null;
  loop_to_first?: boolean;
  primary_color: ColorRgba;
  secondary_color: ColorRgba;
  accent_color: ColorRgba;
  motion_style: string;
  transition: TransitionSpec;
  elements: SlideElement[];
}

export interface TemplateConfig {
  id: string;
  name: string;
  category: string;
  text_bounds: RectF;
  subtitle_bounds?: RectF | null;
  text_style: TextStyle;
  subtitle_style?: TextStyle | null;
  accent_bar_color?: ColorRgba | null;
  default_mask: MaskStyle;
}

export interface LookConfig {
  id: string;
  name: string;
  description?: string;
  enabled_layers: LayerKind[];
  lower_third_mode: boolean;
  transparent_background: boolean;
  slide_blend_mode: string;
  template_id?: string | null;
  mask_override?: MaskStyle | null;
}

export interface LookPresetRecord {
  id: string;
  name: string;
  assignments: Record<string, string>;
  updated_at: string;
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

export type StageDisplayMode = "Confidence" | "ChordChart" | "SheetMusic";

export interface StageLayoutConfig {
  name: string;
  mode?: StageDisplayMode;
  show_current_slide: boolean;
  show_next_slide: boolean;
  show_chords: boolean;
  show_clock: boolean;
  show_timer: boolean;
  show_stage_message: boolean;
  current_rect: RectF;
  next_rect: RectF;
  chords_rect: RectF;
  timer_rect: RectF;
  message_rect: RectF;
  chord_lookahead_sections?: number;
  chord_extra_lines?: number;
  transpose_semitones?: number;
  sheet_two_page_spread?: boolean;
  sheet_dark_invert?: boolean;
}

export interface ChordChartLine {
  chords: string | null;
  lyrics: string;
}

export interface ChordChartSection {
  group: string;
  lines: ChordChartLine[];
}

export interface ChordChartDocument {
  title: string;
  key_signature: string | null;
  tempo_bpm: number | null;
  time_signature: string | null;
  sections: ChordChartSection[];
}

export interface SheetMusicPage {
  page_number: number;
  label: string;
  image_path: string | null;
  staff_lines_hint: string[];
}

export interface SheetMusicDocument {
  title: string;
  source_path: string | null;
  pages: SheetMusicPage[];
  group_to_page: Record<string, number>;
  slide_to_page: Record<string, number>;
}

export interface OutputCalibration {
  edge_blend_left: number;
  edge_blend_right: number;
  edge_blend_top: number;
  edge_blend_bottom: number;
  edge_blend_gamma: number;
  brightness: number;
  contrast: number;
  gamma: number;
}

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
  stage_layout?: StageLayoutConfig | null;
  calibration?: OutputCalibration;
}

export type MediaFitMode = "Fill" | "Contain" | "Stretch";

export interface MediaTransportState {
  playing: boolean;
  looping: boolean;
  muted: boolean;
  volume: number;
  fit_mode: MediaFitMode;
  position_secs: number;
  duration_secs: number;
}

export interface AudioPlaybackState {
  title: string;
  media_path: string;
  playing: boolean;
  looping: boolean;
  volume: number;
}

export interface Arrangement {
  id: string;
  name: string;
  group_order: string[];
}

export interface RecordedCue {
  slide_id: string;
  delay_secs: number;
  label: string;
}

export interface AutoPlayTrack {
  id: string;
  name: string;
  loop_playback: boolean;
  cues: RecordedCue[];
}

export interface BibleVerseRecord {
  translation: string;
  book: string;
  chapter: number;
  verse: number;
  text: string;
  attribution: string;
  cached_at_epoch: number | null;
}

export type CcliDisplayPlacement =
  | "Disabled"
  | "FirstSlide"
  | "LastSlide"
  | "FirstAndLastSlide"
  | "AllSlides";

export interface CcliDisplayConfig {
  enabled: boolean;
  placement: CcliDisplayPlacement;
  church_license_number: string;
  template: string;
  font_size: number;
  text_color: ColorRgba;
  background_color: ColorRgba;
  bounds: RectF;
  h_align: "Left" | "Center" | "Right";
}

export interface SceneState {
  revision: number;
  blackout: boolean;
  freeze_output?: boolean;
  active_slide: Slide | null;
  next_slide_preview: Slide | null;
  active_background?: ActiveBackground | null;
  active_foreground_media?: ForegroundMedia | null;
  media_transport?: MediaTransportState;
  active_audio?: AudioPlaybackState | null;
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
  templates?: Record<string, TemplateConfig>;
  overlays?: Record<string, OverlayItem>;
  active_overlay_ids?: string[];
  active_message?: ActiveMessage | null;
  variables?: Record<string, string>;
  outputs: OutputConfig[];
  deck: Slide[];
  master_deck?: Slide[];
  active_presentation_id?: string | null;
  active_presentation_title?: string | null;
  active_arrangements?: Arrangement[];
  active_arrangement_id?: string | null;
  active_cue_tracks?: AutoPlayTrack[];
  active_chord_chart?: ChordChartDocument | null;
  active_sheet_music?: SheetMusicDocument | null;
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
