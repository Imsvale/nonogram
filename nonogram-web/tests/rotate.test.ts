import { describe, expect, it } from "vitest";
import {
  realDimAddress,
  rotateCCW,
  rotateCW,
  toReal,
  toView,
  viewDerived,
  viewDimSets,
  viewGrid,
  viewPuzzle,
  type Rotation,
} from "../src/ui/rotate";
import type { Puzzle } from "../src/core/types";
import type { Derived } from "../src/state/game";

// Deliberately asymmetric (3 wide, 2 tall; non-palindromic clues) so a row/col swap or a missed
// reversal shows up as a mismatch rather than accidentally canceling out.
const p: Puzzle = {
  name: "3×2",
  width: 3,
  height: 2,
  rowClues: [[2, 1], [1]],
  colClues: [[1], [2], [1, 2]],
};

const ROTS: Rotation[] = [0, 90, 180, 270];

describe("toView / toReal", () => {
  it("round-trips every cell, every rotation", () => {
    for (const rot of ROTS) {
      for (let r = 0; r < p.height; r++) {
        for (let c = 0; c < p.width; c++) {
          const [vr, vc] = toView(rot, p.width, p.height, r, c);
          expect(toReal(rot, p.width, p.height, vr, vc)).toEqual([r, c]);
        }
      }
    }
  });

  it("rotateCW/rotateCCW are inverses and cycle through all four", () => {
    let r: Rotation = 0;
    for (let i = 0; i < 4; i++) r = rotateCW(r);
    expect(r).toBe(0);
    expect(rotateCCW(rotateCW(90))).toBe(90);
  });
});

describe("viewPuzzle", () => {
  it("rot=0 is the identity", () => {
    expect(viewPuzzle(p, 0)).toEqual(p);
  });

  it("rot=90 (CW): dims swap, rows come from columns reversed", () => {
    const v = viewPuzzle(p, 90);
    expect([v.width, v.height]).toEqual([2, 3]);
    expect(v.rowClues).toEqual([[1], [2], [2, 1]]);
    expect(v.colClues).toEqual([[1], [2, 1]]);
  });

  it("rot=180: dims unchanged, every line reordered and reversed", () => {
    const v = viewPuzzle(p, 180);
    expect([v.width, v.height]).toEqual([3, 2]);
    expect(v.rowClues).toEqual([[1], [1, 2]]);
    expect(v.colClues).toEqual([[2, 1], [2], [1]]);
  });

  it("rot=270 (CCW): dims swap, cols come from rows reversed", () => {
    const v = viewPuzzle(p, 270);
    expect([v.width, v.height]).toEqual([2, 3]);
    expect(v.rowClues).toEqual([[1, 2], [2], [1]]);
    expect(v.colClues).toEqual([[1, 2], [1]]);
  });
});

describe("viewGrid", () => {
  // row0: FILLED, EMPTY, UNKNOWN; row1: UNKNOWN, FILLED, EMPTY
  const grid = Uint8Array.from([1, 2, 0, 0, 1, 2]);

  it("rot=90 matches a hand-worked transposition", () => {
    const v = viewGrid(grid, p.width, p.height, 90);
    expect(Array.from(v)).toEqual([0, 1, 1, 2, 2, 0]);
  });
});

describe("viewDerived", () => {
  const d: Derived = {
    rowFulfilled: [true, false],
    colFulfilled: [false, true, true],
    rowIndiv: [[true, false], [false]],
    colIndiv: [[true], [false], [true, false]],
  };

  it("rot=90: fulfilled scalars reorder, indiv arrays reverse like clues", () => {
    const v = viewDerived(d, p, 90);
    expect(v.rowFulfilled).toEqual(d.colFulfilled); // reordered only, no reversal for scalars
    expect(v.colFulfilled).toEqual([...d.rowFulfilled].reverse());
    expect(v.rowIndiv).toEqual([[true], [false], [false, true]]);
    expect(v.colIndiv).toEqual([[false], [true, false]]);
  });
});

describe("dim sets round-trip through rotation", () => {
  it("a real dim mark survives a full view/real round trip, every rotation", () => {
    for (const rot of ROTS) {
      // Dim column 2's clue index 1 ("2" in colClues[2] = [1,2]).
      const dimCols = new Set(["2,1"]);
      const dimRows = new Set<string>();
      const { dimRows: vr, dimCols: vc } = viewDimSets(dimRows, dimCols, p, rot);
      // Whichever single view entry is set, clicking it must resolve back to (isCol:true, line:2, idx:1).
      const hits: { isCol: boolean; line: number; idx: number }[] = [];
      for (const k of vr) {
        const [line, idx] = k.split(",").map(Number);
        hits.push({ isCol: false, line, idx });
      }
      for (const k of vc) {
        const [line, idx] = k.split(",").map(Number);
        hits.push({ isCol: true, line, idx });
      }
      expect(hits).toHaveLength(1);
      const addr = realDimAddress(rot, p, hits[0].isCol, hits[0].line, hits[0].idx);
      expect(addr).toEqual({ isCol: true, line: 2, idx: 1 });
    }
  });
});
