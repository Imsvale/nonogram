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

/**
 * Indices of Unknown cells that must be Empty because they sit immediately past a run that
 * already exactly matches its clue, scanning inward from both edges — the same confirmation
 * `individuallyFulfilledClues` uses (and stopping at the same point), extended one cell further:
 * once a run's length matches its clue exactly, the very next cell can't belong to that run (too
 * long) or the next one (needs a gap first), so it's forced Empty regardless of what lies beyond.
 */
export function forcedEmptyBeyondMatchedRuns(clues: readonly number[], cells: ArrayLike<number>): number[] {
  const n = clues.length;
  const w = cells.length;
  const out = new Set<number>();
  if (n === 0 || w === 0) return [];

  let ci = 0;
  let gi = 0;
  while (ci < n) {
    while (gi < w && cells[gi] === EMPTY) gi++;
    if (gi >= w || cells[gi] === UNKNOWN) break;
    const start = gi;
    while (gi < w && cells[gi] === FILLED) gi++;
    if (gi - start !== clues[ci]) break;
    if (gi < w && cells[gi] === UNKNOWN) out.add(gi);
    ci++;
  }
  const leftMatched = ci;

  ci = n - 1;
  gi = w - 1;
  while (ci >= leftMatched) {
    while (gi >= 0 && cells[gi] === EMPTY) gi--;
    if (gi < 0 || cells[gi] === UNKNOWN) break;
    const end = gi;
    while (gi >= 0 && cells[gi] === FILLED) gi--;
    if (end - gi !== clues[ci]) break;
    if (gi >= 0 && cells[gi] === UNKNOWN) out.add(gi);
    ci--;
  }
  return [...out].sort((a, b) => a - b);
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

export interface LineResolution {
  /** Per-clue: true once its run is confirmed complete, by any of the rules below. */
  dim: boolean[];
  /** Unknown cells just past a confirmed run (any anchor kind) — safe to cross, since the run's
   *  length already matches its clue and the next cell can't extend it. */
  crossable: number[];
  /** Unknown boundary cells of a run that was confirmed only via the undelimited ("guess") rule.
   *  Crossing these unconditionally is what makes the guess durable: once both sides are Empty,
   *  the run can never grow, so it's no longer a guess from the next recompute on. */
  guessCrossable: number[];
}

interface Anchor {
  /** Clue index; -1 and `clues.length` are the line's own two edges (no real run). */
  ci: number;
  start: number;
  end: number;
  /** Found only via the undelimited rule (see `resolveLine`'s `allowUndelimited`). */
  guess: boolean;
}

/**
 * The full per-line "which clues are definitely done" resolution — `individuallyFulfilledClues`
 * generalized with two more ways to anchor a run to a specific clue, beyond just the two edges:
 *
 * 1. A run delimited on both sides (Empty or the line's own edge) that isn't yet reachable from
 *    either edge, whose length matches exactly one still-open clue in the range between its
 *    neighboring anchors, is anchored to that clue — it can't grow, so the match is as good as an
 *    edge match. This is always on (folds into the existing "Dim fulfilled clues" behavior).
 * 2. When `allowUndelimited` is on: the same, but without requiring delimiting, PROVIDED the run's
 *    length equals the unique largest value among that gap's still-open clues — the only case
 *    where an as-yet-ungrown run can't need to grow further, since no bigger clue is left to
 *    become. This is what makes it safe to guess at a run that's still mid-paint elsewhere in the
 *    puzzle: a short, possibly-still-growing run never accidentally matches the *largest* clue.
 *
 * Every clue newly anchored this way becomes exactly the kind of anchor an edge is — clues
 * adjacent to it chain-resolve outward with the same left/right scan the edges already use. The
 * whole thing is recomputed fresh from the current grid every call, so there is nothing to
 * explicitly undo: a run that qualified a moment ago and no longer does just isn't found again,
 * and neither is anything that was only dim because it chained from it.
 */
export function resolveLine(clues: readonly number[], cells: ArrayLike<number>, opts: { allowUndelimited?: boolean } = {}): LineResolution {
  const n = clues.length;
  const w = cells.length;
  const dim = new Array<boolean>(n).fill(false);
  if (n === 0 || w === 0) return { dim, crossable: [], guessCrossable: [] };
  const allowUndelimited = !!opts.allowUndelimited;

  // Every maximal filled run, found once up front; central-run matching below just filters these.
  const runs: Span[] = [];
  for (let i = 0; i < w; ) {
    if (cells[i] !== FILLED) {
      i++;
      continue;
    }
    const start = i;
    while (i < w && cells[i] === FILLED) i++;
    runs.push([start, i]);
  }
  const usedRuns = new Set<number>();

  // The line's own two ends act as fixed anchors for clue index -1 and `n` (no real run).
  let anchors: Anchor[] = [
    { ci: -1, start: 0, end: 0, guess: false },
    { ci: n, start: w, end: w, guess: false },
  ];

  // Each pass chains outward from every current anchor and looks for new central anchors in
  // what's left; newly found anchors are only merged in between passes, so a pass never sees its
  // own not-yet-sorted results. Bounded by `n` since a productive pass anchors at least one clue.
  for (let pass = 0; pass <= n; pass++) {
    anchors.sort((a, b) => a.ci - b.ci);
    const found: Anchor[] = [];

    for (let g = 0; g + 1 < anchors.length; g++) {
      const a = anchors[g];
      const b = anchors[g + 1];
      if (b.ci - a.ci <= 1) continue; // no clues left between these two anchors

      // Chain forward from `a`, exactly like the plain edge scan (`individuallyFulfilledClues`):
      // the outer boundary of each run must be confirmed, the inner side may still be Unknown.
      let ci = a.ci + 1;
      let gi = a.end;
      while (ci < b.ci) {
        while (gi < b.start && cells[gi] === EMPTY) gi++;
        if (gi >= b.start || cells[gi] === UNKNOWN) break;
        const start = gi;
        while (gi < b.start && cells[gi] === FILLED) gi++;
        if (gi - start !== clues[ci]) break;
        found.push({ ci, start, end: gi, guess: false });
        ci++;
      }
      // Mirror, chaining backward from `b`; stops at `ci` so the two chains can't overlap.
      let ciR = b.ci - 1;
      let giR = b.start;
      while (ciR >= ci) {
        while (giR > gi && cells[giR - 1] === EMPTY) giR--;
        if (giR <= gi || cells[giR - 1] === UNKNOWN) break;
        const end = giR;
        while (giR > gi && cells[giR - 1] === FILLED) giR--;
        if (end - giR !== clues[ciR]) break;
        found.push({ ci: ciR, start: giR, end, guess: false });
        ciR--;
      }

      // Central matching: runs entirely inside what neither chain reached, not yet claimed.
      const loPos = gi;
      const hiPos = giR;
      const loCi = ci;
      const hiCi = ciR + 1;
      if (hiCi <= loCi) continue;
      const remaining = clues.slice(loCi, hiCi);
      const claimed = new Set<number>(); // guards against two runs both uniquely matching one clue
      for (let ri = 0; ri < runs.length; ri++) {
        if (usedRuns.has(ri)) continue;
        const [rs, re] = runs[ri];
        if (rs < loPos || re > hiPos) continue;
        const len = re - rs;
        const delimited = (rs === loPos || cells[rs - 1] === EMPTY) && (re === hiPos || cells[re] === EMPTY);
        let value: number | null = null;
        let guess = false;
        if (delimited) {
          if (remaining.filter((v) => v === len).length === 1) value = len;
        } else if (allowUndelimited && remaining.length) {
          const maxV = Math.max(...remaining);
          if (maxV === len && remaining.filter((v) => v === maxV).length === 1) {
            value = len;
            guess = true;
          }
        }
        if (value === null) continue;
        const idx = loCi + remaining.indexOf(value);
        if (claimed.has(idx)) continue; // contradiction (two candidates, one clue) — trust neither
        claimed.add(idx);
        found.push({ ci: idx, start: rs, end: re, guess });
        usedRuns.add(ri);
      }
    }

    if (!found.length) break;
    for (const f of found) anchors.push(f);
  }

  const crossable = new Set<number>();
  const guessCrossable = new Set<number>();
  for (const a of anchors) {
    if (a.ci < 0 || a.ci >= n) continue;
    dim[a.ci] = true;
    const bucket = a.guess ? guessCrossable : crossable;
    if (a.start > 0 && cells[a.start - 1] === UNKNOWN) bucket.add(a.start - 1);
    if (a.end < w && cells[a.end] === UNKNOWN) bucket.add(a.end);
  }
  return {
    dim,
    crossable: [...crossable].sort((x, y) => x - y),
    guessCrossable: [...guessCrossable].sort((x, y) => x - y),
  };
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
