import { invoke } from "@tauri-apps/api/core";
import { ArrowLeftRight, Disc3, Pause, Play, SkipBack, SkipForward } from "lucide-react";
import { useEffect, useState } from "react";
import { DEFAULT_ACCENT, getTranslations } from "../i18n";
import { LanguageCode, MediaState } from "../types";
import { hexToRgba } from "../utils/color";
import { AudioVisualizer } from "./AudioVisualizer";

interface MediaExpandedProps {
  media: MediaState;
  enableVisualizer: boolean;
  lang: LanguageCode;
  accentColor?: string;
}

function formatSecs(total: number): string {
  if (!Number.isFinite(total) || total <= 0) return "0:00";
  const rounded = Math.floor(total);
  const h = Math.floor(rounded / 3600);
  const m = Math.floor((rounded % 3600) / 60);
  const s = rounded % 60;
  if (h > 0) {
    return `${h}:${m.toString().padStart(2, "0")}:${s.toString().padStart(2, "0")}`;
  }
  return `${m}:${s.toString().padStart(2, "0")}`;
}

function friendlySourceName(appId: string, fallback: string): string {
  const lower = appId.toLowerCase();
  if (!lower) return fallback;
  if (lower.includes("spotify")) return "Spotify";
  if (lower.includes("brave")) return "Brave";
  if (lower.includes("chrome")) return "Chrome";
  if (lower.includes("msedge") || lower.includes("edge")) return "Edge";
  if (lower.includes("opera")) return "Opera";
  if (lower.includes("vivaldi")) return "Vivaldi";
  if (lower.includes("apple") || lower.includes("itunes")) return "Apple Music";
  if (lower.includes("tidal")) return "TIDAL";
  if (lower.includes("musicbee")) return "MusicBee";
  if (lower.includes("foobar")) return "foobar2000";
  if (lower.includes("potplayer")) return "PotPlayer";
  if (lower.includes("firefox")) return "Firefox";
  if (lower.includes("vlc")) return "VLC";
  const clean = appId.split("!").pop()?.replace(/\.exe$/i, "") ?? appId;
  return clean.slice(0, 18);
}

