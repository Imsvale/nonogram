import type { Pos } from "../state/game";

/**
 * A rectangle of the Selection tool's marquee — cell indices, inclusive both ends, normalized
 * (r0<=r1, c0<=c1). Purely a view concern: it doesn't care what's in the cells and is never saved
 * with the puzzle's progress.
 */
export interface SelRect {
  r0: number;
  c0: number;
  r1: number;
  c1: number;
}

export interface SelectionState {
  rects: readonly SelRect[];
  /** Index into `rects` that Shift (or Ctrl+Shift) extends; -1 = nothing to extend yet. */
  active: number;
  /** The fixed corner the active rectangle grows from. */
  anchor: Pos | null;
}

export const EMPTY_SELECTION: SelectionState = { rects: [], active: -1, anchor: null };

function normalizeSel(a: Pos, b: Pos): SelRect {
  return { r0: Math.min(a[0], b[0]), c0: Math.min(a[1], b[1]), r1: Math.max(a[0], b[0]), c1: Math.max(a[1], b[1]) };
}

/**
 * The ubiquitous rectangle-selection convention (Explorer, spreadsheets), applied at the moment a
 * new press lands on a cell:
 *
 * - Plain: replace the whole selection with one new rectangle anchored here.
 * - Shift: extend the active rectangle from its own anchor to here — and collapse any other
 *   Ctrl-added rectangles back down to just this one, same as a plain click would.
 * - Ctrl: add a new, independent rectangle anchored here, which becomes the active one; every
 *   existing rectangle is left exactly as it was.
 * - Ctrl+Shift: extend the active rectangle in place, keeping every other rectangle untouched.
 */
export function selectPress(state: SelectionState, cell: Pos, mods: { shift: boolean; ctrl: boolean }): SelectionState {
  const canExtend = mods.shift && state.active >= 0 && state.anchor !== null;
  if (canExtend && mods.ctrl) {
    const rects = state.rects.slice();
    rects[state.active] = normalizeSel(state.anchor!, cell);
    return { rects, active: state.active, anchor: state.anchor };
  }
  if (canExtend) {
    return { rects: [normalizeSel(state.anchor!, cell)], active: 0, anchor: state.anchor };
  }
  if (mods.ctrl) {
    const rects = [...state.rects, { r0: cell[0], c0: cell[1], r1: cell[0], c1: cell[1] }];
    return { rects, active: rects.length - 1, anchor: cell };
  }
  return { rects: [{ r0: cell[0], c0: cell[1], r1: cell[0], c1: cell[1] }], active: 0, anchor: cell };
}

/** Continuously re-fits the active rectangle to `cell` while the drag that started with
 *  `selectPress` continues — a no-op if there's nothing active (shouldn't normally happen). */
export function selectDrag(state: SelectionState, cell: Pos): SelectionState {
  if (state.active < 0 || !state.anchor) return state;
  const rects = state.rects.slice();
  rects[state.active] = normalizeSel(state.anchor, cell);
  return { ...state, rects };
}
