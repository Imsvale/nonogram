/** User settings: the desktop GUI's assistance / visual options, persisted in localStorage. */

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
  /** 3×3 grid; each subcell hosts the horizontal run length, the vertical one, or nothing. */
  subcells: SubcellKind[][];
  numSize: number;
  /** `null` = automatic (contrast against the run colour). */
  labelHColor: string | null;
  labelVColor: string | null;
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
  assist: {
    autoDim: boolean;
    autoFillEmpty: boolean;
    autoCrossEdges: boolean;
    clueSumsWithGaps: boolean;
    axisLock: boolean;
  };
  crosshair: {
    enabled: boolean;
    skipIntersection: boolean;
    rowColor: string;
    rowAlpha: number;
    colColor: string;
    colAlpha: number;
  };
  runLength: RunLengthSettings;
  focusKey: string;
  autoStartTimer: boolean;
}

export const FOCUS_KEYS = ["f", "g", "h", "z", "F9", "F10", "F12"] as const;

export function defaultSettings(): Settings {
  return {
    theme: "system",
    colors: { unknown: null, filled: null, empty: null, clueBg: null, sumBg: null },
    icons: { filled: "none", empty: "none" },
    assist: {
      autoDim: false,
      autoFillEmpty: false,
      autoCrossEdges: false,
      clueSumsWithGaps: false,
      axisLock: false,
    },
    crosshair: {
      enabled: false,
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
      subcells: [
        ["e", "e", "v"],
        ["e", "e", "e"],
        ["h", "e", "e"],
      ],
      numSize: 9,
      labelHColor: null,
      labelVColor: null,
    },
    focusKey: "f",
    autoStartTimer: false,
  };
}

// ── Palettes ────────────────────────────────────────────────────────────────

export interface Palette {
  unknown: string;
  filled: string;
  empty: string;
  clueBg: string;
  sumBg: string;
  clueText: string;
  clueDim: string;
  borderMin: string;
  borderMaj: string;
  canvasBg: string;
}

export const LIGHT: Palette = {
  unknown: "#b8c2d1",
  filled: "#1a1a26",
  empty: "#ffffff",
  clueBg: "#f7f7f7",
  sumBg: "#d1d1d1",
  clueText: "#1a1a1a",
  clueDim: "#b3b3b3",
  borderMin: "#808794",
  borderMaj: "#475270",
  canvasBg: "#ffffff",
};

export const DARK: Palette = {
  unknown: "#5b6577",
  filled: "#e8eaf2",
  empty: "#20242e",
  clueBg: "#2a2f3b",
  sumBg: "#3a4050",
  clueText: "#e6e8ef",
  clueDim: "#666d7d",
  borderMin: "#485064",
  borderMaj: "#9aa4bd",
  canvasBg: "#181b22",
};

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
      if (s === null || typeof s === "string") dst[k] = s;
    } else if (typeof d === typeof s) dst[k] = s;
  }
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
