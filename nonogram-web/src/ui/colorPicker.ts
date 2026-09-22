/**
 * Custom HSB/RGB color picker popup (Photoshop-style): a square + a 1-D strip driven by
 * whichever of H/S/B/R/G/B is selected as the "primary" channel, six numeric fields, a hex
 * field, and new/current swatches. Not encapsulated as a class — one module-level `active`
 * slot enforces "only one open at a time" and gives `closeColorPicker()` something to call
 * (e.g. when the settings drawer closes, or Reset All Settings is pressed).
 */
import { hexToRgb, hsvToRgb, rgbToHex, rgbToHsv, type RGB } from "./color";
import { h } from "./dom";

export type Channel = "h" | "s" | "v" | "r" | "g" | "b";
export type HSV = [number, number, number];

export const CHANNELS: Record<Channel, { group: "hsb" | "rgb"; max: number; unit: string; label: string }> = {
  h: { group: "hsb", max: 360, unit: "°", label: "H" },
  s: { group: "hsb", max: 100, unit: "%", label: "S" },
  v: { group: "hsb", max: 100, unit: "%", label: "V" },
  r: { group: "rgb", max: 255, unit: "", label: "R" },
  g: { group: "rgb", max: 255, unit: "", label: "G" },
  b: { group: "rgb", max: 255, unit: "", label: "B" },
};
const HSB_CHANNELS: Channel[] = ["h", "s", "v"];
const RGB_CHANNELS: Channel[] = ["r", "g", "b"];
// What the square's X/Y axes are for each primary — the other two channels in its own group.
export const SQUARE_AXES: Record<Channel, [Channel, Channel]> = {
  h: ["s", "v"],
  s: ["h", "v"],
  v: ["h", "s"],
  r: ["g", "b"],
  g: ["r", "b"],
  b: ["r", "g"],
};

export function channelValue(ch: Channel, rgb: RGB, hsv: HSV): number {
  switch (ch) {
    case "h":
      return hsv[0];
    case "s":
      return hsv[1];
    case "v":
      return hsv[2];
    case "r":
      return rgb[0];
    case "g":
      return rgb[1];
    case "b":
      return rgb[2];
  }
}

/** `rgb` with channel `ch` set to `value` (clamped, rounded); the other channels follow HSV math when `ch` is h/s/v. */
export function withChannel(rgb: RGB, hsv: HSV, ch: Channel, value: number): RGB {
  const v = Math.min(CHANNELS[ch].max, Math.max(0, value));
  const out: RGB =
    ch === "h" ? hsvToRgb(v, hsv[1], hsv[2])
    : ch === "s" ? hsvToRgb(hsv[0], v, hsv[2])
    : ch === "v" ? hsvToRgb(hsv[0], hsv[1], v)
    : ch === "r" ? [v, rgb[1], rgb[2]]
    : ch === "g" ? [rgb[0], v, rgb[2]]
    : [rgb[0], rgb[1], v];
  return out.map(Math.round) as unknown as RGB;
}

/** Color at normalized (0..1, 0..1) position on the square for the given primary channel (held at its current value). */
export function squareColorAt(primary: Channel, rgb: RGB, hsv: HSV, xNorm: number, yNorm: number): RGB {
  const [xCh, yCh] = SQUARE_AXES[primary];
  const withX = withChannel(rgb, hsv, xCh, xNorm * CHANNELS[xCh].max);
  const hsvAfterX = rgbToHsv(withX);
  return withChannel(withX, hsvAfterX, yCh, (1 - yNorm) * CHANNELS[yCh].max);
}

/**
 * Strip orientation, matching the Photoshop reference this was built from:
 * - Every channel but hue puts its maximum at the top (V, S, R, G, B all read "more" going up).
 * - Hue keeps red (0°/360°) at the top too, but that's where the resemblance to the others
 *   ends: red is the *only* fixed point (0° and 360° are the same color), and going down from
 *   it runs red -> magenta -> blue -> cyan -> green -> yellow -> orange -> red — the opposite
 *   direction around the wheel from "hue counts up like the others do". Get this backwards and
 *   the two ends still both show red, so the mistake only shows up in the middle of the strip.
 * Centralized here so the gradient, the marker, and the drag handler can't independently drift
 * out of sync with each other.
 */
export function stripT(primary: Channel, value: number): number {
  if (primary === "h") {
    // 0 is reachable from both t=0 and t=1 (hue wraps); bias to the top, since that's the
    // default/common case (grayscale colors, or pure red) and matches what the strip's own
    // top pixel shows.
    return value <= 0 ? 0 : (360 - value) / 360;
  }
  const f = value / CHANNELS[primary].max;
  return 1 - f;
}
export function stripValue(primary: Channel, t: number): number {
  if (primary === "h") return (360 - t * 360) % 360;
  const f = 1 - t;
  return f * CHANNELS[primary].max;
}

