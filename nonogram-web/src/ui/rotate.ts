import type { Grid, Puzzle } from "../core/types";
import type { Derived } from "../state/game";

/**
 * View-only transposition layer: lets the grid be shown and played rotated 90°/180°/270° from
 * how the puzzle's own data (clues, grid, undo, dim state) is stored. Nothing downstream of this
 * module — `Game`, `geometry.ts`, `render.ts` — knows rotation exists; they're handed an already-
 * rotated `Puzzle`/`Grid`/etc. for display, and real coordinates once an input reaches `Game`.
 */
export type Rotation = 0 | 90 | 180 | 270;

export const rotateCW = (r: Rotation): Rotation => (((r + 90) % 360) as Rotation);
export const rotateCCW = (r: Rotation): Rotation => (((r + 270) % 360) as Rotation);

export function viewDims(width: number, height: number, rot: Rotation): { width: number; height: number } {
  return rot === 90 || rot === 270 ? { width: height, height: width } : { width, height };
}

/** Real (r,c) in a realW×realH grid -> its position once the grid is rotated `rot`. */
export function toView(rot: Rotation, realW: number, realH: number, r: number, c: number): [number, number] {
  switch (rot) {
    case 0: return [r, c];
    case 90: return [c, realH - 1 - r];
    case 180: return [realH - 1 - r, realW - 1 - c];
    case 270: return [realW - 1 - c, r];
  }
}

/** Inverse of `toView`: a view-space (r,c) back to its real position. */
export function toReal(rot: Rotation, realW: number, realH: number, vr: number, vc: number): [number, number] {
  switch (rot) {
    case 0: return [vr, vc];
    case 90: return [realH - 1 - vc, vr];
    case 180: return [realH - 1 - vr, realW - 1 - vc];
    case 270: return [vc, realW - 1 - vr];
  }
}

/** Which real line backs a given view row/col index, and whether it reads back-to-front. */
export interface LineSource {
  isCol: boolean;
  line: number;
  reversed: boolean;
}

export function rowSource(rot: Rotation, viewRow: number, realW: number, realH: number): LineSource {
  switch (rot) {
    case 0: return { isCol: false, line: viewRow, reversed: false };
    case 90: return { isCol: true, line: viewRow, reversed: true };
    case 180: return { isCol: false, line: realH - 1 - viewRow, reversed: true };
    case 270: return { isCol: true, line: realW - 1 - viewRow, reversed: false };
  }
}

export function colSource(rot: Rotation, viewCol: number, realW: number, realH: number): LineSource {
  switch (rot) {
    case 0: return { isCol: true, line: viewCol, reversed: false };
    case 90: return { isCol: false, line: realH - 1 - viewCol, reversed: false };
    case 180: return { isCol: true, line: realW - 1 - viewCol, reversed: true };
    case 270: return { isCol: false, line: viewCol, reversed: true };
  }
}

function pickLine<T>(src: LineSource, lines: { row: T[]; col: T[] }, reverse: (t: T) => T): T {
  const arr = src.isCol ? lines.col : lines.row;
  const t = arr[src.line];
  return src.reversed ? reverse(t) : t;
}

export function viewPuzzle(p: Puzzle, rot: Rotation): Puzzle {
  if (rot === 0) return p;
  const { width, height } = viewDims(p.width, p.height, rot);
  const revArr = (a: number[]) => [...a].reverse();
  const rowClues: number[][] = [];
  for (let i = 0; i < height; i++) rowClues.push(pickLine(rowSource(rot, i, p.width, p.height), { row: p.rowClues, col: p.colClues }, revArr));
  const colClues: number[][] = [];
  for (let j = 0; j < width; j++) colClues.push(pickLine(colSource(rot, j, p.width, p.height), { row: p.rowClues, col: p.colClues }, revArr));
  return { ...p, width, height, rowClues, colClues };
}

export function viewGrid(real: Grid, realW: number, realH: number, rot: Rotation): Grid {
  if (rot === 0) return real;
  const { width, height } = viewDims(realW, realH, rot);
  const out = new Uint8Array(width * height);
  for (let vr = 0; vr < height; vr++) {
    for (let vc = 0; vc < width; vc++) {
      const [r, c] = toReal(rot, realW, realH, vr, vc);
      out[vr * width + vc] = real[r * realW + c];
    }
  }
  return out;
}

