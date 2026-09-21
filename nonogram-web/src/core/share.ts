import { importFromText, parseNativeLine, safeDecode } from "./parse";
import { encodePuzzlinkData } from "./puzzlink";
import type { Puzzle } from "./types";

/**
 * Shareable app links carry the puzzle in the URL fragment, so no server is
 * involved and GitHub Pages can serve them as-is:
 *
 *   #W/H/DATA?name=Cat&answer=Cat%20in%20a%20hat
 *
 * `DATA` is the puzz.link clue encoding (compact; a 50×50 fits in ~1 KB).
 * Also accepted on load:
 *   #nonogram/W/H/DATA              the first version of the link format
 *   #s=<url-encoded native line>    full fidelity (solution etc.)
 *   ?nonogram/W/H/DATA              a puzz.link URL with the host swapped
 */

export function buildShareLink(base: string, p: Puzzle, opts: { includeAnswer?: boolean } = {}): string {
  const params: string[] = [];
  if (p.name && p.name !== `${p.width}×${p.height}`) params.push(`name=${encodeURIComponent(p.name)}`);
  if (opts.includeAnswer && p.answer) params.push(`answer=${encodeURIComponent(p.answer)}`);
  const q = params.length ? `?${params.join("&")}` : "";
  return `${base}#${p.width}/${p.height}/${encodePuzzlinkData(p)}${q}`;
}

/** Hash for the address bar (the current puzzle, never including the answer). */
export function puzzleHash(p: Puzzle): string {
  return buildShareLink("", p);
}

export type LocationPuzzle =
  | { kind: "none" }
  | { kind: "puzzle"; puzzle: Puzzle }
  | { kind: "error"; message: string };

export function puzzleFromLocation(loc: { hash: string; search: string }): LocationPuzzle {
  const hash = loc.hash.replace(/^#/, "");
  const search = loc.search.replace(/^\?/, "");

  try {
    if (hash.startsWith("s=")) {
      return { kind: "puzzle", puzzle: parseNativeLine(safeDecode(hash.slice(2))) };
    }
    // `importFromText` reads the `nonogram/W/H/DATA` form, so give it that.
    const source = /^\d+\/\d+\//.test(hash)
      ? `nonogram/${hash}`
      : hash.startsWith("nonogram/")
        ? hash
        : search.startsWith("nonogram/")
          ? search
          : null;
    if (source) {
      const res = importFromText(source);
      if (res.puzzles.length) return { kind: "puzzle", puzzle: res.puzzles[0] };
      return { kind: "error", message: res.errors[0] ?? "Could not read puzzle from link" };
    }
  } catch (e) {
    return { kind: "error", message: (e as Error).message };
  }
  return { kind: "none" };
}
