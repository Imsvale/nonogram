import { checkLineFulfilled, clueTotal, forcedEmptyFromEdges, getCol, getRow, isPuzzleSolved, resolveLine } from "../core/lines";
import { EMPTY, FILLED, UNKNOWN, newGrid, type Cell, type Grid, type Puzzle } from "../core/types";
import { gridToString, puzzleId, specFor, stringToGrid, type ProgressEntry } from "./progress";

/**
 * `fill` toggles Filled, `mark` toggles Empty, `cycle` steps Unknown → Filled →
 * Empty → Unknown (for touch screens, where there is no right button).
 */
export type PaintAction = "fill" | "mark" | "cycle";
export type Pos = [row: number, col: number];

export interface TrialTier {
  /** Grid as it was when the tier was opened. */
  snap: Grid;
  /** First cell painted inside the tier — shown as the tier's numbered marker. */
  origin: Pos | null;
}

export interface Stroke {
  target: Cell;
  anchor: Pos;
  last: Pos;
  axisLock: boolean;
  /** Cells that will be painted on release (axis-lock only). */
  preview: Pos[] | null;
  before: Grid;
}

export interface Derived {
  rowFulfilled: boolean[];
  colFulfilled: boolean[];
  rowIndiv: boolean[][];
  colIndiv: boolean[][];
}

const UNDO_LIMIT = 500;

/** Cells on the straight line a→b (inclusive), Bresenham. */
export function lineCells(a: Pos, b: Pos): Pos[] {
  const out: Pos[] = [];
  let [r, c] = a;
  const [r1, c1] = b;
  const dr = Math.abs(r1 - r);
  const dc = Math.abs(c1 - c);
  const sr = r < r1 ? 1 : -1;
  const sc = c < c1 ? 1 : -1;
  let err = dc - dr;
  for (;;) {
    out.push([r, c]);
    if (r === r1 && c === c1) break;
    const e2 = 2 * err;
    if (e2 > -dr) {
      err -= dr;
      c += sc;
    }
    if (e2 < dc) {
      err += dc;
      r += sr;
    }
  }
  return out;
}

/**
 * Axis-locked stroke: the dominant axis of anchor→current wins (ties go
 * horizontal), so the painted line is always straight.
 */
export function axisLine(anchor: Pos, current: Pos): Pos[] {
  const [ar, ac] = anchor;
  const [cr, cc] = current;
  const out: Pos[] = [];
  if (Math.abs(cc - ac) >= Math.abs(cr - ar)) {
    for (let c = Math.min(ac, cc); c <= Math.max(ac, cc); c++) out.push([ar, c]);
  } else {
    for (let r = Math.min(ar, cr); r <= Math.max(ar, cr); r++) out.push([r, ac]);
  }
  return out;
}

export class Game {
  readonly puzzle: Puzzle;
  readonly id: string;
  grid: Grid;
  /** Bumped on every visible change; caches key off it. */
  version = 0;
  undoStack: Grid[] = [];
  redoStack: Grid[] = [];
  trial: TrialTier[] = [];
  dimRows = new Set<string>();
  dimCols = new Set<string>();
  stroke: Stroke | null = null;
  solvedNow = false;
  everSolved = false;
  assist = { autoDim: false, autoFillEmpty: false, autoCrossEdges: false, autoCrossMatched: false, autoDimGuess: false };

  private elapsedBase = 0;
  private startedAt: number | null = null;
  private listeners = new Set<() => void>();
  private derivedCache: { version: number; guess: boolean; value: Derived } | null = null;
  private tierCache: { version: number; value: Uint8Array } | null = null;
  private readonly solvable: boolean;

  constructor(puzzle: Puzzle) {
    this.puzzle = puzzle;
    this.id = puzzleId(puzzle);
    this.grid = newGrid(puzzle);
    // A puzzle with no filled cells at all is trivially "solved" by an empty grid.
    this.solvable = clueTotal(puzzle.rowClues) > 0 && clueTotal(puzzle.rowClues) === clueTotal(puzzle.colClues);
  }

  // ── Observation ───────────────────────────────────────────────────────────

  subscribe(fn: () => void): () => void {
    this.listeners.add(fn);
    return () => this.listeners.delete(fn);
  }

