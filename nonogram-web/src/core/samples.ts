import { FILLED, EMPTY, type Puzzle } from "./types";

function runs(line: boolean[]): number[] {
  const out: number[] = [];
  let n = 0;
  for (const b of line) {
    if (b) n++;
    else if (n) {
      out.push(n);
      n = 0;
    }
  }
  if (n) out.push(n);
  return out;
}

/** Build a puzzle (with its solution) from ASCII art: `#` = filled, anything else = empty. */
export function puzzleFromPicture(name: string, answer: string, art: string[]): Puzzle {
  const height = art.length;
  const width = art[0].length;
  const cell = (r: number, c: number) => art[r][c] === "#";
  const rowClues = art.map((_, r) => runs(Array.from({ length: width }, (_, c) => cell(r, c))));
  const colClues = Array.from({ length: width }, (_, c) => runs(Array.from({ length: height }, (_, r) => cell(r, c))));
  const solution = new Uint8Array(width * height);
  for (let r = 0; r < height; r++) for (let c = 0; c < width; c++) solution[r * width + c] = cell(r, c) ? FILLED : EMPTY;
  return { name, answer, width, height, rowClues, colClues, solution };
}

/** Original pixel-art puzzles so the start page has something to try. */
export const SAMPLES: Puzzle[] = [
  puzzleFromPicture("Warm-up 5×5", "A plus sign", [
    "..#..",
    "..#..",
    "#####",
    "..#..",
    "..#..",
  ]),
  puzzleFromPicture("Heart 10×10", "A heart", [
    ".###..###.",
    "##########",
    "##########",
    "##########",
    "##########",
    ".########.",
    "..######..",
    "...####...",
    "....##....",
    "...#..#...",
  ]),
  puzzleFromPicture("Invader 11×8", "A space invader", [
    "..#.....#..",
    "...#...#...",
    "..#######..",
    ".##.###.##.",
    "###########",
    "#.#######.#",
    "#.#.....#.#",
    "...##.##...",
  ]),
];
