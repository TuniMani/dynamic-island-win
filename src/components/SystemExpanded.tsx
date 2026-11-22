import { BatteryCharging, Cpu, MemoryStick, PlugZap } from "lucide-react";
import { DEFAULT_ACCENT, getTranslations } from "../i18n";
import { LanguageCode, SystemMetrics } from "../types";
import { CircularProgress } from "./CircularProgress";

interface SystemExpandedProps {
  metrics: SystemMetrics;
  lang: LanguageCode;
  accentColor?: string;
}

export function SystemExpanded({
  metrics,
  lang,
  accentColor = DEFAULT_ACCENT,
}: SystemExpandedProps) {
  const t = getTranslations(lang);
  const cpuVal = Math.round(metrics.cpu_percent);
  const ramVal = Math.round(metrics.ram_percent);
  const batteryVal = metrics.battery_percent ?? 100;

  return (
    <div className="grid h-[146px] grid-cols-3 gap-2.5 pt-1">
      <div className="flex min-w-0 flex-col items-center justify-between rounded-2xl border border-white/10 bg-white/[0.04] p-2.5">
        <div className="flex max-w-full items-center gap-1 text-[10px] font-medium text-white/60">
          <Cpu className="h-3 w-3 shrink-0" style={{ color: accentColor }} />
          <span className="truncate">{t.system.cpu}</span>
        </div>
        <CircularProgress
          percent={cpuVal}
          size={46}
          strokeWidth={4}
          color={cpuVal > 85 ? "#fb7185" : accentColor}
          showText
        />
        <span className="max-w-full truncate text-[10px] tabular-nums text-white/50">
          CPU {cpuVal}%
        </span>
      </div>

      <div className="flex min-w-0 flex-col items-center justify-between rounded-2xl border border-white/10 bg-white/[0.04] p-2.5">
        <div className="flex max-w-full items-center gap-1 text-[10px] font-medium text-white/60">
          <MemoryStick className="h-3 w-3 shrink-0 text-violet-400" />
          <span className="truncate">{t.system.memory}</span>
        </div>
        <CircularProgress
          percent={ramVal}
          size={46}
          strokeWidth={4}
          color={ramVal > 85 ? "#f59e0b" : "#a78bfa"}
          showText
        />
        <span className="max-w-full truncate text-[10px] tabular-nums text-white/50">
          {metrics.ram_used_gb.toFixed(1)}/{metrics.ram_total_gb.toFixed(0)} GB
        </span>
      </div>

      <div className="flex min-w-0 flex-col items-center justify-between rounded-2xl border border-white/10 bg-white/[0.04] p-2.5">
        <div className="flex max-w-full items-center gap-1 text-[10px] font-medium text-white/60">
          {metrics.has_battery ? (
            <BatteryCharging className="h-3 w-3 shrink-0 text-emerald-400" />
          ) : (
            <PlugZap className="h-3 w-3 shrink-0 text-emerald-400" />
          )}
          <span className="truncate">{t.system.power}</span>
        </div>

        {metrics.has_battery && metrics.battery_percent !== null ? (
          <>
            <CircularProgress
              percent={batteryVal}
              size={46}
              strokeWidth={4}
              color={batteryVal < 20 ? "#fb7185" : "#34d399"}
              showText
            />
            <span className="max-w-full truncate text-[10px] text-emerald-300/90">
              {metrics.is_charging ? t.system.charging : t.system.battery}
            </span>
          </>
        ) : (
          <>
            <div className="flex h-[46px] w-[46px] items-center justify-center rounded-full border border-emerald-400/30 bg-emerald-500/12 text-emerald-300">
              <PlugZap className="h-5 w-5" />
            </div>
            <span className="max-w-full truncate text-[10px] text-emerald-300/90">
              {t.system.acPower}
            </span>
          </>
        )}
      </div>
    </div>
  );
}
