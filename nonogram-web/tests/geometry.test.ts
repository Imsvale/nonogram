import { describe, expect, it } from "vitest";
import { SAMPLES } from "../src/core/samples";
import { computeHoverRuns, computeLayout, fitCellSize, hitTest, hoverLabelVisible, viewportOk } from "../src/ui/geometry";
import { FILLED, UNKNOWN } from "../src/core/types";

const p = SAMPLES[1]; // Heart 10×10

describe("layout & hit testing", () => {
  it("maps points to cells, clues, heads and the minimap", () => {
    const L = computeLayout(p, 30, 900, 700, 0, 0);
    const mid = (r: number, c: number) => [L.ox + (c + 0.5) * 30, L.oy + (r + 0.5) * 30] as const;
    expect(hitTest(L, p, ...mid(3, 4))).toEqual({ kind: "cell", r: 3, c: 4 });
    // Bottom-most slot of a column strip is its LAST clue.
    const last = p.colClues[3].length - 1;
    expect(hitTest(L, p, L.ox + 3 * 30 + 15, L.oy - 2 - 3)).toEqual({ kind: "colClue", c: 3, i: last });
    // Blank space above a short column's clues still identifies the column.
    expect(hitTest(L, p, L.ox + 15, L.originY + 2)).toEqual({ kind: "colHead", c: 0 });
    expect(hitTest(L, p, L.ox - 2 - 3, L.oy + 15)).toEqual({ kind: "rowClue", r: 0, i: p.rowClues[0].length - 1 });
    expect(hitTest(L, p, L.originX + 3, L.originY + 3).kind).toBe("minimap");
    expect(hitTest(L, p, L.ox + L.cw + 2 + 3, L.oy + L.ch + 2 + 3).kind).toBe("sumToggle");
    expect(hitTest(L, p, 1, 1).kind).toBe("none");
  });

  it("clamps pan and honours it in hit tests", () => {
    const L = computeLayout(p, 60, 500, 400, 10_000, 10_000);
    expect(L.panX).toBe(L.fullW - L.cw);
    const h = hitTest(L, p, L.ox + 1, L.oy + 1);
    expect(h).toEqual({ kind: "cell", r: Math.floor(L.panY / 60), c: Math.floor(L.panX / 60) });
  });

  it("fitCellSize fits when it can and viewportOk rejects over-zoom", () => {
    const C = fitCellSize(p, 900, 700);
    const L = computeLayout(p, C, 900, 700, 0, 0);
    expect(L.cw).toBe(L.fullW);
    expect(L.ch).toBe(L.fullH);
    expect(viewportOk(p, 30, 900, 700)).toBe(true);
    expect(viewportOk(p, 96, 150, 150)).toBe(false);
  });
});

describe("hover runs", () => {
  it("finds maximal same-state runs through a cell; Empty has none", () => {
    const w = 5, h = 3;
    const g = new Uint8Array(w * h);
    g.set([FILLED, FILLED, UNKNOWN, FILLED, FILLED], 0);
    g[1 * w + 1] = FILLED;
    const r = computeHoverRuns(g, w, h, 0, 1);
    expect(r.h).toMatchObject({ fixed: 0, start: 0, end: 1, hover: 1, filled: true });
    expect(r.v).toMatchObject({ fixed: 1, start: 0, end: 1, filled: true });
    g[0] = 2;
    expect(computeHoverRuns(g, w, h, 0, 0)).toEqual({ h: null, v: null });
    const u = computeHoverRuns(g, w, h, 2, 0);
    expect(u.h).toMatchObject({ start: 0, end: 4, filled: false });
  });
});

describe("hover run-length label rule", () => {
  it("shows when at least one end is `threshold`+ cells away", () => {
    // Run of 20 (cells 0..19), threshold 2.
    expect(hoverLabelVisible(0, 19, 10, 2)).toBe(true); // far from both ends
    expect(hoverLabelVisible(0, 19, 1, 2)).toBe(true); // near the start, far from the end
    expect(hoverLabelVisible(0, 19, 18, 2)).toBe(true); // near the end, far from the start
    // Short run: both ends are close, so nothing to add.
    expect(hoverLabelVisible(0, 2, 1, 2)).toBe(false);
    expect(hoverLabelVisible(4, 5, 4, 2)).toBe(false);
    // Threshold 0 always shows.
    expect(hoverLabelVisible(3, 3, 3, 0)).toBe(true);
  });
});