  private bump(): void {
    this.version++;
    for (const fn of this.listeners) fn();
  }

  get canUndo(): boolean {
    return this.undoStack.length > 0;
  }
  get canRedo(): boolean {
    return this.redoStack.length > 0;
  }

  /** The grid to draw: includes the axis-lock preview while a stroke is in flight. */
  displayGrid(): Grid {
    const s = this.stroke;
    if (!s?.preview) return this.grid;
    const g = this.grid.slice();
    const w = this.puzzle.width;
    for (const [r, c] of s.preview) g[r * w + c] = s.target;
    return g;
  }

  derived(): Derived {
    const guess = this.assist.autoDim && this.assist.autoDimGuess;
    if (this.derivedCache?.version === this.version && this.derivedCache.guess === guess) return this.derivedCache.value;
    const { width: w, height: h, rowClues, colClues } = this.puzzle;
    const opts = { allowUndelimited: guess };
    const rowFulfilled: boolean[] = [];
    const rowIndiv: boolean[][] = [];
    for (let r = 0; r < h; r++) {
      const row = getRow(this.grid, w, r);
      rowFulfilled.push(checkLineFulfilled(rowClues[r], row));
      rowIndiv.push(resolveLine(rowClues[r], row, opts).dim);
    }
    const colFulfilled: boolean[] = [];
    const colIndiv: boolean[][] = [];
    for (let c = 0; c < w; c++) {
      const col = getCol(this.grid, w, h, c);
      colFulfilled.push(checkLineFulfilled(colClues[c], col));
      colIndiv.push(resolveLine(colClues[c], col, opts).dim);
    }
    const value = { rowFulfilled, colFulfilled, rowIndiv, colIndiv };
    this.derivedCache = { version: this.version, guess, value };
    return value;
  }

  /** Per cell: 0 = not part of a trial, n = painted during trial tier n. */
  tierMap(): Uint8Array {
    if (this.tierCache?.version === this.version) return this.tierCache.value;
    const out = new Uint8Array(this.grid.length);
    if (this.trial.length) {
      for (let i = 0; i < out.length; i++) {
        const st = this.grid[i];
        if (st === UNKNOWN) continue;
        for (let t = this.trial.length - 1; t >= 0; t--) {
          if (this.trial[t].snap[i] !== st) {
            out[i] = t + 1;
            break;
          }
        }
      }
    }
    this.tierCache = { version: this.version, value: out };
    return out;
  }

  // ── Painting ──────────────────────────────────────────────────────────────

  private writeCell(r: number, c: number, state: Cell): void {
    this.grid[r * this.puzzle.width + c] = state;
    this.applyLineAssistance(r, c, state);
  }

  /** Auto-fill / auto-cross for the row and column of a cell that was just painted. */
  private applyLineAssistance(row: number, col: number, paint: Cell): void {
    if (paint === UNKNOWN) return;
    const { autoFillEmpty, autoCrossEdges, autoCrossMatched, autoDim } = this.assist;
    const autoDimGuess = autoDim && this.assist.autoDimGuess; // the checkbox is a sub-option of "Dim fulfilled clues"
    if (!autoFillEmpty && !autoCrossEdges && !autoCrossMatched && !autoDimGuess) return;
    const { width: w, height: h, rowClues, colClues } = this.puzzle;
    const g = this.grid;

    if (autoFillEmpty) {
      if (checkLineFulfilled(rowClues[row], getRow(g, w, row))) {
        for (let c = 0; c < w; c++) if (g[row * w + c] === UNKNOWN) g[row * w + c] = EMPTY;
      }
      if (checkLineFulfilled(colClues[col], getCol(g, w, h, col))) {
        for (let r = 0; r < h; r++) if (g[r * w + col] === UNKNOWN) g[r * w + col] = EMPTY;
      }
    }
    if (autoCrossEdges) {
      for (const c of forcedEmptyFromEdges(rowClues[row], getRow(g, w, row))) g[row * w + c] = EMPTY;
      for (const r of forcedEmptyFromEdges(colClues[col], getCol(g, w, h, col))) g[r * w + col] = EMPTY;
    }
    if (autoCrossMatched || autoDimGuess) {
      const opts = { allowUndelimited: autoDimGuess };
      const rr = resolveLine(rowClues[row], getRow(g, w, row), opts);
      const cc = resolveLine(colClues[col], getCol(g, w, h, col), opts);
      // The "matched" crossing (any confirmed anchor's open end) and the "guess" crossing (an
      // undelimited anchor's own open ends) are independently gated — a guess-only run's ends
      // still get crossed even with autoCrossMatched off, since the guess assist implies it.
      if (autoCrossMatched) {
        for (const c of rr.crossable) g[row * w + c] = EMPTY;
        for (const r of cc.crossable) g[r * w + col] = EMPTY;
      }
      if (autoDimGuess) {
        for (const c of rr.guessCrossable) g[row * w + c] = EMPTY;
        for (const r of cc.guessCrossable) g[r * w + col] = EMPTY;
      }
    }
  }

