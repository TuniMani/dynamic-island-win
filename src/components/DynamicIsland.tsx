import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { AnimatePresence, motion } from "framer-motion";
import {
  Activity,
  ChevronUp,
  Cpu,
  Disc3,
  GripHorizontal,
  PlugZap,
  Settings,
  Timer as TimerIcon,
} from "lucide-react";
import {
  lazy,
  Suspense,
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { DEFAULT_ACCENT, getTranslations, normalizeLanguage } from "../i18n";
import {
  AnimationSpeed,
  AppSettings,
  AudioAppSession,
  BrightnessState,
  ExpandedTab,
  InitialState,
  MediaState,
  SystemMetrics,
  TaskProgress,
  TimerState,
  VolumeState,
} from "../types";
import { extractAlbumAccent, hexToRgba } from "../utils/color";
import { AudioVisualizer } from "./AudioVisualizer";
import { CircularProgress } from "./CircularProgress";
import { MarqueeText } from "./MarqueeText";

const DropzoneOverlay = lazy(() =>
  import("./DropzoneOverlay").then((m) => ({ default: m.DropzoneOverlay })),
);
const HudOverlay = lazy(() =>
  import("./HudOverlay").then((m) => ({ default: m.HudOverlay })),
);
const MediaExpanded = lazy(() =>
  import("./MediaExpanded").then((m) => ({ default: m.MediaExpanded })),
);
const ProgressExpanded = lazy(() =>
  import("./ProgressExpanded").then((m) => ({ default: m.ProgressExpanded })),
);
const SystemExpanded = lazy(() =>
  import("./SystemExpanded").then((m) => ({ default: m.SystemExpanded })),
);
const TimerExpanded = lazy(() =>
  import("./TimerExpanded").then((m) => ({ default: m.TimerExpanded })),
);
const VolumeExpanded = lazy(() =>
  import("./VolumeExpanded").then((m) => ({ default: m.VolumeExpanded })),
);

const BOOT_PREFS_KEY = "di_boot_prefs";

const CANVAS_W = 640;
const CANVAS_H = 320;

function getSpringConfig(speed: AnimationSpeed) {
  switch (speed) {
    case "smooth":
      return { type: "spring" as const, stiffness: 250, damping: 30, mass: 1.0 };
    case "snappy":
      return { type: "spring" as const, stiffness: 520, damping: 34, mass: 0.65 };
    default:
      return { type: "spring" as const, stiffness: 360, damping: 32, mass: 0.85 };
  }
}

const CONTENT_TRANSITION = {
  duration: 0.16,
  ease: [0.22, 1, 0.36, 1] as const,
};

const DEFAULT_MEDIA: MediaState = {
  has_media: false,
  is_playing: false,
  status: "Idle",
  title: "",
  artist: "",
  album: "",
  app_id: "",
  thumbnail: null,
  position_secs: 0,
  duration_secs: 0,
  playback_rate: 1,
  start_time_secs: 0,
  updated_at_ms: 0,
  sessions: [],
};

const DEFAULT_METRICS: SystemMetrics = {
  cpu_percent: 0,
  ram_percent: 0,
  ram_used_gb: 0,
  ram_total_gb: 0,
  has_battery: false,
  battery_percent: null,
  is_charging: true,
};

const DEFAULT_SETTINGS: AppSettings = {
  language: "en",
  monitor_name: null,
  position_preset: "top_center",
  offset_x: 0,
  offset_y: 8,
  custom_x: null,
  custom_y: null,
  unlock_position: false,
  scale: 1.0,
  border_radius_mode: 999,
  appearance: "pure_black",
  background_opacity: 85,
  accent_mode: "preset",
  accent_color: DEFAULT_ACCENT,
  auto_collapse_on_focus_loss: true,
  mouse_leave_delay_ms: 1000,
  interaction_mode: "hover",
  animation_speed: "balanced",
  start_with_windows: false,
  auto_hide_fullscreen: false,
  auto_hide_idle_secs: 0,
  show_system_metrics_in_idle: false,
  show_media_controls: true,
  hud_duration_ms: 2200,
  enable_visualizer: true,
  enable_media_module: true,
  enable_volume_module: true,
  enable_system_module: true,
  enable_progress_tracker: true,
  enable_timer_module: true,
  enable_dropzone_module: true,
  module_order: ["media", "volume", "system", "progress", "timer"],
};

function formatClock(): string {
  const d = new Date();
  return `${d.getHours().toString().padStart(2, "0")}:${d
    .getMinutes()
    .toString()
    .padStart(2, "0")}`;
}

function formatMMSS(sec: number): string {
  const s = Math.max(0, Math.floor(sec));
  const m = Math.floor(s / 60);
  const r = s % 60;
  return `${m.toString().padStart(2, "0")}:${r.toString().padStart(2, "0")}`;
}

function playTimerChime() {
  try {
    const AudioCtx =
      window.AudioContext ||
      (window as unknown as { webkitAudioContext: typeof AudioContext }).webkitAudioContext;
    const ctx = new AudioCtx();
    const notes = [880, 1174.66];
    notes.forEach((freq, idx) => {
      const osc = ctx.createOscillator();
      const gain = ctx.createGain();
      osc.type = "sine";
      osc.frequency.value = freq;
      const start = ctx.currentTime + idx * 0.14;
      gain.gain.setValueAtTime(0.001, start);
      gain.gain.exponentialRampToValueAtTime(0.08, start + 0.03);
      gain.gain.exponentialRampToValueAtTime(0.0001, start + 0.38);
      osc.connect(gain);
      gain.connect(ctx.destination);
      osc.start(start);
      osc.stop(start + 0.4);
    });
  } catch {
    // Ignore audio restriction
  }
}

function loadInitialSettings(): AppSettings {
  if (typeof window === "undefined") return DEFAULT_SETTINGS;
  const cachedLang = normalizeLanguage(window.localStorage.getItem("di_lang"));
  try {
    const raw = window.localStorage.getItem(BOOT_PREFS_KEY);
    if (!raw) return { ...DEFAULT_SETTINGS, language: cachedLang };
    const parsed = JSON.parse(raw) as Partial<AppSettings>;
    return {
      ...DEFAULT_SETTINGS,
      language: normalizeLanguage(parsed.language ?? cachedLang),
      position_preset:
        parsed.position_preset === "top_left" ||
        parsed.position_preset === "top_right" ||
        parsed.position_preset === "bottom_center" ||
        parsed.position_preset === "custom"
          ? parsed.position_preset
          : "top_center",
      scale:
        typeof parsed.scale === "number" && Number.isFinite(parsed.scale)
          ? Math.max(0.75, Math.min(1.4, parsed.scale))
          : DEFAULT_SETTINGS.scale,
      border_radius_mode:
        typeof parsed.border_radius_mode === "number" &&
        Number.isFinite(parsed.border_radius_mode)
          ? parsed.border_radius_mode
          : DEFAULT_SETTINGS.border_radius_mode,
      appearance:
        parsed.appearance === "glass" || parsed.appearance === "custom"
          ? parsed.appearance
          : "pure_black",
      background_opacity:
        typeof parsed.background_opacity === "number" &&
        Number.isFinite(parsed.background_opacity)
          ? parsed.background_opacity
          : DEFAULT_SETTINGS.background_opacity,
      accent_mode:
        parsed.accent_mode === "dynamic_album" || parsed.accent_mode === "custom"
          ? parsed.accent_mode
          : "preset",
      accent_color:
        typeof parsed.accent_color === "string" && parsed.accent_color.length > 0
          ? parsed.accent_color
          : DEFAULT_ACCENT,
      show_media_controls:
        typeof parsed.show_media_controls === "boolean"
          ? parsed.show_media_controls
          : DEFAULT_SETTINGS.show_media_controls,
    };
  } catch {
    return { ...DEFAULT_SETTINGS, language: cachedLang };
  }
}

function saveBootPrefs(s: AppSettings) {
  if (typeof window === "undefined") return;
  try {
    window.localStorage.setItem("di_lang", s.language);
    window.localStorage.setItem(
      BOOT_PREFS_KEY,
      JSON.stringify({
        language: s.language,
        position_preset: s.position_preset,
        scale: s.scale,
        border_radius_mode: s.border_radius_mode,
        appearance: s.appearance,
        background_opacity: s.background_opacity,
        accent_mode: s.accent_mode,
        accent_color: s.accent_color,
        show_media_controls: s.show_media_controls,
      }),
    );
  } catch {
    // Ignore storage quota errors
  }
}

export function DynamicIsland() {
  const [media, setMedia] = useState<MediaState>(DEFAULT_MEDIA);
  const [volume, setVolume] = useState<VolumeState>({ level: 50, muted: false });
  const [brightness, setBrightness] = useState<BrightnessState>({
    supported: false,
    level: 100,
  });
  const [metrics, setMetrics] = useState<SystemMetrics>(DEFAULT_METRICS);
  const [tasks, setTasks] = useState<TaskProgress[]>([]);
  const [audioSessions, setAudioSessions] = useState<AudioAppSession[]>([]);
  const [settings, setSettings] = useState<AppSettings>(loadInitialSettings);
  const [bootSettled, setBootSettled] = useState<boolean>(false);

  const [albumAccent, setAlbumAccent] = useState<string | null>(null);
  const [pillImgError, setPillImgError] = useState<boolean>(false);
  const [expanded, setExpanded] = useState<boolean>(false);
  const [isHovered, setIsHovered] = useState<boolean>(false);
  const [idleHidden, setIdleHidden] = useState<boolean>(false);
  const [activeTab, setActiveTab] = useState<ExpandedTab>("media");
  const [hudKind, setHudKind] = useState<"volume" | "brightness" | null>(null);
  const [dragOver, setDragOver] = useState<boolean>(false);
  const [droppedPaths, setDroppedPaths] = useState<string[]>([]);
  const [clock, setClock] = useState<string>(formatClock);

  const [timer, setTimer] = useState<TimerState>({
    totalSeconds: 300,
    remainingSeconds: 300,
    running: false,
    finished: false,
  });

  const hoverOpenTimerRef = useRef<number | null>(null);
  const hoverCloseTimerRef = useRef<number | null>(null);
  const hudTimerRef = useRef<number | null>(null);
  const settingsRef = useRef<AppSettings>(settings);
  const expandedRef = useRef<boolean>(expanded);
  const activeTabRef = useRef<ExpandedTab>(activeTab);
  const draggingPidRef = useRef<number | null>(null);
  const recentAppTouchRef = useRef<Map<number, number>>(new Map());
  const masterPendingRef = useRef<{ inflight: boolean; queued: number | null }>({
    inflight: false,
    queued: null,
  });

  // Reveal main pill window immediately once React commits frame 1
  useLayoutEffect(() => {
    void invoke("reveal_main_window");
  }, []);

  useEffect(() => {
    expandedRef.current = expanded;
  }, [expanded]);

  useEffect(() => {
    activeTabRef.current = activeTab;
  }, [activeTab]);

  useEffect(() => {
    const id = window.setTimeout(() => setBootSettled(true), 180);
    return () => window.clearTimeout(id);
  }, []);

  useEffect(() => {
    settingsRef.current = settings;
    saveBootPrefs(settings);
  }, [settings]);

  // Sync expanded flag to Rust Win32 monitor for auto-collapse detection
  useEffect(() => {
    void invoke("set_island_expanded", { expanded });
  }, [expanded]);

  const refreshAudioSessions = useCallback(async () => {
    try {
      const fresh = await invoke<AudioAppSession[]>("get_audio_sessions");
      const now = Date.now();
      setAudioSessions((prev) =>
        fresh.map((item) => {
          const touchedAt = recentAppTouchRef.current.get(item.pid) ?? 0;
          if (draggingPidRef.current === item.pid || now - touchedAt < 650) {
            const existing = prev.find((p) => p.pid === item.pid);
            if (existing) {
              return {
                ...item,
                volume: existing.volume,
                muted: existing.muted,
              };
            }
          }
          return item;
        }),
      );
    } catch {
      // Ignore transient COM session enumeration errors
    }
  }, []);

  useEffect(() => {
    if (
      !expanded ||
      (activeTab !== "volume" &&
        !(activeTab === "media" && settings.show_media_controls))
    ) {
      return;
    }
    void refreshAudioSessions();
    const id = window.setInterval(() => {
      void refreshAudioSessions();
    }, 850);
    return () => window.clearInterval(id);
  }, [expanded, activeTab, settings.show_media_controls, refreshAudioSessions]);

  const handleMasterVolumeChange = useCallback((level: number) => {
    setVolume({ level, muted: false });
    const state = masterPendingRef.current;
    if (state.inflight) {
      state.queued = level;
      return;
    }
    const flush = async (lvl: number) => {
      state.inflight = true;
      try {
        await invoke("set_system_volume", { level: lvl });
      } finally {
        state.inflight = false;
        const next = state.queued;
        if (next !== null) {
          state.queued = null;
          void flush(next);
        }
      }
    };
    void flush(level);
  }, []);

  const handleMasterMuteToggle = useCallback(() => {
    setVolume((prev) => ({ ...prev, muted: !prev.muted }));
    void invoke("toggle_system_mute");
  }, []);

  const handleAppVolumeChange = useCallback((pid: number, level: number) => {
    recentAppTouchRef.current.set(pid, Date.now());
    setAudioSessions((prev) =>
      prev.map((s) => (s.pid === pid ? { ...s, volume: level } : s)),
    );
    void invoke("set_app_volume", { pid, level });
  }, []);

  const handleAppMuteToggle = useCallback(
    async (pid: number) => {
      recentAppTouchRef.current.set(pid, Date.now());
      setAudioSessions((prev) =>
        prev.map((s) => (s.pid === pid ? { ...s, muted: !s.muted } : s)),
      );
      try {
        const nextMuted = await invoke<boolean>("toggle_app_mute", { pid });
        recentAppTouchRef.current.set(pid, Date.now());
        setAudioSessions((prev) =>
          prev.map((s) => (s.pid === pid ? { ...s, muted: nextMuted } : s)),
        );
      } catch {
        void refreshAudioSessions();
      }
    },
    [refreshAudioSessions],
  );

  const handleAppDragStateChange = useCallback((pid: number | null) => {
    const prevPid = draggingPidRef.current;
    draggingPidRef.current = pid;
    if (pid === null && prevPid !== null) {
      recentAppTouchRef.current.set(prevPid, Date.now());
    }
  }, []);

  // Extract dominant album art color when dynamic_album accent mode is active
  useEffect(() => {
    setPillImgError(false);
    let active = true;
    if (settings.accent_mode !== "dynamic_album" || !media.thumbnail) {
      setAlbumAccent(null);
      return;
    }
    void extractAlbumAccent(media.thumbnail).then((hex) => {
      if (active) setAlbumAccent(hex);
    });
    return () => {
      active = false;
    };
  }, [settings.accent_mode, media.thumbnail]);

  // Clock tick every 15s
  useEffect(() => {
    const id = window.setInterval(() => setClock(formatClock()), 15000);
    return () => window.clearInterval(id);
  }, []);

  // Timer tick when running
  useEffect(() => {
    if (!timer.running) return;
    const id = window.setInterval(() => {
      setTimer((prev) => {
        if (!prev.running) return prev;
        if (prev.remainingSeconds <= 1) {
          playTimerChime();
          return {
            ...prev,
            remainingSeconds: 0,
            running: false,
            finished: true,
          };
        }
        return {
          ...prev,
          remainingSeconds: prev.remainingSeconds - 1,
        };
      });
    }, 1000);
    return () => window.clearInterval(id);
  }, [timer.running]);

  const triggerHud = useCallback((kind: "volume" | "brightness") => {
    setHudKind(kind);
    setIdleHidden(false);
    if (hudTimerRef.current) {
      window.clearTimeout(hudTimerRef.current);
    }
    hudTimerRef.current = window.setTimeout(() => {
      setHudKind(null);
    }, settingsRef.current.hud_duration_ms);
  }, []);

  // Subscribe to Tauri backend state & events
  useEffect(() => {
    let mounted = true;
    const unlisteners: Array<() => void> = [];

    void invoke<InitialState>("get_initial_state")
      .then((init) => {
        if (!mounted) return;
        setMedia(init.media);
        setVolume(init.volume);
        setBrightness(init.brightness);
        setMetrics(init.metrics);
        setTasks(init.tasks);
        setSettings(init.settings);
      })
      .catch((err) => {
        console.error("[DynamicIsland] get_initial_state failed:", err);
      });

    const setupListeners = async () => {
      unlisteners.push(
        await listen<MediaState>("media-changed", (e) => {
          setMedia(e.payload);
        }),
      );
      unlisteners.push(
        await listen<VolumeState>("volume-changed", (e) => {
          setVolume(e.payload);
          if (
            !(
              expandedRef.current &&
              (activeTabRef.current === "volume" || activeTabRef.current === "media")
            )
          ) {
            triggerHud("volume");
          }
        }),
      );
      unlisteners.push(
        await listen<VolumeState>("volume-changed-init", (e) => {
          setVolume(e.payload);
        }),
      );
      unlisteners.push(
        await listen<BrightnessState>("brightness-changed", (e) => {
          setBrightness(e.payload);
          triggerHud("brightness");
        }),
      );
      unlisteners.push(
        await listen<SystemMetrics>("system-metrics", (e) => {
          setMetrics(e.payload);
        }),
      );
      unlisteners.push(
        await listen<TaskProgress[]>("progress-update", (e) => {
          setTasks(e.payload);
        }),
      );
      unlisteners.push(
        await listen<AppSettings>("settings-changed", (e) => {
          setSettings(e.payload);
        }),
      );
      unlisteners.push(
        await listen<string>("island-auto-collapse", () => {
          setExpanded(false);
        }),
      );

      try {
        const unlistenDrop = await getCurrentWebview().onDragDropEvent((event) => {
          if (!settingsRef.current.enable_dropzone_module) return;
          if (event.payload.type === "enter" || event.payload.type === "over") {
            setDragOver(true);
            setIdleHidden(false);
          } else if (event.payload.type === "drop") {
            setDragOver(false);
            if (event.payload.paths.length > 0) {
              setDroppedPaths(event.payload.paths);
              setExpanded(false);
            }
          } else if (event.payload.type === "leave") {
            setDragOver(false);
          }
        });
        unlisteners.push(unlistenDrop);
      } catch (err) {
        console.warn("[Dropzone] dragDrop listener unavailable:", err);
      }
    };

    void setupListeners();

    return () => {
      mounted = false;
      unlisteners.forEach((fn) => fn());
      if (hudTimerRef.current) window.clearTimeout(hudTimerRef.current);
      if (hoverOpenTimerRef.current) window.clearTimeout(hoverOpenTimerRef.current);
      if (hoverCloseTimerRef.current) window.clearTimeout(hoverCloseTimerRef.current);
    };
  }, [triggerHud]);

  const enabledTabs = useMemo<ExpandedTab[]>(() => {
    const isEnabled = (id: ExpandedTab): boolean => {
      switch (id) {
        case "media":
          return settings.enable_media_module;
        case "volume":
          return settings.enable_volume_module;
        case "system":
          return settings.enable_system_module;
        case "progress":
          return settings.enable_progress_tracker;
        case "timer":
          return settings.enable_timer_module;
      }
    };
    const ordered = settings.module_order.filter(isEnabled);
    // Append any enabled tab missing from custom order
    (["media", "volume", "system", "progress", "timer"] as ExpandedTab[]).forEach((id) => {
      if (isEnabled(id) && !ordered.includes(id)) ordered.push(id);
    });
    return ordered;
  }, [
    settings.module_order,
    settings.enable_media_module,
    settings.enable_volume_module,
    settings.enable_system_module,
    settings.enable_progress_tracker,
    settings.enable_timer_module,
  ]);

  useEffect(() => {
    if (enabledTabs.length > 0 && !enabledTabs.includes(activeTab)) {
      setActiveTab(enabledTabs[0]);
    }
  }, [enabledTabs, activeTab]);

  const isDropActive =
    settings.enable_dropzone_module && (dragOver || droppedPaths.length > 0);

  const showProgressCompact = settings.enable_progress_tracker && tasks.length > 0;
  const showMediaCompact =
    settings.enable_media_module && media.has_media && !showProgressCompact;
  const showTimerCompact =
    settings.enable_timer_module &&
    !showProgressCompact &&
    !showMediaCompact &&
    (timer.running || timer.finished);
  const showMetricsCompact =
    settings.enable_system_module &&
    !showProgressCompact &&
    !showMediaCompact &&
    !showTimerCompact &&
    settings.show_system_metrics_in_idle;

  const isPureIdle =
    !expanded &&
    !isDropActive &&
    hudKind === null &&
    !showProgressCompact &&
    !showMediaCompact &&
    !showTimerCompact &&
    !showMetricsCompact;

  // Auto-hide when idle timer
  useEffect(() => {
    if (settings.auto_hide_idle_secs <= 0 || !isPureIdle || isHovered) {
      setIdleHidden(false);
      return;
    }
    const id = window.setTimeout(() => {
      setIdleHidden(true);
    }, settings.auto_hide_idle_secs * 1000);
    return () => window.clearTimeout(id);
  }, [settings.auto_hide_idle_secs, isPureIdle, isHovered]);

  const chooseDefaultTab = (): ExpandedTab => {
    if (settings.enable_progress_tracker && tasks.length > 0) return "progress";
    if (settings.enable_media_module && media.has_media) return "media";
    if (settings.enable_timer_module && (timer.running || timer.finished)) return "timer";
    if (settings.enable_system_module && settings.show_system_metrics_in_idle) return "system";
    if (enabledTabs.includes(activeTab)) return activeTab;
    return enabledTabs[0] ?? "media";
  };

  const handleMouseEnter = () => {
    setIsHovered(true);
    setIdleHidden(false);
    if (hoverCloseTimerRef.current) {
      window.clearTimeout(hoverCloseTimerRef.current);
      hoverCloseTimerRef.current = null;
    }
    if (
      isDropActive ||
      expanded ||
      hudKind !== null ||
      settings.interaction_mode !== "hover"
    )
      return;
    hoverOpenTimerRef.current = window.setTimeout(() => {
      setActiveTab(chooseDefaultTab());
      setExpanded(true);
    }, 45);
  };

  const handleMouseLeave = () => {
    setIsHovered(false);
    if (hoverOpenTimerRef.current) {
      window.clearTimeout(hoverOpenTimerRef.current);
      hoverOpenTimerRef.current = null;
    }
    if (isDropActive || settings.mouse_leave_delay_ms < 0) return;
    const delay = settings.mouse_leave_delay_ms;
    hoverCloseTimerRef.current = window.setTimeout(() => {
      setExpanded(false);
    }, delay);
  };

  const handlePillClick = () => {
    if (isDropActive) return;
    setIdleHidden(false);
    if (!expanded) {
      setActiveTab(chooseDefaultTab());
      setExpanded(true);
    } else if (settings.interaction_mode === "click") {
      setExpanded(false);
    }
  };

  const handleDragMouseDown = (e: React.MouseEvent) => {
    if (!settings.unlock_position || e.button !== 0) return;
    e.stopPropagation();
    void invoke("start_island_drag");
  };

  const t = getTranslations(settings.language);
  const spring = useMemo(() => getSpringConfig(settings.animation_speed), [settings.animation_speed]);
  const accentColor =
    settings.accent_mode === "dynamic_album" && media.has_media && albumAccent
      ? albumAccent
      : settings.accent_color || DEFAULT_ACCENT;
  const scale = Math.max(0.75, Math.min(1.4, settings.scale || 1.0));

  const isCompactMediaExpanded =
    expanded &&
    activeTab === "media" &&
    !settings.show_media_controls;

  // Base unscaled dimensions & corner radius
  let baseWidth = 136;
  let baseHeight = 34;
  let baseRadius = 18;
  let viewKey = "compact-idle";

  if (idleHidden) {
    baseWidth = 54;
    baseHeight = 8;
    baseRadius = 4;
    viewKey = "idle-hidden";
  } else if (droppedPaths.length > 0 && settings.enable_dropzone_module) {
    baseWidth = 418;
    baseHeight = 116;
    baseRadius = 24;
    viewKey = "dropzone";
  } else if (dragOver && settings.enable_dropzone_module) {
    baseWidth = 320;
    baseHeight = 76;
    baseRadius = 24;
    viewKey = "dropzone";
  } else if (expanded) {
    baseWidth = 414;
    baseHeight = isCompactMediaExpanded ? 192 : 212;
    baseRadius = 28;
    viewKey = "expanded";
  } else if (hudKind !== null) {
    baseWidth = 272;
    baseHeight = 46;
    baseRadius = 23;
    viewKey = "hud";
  } else if (showProgressCompact) {
    baseWidth = 236;
    baseHeight = 36;
    baseRadius = 18;
    viewKey = "compact-progress";
  } else if (showMediaCompact) {
    baseWidth = timer.running && settings.enable_timer_module ? 268 : 226;
    baseHeight = 36;
    baseRadius = 18;
    viewKey = "compact-media";
  } else if (showTimerCompact) {
    baseWidth = 176;
    baseHeight = 34;
    baseRadius = 18;
    viewKey = "compact-timer";
  } else if (showMetricsCompact) {
    baseWidth = 232;
    baseHeight = 34;
    baseRadius = 18;
    viewKey = "compact-metrics";
  }

  const scaledWidth = Math.round(baseWidth * scale);
  const scaledHeight = Math.round(baseHeight * scale);
  const maxPillRadius = Math.round(scaledHeight / 2);
  const targetRadius = idleHidden
    ? 4
    : Math.min(
        settings.border_radius_mode,
        expanded || isDropActive ? Math.round(baseRadius * scale) : maxPillRadius,
      );

  // Compute exact viewport alignment and hitbox based on position_preset
  let alignClass = "items-start justify-center pt-[4px]";
  let hitX = (CANVAS_W - scaledWidth) / 2;
  let hitY = 4;

  if (settings.position_preset === "top_left") {
    alignClass = "items-start justify-start pt-[4px] pl-[12px]";
    hitX = 12;
    hitY = 4;
  } else if (settings.position_preset === "top_right") {
    alignClass = "items-start justify-end pt-[4px] pr-[12px]";
    hitX = CANVAS_W - 12 - scaledWidth;
    hitY = 4;
  } else if (settings.position_preset === "bottom_center") {
    alignClass = "items-end justify-center pb-[4px]";
    hitX = (CANVAS_W - scaledWidth) / 2;
    hitY = CANVAS_H - 4 - scaledHeight;
  }

  useEffect(() => {
    void invoke("update_island_hit_rect", {
      x: hitX,
      y: hitY,
      width: scaledWidth,
      height: scaledHeight,
    });
  }, [hitX, hitY, scaledWidth, scaledHeight]);

  const customAlpha = Math.max(20, Math.min(100, settings.background_opacity)) / 100;
  const appearanceStyle =
    settings.appearance === "glass"
      ? {
          backgroundColor: `rgba(12, 14, 22, ${Math.min(0.85, customAlpha)})`,
          backdropFilter: "blur(24px) saturate(160%)",
          WebkitBackdropFilter: "blur(24px) saturate(160%)",
        }
      : settings.appearance === "custom"
        ? {
            backgroundColor: `rgba(8, 10, 15, ${customAlpha})`,
            backdropFilter: customAlpha < 0.96 ? "blur(18px)" : undefined,
            WebkitBackdropFilter: customAlpha < 0.96 ? "blur(18px)" : undefined,
          }
        : {
            backgroundColor: "#000000",
          };

  return (
    <div className={`flex h-full w-full ${alignClass}`}>
      <motion.div
        onMouseEnter={handleMouseEnter}
        onMouseLeave={handleMouseLeave}
        onMouseDown={!expanded ? handleDragMouseDown : undefined}
        onClick={handlePillClick}
        initial={false}
        animate={{
          width: scaledWidth,
          height: scaledHeight,
          borderRadius: targetRadius,
          opacity: idleHidden ? 0.35 : 1,
        }}
        transition={bootSettled ? spring : { duration: 0 }}
        style={{
          ...appearanceStyle,
          border: timer.finished
            ? "1px solid rgba(52, 211, 153, 0.55)"
            : settings.unlock_position
              ? `1px solid ${hexToRgba(accentColor, 0.55)}`
              : "1px solid rgba(255, 255, 255, 0.12)",
          boxShadow: "0 12px 32px rgba(0, 0, 0, 0.55)",
          cursor: settings.unlock_position && !expanded ? "grab" : "default",
        }}
        className="di-pill-enter relative overflow-hidden text-white will-change-transform"
      >
        <div
          style={{
            width: baseWidth,
            height: baseHeight,
            transform: scale !== 1 ? `scale(${scale})` : undefined,
            transformOrigin: "top left",
          }}
        >
          <AnimatePresence mode="popLayout" initial={false}>
            <motion.div
              key={viewKey}
              initial={{ opacity: 0, scale: 0.96 }}
              animate={{ opacity: 1, scale: 1 }}
              exit={{ opacity: 0, scale: 0.96 }}
              transition={CONTENT_TRANSITION}
              className="h-full w-full"
            >
              {idleHidden ? (
                <div className="h-full w-full" />
              ) : isDropActive ? (
                <Suspense fallback={null}>
                  <DropzoneOverlay
                    dragOver={dragOver}
                    paths={droppedPaths}
                    lang={settings.language}
                    accentColor={accentColor}
                    onClear={() => setDroppedPaths([])}
                  />
                </Suspense>
              ) : expanded ? (
                <div
                  className="flex h-full w-full flex-col justify-between p-3.5"
                  onClick={(e) => e.stopPropagation()}
                >
                  {/* Top Tabs & Controls */}
                  <div className="flex items-center justify-between gap-1.5">
                    <div className="flex min-w-0 max-w-full items-center gap-0.5 overflow-hidden rounded-full bg-white/[0.06] p-0.5">
                      {enabledTabs.map((tabId) => {
                        const active = activeTab === tabId;
                        return (
                          <button
                            key={tabId}
                            type="button"
                            title={t.tabs[tabId]}
                            onClick={() => setActiveTab(tabId)}
                            className={`relative min-w-0 truncate whitespace-nowrap rounded-full px-2 py-1 text-[10px] font-medium transition ${
                              active ? "text-black" : "text-white/65 hover:text-white"
                            }`}
                          >
                            {active && (
                              <motion.span
                                layoutId="island-tab-pill"
                                transition={spring}
                                className="absolute inset-0 rounded-full bg-white"
                              />
                            )}
                            <span className="relative z-10 flex items-center justify-center gap-1">
                              <span className="truncate">{t.tabs[tabId]}</span>
                              {tabId === "progress" && tasks.length > 0 && (
                                <span
                                  className="h-1.5 w-1.5 shrink-0 rounded-full"
                                  style={{ backgroundColor: accentColor }}
                                />
                              )}
                            </span>
                          </button>
                        );
                      })}
                    </div>

                    <div className="flex shrink-0 items-center gap-1">
                      {settings.unlock_position && (
                        <button
                          type="button"
                          onMouseDown={handleDragMouseDown}
                          className="flex h-6 w-6 cursor-grab items-center justify-center rounded-full bg-white/10 text-white/80 hover:bg-white/20"
                          style={{ color: accentColor }}
                          title={t.dragTooltip}
                        >
                          <GripHorizontal className="h-3.5 w-3.5" />
                        </button>
                      )}
                      <button
                        type="button"
                        onClick={() => void invoke("open_settings_window")}
                        className="flex h-6 w-6 items-center justify-center rounded-full bg-white/8 text-white/65 transition hover:bg-white/16 hover:text-white"
                        title={t.openSettings}
                      >
                        <Settings className="h-3.5 w-3.5" />
                      </button>
                      <button
                        type="button"
                        onClick={() => setExpanded(false)}
                        className="flex h-6 w-6 items-center justify-center rounded-full bg-white/8 text-white/65 transition hover:bg-white/16 hover:text-white"
                        title={t.collapse}
                      >
                        <ChevronUp className="h-3.5 w-3.5" />
                      </button>
                    </div>
                  </div>

                  {/* Active Tab Body */}
                  <div className="min-h-0 flex-1">
                    {enabledTabs.length === 0 ? (
                      <div className="flex h-full items-center justify-center text-xs text-white/50">
                        {t.openSettings}
                      </div>
                    ) : (
                      <Suspense
                        fallback={
                          <div className={isCompactMediaExpanded ? "h-[126px]" : "h-[146px]"} />
                        }
                      >
                        {activeTab === "media" ? (
                          <MediaExpanded
                            media={media}
                            enableVisualizer={settings.enable_visualizer}
                            showMediaControls={settings.show_media_controls}
                            lang={settings.language}
                            accentColor={accentColor}
                            volume={volume}
                            sessions={audioSessions}
                            onMasterVolumeChange={handleMasterVolumeChange}
                            onMasterMuteToggle={handleMasterMuteToggle}
                            onAppVolumeChange={handleAppVolumeChange}
                            onAppMuteToggle={handleAppMuteToggle}
                            onAppDragStateChange={handleAppDragStateChange}
                          />
                        ) : activeTab === "volume" ? (
                          <VolumeExpanded
                            volume={volume}
                            sessions={audioSessions}
                            lang={settings.language}
                            accentColor={accentColor}
                            onMasterVolumeChange={handleMasterVolumeChange}
                            onMasterMuteToggle={handleMasterMuteToggle}
                            onAppVolumeChange={handleAppVolumeChange}
                            onAppMuteToggle={handleAppMuteToggle}
                            onAppDragStateChange={handleAppDragStateChange}
                          />
                        ) : activeTab === "system" ? (
                          <SystemExpanded
                            metrics={metrics}
                            lang={settings.language}
                            accentColor={accentColor}
                          />
                        ) : activeTab === "progress" ? (
                          <ProgressExpanded
                            tasks={tasks}
                            lang={settings.language}
                            accentColor={accentColor}
                          />
                        ) : (
                          <TimerExpanded
                            timer={timer}
                            lang={settings.language}
                            accentColor={accentColor}
                            onSetPreset={(min) =>
                              setTimer({
                                totalSeconds: min * 60,
                                remainingSeconds: min * 60,
                                running: true,
                                finished: false,
                              })
                            }
                            onAdjustSeconds={(delta) =>
                              setTimer((prev) => {
                                const nextRem = Math.max(10, prev.remainingSeconds + delta);
                                return {
                                  ...prev,
                                  totalSeconds: Math.max(prev.totalSeconds, nextRem),
                                  remainingSeconds: nextRem,
                                  finished: false,
                                };
                              })
                            }
                            onToggleRun={() =>
                              setTimer((prev) => ({
                                ...prev,
                                running: prev.remainingSeconds > 0 ? !prev.running : false,
                                finished: false,
                              }))
                            }
                            onReset={() =>
                              setTimer((prev) => ({
                                ...prev,
                                remainingSeconds: prev.totalSeconds,
                                running: false,
                                finished: false,
                              }))
                            }
                          />
                        )}
                      </Suspense>
                    )}
                  </div>
                </div>
              ) : hudKind !== null ? (
                <Suspense fallback={null}>
                  <HudOverlay
                    kind={hudKind}
                    level={hudKind === "volume" ? volume.level : brightness.level}
                    muted={volume.muted}
                    lang={settings.language}
                    accentColor={accentColor}
                    onInteract={() => triggerHud(hudKind)}
                    onVolumePreview={(lvl) =>
                      setVolume((prev) => ({ ...prev, level: lvl, muted: false }))
                    }
                  />
                </Suspense>
              ) : showProgressCompact ? (
                <div className="flex h-full w-full items-center justify-between gap-2 px-3">
                  <div className="flex min-w-0 items-center gap-2">
                    <Activity className="h-3.5 w-3.5 shrink-0" style={{ color: accentColor }} />
                    <span className="truncate text-[11px] font-medium text-white/90">
                      {tasks[0].title}
                    </span>
                  </div>
                  <div className="flex shrink-0 items-center gap-1.5">
                    <span
                      className="text-[10.5px] font-semibold tabular-nums"
                      style={{ color: accentColor }}
                    >
                      {tasks[0].percent}%
                    </span>
                    <CircularProgress
                      percent={tasks[0].percent}
                      size={20}
                      strokeWidth={2.5}
                      color={accentColor}
                    />
                  </div>
                </div>
              ) : showMediaCompact ? (
                <div className="flex h-full w-full items-center justify-between gap-2 px-2.5">
                  <div className="flex min-w-0 flex-1 items-center gap-2">
                    <div className="h-5 w-5 shrink-0 overflow-hidden rounded-full border border-white/15 bg-white/10">
                      {media.thumbnail && !pillImgError ? (
                        <img
                          src={media.thumbnail}
                          alt=""
                          onError={() => setPillImgError(true)}
                          className="h-full w-full object-cover"
                          draggable={false}
                        />
                      ) : (
                        <div className="flex h-full w-full items-center justify-center">
                          <Disc3 className="h-3 w-3" style={{ color: accentColor }} />
                        </div>
                      )}
                    </div>
                    <MarqueeText
                      text={media.title || t.media.sourceDefault}
                      className="text-[11px] font-medium text-white/90"
                    />
                  </div>

                  <div className="flex shrink-0 items-center gap-2 pr-0.5">
                    {timer.running && settings.enable_timer_module && (
                      <span className="rounded-full bg-amber-400/20 px-1.5 py-0.5 text-[9.5px] font-semibold tabular-nums text-amber-300">
                        {formatMMSS(timer.remainingSeconds)}
                      </span>
                    )}
                    {settings.enable_visualizer && (
                      <AudioVisualizer
                        active={media.is_playing}
                        compact
                        accentColor={accentColor}
                        livePeak={null}
                      />
                    )}
                  </div>
                </div>
              ) : showTimerCompact ? (
                <div className="flex h-full w-full items-center justify-between gap-2 px-3">
                  <div className="flex min-w-0 items-center gap-1.5">
                    <TimerIcon
                      className={`h-3.5 w-3.5 shrink-0 ${
                        timer.finished ? "text-emerald-400" : "text-amber-400"
                      }`}
                    />
                    <span className="truncate text-[10.5px] font-medium text-white/75">
                      {timer.finished ? t.timer.doneShort : t.tabs.timer}
                    </span>
                  </div>
                  <span
                    className={`shrink-0 text-[12px] font-semibold tabular-nums ${
                      timer.finished ? "text-emerald-300" : "text-amber-300"
                    }`}
                  >
                    {formatMMSS(timer.remainingSeconds)}
                  </span>
                </div>
              ) : showMetricsCompact ? (
                <div className="flex h-full w-full items-center justify-between gap-2 px-3 text-[10.5px] font-medium tabular-nums">
                  <span className="flex items-center gap-1" style={{ color: accentColor }}>
                    <Cpu className="h-3 w-3" />
                    {Math.round(metrics.cpu_percent)}%
                  </span>
                  <span className="text-white/25">•</span>
                  <span className="text-violet-300">RAM {Math.round(metrics.ram_percent)}%</span>
                  <span className="text-white/25">•</span>
                  {metrics.has_battery && metrics.battery_percent !== null ? (
                    <span className="text-emerald-300">{metrics.battery_percent}%</span>
                  ) : (
                    <span className="flex items-center gap-0.5 text-emerald-300">
                      <PlugZap className="h-3 w-3" />
                      AC
                    </span>
                  )}
                </div>
              ) : (
                <div className="flex h-full w-full items-center justify-between gap-2 px-3.5">
                  <div className="flex min-w-0 items-center gap-2">
                    <span
                      className="h-2 w-2 shrink-0 rounded-full"
                      style={{
                        backgroundColor: accentColor,
                        boxShadow: `0 0 8px ${hexToRgba(accentColor, 0.85)}`,
                      }}
                    />
                    <span className="min-w-0 truncate text-[10.5px] font-medium tracking-wide text-white/65">
                      {t.islandLabel}
                    </span>
                  </div>
                  <span className="shrink-0 text-[11px] font-semibold tabular-nums text-white/90">
                    {clock}
                  </span>
                </div>
              )}
            </motion.div>
          </AnimatePresence>
        </div>
      </motion.div>
    </div>
  );
}
