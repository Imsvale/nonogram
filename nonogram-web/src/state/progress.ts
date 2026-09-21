import { puzzleToNative } from "../core/export";
import { parseNativeLine } from "../core/parse";
import { EMPTY, FILLED, UNKNOWN, type Grid, type Puzzle } from "../core/types";

/**
 * Per-puzzle progress in localStorage, keyed by a hash of the clues (so the
 * same puzzle reached via a link, a file or puzz.link shares one save).
 * Bounded to the most recent MAX_ENTRIES puzzles.
 */

const KEY = "nonogram-web:progress:v1";
const MAX_ENTRIES = 40;

export interface ProgressEntry {
  id: string;
  /** Native-format line so "recent puzzles" can reopen without the link. */
  spec: string;
  name: string;
  width: number;
  height: number;
  grid: string;
  trial: { snap: string; origin: [number, number] | null }[];
  dimRows: string[];
  dimCols: string[];
  elapsedMs: number;
  everSolved: boolean;
  updated: number;
}

export function puzzleId(p: Puzzle): string {
  const s = `${p.width}x${p.height}|${p.colClues.map((c) => c.join(".")).join(",")}|${p.rowClues.map((c) => c.join(".")).join(",")}`;
  let h1 = 0x811c9dc5;
  let h2 = 0x9747b28c;
  for (let i = 0; i < s.length; i++) {
    const c = s.charCodeAt(i);
    h1 = Math.imul(h1 ^ c, 0x01000193) >>> 0;
    h2 = Math.imul(h2 ^ c, 0x85ebca6b) >>> 0;
  }
  return h1.toString(16).padStart(8, "0") + h2.toString(16).padStart(8, "0");
}

export function gridToString(g: Grid): string {
  let s = "";
  for (let i = 0; i < g.length; i++) s += g[i] === FILLED ? "F" : g[i] === EMPTY ? "E" : "U";
  return s;
}

export function stringToGrid(s: string, size: number): Grid {
  const g = new Uint8Array(size);
  for (let i = 0; i < Math.min(size, s.length); i++) g[i] = s[i] === "F" ? FILLED : s[i] === "E" ? EMPTY : UNKNOWN;
  return g;
}

function readAll(): Record<string, ProgressEntry> {
  try {
    const raw = localStorage.getItem(KEY);
    if (!raw) return {};
    const parsed: unknown = JSON.parse(raw);
    return typeof parsed === "object" && parsed !== null ? (parsed as Record<string, ProgressEntry>) : {};
  } catch {
    return {};
  }
}

function writeAll(all: Record<string, ProgressEntry>): void {
  const ids = Object.keys(all);
  if (ids.length > MAX_ENTRIES) {
    ids.sort((a, b) => all[b].updated - all[a].updated);
    for (const id of ids.slice(MAX_ENTRIES)) delete all[id];
  }
  try {
    localStorage.setItem(KEY, JSON.stringify(all));
  } catch {
    /* quota / private mode: progress just isn't saved */
  }
}

export function loadProgress(p: Puzzle): ProgressEntry | null {
  const e = readAll()[puzzleId(p)];
  if (!e || e.width !== p.width || e.height !== p.height) return null;
  return e;
}

export function saveProgress(entry: ProgressEntry): void {
  const all = readAll();
  all[entry.id] = entry;
  writeAll(all);
}

export function deleteProgress(id: string): void {
  const all = readAll();
  delete all[id];
  writeAll(all);
}

export function listRecent(): { entry: ProgressEntry; puzzle: Puzzle }[] {
  const out: { entry: ProgressEntry; puzzle: Puzzle }[] = [];
  for (const entry of Object.values(readAll())) {
    try {
      out.push({ entry, puzzle: parseNativeLine(entry.spec) });
    } catch {
      /* skip corrupt entries */
    }
  }
  return out.sort((a, b) => b.entry.updated - a.entry.updated);
}

/** Serialise the spec, dropping the solution (it is spoiler-sized and unused here). */
export function specFor(p: Puzzle): string {
  return puzzleToNative({ ...p, solution: undefined });
}

/** Share of cells that are no longer Unknown, for the recent-puzzles list. */
export function progressFraction(e: ProgressEntry): number {
  let n = 0;
  for (const ch of e.grid) if (ch !== "U") n++;
  return e.grid.length ? n / e.grid.length : 0;
}

// ── Resume ──────────────────────────────────────────────────────────────────

const LAST_KEY = "nonogram-web:last:v1";

/** Remember which puzzle was open most recently (for the start page's "Resume"). */
export function setLastOpen(id: string | null): void {
  try {
    if (id) localStorage.setItem(LAST_KEY, id);
    else localStorage.removeItem(LAST_KEY);
  } catch {
    /* ignore */
  }
}

/**
 * The puzzle that was open last time, if it is worth resuming: it has some
 * progress (marked cells or elapsed time) and has never been solved.
 */
export function resumeCandidate(): { entry: ProgressEntry; puzzle: Puzzle } | null {
  let id: string | null = null;
  try {
    id = localStorage.getItem(LAST_KEY);
  } catch {
    return null;
  }
  if (!id) return null;
  const e = readAll()[id];
  if (!e || e.everSolved) return null;
  if (!/[FE]/.test(e.grid) && !(e.elapsedMs > 0)) return null;
  try {
    return { entry: e, puzzle: parseNativeLine(e.spec) };
  } catch {
    return null;
  }
}
