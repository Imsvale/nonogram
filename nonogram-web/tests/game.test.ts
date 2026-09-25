import { describe, expect, it } from "vitest";
import { axisLine, Game, lineCells, type Pos } from "../src/state/game";
import { EMPTY, FILLED, UNKNOWN, type Puzzle } from "../src/core/types";

const plus: Puzzle = {
  name: "plus",
  width: 3,
  height: 3,
  colClues: [[1], [3], [1]],
  rowClues: [[1], [3], [1]],
};

const state = (g: Game) => Array.from(g.grid).join("");
const stroke = (g: Game, cells: Pos[], action: "fill" | "mark" | "cycle" = "fill", axisLock = false) => {
  g.startStroke(cells[0][0], cells[0][1], action, axisLock);
  for (const [r, c] of cells.slice(1)) g.strokeTo(r, c);
  return g.endStroke();
};

describe("line helpers", () => {
  it("lineCells is gap-free in both directions and includes endpoints", () => {
    expect(lineCells([0, 0], [0, 3])).toEqual([[0, 0], [0, 1], [0, 2], [0, 3]]);
    expect(lineCells([2, 2], [0, 0])).toEqual([[2, 2], [1, 1], [0, 0]]);
    const l = lineCells([0, 0], [2, 7]);
    expect(l[0]).toEqual([0, 0]);
    expect(l[l.length - 1]).toEqual([2, 7]);
    for (let i = 1; i < l.length; i++) {
      expect(Math.abs(l[i][0] - l[i - 1][0])).toBeLessThanOrEqual(1);
      expect(Math.abs(l[i][1] - l[i - 1][1])).toBeLessThanOrEqual(1);
    }
  });

  it("axisLine picks the dominant axis, ties horizontal", () => {
    expect(axisLine([1, 1], [1, 4])).toEqual([[1, 1], [1, 2], [1, 3], [1, 4]]);
    expect(axisLine([1, 1], [3, 2])).toEqual([[1, 1], [2, 1], [3, 1]]);
    expect(axisLine([2, 2], [3, 3])).toEqual([[2, 2], [2, 3]]);
    expect(axisLine([2, 2], [2, 0])).toEqual([[2, 0], [2, 1], [2, 2]]);
  });
});

