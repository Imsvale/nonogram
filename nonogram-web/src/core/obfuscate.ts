/**
 * Light obfuscation for the answer (spoiler title) carried in share links.
 *
 * This is NOT encryption: anyone with the source, or the patience, can undo it.
 * The aim is only that the answer can't be read at a glance in the address bar,
 * a chat preview or a screenshot. So it isn't plain text, isn't recognizable
 * base64 of the text, and looks different for every puzzle: the text is XORed
 * with a keystream seeded from the puzzle's own clue data (`W/H/DATA`).
 */

function hash32(s: string): number {
  let h = 0x811c9dc5;
  for (let i = 0; i < s.length; i++) h = Math.imul(h ^ s.charCodeAt(i), 0x01000193) >>> 0;
  return h;
}

/** Deterministic byte stream (mulberry32) from a seed string. */
function keystream(seed: string): () => number {
  let a = hash32(`nonogram:${seed}`);
  return () => {
    a = (a + 0x6d2b79f5) | 0;
    let t = Math.imul(a ^ (a >>> 15), 1 | a);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return ((t ^ (t >>> 14)) >>> 0) & 0xff;
  };
}

function xor(bytes: Uint8Array, seed: string): Uint8Array {
  const next = keystream(seed);
  return bytes.map((b) => b ^ next());
}

function toBase64Url(bytes: Uint8Array): string {
  let bin = "";
  for (const b of bytes) bin += String.fromCharCode(b);
  return btoa(bin).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
}

function fromBase64Url(s: string): Uint8Array {
  const bin = atob(s.replace(/-/g, "+").replace(/_/g, "/"));
  return Uint8Array.from(bin, (c) => c.charCodeAt(0));
}

/** URL-safe scrambled form of `text`, keyed by the puzzle's `W/H/DATA`. */
export function obfuscate(text: string, seed: string): string {
  return toBase64Url(xor(new TextEncoder().encode(text), seed));
}

/** Inverse of `obfuscate`; `null` if the input isn't valid (bad base64 / not UTF-8). */
export function deobfuscate(s: string, seed: string): string | null {
  try {
    return new TextDecoder("utf-8", { fatal: true }).decode(xor(fromBase64Url(s), seed));
  } catch {
    return null;
  }
}