/**
 * Color at position `tNorm` (0=top, 1=bottom — see `stripT`/`stripValue` for what that means
 * per channel) along the strip for the given primary. The hue strip always shows the pure
 * spectrum (S=100, V=100): showing it "at the current S/V" would often be a useless flat
 * gray/black and defeat the point of a hue picker. The other five strips show the real
 * gradient at the current values of their group's other two channels.
 */
export function stripColorAt(primary: Channel, rgb: RGB, hsv: HSV, tNorm: number): RGB {
  const value = stripValue(primary, tNorm);
  if (primary === "h") return hsvToRgb(value, 100, 100);
  return withChannel(rgb, hsv, primary, value);
}

/**
 * Advances the picker's PERSISTED state (see `openColorPicker`) by one channel edit.
 *
 * For h/s/v this sets that HSV component directly, WITHOUT round-tripping through RGB —
 * unlike `withChannel` above, which is fine for one-off rendering math but wrong to use for
 * state that has to survive across edits. The reason: at S=0 (or V=0), no RGB triple can
 * tell "gray" apart from "gray, but I'd set the hue to 200° before saturation hit zero" —
 * there's only one gray. Round-tripping through RGB there silently snaps hue back to
 * whatever a gray pixel happens to decode as (0°), so the H slider looks "locked" the
 * moment its effect on the visible color reaches zero, even though the user can still see
 * and move the slider (and should reasonably expect the value to still apply once S rises
 * again). r/g/b have no such degeneracy — every RGB triple is exactly recoverable — so
 * those do go via the current RGB, changing just that one channel.
 */
export function withPickerChannel(hsv: HSV, ch: Channel, value: number): HSV {
  const v = Math.min(CHANNELS[ch].max, Math.max(0, value));
  if (ch === "h") return [v, hsv[1], hsv[2]];
  if (ch === "s") return [hsv[0], v, hsv[2]];
  if (ch === "v") return [hsv[0], hsv[1], v];
  const rgb = hsvToRgb(hsv[0], hsv[1], hsv[2]).map(Math.round) as RGB;
  const newRgb = (ch === "r" ? [v, rgb[1], rgb[2]] : ch === "g" ? [rgb[0], v, rgb[2]] : [rgb[0], rgb[1], v]) as RGB;
  return rgbToHsv(newRgb);
}

export interface ColorPickerOptions {
  anchor: HTMLElement;
  initial: string;
  /** Fired on every change (drag, type, wheel) — live, like the rest of the app's settings. */
  onChange: (hex: string) => void;
  onClose?: () => void;
}

let active: { close: () => void } | null = null;

/** Close whatever color picker is currently open, if any. Safe to call when none is open. */
export function closeColorPicker(): void {
  active?.close();
}

const SQ = 200;
const STRIP_W = 22;
const STRIP_H = 200;
const STRIP_STOPS = 16;

