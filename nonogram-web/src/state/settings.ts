/** User settings: the desktop GUI's assistance / visual options, persisted in localStorage. */

import { DARK, LIGHT, type Palette } from "./colors";

export type ThemeChoice = "system" | "light" | "dark";
export type ResolvedTheme = "light" | "dark";
export type IconKind = "none" | "circle" | "square" | "diamond" | "check" | "x" | "dash" | "dot";
export type SubcellKind = "e" | "h" | "v";

export const ICON_KINDS: IconKind[] = ["none", "circle", "square", "diamond", "check", "x", "dash", "dot"];

export interface RunLengthSettings {
  showStart: boolean;
  startThreshold: number;
  showEnd: boolean;
  endThreshold: number;
  showHover: boolean;
  hoverThreshold: number;
  adjLabelEnabled: boolean;
  adjLabelHPreferAfter: boolean;
  adjLabelVPreferAfter: boolean;
  fourDirLabels: boolean;
  /** Tint the run of blank cells under the pointer; only used while the crosshair is off. */
  highlightEmptyRuns: boolean;
  /** 3×3 grid; each subcell hosts the horizontal run length, the vertical one, or nothing. */
  subcells: SubcellKind[][];
  numSize: number;
  /**
   * Where the auto ink (and the custom hue/saturation inks below) switch from dark to
   * light, by the background's luminance (0..1).
   */
  labelContrastThreshold: number;
  /** `null` = automatic black/white; otherwise a hue/saturation (lightness is picked for contrast). */
  labelHColor: { hue: number; sat: number } | null;
  labelVColor: { hue: number; sat: number } | null;
}

export interface Settings {
  theme: ThemeChoice;
  /** `null` = theme default. */
  colors: {
    unknown: string | null;
    filled: string | null;
    empty: string | null;
    clueBg: string | null;
    sumBg: string | null;
  };
  icons: { filled: IconKind; empty: IconKind };
  /** Marker colors; `null` = automatic (contrasts with the cell). */
  iconColors: { filled: string | null; empty: string | null };
  assist: {
    autoDim: boolean;
    autoFillEmpty: boolean;
    autoCrossEdges: boolean;
    /** Show the line-sum strips (right column, bottom row and their totals). */
    showSums: boolean;
    clueSumsWithGaps: boolean;
    axisLock: boolean;
  };
  crosshair: {
    /** Highlight the row and column under the pointer. */
    enabled: boolean;
    /** ...and also the clue strips of that row and column. */
    headers: boolean;
    /** Leave the hovered cell itself unhighlighted. */
    skipIntersection: boolean;
    rowColor: string;
    rowAlpha: number;
    colColor: string;
    colAlpha: number;
  };
  runLength: RunLengthSettings;
  /** Key that toggles the browser's fullscreen. */
  fullscreenKey: string;
  /** Show the timer (it runs from the moment a puzzle opens; pausing hides the puzzle). */
  showTimer: boolean;
  /** What the left mouse button (or a tap) does; the right button / Shift does the other. */
  primaryMode: "fill" | "mark";
  /**
   * What the mouse wheel does: `zoom` (default; panning is by dragging) or `scroll` the
   * grid (Shift: sideways). In `scroll` mode Ctrl + wheel zooms.
   */
  wheelMode: "zoom" | "scroll";
  /** Cell size in px that counts as "100%" zoom. */
  zoomReference: number;
  /** Keep the puzzle frame where it is; dragging then only scrolls the grid inside it. */
  lockFrame: boolean;
  headerHidden: boolean;
}

export const FULLSCREEN_KEYS = ["f", "g", "h", "z", "F9", "F10", "F12"] as const;

