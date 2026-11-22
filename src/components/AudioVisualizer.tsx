import { listen } from "@tauri-apps/api/event";
import { useEffect, useRef, useState } from "react";
import { DEFAULT_ACCENT } from "../i18n";

interface AudioVisualizerProps {
  active: boolean;
  compact?: boolean;
  accentColor?: string;
  livePeak?: number | null;
}

const RESTING_SCALE = 0.22;
const BAR_MULTIPLIERS = [0.45, 0.9, 0.65, 0.8, 0.52] as const;
const BAR_PHASES = [0.0, 1.45, 2.85, 4.25, 5.55] as const;
const BAR_SPEEDS = [1.0, 1.26, 0.88, 1.14, 0.94] as const;

let cachedPhase = 0;
let cachedEnergy = 0.36;
let cachedBarScales = [
  RESTING_SCALE,
  RESTING_SCALE,
  RESTING_SCALE,
  RESTING_SCALE,
  RESTING_SCALE,
];
let latestRawPeak = 0;
let latestPeakAt = 0;

export function AudioVisualizer({
  active,
  compact = false,
  accentColor = DEFAULT_ACCENT,
  livePeak = null,
}: AudioVisualizerProps) {
  const [barScales, setBarScales] = useState<number[]>(() =>
    active
      ? [...cachedBarScales]
      : [RESTING_SCALE, RESTING_SCALE, RESTING_SCALE, RESTING_SCALE, RESTING_SCALE],
  );
  const livePeakRef = useRef<number | null>(livePeak);
  livePeakRef.current = livePeak;

  useEffect(() => {
    if (!active) {
      latestRawPeak = 0;
      latestPeakAt = 0;
      cachedEnergy = 0.36;
      cachedBarScales = [
        RESTING_SCALE,
        RESTING_SCALE,
        RESTING_SCALE,
        RESTING_SCALE,
        RESTING_SCALE,
      ];
      setBarScales([...cachedBarScales]);
      return;
    }

    let unlisten: (() => void) | null = null;
    let cancelled = false;

    void listen<number>("audio-peak", (e) => {
      if (cancelled) return;
      latestRawPeak = Math.max(0, Math.min(1, e.payload));
      latestPeakAt = Date.now();
    }).then((fn) => {
      if (cancelled) {
        fn();
      } else {
        unlisten = fn;
      }
    });

    const tickWave = () => {
      cachedPhase += 0.34;
      const now = Date.now();
      const hasFreshLivePeak =
        latestPeakAt > 0 && now - latestPeakAt < 450 && latestRawPeak > 0.015;

      const targetEnergy = hasFreshLivePeak
        ? 0.28 + Math.min(0.62, Math.pow(latestRawPeak, 0.78) * 0.66)
        : 0.34 + Math.min(0.22, Math.max(0, livePeakRef.current ?? 0) * 0.35);

      const energyAlpha = targetEnergy > cachedEnergy ? 0.34 : 0.22;
      const energyDelta = Math.max(
        -0.12,
        Math.min(0.12, (targetEnergy - cachedEnergy) * energyAlpha),
      );
      cachedEnergy = Math.max(0.26, Math.min(0.92, cachedEnergy + energyDelta));

      const nextScales = BAR_MULTIPLIERS.map((mult, idx) => {
        const phase = cachedPhase * BAR_SPEEDS[idx] + BAR_PHASES[idx];
        const wave = 0.5 + 0.36 * Math.sin(phase) + 0.14 * Math.cos(phase * 0.67 + idx);
        const minBar = 0.25 + mult * 0.11;
        const maxLift = Math.min(0.92 - minBar, cachedEnergy * (0.55 + mult * 0.45));
        const barTarget = minBar + wave * Math.max(0.06, maxLift);

        const prev = cachedBarScales[idx];
        const barAlpha = barTarget > prev ? 0.36 : 0.24;
        const delta = Math.max(-0.13, Math.min(0.13, (barTarget - prev) * barAlpha));
        return Math.max(RESTING_SCALE, Math.min(0.92, prev + delta));
      });

      cachedBarScales = nextScales;
      setBarScales(nextScales);
    };

    const intervalId = window.setInterval(tickWave, 110);

    return () => {
      cancelled = true;
      window.clearInterval(intervalId);
      if (unlisten) unlisten();
    };
  }, [active]);

  const barCount = compact ? 4 : 5;
  const maxH = compact ? 15 : 22;

  return (
    <div
      className="flex items-center gap-[2.5px]"
      style={{ height: maxH }}
      aria-label="Audio visualizer"
    >
      {BAR_MULTIPLIERS.slice(0, barCount).map((_, idx) => {
        const scaleY = active ? barScales[idx] : RESTING_SCALE;

        return (
          <span
            key={idx}
            className="w-[2.5px] rounded-full will-change-transform"
            style={{
              height: maxH,
              opacity: active ? 0.95 : 0.35,
              backgroundImage: `linear-gradient(to top, ${accentColor}, #ffffff)`,
              transform: `scaleY(${scaleY.toFixed(3)})`,
              transformOrigin: "center",
              transition: "transform 180ms cubic-bezier(0.22, 1, 0.36, 1), opacity 180ms ease",
            }}
          />
        );
      })}
    </div>
  );
}