  /** Begin a paint stroke at a cell. `fill` toggles Filled, `mark` toggles Empty. */
  startStroke(row: number, col: number, action: PaintAction, axisLock: boolean): void {
    if (this.stroke) this.cancelStroke();
    const cur = this.grid[row * this.puzzle.width + col];
    const target: Cell =
      action === "cycle"
        ? cur === UNKNOWN
          ? FILLED
          : cur === FILLED
            ? EMPTY
            : UNKNOWN
        : action === "mark"
          ? cur === EMPTY
            ? UNKNOWN
            : EMPTY
          : cur === FILLED
            ? UNKNOWN
            : FILLED;

    const tier = this.trial[this.trial.length - 1];
    if (tier && !tier.origin) tier.origin = [row, col];

    this.stroke = {
      target,
      anchor: [row, col],
      last: [row, col],
      axisLock,
      preview: axisLock ? [[row, col]] : null,
      before: this.grid.slice(),
    };
    if (!axisLock) {
      this.writeCell(row, col, target);
      this.checkSolved();
    }
    this.bump();
  }

  /** Extend the current stroke to a cell (call as the pointer moves). */
  strokeTo(row: number, col: number): void {
    const s = this.stroke;
    if (!s || (s.last[0] === row && s.last[1] === col)) return;
    if (s.axisLock) {
      s.last = [row, col];
      s.preview = axisLine(s.anchor, s.last);
    } else {
      // Interpolate so a fast drag can't skip cells.
      for (const [r, c] of lineCells(s.last, [row, col]).slice(1)) this.writeCell(r, c, s.target);
      s.last = [row, col];
      this.checkSolved();
    }
    this.bump();
  }

  /** Finish the stroke. Returns true if the grid changed. */
  endStroke(): boolean {
    const s = this.stroke;
    if (!s) return false;
    if (s.axisLock && s.preview) {
      for (const [r, c] of s.preview) this.writeCell(r, c, s.target);
    }
    this.stroke = null;
    const changed = !sameGrid(s.before, this.grid);
    if (changed) this.pushUndo(s.before);
    this.checkSolved();
    this.bump();
    return changed;
  }

  /** Abandon the stroke and put the grid back as it was (e.g. a second finger landed). */
  cancelStroke(): void {
    const s = this.stroke;
    if (!s) return;
    this.grid = s.before;
    this.stroke = null;
    this.checkSolved();
    this.bump();
  }

  private pushUndo(snapshot: Grid): void {
    this.undoStack.push(snapshot);
    if (this.undoStack.length > UNDO_LIMIT) this.undoStack.shift();
    this.redoStack = [];
  }

  undo(): void {
    const prev = this.undoStack.pop();
    if (!prev) return;
    this.redoStack.push(this.grid);
    this.grid = prev;
    this.afterBulkChange();
  }

  redo(): void {
    const next = this.redoStack.pop();
    if (!next) return;
    this.undoStack.push(this.grid);
    this.grid = next;
    this.afterBulkChange();
  }

  private afterBulkChange(): void {
    this.checkSolved();
    this.bump();
  }

  // ── Trial mode ────────────────────────────────────────────────────────────

  enterTrial(): void {
    this.trial.push({ snap: this.grid.slice(), origin: null });
    this.bump();
  }

  /** Keep everything painted in the innermost tier (merges it into the tier below). */
  acceptTrial(): void {
    if (!this.trial.pop()) return;
    this.bump();
  }

