import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import {
  ArrowDown,
  ArrowUp,
  Globe,
  Layers,
  Monitor,
  Palette,
  Power,
  RotateCcw,
  Sliders,
  SlidersHorizontal,
  Sparkles,
} from "lucide-react";
import { useEffect, useState } from "react";
import {
  ACCENT_PRESETS,
  DEFAULT_ACCENT,
  getTranslations,
  LANGUAGE_OPTIONS,
  normalizeLanguage,
} from "../i18n";
import {
  AnimationSpeed,
  AppSettings,
  ExpandedTab,
  InitialState,
  LanguageCode,
  MonitorInfo,
  PositionPreset,
} from "../types";

type SettingsCategory = "position" | "appearance" | "behavior" | "modules";

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

export function SettingsWindow() {
  const [settings, setSettings] = useState<AppSettings>(() => {
    const cached =
      typeof window !== "undefined" ? window.localStorage.getItem("di_lang") : null;
    return { ...DEFAULT_SETTINGS, language: normalizeLanguage(cached) };
  });
  const [monitors, setMonitors] = useState<MonitorInfo[]>([]);
  const [category, setCategory] = useState<SettingsCategory>("position");
  const [savedHint, setSavedHint] = useState<string>("");

  useEffect(() => {
    void invoke<InitialState>("get_initial_state").then((init) => {
      setSettings(init.settings);
      setMonitors(init.monitors);
      window.localStorage.setItem("di_lang", init.settings.language);
    });

    const unlistenPromise = listen<AppSettings>("settings-changed", (e) => {
      setSettings(e.payload);
      window.localStorage.setItem("di_lang", e.payload.language);
    });

    return () => {
      void unlistenPromise.then((fn) => fn());
    };
  }, []);

  const t = getTranslations(settings.language);

  const persist = async (next: AppSettings, hintText?: string) => {
    setSettings(next);
    window.localStorage.setItem("di_lang", next.language);
    try {
      const saved = await invoke<AppSettings>("save_settings", { settings: next });
      setSettings(saved);
      setSavedHint(hintText ?? getTranslations(saved.language).settings.saved);
      window.setTimeout(() => setSavedHint(""), 1300);
    } catch (err) {
      setSavedHint(String(err));
    }
  };

  const handleResetPosition = async () => {
    try {
      const updated = await invoke<AppSettings>("reset_position");
      setSettings(updated);
      setSavedHint(getTranslations(updated.language).settings.resetSaved);
      window.setTimeout(() => setSavedHint(""), 1400);
    } catch (err) {
      setSavedHint(String(err));
    }
  };

  const moveModule = (index: number, delta: -1 | 1) => {
    const nextOrder = [...settings.module_order];
    const target = index + delta;
    if (target < 0 || target >= nextOrder.length) return;
    [nextOrder[index], nextOrder[target]] = [nextOrder[target], nextOrder[index]];
    void persist({ ...settings, module_order: nextOrder });
  };

  const presetLabels: Array<{ id: PositionPreset; label: string }> = [
    { id: "top_center", label: t.settings.position.topCenter },
    { id: "top_left", label: t.settings.position.topLeft },
    { id: "top_right", label: t.settings.position.topRight },
    { id: "bottom_center", label: t.settings.position.bottomCenter },
  ];

  const moduleMeta: Record<
    ExpandedTab,
    { title: string; desc: string; key: keyof AppSettings }
  > = {
    media: {
      title: t.settings.modules.media,
      desc: t.settings.modules.mediaDesc,
      key: "enable_media_module",
    },
    volume: {
      title: t.settings.modules.volume,
      desc: t.settings.modules.volumeDesc,
      key: "enable_volume_module",
    },
    system: {
      title: t.settings.modules.system,
      desc: t.settings.modules.systemDesc,
      key: "enable_system_module",
    },
    progress: {
      title: t.settings.modules.progress,
      desc: t.settings.modules.progressDesc,
      key: "enable_progress_tracker",
    },
    timer: {
      title: t.settings.modules.timer,
      desc: t.settings.modules.timerDesc,
      key: "enable_timer_module",
    },
  };

  return (
    <div className="flex h-full w-full flex-col bg-[#0b0d13] text-white select-none">
      {/* Header + Language Picker */}
      <div className="flex items-center justify-between gap-3 border-b border-white/10 px-5 py-3.5">
        <div className="flex items-center gap-2.5 min-w-0">
          <div className="flex h-9 w-9 shrink-0 items-center justify-center rounded-xl border border-sky-400/20 bg-sky-500/15 text-sky-300">
            <SlidersHorizontal className="h-4 w-4" />
          </div>
          <div className="min-w-0">
            <h1 className="truncate text-sm font-semibold">{t.settings.title}</h1>
            <p className="truncate text-[11px] text-white/50">{t.settings.subtitle}</p>
          </div>
        </div>

        <div className="flex items-center gap-2 shrink-0">
          {savedHint && (
            <span className="rounded-full bg-emerald-500/15 px-2.5 py-0.5 text-[10.5px] font-medium text-emerald-300">
              {savedHint}
            </span>
          )}
          <div className="flex max-w-[140px] shrink-0 items-center gap-1.5 rounded-xl border border-white/12 bg-[#141824] px-2 py-1">
            <Globe className="h-3.5 w-3.5 shrink-0 text-sky-400" />
            <select
              aria-label={t.settings.languageLabel}
              value={settings.language}
              onChange={(e) => {
                const lang = e.target.value as LanguageCode;
                void persist({ ...settings, language: lang });
              }}
              className="min-w-0 w-full cursor-pointer bg-transparent text-xs font-medium text-white outline-none"
            >
              {LANGUAGE_OPTIONS.map((opt) => (
                <option key={opt.code} value={opt.code} className="bg-[#141824] text-white">
                  {opt.label}
                </option>
              ))}
            </select>
          </div>
        </div>
      </div>

      {/* Category Tabs */}
      <div className="grid grid-cols-4 gap-1.5 border-b border-white/10 bg-white/[0.02] px-5 py-2.5">
        {(
          [
            { id: "position", label: t.settings.categories.position, Icon: Monitor },
            { id: "appearance", label: t.settings.categories.appearance, Icon: Palette },
            { id: "behavior", label: t.settings.categories.behavior, Icon: Sliders },
            { id: "modules", label: t.settings.categories.modules, Icon: Layers },
          ] as const
        ).map(({ id, label, Icon }) => {
          const active = category === id;
          return (
            <button
              key={id}
              type="button"
              title={label}
              onClick={() => setCategory(id)}
              className={`flex min-w-0 items-center justify-center gap-1.5 rounded-xl px-1.5 py-2 text-[11px] font-medium transition ${
                active
                  ? "bg-sky-500 text-white shadow-sm"
                  : "bg-white/[0.04] text-white/65 hover:bg-white/[0.08] hover:text-white"
              }`}
            >
              <Icon className="h-3.5 w-3.5 shrink-0" />
              <span className="truncate">{label}</span>
            </button>
          );
        })}
      </div>

      {/* Scrollable Category Body */}
      <div className="flex-1 overflow-y-auto px-5 py-4 text-xs space-y-3.5">
        {category === "position" && (
          <>
            <section className="space-y-2.5 rounded-2xl border border-white/10 bg-white/[0.03] p-3.5">
              <div className="flex items-center justify-between gap-2">
                <span className="min-w-0 truncate font-semibold text-white/90">
                  {t.settings.position.presetsTitle}
                </span>
                <button
                  type="button"
                  onClick={() => void handleResetPosition()}
                  className="flex shrink-0 items-center gap-1 whitespace-nowrap rounded-lg border border-white/12 bg-white/6 px-2.5 py-1 text-[11px] text-white/80 hover:bg-white/12"
                >
                  <RotateCcw className="h-3 w-3 shrink-0" />
                  <span>{t.settings.position.resetBtn}</span>
                </button>
              </div>

              <div className="grid grid-cols-2 gap-2">
                {presetLabels.map((preset) => {
                  const active = settings.position_preset === preset.id;
                  return (
                    <button
                      key={preset.id}
                      type="button"
                      onClick={() =>
                        void persist({
                          ...settings,
                          position_preset: preset.id,
                          offset_x: 0,
                          offset_y: 8,
                          custom_x: null,
                          custom_y: null,
                        })
                      }
                      className={`rounded-xl border px-3 py-2 text-left font-medium transition ${
                        active
                          ? "border-sky-400 bg-sky-500/20 text-white"
                          : "border-white/10 bg-white/[0.03] text-white/75 hover:bg-white/[0.07]"
                      }`}
                    >
                      {preset.label}
                    </button>
                  );
                })}
              </div>

              {settings.position_preset === "custom" && (
                <p className="text-[11px] text-sky-300">
                  {t.settings.position.custom}: ({settings.custom_x ?? 0}px,{" "}
                  {settings.custom_y ?? 0}px)
                </p>
              )}
            </section>

            <section className="space-y-3 rounded-2xl border border-white/10 bg-white/[0.03] p-3.5">
              <label className="flex cursor-pointer items-center justify-between gap-3">
                <div className="min-w-0">
                  <p className="font-medium text-white/90">
                    {t.settings.position.unlockTitle}
                  </p>
                  <p className="text-[10.5px] text-white/50">
                    {t.settings.position.unlockDesc}
                  </p>
                </div>
                <input
                  type="checkbox"
                  checked={settings.unlock_position}
                  onChange={(e) =>
                    void persist({ ...settings, unlock_position: e.target.checked })
                  }
                  className="h-4 w-4 shrink-0 accent-sky-400"
                />
              </label>

              <label className="block space-y-1">
                <span className="text-[11px] text-white/60">
                  {t.settings.position.monitorLabel}
                </span>
                <select
                  value={settings.monitor_name ?? ""}
                  onChange={(e) =>
                    void persist({
                      ...settings,
                      monitor_name: e.target.value === "" ? null : e.target.value,
                    })
                  }
                  className="w-full rounded-xl border border-white/12 bg-[#141824] px-3 py-2 text-xs text-white outline-none focus:border-sky-400"
                >
                  <option value="">{t.settings.position.monitorAuto}</option>
                  {monitors.map((m) => (
                    <option key={m.name} value={m.name}>
                      {m.name} ({m.width}×{m.height})
                      {m.is_primary ? ` • ${t.settings.position.primaryTag}` : ""}
                    </option>
                  ))}
                </select>
              </label>

              <label className="block space-y-1">
                <div className="flex justify-between text-[11px]">
                  <span className="text-white/60">{t.settings.position.offsetX}</span>
                  <span className="tabular-nums text-sky-300">{settings.offset_x} px</span>
                </div>
                <input
                  type="range"
                  min={-600}
                  max={600}
                  step={2}
                  value={settings.offset_x}
                  onChange={(e) =>
                    void persist({
                      ...settings,
                      position_preset:
                        settings.position_preset === "custom"
                          ? "top_center"
                          : settings.position_preset,
                      offset_x: Number(e.target.value),
                    })
                  }
                  className="w-full accent-sky-400"
                />
              </label>

              <label className="block space-y-1">
                <div className="flex justify-between text-[11px]">
                  <span className="text-white/60">{t.settings.position.offsetY}</span>
                  <span className="tabular-nums text-sky-300">{settings.offset_y} px</span>
                </div>
                <input
                  type="range"
                  min={0}
                  max={250}
                  step={1}
                  value={settings.offset_y}
                  onChange={(e) =>
                    void persist({
                      ...settings,
                      position_preset:
                        settings.position_preset === "custom"
                          ? "top_center"
                          : settings.position_preset,
                      offset_y: Number(e.target.value),
                    })
                  }
                  className="w-full accent-sky-400"
                />
              </label>
            </section>
          </>
        )}

        {category === "appearance" && (
          <>
            <section className="space-y-3 rounded-2xl border border-white/10 bg-white/[0.03] p-3.5">
              <label className="block space-y-1">
                <div className="flex justify-between text-[11px]">
                  <span className="text-white/75 font-medium">
                    {t.settings.appearance.scaleLabel}
                  </span>
                  <span className="tabular-nums text-sky-300">
                    {Math.round(settings.scale * 100)}%
                  </span>
                </div>
                <input
                  type="range"
                  min={75}
                  max={140}
                  step={5}
                  value={Math.round(settings.scale * 100)}
                  onChange={(e) =>
                    void persist({ ...settings, scale: Number(e.target.value) / 100 })
                  }
                  className="w-full accent-sky-400"
                />
              </label>

              <label className="block space-y-1">
                <div className="flex justify-between text-[11px]">
                  <span className="text-white/75 font-medium">
                    {t.settings.appearance.radiusLabel}
                  </span>
                  <span className="tabular-nums text-sky-300">
                    {settings.border_radius_mode >= 100
                      ? t.settings.appearance.radiusPill
                      : `${settings.border_radius_mode} px`}
                  </span>
                </div>
                <input
                  type="range"
                  min={16}
                  max={100}
                  step={2}
                  value={settings.border_radius_mode >= 100 ? 100 : settings.border_radius_mode}
                  onChange={(e) => {
                    const val = Number(e.target.value);
                    void persist({
                      ...settings,
                      border_radius_mode: val >= 100 ? 999 : val,
                    });
                  }}
                  className="w-full accent-sky-400"
                />
              </label>
            </section>

            <section className="space-y-2.5 rounded-2xl border border-white/10 bg-white/[0.03] p-3.5">
              <span className="font-semibold text-white/90">
                {t.settings.appearance.bgStyleLabel}
              </span>
              <div className="grid grid-cols-3 gap-2">
                {(
                  [
                    {
                      id: "pure_black",
                      title: t.settings.appearance.pureBlack,
                      desc: t.settings.appearance.pureBlackDesc,
                    },
                    {
                      id: "glass",
                      title: t.settings.appearance.glass,
                      desc: t.settings.appearance.glassDesc,
                    },
                    {
                      id: "custom",
                      title: t.settings.appearance.custom,
                      desc: t.settings.appearance.customDesc,
                    },
                  ] as const
                ).map((opt) => {
                  const selected = settings.appearance === opt.id;
                  return (
                    <button
                      key={opt.id}
                      type="button"
                      onClick={() => void persist({ ...settings, appearance: opt.id })}
                      className={`min-w-0 rounded-xl border p-2 text-left transition ${
                        selected
                          ? "border-sky-400 bg-sky-500/15"
                          : "border-white/10 bg-white/[0.03] hover:bg-white/[0.07]"
                      }`}
                    >
                      <p className="break-words text-[11px] font-medium text-white">{opt.title}</p>
                      <p className="break-words text-[10px] text-white/50">{opt.desc}</p>
                    </button>
                  );
                })}
              </div>

              {settings.appearance !== "pure_black" && (
                <label className="block space-y-1 pt-1">
                  <div className="flex justify-between text-[11px]">
                    <span className="text-white/60">
                      {t.settings.appearance.opacityLabel}
                    </span>
                    <span className="tabular-nums text-sky-300">
                      {settings.background_opacity}%
                    </span>
                  </div>
                  <input
                    type="range"
                    min={20}
                    max={100}
                    step={5}
                    value={settings.background_opacity}
                    onChange={(e) =>
                      void persist({
                        ...settings,
                        background_opacity: Number(e.target.value),
                      })
                    }
                    className="w-full accent-sky-400"
                  />
                </label>
              )}
            </section>

            <section className="space-y-2.5 rounded-2xl border border-white/10 bg-white/[0.03] p-3.5">
              <span className="flex items-center gap-1.5 font-semibold text-white/90">
                <Sparkles className="h-3.5 w-3.5 text-sky-400" />
                {t.settings.appearance.accentLabel}
              </span>

              <div className="flex flex-wrap items-center gap-2">
                <button
                  type="button"
                  onClick={() =>
                    void persist({ ...settings, accent_mode: "dynamic_album" })
                  }
                  className={`rounded-xl border px-2.5 py-1.5 text-[11px] font-medium transition ${
                    settings.accent_mode === "dynamic_album"
                      ? "border-sky-400 bg-sky-500/20 text-white"
                      : "border-white/10 bg-white/[0.04] text-white/75 hover:bg-white/10"
                  }`}
                >
                  {t.settings.appearance.dynamicAlbum}
                </button>

                {ACCENT_PRESETS.map((preset) => {
                  const selected =
                    settings.accent_mode !== "dynamic_album" &&
                    settings.accent_color.toUpperCase() === preset.hex.toUpperCase();
                  return (
                    <button
                      key={preset.id}
                      type="button"
                      onClick={() =>
                        void persist({
                          ...settings,
                          accent_mode: "preset",
                          accent_color: preset.hex,
                        })
                      }
                      title={preset.labels[settings.language] ?? preset.labels.en}
                      className={`flex h-7 w-7 items-center justify-center rounded-full border-2 transition ${
                        selected ? "border-white scale-110" : "border-white/20 hover:scale-105"
                      }`}
                      style={{ backgroundColor: preset.hex }}
                    />
                  );
                })}

                <label
                  className="flex cursor-pointer items-center gap-1.5 rounded-xl border border-white/12 bg-white/[0.04] px-2 py-1 text-[11px] text-white/75 hover:bg-white/10"
                  title={t.settings.appearance.customPicker}
                >
                  <input
                    type="color"
                    value={settings.accent_color}
                    onChange={(e) =>
                      void persist({
                        ...settings,
                        accent_mode: "custom",
                        accent_color: e.target.value,
                      })
                    }
                    className="h-5 w-5 cursor-pointer border-0 bg-transparent p-0"
                  />
                  <span>{t.settings.appearance.customPicker}</span>
                </label>
              </div>

              <div className="pt-1 space-y-1.5">
                <span className="text-[11px] text-white/60">
                  {t.settings.appearance.animSpeedLabel}
                </span>
                <div className="grid grid-cols-3 gap-2">
                  {(
                    [
                      { id: "smooth", label: t.settings.appearance.speedSmooth },
                      { id: "balanced", label: t.settings.appearance.speedBalanced },
                      { id: "snappy", label: t.settings.appearance.speedSnappy },
                    ] as Array<{ id: AnimationSpeed; label: string }>
                  ).map((sp) => (
                    <button
                      key={sp.id}
                      type="button"
                      title={sp.label}
                      onClick={() =>
                        void persist({ ...settings, animation_speed: sp.id })
                      }
                      className={`min-w-0 truncate rounded-xl border px-1.5 py-1.5 text-[11px] font-medium transition ${
                        settings.animation_speed === sp.id
                          ? "border-sky-400 bg-sky-500/20 text-white"
                          : "border-white/10 bg-white/[0.03] text-white/70 hover:bg-white/10"
                      }`}
                    >
                      {sp.label}
                    </button>
                  ))}
                </div>
              </div>
            </section>
          </>
        )}

        {category === "behavior" && (
          <>
            <section className="space-y-3 rounded-2xl border border-white/10 bg-white/[0.03] p-3.5">
              <label className="flex cursor-pointer items-center justify-between gap-3">
                <div className="min-w-0">
                  <p className="font-medium text-white/90">
                    {t.settings.behavior.autoCollapseFocus}
                  </p>
                  <p className="text-[10.5px] text-white/50">
                    {t.settings.behavior.autoCollapseFocusDesc}
                  </p>
                </div>
                <input
                  type="checkbox"
                  checked={settings.auto_collapse_on_focus_loss}
                  onChange={(e) =>
                    void persist({
                      ...settings,
                      auto_collapse_on_focus_loss: e.target.checked,
                    })
                  }
                  className="h-4 w-4 shrink-0 accent-sky-400"
                />
              </label>

              <div className="space-y-1.5">
                <span className="text-[11px] text-white/65">
                  {t.settings.behavior.mouseLeaveLabel}
                </span>
                <div className="grid grid-cols-4 gap-1.5">
                  {[
                    { val: 0, label: t.settings.behavior.delayInstant },
                    { val: 1000, label: t.settings.behavior.delay1s },
                    { val: 3000, label: t.settings.behavior.delay3s },
                    { val: -1, label: t.settings.behavior.delayOff },
                  ].map((opt) => (
                    <button
                      key={opt.val}
                      type="button"
                      title={opt.label}
                      onClick={() =>
                        void persist({ ...settings, mouse_leave_delay_ms: opt.val })
                      }
                      className={`min-w-0 truncate rounded-xl border px-1 py-1.5 text-[10.5px] font-medium transition ${
                        settings.mouse_leave_delay_ms === opt.val
                          ? "border-sky-400 bg-sky-500/20 text-white"
                          : "border-white/10 bg-white/[0.03] text-white/70 hover:bg-white/10"
                      }`}
                    >
                      {opt.label}
                    </button>
                  ))}
                </div>
              </div>

              <div className="space-y-1.5">
                <span className="text-[11px] text-white/65">
                  {t.settings.behavior.interactionLabel}
                </span>
                <div className="grid grid-cols-2 gap-2">
                  {(
                    [
                      { id: "hover", label: t.settings.behavior.onHover },
                      { id: "click", label: t.settings.behavior.onClick },
                    ] as const
                  ).map((opt) => (
                    <button
                      key={opt.id}
                      type="button"
                      title={opt.label}
                      onClick={() =>
                        void persist({ ...settings, interaction_mode: opt.id })
                      }
                      className={`min-w-0 truncate rounded-xl border px-2 py-1.5 text-[11px] font-medium transition ${
                        settings.interaction_mode === opt.id
                          ? "border-sky-400 bg-sky-500/20 text-white"
                          : "border-white/10 bg-white/[0.03] text-white/70 hover:bg-white/10"
                      }`}
                    >
                      {opt.label}
                    </button>
                  ))}
                </div>
              </div>
            </section>

            <section className="space-y-3 rounded-2xl border border-white/10 bg-white/[0.03] p-3.5">
              <label className="flex cursor-pointer items-center justify-between gap-3">
                <div className="min-w-0">
                  <p className="font-medium text-white/90">
                    {t.settings.behavior.startWithWindows}
                  </p>
                  <p className="text-[10.5px] text-white/50">
                    {t.settings.behavior.startWithWindowsDesc}
                  </p>
                </div>
                <input
                  type="checkbox"
                  checked={settings.start_with_windows}
                  onChange={(e) =>
                    void persist({ ...settings, start_with_windows: e.target.checked })
                  }
                  className="h-4 w-4 shrink-0 accent-sky-400"
                />
              </label>

              <label className="flex cursor-pointer items-center justify-between gap-3">
                <div className="min-w-0">
                  <p className="font-medium text-white/90">
                    {t.settings.behavior.autoHideFullscreen}
                  </p>
                  <p className="text-[10.5px] text-white/50">
                    {t.settings.behavior.autoHideFullscreenDesc}
                  </p>
                </div>
                <input
                  type="checkbox"
                  checked={settings.auto_hide_fullscreen}
                  onChange={(e) =>
                    void persist({ ...settings, auto_hide_fullscreen: e.target.checked })
                  }
                  className="h-4 w-4 shrink-0 accent-sky-400"
                />
              </label>

              <div className="space-y-1.5">
                <span className="text-[11px] text-white/65">
                  {t.settings.behavior.autoHideIdleLabel}
                </span>
                <div className="grid grid-cols-4 gap-1.5">
                  {[
                    { val: 0, label: t.settings.behavior.idleOff },
                    { val: 5, label: t.settings.behavior.idle5s },
                    { val: 15, label: t.settings.behavior.idle15s },
                    { val: 30, label: t.settings.behavior.idle30s },
                  ].map((opt) => (
                    <button
                      key={opt.val}
                      type="button"
                      title={opt.label}
                      onClick={() =>
                        void persist({ ...settings, auto_hide_idle_secs: opt.val })
                      }
                      className={`min-w-0 truncate rounded-xl border px-1 py-1.5 text-[10.5px] font-medium transition ${
                        settings.auto_hide_idle_secs === opt.val
                          ? "border-sky-400 bg-sky-500/20 text-white"
                          : "border-white/10 bg-white/[0.03] text-white/70 hover:bg-white/10"
                      }`}
                    >
                      {opt.label}
                    </button>
                  ))}
                </div>
              </div>

              <label className="block space-y-1">
                <div className="flex justify-between text-[11px]">
                  <span className="text-white/60">
                    {t.settings.behavior.hudDurationLabel}
                  </span>
                  <span className="tabular-nums text-sky-300">
                    {(settings.hud_duration_ms / 1000).toFixed(1)}{" "}
                    {t.settings.behavior.secUnit}
                  </span>
                </div>
                <input
                  type="range"
                  min={1000}
                  max={5000}
                  step={200}
                  value={settings.hud_duration_ms}
                  onChange={(e) =>
                    void persist({ ...settings, hud_duration_ms: Number(e.target.value) })
                  }
                  className="w-full accent-sky-400"
                />
              </label>
            </section>
          </>
        )}

        {category === "modules" && (
          <section className="space-y-2.5 rounded-2xl border border-white/10 bg-white/[0.03] p-3.5">
            <span className="font-semibold text-white/90">
              {t.settings.modules.sectionTitle}
            </span>

            <div className="space-y-2">
              {settings.module_order.map((modId, idx) => {
                const info = moduleMeta[modId];
                if (!info) return null;
                const enabled = Boolean(settings[info.key]);
                return (
                  <div
                    key={modId}
                    className="flex items-center justify-between gap-2 rounded-xl border border-white/10 bg-white/[0.02] px-3 py-2"
                  >
                    <div className="min-w-0 flex-1">
                      <p className="font-medium text-white/90">{info.title}</p>
                      <p className="truncate text-[10.5px] text-white/50">{info.desc}</p>
                    </div>
                    <div className="flex items-center gap-1.5">
                      <button
                        type="button"
                        disabled={idx === 0}
                        onClick={() => moveModule(idx, -1)}
                        className="flex h-6 w-6 items-center justify-center rounded-lg bg-white/8 text-white/70 hover:bg-white/16 disabled:opacity-30"
                        title={t.settings.modules.moveUp}
                      >
                        <ArrowUp className="h-3 w-3" />
                      </button>
                      <button
                        type="button"
                        disabled={idx === settings.module_order.length - 1}
                        onClick={() => moveModule(idx, 1)}
                        className="flex h-6 w-6 items-center justify-center rounded-lg bg-white/8 text-white/70 hover:bg-white/16 disabled:opacity-30"
                        title={t.settings.modules.moveDown}
                      >
                        <ArrowDown className="h-3 w-3" />
                      </button>
                      <input
                        type="checkbox"
                        checked={enabled}
                        onChange={(e) =>
                          void persist({ ...settings, [info.key]: e.target.checked })
                        }
                        className="ml-1 h-4 w-4 shrink-0 accent-sky-400"
                      />
                    </div>
                  </div>
                );
              })}

              {/* Additional Sub-features */}
              {[
                {
                  key: "show_media_controls" as const,
                  title: t.settings.modules.mediaControls,
                  desc: t.settings.modules.mediaControlsDesc,
                },
                {
                  key: "enable_visualizer" as const,
                  title: t.settings.modules.visualizer,
                  desc: t.settings.modules.visualizerDesc,
                },
                {
                  key: "show_system_metrics_in_idle" as const,
                  title: t.settings.modules.idleMetrics,
                  desc: t.settings.modules.idleMetricsDesc,
                },
                {
                  key: "enable_dropzone_module" as const,
                  title: t.settings.modules.dropzone,
                  desc: t.settings.modules.dropzoneDesc,
                },
              ].map((sub) => (
                <label
                  key={sub.key}
                  className="flex cursor-pointer items-center justify-between gap-3 rounded-xl border border-white/8 bg-white/[0.015] px-3 py-2 hover:bg-white/[0.04]"
                >
                  <div className="min-w-0">
                    <p className="font-medium text-white/90">{sub.title}</p>
                    <p className="text-[10.5px] text-white/50">{sub.desc}</p>
                  </div>
                  <input
                    type="checkbox"
                    checked={settings[sub.key]}
                    onChange={(e) =>
                      void persist({ ...settings, [sub.key]: e.target.checked })
                    }
                    className="h-4 w-4 shrink-0 accent-sky-400"
                  />
                </label>
              ))}
            </div>
          </section>
        )}
      </div>

      {/* Prominent Red Quit Footer */}
      <div className="border-t border-white/10 bg-[#090b10] px-5 py-3">
        <button
          type="button"
          onClick={() => void invoke("quit_app")}
          className="flex w-full items-center justify-center gap-2 rounded-xl border border-rose-500/40 bg-rose-600/20 py-2.5 text-xs font-semibold text-rose-200 transition hover:bg-rose-600/35 hover:text-white active:scale-[0.99]"
        >
          <Power className="h-4 w-4 text-rose-400" />
          <span>{t.settings.quitBtn}</span>
        </button>
      </div>
    </div>
  );
}