export function defaultSettings(): Settings {
  return {
    theme: "system",
    colors: { unknown: null, filled: null, empty: null, clueBg: null, sumBg: null },
    icons: { filled: "none", empty: "x" },
    iconColors: { filled: null, empty: null },
    assist: {
      autoDim: false,
      autoFillEmpty: false,
      autoCrossEdges: false,
      showSums: false,
      clueSumsWithGaps: false,
      axisLock: false,
    },
    crosshair: {
      enabled: true,
      headers: true,
      skipIntersection: false,
      rowColor: "#66b8ff",
      rowAlpha: 0.18,
      colColor: "#66b8ff",
      colAlpha: 0.18,
    },
    runLength: {
      showStart: false,
      startThreshold: 3,
      showEnd: false,
      endThreshold: 3,
      showHover: true,
      hoverThreshold: 2,
      adjLabelEnabled: false,
      adjLabelHPreferAfter: true,
      adjLabelVPreferAfter: true,
      fourDirLabels: false,
      highlightEmptyRuns: true,
      subcells: [
        ["e", "e", "v"],
        ["e", "e", "e"],
        ["h", "e", "e"],
      ],
      numSize: 9,
      labelContrastThreshold: 0.5,
      labelHColor: null,
      labelVColor: null,
    },
    fullscreenKey: "f",
    showTimer: true,
    primaryMode: "fill",
    wheelMode: "zoom",
    zoomReference: 26,
    lockFrame: false,
    headerHidden: false,
  };
}

// Palettes (LIGHT / DARK) and every other color the grid uses live in ./colors.ts.

export function resolveTheme(choice: ThemeChoice): ResolvedTheme {
  if (choice !== "system") return choice;
  return typeof matchMedia === "function" && matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light";
}

export function paletteFor(s: Settings, theme: ResolvedTheme): Palette {
  const base = theme === "dark" ? DARK : LIGHT;
  return {
    ...base,
    unknown: s.colors.unknown ?? base.unknown,
    filled: s.colors.filled ?? base.filled,
    empty: s.colors.empty ?? base.empty,
    clueBg: s.colors.clueBg ?? base.clueBg,
    sumBg: s.colors.sumBg ?? base.sumBg,
  };
}

// ── Persistence ─────────────────────────────────────────────────────────────

const KEY = "nonogram-web:settings:v1";

function isObject(v: unknown): v is Record<string, unknown> {
  return typeof v === "object" && v !== null && !Array.isArray(v);
}

/** Copy values from `src` into `dst` only where the key exists in `dst` with the same type. */
function mergeInto(dst: Record<string, unknown>, src: Record<string, unknown>): void {
  for (const k of Object.keys(dst)) {
    if (!(k in src)) continue;
    const d = dst[k];
    const s = src[k];
    if (isObject(d) && isObject(s)) mergeInto(d, s);
    else if (Array.isArray(d) && Array.isArray(s)) {
      if (JSON.stringify(d.map((x) => (Array.isArray(x) ? x.length : 0))) === JSON.stringify(s.map((x) => (Array.isArray(x) ? x.length : 0)))) {
        dst[k] = s;
      }
    } else if (d === null || s === null) {
      // Object here covers labelHColor/labelVColor ({hue,sat} | null): validated below,
      // since a merge this generic can't check the object's own shape.
      if (s === null || typeof s === "string" || isObject(s)) dst[k] = s;
    } else if (typeof d === typeof s) dst[k] = s;
  }
}

function isHueSat(v: unknown): v is { hue: number; sat: number } {
  return isObject(v) && typeof v.hue === "number" && typeof v.sat === "number" && Number.isFinite(v.hue) && Number.isFinite(v.sat);
}

export function loadSettings(): Settings {
  const s = defaultSettings();
  try {
    const raw = localStorage.getItem(KEY);
    if (raw) {
      const parsed: unknown = JSON.parse(raw);
      if (isObject(parsed)) mergeInto(s as unknown as Record<string, unknown>, parsed);
    }
  } catch {
    /* storage unavailable or corrupt — use defaults */
  }
  // These changed shape from a hex string to {hue,sat}; anything that isn't a valid
  // {hue,sat} (an old string, or garbage) resets to automatic rather than breaking.
  if (!isHueSat(s.runLength.labelHColor)) s.runLength.labelHColor = null;
  if (!isHueSat(s.runLength.labelVColor)) s.runLength.labelVColor = null;
  return s;
}

let saveTimer: ReturnType<typeof setTimeout> | undefined;

/** Write immediately (used when the page is going away). */
export function saveSettingsNow(s: Settings): void {
  clearTimeout(saveTimer);
  try {
    localStorage.setItem(KEY, JSON.stringify(s));
  } catch {
    /* ignore quota / private-mode errors */
  }
}

/** Debounced save for rapid changes such as slider drags. */
export function saveSettings(s: Settings): void {
  clearTimeout(saveTimer);
  saveTimer = setTimeout(() => saveSettingsNow(s), 150);
}