describe("painting", () => {
  it("left toggles Filled, right toggles Empty; drag repeats the first target", () => {
    const g = new Game(plus);
    stroke(g, [[0, 0]]);
    expect(state(g)).toBe("100000000");
    stroke(g, [[0, 0]]);
    expect(state(g)).toBe("000000000");
    stroke(g, [[1, 0], [1, 1], [1, 2]]);
    expect(state(g)).toBe("000111000");
    stroke(g, [[1, 1]], "mark");
    expect(state(g)).toBe("000121000");
    // Dragging from a filled cell erases along the way.
    stroke(g, [[1, 0], [1, 1], [1, 2]]);
    expect(state(g)).toBe("000000000");
  });

  it("cycle (touch taps) steps unknown → filled → crossed → unknown", () => {
    const g = new Game(plus);
    stroke(g, [[0, 0]], "cycle");
    expect(state(g)).toBe("100000000");
    stroke(g, [[0, 0]], "cycle");
    expect(state(g)).toBe("200000000");
    stroke(g, [[0, 0]], "cycle");
    expect(state(g)).toBe("000000000");
    // A drag applies the first cell's next state along the whole stroke.
    stroke(g, [[1, 0], [1, 1], [1, 2]], "cycle");
    expect(state(g)).toBe("000111000");
  });

  it("a whole drag is a single undo step", () => {
    const g = new Game(plus);
    stroke(g, [[0, 0], [0, 1], [0, 2]]);
    expect(g.undoStack).toHaveLength(1);
    g.undo();
    expect(state(g)).toBe("000000000");
    expect(g.canRedo).toBe(true);
    g.redo();
    expect(state(g)).toBe("111000000");
    // A new stroke drops the redo history.
    g.undo();
    stroke(g, [[2, 2]]);
    expect(g.canRedo).toBe(false);
  });

  it("does not record an undo step for a stroke that changed nothing", () => {
    const g = new Game(plus);
    g.startStroke(0, 0, "fill", false);
    g.cancelStroke();
    expect(state(g)).toBe("000000000");
    expect(g.undoStack).toHaveLength(0);
  });

  it("a fast drag that jumps cells is interpolated", () => {
    const g = new Game({ ...plus, width: 6, colClues: Array(6).fill([]) });
    g.startStroke(0, 0, "fill", false);
    g.strokeTo(0, 5);
    g.endStroke();
    expect(Array.from(g.grid.slice(0, 6))).toEqual([1, 1, 1, 1, 1, 1]);
  });

  it("axis lock defers the write to release and keeps the line straight", () => {
    const g = new Game({ ...plus, width: 5, height: 5, colClues: Array(5).fill([]), rowClues: Array(5).fill([]) });
    g.startStroke(2, 1, "fill", true);
    g.strokeTo(2, 4);
    g.strokeTo(3, 4); // still more horizontal than vertical → stays on row 2
    expect(g.grid.every((c) => c === UNKNOWN)).toBe(true);
    const shown = g.displayGrid();
    expect(Array.from(shown.slice(10, 15))).toEqual([0, 1, 1, 1, 1]);
    expect(shown[3 * 5 + 4]).toBe(UNKNOWN);
    g.endStroke();
    expect(Array.from(g.grid.slice(10, 15))).toEqual([0, 1, 1, 1, 1]);
    expect(g.undoStack).toHaveLength(1);
  });

  it("cancelStroke restores the pre-stroke grid (multi-touch)", () => {
    const g = new Game(plus);
    stroke(g, [[0, 0]]);
    g.startStroke(1, 0, "fill", false);
    g.strokeTo(1, 2);
    g.cancelStroke();
    expect(state(g)).toBe("100000000");
    expect(g.stroke).toBeNull();
  });
});

