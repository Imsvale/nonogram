import { describe, expect, it } from "vitest";
import { EMPTY_SELECTION, selectDrag, selectPress, type SelectionState } from "../src/ui/selection";

const press = (state: SelectionState, r: number, c: number, mods: { shift?: boolean; ctrl?: boolean } = {}) =>
  selectPress(state, [r, c], { shift: !!mods.shift, ctrl: !!mods.ctrl });

describe("selection (the Selection tool's Shift/Ctrl/Ctrl+Shift combining logic)", () => {
  it("a plain press starts a fresh single-cell rectangle", () => {
    const s = press(EMPTY_SELECTION, 2, 3);
    expect(s.rects).toEqual([{ r0: 2, c0: 3, r1: 2, c1: 3 }]);
    expect(s.active).toBe(0);
    expect(s.anchor).toEqual([2, 3]);
  });

  it("a later plain press replaces the whole selection, not just the active rectangle", () => {
    let s = press(EMPTY_SELECTION, 0, 0, { ctrl: true }); // an extra rectangle first
    s = press(s, 5, 5, { ctrl: true });
    s = press(s, 9, 9); // plain — wipes everything
    expect(s.rects).toEqual([{ r0: 9, c0: 9, r1: 9, c1: 9 }]);
    expect(s.active).toBe(0);
  });

  it("dragging (selectDrag) grows the active rectangle from its anchor", () => {
    let s = press(EMPTY_SELECTION, 2, 2);
    s = selectDrag(s, [5, 6]);
    expect(s.rects).toEqual([{ r0: 2, c0: 2, r1: 5, c1: 6 }]);
    // Dragging back past the anchor still normalizes correctly.
    s = selectDrag(s, [0, 1]);
    expect(s.rects).toEqual([{ r0: 0, c0: 1, r1: 2, c1: 2 }]);
  });

  it("selectDrag is a no-op when nothing is active yet", () => {
    expect(selectDrag(EMPTY_SELECTION, [3, 3])).toBe(EMPTY_SELECTION);
  });

  it("Shift extends the active rectangle from its original anchor, not from where the drag ended", () => {
    let s = press(EMPTY_SELECTION, 2, 2);
    s = selectDrag(s, [4, 4]); // dragged to (4,4) and released there — release itself presses nothing
    s = press(s, 6, 6, { shift: true }); // Shift click elsewhere
    expect(s.rects).toEqual([{ r0: 2, c0: 2, r1: 6, c1: 6 }]); // extended from (2,2), not (4,4)
  });

  it("Ctrl adds an independent rectangle, leaving earlier ones untouched", () => {
    let s = press(EMPTY_SELECTION, 0, 0);
    s = press(s, 5, 5, { ctrl: true });
    expect(s.rects).toEqual([
      { r0: 0, c0: 0, r1: 0, c1: 0 },
      { r0: 5, c0: 5, r1: 5, c1: 5 },
    ]);
    expect(s.active).toBe(1); // the new one is active for the next Shift
  });

  it("Ctrl+Shift extends the active rectangle in place, keeping every other one exactly as it was", () => {
    let s = press(EMPTY_SELECTION, 0, 0);
    s = press(s, 5, 5, { ctrl: true }); // second, independent, now active
    s = press(s, 7, 8, { ctrl: true, shift: true }); // extend the active one only
    expect(s.rects).toEqual([
      { r0: 0, c0: 0, r1: 0, c1: 0 }, // untouched
      { r0: 5, c0: 5, r1: 7, c1: 8 }, // extended from its own anchor (5,5)
    ]);
    expect(s.active).toBe(1);
  });

  it("plain Shift (no Ctrl) collapses other Ctrl-added rectangles back down to just the active one", () => {
    let s = press(EMPTY_SELECTION, 0, 0);
    s = press(s, 5, 5, { ctrl: true }); // two rectangles now
    s = press(s, 7, 7, { shift: true }); // Shift alone
    expect(s.rects).toEqual([{ r0: 5, c0: 5, r1: 7, c1: 7 }]); // only the extended one survives
    expect(s.active).toBe(0);
  });

  it("Shift or Ctrl+Shift with nothing active yet behaves like a plain press", () => {
    expect(selectPress(EMPTY_SELECTION, [3, 4], { shift: true, ctrl: false })).toEqual(press(EMPTY_SELECTION, 3, 4));
    expect(selectPress(EMPTY_SELECTION, [3, 4], { shift: true, ctrl: true })).toEqual(press(EMPTY_SELECTION, 3, 4, { ctrl: true }));
  });

  it("normalizes so r0<=r1 and c0<=c1 regardless of drag direction", () => {
    const s = press(EMPTY_SELECTION, 8, 9, { ctrl: false });
    const dragged = selectDrag(s, [2, 1]);
    expect(dragged.rects).toEqual([{ r0: 2, c0: 1, r1: 8, c1: 9 }]);
  });
});
