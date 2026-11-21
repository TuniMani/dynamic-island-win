import { Minus, Pause, Play, Plus, RotateCcw } from "lucide-react";
import { DEFAULT_ACCENT, getTranslations } from "../i18n";
import { LanguageCode, TimerState } from "../types";
import { CircularProgress } from "./CircularProgress";

interface TimerExpandedProps {
  timer: TimerState;
  lang: LanguageCode;
  accentColor?: string;
  onSetPreset: (minutes: number) => void;
  onAdjustSeconds: (delta: number) => void;
  onToggleRun: () => void;
  onReset: () => void;
}

function formatMMSS(sec: number): string {
  const s = Math.max(0, Math.floor(sec));
  const mins = Math.floor(s / 60);
  const rem = s % 60;
  return `${mins.toString().padStart(2, "0")}:${rem.toString().padStart(2, "0")}`;
}

export function TimerExpanded({
  timer,
  lang,
  accentColor = DEFAULT_ACCENT,
  onSetPreset,
  onAdjustSeconds,
  onToggleRun,
  onReset,
}: TimerExpandedProps) {
  const t = getTranslations(lang);
  const pct =
    timer.totalSeconds > 0
      ? ((timer.totalSeconds - timer.remainingSeconds) / timer.totalSeconds) * 100
      : 0;

  return (
    <div className="flex h-[126px] flex-col justify-between pt-1">
      <div className="flex items-center justify-between gap-3">
        <div className="flex items-center gap-3">
          <CircularProgress
            percent={pct}
            size={48}
            strokeWidth={4}
            color={timer.finished ? "#34d399" : accentColor}
          />
          <div>
            <p className="text-[10px] font-medium uppercase tracking-wider text-white/45">
              {timer.finished
                ? t.timer.finished
                : timer.running
                  ? t.timer.running
                  : t.timer.idle}
            </p>
            <p className="text-2xl font-bold tracking-tight tabular-nums text-white">
              {formatMMSS(timer.remainingSeconds)}
            </p>
          </div>
        </div>

        <div className="flex items-center gap-1.5">
          <button
            type="button"
            onClick={() => onAdjustSeconds(-60)}
            className="flex h-7 w-7 items-center justify-center rounded-full border border-white/12 bg-white/6 text-white/75 transition hover:bg-white/14"
            title={t.timer.minusMin}
          >
            <Minus className="h-3.5 w-3.5" />
          </button>
          <button
            type="button"
            onClick={() => onAdjustSeconds(60)}
            className="flex h-7 w-7 items-center justify-center rounded-full border border-white/12 bg-white/6 text-white/75 transition hover:bg-white/14"
            title={t.timer.plusMin}
          >
            <Plus className="h-3.5 w-3.5" />
          </button>
          <button
            type="button"
            onClick={onToggleRun}
            className="flex h-8 w-8 items-center justify-center rounded-full bg-amber-400 text-black shadow transition hover:bg-amber-300 active:scale-95"
            title={timer.running ? t.timer.pause : t.timer.start}
          >
            {timer.running ? (
              <Pause className="h-3.5 w-3.5 fill-current" />
            ) : (
              <Play className="h-3.5 w-3.5 translate-x-[1px] fill-current" />
            )}
          </button>
          <button
            type="button"
            onClick={onReset}
            className="flex h-8 w-8 items-center justify-center rounded-full border border-white/12 bg-white/8 text-white/75 transition hover:bg-white/16"
            title={t.timer.reset}
          >
            <RotateCcw className="h-3.5 w-3.5" />
          </button>
        </div>
      </div>

      <div className="grid grid-cols-4 gap-1.5">
        {[1, 5, 10, 25].map((min) => (
          <button
            key={min}
            type="button"
            onClick={() => onSetPreset(min)}
            className="rounded-xl border border-white/10 bg-white/[0.06] py-1.5 text-[11px] font-medium text-white/80 transition hover:bg-white/14 hover:text-white"
          >
            {min} {t.timer.minSuffix}
          </button>
        ))}
      </div>
    </div>
  );
}
