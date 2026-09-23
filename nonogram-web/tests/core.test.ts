import { describe, expect, it } from "vitest";
import { puzzleToNative, puzzleToPzprv3 } from "../src/core/export";
import {
  checkLineFulfilled,
  forcedEmptyBeyondMatchedRuns,
  forcedEmptyFromEdges,
  individuallyFulfilledClues,
  isPuzzleSolved,
  resolveLine,
  trivialInvalidReason,
} from "../src/core/lines";
import { importFromText, parseNativeLine, parseNativeText, parsePzprv3 } from "../src/core/parse";
import { decodePuzzlinkParts, encodePuzzlinkData, parsePuzzlinkUrl, puzzleToPuzzlinkUrl } from "../src/core/puzzlink";
import { deobfuscate, obfuscate } from "../src/core/obfuscate";
import { buildShareLink, puzzleFromLocation, puzzleHash } from "../src/core/share";
import { EMPTY, FILLED, UNKNOWN, type Puzzle } from "../src/core/types";

const cells = (s: string) => Uint8Array.from(s, (ch) => (ch === "#" ? FILLED : ch === "x" ? EMPTY : UNKNOWN));

function mk(w: number, h: number, cols: number[][], rows: number[][]): Puzzle {
  return { name: `${w}×${h}`, width: w, height: h, colClues: cols, rowClues: rows };
}

describe("puzz.link codec", () => {
  it("encodes the spec's multi-digit worked example (Boot 20×15)", () => {
    const cols = Array.from({ length: 20 }, () => [] as number[]);
    cols[1] = [10];
    cols[2] = [13];
    cols[12] = [1, 11];
    cols[13] = [2, 10];
    const rows = Array.from({ length: 15 }, () => [] as number[]);
    rows[10] = [1, 16, 1];
    rows[11] = [17, 2];
    rows[12] = [19];
    const p = mk(20, 15, cols, rows);
    const data = encodePuzzlinkData(p);
    // gCol = 8: [10] → "a"+7 zeros ('m'); gRow = 10.
    expect(data).toContain("am");
    expect(data).toContain("dm");
    expect(data).toContain("b1l");
    expect(data).toContain("a2l");
    expect(data).toContain("1-101m");
    expect(data).toContain("2-11n");
    expect(data).toContain("-13o");
    expect(decodePuzzlinkParts(20, 15, data)).toEqual(p);
  });

  it("uses z for 20 zeros (empty col in 100×100 → zzp)", () => {
    const p = mk(1, 100, [[]], Array.from({ length: 100 }, () => [] as number[]));
    expect(encodePuzzlinkData(p).startsWith("zzp")).toBe(true);
    expect(decodePuzzlinkParts(1, 100, encodePuzzlinkData(p))).toEqual(p);
  });

  it("round-trips random puzzles of assorted shapes", () => {
    let seed = 12345;
    const rnd = () => ((seed = (seed * 1103515245 + 12345) & 0x7fffffff) / 0x7fffffff);
    for (const [w, h] of [[5, 5], [10, 7], [15, 15], [30, 15], [50, 50], [1, 1], [2, 40], [33, 21]]) {
      const grid = Array.from({ length: h }, () => Array.from({ length: w }, () => rnd() < 0.55));
      const clueOf = (line: boolean[]) => {
        const out: number[] = [];
        let run = 0;
        for (const b of line) {
          if (b) run++;
          else if (run) (out.push(run), (run = 0));
        }
        if (run) out.push(run);
        return out;
      };
      const rows = grid.map(clueOf);
      const cols = Array.from({ length: w }, (_, c) => clueOf(grid.map((row) => row[c])));
      const p = mk(w, h, cols, rows);
      expect(parsePuzzlinkUrl(puzzleToPuzzlinkUrl(p))).toEqual(p);
    }
  });

  it("does not swallow a following group's leading 'f' (value 15) after a full group", () => {
    // 2×30: col groups hold 15 slots. Col 1 = 15 ones (full, no padding), col 2 = [15].
    const p = mk(2, 30, [Array(15).fill(1), [15]], Array.from({ length: 30 }, () => [] as number[]));
    expect(decodePuzzlinkParts(2, 30, encodePuzzlinkData(p))).toEqual(p);
  });

  it("still accepts an explicit 'f' marker after a full group", () => {
    // 2×2 → every group has 1 slot. Explicit-f encoding of cols [1],[2] rows [1],[1].
    const p = decodePuzzlinkParts(2, 2, "1f2f1f1f");
    expect(p.colClues).toEqual([[1], [2]]);
    expect(p.rowClues).toEqual([[1], [1]]);
  });

  it("rejects malformed data", () => {
    expect(() => decodePuzzlinkParts(3, 3, "1")).toThrow();
    expect(() => decodePuzzlinkParts(0, 3, "")).toThrow();
    expect(() => parsePuzzlinkUrl("https://example.com")).toThrow();
  });
});

