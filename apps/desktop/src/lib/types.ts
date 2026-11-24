export interface ColorRgba {
  r: number;
  g: number;
  b: number;
  a: number;
}

export type TransitionSpec =
  | "Cut"
  | { Dissolve: { duration_ms: number } };

export type LayerKind =
  | "VideoInput"
  | "MediaBackground"
  | "Slide"
  | "Announcements"
  | "PropsAndOverlays"
  | "Masks"
  | "MessagesAndTimers";

export interface Slide {
  id: string;
  label: string;
  group: string;
  headline: string;
  body: string;
  primary_color: ColorRgba;
  secondary_color: ColorRgba;
  accent_color: ColorRgba;
  motion_style: string;
  transition: TransitionSpec;
}

export interface LookConfig {
  id: string;
  name: string;
  enabled_layers: LayerKind[];
  lower_third_mode: boolean;
  transparent_background: boolean;
}

export interface OutputConfig {
  id: string;
  name: string;
  kind: "Audience" | "Stage";
  monitor_index: number | null;
  width: number;
  height: number;
  fullscreen: boolean;
  look_id: string;
}

export interface SceneState {
  revision: number;
  blackout: boolean;
  active_slide: Slide | null;
  media_layer_active: boolean;
  slide_layer_active: boolean;
  overlay_layer_active: boolean;
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