describe("assistance", () => {
  const row: Puzzle = { name: "r", width: 4, height: 1, colClues: [[], [1], [1], []], rowClues: [[2]] };

  it("auto-fill empties the rest of a line once its clues are met", () => {
    const g = new Game(row);
    g.assist.autoFillEmpty = true;
    stroke(g, [[0, 1]]);
    expect(state(g)).toBe("0100"); // row [2] not yet met
    stroke(g, [[0, 2]]);
    expect(state(g)).toBe("2112");
  });

  it("auto-cross fills the gap between edge-confirmed runs once they cover every clue", () => {
    const puzzle: Puzzle = { name: "x", width: 7, height: 1, colClues: Array(7).fill([]), rowClues: [[1, 1]] };
    const g = new Game(puzzle);
    g.assist.autoCrossEdges = true;
    g.grid[1] = EMPTY;
    g.grid[5] = EMPTY;
    stroke(g, [[0, 0]]);
    expect(state(g)).toBe("1200020"); // only one end confirmed so far
    stroke(g, [[0, 6]]);
    expect(state(g)).toBe("1222221");

    const off = new Game(puzzle);
    off.grid[1] = EMPTY;
    off.grid[5] = EMPTY;
    stroke(off, [[0, 0]]);
    stroke(off, [[0, 6]]);
    expect(state(off)).toBe("1200021");
  });

  it("auto-cross also crosses the side of a central run where nothing else can fit", () => {
    // clue [3, 2]: the "2" is delimited and the LAST clue, so once it's placed, its trailing gap
    // (nothing can follow it) crosses too — even though it was never reachable from either edge,
    // and even though the "3" before it is still completely open.
    const puzzle: Puzzle = { name: "e", width: 10, height: 1, colClues: Array(10).fill([]), rowClues: [[3, 2]] };
    const g = new Game(puzzle);
    g.assist.autoCrossEdges = true;
    stroke(g, [[0, 3]], "mark");
    stroke(g, [[0, 6]], "mark");
    stroke(g, [[0, 4], [0, 5]]);
    expect(state(g)).toBe("0002112222");
  });

  it("auto-cross-matched crosses the cell right past a run that already matches its clue", () => {
    const puzzle: Puzzle = { name: "m", width: 5, height: 1, colClues: Array(5).fill([]), rowClues: [[3]] };
    const g = new Game(puzzle);
    g.assist.autoCrossMatched = true;
    stroke(g, [[0, 0], [0, 2]]);
    expect(state(g)).toBe("11120"); // run touches the left edge; the cell past it is crossed
  });

  it("auto-dim guess needs \"Dim fulfilled clues\" too, and then crosses both open ends", () => {
    const puzzle: Puzzle = { name: "g", width: 8, height: 1, colClues: Array(8).fill([]), rowClues: [[2, 5]] };

    const withoutDim = new Game(puzzle);
    withoutDim.assist.autoDimGuess = true; // autoDim left off
    stroke(withoutDim, [[0, 2], [0, 6]]);
    expect(state(withoutDim)).toBe("00111110"); // nothing crossed — the checkbox is a no-op alone

    const withDim = new Game(puzzle);
    withDim.assist.autoDim = true;
    withDim.assist.autoDimGuess = true;
    stroke(withDim, [[0, 2], [0, 6]]);
    expect(state(withDim)).toBe("02111112"); // 5 is the unique largest clue → both open ends crossed
  });

  it("derived().rowIndiv reflects a central anchor once its run is delimited", () => {
    // derived() itself always includes central-run anchoring (see resolveLine); autoDim only
    // gates whether the renderer draws it and whether gridview bothers calling derived() at all.
    const puzzle: Puzzle = { name: "c", width: 10, height: 1, colClues: Array(10).fill([]), rowClues: [[3, 2]] };
    const g = new Game(puzzle);
    // Delimit an isolated "2" in the middle by hand (mark, not fill, for the boundary cells).
    stroke(g, [[0, 3]], "mark");
    stroke(g, [[0, 6]], "mark");
    stroke(g, [[0, 4], [0, 5]]);
    expect(g.derived().rowIndiv[0]).toEqual([false, true]); // the "3" is still wide open; the "2" is anchored
  });
});

describe("solving", () => {
  it("detects a solved grid, pauses the timer, and unsolves on edit", () => {
    const g = new Game(plus);
    g.startTimer();
    expect(g.timerRunning).toBe(true);
    stroke(g, [[0, 1]]);
    stroke(g, [[1, 0], [1, 1], [1, 2]]);
    expect(g.solvedNow).toBe(false);
    stroke(g, [[2, 1]]);
    expect(g.solvedNow).toBe(true);
    expect(g.everSolved).toBe(true);
    expect(g.timerRunning).toBe(false);
    g.undo();
    expect(g.solvedNow).toBe(false);
    expect(g.everSolved).toBe(true);
  });

  it("does not treat an untouched blank puzzle as solved", () => {
    const g = new Game({ name: "blank", width: 2, height: 2, colClues: [[], []], rowClues: [[], []] });
    expect(g.solvedNow).toBe(false);
    stroke(g, [[0, 0]], "mark");
    expect(g.solvedNow).toBe(false);
  });
});

