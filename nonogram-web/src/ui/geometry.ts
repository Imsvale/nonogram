import { clueTotal } from "../core/lines";
import { EMPTY, FILLED, type Grid, type Puzzle } from "../core/types";

/** Cell size from which the heavy separators / 5-cell lines are drawn 2px thick; smaller cells use 1px. */
export const HEAVY_LINE_MIN_CELL = 22;
/** Breathing room around the puzzle frame. */
export const MARGIN = 6;
/** Extra room above the frame, so the header show/hide tab never covers the puzzle. */
export const MARGIN_TOP = 20;
/** Vertical space kept under the frame for the footer (buttons + gaps). */
export const FOOTER_RESERVE = 44;
/** Gap between the frame's bottom edge and the footer. */
export const FOOTER_GAP = 8;

export interface Metrics {
  /** Cell size in CSS px. */
  C: number;
  /** Clue / sum font size. */
  fs: number;
  /** Thickness of one clue slot (the short side of a clue cell). */
  N: number;
  sumW: number;
  /** Whether the line-sum strips (right column, bottom row, totals) are shown at all. */
  showSums: boolean;
  /** Space the right-hand sum column takes, separator included; 0 when sums are hidden. */
  rightW: number;
  /** Space the bottom sum row takes, separator included; 0 when sums are hidden. */
  bottomH: number;
  /** Thickness of the heavy separators between clue strips, grid and sums (and 5-cell lines). */
  sep: number;
  /** Width of the row-clue strip including its separator. */
  leftW: number;
  /** Height of the column-clue strip including its separator. */
  topH: number;
  maxRd: number;
  maxCd: number;
}

export function computeMetrics(p: Puzzle, C: number, showSums = true): Metrics {
  // Clue text follows the cell size but stops growing: zooming in must not let
  // the (frozen) clue strips take over the window.
  const fs = Math.min(18, Math.max(9, Math.round(C * 0.5)));
  const N = Math.round(fs * 1.65);
  const maxRd = Math.max(1, ...p.rowClues.map((c) => c.length));
  const maxCd = Math.max(1, ...p.colClues.map((c) => c.length));
  const total = Math.max(clueTotal(p.rowClues), clueTotal(p.colClues));
  const digits = String(total).length;
  const sumW = Math.max(N, Math.round(digits * fs * 0.62 + 10));
  const sep = C >= HEAVY_LINE_MIN_CELL ? 2 : 1;
  return {
    C,
    fs,
    N,
    sumW,
    showSums,
    rightW: showSums ? sep + sumW : 0,
    bottomH: showSums ? sep + N : 0,
    sep,
    leftW: maxRd * N + sep,
    topH: maxCd * N + sep,
    maxRd,
    maxCd,
  };
}

export interface Layout {
  m: Metrics;
  W: number;
  H: number;
  /** Top-left of the framed puzzle (clue strips included). */
  originX: number;
  originY: number;
  /** Size of the whole frame: clue strips + cells + sums. */
  frameW: number;
  frameH: number;
  /** Where the footer's top edge goes: just under the frame. */
  footerTop: number;
  /** How far the frame has been moved from its centred position (clamped to the window). */
  offX: number;
  offY: number;
  /** Top-left / size of the scrolling cell viewport, in canvas px. */
  ox: number;
  oy: number;
  cw: number;
  ch: number;
  fullW: number;
  fullH: number;
  panX: number;
  panY: number;
}

export function clampPan(v: number, full: number, view: number): number {
  return Math.min(Math.max(0, full - view), Math.max(0, v));
}

const clamp = (v: number, lo: number, hi: number) => Math.min(Math.max(lo, hi), Math.max(lo, v));

/**
 * `panX/panY` scroll the cells under the frozen clue strips; `offX/offY` move
 * the whole frame around the window (clamped so it stays fully on screen).
 * The two are independent: the frame only has room to move on an axis where
 * the puzzle isn't already filling the window, i.e. where it isn't scrollable.
 */
export function computeLayout(p: Puzzle, C: number, W: number, H: number, panX = 0, panY = 0, offX = 0, offY = 0, showSums = true): Layout {
  const m = computeMetrics(p, C, showSums);
  const fullW = p.width * C;
  const fullH = p.height * C;
  const availW = Math.max(40, W - 2 * MARGIN - m.leftW - m.rightW);
  const availH = Math.max(40, H - MARGIN_TOP - MARGIN - FOOTER_RESERVE - m.topH - m.bottomH);
  const cw = Math.min(fullW, availW);
  const ch = Math.min(fullH, availH);
  const frameW = m.leftW + cw + m.rightW;
  const frameH = m.topH + ch + m.bottomH;

  const minX = MARGIN;
  const maxX = W - MARGIN - frameW;
  const minY = MARGIN_TOP;
  const maxY = H - MARGIN - FOOTER_RESERVE - frameH;
  const baseX = clamp(Math.floor((W - frameW) / 2), minX, maxX);
  const baseY = clamp(Math.floor((H - FOOTER_RESERVE - frameH) / 2), minY, maxY);
  const originX = clamp(baseX + offX, minX, maxX);
  const originY = clamp(baseY + offY, minY, maxY);

  return {
    m,
    W,
    H,
    originX,
    originY,
    frameW,
    frameH,
    footerTop: originY + frameH + FOOTER_GAP,
    offX: originX - baseX,
    offY: originY - baseY,
    ox: originX + m.leftW,
    oy: originY + m.topH,
    cw,
    ch,
    fullW,
    fullH,
    panX: Math.round(clampPan(panX, fullW, cw)),
    panY: Math.round(clampPan(panY, fullH, ch)),
  };
}

