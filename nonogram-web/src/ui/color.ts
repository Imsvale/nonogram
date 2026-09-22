export type RGB = [number, number, number];

export function hexToRgb(hex: string): RGB {
  let h = hex.replace("#", "");
  if (h.length === 3) h = [...h].map((c) => c + c).join("");
  const n = parseInt(h.slice(0, 6), 16);
  if (Number.isNaN(n)) return [0, 0, 0];
  return [(n >> 16) & 255, (n >> 8) & 255, n & 255];
}

export function rgbToHex([r, g, b]: RGB): string {
  const x = (v: number) => Math.max(0, Math.min(255, Math.round(v))).toString(16).padStart(2, "0");
  return `#${x(r)}${x(g)}${x(b)}`;
}

export function rgba(hex: string, a: number): string {
  const [r, g, b] = hexToRgb(hex);
  return `rgba(${r},${g},${b},${a})`;
}

/** Perceived luminance in 0..1 (same weights as the desktop GUI). */
export function luminance(hex: string): number {
  const [r, g, b] = hexToRgb(hex);
  return (0.299 * r + 0.587 * g + 0.114 * b) / 255;
}

/** Legible ink color for text/icons drawn on `bg`; `threshold` is where it flips (0..1 luminance). */
export function contrastOn(bg: string, threshold = 0.5): string {
  return luminance(bg) > threshold ? "#262626" : "#ffffff";
}

/** HSL → hex. `h` in degrees [0, 360); `s` and `l` are fractions [0, 1]. */
export function hslToHex(h: number, s: number, l: number): string {
  h = ((h % 360) + 360) % 360;
  s = Math.min(1, Math.max(0, s));
  l = Math.min(1, Math.max(0, l));
  const c = (1 - Math.abs(2 * l - 1)) * s;
  const x = c * (1 - Math.abs(((h / 60) % 2) - 1));
  const m = l - c / 2;
  const [r, g, b] =
    h < 60 ? [c, x, 0] : h < 120 ? [x, c, 0] : h < 180 ? [0, c, x] : h < 240 ? [0, x, c] : h < 300 ? [x, 0, c] : [c, 0, x];
  return rgbToHex([(r + m) * 255, (g + m) * 255, (b + m) * 255]);
}

/**
 * Ink in the given hue/saturation that stays readable on `bg`: a deep shade on light
 * backgrounds, a light tint on dark ones — the same switch as `contrastOn`, but colored.
 * Lightness is fixed by this function, not user-chosen: that's what guarantees the
 * contrast, which a plain custom RGB pick couldn't.
 */
export function huedInk(hue: number, satPct: number, bg: string, threshold = 0.5): string {
  const sat = Math.min(100, Math.max(0, satPct)) / 100;
  return luminance(bg) > threshold ? hslToHex(hue, sat, 0.28) : hslToHex(hue, sat, 0.8);
}

/** RGB → HSV. Returns `[h in 0..360, s in 0..100, v in 0..100]`. */
export function rgbToHsv([r8, g8, b8]: RGB): [number, number, number] {
  const r = r8 / 255;
  const g = g8 / 255;
  const b = b8 / 255;
  const max = Math.max(r, g, b);
  const min = Math.min(r, g, b);
  const d = max - min;
  let h = 0;
  if (d !== 0) {
    if (max === r) h = ((g - b) / d) % 6;
    else if (max === g) h = (b - r) / d + 2;
    else h = (r - g) / d + 4;
    h *= 60;
    if (h < 0) h += 360;
  }
  const s = max === 0 ? 0 : d / max;
  return [h, s * 100, max * 100];
}

/** HSV → RGB. `h` in degrees [0, 360); `s` and `v` are percent [0, 100]. */
export function hsvToRgb(h: number, s: number, v: number): RGB {
  h = ((h % 360) + 360) % 360;
  const sf = Math.min(100, Math.max(0, s)) / 100;
  const vf = Math.min(100, Math.max(0, v)) / 100;
  const c = vf * sf;
  const x = c * (1 - Math.abs(((h / 60) % 2) - 1));
  const m = vf - c;
  const [r, g, b] =
    h < 60 ? [c, x, 0] : h < 120 ? [x, c, 0] : h < 180 ? [0, c, x] : h < 240 ? [0, x, c] : h < 300 ? [x, 0, c] : [c, 0, x];
  return [(r + m) * 255, (g + m) * 255, (b + m) * 255];
}

/** Linear mix: `t` = 0 → a, 1 → b. */
export function mix(a: string, b: string, t: number): string {
  const [ar, ag, ab] = hexToRgb(a);
  const [br, bg, bb] = hexToRgb(b);
  return rgbToHex([ar + (br - ar) * t, ag + (bg - ag) * t, ab + (bb - ab) * t]);
}

/** Nudge toward black (light colors) or white (dark colors) — used for hover states. */
export function hoverShade(hex: string, amount = 0.07): string {
  return luminance(hex) > 0.5 ? mix(hex, "#000000", amount * 1.6) : mix(hex, "#ffffff", amount * 1.6);
}
