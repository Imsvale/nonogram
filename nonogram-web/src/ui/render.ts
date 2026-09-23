import { clueTotal, minSpan } from "../core/lines";
import { EMPTY, FILLED, type Grid, type Puzzle } from "../core/types";
import type { Derived, Pos, TrialTier } from "../state/game";
import {
  HOVER_RUN_TINT,
  MINIMAP_VIEWPORT,
  RUN_LABEL_INK,
  TRIAL_ORIGIN_INK,
  trialEmpty,
  trialFilled,
  WARN,
  type Palette,
} from "../state/colors";
import type { ResolvedTheme, Settings, SubcellKind } from "../state/settings";
import { contrastOn, hoverShade, huedInk, luminance, mix, rgba } from "./color";
import { computeHoverRuns, hoverLabelVisible, type HoverRun, type Layout } from "./geometry";
import { drawIcon } from "./icons";

export interface HoverState {
  cell: Pos | null;
  /** Row/column the pointer is over in a clue strip (drives the crosshair). */
  rowHead: number | null;
  colHead: number | null;
  clue: { isCol: boolean; line: number; idx: number } | null;
  sumToggle: boolean;
}

export const NO_HOVER: HoverState = { cell: null, rowHead: null, colHead: null, clue: null, sumToggle: false };

export interface RenderInput {
  ctx: CanvasRenderingContext2D;
  dpr: number;
  layout: Layout;
  puzzle: Puzzle;
  grid: Grid;
  tierMap: Uint8Array;
  trial: readonly TrialTier[];
  /** Only supplied when auto-dim is on. */
  derived: Derived | null;
  dimRows: ReadonlySet<string>;
  dimCols: ReadonlySet<string>;
  settings: Settings;
  theme: ResolvedTheme;
  palette: Palette;
  hover: HoverState;
}

const FONT = 'system-ui, -apple-system, "Segoe UI", Roboto, sans-serif';

export function render(inp: RenderInput): void {
  const { ctx, dpr, layout: L, palette: pal } = inp;
  ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  ctx.fillStyle = pal.canvasBg;
  ctx.fillRect(0, 0, L.W, L.H);
  ctx.textBaseline = "middle";

  const x = new Draw(inp);
  x.cells();
  x.colClues();
  x.rowClues();
  x.sums();
  x.minimap();
  x.frame(); // last: the borders must never be painted over
}

class Draw {
  private ctx: CanvasRenderingContext2D;
  private L: Layout;
  private p: Puzzle;
  private pal: Palette;
  private s: Settings;
  private w: number;
  private h: number;
  private C: number;
  private c0: number;
  private c1: number;
  private r0: number;
  private r1: number;
  private xr: number | null = null;
  private xc: number | null = null;

  constructor(private inp: RenderInput) {
    this.ctx = inp.ctx;
    this.L = inp.layout;
    this.p = inp.puzzle;
    this.pal = inp.palette;
    this.s = inp.settings;
    this.w = this.p.width;
    this.h = this.p.height;
    this.C = this.L.m.C;
    const { panX, panY, cw, ch } = this.L;
    this.c0 = Math.max(0, Math.floor(panX / this.C));
    this.c1 = Math.min(this.w - 1, Math.floor((panX + cw - 1) / this.C));
    this.r0 = Math.max(0, Math.floor(panY / this.C));
    this.r1 = Math.min(this.h - 1, Math.floor((panY + ch - 1) / this.C));

    if (this.s.crosshair.enabled) {
      const hv = inp.hover;
      if (hv.cell) {
        this.xr = hv.cell[0];
        this.xc = hv.cell[1];
      } else {
        this.xr = hv.rowHead;
        this.xc = hv.colHead;
      }
    }
  }

  private cx(c: number): number {
    return this.L.ox + c * this.C - this.L.panX;
  }
  private cy(r: number): number {
    return this.L.oy + r * this.C - this.L.panY;
  }

  private stateColor(state: number, tier: number): string {
    if (tier > 0) {
      if (state === FILLED) return trialFilled(tier);
      if (state === EMPTY) return trialEmpty(tier, this.inp.theme, this.pal.empty);
    }
    return state === FILLED ? this.pal.filled : state === EMPTY ? this.pal.empty : this.pal.unknown;
  }