describe("trial mode", () => {
  it("tags cells by the tier they were painted in", () => {
    const g = new Game(plus);
    stroke(g, [[0, 1]]);
    g.enterTrial();
    stroke(g, [[1, 0]]);
    g.enterTrial();
    stroke(g, [[1, 2]], "mark");
    const t = g.tierMap();
    expect(t[1]).toBe(0);
    expect(t[3]).toBe(1);
    expect(t[5]).toBe(2);
    expect(g.trial[0].origin).toEqual([1, 0]);
    expect(g.trial[1].origin).toEqual([1, 2]);
  });

  it("reject restores the tier's snapshot and is itself undoable; accept keeps the work", () => {
    const g = new Game(plus);
    stroke(g, [[0, 1]]);
    g.enterTrial();
    stroke(g, [[1, 0], [1, 1]]);
    g.rejectTrial();
    expect(state(g)).toBe("010000000");
    expect(g.trial).toHaveLength(0);
    g.undo();
    expect(state(g)).toBe("010110000");

    g.rejectTrial(); // nothing open: no-op
    g.redo();
    g.enterTrial();
    stroke(g, [[2, 1]]);
    g.acceptTrial();
    expect(state(g)).toBe("010000010");
    expect(g.trial).toHaveLength(0);
    expect(g.tierMap().every((v) => v === 0)).toBe(true);
  });

  it("undo steps store only the cells a stroke actually touched, not a whole grid", () => {
    const big: Puzzle = { name: "big", width: 20, height: 20, colClues: Array(20).fill([]), rowClues: Array(20).fill([]) };
    const g = new Game(big);
    stroke(g, [[0, 0]]);
    expect(g.undoStack).toHaveLength(1);
    expect(g.undoStack[0]).toHaveLength(1); // one cell, not 400
    expect(g.undoStack[0][0]).toEqual([0, UNKNOWN, FILLED]);
  });

  it("accepting a trial with several strokes is one undo step, not a replay of each", () => {
    const g = new Game(plus);
    g.enterTrial();
    stroke(g, [[0, 0]]);
    stroke(g, [[0, 1]]);
    stroke(g, [[0, 2]]);
    expect(state(g)).toBe("111000000");
    g.acceptTrial();
    expect(g.undoStack).toHaveLength(1); // flattened, not three
    g.undo();
    expect(state(g)).toBe("000000000"); // the whole trial reverts in one call
    g.redo();
    expect(state(g)).toBe("111000000"); // and reapplies in one call too
  });

  it("undo/redo inside an open trial only steps through that trial's own history", () => {
    const g = new Game(plus);
    stroke(g, [[0, 0]]); // pre-trial history
    g.enterTrial();
    stroke(g, [[0, 1]]);
    stroke(g, [[0, 2]]);
    expect(state(g)).toBe("111000000");
    g.undo();
    expect(state(g)).toBe("110000000");
    g.undo();
    expect(state(g)).toBe("100000000"); // back to the trial's own snapshot
    expect(g.canUndo).toBe(false); // the pre-trial stroke is on a different, inaccessible history
    g.undo(); // no-op
    expect(state(g)).toBe("100000000");
    g.redo();
    g.redo();
    expect(state(g)).toBe("111000000");
  });

  it("nested trials each get their own isolated history", () => {
    const g = new Game(plus);
    g.enterTrial();
    stroke(g, [[0, 0]]); // outer tier
    g.enterTrial();
    stroke(g, [[0, 1]]); // inner tier
    expect(state(g)).toBe("110000000");
    g.undo(); // the inner tier's own stroke, not the outer's
    expect(state(g)).toBe("100000000");
    expect(g.canUndo).toBe(false); // inner tier's history is empty; the outer's isn't reachable here
  });
});

