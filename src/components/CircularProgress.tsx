import { motion } from "framer-motion";

interface CircularProgressProps {
  percent: number;
  size?: number;
  strokeWidth?: number;
  color?: string;
  trackColor?: string;
  showText?: boolean;
}

export function CircularProgress({
  percent,
  size = 24,
  strokeWidth = 2.6,
  color = "#38bdf8",
  trackColor = "rgba(255,255,255,0.14)",
  showText = false,
}: CircularProgressProps) {
  const clamped = Math.max(0, Math.min(100, percent));
  const radius = (size - strokeWidth) / 2;
  const circumference = 2 * Math.PI * radius;
  const offset = circumference - (clamped / 100) * circumference;

  return (
    <div
      className="relative inline-flex items-center justify-center shrink-0"
      style={{ width: size, height: size }}
    >
      <svg width={size} height={size} className="-rotate-90">
        <circle
          cx={size / 2}
          cy={size / 2}
          r={radius}
          fill="none"
          stroke={trackColor}
          strokeWidth={strokeWidth}
        />
        <motion.circle
          cx={size / 2}
          cy={size / 2}
          r={radius}
          fill="none"
          stroke={color}
          strokeWidth={strokeWidth}
          strokeLinecap="round"
          strokeDasharray={circumference}
          animate={{ strokeDashoffset: offset }}
          transition={{ type: "spring", stiffness: 320, damping: 32 }}
        />
      </svg>
      {showText && (
        <span className="absolute text-[9px] font-semibold tabular-nums text-white/90">
          {Math.round(clamped)}
        </span>
      )}
    </div>
  );
}