  /** Thickness in CSS px, rounded to whole device pixels (never below one). */
  private thick(w: number): number {
    const d = this.inp.dpr;
    return Math.max(1, Math.round(w * d)) / d;
  }
  private snap(v: number): number {
    const d = this.inp.dpr;
    return Math.round(v * d) / d;
  }

  // Lines are snapped to device pixels: a 1px line centered on a pixel boundary
  // smears over two pixels and reads as thicker and grayer than it should.
  private vline(x: number, y0: number, y1: number, major: boolean): void {
    const t = this.thick(major ? this.L.m.sep : 1);
    this.ctx.fillStyle = major ? this.pal.borderMaj : this.pal.borderMin;
    this.ctx.fillRect(this.snap(x - t / 2), y0, t, y1 - y0);
  }
  private hline(y: number, x0: number, x1: number, major: boolean): void {
    const t = this.thick(major ? this.L.m.sep : 1);
    this.ctx.fillStyle = major ? this.pal.borderMaj : this.pal.borderMin;
    this.ctx.fillRect(x0, this.snap(y - t / 2), x1 - x0, t);
  }

  // ── Cells ─────────────────────────────────────────────────────────────────

  cells(): void {
    const { ctx, L, C, w, h, inp } = this;
    const { grid, tierMap } = inp;
    ctx.save();
    ctx.beginPath();
    ctx.rect(L.ox, L.oy, L.cw, L.ch);
    ctx.clip();

    // 1. Backgrounds
    for (let r = this.r0; r <= this.r1; r++) {
      const y = this.cy(r);
      for (let c = this.c0; c <= this.c1; c++) {
        const i = r * w + c;
        ctx.fillStyle = this.stateColor(grid[i], tierMap[i]);
        ctx.fillRect(this.cx(c), y, C, C);
      }
    }

    // 2. Crosshair overlays. The hovered cell sits on both the row and the column,
    // so it gets ONE layer (not row + column stacked, which would read twice as strong).
    if (this.xr !== null || this.xc !== null) {
      const ch = this.s.crosshair;
      const both = this.xr !== null && this.xc !== null;
      if (this.xr !== null && this.xr >= this.r0 && this.xr <= this.r1) {
        ctx.fillStyle = rgba(ch.rowColor, ch.rowAlpha);
        for (let c = this.c0; c <= this.c1; c++) {
          if (both && c === this.xc) continue;
          ctx.fillRect(this.cx(c), this.cy(this.xr), C, C);
        }
      }
      if (this.xc !== null && this.xc >= this.c0 && this.xc <= this.c1) {
        ctx.fillStyle = rgba(ch.colColor, ch.colAlpha);
        for (let r = this.r0; r <= this.r1; r++) {
          if (both && r === this.xr) continue;
          ctx.fillRect(this.cx(this.xc), this.cy(r), C, C);
        }
      }
      if (both && !ch.skipIntersection && this.xr! >= this.r0 && this.xr! <= this.r1 && this.xc! >= this.c0 && this.xc! <= this.c1) {
        ctx.fillStyle = rgba(mix(ch.rowColor, ch.colColor, 0.5), Math.max(ch.rowAlpha, ch.colAlpha));
        ctx.fillRect(this.cx(this.xc!), this.cy(this.xr!), C, C);
      }
    }

    // 3. Hover-run tint
    const hc = inp.hover.cell;
    const runs = hc ? computeHoverRuns(grid, w, h, hc[0], hc[1]) : { h: null, v: null };
    const tint = (r: number, c: number) => {
      const bg = this.stateColor(grid[r * w + c], tierMap[r * w + c]);
      ctx.fillStyle = luminance(bg) > 0.5 ? HOVER_RUN_TINT.onLight : HOVER_RUN_TINT.onDark;
      ctx.fillRect(this.cx(c), this.cy(r), C, C);
    };
    // The blank-run tint would compete with the crosshair (same row and column), so the
    // crosshair wins; filled runs keep their tint regardless.
    const blankTint = !this.s.crosshair.enabled && this.s.runLength.highlightEmptyRuns;
    if (runs.h && (runs.h.filled || blankTint) && runs.h.fixed >= this.r0 && runs.h.fixed <= this.r1) {
      for (let c = Math.max(runs.h.start, this.c0); c <= Math.min(runs.h.end, this.c1); c++) tint(runs.h.fixed, c);
    }
    if (runs.v && (runs.v.filled || blankTint) && runs.v.fixed >= this.c0 && runs.v.fixed <= this.c1) {
      for (let r = Math.max(runs.v.start, this.r0); r <= Math.min(runs.v.end, this.r1); r++) tint(r, runs.v.fixed);
    }

    // 4. Grid lines
    // Outer edges (index 0 and w / h) belong to the frame, which isn't clipped, so its
    // borders keep their exact thickness. Only inner lines are drawn here.
    for (let c = Math.max(1, this.c0); c <= Math.min(w - 1, this.c1 + 1); c++) {
      this.vline(this.cx(c), L.oy, L.oy + L.ch, c % 5 === 0);
    }
    for (let r = Math.max(1, this.r0); r <= Math.min(h - 1, this.r1 + 1); r++) {
      this.hline(this.cy(r), L.ox, L.ox + L.cw, r % 5 === 0);
    }

    // 5. Icons and trial-origin markers
    this.cellContent();

    // 6. Run-length labels
    if (runs.h || runs.v) this.runLabels(runs.h, runs.v, hc!);

    ctx.restore();
  }

