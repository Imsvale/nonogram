import type { Puzzle } from "./types";

/**
 * puzz.link nonogram codec. Format reference: `specs/puzzlink.md` in the
 * workspace. Columns then rows; each group is its clues in REVERSE order
 * followed by a padding suffix bringing it to ceil(len / 2) slots.
 */

function encodeValue(v: number): string {
  if (v <= 9) return String(v);
  if (v <= 15) return String.fromCharCode(97 + v - 10); // a–f
  return "-" + v.toString(16).padStart(2, "0");
}

function encodeGroup(clues: readonly number[], g: number): string {
  let out = "";
  for (let i = clues.length - 1; i >= 0; i--) out += encodeValue(clues[i]);
  let zeros = Math.max(0, g - clues.length);
  while (zeros >= 20) {
    out += "z";
    zeros -= 20;
  }
  if (zeros > 0) out += String.fromCharCode(102 + zeros); // 'f' + zeros
  return out;
}

/** Just the `{data}` part of a puzz.link URL. */
export function encodePuzzlinkData(p: Puzzle): string {
  const gCol = Math.ceil(p.height / 2);
  const gRow = Math.ceil(p.width / 2);
  let data = "";
  for (let c = 0; c < p.width; c++) data += encodeGroup(p.colClues[c], gCol);
  for (let r = 0; r < p.height; r++) data += encodeGroup(p.rowClues[r], gRow);
  return data;
}

export function puzzleToPuzzlinkUrl(p: Puzzle): string {
  return `https://puzz.link/p?nonogram/${p.width}/${p.height}/${encodePuzzlinkData(p)}`;
}

class DecodeError extends Error {}

function decodeGroup(data: string, pos: { i: number }, g: number, explicitF: boolean): number[] {
  const values: number[] = [];
  let zeros = 0;
  for (;;) {
    if (values.length === g) {
      // All slots full → no padding. Some encoders emit an explicit 'f' anyway;
      // that is ambiguous with a following group starting with 15, so it is
      // only consumed on the lenient retry pass.
      if (explicitF && data[pos.i] === "f") pos.i++;
      break;
    }
    if (pos.i >= data.length) throw new DecodeError("unexpected end of data in group");
    const ch = data[pos.i++];
    if (ch >= "g") {
      // Suffix: each 'z' adds 20; a non-'z' char adds (ch - 'f') and ends it.
      let c = ch;
      let count = 0;
      for (;;) {
        count += c.charCodeAt(0) - 102;
        if (c !== "z") break;
        const next = data[pos.i];
        if (next !== undefined && next >= "g") {
          c = next;
          pos.i++;
        } else break;
      }
      zeros = count;
      break;
    }
    if (ch >= "0" && ch <= "9") values.push(ch.charCodeAt(0) - 48);
    else if (ch >= "a" && ch <= "f") values.push(10 + ch.charCodeAt(0) - 97);
    else if (ch === "-") {
      const hex = data.slice(pos.i, pos.i + 2);
      if (hex.length < 2) throw new DecodeError("truncated hex value");
      const v = parseInt(hex, 16);
      if (!/^[0-9a-f]{2}$/.test(hex) || Number.isNaN(v)) throw new DecodeError(`invalid hex: -${hex}`);
      pos.i += 2;
      values.push(v);
    } else {
      throw new DecodeError(`unexpected char '${ch}'`);
    }
  }
  if (values.length + zeros !== g) {
    throw new DecodeError(`group size mismatch: ${values.length} values + ${zeros} zeros ≠ ${g}`);
  }
  return values.reverse();
}

function decodeData(w: number, h: number, data: string, explicitF: boolean, requireEnd: boolean): Puzzle {
  const gCol = Math.ceil(h / 2);
  const gRow = Math.ceil(w / 2);
  const pos = { i: 0 };
  const colClues: number[][] = [];
  const rowClues: number[][] = [];
  try {
    for (let c = 0; c < w; c++) colClues.push(decodeGroup(data, pos, gCol, explicitF));
  } catch (e) {
    throw new DecodeError(`col ${colClues.length + 1}: ${(e as Error).message}`);
  }
  try {
    for (let r = 0; r < h; r++) rowClues.push(decodeGroup(data, pos, gRow, explicitF));
  } catch (e) {
    throw new DecodeError(`row ${rowClues.length + 1}: ${(e as Error).message}`);
  }
  if (requireEnd && pos.i !== data.length) throw new DecodeError("trailing data after last group");
  return { name: `${w}×${h}`, width: w, height: h, colClues, rowClues };
}

/** Decode the width/height/data triple of a puzz.link nonogram URL. */
export function decodePuzzlinkParts(w: number, h: number, data: string): Puzzle {
  if (!(w > 0 && h > 0)) throw new Error("Zero-dimension puzzle");
  if (w > 500 || h > 500) throw new Error("Puzzle too large");
  let firstError: Error | null = null;
  // Strict first (never swallow an 'f'), then tolerate an explicit 'f' marker,
  // then tolerate trailing junk.
  for (const [explicitF, requireEnd] of [[false, true], [true, true], [false, false]] as const) {
    try {
      return decodeData(w, h, data, explicitF, requireEnd);
    } catch (e) {
      firstError ??= e as Error;
    }
  }
  throw firstError!;
}

/**
 * `nonogram/W/H/data` (puzz.link URLs), `#W/H/data` (this app's links) or a bare
 * `W/H/data` at the start of the text.
 */
export const PUZZLINK_RE = /(?:nonogram\/|#|^\s*)(\d+)\/(\d+)\/([0-9a-z-]*)/;

/** Decode any string containing a puzz.link / app link (full URL or just the tail). */
export function parsePuzzlinkUrl(url: string): Puzzle {
  const m = PUZZLINK_RE.exec(url);
  if (!m) throw new Error("Not a puzz.link nonogram URL");
  return decodePuzzlinkParts(Number(m[1]), Number(m[2]), m[3]);
}
