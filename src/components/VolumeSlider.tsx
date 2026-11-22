import { useCallback, useEffect, useRef, useState } from "react";

interface VolumeSliderProps {
  value: number;
  muted?: boolean;
  accentColor: string;
  disabled?: boolean;
  heightClass?: string;
  fillGradient?: string;
  title?: string;
  onChange: (nextLevel: number) => void;
  onDragStateChange?: (dragging: boolean) => void;
  onInteract?: () => void;
}

export function VolumeSlider({
  value,
  muted = false,
  accentColor,
  disabled = false,
  heightClass = "h-2",
  fillGradient,
  title,
  onChange,
  onDragStateChange,
  onInteract,
}: VolumeSliderProps) {
  const trackRef = useRef<HTMLDivElement | null>(null);
  const [dragging, setDragging] = useState(false);

  const latestRef = useRef({ value, disabled, onChange, onDragStateChange, onInteract });
  useEffect(() => {
    latestRef.current = { value, disabled, onChange, onDragStateChange, onInteract };
  }, [value, disabled, onChange, onDragStateChange, onInteract]);

  const computePct = useCallback((clientX: number): number => {
    const el = trackRef.current;
    if (!el) return latestRef.current.value;
    const rect = el.getBoundingClientRect();
    if (rect.width <= 0) return latestRef.current.value;
    const ratio = Math.max(0, Math.min(1, (clientX - rect.left) / rect.width));
    return Math.round(ratio * 100);
  }, []);

  useEffect(() => {
    const el = trackRef.current;
    if (!el) return;
    const handleWheel = (e: WheelEvent) => {
      if (latestRef.current.disabled) return;
      e.preventDefault();
      e.stopPropagation();
      const delta = e.deltaY < 0 ? 2 : -2;
      const next = Math.max(0, Math.min(100, Math.round(latestRef.current.value + delta)));
      latestRef.current.onInteract?.();
      latestRef.current.onChange(next);
    };
    el.addEventListener("wheel", handleWheel, { passive: false });
    return () => el.removeEventListener("wheel", handleWheel);
  }, []);

  const effective = muted ? 0 : Math.max(0, Math.min(100, value));
  const backgroundImage =
    fillGradient ??
    (muted
      ? "linear-gradient(to right, #fb7185, #fb7185)"
      : `linear-gradient(to right, ${accentColor}, #ffffff)`);

  return (
    <div
      ref={trackRef}
      role="slider"
      aria-valuemin={0}
      aria-valuemax={100}
      aria-valuenow={effective}
      title={title}
      onClick={(e) => e.stopPropagation()}
      onPointerDown={(e) => {
        if (disabled || e.button !== 0) return;
        e.stopPropagation();
        e.currentTarget.setPointerCapture(e.pointerId);
        setDragging(true);
        onDragStateChange?.(true);
        onInteract?.();
        onChange(computePct(e.clientX));
      }}
      onPointerMove={(e) => {
        if (!dragging || disabled) return;
        e.stopPropagation();
        onInteract?.();
        onChange(computePct(e.clientX));
      }}
      onPointerUp={(e) => {
        if (!dragging) return;
        e.stopPropagation();
        if (e.currentTarget.hasPointerCapture(e.pointerId)) {
          e.currentTarget.releasePointerCapture(e.pointerId);
        }
        setDragging(false);
        onDragStateChange?.(false);
      }}
      onPointerCancel={() => {
        if (!dragging) return;
        setDragging(false);
        onDragStateChange?.(false);
      }}
      className={`group relative w-full touch-none ${heightClass} overflow-hidden rounded-full bg-white/14 ${
        disabled ? "" : "cursor-pointer"
      }`}
    >
      <div
        className={`h-full rounded-full ${dragging ? "" : "transition-[width] duration-75"}`}
        style={{
          width: `${effective}%`,
          backgroundImage,
        }}
      />
    </div>
  );
}