export function viewIndexSet(real: ReadonlySet<number>, realW: number, realH: number, rot: Rotation): Set<number> {
  if (rot === 0) return new Set(real);
  const { width } = viewDims(realW, realH, rot);
  const out = new Set<number>();
  for (const i of real) {
    const r = Math.floor(i / realW);
    const c = i % realW;
    const [vr, vc] = toView(rot, realW, realH, r, c);
    out.add(vr * width + vc);
  }
  return out;
}

export function viewDerived(d: Derived, p: Puzzle, rot: Rotation): Derived {
  if (rot === 0) return d;
  const { width, height } = viewDims(p.width, p.height, rot);
  const revArr = <T>(a: T[]) => [...a].reverse();
  const id = <T>(a: T) => a;
  const rowFulfilled: boolean[] = [];
  const rowIndiv: boolean[][] = [];
  for (let i = 0; i < height; i++) {
    const src = rowSource(rot, i, p.width, p.height);
    rowFulfilled.push(pickLine(src, { row: d.rowFulfilled, col: d.colFulfilled }, id));
    rowIndiv.push(pickLine(src, { row: d.rowIndiv, col: d.colIndiv }, revArr));
  }
  const colFulfilled: boolean[] = [];
  const colIndiv: boolean[][] = [];
  for (let j = 0; j < width; j++) {
    const src = colSource(rot, j, p.width, p.height);
    colFulfilled.push(pickLine(src, { row: d.rowFulfilled, col: d.colFulfilled }, id));
    colIndiv.push(pickLine(src, { row: d.rowIndiv, col: d.colIndiv }, revArr));
  }
  return { rowFulfilled, colFulfilled, rowIndiv, colIndiv };
}

function dimBool(set: ReadonlySet<string>, line: number, idx: number): boolean {
  return set.has(`${line},${idx}`);
}

export function viewDimSets(
  dimRows: ReadonlySet<string>,
  dimCols: ReadonlySet<string>,
  p: Puzzle,
  rot: Rotation,
): { dimRows: Set<string>; dimCols: Set<string> } {
  if (rot === 0) return { dimRows: new Set(dimRows), dimCols: new Set(dimCols) };
  const { width, height } = viewDims(p.width, p.height, rot);
  const outRows = new Set<string>();
  for (let i = 0; i < height; i++) {
    const src = rowSource(rot, i, p.width, p.height);
    const clueLen = (src.isCol ? p.colClues : p.rowClues)[src.line].length;
    const set = src.isCol ? dimCols : dimRows;
    for (let idx = 0; idx < clueLen; idx++) {
      if (dimBool(set, src.line, idx)) outRows.add(`${i},${src.reversed ? clueLen - 1 - idx : idx}`);
    }
  }
  const outCols = new Set<string>();
  for (let j = 0; j < width; j++) {
    const src = colSource(rot, j, p.width, p.height);
    const clueLen = (src.isCol ? p.colClues : p.rowClues)[src.line].length;
    const set = src.isCol ? dimCols : dimRows;
    for (let idx = 0; idx < clueLen; idx++) {
      if (dimBool(set, src.line, idx)) outCols.add(`${j},${src.reversed ? clueLen - 1 - idx : idx}`);
    }
  }
  return { dimRows: outRows, dimCols: outCols };
}

/** Input-side inverse: a clue clicked in view space at (isColView, lineView, idxView) ->
 *  the real (isCol, line, idx) `Game.toggleDim` expects. */
export function realDimAddress(
  rot: Rotation,
  p: Puzzle,
  isColView: boolean,
  lineView: number,
  idxView: number,
): { isCol: boolean; line: number; idx: number } {
  if (rot === 0) return { isCol: isColView, line: lineView, idx: idxView };
  const src = isColView ? colSource(rot, lineView, p.width, p.height) : rowSource(rot, lineView, p.width, p.height);
  const realLen = (src.isCol ? p.colClues : p.rowClues)[src.line].length;
  return { isCol: src.isCol, line: src.line, idx: src.reversed ? realLen - 1 - idxView : idxView };
}
