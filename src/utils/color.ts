export function hexToRgba(hex: string, alpha: number): string {
  const clean = hex.replace("#", "").trim();
  if (clean.length === 3) {
    const r = parseInt(clean[0] + clean[0], 16);
    const g = parseInt(clean[1] + clean[1], 16);
    const b = parseInt(clean[2] + clean[2], 16);
    if (!Number.isNaN(r) && !Number.isNaN(g) && !Number.isNaN(b)) {
      return `rgba(${r}, ${g}, ${b}, ${alpha})`;
    }
  }
  if (clean.length === 6) {
    const r = parseInt(clean.slice(0, 2), 16);
    const g = parseInt(clean.slice(2, 4), 16);
    const b = parseInt(clean.slice(4, 6), 16);
    if (!Number.isNaN(r) && !Number.isNaN(g) && !Number.isNaN(b)) {
      return `rgba(${r}, ${g}, ${b}, ${alpha})`;
    }
  }
  return `rgba(10, 132, 255, ${alpha})`;
}

export function extractAlbumAccent(dataUrl: string): Promise<string | null> {
  return new Promise((resolve) => {
    const img = new Image();
    img.onload = () => {
      try {
        const canvas = document.createElement("canvas");
        canvas.width = 16;
        canvas.height = 16;
        const ctx = canvas.getContext("2d");
        if (!ctx) {
          resolve(null);
          return;
        }
        ctx.drawImage(img, 0, 0, 16, 16);
        const { data } = ctx.getImageData(0, 0, 16, 16);

        let bestScore = -1;
        let bestRgb: [number, number, number] | null = null;
        let sumR = 0;
        let sumG = 0;
        let sumB = 0;
        let count = 0;

        for (let i = 0; i < data.length; i += 4) {
          const r = data[i];
          const g = data[i + 1];
          const b = data[i + 2];
          const a = data[i + 3];
          if (a < 128) continue;

          sumR += r;
          sumG += g;
          sumB += b;
          count++;

          const max = Math.max(r, g, b);
          const min = Math.min(r, g, b);
          const lum = (max + min) / 510;
          const sat = max === 0 ? 0 : (max - min) / max;

          // Prefer vibrant, readable colors over pure dark/white pixels
          if (lum < 0.14 || lum > 0.92) continue;
          const score = sat * 2.0 + (1 - Math.abs(lum - 0.55));
          if (score > bestScore) {
            bestScore = score;
            bestRgb = [r, g, b];
          }
        }

        if (bestRgb) {
          const toHex = (n: number) => Math.max(0, Math.min(255, n)).toString(16).padStart(2, "0");
          resolve(`#${toHex(bestRgb[0])}${toHex(bestRgb[1])}${toHex(bestRgb[2])}`);
          return;
        }

        if (count > 0) {
          const r = Math.min(255, Math.round(sumR / count) + 35);
          const g = Math.min(255, Math.round(sumG / count) + 35);
          const b = Math.min(255, Math.round(sumB / count) + 35);
          const toHex = (n: number) => n.toString(16).padStart(2, "0");
          resolve(`#${toHex(r)}${toHex(g)}${toHex(b)}`);
          return;
        }
        resolve(null);
      } catch {
        resolve(null);
      }
    };
    img.onerror = () => resolve(null);
    img.src = dataUrl;
  });
}
