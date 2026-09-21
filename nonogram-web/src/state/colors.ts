/**
 * Every colour the puzzle grid uses, in one place.
 *
 * - `LIGHT` / `DARK`: the default grid palette per theme. The user can override
 *   the cell, clue and sum colours in Settings (those overrides are stored as
 *   `settings.colors` and win over these defaults).
 * - Trial-tier colours, hover tints, label and marker inks: fixed here.
 * - Not here: page chrome (header, buttons, drawer) is in `src/style.css`
 *   (CSS variables at the top). The footer's "Tier N" label colours are the
 *   `--tier-1…5` variables there; keep them in step with `TRIAL_FILLED` below.
 */

import { mix } from "../ui/color";

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
  unknown: "#ffffff",
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

// ── Trial tiers ─────────────────────────────────────────────────────────────

/** Cells painted in trial tier 1, 2, 3, … (cycles after 5): saturated for filled… */
export const TRIAL_FILLED = ["#405294", "#73388c", "#2e7a61", "#8c612e", "#803347"];
/** …and pale for crossed-out cells (light theme; dark theme mixes from TRIAL_FILLED). */
export const TRIAL_EMPTY_LIGHT = ["#dbe8fc", "#f0e0fc", "#e0faed", "#fcf2d6", "#fce0e8"];

export function trialFilled(tier: number): string {
  return TRIAL_FILLED[(tier - 1) % TRIAL_FILLED.length];
}

export function trialEmpty(tier: number, theme: "light" | "dark", emptyBase: string): string {
  if (theme === "light") return TRIAL_EMPTY_LIGHT[(tier - 1) % TRIAL_EMPTY_LIGHT.length];
  return mix(emptyBase, trialFilled(tier), 0.3);
}

// ── Overlays and small inks ─────────────────────────────────────────────────

/** Amber used for "this line can't fit its clues" and mismatched totals. */
export const WARN = { light: "#bf6100", dark: "#ffb347" };

/** Tint over the hovered run: dark tint on light cells, light tint on dark cells. */
export const HOVER_RUN_TINT = { onLight: "rgba(0,0,60,0.12)", onDark: "rgba(255,255,255,0.10)" };

/** The number marking where a trial tier started. */
export const TRIAL_ORIGIN_INK = { onEmptyLight: "#4d4d73", onEmptyDark: "#b9bfe0", onFilled: "#ffffff" };

/** Neighbour-cell run length and four-direction counts, by cell brightness. */
export const RUN_LABEL_INK = {
  adjOnLight: "#0033b3",
  adjOnDark: "#8dccff",
  fourOnLight: "#994d00",
  fourOnDark: "#ffd966",
};

/** Outline of the visible region on the minimap when zoomed in. */
export const MINIMAP_VIEWPORT = "#ff8c1a";