  private cellContent(): void {
    const { ctx, C, w, inp } = this;
    const { grid, tierMap, trial, settings } = inp;
    const origin = new Map<number, number>();
    trial.forEach((t, i) => {
      if (t.origin) {
        const k = t.origin[0] * w + t.origin[1];
        if (!origin.has(k)) origin.set(k, i + 1);
      }
    });
    const iconSize = C * 0.55;

    for (let r = this.r0; r <= this.r1; r++) {
      for (let c = this.c0; c <= this.c1; c++) {
        const i = r * w + c;
        const st = grid[i];
        const tier = tierMap[i];
        const bg = this.stateColor(st, tier);
        const x = this.cx(c);
        const y = this.cy(r);
        const ot = origin.get(i);

        if (ot !== undefined) {
          if (st === EMPTY) {
            if (settings.icons.empty !== "none") {
              drawIcon(ctx, settings.icons.empty, x + C / 2, y + C / 2, iconSize, trialFilled(tier || ot));
            }
            ctx.fillStyle = this.inp.theme === "dark" ? TRIAL_ORIGIN_INK.onEmptyDark : TRIAL_ORIGIN_INK.onEmptyLight;
            ctx.font = `${Math.max(8, C * 0.34)}px ${FONT}`;
            ctx.textAlign = "right";
            ctx.fillText(String(ot), x + C - 2, y + C - C * 0.2);
          } else {
            ctx.fillStyle = TRIAL_ORIGIN_INK.onFilled;
            ctx.font = `${Math.max(9, C * 0.5)}px ${FONT}`;
            ctx.textAlign = "center";
            ctx.fillText(String(ot), x + C / 2, y + C / 2 + 0.5);
          }
        } else if (tier === 0) {
          const kind = st === FILLED ? settings.icons.filled : st === EMPTY ? settings.icons.empty : "none";
          if (kind !== "none") {
            const custom = st === FILLED ? settings.iconColors.filled : settings.iconColors.empty;
            drawIcon(ctx, kind, x + C / 2, y + C / 2, iconSize, custom ?? contrastOn(bg));
          }
        } else if (st === EMPTY && settings.icons.empty !== "none") {
          drawIcon(ctx, settings.icons.empty, x + C / 2, y + C / 2, iconSize, trialFilled(tier));
        }
      }
    }
  }

