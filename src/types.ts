export type LanguageCode = "en" | "tr" | "de" | "es" | "fr" | "ru" | "ja" | "pt";

export type PositionPreset =
  | "top_center"
  | "top_left"
  | "top_right"
  | "bottom_center"
  | "custom";

export type AppearanceMode = "pure_black" | "glass" | "custom";
export type AccentMode = "preset" | "dynamic_album" | "custom";
export type InteractionMode = "hover" | "click";
export type AnimationSpeed = "smooth" | "balanced" | "snappy";
export type ExpandedTab = "media" | "volume" | "system" | "progress" | "timer";

export interface VolumeState {
  level: number;
  muted: boolean;
}

export interface AudioAppSession {
  pid: number;
  name: string;
  process_name: string;
  volume: number;
  muted: boolean;
  peak: number;
  active: boolean;
}

export interface MediaSessionSummary {
  id: string;
  app_id: string;
  title: string;
  artist: string;
  is_playing: boolean;
  is_active: boolean;
}

export interface MediaState {
  has_media: boolean;
  is_playing: boolean;
  status: "Playing" | "Paused" | "Stopped" | "Idle";
  title: string;
  artist: string;
  album: string;
  app_id: string;
  thumbnail: string | null;
  position_secs: number;
  duration_secs: number;
  playback_rate: number;
  start_time_secs: number;
  updated_at_ms: number;
  sessions: MediaSessionSummary[];
}

export interface BrightnessState {
  supported: boolean;
  level: number;
}

export interface SystemMetrics {
  cpu_percent: number;
  ram_percent: number;
  ram_used_gb: number;
  ram_total_gb: number;
  has_battery: boolean;
  battery_percent: number | null;
  is_charging: boolean;
}

export interface TaskProgress {
  id: string;
  title: string;
  subtitle: string;
  percent: number;
  source: "Explorer" | "App" | "Dropzone" | string;
}

export interface AppSettings {
  language: LanguageCode;
  monitor_name: string | null;
  position_preset: PositionPreset;
  offset_x: number;
  offset_y: number;
  custom_x: number | null;
  custom_y: number | null;
  unlock_position: boolean;
  scale: number;
  border_radius_mode: number;
  appearance: AppearanceMode;
  background_opacity: number;
  accent_mode: AccentMode;
  accent_color: string;
  auto_collapse_on_focus_loss: boolean;
  mouse_leave_delay_ms: number;
  interaction_mode: InteractionMode;
  animation_speed: AnimationSpeed;
  start_with_windows: boolean;
  auto_hide_fullscreen: boolean;
  auto_hide_idle_secs: number;
  show_system_metrics_in_idle: boolean;
  show_media_controls: boolean;
  hud_duration_ms: number;
  enable_visualizer: boolean;
  enable_media_module: boolean;
  enable_volume_module: boolean;
  enable_system_module: boolean;
  enable_progress_tracker: boolean;
  enable_timer_module: boolean;
  enable_dropzone_module: boolean;
  module_order: ExpandedTab[];
}

export interface MonitorInfo {
  name: string;
  width: number;
  height: number;
  x: number;
  y: number;
  scale_factor: number;
  is_primary: boolean;
}

export interface InitialState {
  media: MediaState;
  volume: VolumeState;
  brightness: BrightnessState;
  metrics: SystemMetrics;
  tasks: TaskProgress[];
  settings: AppSettings;
  monitors: MonitorInfo[];
}

export interface TimerState {
  totalSeconds: number;
  remainingSeconds: number;
  running: boolean;
  finished: boolean;
}
