import { beforeEach, describe, expect, it } from "vitest";
import { Game } from "../src/state/game";
import { deleteProgress, resumeCandidate, saveProgress, setLastOpen } from "../src/state/progress";
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