export function MediaExpanded({
  media,
  enableVisualizer,
  lang,
  accentColor = DEFAULT_ACCENT,
}: MediaExpandedProps) {
  const t = getTranslations(lang);
  const [nowMs, setNowMs] = useState<number>(() => Date.now());
  const [imgError, setImgError] = useState<boolean>(false);

  useEffect(() => {
    setImgError(false);
  }, [media.thumbnail]);

  useEffect(() => {
    setNowMs(Date.now());
    if (!media.has_media || !media.is_playing) return;
    const id = window.setInterval(() => setNowMs(Date.now()), 250);
    return () => window.clearInterval(id);
  }, [
    media.has_media,
    media.is_playing,
    media.updated_at_ms,
    media.position_secs,
    media.app_id,
    media.title,
  ]);

  if (!media.has_media) {
    return (
      <div className="flex h-[126px] flex-col items-center justify-center gap-2 text-center">
        <div className="flex h-10 w-10 items-center justify-center rounded-full bg-white/8 text-white/45">
          <Disc3 className="h-5 w-5" />
        </div>
        <div>
          <p className="text-xs font-medium text-white/80">{t.media.noMedia}</p>
          <p className="text-[11px] text-white/45">{t.media.noMediaHint}</p>
        </div>
      </div>
    );
  }

  const rate = media.playback_rate > 0 ? media.playback_rate : 1;
  const elapsedOffset = media.is_playing
    ? Math.max(0, (nowMs - media.updated_at_ms) / 1000) * rate
    : 0;
  const currentPos =
    media.duration_secs > 0
      ? Math.min(media.duration_secs, media.position_secs + elapsedOffset)
      : media.position_secs + elapsedOffset;
  const progressPct =
    media.duration_secs > 0
      ? Math.max(0, Math.min(100, (currentPos / media.duration_secs) * 100))
      : 0;

  const handleSeekClick = (e: React.MouseEvent<HTMLDivElement>) => {
    if (media.duration_secs <= 0) return;
    const rect = e.currentTarget.getBoundingClientRect();
    const ratio = Math.max(0, Math.min(1, (e.clientX - rect.left) / rect.width));
    void invoke("media_seek", { positionSecs: ratio * media.duration_secs });
  };

  const hasMultipleSessions = media.sessions.length > 1;
  const rawActiveIdx = media.sessions.findIndex((s) => s.is_active);
  const activeIdx = rawActiveIdx >= 0 ? rawActiveIdx : 0;
  const nextSession = hasMultipleSessions
    ? media.sessions[(activeIdx + 1) % media.sessions.length]
    : null;

  return (
    <div className="flex flex-col gap-2.5 pt-0.5">
      <div className="flex items-center gap-3">
        <div className="relative h-14 w-14 shrink-0 overflow-hidden rounded-2xl border border-white/12 bg-white/6 shadow-lg">
          {media.thumbnail && !imgError ? (
            <img
              src={media.thumbnail}
              alt={media.title}
              onError={() => setImgError(true)}
              className="h-full w-full object-cover"
              draggable={false}
            />
          ) : (
            <div
              className="flex h-full w-full items-center justify-center text-white/80"
              style={{
                background: `linear-gradient(135deg, ${hexToRgba(accentColor, 0.35)}, rgba(255,255,255,0.08))`,
              }}
            >
              <Disc3 className="h-7 w-7" />
            </div>
          )}
        </div>

        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-1.5">
            <span
              className="shrink-0 rounded-full px-1.5 py-0.5 text-[9px] font-medium"
              style={{
                backgroundColor: hexToRgba(accentColor, 0.2),
                color: accentColor,
              }}
            >
              {friendlySourceName(media.app_id, t.media.sourceDefault)}
            </span>
            <span className="truncate text-[10px] text-white/45">
              {media.is_playing ? t.media.playing : t.media.paused}
            </span>

            {hasMultipleSessions && nextSession && (
              <button
                type="button"
                onClick={() => void invoke("media_cycle_session")}
                className="ml-auto flex shrink-0 items-center gap-1 rounded-full border border-white/12 bg-white/[0.08] px-1.5 py-0.5 text-[9px] font-medium text-white/80 transition hover:bg-white/16 hover:text-white active:scale-95"
                title={t.media.switchSourceHint(
                  friendlySourceName(nextSession.app_id, t.media.sourceDefault),
                  activeIdx + 1,
                  media.sessions.length,
                )}
              >
                <ArrowLeftRight className="h-2.5 w-2.5" style={{ color: accentColor }} />
                <span className="tabular-nums">
                  {activeIdx + 1}/{media.sessions.length}
                </span>
              </button>
            )}
          </div>
          <p className="mt-0.5 truncate text-[13px] font-semibold text-white">
            {media.title || t.media.unknownTrack}
          </p>
          <p className="truncate text-[11px] text-white/60">
            {media.artist || media.album || t.media.unknownArtist}
          </p>
        </div>

        {enableVisualizer && (
          <div className="shrink-0 pr-1">
            <AudioVisualizer active={media.is_playing} accentColor={accentColor} />
          </div>
        )}
      </div>

      <div className="space-y-1">
        <div
          onClick={handleSeekClick}
          className="group relative h-1.5 w-full cursor-pointer overflow-hidden rounded-full bg-white/14"
          title={t.media.seekTitle}
        >
          <div
            className="h-full rounded-full transition-[width] duration-200 ease-linear"
            style={{
              width: `${progressPct}%`,
              backgroundImage: `linear-gradient(to right, ${accentColor}, #ffffff)`,
            }}
          />
        </div>
        <div className="flex justify-between text-[10px] tabular-nums text-white/45">
          <span>{formatSecs(currentPos)}</span>
          <span>{media.duration_secs > 0 ? formatSecs(media.duration_secs) : "--:--"}</span>
        </div>
      </div>

      <div className="flex items-center justify-center gap-5">
        <button
          type="button"
          onClick={() => void invoke("media_previous")}
          className="flex h-8 w-8 items-center justify-center rounded-full text-white/75 transition hover:bg-white/12 hover:text-white active:scale-95"
          title={t.media.prevTitle}
        >
          <SkipBack className="h-4 w-4 fill-current" />
        </button>

        <button
          type="button"
          onClick={() => void invoke("media_play_pause")}
          className="flex h-9 w-9 items-center justify-center rounded-full bg-white text-black shadow-md transition hover:scale-105 active:scale-95"
          title={media.is_playing ? t.media.pauseTitle : t.media.playTitle}
        >
          {media.is_playing ? (
            <Pause className="h-4 w-4 fill-current" />
          ) : (
            <Play className="h-4 w-4 translate-x-[1px] fill-current" />
          )}
        </button>

        <button
          type="button"
          onClick={() => void invoke("media_next")}
          className="flex h-8 w-8 items-center justify-center rounded-full text-white/75 transition hover:bg-white/12 hover:text-white active:scale-95"
          title={t.media.nextTitle}
        >
          <SkipForward className="h-4 w-4 fill-current" />
        </button>
      </div>
    </div>
  );
}
