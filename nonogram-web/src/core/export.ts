import { FILLED, type Grid, type Puzzle } from "./types";

const clean = (s: string) => s.replace(/[;\r\n]+/g, ",").trim();

/** Native single-line format: `name;C:…/R:…[;answer[;solution]]`. */
export function puzzleToNative(p: Puzzle): string {
  const enc = (lines: number[][]) => lines.map((g) => (g.length ? g.join(" ") : "0")).join("|");
  let s = `${clean(p.name)};C:${enc(p.colClues)}/R:${enc(p.rowClues)}`;
  const sol = p.solution ? Array.from(p.solution, (c) => (c === FILLED ? "1" : "0")).join("") : null;
  if (p.answer && sol) s += `;${clean(p.answer)};${sol}`;
  else if (p.answer) s += `;${clean(p.answer)}`;
  else if (sol) s += `;;${sol}`;
  return s;
}

/**
 * Puz-Pre v3 export. Column clues bottom-aligned, row clues right-aligned,
 * `#` for filled cells, `c` prefix on clues of fulfilled lines.
 */
export function puzzleToPzprv3(
  p: Puzzle,
  grid: Grid | null,
  fulfilledRows: readonly boolean[],
  fulfilledCols: readonly boolean[],
): string {
  const { width: w, height: h } = p;
  const maxCd = Math.ceil(h / 2);
  const maxRd = Math.ceil(w / 2);
  const out: string[] = ["pzprv3", "nonogram", String(h), String(w)];
  for (let ri = 0; ri < maxCd + h; ri++) {
    const tokens: string[] = [];
    for (let ci = 0; ci < maxRd + w; ci++) {
      if (ri < maxCd) {
        if (ci < maxRd) {
          tokens.push(".");
        } else {
          const c = ci - maxRd;
          const clues = p.colClues[c];
          const pad = maxCd - clues.length;
          tokens.push(ri >= pad ? (fulfilledCols[c] ? "c" : "") + clues[ri - pad] : ".");
        }
      } else {
        const r = ri - maxCd;
        if (ci < maxRd) {
          const clues = p.rowClues[r];
          const pad = maxRd - clues.length;
          tokens.push(ci >= pad ? (fulfilledRows[r] ? "c" : "") + clues[ci - pad] : ".");
        } else {
          const c = ci - maxRd;
          tokens.push(grid && grid[r * w + c] === FILLED ? "#" : ".");
        }
      }
    }
    out.push(tokens.join(" "));
  }
  return out.join("\n") + "\n";
}