  /** Discard the innermost tier. Undoable, since it throws work away. */
  rejectTrial(): void {
    const tier = this.trial.pop();
    if (!tier) return;
    if (!sameGrid(tier.snap, this.grid)) this.pushUndo(this.grid);
    this.grid = tier.snap;
    this.afterBulkChange();
  }

  // ── Clue dimming ──────────────────────────────────────────────────────────

  toggleDim(isCol: boolean, line: number, idx: number): void {
    const set = isCol ? this.dimCols : this.dimRows;
    const k = `${line},${idx}`;
    if (!set.delete(k)) set.add(k);
    this.bump();
  }

  // ── Solved state & timer ──────────────────────────────────────────────────

  private checkSolved(): void {
    const now = this.solvable && isPuzzleSolved(this.puzzle, this.grid);
    const was = this.solvedNow;
    // Set first: pausing notifies listeners, who must already see "solved" (not "paused").
    this.solvedNow = now;
    if (now && !was) {
      this.everSolved = true;
      this.pauseTimer();
    }
  }

  timerElapsedMs(): number {
    return this.elapsedBase + (this.startedAt !== null ? Date.now() - this.startedAt : 0);
  }
  get timerRunning(): boolean {
    return this.startedAt !== null;
  }
  startTimer(): void {
    if (this.startedAt === null) {
      this.startedAt = Date.now();
      this.bump();
    }
  }
  pauseTimer(): void {
    if (this.startedAt !== null) {
      this.elapsedBase += Date.now() - this.startedAt;
      this.startedAt = null;
      this.bump();
    }
  }
  /** Back to zero; keeps running if it was running. */
  /**
   * Start over: blank grid, no open trial tiers, every clue un-dimmed, undo/redo
   * history dropped, timer back to zero (still running if it was). Not undoable.
   */
  restart(): void {
    this.grid = newGrid(this.puzzle);
    this.trial = [];
    this.dimRows.clear();
    this.dimCols.clear();
    this.undoStack = [];
    this.redoStack = [];
    this.elapsedBase = 0;
    this.startedAt = this.startedAt !== null ? Date.now() : null;
    this.afterBulkChange();
  }

  resetTimer(): void {
    this.elapsedBase = 0;
    this.startedAt = this.startedAt !== null ? Date.now() : null;
    this.bump();
  }

  // ── Persistence ───────────────────────────────────────────────────────────

  toProgress(): ProgressEntry {
    return {
      id: this.id,
      spec: specFor(this.puzzle),
      name: this.puzzle.name,
      width: this.puzzle.width,
      height: this.puzzle.height,
      grid: gridToString(this.grid),
      trial: this.trial.map((t) => ({ snap: gridToString(t.snap), origin: t.origin })),
      dimRows: [...this.dimRows],
      dimCols: [...this.dimCols],
      elapsedMs: this.timerElapsedMs(),
      everSolved: this.everSolved,
      updated: Date.now(),
    };
  }

  restore(e: ProgressEntry): void {
    const size = this.puzzle.width * this.puzzle.height;
    this.grid = stringToGrid(e.grid, size);
    this.trial = (e.trial ?? []).map((t) => ({ snap: stringToGrid(t.snap, size), origin: t.origin }));
    this.dimRows = new Set(e.dimRows ?? []);
    this.dimCols = new Set(e.dimCols ?? []);
    this.elapsedBase = e.elapsedMs ?? 0;
    this.startedAt = null;
    this.undoStack = [];
    this.redoStack = [];
    this.solvedNow = this.solvable && isPuzzleSolved(this.puzzle, this.grid);
    this.everSolved = !!e.everSolved || this.solvedNow;
    this.bump();
  }

  /** Like `restore`, but for a plain grid with no trial/dim/timer history (e.g. a puzzle file's
   *  own embedded progress) — used only when this browser has no saved progress to `restore()`. */
  seedGrid(grid: Grid): void {
    this.grid = grid;
    this.undoStack = [];
    this.redoStack = [];
    this.solvedNow = this.solvable && isPuzzleSolved(this.puzzle, this.grid);
    this.everSolved = this.solvedNow;
    this.bump();
  }
}

function sameGrid(a: Grid, b: Grid): boolean {
  if (a.length !== b.length) return false;
  for (let i = 0; i < a.length; i++) if (a[i] !== b[i]) return false;
  return true;
}