  private runLabels(hr: HoverRun | null, vr: HoverRun | null, hover: Pos): void {
    const { ctx, C, w, h, s, inp } = this;
    const rl = s.runLength;
    const { grid, tierMap } = inp;
    const size = Math.max(7, rl.numSize * (C / 26));
    ctx.font = `${size}px ${FONT}`;

    const cellsOf = (kind: SubcellKind) => {
      const out: [number, number][] = [];
      for (let sr = 0; sr < 3; sr++) for (let sc = 0; sc < 3; sc++) if (rl.subcells[sr][sc] === kind) out.push([sr, sc]);
      return out;
    };
    const hSub = cellsOf("h");
    const vSub = cellsOf("v");
    const [hAdjRow] = hSub[0] ?? [2, 0];
    const [, vAdjCol] = vSub[0] ?? [0, 2];

    const pad = 2;
    const put = (text: string, x: number, y: number, ax: 0 | 1 | 2, ay: 0 | 1 | 2, color: string) => {
      ctx.fillStyle = color;
      ctx.textAlign = ax === 0 ? "left" : ax === 1 ? "center" : "right";
      ctx.textBaseline = ay === 0 ? "top" : ay === 1 ? "middle" : "bottom";
      const px = ax === 0 ? x + pad : ax === 1 ? x + C / 2 : x + C - pad;
      const py = ay === 0 ? y + pad : ay === 1 ? y + C / 2 : y + C - pad;
      ctx.fillText(text, px, py);
    };

    const runFilled = (hr ?? vr)!.filled;
    const runBg = runFilled ? this.pal.filled : this.pal.unknown;
    const th = rl.labelContrastThreshold;
    const auto = contrastOn(runBg, th);
    const hColor = rl.labelHColor ? huedInk(rl.labelHColor.hue, rl.labelHColor.sat, runBg, th) : auto;
    const vColor = rl.labelVColor ? huedInk(rl.labelVColor.hue, rl.labelVColor.sat, runBg, th) : auto;

    const hLen = hr ? hr.end - hr.start + 1 : 0;
    const vLen = vr ? vr.end - vr.start + 1 : 0;
    const ht = rl.hoverThreshold;

    // A "1×1 island": a single cell that is, on its own, a run of 1 in both directions. With the
    // start/end thresholds both at 1, the ordinary per-edge logic below would stack up to four
    // "1"s on that one cell (h-start, h-end, v-start, v-end, all hugging different edges of the
    // same tiny square). Special-case it: one bigger, centered "1" instead.
    const isIsland = !!(hr && vr && hLen === 1 && vLen === 1);
    const islandR = isIsland ? vr!.start : -1;
    const islandC = isIsland ? hr!.start : -1;

    const labelCell = (r: number, c: number) => {
      const x = this.cx(c);
      const y = this.cy(r);
      const bg = this.stateColor(grid[r * w + c], tierMap[r * w + c]);
      const light = luminance(bg) > 0.5;
      const adjColor = light ? RUN_LABEL_INK.adjOnLight : RUN_LABEL_INK.adjOnDark;
      const fourColor = light ? RUN_LABEL_INK.fourOnLight : RUN_LABEL_INK.fourOnDark;
      const isIslandCell = isIsland && r === islandR && c === islandC;

      // Labels at the run's ends always hug the outer edge of the run (start: middle-left for a
      // horizontal run / middle-top for a vertical one; end: the opposite edge) — the configurable
      // "label placement" position only applies to the hover label below.
      if (hr && r === hr.fixed && !isIslandCell) {
        if (c === hr.start && rl.showStart && hLen >= rl.startThreshold) put(String(hLen), x, y, 0, 1, hColor);
        if (c === hr.end && rl.showEnd && hLen >= rl.endThreshold) put(String(hLen), x, y, 2, 1, hColor);
        // The neighboring-cell label (below) replaces this one when enabled, rather than joining it.
        if (c === hr.hover && rl.showHover && !rl.adjLabelEnabled && hoverLabelVisible(hr.start, hr.end, hr.hover, ht))
          for (const [sr, sc] of hSub) put(String(hLen), x, y, sc as 0 | 1 | 2, sr as 0 | 1 | 2, hColor);
      }
      if (vr && c === vr.fixed && !isIslandCell) {
        if (r === vr.start && rl.showStart && vLen >= rl.startThreshold) put(String(vLen), x, y, 1, 0, vColor);
        if (r === vr.end && rl.showEnd && vLen >= rl.endThreshold) put(String(vLen), x, y, 1, 2, vColor);
        if (r === vr.hover && rl.showHover && !rl.adjLabelEnabled && hoverLabelVisible(vr.start, vr.end, vr.hover, ht))
          for (const [sr, sc] of vSub) put(String(vLen), x, y, sc as 0 | 1 | 2, sr as 0 | 1 | 2, vColor);
      }
      if (isIslandCell && ((rl.showStart && rl.startThreshold <= 1) || (rl.showEnd && rl.endThreshold <= 1))) {
        const bigSize = size * 1.6;
        ctx.font = `${bigSize}px ${FONT}`;
        put("1", x, y, 1, 1, hColor);
        ctx.font = `${size}px ${FONT}`;
      }

      // Adjacent-cell label: the hover label itself, shifted to a neighbor of the hovered cell
      // instead of drawn on it (so as not to crowd it) — same visibility rule as the hover label.
      if (rl.adjLabelEnabled && rl.showHover) {
        if (hr && r === hr.fixed && hoverLabelVisible(hr.start, hr.end, hr.hover, ht)) {
          const target =
            hr.hover === hr.start ? hr.hover + 1 : hr.hover === hr.end ? hr.hover - 1 : rl.adjLabelHPreferAfter ? hr.hover + 1 : hr.hover - 1;
          if (target === c && target >= 0 && target < w) put(String(hLen), x, y, c > hr.hover ? 0 : 2, hAdjRow as 0 | 1 | 2, adjColor);
        }
        if (vr && c === vr.fixed && hoverLabelVisible(vr.start, vr.end, vr.hover, ht)) {
          const target =
            vr.hover === vr.start ? vr.hover + 1 : vr.hover === vr.end ? vr.hover - 1 : rl.adjLabelVPreferAfter ? vr.hover + 1 : vr.hover - 1;
          if (target === r && target >= 0 && target < h) put(String(vLen), x, y, vAdjCol as 0 | 1 | 2, r > vr.hover ? 0 : 2, adjColor);
        }
      }

      // Four-direction counts: cells on each side of the hovered cell, each subject to the same
      // "far enough from the end" rule as the hover label — but per direction, against only its
      // own end (each of these speaks for one side, not both).
      if (rl.fourDirLabels) {
        if (hr && r === hr.fixed) {
          const left = hr.hover - hr.start;
          const right = hr.end - hr.hover;
          if (c + 1 === hr.hover && left > 0 && left >= ht) put(String(left), x, y, 2, hAdjRow as 0 | 1 | 2, fourColor);
          if (c === hr.hover + 1 && c < w && right > 0 && right >= ht) put(String(right), x, y, 0, hAdjRow as 0 | 1 | 2, fourColor);
        }
        if (vr && c === vr.fixed) {
          const up = vr.hover - vr.start;
          const down = vr.end - vr.hover;
          if (r + 1 === vr.hover && up > 0 && up >= ht) put(String(up), x, y, vAdjCol as 0 | 1 | 2, 2, fourColor);
          if (r === vr.hover + 1 && r < h && down > 0 && down >= ht) put(String(down), x, y, vAdjCol as 0 | 1 | 2, 0, fourColor);
        }
      }
    };

    const [hovR, hovC] = hover;
    for (let c = this.c0; c <= this.c1; c++) if (hovR >= this.r0 && hovR <= this.r1) labelCell(hovR, c);
    for (let r = this.r0; r <= this.r1; r++) if (r !== hovR && hovC >= this.c0 && hovC <= this.c1) labelCell(r, hovC);
    ctx.textBaseline = "middle";
  }