/** Whether cell size `C` still leaves a usable scrolling area next to the frozen clue strips. */
export function viewportOk(p: Puzzle, C: number, W: number, H: number, showSums = true): boolean {
  const L = computeLayout(p, C, W, H, 0, 0, 0, 0, showSums);
  return L.cw >= Math.min(L.fullW, W * 0.35) && L.ch >= Math.min(L.fullH, H * 0.35);
}

/** Largest cell size (within [min, max]) at which the whole puzzle fits in W×H. */
export function fitCellSize(p: Puzzle, W: number, H: number, min = 10, max = 36, showSums = true): number {
  for (let C = max; C > min; C--) {
    const m = computeMetrics(p, C, showSums);
    const tw = m.leftW + p.width * C + m.rightW + 2 * MARGIN;
    const th = m.topH + p.height * C + m.bottomH + MARGIN_TOP + MARGIN + FOOTER_RESERVE;
    if (tw <= W && th <= H) return C;
  }
  return min;
}

export type Hit =
  | { kind: "cell"; r: number; c: number }
  | { kind: "colClue"; c: number; i: number }
  | { kind: "rowClue"; r: number; i: number }
  /** Blank part of a clue strip: still identifies the line for the crosshair. */
  | { kind: "colHead"; c: number }
  | { kind: "rowHead"; r: number }
  | { kind: "minimap"; x: number; y: number }
  | { kind: "sumToggle" }
  | { kind: "none" };

export function hitTest(L: Layout, p: Puzzle, x: number, y: number): Hit {
  const { m, ox, oy, cw, ch, panX, panY, originX, originY } = L;
  const { C, N, sep } = m;
  const inX = x >= ox && x < ox + cw;
  const inY = y >= oy && y < oy + ch;

  if (inX && inY) {
    return { kind: "cell", r: Math.floor((y - oy + panY) / C), c: Math.floor((x - ox + panX) / C) };
  }
  if (inX && y >= originY && y < oy - sep) {
    const c = Math.floor((x - ox + panX) / C);
    const clues = p.colClues[c];
    const i = clues.length - Math.ceil((oy - sep - y) / N);
    return i >= 0 && i < clues.length ? { kind: "colClue", c, i } : { kind: "colHead", c };
  }
  if (inY && x >= originX && x < ox - sep) {
    const r = Math.floor((y - oy + panY) / C);
    const clues = p.rowClues[r];
    const i = clues.length - Math.ceil((ox - sep - x) / N);
    return i >= 0 && i < clues.length ? { kind: "rowClue", r, i } : { kind: "rowHead", r };
  }
  if (x >= originX && x < ox - sep && y >= originY && y < oy - sep) {
    return { kind: "minimap", x: x - originX, y: y - originY };
  }
  const bx = ox + cw + sep;
  const by = oy + ch + sep;
  if (m.showSums && x >= bx && x < bx + m.sumW && y >= by && y < by + N) return { kind: "sumToggle" };
  return { kind: "none" };
}

export interface HoverRun {
  /** Row (horizontal run) or column (vertical run) the run lies in. */
  fixed: number;
  start: number;
  end: number;
  /** Position of the hovered cell along the run's axis. */
  hover: number;
  /** true = a run of Filled cells; false = a run of Unknown (blank) cells. */
  filled: boolean;
}

export interface HoverRuns {
  h: HoverRun | null;
  v: HoverRun | null;
}

/** Maximal runs of the hovered cell's state through it, horizontally and vertically. */
export function computeHoverRuns(grid: Grid, w: number, h: number, hr: number, hc: number): HoverRuns {
  if (hr < 0 || hc < 0 || hr >= h || hc >= w) return { h: null, v: null };
  const st = grid[hr * w + hc];
  if (st === EMPTY) return { h: null, v: null };
  const filled = st === FILLED;

  let hs = hc;
  while (hs > 0 && grid[hr * w + hs - 1] === st) hs--;
  let he = hc;
  while (he + 1 < w && grid[hr * w + he + 1] === st) he++;
  let vs = hr;
  while (vs > 0 && grid[(vs - 1) * w + hc] === st) vs--;
  let ve = hr;
  while (ve + 1 < h && grid[(ve + 1) * w + hc] === st) ve++;

  return {
    h: { fixed: hr, start: hs, end: he, hover: hc, filled },
    v: { fixed: hc, start: vs, end: ve, hover: hr, filled },
  };
}

/**
 * Whether the run length shows under the pointer. It appears when at least one
 * end of the run is `threshold` or more cells away — the point is to give a
 * length when a long run's end(s) are off-screen, so a nearby end must not
 * suppress it while the other end is far.
 */
export function hoverLabelVisible(start: number, end: number, hover: number, threshold: number): boolean {
  return hover - start >= threshold || end - hover >= threshold;
}
