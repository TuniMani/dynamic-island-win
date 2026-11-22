import { invoke } from "@tauri-apps/api/core";
import { Sun, Volume2, VolumeX } from "lucide-react";
import { useEffect, useRef } from "react";
import { DEFAULT_ACCENT, getTranslations } from "../i18n";
import { LanguageCode } from "../types";
import { VolumeSlider } from "./VolumeSlider";

interface HudOverlayProps {
  kind: "volume" | "brightness";
  level: number;
  muted?: boolean;
  lang: LanguageCode;
  accentColor?: string;
  onInteract?: () => void;
  onVolumePreview?: (level: number) => void;
}

export function HudOverlay({
  kind,
  level,
  muted = false,
  lang,
  accentColor = DEFAULT_ACCENT,
  onInteract,
  onVolumePreview,
}: HudOverlayProps) {
  const t = getTranslations(lang);
  const isVolume = kind === "volume";
  const effectiveLevel = muted ? 0 : Math.max(0, Math.min(100, level));
  const rootRef = useRef<HTMLDivElement | null>(null);

  const latestRef = useRef({ isVolume, level, onInteract, onVolumePreview });
  useEffect(() => {
    latestRef.current = { isVolume, level, onInteract, onVolumePreview };
  }, [isVolume, level, onInteract, onVolumePreview]);

  useEffect(() => {
    const el = rootRef.current;
    if (!el || !isVolume) return;
    const handleWheel = (e: WheelEvent) => {
      e.preventDefault();
      e.stopPropagation();
      const delta = e.deltaY < 0 ? 2 : -2;
      const next = Math.max(0, Math.min(100, Math.round(latestRef.current.level + delta)));
      latestRef.current.onInteract?.();
      latestRef.current.onVolumePreview?.(next);
      void invoke("set_system_volume", { level: next });
    };
    el.addEventListener("wheel", handleWheel, { passive: false });
    return () => el.removeEventListener("wheel", handleWheel);
  }, [isVolume]);

  const handleSetVolume = (pct: number) => {
    if (!isVolume) return;
    onInteract?.();
    onVolumePreview?.(pct);
    void invoke("set_system_volume", { level: pct });
  };

  return (
    <div ref={rootRef} className="flex h-full w-full items-center gap-3 px-4">
      <button
        type="button"
        onClick={(e) => {
          e.stopPropagation();
          if (isVolume) {
            onInteract?.();
            void invoke("toggle_system_mute");
          }
        }}
        className="flex h-6 w-6 shrink-0 items-center justify-center rounded-full bg-white/10 text-white transition hover:bg-white/20"
        title={isVolume ? t.hud.toggleMute : t.hud.brightness}
      >
        {isVolume ? (
          muted || level === 0 ? (
            <VolumeX className="h-3.5 w-3.5 text-rose-400" />
          ) : (
            <Volume2 className="h-3.5 w-3.5" style={{ color: accentColor }} />
          )
        ) : (
          <Sun className="h-3.5 w-3.5 text-amber-300" />
        )}
      </button>

      <div className="flex min-w-0 flex-1 flex-col gap-1">
        <div className="flex items-center justify-between gap-2 text-[10px] font-medium text-white/75">
          <span className="truncate">
            {isVolume ? (muted ? t.hud.muted : t.hud.volume) : t.hud.brightness}
          </span>
          <span className="shrink-0 tabular-nums text-white">{effectiveLevel}%</span>
        </div>

        {isVolume ? (
          <VolumeSlider
            value={level}
            muted={muted}
            accentColor={accentColor}
            heightClass="h-2"
            title={t.volume.wheelHint}
            onInteract={onInteract}
            onChange={handleSetVolume}
          />
        ) : (
          <div className="h-1.5 w-full overflow-hidden rounded-full bg-white/14">
            <div
              className="h-full rounded-full transition-all duration-150"
              style={{
                width: `${effectiveLevel}%`,
                backgroundImage: "linear-gradient(to right, #fbbf24, #fef08a)",
              }}
            />
          </div>
        )}
      </div>
    </div>
  );
}