describe("native format", () => {
  it("parses name/clues/answer/solution and tolerates C/R order", () => {
    const p = parseNativeLine("Cat;R:1 1|3/C:1|2|1;A cat;010111");
    expect(p.name).toBe("Cat");
    expect(p.colClues).toEqual([[1], [2], [1]]);
    expect(p.rowClues).toEqual([[1, 1], [3]]);
    expect(p.answer).toBe("A cat");
    expect(Array.from(p.solution!)).toEqual([EMPTY, FILLED, EMPTY, FILLED, FILLED, FILLED]);
  });

  it("treats 0 as an empty line", () => {
    const p = parseNativeLine("t;C:1|0|1/R:1 1|0");
    expect(p.colClues).toEqual([[1], [], [1]]);
    expect(p.rowClues).toEqual([[1, 1], []]);
  });

  it("rejects bad lines but keeps parsing the rest of a file", () => {
    const { puzzles, errors } = parseNativeText("# comment\n\nok;C:1/R:1\nbad line\n;C:1/R:1\nz;C:1/R:1;a;01\n");
    expect(puzzles.map((p) => p.name)).toEqual(["ok"]);
    expect(errors).toHaveLength(3);
  });

  it("round-trips through puzzleToNative", () => {
    const line = "Example;C:1|1|1/R:1|1|1;Diagonal;100010001";
    expect(puzzleToNative(parseNativeLine(line))).toBe(line);
    expect(puzzleToNative(parseNativeLine("x;C:1|0/R:1"))).toBe("x;C:1|0/R:1");
  });
});

describe("Puz-Pre v3", () => {
  const p = mk(3, 3, [[1], [3], [1]], [[1], [3], [1]]);

  it("round-trips clues and marks fulfilled lines / filled cells", () => {
    const grid = cells("x#x###x#x");
    const text = puzzleToPzprv3(p, grid, [true, true, true], [true, true, true]);
    expect(text.split("\n")[0]).toBe("pzprv3");
    expect(text).toContain("c3 #");
    const back = parsePzprv3(text);
    expect(back.colClues).toEqual(p.colClues);
    expect(back.rowClues).toEqual(p.rowClues);
  });

  it("is detected by importFromText", () => {
    const text = puzzleToPzprv3(p, null, [], []);
    expect(importFromText(text).puzzles[0].width).toBe(3);
  });

  it("round-trips the grid as `progress` — the whole point of the \"with progress\" download", () => {
    const grid = cells("x#x###x#x");
    const text = puzzleToPzprv3(p, grid, [false, true, false], [false, true, false]);
    const back = parsePzprv3(text);
    // `.` is ambiguous between marked-empty and still-unknown, so it comes back unknown either way.
    expect(Array.from(back.progress!)).toEqual([UNKNOWN, FILLED, UNKNOWN, FILLED, FILLED, FILLED, UNKNOWN, FILLED, UNKNOWN]);
  });

  it("has no `progress` when nothing was filled (an empty grid, or none passed)", () => {
    expect(parsePzprv3(puzzleToPzprv3(p, null, [], [])).progress).toBeUndefined();
    expect(parsePzprv3(puzzleToPzprv3(p, cells("x x x x x x x x x"), [], [])).progress).toBeUndefined();
  });
});

