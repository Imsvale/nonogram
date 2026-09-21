import { EMPTY, FILLED, UNKNOWN, type Grid, type Puzzle } from "./types";

export function getRow(grid: Grid, w: number, r: number): Uint8Array {
  return grid.subarray(r * w, r * w + w);
}

export function getCol(grid: Grid, w: number, h: number, c: number): Uint8Array {
  const out = new Uint8Array(h);
  for (let r = 0; r < h; r++) out[r] = grid[r * w + c];
  return out;
}

/** True when the filled runs of `cells` are exactly `clues`. */
export function checkLineFulfilled(clues: readonly number[], cells: ArrayLike<number>): boolean {
  let idx = 0;
  let run = 0;
  for (let i = 0; i < cells.length; i++) {
    if (cells[i] === FILLED) {
      run++;
    } else if (run > 0) {
      if (idx >= clues.length || clues[idx] !== run) return false;
      idx++;
      run = 0;
    }
  }
  if (run > 0) {
    if (idx >= clues.length || clues[idx] !== run) return false;
    idx++;
  }
  return idx === clues.length;
}

export function isPuzzleSolved(p: Puzzle, grid: Grid): boolean {
  const { width: w, height: h } = p;
  for (let r = 0; r < h; r++) {
    if (!checkLineFulfilled(p.rowClues[r], getRow(grid, w, r))) return false;
  }
  for (let c = 0; c < w; c++) {
    if (!checkLineFulfilled(p.colClues[c], getCol(grid, w, h, c))) return false;
  }
  return true;
}

/**
 * Per-clue "this clue is definitely satisfied" flags, scanning inward from both
 * edges. A run counts once its outer boundary is confirmed (edge or Empty); an
 * Unknown on the inner side doesn't block it. Mirrors the desktop GUI exactly.
 */
export function individuallyFulfilledClues(clues: readonly number[], cells: ArrayLike<number>): boolean[] {
  const n = clues.length;
  const w = cells.length;
  const dim = new Array<boolean>(n).fill(false);
  if (n === 0 || w === 0) return dim;

  let ci = 0;
  let gi = 0;
  while (ci < n) {
    while (gi < w && cells[gi] === EMPTY) gi++;
    if (gi >= w || cells[gi] === UNKNOWN) break;
    const start = gi;
    while (gi < w && cells[gi] === FILLED) gi++;
    if (gi - start === clues[ci]) {
      dim[ci] = true;
      ci++;
    } else break;
  }
  const leftMatched = ci;

  ci = n - 1;
  gi = w - 1;
  while (ci >= leftMatched) {
    while (gi >= 0 && cells[gi] === EMPTY) gi--;
    if (gi < 0 || cells[gi] === UNKNOWN) break;
    const end = gi;
    while (gi >= 0 && cells[gi] === FILLED) gi--;
    if (end - gi === clues[ci]) {
      dim[ci] = true;
      ci--;
    } else break;
  }
  return dim;
}

type Span = [start: number, endExclusive: number];

// Left scan where BOTH boundaries of each run are confirmed (Empty or edge).
function scanConfirmedLeft(clues: readonly number[], cells: ArrayLike<number>): Span[] {
  const n = clues.length;
  const w = cells.length;
  const runs: Span[] = [];
  let ci = 0;
  let gi = 0;
  while (ci < n) {
    while (gi < w && cells[gi] === EMPTY) gi++;
    if (gi >= w || cells[gi] === UNKNOWN) break;
    const start = gi;
    while (gi < w && cells[gi] === FILLED) gi++;
    const end = gi;
    if (gi < w && cells[gi] === UNKNOWN) break; // right boundary uncertain
    if (end - start === clues[ci]) {
      runs.push([start, end]);
      ci++;
    } else break;
  }
  return runs;
}

// Mirror of the above, rightmost run first.
function scanConfirmedRight(clues: readonly number[], cells: ArrayLike<number>, skipLeft: number): Span[] {
  const n = clues.length;
  const w = cells.length;
  const runs: Span[] = [];
  let ci = n - 1;
  let gi = w - 1;
  while (ci >= skipLeft) {
    while (gi >= 0 && cells[gi] === EMPTY) gi--;
    if (gi < 0 || cells[gi] === UNKNOWN) break;
    const endIncl = gi;
    while (gi >= 0 && cells[gi] === FILLED) gi--;
    const start = gi + 1;
    if (gi >= 0 && cells[gi] === UNKNOWN) break; // left boundary uncertain
    if (endIncl - start + 1 === clues[ci]) {
      runs.push([start, endIncl + 1]);
      ci--;
    } else break;
  }
  return runs;
}

/**
 * Indices of Unknown cells that are provably Empty because they lie outside /
 * between runs that are confirmed from the line's edges.
 */
export function forcedEmptyFromEdges(clues: readonly number[], cells: ArrayLike<number>): number[] {
  const n = clues.length;
  const len = cells.length;
  if (len === 0) return [];

  const left = scanConfirmedLeft(clues, cells);
  const right = scanConfirmedRight(clues, cells, left.length);
  const out = new Set<number>();
  const mark = (a: number, b: number) => {
    for (let i = a; i < b; i++) if (cells[i] === UNKNOWN) out.add(i);
  };

  if (left.length) mark(0, left[0][0]);
  for (let i = 0; i + 1 < left.length; i++) mark(left[i][1], left[i + 1][0]);
  // When the confirmed runs account for every clue, nothing can sit between the
  // two frontiers. Deliberately also fires when only one side has runs (the
  // desktop GUI requires both, which leaves e.g. `###x...` for clue [3] alone).
  if (left.length + right.length === n) {
    const from = left.length ? left[left.length - 1][1] : 0;
    const to = right.length ? right[right.length - 1][0] : len;
    mark(from, to);
  }
  for (let i = 0; i + 1 < right.length; i++) mark(right[i + 1][1], right[i][0]);
  if (right.length) mark(right[0][1], len);

  return [...out].sort((a, b) => a - b);
}

/** Minimum span a clue list needs: sum + one gap between each pair. */
export function minSpan(clues: readonly number[]): number {
  let s = 0;
  for (const c of clues) s += c;
  return s + Math.max(0, clues.length - 1);
}

export function clueTotal(lines: readonly (readonly number[])[]): number {
  let s = 0;
  for (const l of lines) for (const c of l) s += c;
  return s;
}

/** Cheap structural problems with a puzzle, or null if it looks well-formed. */
export function trivialInvalidReason(p: Puzzle): string | null {
  const rowSum = clueTotal(p.rowClues);
  const colSum = clueTotal(p.colClues);
  if (rowSum !== colSum) {
    return `clue sums don't match (rows sum to ${rowSum}, cols sum to ${colSum})`;
  }
  for (let i = 0; i < p.rowClues.length; i++) {
    const min = minSpan(p.rowClues[i]);
    if (min > p.width) return `row ${i + 1} requires at least ${min} cells but the grid is only ${p.width} wide`;
  }
  for (let i = 0; i < p.colClues.length; i++) {
    const min = minSpan(p.colClues[i]);
    if (min > p.height) return `col ${i + 1} requires at least ${min} cells but the grid is only ${p.height} tall`;
  }
  return null;
}
