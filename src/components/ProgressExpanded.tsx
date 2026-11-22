import { Activity, FolderArchive, Globe } from "lucide-react";
import { DEFAULT_ACCENT, getTranslations } from "../i18n";
import { LanguageCode, TaskProgress } from "../types";
import { hexToRgba } from "../utils/color";
import { CircularProgress } from "./CircularProgress";

interface ProgressExpandedProps {
  tasks: TaskProgress[];
  lang: LanguageCode;
  accentColor?: string;
}

export function ProgressExpanded({
  tasks,
  lang,
  accentColor = DEFAULT_ACCENT,
}: ProgressExpandedProps) {
  const t = getTranslations(lang);

  if (tasks.length === 0) {
    return (
      <div className="flex h-[146px] flex-col items-center justify-center gap-2 text-center">
        <div className="flex h-10 w-10 items-center justify-center rounded-full bg-white/8 text-white/45">
          <Activity className="h-5 w-5" />
        </div>
        <div>
          <p className="text-xs font-medium text-white/80">{t.progress.emptyTitle}</p>
          <p className="text-[11px] text-white/45">{t.progress.emptyHint}</p>
        </div>
      </div>
    );
  }

  return (
    <div className="flex max-h-[146px] flex-col gap-2 overflow-y-auto pt-1 pr-1">
      {tasks.map((task) => (
        <div
          key={task.id}
          className="flex items-center gap-2.5 rounded-xl border border-white/10 bg-white/[0.04] px-2.5 py-2"
        >
          <div
            className="flex h-7 w-7 shrink-0 items-center justify-center rounded-lg"
            style={{
              backgroundColor: hexToRgba(accentColor, 0.18),
              color: accentColor,
            }}
          >
            {task.source === "Dropzone" ? (
              <FolderArchive className="h-3.5 w-3.5" />
            ) : task.source === "Explorer" ? (
              <Activity className="h-3.5 w-3.5" />
            ) : (
              <Globe className="h-3.5 w-3.5" />
            )}
          </div>
          <div className="min-w-0 flex-1">
            <p className="truncate text-[11.5px] font-medium text-white">{task.title}</p>
            <p className="truncate text-[10px] text-white/50">
              {task.source} • {task.subtitle}
            </p>
          </div>
          <div className="flex items-center gap-1.5">
            <CircularProgress
              percent={task.percent}
              size={26}
              strokeWidth={3}
              color={accentColor}
            />
            <span
              className="w-8 text-right text-[11px] font-semibold tabular-nums"
              style={{ color: accentColor }}
            >
              {task.percent}%
            </span>
          </div>
        </div>
      ))}
    </div>
  );
}
