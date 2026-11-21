import { listen } from "@tauri-apps/api/event";
import { useEffect, useState } from "react";
import { DEFAULT_ACCENT } from "../i18n";

interface AudioVisualizerProps {
  active: boolean;
  compact?: boolean;
  accentColor?: string;
}

const BAR_WEIGHTS = [0.58, 0.96, 0.74, 1.0, 0.64];

export function AudioVisualizer({
  active,
  compact = false,
  accentColor = DEFAULT_ACCENT,
}: AudioVisualizerProps) {
  const [peak, setPeak] = useState<number>(0);
  const [tick, setTick] = useState<number>(0);

  useEffect(() => {
    if (!active) {
      setPeak(0);
      return;
    }

    let unlisten: (() => void) | null = null;
    let cancelled = false;

    void listen<number>("audio-peak", (e) => {
      if (cancelled) return;
      setPeak(e.payload);
      setTick((t) => (t + 1) & 15);
    }).then((fn) => {
      if (cancelled) {
        fn();
      } else {
        unlisten = fn;
      }
    });

    return () => {
      cancelled = true;
      if (unlisten) unlisten();
    };
  }, [active]);

  const barCount = compact ? 4 : 5;
  const maxH = compact ? 15 : 22;
  const effectivePeak = active ? Math.max(0.24, Math.min(1, peak * 1.45)) : 0;

  return (
    <div
      className="flex items-center gap-[2.5px]"
      style={{ height: maxH }}
      aria-label="Audio visualizer"
    >
      {BAR_WEIGHTS.slice(0, barCount).map((weight, idx) => {
        const phaseMod = active ? 0.86 + (((tick + idx * 3) % 5) / 4) * 0.24 : 1;
        const scaleY = active
          ? Math.max(0.22, Math.min(1, effectivePeak * weight * phaseMod))
          : 0.2;

        return (
          <span
            key={idx}
            className="w-[2.5px] rounded-full transition-transform duration-150 ease-out will-change-transform"
            style={{
              height: maxH,
              opacity: active ? 0.95 : 0.35,
              backgroundImage: `linear-gradient(to top, ${accentColor}, #ffffff)`,
              transform: `scaleY(${scaleY.toFixed(3)})`,
              transformOrigin: "center",
            }}
          />
        );
      })}
    </div>
  );
}
