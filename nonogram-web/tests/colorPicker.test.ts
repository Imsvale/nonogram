import { describe, expect, it } from "vitest";
import { hsvToRgb, rgbToHsv } from "../src/ui/color";
import { CHANNELS, channelValue, SQUARE_AXES, squareColorAt, stripColorAt, stripT, stripValue, withChannel, withPickerChannel, type Channel, type HSV } from "../src/ui/colorPicker";

const HSV_AT = (rgb: [number, number, number]) => rgbToHsv(rgb);

describe("SQUARE_AXES", () => {
  it("pairs each primary with the other two channels in its own group", () => {
    for (const [primary, axes] of Object.entries(SQUARE_AXES) as [Channel, [Channel, Channel]][]) {
      const group = CHANNELS[primary].group;
      const rest = (group === "hsb" ? ["h", "s", "v"] : ["r", "g", "b"]).filter((c) => c !== primary);
      expect(axes.slice().sort()).toEqual(rest.sort());
    }
  });
});

describe("withChannel", () => {
  it("sets r/g/b independently of the other two", () => {
    const rgb: [number, number, number] = [10, 20, 30];
    const hsv = HSV_AT(rgb);
    expect(withChannel(rgb, hsv, "r", 200)).toEqual([200, 20, 30]);
    expect(withChannel(rgb, hsv, "g", 200)).toEqual([10, 200, 30]);
    expect(withChannel(rgb, hsv, "b", 200)).toEqual([10, 20, 200]);
  });

  it("setting h/s/v preserves the other two hsv components", () => {
    const rgb: [number, number, number] = hsvToRgb(210, 60, 80).map(Math.round) as [number, number, number];
    const hsv = HSV_AT(rgb);
    const afterH = rgbToHsv(withChannel(rgb, hsv, "h", 30));
    expect(Math.round(afterH[0])).toBe(30);
    expect(Math.round(afterH[1])).toBe(Math.round(hsv[1]));
    expect(Math.round(afterH[2])).toBe(Math.round(hsv[2]));

    const afterS = rgbToHsv(withChannel(rgb, hsv, "s", 10));
    expect(Math.round(afterS[0])).toBe(Math.round(hsv[0]));
    expect(Math.round(afterS[1])).toBe(10);
    expect(Math.round(afterS[2])).toBe(Math.round(hsv[2]));
  });

  it("clamps to the channel's range", () => {
    const rgb: [number, number, number] = [0, 0, 0];
    const hsv = HSV_AT(rgb);
    expect(withChannel(rgb, hsv, "r", 500)).toEqual([255, 0, 0]);
    expect(withChannel(rgb, hsv, "r", -5)).toEqual([0, 0, 0]);
    // Saturation only shows up once value is nonzero — black stays black at any S.
    const gray = hsvToRgb(210, 0, 60).map(Math.round) as [number, number, number];
    expect(Math.round(rgbToHsv(withChannel(gray, HSV_AT(gray), "s", 500))[1])).toBe(100);
  });
});

describe("squareColorAt (H primary — the classic saturation/value square)", () => {
  const rgb = hsvToRgb(0, 100, 100).map(Math.round) as [number, number, number]; // pure red
  const hsv = HSV_AT(rgb);

  it("top-left is white (S=0, V=100) and bottom-right is black-ish (S=100, V=0)", () => {
    expect(squareColorAt("h", rgb, hsv, 0, 0)).toEqual([255, 255, 255]);
    expect(squareColorAt("h", rgb, hsv, 1, 1)).toEqual([0, 0, 0]);
  });

  it("top-right is the pure primary hue (S=100, V=100)", () => {
    expect(squareColorAt("h", rgb, hsv, 1, 0)).toEqual([255, 0, 0]);
  });

  it("X increases saturation left to right; Y increases value bottom to top", () => {
    const left = rgbToHsv(squareColorAt("h", rgb, hsv, 0, 0.5));
    const right = rgbToHsv(squareColorAt("h", rgb, hsv, 1, 0.5));
    expect(right[1]).toBeGreaterThan(left[1]);
    const bottom = rgbToHsv(squareColorAt("h", rgb, hsv, 0.5, 1));
    const top = rgbToHsv(squareColorAt("h", rgb, hsv, 0.5, 0));
    expect(top[2]).toBeGreaterThan(bottom[2]);
  });
});

