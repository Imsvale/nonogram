import { obfuscate } from "./obfuscate";
import { importFromText } from "./parse";
import { encodePuzzlinkData } from "./puzzlink";
import type { Puzzle } from "./types";

/**
 * Shareable app links carry the puzzle in the URL fragment, so no server is
 * involved and GitHub Pages can serve them as-is:
 *
 *   #W/H/DATA?name=Cat&a=<scrambled answer>
 *
 * `DATA` is the puzz.link clue encoding (compact; a 50×50 fits in ~1 KB).
 * `a` is the spoiler title, scrambled (see obfuscate.ts) so it can't be read in the
 * address bar; the app only reveals it once the puzzle is solved. The plain
 * `&answer=Cat` form is still read, for older links.
 * Also accepted on load:
 *   #nonogram/W/H/DATA              the first version of the link format
 *   ?nonogram/W/H/DATA              a puzz.link URL with the host swapped
 */

export function buildShareLink(base: string, p: Puzzle, opts: { includeAnswer?: boolean } = {}): string {
  const params: string[] = [];
  if (p.name && p.name !== `${p.width}×${p.height}`) params.push(`name=${encodeURIComponent(p.name)}`);
  const data = encodePuzzlinkData(p);
  if (opts.includeAnswer && p.answer) params.push(`a=${obfuscate(p.answer, `${p.width}/${p.height}/${data}`)}`);
  const q = params.length ? `?${params.join("&")}` : "";
  return `${base}#${p.width}/${p.height}/${data}${q}`;
}

/**
 * Hash for the address bar. Includes the (scrambled) answer when the puzzle has
 * one, so a reload doesn't lose it.
 */
export function puzzleHash(p: Puzzle): string {
  return buildShareLink("", p, { includeAnswer: true });
}

export type LocationPuzzle =
  | { kind: "none" }
  | { kind: "puzzle"; puzzle: Puzzle }
  | { kind: "error"; message: string };

export function puzzleFromLocation(loc: { hash: string; search: string }): LocationPuzzle {
  const hash = loc.hash.replace(/^#/, "");
  const search = loc.search.replace(/^\?/, "");

  try {
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