  // ── Clue strips ───────────────────────────────────────────────────────────

  private clueInk(dim: boolean): string {
    return dim ? this.pal.clueDim : this.pal.clueText;
  }

  colClues(): void {
    const { ctx, L, p, pal, inp } = this;
    const { m } = L;
    const yTop = L.originY;
    const yBottom = L.oy - L.m.sep;
    ctx.save();
    ctx.beginPath();
    ctx.rect(L.ox, yTop, L.cw, yBottom - yTop);
    ctx.clip();
    ctx.font = `bold ${m.fs}px ${FONT}`;
    ctx.textAlign = "center";
    for (let c = this.c0; c <= this.c1; c++) {
      const x = this.cx(c);
      ctx.fillStyle = pal.clueBg;
      ctx.fillRect(x, yTop, this.C, yBottom - yTop);
      if (this.xc === c && this.s.crosshair.headers) {
        ctx.fillStyle = rgba(this.s.crosshair.colColor, this.s.crosshair.colAlpha);
        ctx.fillRect(x, yTop, this.C, yBottom - yTop);
      }
      const clues = p.colClues[c];
      for (let i = 0; i < clues.length; i++) {
        const y = yBottom - (clues.length - i) * m.N;
        const hv = inp.hover.clue;
        if (hv && hv.isCol && hv.line === c && hv.idx === i) {
          ctx.fillStyle = hoverShade(pal.clueBg);
          ctx.fillRect(x, y, this.C, m.N);
        }
        const dim = !!inp.derived?.colFulfilled[c] || !!inp.derived?.colIndiv[c][i] || inp.dimCols.has(`${c},${i}`);
        ctx.fillStyle = this.clueInk(dim);
        ctx.fillText(String(clues[i]), x + this.C / 2, y + m.N / 2 + 0.5);
      }
    }
    for (let c = this.c0; c <= this.c1 + 1; c++) {
      if (c === 0 || c === this.w) continue; // borders: drawn once, by the frame
      this.vline(this.cx(c), yTop, yBottom, c % 5 === 0);
    }
    ctx.restore();
  }