describe("share links", () => {
  const p = { ...mk(3, 3, [[1], [3], [1]], [[1], [3], [1]]), name: "Plus / sign", answer: "A cross" };

  it("round-trips name (and optionally answer) through the URL hash", () => {
    const link = buildShareLink("https://imsvale.github.io/nonogram/", p);
    expect(link).toContain("#3/3/");
    expect(link).not.toContain("nonogram/3/3");
    expect(link).not.toContain("answer");
    const u = new URL(link);
    const res = puzzleFromLocation({ hash: u.hash, search: u.search });
    expect(res.kind === "puzzle" && res.puzzle.name).toBe("Plus / sign");
    expect(res.kind === "puzzle" && res.puzzle.colClues).toEqual(p.colClues);

    const withAns = new URL(buildShareLink("https://x/", p, { includeAnswer: true }));
    const res2 = puzzleFromLocation({ hash: withAns.hash, search: "" });
    expect(res2.kind === "puzzle" && res2.puzzle.answer).toBe("A cross");
  });

  it("accepts a puzz.link-style query; unknown fragments are not puzzles", () => {
    const data = encodePuzzlinkData(p);
    const a = puzzleFromLocation({ hash: "", search: `?nonogram/3/3/${data}` });
    expect(a.kind).toBe("puzzle");
    // The former `#s=<native line>` form is gone: it is simply not a puzzle link any more.
    expect(puzzleFromLocation({ hash: "#s=" + encodeURIComponent("t;C:1|1/R:2"), search: "" }).kind).toBe("none");
    expect(puzzleFromLocation({ hash: "", search: "" }).kind).toBe("none");
    expect(puzzleFromLocation({ hash: "#nonogram/3/3/zz", search: "" }).kind).toBe("error");
  });

  it("still reads the first-version link format (#nonogram/W/H/DATA)", () => {
    const data = encodePuzzlinkData(p);
    const legacy = puzzleFromLocation({ hash: `#nonogram/3/3/${data}?name=Old`, search: "" });
    expect(legacy.kind === "puzzle" && legacy.puzzle.name).toBe("Old");
    const now = puzzleFromLocation({ hash: `#3/3/${data}`, search: "" });
    expect(now.kind === "puzzle" && now.puzzle.colClues).toEqual(p.colClues);
    // Bare and pasted forms.
    expect(importFromText(`3/3/${data}`).puzzles[0].rowClues).toEqual(p.rowClues);
    expect(importFromText(`https://imsvale.github.io/nonogram/#3/3/${data}`).puzzles[0].width).toBe(3);
    // A native line is never mistaken for a link, even if its name looks like one.
    const native = importFromText("3/3/x;C:1|1/R:2").puzzles[0];
    expect([native.name, native.width]).toEqual(["3/3/x", 2]);
  });

  it("imports a pasted share link or puzz.link URL", () => {
    const link = buildShareLink("https://imsvale.github.io/nonogram/", p);
    expect(importFromText(link).puzzles[0].name).toBe("Plus / sign");
    expect(importFromText(puzzleToPuzzlinkUrl(p)).puzzles[0].colClues).toEqual(p.colClues);
  });
});