export function openColorPicker(opts: ColorPickerOptions): void {
  closeColorPicker();

  // hsv is the persisted state (see withPickerChannel for why); rgb is derived from it fresh
  // wherever it's needed, never the other way around.
  let hsv: HSV = rgbToHsv(hexToRgb(opts.initial));
  const currentRgb = (): RGB => hsvToRgb(hsv[0], hsv[1], hsv[2]).map(Math.round) as RGB;
  let primary: Channel = "h";
  let closed = false;
  const dpr = window.devicePixelRatio || 1;

  // ── Square + strip ─────────────────────────────────────────────────────

  const sqCanvas = h("canvas", { class: "cp-square" });
  sqCanvas.width = SQ * dpr;
  sqCanvas.height = SQ * dpr;
  sqCanvas.style.width = `${SQ}px`;
  sqCanvas.style.height = `${SQ}px`;
  const sqMarker = h("div", { class: "cp-square-marker" });

  const stripCanvas = h("canvas", { class: "cp-strip" });
  stripCanvas.width = STRIP_W * dpr;
  stripCanvas.height = STRIP_H * dpr;
  stripCanvas.style.width = `${STRIP_W}px`;
  stripCanvas.style.height = `${STRIP_H}px`;
  const stripMarker = h("div", { class: "cp-strip-marker" });

  function drawSquare(rgb: RGB): void {
    const ctx = sqCanvas.getContext("2d")!;
    const w = SQ * dpr;
    const img = ctx.createImageData(w, w);
    for (let py = 0; py < w; py++) {
      const yN = py / (w - 1);
      for (let px = 0; px < w; px++) {
        const xN = px / (w - 1);
        const [r, g, b] = squareColorAt(primary, rgb, hsv, xN, yN);
        const i = (py * w + px) * 4;
        img.data[i] = r;
        img.data[i + 1] = g;
        img.data[i + 2] = b;
        img.data[i + 3] = 255;
      }
    }
    ctx.putImageData(img, 0, 0);
  }

  function drawStrip(rgb: RGB): void {
    const ctx = stripCanvas.getContext("2d")!;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    const grad = ctx.createLinearGradient(0, 0, 0, STRIP_H);
    for (let i = 0; i <= STRIP_STOPS; i++) {
      const t = i / STRIP_STOPS;
      // Hue wraps (0° = 360°): sampling it very slightly short of the true end keeps the
      // strip from painting the same red at both top and bottom, which reads as a seam/glitch.
      const colorT = primary === "h" ? t * ((STRIP_STOPS - 1) / STRIP_STOPS) : t;
      grad.addColorStop(t, rgbToHex(stripColorAt(primary, rgb, hsv, colorT)));
    }
    ctx.fillStyle = grad;
    ctx.fillRect(0, 0, STRIP_W, STRIP_H);
  }

  function positionMarkers(rgb: RGB): void {
    const [xCh, yCh] = SQUARE_AXES[primary];
    const xN = channelValue(xCh, rgb, hsv) / CHANNELS[xCh].max;
    const yN = 1 - channelValue(yCh, rgb, hsv) / CHANNELS[yCh].max;
    sqMarker.style.left = `${xN * SQ}px`;
    sqMarker.style.top = `${yN * SQ}px`;
    stripMarker.style.top = `${stripT(primary, channelValue(primary, rgb, hsv)) * STRIP_H}px`;
  }

  // ── Swatches ───────────────────────────────────────────────────────────

  const swNew = h("div", { class: "cp-swatch" });
  const swCurrent = h("button", { type: "button", class: "cp-swatch", title: "Back to the original color" });
  swCurrent.style.background = opts.initial;
  swCurrent.addEventListener("click", () => {
    hsv = rgbToHsv(hexToRgb(opts.initial));
    renderAll();
  });

  // ── Fields ─────────────────────────────────────────────────────────────

  const radios = {} as Record<Channel, HTMLInputElement>;
  const numInputs = {} as Record<Channel, HTMLInputElement>;
  const hexInput = h("input", { type: "text", class: "cp-hex", spellcheck: "false" });

  function buildFieldRow(ch: Channel): HTMLElement {
    const radio = h("input", { type: "radio", name: "cp-primary" });
    const num = h("input", { type: "text", inputmode: "numeric", class: "cp-num" });
    radios[ch] = radio;
    numInputs[ch] = num;
    radio.addEventListener("change", () => {
      primary = ch;
      renderAll();
    });
    num.addEventListener("input", () => {
      const n = Number(num.value);
      if (Number.isFinite(n) && num.value.trim() !== "") setChannel(ch, n);
    });
    num.addEventListener("blur", () => syncFields(currentRgb()));
    num.addEventListener(
      "wheel",
      (e) => {
        e.preventDefault();
        setChannel(ch, channelValue(ch, currentRgb(), hsv) + (e.deltaY < 0 ? 1 : -1));
        // A wheel step is a discrete commit (unlike typing), so it should show up in this
        // field immediately even if the field still has focus from a previous edit — syncFields()
        // otherwise deliberately leaves a focused field alone so it doesn't fight live typing.
        num.value = String(Math.round(channelValue(ch, currentRgb(), hsv)));
      },
      { passive: false },
    );
    const unit = CHANNELS[ch].unit ? h("span", { class: "cp-unit" }, CHANNELS[ch].unit) : null;
    return h("label", { class: "cp-field" }, radio, h("span", { class: "cp-field-label" }, CHANNELS[ch].label), num, unit);
  }

  const fieldsEl = h(
    "div",
    { class: "cp-fields" },
    ...HSB_CHANNELS.map(buildFieldRow),
    h("div", { class: "cp-field-gap" }),
    ...RGB_CHANNELS.map(buildFieldRow),
  );

  hexInput.addEventListener("input", () => {
    const v = hexInput.value.trim().replace(/^#/, "");
    if (/^[0-9a-fA-F]{3}$/.test(v) || /^[0-9a-fA-F]{6}$/.test(v)) {
      hsv = rgbToHsv(hexToRgb(`#${v}`));
      renderAll();
    }
  });
  hexInput.addEventListener("blur", () => syncFields(currentRgb()));

  // ── Assembly ───────────────────────────────────────────────────────────

  const closeBtn = h("button", { type: "button", class: "btn primary" }, "Close");
  const popup = h(
    "div",
    { class: "color-popup", role: "dialog", "aria-label": "Color picker", tabindex: "-1" },
    h(
      "div",
      { class: "cp-main" },
      h(
        "div",
        { class: "cp-swatches" },
        h("span", { class: "cp-swatch-label" }, "New"),
        h("div", { class: "cp-swatch-pair" }, swNew, swCurrent),
        h("span", { class: "cp-swatch-label" }, "Current"),
      ),
      h("div", { class: "cp-square-wrap" }, sqCanvas, sqMarker),
      h("div", { class: "cp-strip-wrap" }, stripCanvas, stripMarker),
      fieldsEl,
    ),
    h("div", { class: "cp-bottom" }, h("label", { class: "cp-hex-row" }, h("span", {}, "#"), hexInput), closeBtn),
  );

  // ── Interaction ────────────────────────────────────────────────────────

  function setChannel(ch: Channel, value: number): void {
    hsv = withPickerChannel(hsv, ch, value);
    renderAll();
  }

  function bindDrag(el: HTMLElement, onMove: (e: PointerEvent) => void): void {
    el.addEventListener("pointerdown", (e) => {
      el.setPointerCapture(e.pointerId);
      onMove(e);
    });
    el.addEventListener("pointermove", (e) => {
      if (e.buttons & 1) onMove(e);
    });
  }

  function normOf(el: HTMLElement, e: PointerEvent): [number, number] {
    const r = el.getBoundingClientRect();
    return [Math.min(1, Math.max(0, (e.clientX - r.left) / r.width)), Math.min(1, Math.max(0, (e.clientY - r.top) / r.height))];
  }

  bindDrag(sqCanvas, (e) => {
    const [xN, yN] = normOf(sqCanvas, e);
    const [xCh, yCh] = SQUARE_AXES[primary];
    hsv = withPickerChannel(hsv, xCh, xN * CHANNELS[xCh].max);
    hsv = withPickerChannel(hsv, yCh, (1 - yN) * CHANNELS[yCh].max);
    renderAll();
  });
  bindDrag(stripCanvas, (e) => {
    const [, tN] = normOf(stripCanvas, e);
    setChannel(primary, stripValue(primary, tN));
  });

  function syncFields(rgb: RGB): void {
    for (const ch of [...HSB_CHANNELS, ...RGB_CHANNELS]) {
      radios[ch].checked = ch === primary;
      if (document.activeElement !== numInputs[ch]) numInputs[ch].value = String(Math.round(channelValue(ch, rgb, hsv)));
    }
    if (document.activeElement !== hexInput) hexInput.value = rgbToHex(rgb).slice(1);
  }

  function renderAll(): void {
    const rgb = currentRgb();
    drawSquare(rgb);
    drawStrip(rgb);
    positionMarkers(rgb);
    syncFields(rgb);
    swNew.style.background = rgbToHex(rgb);
    opts.onChange(rgbToHex(rgb));
  }

  // ── Positioning, lifecycle ─────────────────────────────────────────────

  function position(): void {
    const r = opts.anchor.getBoundingClientRect();
    const pw = popup.offsetWidth || 280;
    const ph = popup.offsetHeight || 440;
    const x = Math.min(Math.max(8, r.left), window.innerWidth - pw - 8);
    const y = Math.min(Math.max(8, r.bottom + 6), window.innerHeight - ph - 8);
    popup.style.left = `${x}px`;
    popup.style.top = `${y}px`;
  }

  function onOutside(e: PointerEvent): void {
    if (!popup.contains(e.target as Node)) close();
  }
  function onScroll(): void {
    close();
  }
  function onKey(e: KeyboardEvent): void {
    if (e.key === "Escape") {
      e.stopPropagation();
      close();
    }
  }

  function close(): void {
    if (closed) return;
    closed = true;
    document.removeEventListener("pointerdown", onOutside);
    window.removeEventListener("resize", position);
    window.removeEventListener("scroll", onScroll, true);
    popup.remove();
    if (active?.close === close) active = null;
    opts.onClose?.();
  }
  closeBtn.addEventListener("click", close);
  popup.addEventListener("keydown", onKey);
  active = { close };

  document.body.append(popup);
  position();
  window.addEventListener("resize", position);
  window.addEventListener("scroll", onScroll, true); // capture: catches scroll on any scrollable ancestor (e.g. the settings drawer)
  setTimeout(() => document.addEventListener("pointerdown", onOutside), 0);

  renderAll();
}
