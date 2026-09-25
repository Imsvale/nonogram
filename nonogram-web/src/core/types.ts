/** Cell states. Grids are `Uint8Array`s of these values (row-major). */
export const UNKNOWN = 0;
export const FILLED = 1;
export const EMPTY = 2;
export type Cell = 0 | 1 | 2;
export type Grid = Uint8Array;

/** One changed cell: flat grid index, its value before, and after. */
export type CellChange = readonly [index: number, before: Cell, after: Cell];
/** Everything one undoable action changed — an empty diff means nothing actually changed. */
export type Diff = readonly CellChange[];

export interface Puzzle {
  name: string;
  width: number;
  height: number;
  /** One clue list per row (top → bottom); an empty list means a blank line. */
  rowClues: number[][];
  /** One clue list per column (left → right). */
  colClues: number[][];
  /** Title revealed once the puzzle is solved. */
  answer?: string;
  /** Row-major; FILLED / EMPTY only. Kept so exports round-trip. */
  solution?: Uint8Array;
  /**
   * Row-major grid to seed a freshly opened game with, when this browser has no saved progress
   * for the puzzle yet (a Puz-Pre v3 "with progress" file carries this). Unlike `solution`, this
   * is never a spoiler — only ever written by this app's own progress export.
   */
  progress?: Grid;
}

export function newGrid(p: { width: number; height: number }): Grid {
  return new Uint8Array(p.width * p.height);
}