describe("squareColorAt (RGB-group primary)", () => {
  it("R primary plots G x B, holding R fixed", () => {
    const rgb: [number, number, number] = [128, 0, 0];
    const hsv = HSV_AT(rgb);
    expect(squareColorAt("r", rgb, hsv, 0, 1)).toEqual([128, 0, 0]); // G=0 (x=0), B=0 (y=1=bottom=min)
    expect(squareColorAt("r", rgb, hsv, 1, 1)).toEqual([128, 255, 0]); // G=255 (x=1)
    expect(squareColorAt("r", rgb, hsv, 0, 0)).toEqual([128, 0, 255]); // B=255 (y=0=top=max)
    // R itself never moves within the square.
    for (const [x, y] of [[0.2, 0.7], [0.9, 0.1], [0.5, 0.5]] as const) {
      expect(squareColorAt("r", rgb, hsv, x, y)[0]).toBe(128);
    }
  });
});

describe("stripT / stripValue (strip orientation)", () => {
  it("red (0°/360°, hue's only fixed point) sits at the top; everything else has its max at the top", () => {
    expect(stripT("h", 0)).toBe(0);
    expect(stripT("h", 360)).toBe(0); // 360 is the same red as 0 — also the top, not the bottom
    for (const ch of ["s", "v", "r", "g", "b"] as Channel[]) {
      expect(stripT(ch, 0)).toBe(1); // min -> bottom
      expect(stripT(ch, CHANNELS[ch].max)).toBe(0); // max -> top
    }
  });

  it("hue runs the OPPOSITE direction from every other channel: down from red goes magenta, blue, cyan, green, yellow, orange, not the other way", () => {
    // t=0.25 (a quarter of the way down): a "counts up like the others" strip would read 90°
    // (yellow-green) here; the real (Photoshop) direction reads 270° (blue-violet) instead.
    expect(stripValue("h", 0.25)).toBeCloseTo(270, 6);
    expect(stripValue("h", 0.5)).toBeCloseTo(180, 6); // the midpoint is its own mirror image
    expect(stripValue("h", 0.75)).toBeCloseTo(90, 6);
    // And the inverse: a hue past the midpoint (e.g. yellow, 60°) sits BELOW the midpoint
    // of the strip (t > 0.5), not above it.
    expect(stripT("h", 60)).toBeGreaterThan(0.5);
    expect(stripT("h", 300)).toBeLessThan(0.5); // magenta sits above the midpoint
  });

  it("stripValue is the exact inverse of stripT, except at hue's shared 0°/360° fixed point", () => {
    for (const ch of ["h", "s", "v", "r", "g", "b"] as Channel[]) {
      for (const frac of [0, 0.25, 0.5, 0.75, 1]) {
        const value = frac * CHANNELS[ch].max;
        if (ch === "h" && value === 360) {
          // 360 and 0 are the same color; the round trip lands back on 0, not literally 360.
          expect(stripValue(ch, stripT(ch, value))).toBeCloseTo(0, 6);
        } else {
          expect(stripValue(ch, stripT(ch, value))).toBeCloseTo(value, 6);
        }
      }
    }
  });
});

describe("stripColorAt", () => {
  it("the hue strip is always pure spectrum, regardless of current S/V", () => {
    const dim: [number, number, number] = hsvToRgb(90, 10, 20).map(Math.round) as [number, number, number]; // low S, low V
    const hsv = HSV_AT(dim);
    const atTop = rgbToHsv(stripColorAt("h", dim, hsv, 0));
    const atMid = rgbToHsv(stripColorAt("h", dim, hsv, 0.5));
    expect(Math.round(atTop[1])).toBe(100);
    expect(Math.round(atTop[2])).toBe(100);
    expect(Math.round(atMid[1])).toBe(100);
    expect(Math.round(atMid[2])).toBe(100);
    // Top = 0°, bottom = 360° (same as 0°) — hue is the one channel NOT flipped.
    expect(Math.round(atTop[0])).toBe(0);
  });

  it("the saturation strip runs 100 at the top to 0 at the bottom, at the CURRENT hue and value", () => {
    const rgb = hsvToRgb(210, 50, 70).map(Math.round) as [number, number, number];
    const hsv = HSV_AT(rgb);
    const top = rgbToHsv(stripColorAt("s", rgb, hsv, 0));
    const mid = rgbToHsv(stripColorAt("s", rgb, hsv, 0.5));
    const bottom = rgbToHsv(stripColorAt("s", rgb, hsv, 1));
    expect(Math.round(top[1])).toBe(100);
    expect(Math.round(mid[1])).toBe(50);
    expect(Math.round(bottom[1])).toBe(0); // S=0: achromatic, hue is not a meaningful/observable property here
    // Hue and value are only observable (and only need to be preserved) once S > 0.
    expect(Math.round(mid[0])).toBe(210);
    expect(Math.round(mid[2])).toBe(70);
    expect(Math.round(top[2])).toBe(70); // V itself is still held, even though it's only visible once S > 0
  });

  it("an RGB-channel strip ramps 255 at the top to 0 at the bottom, at the current other two channels", () => {
    const rgb: [number, number, number] = [10, 20, 30];
    const hsv = HSV_AT(rgb);
    expect(stripColorAt("g", rgb, hsv, 0)).toEqual([10, 255, 30]);
    expect(stripColorAt("g", rgb, hsv, 1)).toEqual([10, 0, 30]);
  });
});

