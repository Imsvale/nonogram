/** Cell states. Grids are `Uint8Array`s of these values (row-major). */
export const UNKNOWN = 0;
export const FILLED = 1;
export const EMPTY = 2;
export type Cell = 0 | 1 | 2;
export type Grid = Uint8Array;

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
}

export function newGrid(p: { width: number; height: number }): Grid {
  return new Uint8Array(p.width * p.height);
}
