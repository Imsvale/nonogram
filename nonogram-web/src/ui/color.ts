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

/** Legible ink colour for text/icons drawn on `bg`. */
export function contrastOn(bg: string): string {
  return luminance(bg) > 0.5 ? "#262626" : "#ffffff";
}

/** Linear mix: `t` = 0 → a, 1 → b. */
export function mix(a: string, b: string, t: number): string {
  const [ar, ag, ab] = hexToRgb(a);
  const [br, bg, bb] = hexToRgb(b);
  return rgbToHex([ar + (br - ar) * t, ag + (bg - ag) * t, ab + (bb - ab) * t]);
}

/** Nudge toward black (light colours) or white (dark colours) — used for hover states. */
export function hoverShade(hex: string, amount = 0.07): string {
  return luminance(hex) > 0.5 ? mix(hex, "#000000", amount * 1.6) : mix(hex, "#ffffff", amount * 1.6);
}
