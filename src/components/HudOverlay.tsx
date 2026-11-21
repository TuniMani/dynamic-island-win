import { invoke } from "@tauri-apps/api/core";
import { Sun, Volume2, VolumeX } from "lucide-react";
import { DEFAULT_ACCENT, getTranslations } from "../i18n";
import { LanguageCode } from "../types";

interface HudOverlayProps {
  kind: "volume" | "brightness";
  level: number;
  muted?: boolean;
  lang: LanguageCode;
  accentColor?: string;
}

export function HudOverlay({
  kind,
  level,
  muted = false,
  lang,
  accentColor = DEFAULT_ACCENT,
}: HudOverlayProps) {
  const t = getTranslations(lang);
  const isVolume = kind === "volume";
  const effectiveLevel = muted ? 0 : Math.max(0, Math.min(100, level));

  const handleBarClick = (e: React.MouseEvent<HTMLDivElement>) => {
    if (!isVolume) return;
    e.stopPropagation();
    const rect = e.currentTarget.getBoundingClientRect();
    const pct = Math.round(Math.max(0, Math.min(1, (e.clientX - rect.left) / rect.width)) * 100);
    void invoke("set_system_volume", { level: pct });
  };

  return (
    <div className="flex h-full w-full items-center gap-3 px-4">
      <button
        type="button"
        onClick={(e) => {
          e.stopPropagation();
          if (isVolume) {
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
        <div
          onClick={handleBarClick}
          className={`h-1.5 w-full overflow-hidden rounded-full bg-white/14 ${
            isVolume ? "cursor-pointer" : ""
          }`}
        >
          <div
            className="h-full rounded-full transition-all duration-150"
            style={{
              width: `${effectiveLevel}%`,
              backgroundImage: isVolume
                ? muted
                  ? "linear-gradient(to right, #fb7185, #fb7185)"
                  : `linear-gradient(to right, ${accentColor}, #ffffff)`
                : "linear-gradient(to right, #fbbf24, #fef08a)",
            }}
          />
        </div>
      </div>
    </div>
  );
}