  rowClues(): void {
    const { ctx, L, p, pal, inp } = this;
    const { m } = L;
    const xLeft = L.originX;
    const xRight = L.ox - L.m.sep;
    ctx.save();
    ctx.beginPath();
    ctx.rect(xLeft, L.oy, xRight - xLeft, L.ch);
    ctx.clip();
    ctx.font = `bold ${m.fs}px ${FONT}`;
    ctx.textAlign = "center";
    for (let r = this.r0; r <= this.r1; r++) {
      const y = this.cy(r);
      ctx.fillStyle = pal.clueBg;
      ctx.fillRect(xLeft, y, xRight - xLeft, this.C);
      if (this.xr === r && this.s.crosshair.headers) {
        ctx.fillStyle = rgba(this.s.crosshair.rowColor, this.s.crosshair.rowAlpha);
        ctx.fillRect(xLeft, y, xRight - xLeft, this.C);
      }
      const clues = p.rowClues[r];
      for (let i = 0; i < clues.length; i++) {
        const x = xRight - (clues.length - i) * m.N;
        const hv = inp.hover.clue;
        if (hv && !hv.isCol && hv.line === r && hv.idx === i) {
          ctx.fillStyle = hoverShade(pal.clueBg);
          ctx.fillRect(x, y, m.N, this.C);
        }
        const dim = !!inp.derived?.rowFulfilled[r] || !!inp.derived?.rowIndiv[r][i] || inp.dimRows.has(`${r},${i}`);
        ctx.fillStyle = this.clueInk(dim);
        ctx.fillText(String(clues[i]), x + m.N / 2, y + this.C / 2 + 0.5);
      }
    }
    for (let r = this.r0; r <= this.r1 + 1; r++) {
      if (r === 0 || r === this.h) continue; // borders: drawn once, by the frame
      this.hline(this.cy(r), xLeft, xRight, r % 5 === 0);
    }
    ctx.restore();
  }

  // ── Sums ──────────────────────────────────────────────────────────────────

  /** `indiv` is that line's `individuallyFulfilledClues` result, when available: with "exclude
   *  completed" on, those clues drop out, so the sum stays a relevant comparison against what's
   *  actually left to place (not the grand totals — those must stay full sums, or a puzzle whose
   *  rows and columns happen to have completed different amounts would falsely look mismatched). */
  private sumOf(clues: readonly number[], indiv: boolean[] | null): number {
    const live = this.s.assist.clueSumsExcludeCompleted && indiv ? clues.filter((_, i) => !indiv[i]) : clues;
    let s = 0;
    for (const c of live) s += c;
    return this.s.assist.clueSumsWithGaps ? s + Math.max(0, live.length - 1) : s;
  }