describe("line logic", () => {
  it("checkLineFulfilled compares filled runs to clues", () => {
    expect(checkLineFulfilled([2, 1], cells("##x#x"))).toBe(true);
    expect(checkLineFulfilled([2, 1], cells("##.#."))).toBe(true); // unknown counts as not filled
    expect(checkLineFulfilled([2, 1], cells("###.#"))).toBe(false);
    expect(checkLineFulfilled([], cells("xxx"))).toBe(true);
    expect(checkLineFulfilled([1], cells("xxx"))).toBe(false);
    expect(checkLineFulfilled([3], cells("###"))).toBe(true);
  });

  it("isPuzzleSolved checks every row and column", () => {
    const p = mk(3, 3, [[1], [3], [1]], [[1], [3], [1]]);
    expect(isPuzzleSolved(p, cells("x#x###x#x"))).toBe(true);
    expect(isPuzzleSolved(p, cells("#x#x#x#x#"))).toBe(false);
  });

  it("individuallyFulfilledClues needs a confirmed outer boundary", () => {
    expect(individuallyFulfilledClues([2, 1], cells("##...."))).toEqual([true, false]);
    expect(individuallyFulfilledClues([2, 1], cells(".##..."))).toEqual([false, false]);
    expect(individuallyFulfilledClues([2, 1], cells("x##..."))).toEqual([true, false]);
    expect(individuallyFulfilledClues([2, 1], cells("....#x"))).toEqual([false, true]);
    // A lone 1-run against the far edge is confirmed on both sides.
    expect(individuallyFulfilledClues([2, 1], cells("##x..#"))).toEqual([true, true]);
  });

  it("forcedEmptyFromEdges finds cells between confirmed runs", () => {
    // Run touches an Unknown → it might still grow, so it isn't confirmed.
    expect(forcedEmptyFromEdges([3], cells("###...."))).toEqual([]);
    expect(forcedEmptyFromEdges([2, 1], cells("......#x"))).toEqual([]);
    // Left confirmed runs cover every clue → the tail is forced empty.
    expect(forcedEmptyFromEdges([3], cells("###x..."))).toEqual([4, 5, 6]);
    // Right confirmed runs cover every clue → the head is forced empty.
    expect(forcedEmptyFromEdges([1], cells("...x#"))).toEqual([0, 1, 2]);
    // Middle gap between two confirmed runs covering all clues.
    expect(forcedEmptyFromEdges([1, 1], cells("#x...x#"))).toEqual([2, 3, 4]);
    // …but not while a clue is unaccounted for.
    expect(forcedEmptyFromEdges([1, 1, 1], cells("#x...x#"))).toEqual([]);
    // A blank line is entirely empty.
    expect(forcedEmptyFromEdges([], cells("..#.."))).toEqual([0, 1, 3, 4]);
  });

  it("forcedEmptyBeyondMatchedRuns crosses just past a run that already matches its clue", () => {
    // Confirmed only from the left; the cell right past the matched run is forced empty even
    // though the run's far side is still open (unlike forcedEmptyFromEdges, which needs both).
    expect(forcedEmptyBeyondMatchedRuns([3], cells("###....."))).toEqual([3]);
    expect(forcedEmptyFromEdges([3], cells("###....."))).toEqual([]); // the weaker deduction doesn't fire here
    // Same, from the right.
    expect(forcedEmptyBeyondMatchedRuns([1], cells(".....#"))).toEqual([4]);
    // A run one cell too short (or long) isn't a match yet — nothing to cross.
    expect(forcedEmptyBeyondMatchedRuns([3], cells("##......"))).toEqual([]);
    // Second clue's run matches too, once the first is confirmed and out of the way.
    expect(forcedEmptyBeyondMatchedRuns([1, 2], cells("#x##....."))).toEqual([4]);
    // Nothing to add once the cell past the run is already known (edge, or already marked).
    expect(forcedEmptyBeyondMatchedRuns([3], cells("###"))).toEqual([]);
    expect(forcedEmptyBeyondMatchedRuns([3], cells("###x"))).toEqual([]);
    // A blank line (no clues) has no runs to match, so nothing to cross.
    expect(forcedEmptyBeyondMatchedRuns([], cells("..#.."))).toEqual([]);
  });

  describe("resolveLine (central-run anchoring, on top of the plain edge scan)", () => {
    it("matches individuallyFulfilledClues exactly when nothing central applies", () => {
      // Same cases as the individuallyFulfilledClues test above, unchanged by resolveLine.
      expect(resolveLine([2, 1], cells("##....")).dim).toEqual([true, false]);
      expect(resolveLine([2, 1], cells(".##...")).dim).toEqual([false, false]);
      expect(resolveLine([2, 1], cells("x##...")).dim).toEqual([true, false]);
      expect(resolveLine([2, 1], cells("....#x")).dim).toEqual([false, true]);
      expect(resolveLine([2, 1], cells("##x..#")).dim).toEqual([true, true]);
    });

    it("crossable reproduces forcedEmptyBeyondMatchedRuns for plain edge-reached runs", () => {
      expect(resolveLine([3], cells("###.....")).crossable).toEqual([3]);
      expect(resolveLine([1], cells(".....#")).crossable).toEqual([4]);
      expect(resolveLine([1, 2], cells("#x##.....")).crossable).toEqual([4]);
    });

    it("anchors a delimited central run whose length matches exactly one open clue", () => {
      const { dim, crossable } = resolveLine([3, 2], cells("...x##x..."));
      expect(dim).toEqual([false, true]); // only the "2" is confirmed; "3" is still wide open
      expect(crossable).toEqual([]); // already delimited on both sides — nothing new to cross
    });

    it("does not anchor a delimited central run when its length isn't unique among open clues", () => {
      // Both clues are 2 — a delimited run of length 2 could be either, so neither is confirmed.
      expect(resolveLine([2, 2], cells("...x##x...")).dim).toEqual([false, false]);
    });

    it("chains outward from a central anchor, and resolves duplicate-valued clues once the gap narrows", () => {
      // clue [2, 5, 2]: the "5" is delimited and globally unique, so it anchors in pass 1.
      // The second "2" sits past an Unknown gap (so it can't be reached by ordinary chaining),
      // delimited but NOT globally unique (two clues are "2") — it only resolves once the "5"
      // anchor narrows its gap's remaining clues down to just the one "2", in pass 2.
      const cellStr = "....x#####x.x##x..";
      const { dim, crossable } = resolveLine([2, 5, 2], cells(cellStr));
      expect(dim).toEqual([false, true, true]);
      expect(crossable).toEqual([]); // everything involved is already marked on both sides
      // The leading "2" has no delimited run at all yet — correctly still open.
    });

    it("undelimited runs are ignored unless allowUndelimited is on", () => {
      const line = cells("..#####...");
      expect(resolveLine([2, 5], line).dim).toEqual([false, false]);
      expect(resolveLine([2, 5], line, { allowUndelimited: true }).dim).toEqual([false, true]);
    });

    it("allowUndelimited only anchors the run matching the unique largest remaining clue", () => {
      // Length 5 is the max of [2, 5] → anchors. Length 2 is not the max → never anchors, however
      // long it might eventually turn out to be (it could still be mid-paint).
      expect(resolveLine([2, 5], cells("..#####..."), { allowUndelimited: true }).dim).toEqual([false, true]);
      expect(resolveLine([2, 5], cells("..##......"), { allowUndelimited: true }).dim).toEqual([false, false]);
      // Tied for largest ([3, 3]): which clue this run belongs to is genuinely ambiguous.
      expect(resolveLine([3, 3], cells("...###...."), { allowUndelimited: true }).dim).toEqual([false, false]);
    });

    it("crosses both open ends of an allowUndelimited anchor, and only those, via guessCrossable", () => {
      const { dim, crossable, guessCrossable } = resolveLine([2, 5], cells("..#####..."), { allowUndelimited: true });
      expect(dim).toEqual([false, true]);
      expect(crossable).toEqual([]);
      expect(guessCrossable).toEqual([1, 7]);
    });

    it("un-dimming is implicit: the same run no longer qualifies once it's no longer the largest", () => {
      // A second clue is added that's now bigger than the run — the guess must not fire.
      expect(resolveLine([2, 5], cells("..#####..."), { allowUndelimited: true }).dim).toEqual([false, true]);
      expect(resolveLine([2, 5, 6], cells("..#####......."), { allowUndelimited: true }).dim).toEqual([false, false, false]);
    });
  });

  it("flags structurally impossible puzzles", () => {
    expect(trivialInvalidReason(mk(3, 3, [[1], [1], [1]], [[3], [], []]))).toBeNull();
    expect(trivialInvalidReason(mk(3, 3, [[1], [1], [1]], [[3], [1], []]))).toMatch(/sums don't match/);
    expect(trivialInvalidReason(mk(2, 2, [[1, 1], []], [[2], []]))).toMatch(/col 1 requires at least 3/);
  });
});

describe("answer obfuscation", () => {
  const seed = "3/3/1g3g1g1g3g1g";

  it("round-trips text, including non-ASCII", () => {
    for (const t of ["Cat", "A cat in a hat", "Café ☕ — Ünï", "x"]) {
      expect(deobfuscate(obfuscate(t, seed), seed)).toBe(t);
    }
  });

  it("is not readable at a glance: not the text, not its plain base64, and puzzle-specific", () => {
    const text = "Space invader";
    const o = obfuscate(text, seed);
    expect(o.toLowerCase()).not.toContain("invader");
    expect(o).not.toContain(btoa(text).replace(/=+$/, ""));
    expect(o).toMatch(/^[A-Za-z0-9_-]+$/); // safe in a URL without escaping
    expect(obfuscate(text, "5/5/other")).not.toBe(o);
  });

  it("does not decode with the wrong puzzle data, and rejects garbage", () => {
    const o = obfuscate("Space invader", seed);
    expect(deobfuscate(o, "5/5/other")).not.toBe("Space invader");
    expect(deobfuscate("!!!not base64!!!", seed)).toBeNull();
  });

  const p: Puzzle = { ...mk(3, 3, [[1], [3], [1]], [[1], [3], [1]]), name: "Plus", answer: "A secret cross" };

  it("links carry the answer scrambled, and reading them recovers it", () => {
    const link = buildShareLink("https://x/", p, { includeAnswer: true });
    expect(link).toContain("&a=");
    expect(link).not.toContain("answer=");
    expect(decodeURIComponent(link).toLowerCase()).not.toContain("secret");
    const u = new URL(link);
    const res = puzzleFromLocation({ hash: u.hash, search: "" });
    expect(res.kind === "puzzle" && res.puzzle.answer).toBe("A secret cross");
    expect(buildShareLink("https://x/", p)).not.toContain("a=");
  });

  it("keeps the answer in the address-bar hash (so a reload doesn't lose it)", () => {
    const res = puzzleFromLocation({ hash: puzzleHash(p), search: "" });
    expect(res.kind === "puzzle" && res.puzzle.answer).toBe("A secret cross");
  });

  it("still reads the older plain answer=, and ignores a corrupted a=", () => {
    const data = encodePuzzlinkData(p);
    const old = puzzleFromLocation({ hash: `#3/3/${data}?answer=Plain%20one`, search: "" });
    expect(old.kind === "puzzle" && old.puzzle.answer).toBe("Plain one");
    const bad = puzzleFromLocation({ hash: `#3/3/${data}?a=%21%21%21`, search: "" });
    expect(bad.kind === "puzzle" && bad.puzzle.answer).toBeUndefined();
    expect(bad.kind).toBe("puzzle");
  });
});
