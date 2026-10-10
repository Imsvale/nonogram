import { beforeEach, describe, expect, it } from "vitest";
import { Game } from "../src/state/game";
import { deleteProgress, progressFraction, resumeCandidate, saveProgress, setLastOpen } from "../src/state/progress";
import type { Puzzle } from "../src/core/types";

// Minimal in-memory localStorage for the node test environment.
const store = new Map<string, string>();
(globalThis as unknown as { localStorage: Storage }).localStorage = {
  getItem: (k: string) => store.get(k) ?? null,
  setItem: (k: string, v: string) => void store.set(k, v),
  removeItem: (k: string) => void store.delete(k),
  clear: () => store.clear(),
  key: () => null,
  length: 0,
} as Storage;

const plus: Puzzle = { name: "plus", width: 3, height: 3, colClues: [[1], [3], [1]], rowClues: [[1], [3], [1]] };

function open(p: Puzzle, paint: (g: Game) => void): Game {
  const g = new Game(p);
  paint(g);
  saveProgress(g.toProgress());
  setLastOpen(g.id);
  return g;
}
const stroke = (g: Game, r: number, c: number) => {
  g.startStroke(r, c, "fill", false);
  g.endStroke();
};

describe("resume candidate", () => {
  beforeEach(() => store.clear());

  it("offers the last-opened puzzle once it has progress", () => {
    open(plus, (g) => stroke(g, 0, 1));
    const c = resumeCandidate();
    expect(c?.puzzle.name).toBe("plus");
    expect(c?.entry.grid[1]).toBe("F");
  });

  it("does not offer an untouched puzzle, a solved one, or a deleted one", () => {
    const g0 = open(plus, () => {});
    expect(resumeCandidate()).toBeNull(); // opened but nothing done

    const g1 = open(plus, (g) => {
      for (const [r, c] of [[0, 1], [1, 0], [1, 1], [1, 2], [2, 1]]) stroke(g, r, c);
    });
    expect(g1.solvedNow).toBe(true);
    expect(resumeCandidate()).toBeNull(); // finished

    const g2 = open({ ...plus, name: "again", rowClues: [[1], [3], [1]] }, () => {});
    expect(g2.id).toBe(g0.id); // same clues → same save slot
    stroke(g2, 0, 1);
    saveProgress({ ...g2.toProgress(), everSolved: false });
    expect(resumeCandidate()).not.toBeNull();
    deleteProgress(g2.id);
    expect(resumeCandidate()).toBeNull();
  });

  it("only considers the most recently opened puzzle", () => {
    const other: Puzzle = { name: "other", width: 2, height: 2, colClues: [[1], [1]], rowClues: [[1], [1]] };
    open(plus, (g) => stroke(g, 0, 1));
    open(other, () => {}); // last opened, but untouched
    expect(resumeCandidate()).toBeNull();
  });
});

describe("progress fraction", () => {
  beforeEach(() => store.clear());

  it("tracks marked cells for an unfinished puzzle", () => {
    const g = open(plus, (game) => stroke(game, 0, 1));
    expect(progressFraction(g.toProgress())).toBeCloseTo(1 / 9);
  });

  it("reads as full once solved, even with cells left Unknown", () => {
    // The plus's four corners never get crossed out here — a clue reads as fulfilled once its
    // runs are placed and bounded, not once every cell is marked (see isPuzzleSolved) — so a
    // completed puzzle can legitimately still have Unknown cells in its saved grid.
    const g = open(plus, (game) => {
      stroke(game, 0, 1);
      stroke(game, 1, 0);
      stroke(game, 1, 1);
      stroke(game, 1, 2);
      stroke(game, 2, 1);
    });
    const entry = g.toProgress();
    expect(entry.everSolved).toBe(true);
    expect(entry.grid).toContain("U"); // corners still blank
    expect(progressFraction(entry)).toBe(1);
  });
});

describe("settings migration: run-length label color", () => {
  it("drops an old hex-string value and falls back to automatic", async () => {
    store.clear();
    store.set(
      "nonogram-web:settings:v1",
      JSON.stringify({ runLength: { labelHColor: "#66b8ff", labelVColor: null } }),
    );
    const { loadSettings } = await import("../src/state/settings");
    const s = loadSettings();
    expect(s.runLength.labelHColor).toBeNull();
    expect(s.runLength.labelVColor).toBeNull();
  });

  it("keeps a valid {hue,sat} value and defaults the contrast threshold", async () => {
    store.clear();
    store.set(
      "nonogram-web:settings:v1",
      JSON.stringify({ runLength: { labelHColor: { hue: 200, sat: 60 } } }),
    );
    const { loadSettings } = await import("../src/state/settings");
    const s = loadSettings();
    expect(s.runLength.labelHColor).toEqual({ hue: 200, sat: 60 });
    expect(s.runLength.labelContrastThreshold).toBe(0.5);
  });
});
