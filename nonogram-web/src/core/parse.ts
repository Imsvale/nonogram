import { decodePuzzlinkParts, PUZZLINK_RE } from "./puzzlink";
import { EMPTY, FILLED, type Puzzle } from "./types";

export class ParseError extends Error {}

/** `s.split(sep)` limited to `n` parts; the last part keeps the remainder. */
function splitN(s: string, sep: string, n: number): string[] {
  const out: string[] = [];
  let rest = s;
  while (out.length < n - 1) {
    const i = rest.indexOf(sep);
    if (i < 0) break;
    out.push(rest.slice(0, i));
    rest = rest.slice(i + sep.length);
  }
  out.push(rest);
  return out;
}

function parseClueNumber(tok: string): number {
  if (!/^\d+$/.test(tok)) throw new ParseError(`invalid clue number: ${tok}`);
  return Number(tok);
}

function parsePipeGroup(s: string): number[][] {
  return s.split("|").map((line) => {
    const clues = line.split(/\s+/).filter(Boolean).map(parseClueNumber);
    // `0` is the "blank line" sentinel.
    return clues.some((n) => n === 0) ? clues.filter((n) => n !== 0) : clues;
  });
}

function splitSectionPrefix(s: string): ["C" | "R", string] {
  const t = s.trim();
  if (t.startsWith("C:")) return ["C", t.slice(2)];
  if (t.startsWith("R:")) return ["R", t.slice(2)];
  throw new ParseError(`clue section missing C: or R: prefix: "${t.slice(0, 20)}"`);
}

/**
 * One puzzle line: `name;C:col_clues/R:row_clues[;answer[;solution]]`.
 * Same grammar as `nonogram-core::parse_file`.
 */
export function parseNativeLine(line: string): Puzzle {
  const parts = splitN(line, ";", 4);
  if (parts.length < 2) throw new ParseError("missing clue section");

  const name = parts[0].trim();
  if (!name) throw new ParseError("empty puzzle name");

  const sections = splitN(parts[1].trim(), "/", 2);
  if (sections.length !== 2) throw new ParseError("missing clue section");
  const [aTag, aStr] = splitSectionPrefix(sections[0]);
  const [bTag, bStr] = splitSectionPrefix(sections[1]);

  let colClues: number[][];
  let rowClues: number[][];
  if (aTag === "C" && bTag === "R") {
    colClues = parsePipeGroup(aStr);
    rowClues = parsePipeGroup(bStr);
  } else if (aTag === "R" && bTag === "C") {
    colClues = parsePipeGroup(bStr);
    rowClues = parsePipeGroup(aStr);
  } else {
    throw new ParseError(`clue section missing C: or R: prefix: "${parts[1].slice(0, 20)}"`);
  }

  const width = colClues.length;
  const height = rowClues.length;
  const answerRaw = parts[2]?.trim();
  const puzzle: Puzzle = { name, width, height, rowClues, colClues };
  if (answerRaw) puzzle.answer = answerRaw;

  if (parts.length === 4) {
    const s = parts[3].trim();
    const sol = new Uint8Array(s.length);
    for (let i = 0; i < s.length; i++) {
      if (s[i] === "1") sol[i] = FILLED;
      else if (s[i] === "0") sol[i] = EMPTY;
      else throw new ParseError(`invalid clue number: ${s[i]}`);
    }
    if (sol.length !== width * height) {
      throw new ParseError(`solution length ${sol.length} does not match grid ${width * height}`);
    }
    puzzle.solution = sol;
  }
  return puzzle;
}

