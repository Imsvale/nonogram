import { describe, expect, it } from "vitest";
import { contrastOn, hslToHex, huedInk, luminance, rgbToHsv, hsvToRgb, type RGB } from "../src/ui/color";

describe("contrastOn", () => {
  it("flips at the threshold", () => {
    expect(contrastOn("#ffffff")).toBe("#262626");
    expect(contrastOn("#000000")).toBe("#ffffff");
    // A mid-gray moved across a moved threshold flips which ink it gets.
    const midGray = "#808080"; // luminance ~0.5
    expect(contrastOn(midGray, 0.2)).toBe("#262626"); // now "light enough" for dark ink
    expect(contrastOn(midGray, 0.8)).toBe("#ffffff"); // now "dark enough" for light ink
  });
});

describe("hslToHex", () => {
  it("matches known colors", () => {
    expect(hslToHex(0, 1, 0.5)).toBe("#ff0000");
    expect(hslToHex(120, 1, 0.5)).toBe("#00ff00");
    expect(hslToHex(240, 1, 0.5)).toBe("#0000ff");
    expect(hslToHex(0, 0, 0.5)).toBe("#808080"); // no saturation -> gray, hue irrelevant
    expect(hslToHex(0, 0, 1)).toBe("#ffffff");
    expect(hslToHex(0, 0, 0)).toBe("#000000");
  });

  it("wraps hue and clamps s/l", () => {
    expect(hslToHex(360, 1, 0.5)).toBe(hslToHex(0, 1, 0.5));
    expect(hslToHex(-30, 1, 0.5)).toBe(hslToHex(330, 1, 0.5));
    expect(hslToHex(0, 5, 0.5)).toBe(hslToHex(0, 1, 0.5)); // saturation clamped to 1
  });
});

describe("huedInk", () => {
  it("picks a deep shade on a light background and a light tint on a dark one", () => {
    const onWhite = huedInk(210, 70, "#ffffff");
    const onBlack = huedInk(210, 70, "#000000");
    expect(luminance(onWhite)).toBeLessThan(0.5);
    expect(luminance(onBlack)).toBeGreaterThan(0.5);
    // Both are still visibly the same hue family (blue-ish), not gray: saturation survived.
    expect(onWhite).not.toBe("#262626");
    expect(onBlack).not.toBe("#ffffff");
  });

  it("always contrasts, for a spread of hues and saturations, on both extreme backgrounds", () => {
    for (const hue of [0, 45, 90, 135, 180, 225, 270, 315]) {
      for (const sat of [10, 50, 100]) {
        const onWhite = luminance(huedInk(hue, sat, "#ffffff"));
        const onBlack = luminance(huedInk(hue, sat, "#000000"));
        expect(onWhite).toBeLessThan(0.5);
        expect(onBlack).toBeGreaterThan(0.5);
        // The two are meaningfully different from their backgrounds, not just barely.
        expect(1 - onWhite).toBeGreaterThan(0.35);
        expect(onBlack - 0).toBeGreaterThan(0.35);
      }
    }
  });

  it("follows a moved threshold, same as contrastOn", () => {
    const midGray = "#808080";
    const lo = huedInk(0, 100, midGray, 0.2);
    const hi = huedInk(0, 100, midGray, 0.8);
    expect(luminance(lo)).toBeLessThan(0.5); // "light enough" now -> deep ink
    expect(luminance(hi)).toBeGreaterThan(0.5); // "dark enough" now -> pale ink
  });

  it("clamps out-of-range saturation instead of producing nonsense", () => {
    expect(huedInk(0, 500, "#ffffff")).toBe(huedInk(0, 100, "#ffffff"));
    expect(huedInk(0, -50, "#ffffff")).toBe(huedInk(0, 0, "#ffffff"));
  });
});

describe("rgbToHsv / hsvToRgb", () => {
  it("matches known fixed points", () => {
    expect(rgbToHsv([255, 0, 0])).toEqual([0, 100, 100]);
    expect(rgbToHsv([0, 255, 0])).toEqual([120, 100, 100]);
    expect(rgbToHsv([0, 0, 255])).toEqual([240, 100, 100]);
    expect(rgbToHsv([255, 255, 255])).toEqual([0, 0, 100]);
    expect(rgbToHsv([0, 0, 0])).toEqual([0, 0, 0]);
    expect(hsvToRgb(0, 0, 100).map(Math.round)).toEqual([255, 255, 255]);
    expect(hsvToRgb(0, 0, 0).map(Math.round)).toEqual([0, 0, 0]);
    expect(hsvToRgb(0, 100, 100).map(Math.round)).toEqual([255, 0, 0]);
  });

  it("round-trips rgb -> hsv -> rgb for a spread of colors", () => {
    const samples: RGB[] = [
      [255, 0, 0], [0, 255, 0], [0, 0, 255], [255, 255, 0], [0, 255, 255], [255, 0, 255],
      [128, 64, 32], [10, 200, 150], [1, 254, 3], [90, 90, 90], [255, 128, 0], [17, 33, 250],
    ];
    for (const rgb of samples) {
      const [h, s, v] = rgbToHsv(rgb);
      const back = hsvToRgb(h, s, v).map(Math.round);
      expect(back).toEqual(rgb);
    }
  });

  it("clamps and wraps out-of-range input", () => {
    expect(hsvToRgb(720, 100, 100).map(Math.round)).toEqual(hsvToRgb(0, 100, 100).map(Math.round));
    expect(hsvToRgb(-90, 100, 100).map(Math.round)).toEqual(hsvToRgb(270, 100, 100).map(Math.round));
    expect(hsvToRgb(0, 500, 500).map(Math.round)).toEqual(hsvToRgb(0, 100, 100).map(Math.round));
  });
});