describe("persistence", () => {
  it("round-trips grid, trial, dims and elapsed time", () => {
    const g = new Game({ ...plus, answer: "Plus" });
    stroke(g, [[0, 1]]);
    g.enterTrial();
    stroke(g, [[1, 1]], "mark");
    g.toggleDim(true, 1, 0);
    g.toggleDim(false, 2, 0);
    const saved = JSON.parse(JSON.stringify(g.toProgress()));

    const h = new Game(plus);
    h.restore(saved);
    expect(state(h)).toBe(state(g));
    expect(h.trial).toHaveLength(1);
    expect(h.trial[0].origin).toEqual([1, 1]);
    expect(h.dimCols.has("1,0")).toBe(true);
    expect(h.dimRows.has("2,0")).toBe(true);
    expect(h.timerRunning).toBe(false);
    expect(h.grid[4]).toBe(EMPTY);
    expect(saved.spec).toContain("C:1|3|1");
    expect(saved.spec).not.toMatch(/;[01]{9}$/);
    expect(FILLED).toBe(1);
  });

  it("round-trips undo/redo history, including an open trial's own", () => {
    const g = new Game(plus);
    stroke(g, [[0, 0]]);
    stroke(g, [[0, 1]]);
    g.undo(); // one step each side, on the main history
    g.enterTrial();
    stroke(g, [[1, 1]]);
    const saved = JSON.parse(JSON.stringify(g.toProgress()));

    const h = new Game(plus);
    h.restore(saved);
    expect(state(h)).toBe(state(g));
    expect(h.undoStack).toHaveLength(1);
    expect(h.redoStack).toHaveLength(1);
    expect(h.trial).toHaveLength(1);
    expect(h.trial[0].undo).toHaveLength(1);
    expect(h.trial[0].redo).toHaveLength(0);
    // Undoing after restore acts on the still-open trial's own history, same as before saving.
    h.undo();
    expect(state(h)).toBe("100000000");
  });
});

describe("timer", () => {
  it("reset goes back to zero and keeps running if it was running", () => {
    const g = new Game(plus);
    g.startTimer();
    g.resetTimer();
    expect(g.timerRunning).toBe(true);
    expect(g.timerElapsedMs()).toBeLessThan(50);
    g.pauseTimer();
    g.resetTimer();
    expect(g.timerRunning).toBe(false);
    expect(g.timerElapsedMs()).toBe(0);
  });

  it("a listener never sees 'timer stopped but not solved' at the moment of solving", () => {
    const g = new Game(plus);
    g.startTimer();
    stroke(g, [[0, 1]]);
    stroke(g, [[1, 0], [1, 1], [1, 2]]);
    const seen: string[] = [];
    g.subscribe(() => seen.push(`${g.timerRunning ? "running" : "stopped"}/${g.solvedNow ? "solved" : "open"}`));
    stroke(g, [[2, 1]]);
    expect(seen).not.toContain("stopped/open");
    expect(seen).toContain("stopped/solved");
  });
});

describe("restart", () => {
  it("blanks the grid, closes trials, un-dims clues, drops the undo history and zeroes the timer", () => {
    const g = new Game(plus);
    stroke(g, [[0, 1]]);
    g.enterTrial();
    stroke(g, [[1, 0], [1, 1]]);
    g.toggleDim(true, 1, 0);
    g.toggleDim(false, 2, 0);
    g.startTimer();
    g.restart();
    expect(state(g)).toBe("000000000");
    expect(g.trial).toHaveLength(0);
    expect(g.dimCols.size + g.dimRows.size).toBe(0);
    expect(g.timerElapsedMs()).toBeLessThan(50);
    expect(g.timerRunning).toBe(true); // was running, stays running
    expect(g.canUndo).toBe(false);
    expect(g.canRedo).toBe(false);
  });

  it("also drops the redo history", () => {
    const g = new Game(plus);
    stroke(g, [[0, 1]]);
    g.undo();
    expect(g.canRedo).toBe(true);
    g.restart();
    expect(g.canRedo).toBe(false);
    expect(state(g)).toBe("000000000");
  });

  it("keeps a stopped timer stopped and works on an empty grid", () => {
    const g = new Game(plus);
    g.toggleDim(true, 0, 0);
    g.restart();
    expect(g.undoStack).toHaveLength(0);
    expect(g.dimCols.size).toBe(0);
    expect(g.timerRunning).toBe(false);
  });
});