/** Puz-Pre v3 (`pzprv3`) nonogram file. Clue tokens may carry a `c` prefix. */
export function parsePzprv3(text: string): Puzzle {
  const lines = text.split(/\r?\n/);
  if ((lines[0] ?? "").trim() !== "pzprv3") throw new ParseError(`Expected 'pzprv3', got '${(lines[0] ?? "").trim()}'`);
  if ((lines[1] ?? "").trim() !== "nonogram") throw new ParseError(`Expected 'nonogram', got '${(lines[1] ?? "").trim()}'`);
  const h = Number((lines[2] ?? "").trim());
  const w = Number((lines[3] ?? "").trim());
  if (!Number.isInteger(h) || !Number.isInteger(w) || w <= 0 || h <= 0) throw new ParseError("Invalid dimensions");

  const maxCd = Math.ceil(h / 2);
  const maxRd = Math.ceil(w / 2);
  const totalRows = maxCd + h;
  const totalCols = maxRd + w;

  const grid: string[][] = [];
  for (let i = 0; i < totalRows; i++) {
    const line = lines[4 + i];
    if (line === undefined) throw new ParseError(`Missing row ${i}`);
    const tokens = line.trim().split(/\s+/);
    if (tokens.length !== totalCols) {
      throw new ParseError(`Row ${i}: expected ${totalCols} tokens, got ${tokens.length}`);
    }
    grid.push(tokens);
  }

  const tok = (t: string): number => {
    const s = t.startsWith("c") ? t.slice(1) : t;
    if (!/^\d+$/.test(s)) throw new ParseError(`Invalid clue token '${t}'`);
    return Number(s);
  };

  const colClues: number[][] = [];
  for (let c = 0; c < w; c++) {
    const clue: number[] = [];
    for (let r = 0; r < maxCd; r++) if (grid[r][maxRd + c] !== ".") clue.push(tok(grid[r][maxRd + c]));
    colClues.push(clue);
  }
  const rowClues: number[][] = [];
  for (let r = 0; r < h; r++) {
    const clue: number[] = [];
    for (let c = 0; c < maxRd; c++) if (grid[maxCd + r][c] !== ".") clue.push(tok(grid[maxCd + r][c]));
    rowClues.push(clue);
  }
  return { name: `${w}×${h}`, width: w, height: h, rowClues, colClues };
}

export interface ImportResult {
  puzzles: Puzzle[];
  /** Human-readable problems for lines that could not be parsed. */
  errors: string[];
}

/** Parse a multi-line native-format document (`#` comments and blank lines skipped). */
export function parseNativeText(text: string): ImportResult {
  const puzzles: Puzzle[] = [];
  const errors: string[] = [];
  text.split(/\r?\n/).forEach((raw, i) => {
    const line = raw.trim();
    if (!line || line.startsWith("#")) return;
    try {
      puzzles.push(parseNativeLine(line));
    } catch (e) {
      errors.push(`line ${i + 1}: ${(e as Error).message}`);
    }
  });
  return { puzzles, errors };
}

/**
 * Accepts whatever a user is likely to paste or drop: a puzz.link URL, a link
 * to this app, a Puz-Pre v3 file, or native-format lines.
 */
export function importFromText(input: string): ImportResult {
  const text = input.replace(/^﻿/, "").trim();
  if (!text) return { puzzles: [], errors: ["Nothing to import"] };

  if (text.startsWith("pzprv3")) {
    try {
      return { puzzles: [parsePzprv3(text)], errors: [] };
    } catch (e) {
      return { puzzles: [], errors: [(e as Error).message] };
    }
  }

  // Native lines contain `;C:`/`;R:`; a link contains `W/H/DATA`.
  if (!/;\s*[CR]:/.test(text)) {
    const m = PUZZLINK_RE.exec(text);
    if (m) {
      try {
        const p = decodePuzzlinkParts(Number(m[1]), Number(m[2]), m[3]);
        const name = /[?&]name=([^&#]*)/.exec(text);
        if (name) p.name = safeDecode(name[1]) || p.name;
        const answer = /[?&]answer=([^&#]*)/.exec(text);
        if (answer) p.answer = safeDecode(answer[1]) || undefined;
        return { puzzles: [p], errors: [] };
      } catch (e) {
        return { puzzles: [], errors: [(e as Error).message] };
      }
    }
  }

  const res = parseNativeText(text);
  if (!res.puzzles.length && !res.errors.length) res.errors.push("No puzzles found");
  return res;
}

export function safeDecode(s: string): string {
  try {
    return decodeURIComponent(s.replace(/\+/g, " "));
  } catch {
    return s;
  }
}