  /** A number in a sum cell; infeasible lines get an amber badge like the desktop GUI. */
  private sumText(text: string, cx: number, cy: number, warn: boolean, align: "center" | "right" = "center"): void {
    const { ctx, L } = this;
    const warnInk = WARN[this.inp.theme];
    ctx.font = `${L.m.fs}px ${FONT}`;
    ctx.textAlign = align;
    if (warn) {
      const tw = ctx.measureText(text).width;
      const bw = tw + 8;
      const bh = L.m.fs + 6;
      const bx = align === "center" ? cx - bw / 2 : cx - bw + 4;
      ctx.fillStyle = rgba(warnInk, 0.16);
      ctx.strokeStyle = rgba(warnInk, 0.7);
      ctx.lineWidth = 1;
      ctx.beginPath();
      ctx.roundRect(bx, cy - bh / 2, bw, bh, 3);
      ctx.fill();
      ctx.stroke();
    }
    ctx.fillStyle = warn ? warnInk : this.pal.clueText;
    ctx.fillText(text, cx, cy + 0.5);
  }

  sums(): void {
    if (!this.L.m.showSums) return;
    const { ctx, L, p, pal, inp } = this;
    const { m } = L;
    const rx = L.ox + L.cw + L.m.sep;
    const by = L.oy + L.ch + L.m.sep;
    const totalRow = clueTotal(p.rowClues);
    const totalCol = clueTotal(p.colClues);
    const mismatch = totalRow !== totalCol;

    // Row sums (right strip)
    ctx.save();
    ctx.beginPath();
    ctx.rect(rx, L.oy, m.sumW, L.ch);
    ctx.clip();
    ctx.fillStyle = pal.sumBg;
    ctx.fillRect(rx, L.oy, m.sumW, L.ch);
    for (let r = this.r0; r <= this.r1; r++) {
      this.sumText(String(this.sumOf(p.rowClues[r], inp.derived?.rowIndiv[r] ?? null)), rx + m.sumW / 2, this.cy(r) + this.C / 2, minSpan(p.rowClues[r]) > p.width);
    }
    for (let r = this.r0; r <= this.r1 + 1; r++) {
      if (r === 0 || r === this.h) continue;
      this.hline(this.cy(r), rx, rx + m.sumW, r % 5 === 0);
    }
    ctx.restore();

    // Column sums (bottom strip)
    ctx.save();
    ctx.beginPath();
    ctx.rect(L.ox, by, L.cw, m.N);
    ctx.clip();
    ctx.fillStyle = pal.sumBg;
    ctx.fillRect(L.ox, by, L.cw, m.N);
    for (let c = this.c0; c <= this.c1; c++) {
      this.sumText(String(this.sumOf(p.colClues[c], inp.derived?.colIndiv[c] ?? null)), this.cx(c) + this.C / 2, by + m.N / 2, minSpan(p.colClues[c]) > p.height);
    }
    for (let c = this.c0; c <= this.c1 + 1; c++) {
      if (c === 0 || c === this.w) continue;
      this.vline(this.cx(c), by, by + m.N, c % 5 === 0);
    }
    ctx.restore();

    // Grand totals: each sits beside the clue strip it totals.
    ctx.fillStyle = pal.sumBg;
    ctx.fillRect(rx, L.originY, m.sumW, L.oy - L.m.sep - L.originY);
    this.sumText(String(totalCol), rx + m.sumW / 2, L.oy - L.m.sep - m.N / 2, mismatch);
    ctx.fillStyle = pal.sumBg;
    ctx.fillRect(L.originX, by, L.ox - L.m.sep - L.originX, m.N);
    this.sumText(String(totalRow), L.ox - L.m.sep - 6, by + m.N / 2, mismatch, "right");

    // Bottom-right: toggles "sums include gaps".
    ctx.fillStyle = inp.hover.sumToggle ? mix(pal.sumBg, luminance(pal.sumBg) > 0.5 ? "#000000" : "#ffffff", 0.12) : pal.sumBg;
    ctx.fillRect(rx, by, m.sumW, m.N);
    ctx.fillStyle = rgba(pal.clueText, this.s.assist.clueSumsWithGaps ? 0.85 : 0.4);
    ctx.font = `${Math.round(m.fs * 0.85)}px ${FONT}`;
    ctx.textAlign = "center";
    ctx.fillText(this.s.assist.clueSumsWithGaps ? "+g" : "Σ", rx + m.sumW / 2, by + m.N / 2 + 0.5);
  }

