import { useLayoutEffect, useRef, useState } from "react";

interface MarqueeTextProps {
  text: string;
  className?: string;
  gapPx?: number;
  speedPxPerSec?: number;
}

const MASK_STYLE = {
  maskImage:
    "linear-gradient(to right, transparent 0%, black 7%, black 93%, transparent 100%)",
  WebkitMaskImage:
    "linear-gradient(to right, transparent 0%, black 7%, black 93%, transparent 100%)",
} as const;

export function MarqueeText({
  text,
  className = "",
  gapPx = 32,
  speedPxPerSec = 26,
}: MarqueeTextProps) {
  const containerRef = useRef<HTMLDivElement | null>(null);
  const measureRef = useRef<HTMLSpanElement | null>(null);
  const [layout, setLayout] = useState<{ overflowing: boolean; contentWidth: number }>({
    overflowing: false,
    contentWidth: 0,
  });

  useLayoutEffect(() => {
    const container = containerRef.current;
    const measure = measureRef.current;
    if (!container || !measure) return;

    const update = () => {
      const cw = container.clientWidth;
      const sw = measure.scrollWidth;
      const overflowing = cw > 0 && sw > cw + 1;
      setLayout((prev) =>
        prev.overflowing === overflowing && prev.contentWidth === sw
          ? prev
          : { overflowing, contentWidth: sw },
      );
    };

    update();
    const ro = new ResizeObserver(update);
    ro.observe(container);
    ro.observe(measure);
    return () => ro.disconnect();
  }, [text]);

  const scrollDuration = Math.max(4.5, (layout.contentWidth + gapPx) / speedPxPerSec);
  const totalDuration = scrollDuration / 0.84;

  return (
    <div
      ref={containerRef}
      className="relative min-w-0 flex-1 overflow-hidden whitespace-nowrap"
      style={layout.overflowing ? MASK_STYLE : undefined}
    >
      {/* Hidden measurement copy uses CSS layout pixels so parent scale() doesn't skew check */}
      <span
        ref={measureRef}
        aria-hidden="true"
        className={`pointer-events-none invisible absolute left-0 top-0 inline-block whitespace-nowrap ${className}`}
      >
        {text}
      </span>

      {layout.overflowing ? (
        <div
          key={text}
          className="di-marquee-track inline-flex w-max items-center"
          style={{ animationDuration: `${totalDuration.toFixed(2)}s` }}
        >
          <span
            className={`inline-block whitespace-nowrap ${className}`}
            style={{ paddingRight: `${gapPx}px` }}
          >
            {text}
          </span>
          <span
            aria-hidden="true"
            className={`inline-block whitespace-nowrap ${className}`}
            style={{ paddingRight: `${gapPx}px` }}
          >
            {text}
          </span>
        </div>
      ) : (
        <span className={`block truncate ${className}`}>{text}</span>
      )}
    </div>
  );
}