describe("channelValue", () => {
  it("reads back what withChannel wrote, for every channel", () => {
    let rgb: [number, number, number] = [40, 90, 200];
    for (const ch of ["h", "s", "v", "r", "g", "b"] as Channel[]) {
      const hsv = HSV_AT(rgb);
      const target = CHANNELS[ch].max * 0.3;
      rgb = withChannel(rgb, hsv, ch, target);
      expect(Math.round(channelValue(ch, rgb, rgbToHsv(rgb)))).toBe(Math.round(target));
    }
  });
});

describe("withPickerChannel (the S=0 / V=0 \"stuck slider\" bug)", () => {
  it("setting H while S=0 changes H, even though the visible color (gray) doesn't move", () => {
    const white: HSV = [0, 0, 100]; // S=0: hue has no visible effect here
    const after = withPickerChannel(white, "h", 200);
    expect(after[0]).toBe(200); // NOT silently discarded/reset to 0
    expect(after[1]).toBe(0); // S untouched
    expect(after[2]).toBe(100); // V untouched
    // The color is still visually gray at this hue (S=0) — the point is H is REMEMBERED, not visible yet.
    expect(hsvToRgb(after[0], after[1], after[2]).map(Math.round)).toEqual([255, 255, 255]);
  });

  it("raising S afterwards reveals the hue that was set while S was 0", () => {
    let hsv: HSV = [0, 0, 100];
    hsv = withPickerChannel(hsv, "h", 200); // set hue while it's invisible
    hsv = withPickerChannel(hsv, "s", 100); // now make it visible
    expect(Math.round(hsv[0])).toBe(200);
    expect(Math.round(hsv[1])).toBe(100);
    const rgb = hsvToRgb(hsv[0], hsv[1], hsv[2]).map(Math.round);
    expect(rgb).not.toEqual([255, 255, 255]); // now visibly colored
    expect(Math.round(rgbToHsv(rgb as [number, number, number])[0])).toBe(200); // and it's the hue we set
  });

  it("the same bug shape for S: setting S while V=0 (black) is remembered once V rises", () => {
    let hsv: HSV = [210, 0, 0]; // V=0: black, saturation has no visible effect
    hsv = withPickerChannel(hsv, "s", 80);
    expect(hsv[1]).toBe(80); // not discarded
    hsv = withPickerChannel(hsv, "v", 60); // reveal it
    expect(Math.round(hsv[0])).toBe(210);
    expect(Math.round(hsv[1])).toBe(80);
  });

  it("repeated H edits while S stays 0 all stick, not just the first one", () => {
    let hsv: HSV = [0, 0, 50];
    for (const target of [10, 200, 359, 45]) {
      hsv = withPickerChannel(hsv, "h", target);
      expect(hsv[0]).toBe(target);
      expect(hsv[1]).toBe(0); // still invisible, still remembered correctly
    }
  });

  it("r/g/b have no such degeneracy: setting one channel never disturbs the other two", () => {
    const hsv: HSV = rgbToHsv([10, 20, 30]);
    const afterR = withPickerChannel(hsv, "r", 200);
    const rgbAfterR = hsvToRgb(afterR[0], afterR[1], afterR[2]).map(Math.round);
    expect(rgbAfterR).toEqual([200, 20, 30]);
  });

  it("clamps to the channel's range", () => {
    const hsv: HSV = [0, 0, 50];
    expect(withPickerChannel(hsv, "h", 500)[0]).toBe(360);
    expect(withPickerChannel(hsv, "h", -10)[0]).toBe(0);
    expect(withPickerChannel(hsv, "s", 500)[1]).toBe(100);
  });
});