  // ── Frame & minimap ───────────────────────────────────────────────────────

  frame(): void {
    const { ctx, L, pal } = this;
    const { m } = L;
    const right = L.ox + L.cw + m.rightW;
    const bottom = L.oy + L.ch + m.bottomH;
    // Same thickness as the 5-cell grid lines, so a border never reads heavier than they do.
    const t = this.thick(m.sep);
    ctx.fillStyle = pal.borderMaj;
    ctx.fillRect(this.snap(L.ox - t), L.originY, t, bottom - L.originY); // left of cells
    ctx.fillRect(L.originX, this.snap(L.oy - t), right - L.originX, t); // above cells
    if (m.showSums) {
      ctx.fillRect(this.snap(L.ox + L.cw), L.originY, t, bottom - L.originY); // right of cells
      ctx.fillRect(L.originX, this.snap(L.oy + L.ch), right - L.originX, t); // below cells
      // Thin outline around the whole frame.
      ctx.fillRect(L.originX - 1, L.originY - 1, right - L.originX + 2, 1);
      ctx.fillRect(L.originX - 1, L.originY - 1, 1, bottom - L.originY + 2);
      ctx.fillRect(right, L.originY - 1, 1, bottom - L.originY + 2);
      ctx.fillRect(L.originX - 1, bottom, right - L.originX + 2, 1);
    } else {
      // No sum strips: the grid's own edge closes the frame, drawn just inside it.
      ctx.fillRect(this.snap(right - t), L.originY, t, bottom - L.originY); // right edge, clue strips included
      ctx.fillRect(L.originX, this.snap(bottom - t), right - L.originX, t); // bottom edge, clue strips included
      ctx.fillRect(L.originX - 1, L.originY - 1, right - L.originX + 1, 1); // outline: top…
      ctx.fillRect(L.originX - 1, L.originY - 1, 1, bottom - L.originY + 1); // …and left only
    }
  }

  minimap(): void {
    const { ctx, L, p, pal, inp } = this;
    const { m } = L;
    const areaW = m.leftW - L.m.sep;
    const areaH = m.topH - L.m.sep;
    const x0 = L.originX;
    const y0 = L.originY;
    ctx.save();
    ctx.beginPath();
    ctx.rect(x0, y0, areaW, areaH);
    ctx.clip();
    ctx.fillStyle = pal.canvasBg;
    ctx.fillRect(x0, y0, areaW, areaH);

    const g = minimapGeometry(L, p);
    const { cs, mw, mh } = g;
    const px = x0 + g.px;
    const py = y0 + g.py;
    const { grid, tierMap } = inp;
    for (let r = 0; r < this.h; r++) {
      for (let c = 0; c < this.w; c++) {
        const i = r * this.w + c;
        ctx.fillStyle = this.stateColor(grid[i], tierMap[i]);
        const xx = Math.round(px + c * cs);
        const yy = Math.round(py + r * cs);
        ctx.fillRect(xx, yy, Math.round(px + (c + 1) * cs) - xx, Math.round(py + (r + 1) * cs) - yy);
      }
    }

    // Viewport indicator when the puzzle is scrolled/zoomed past the window.
    if (L.cw < L.fullW || L.ch < L.fullH) {
      const vx = px + (L.panX / L.fullW) * mw;
      const vy = py + (L.panY / L.fullH) * mh;
      const vw = (L.cw / L.fullW) * mw;
      const vh = (L.ch / L.fullH) * mh;
      ctx.strokeStyle = MINIMAP_VIEWPORT;
      ctx.lineWidth = 1.5;
      ctx.strokeRect(vx, vy, vw, vh);
    }
    ctx.restore();
  }
}

/** Exposed for tests / the app: where the minimap's grid sits inside its area. */
export function minimapGeometry(L: Layout, p: Puzzle) {
  const areaW = L.m.leftW - L.m.sep;
  const areaH = L.m.topH - L.m.sep;
  const cs = Math.max(1, Math.min(areaW / (p.width + 2), areaH / (p.height + 2)));
  const mw = cs * p.width;
  const mh = cs * p.height;
  return { areaW, areaH, cs, mw, mh, px: (areaW - mw) / 2, py: (areaH - mh) / 2 };
}

