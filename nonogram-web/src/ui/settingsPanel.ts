import {
  FULLSCREEN_KEYS,
  ICON_KINDS,
  paletteFor,
  type IconKind,
  type ResolvedTheme,
  type Settings,
  type SubcellKind,
  type ThemeChoice,
} from "../state/settings";
import { contrastOn, huedInk } from "./color";
import { closeColorPicker, openColorPicker } from "./colorPicker";
import { h } from "./dom";
import { drawIcon } from "./icons";

export interface SettingsPanelDeps {
  settings: Settings;
  getTheme(): ResolvedTheme;
  /** Called after any change; the app persists and re-renders. */
  onChange(): void;
  /** Replace all settings with defaults. */
  onReset(): void;
}

/** Builds the settings drawer. Returns the element and a `refresh()` to re-sync inputs. */
export function buildSettingsPanel(deps: SettingsPanelDeps): { el: HTMLElement; refresh(): void } {
  const s = deps.settings;
  const refreshers: (() => void)[] = [];
  const changed = () => {
    deps.onChange();
    refreshers.forEach((f) => f());
  };

  // ── Control builders ────────────────────────────────────────────────────

  const check = (label: string, get: () => boolean, set: (v: boolean) => void, hint?: string, enabled: () => boolean = () => true) => {
    const input = h("input", { type: "checkbox" });
    input.addEventListener("change", () => {
      set(input.checked);
      changed();
    });
    const row = h("label", { class: "row check" }, input, h("span", { class: "grow" }, label, hint ? h("small", {}, hint) : null));
    refreshers.push(() => {
      input.checked = get();
      input.disabled = !enabled();
      row.classList.toggle("disabled", !enabled());
    });
    return row;
  };

  const range = (label: string, min: number, max: number, step: number, get: () => number, set: (v: number) => void, fmt: (v: number) => string = String, enabled: () => boolean = () => true) => {
    const input = h("input", { type: "range", min, max, step });
    const out = h("output", {});
    input.addEventListener("input", () => {
      set(Number(input.value));
      out.textContent = fmt(get());
      deps.onChange();
    });
    const row = h("label", { class: "row range" }, h("span", { class: "grow" }, label), input, out);
    refreshers.push(() => {
      input.value = String(get());
      out.textContent = fmt(get());
      input.disabled = !enabled();
      row.classList.toggle("disabled", !enabled());
    });
    return row;
  };

  const colorRow = (label: string, get: () => string | null, set: (v: string | null) => void, fallback: () => string, resettable = true, enabled: () => boolean = () => true) => {
    const swatch = h("button", { type: "button", class: "swatch-btn", title: "Choose a color" });
    const reset = h("button", { class: "mini", type: "button", title: "Use the theme default" }, "Reset");
    swatch.addEventListener("click", () => {
      if (swatch.disabled) return;
      openColorPicker({
        anchor: swatch,
        initial: get() ?? fallback(),
        onChange: (hex) => {
          set(hex);
          deps.onChange();
          reset.hidden = !resettable;
          swatch.style.background = hex;
        },
      });
    });
    reset.addEventListener("click", () => {
      set(null);
      changed();
    });
    const row = h("div", { class: "row" }, h("span", { class: "grow" }, label), reset, swatch);
    refreshers.push(() => {
      swatch.style.background = get() ?? fallback();
      reset.hidden = !resettable || get() === null;
      swatch.disabled = !enabled();
      row.classList.toggle("disabled", !enabled());
    });
    return row;
  };

  const selectRow = <T extends string>(label: string, options: [T, string][], get: () => T, set: (v: T) => void) => {
    const sel = h("select", {}, ...options.map(([v, t]) => h("option", { value: v }, t)));
    sel.addEventListener("change", () => {
      set(sel.value as T);
      changed();
    });
    refreshers.push(() => (sel.value = get()));
    return h("label", { class: "row" }, h("span", { class: "grow" }, label), sel);
  };

  /** Two (or a few) exclusive choices as a segmented pill; labels stay short. */
  const segRow = <T extends string>(label: string, options: [T, string][], get: () => T, set: (v: T) => void) => {
    const group = h("div", { class: "segmented", role: "group", "aria-label": label });
    const buttons = options.map(([value, text]) => {
      const b = h("button", { type: "button", class: "seg" }, text);
      b.addEventListener("click", () => {
        set(value);
        changed();
      });
      group.append(b);
      return [value, b] as const;
    });
    refreshers.push(() => {
      for (const [value, b] of buttons) {
        b.classList.toggle("on", get() === value);
        b.setAttribute("aria-pressed", String(get() === value));
      }
    });
    return h("div", { class: "row" }, h("span", { class: "grow" }, label), group);
  };

  const SYMBOL_NAMES: Record<IconKind, string> = {
    none: "Blank",
    circle: "Circle",
    square: "Square",
    diamond: "Diamond",
    check: "Checkmark",
    x: "X",
    dash: "Dash",
    dot: "Dot",
  };

  /** A row of symbol buttons with the symbol's color picker on the same line. */
  const iconPicker = (
    label: string,
    target: "filled" | "empty",
    get: () => IconKind,
    set: (v: IconKind) => void,
    getColor: () => string | null,
    setColor: (v: string | null) => void,
  ) => {
    const wrap = h("div", { class: "icon-picker" });
    const autoInk = () => contrastOn(paletteFor(s, deps.getTheme())[target]);
    const buttons: [IconKind, HTMLButtonElement][] = ICON_KINDS.map((kind) => {
      const cv = h("canvas", { width: 44, height: 44 });
      cv.style.width = "22px";
      cv.style.height = "22px";
      const b = h("button", { type: "button", class: "icon-opt", title: SYMBOL_NAMES[kind], "aria-label": SYMBOL_NAMES[kind] }, cv);
      b.addEventListener("click", () => {
        set(kind);
        changed();
      });
      wrap.append(b);
      return [kind, b];
    });

    const color = h("button", { type: "button", class: "swatch-btn", title: "Symbol color" });
    const auto = h("button", { class: "mini", type: "button", title: "Back to automatic (contrasts with the cell)" }, "Reset");
    color.addEventListener("click", () => {
      openColorPicker({
        anchor: color,
        initial: getColor() ?? autoInk(),
        onChange: (hex) => {
          setColor(hex);
          deps.onChange();
          auto.hidden = false;
          paint();
        },
      });
    });
    auto.addEventListener("click", () => {
      setColor(null);
      changed();
    });
    wrap.append(h("span", { class: "icon-color" }, auto, color));

    const paint = () => {
      const pal = paletteFor(s, deps.getTheme());
      const ink = getColor();
      color.style.background = ink ?? autoInk();
      auto.hidden = ink === null;
      for (const [kind, b] of buttons) {
        const cv = b.firstElementChild as HTMLCanvasElement;
        const ctx = cv.getContext("2d")!;
        ctx.setTransform(2, 0, 0, 2, 0, 0);
        ctx.clearRect(0, 0, 22, 22);
        const bg = pal[target];
        ctx.fillStyle = bg;
        ctx.fillRect(1, 1, 20, 20);
        const lum = (parseInt(bg.slice(1, 3), 16) * 0.299 + parseInt(bg.slice(3, 5), 16) * 0.587 + parseInt(bg.slice(5, 7), 16) * 0.114) / 255;
        if (kind === "none") {
          ctx.strokeStyle = lum > 0.5 ? "#999" : "#888";
          ctx.setLineDash([2, 2]);
          ctx.strokeRect(4, 4, 14, 14);
        } else drawIcon(ctx, kind, 11, 11, 14, ink ?? (lum > 0.5 ? "#262626" : "#ffffff"));
        b.classList.toggle("on", get() === kind);
      }
    };
    refreshers.push(paint);
    return h("div", { class: "row stack" }, h("span", {}, label), wrap);
  };

  /**
   * Hue + saturation only — lightness is picked automatically per background (see
   * `huedInk`), so a custom label color can't end up unreadable the way a flat RGB
   * pick could (the label can land on either a filled-cell or a blank-cell background).
   */
  const hueSatRow = (label: string, get: () => { hue: number; sat: number } | null, set: (v: { hue: number; sat: number } | null) => void) => {
    const hueIn = h("input", { type: "range", min: 0, max: 359, step: 1 });
    const satIn = h("input", { type: "range", min: 0, max: 100, step: 1 });
    const hueOut = h("output", {});
    const satOut = h("output", {});
    const reset = h("button", { class: "mini", type: "button", title: "Back to automatic black/white" }, "Reset");
    const swL = h("span", { class: "hue-swatch on-light" }, "8");
    const swD = h("span", { class: "hue-swatch on-dark" }, "8");

    const commit = () => {
      set({ hue: Number(hueIn.value), sat: Number(satIn.value) });
      deps.onChange();
      paint();
    };
    hueIn.addEventListener("input", commit);
    satIn.addEventListener("input", commit);
    reset.addEventListener("click", () => {
      set(null);
      changed();
    });

    const paint = () => {
      const v = get();
      const shown = v ?? { hue: 210, sat: 70 }; // starting point the first time this is opened
      hueIn.value = String(shown.hue);
      satIn.value = String(shown.sat);
      hueOut.textContent = `${shown.hue}°`;
      satOut.textContent = `${shown.sat}%`;
      reset.hidden = v === null;
      const threshold = s.runLength.labelContrastThreshold;
      const inkOn = (bg: string) => (v ? huedInk(v.hue, v.sat, bg, threshold) : contrastOn(bg, threshold));
      swL.style.color = inkOn("#ffffff");
      swD.style.color = inkOn("#14151a");
    };
    refreshers.push(paint);

    return h(
      "div",
      { class: "row stack" },
      h("div", { class: "row" }, h("span", { class: "grow" }, label), h("span", { class: "hue-preview" }, swL, swD), reset),
      h("label", { class: "row range" }, h("span", { class: "grow" }, "Hue"), hueIn, hueOut),
      h("label", { class: "row range" }, h("span", { class: "grow" }, "Saturation"), satIn, satOut),
    );
  };

  const section = (title: string, ...body: HTMLElement[]) => {
    const d = h("details", { open: true }, h("summary", {}, title), ...body);
    return d;
  };

  // ── Sections ─────────────────────────────────────────────────────────────

  const appearance = section(
    "Appearance",
    selectRow<ThemeChoice>("Theme", [["system", "Match system"], ["light", "Light"], ["dark", "Dark"]], () => s.theme, (v) => (s.theme = v)),
    check("Hide timer", () => !s.showTimer, (v) => (s.showTimer = !v)),
    check(
      "Auto-pause when you leave",
      () => s.pauseOnAway,
      (v) => (s.pauseOnAway = v),
      "Switching tabs, minimizing, or switching to another app.",
      () => s.showTimer,
    ),
    colorRow("Unknown cell", () => s.colors.unknown, (v) => (s.colors.unknown = v), () => paletteFor(s, deps.getTheme()).unknown),
    colorRow("Filled cell", () => s.colors.filled, (v) => (s.colors.filled = v), () => paletteFor(s, deps.getTheme()).filled),
    colorRow("Empty (crossed) cell", () => s.colors.empty, (v) => (s.colors.empty = v), () => paletteFor(s, deps.getTheme()).empty),
    iconPicker("Filled symbol and color", "filled", () => s.icons.filled, (v) => (s.icons.filled = v), () => s.iconColors.filled, (v) => (s.iconColors.filled = v)),
    iconPicker("Empty symbol and color", "empty", () => s.icons.empty, (v) => (s.icons.empty = v), () => s.iconColors.empty, (v) => (s.iconColors.empty = v)),
    colorRow("Clue background", () => s.colors.clueBg, (v) => (s.colors.clueBg = v), () => paletteFor(s, deps.getTheme()).clueBg),
    colorRow("Line-sum background", () => s.colors.sumBg, (v) => (s.colors.sumBg = v), () => paletteFor(s, deps.getTheme()).sumBg),
  );

  const painting = section(
    "Mouse & zoom",
    segRow<"fill" | "mark">("Left button / tap", [["fill", "Fill"], ["mark", "Mark"]], () => s.primaryMode, (v) => (s.primaryMode = v)),
    h("p", { class: "note" }, "The right button (or holding Shift) does the other one. Press X to swap. On a touch screen a tap cycles a cell: unknown → filled → crossed → unknown."),
    check(
      "Cycle for mouse/pen too",
      () => s.cycleAnyInput,
      (v) => (s.cycleAnyInput = v),
      "Every click steps through the states like a touch tap does, instead of the left/right button split above (which is then unused, but kept for when you turn this back off).",
    ),
    segRow<"zoom" | "scroll">("Mouse wheel", [["zoom", "Zoom"], ["scroll", "Scroll"]], () => s.wheelMode, (v) => (s.wheelMode = v)),
    h("p", { class: "note" }, "Scroll moves the grid up and down (Shift: sideways) and needs Ctrl to zoom."),
    check("Lock the puzzle's position", () => s.lockFrame, (v) => (s.lockFrame = v), "Dragging then only scrolls the grid inside the frame; the frame stays where it is. (L)"),
    range("100% zoom is a cell size of", 12, 60, 1, () => s.zoomReference, (v) => (s.zoomReference = v), (v) => `${v} px`),
  );

  const assist = section(
    "Assistance",
    check("Dim fulfilled clues", () => s.assist.autoDim, (v) => (s.assist.autoDim = v), "Gray out clues that the grid already satisfies."),
    check(
      "…including a guess at the longest run",
      () => s.assist.autoDimGuess,
      (v) => (s.assist.autoDimGuess = v),
      "Dim (and cross both ends of) an isolated run before it's delimited, once its length already matches the biggest clue still open around it — the only length that can't still be mid-paint toward something bigger.",
      () => s.assist.autoDim,
    ),
    check("Auto-fill empty", () => s.assist.autoFillEmpty, (v) => (s.assist.autoFillEmpty = v), "Cross out the rest of a line once its clues are met."),
    check(
      "Auto-cross from edges",
      () => s.assist.autoCrossEdges,
      (v) => (s.assist.autoCrossEdges = v),
      "Cross out cells between two confirmed runs — including a confirmed central run and the edge (or another confirmed run) beyond it, once nothing else could fit between them.",
    ),
    check(
      "Auto-cross a matched run's open end",
      () => s.assist.autoCrossMatched,
      (v) => (s.assist.autoCrossMatched = v),
      "Once a run's length already matches its clue, cross the cell right past it — even before that side is otherwise edge-confirmed.",
    ),
    check("Show clue sums", () => s.assist.showSums, (v) => (s.assist.showSums = v), "The totals column on the right and row along the bottom."),
    check("Line sums include gaps", () => s.assist.clueSumsWithGaps, (v) => (s.assist.clueSumsWithGaps = v), "Shows the minimum span each line needs.", () => s.assist.showSums),
    check(
      "Line sums exclude completed clues",
      () => s.assist.clueSumsExcludeCompleted,
      (v) => (s.assist.clueSumsExcludeCompleted = v),
      "Drop clues the line has already fulfilled from its sum, so it stays a fair comparison against what's left.",
      () => s.assist.showSums,
    ),
    check("Axis-lock dragging", () => s.assist.axisLock, (v) => (s.assist.axisLock = v), "Drags paint a straight line, previewed until you release."),
  );

  const xOn = () => s.crosshair.enabled;
  const crosshair = section(
    "Crosshair highlight",
    check("Highlight the hovered row & column", () => s.crosshair.enabled, (v) => (s.crosshair.enabled = v)),
    check("Also highlight their clues", () => s.crosshair.headers, (v) => (s.crosshair.headers = v), undefined, xOn),
    check("Skip the hovered cell", () => s.crosshair.skipIntersection, (v) => (s.crosshair.skipIntersection = v), undefined, xOn),
    colorRow("Row color", () => s.crosshair.rowColor, (v) => v && (s.crosshair.rowColor = v), () => s.crosshair.rowColor, false, xOn),
    range("Row opacity", 0.05, 0.6, 0.01, () => s.crosshair.rowAlpha, (v) => (s.crosshair.rowAlpha = v), (v) => `${Math.round(v * 100)}%`, xOn),
    colorRow("Column color", () => s.crosshair.colColor, (v) => v && (s.crosshair.colColor = v), () => s.crosshair.colColor, false, xOn),
    range("Column opacity", 0.05, 0.6, 0.01, () => s.crosshair.colAlpha, (v) => (s.crosshair.colAlpha = v), (v) => `${Math.round(v * 100)}%`, xOn),
    check(
      "Highlight blank runs under the pointer",
      () => s.runLength.highlightEmptyRuns,
      (v) => (s.runLength.highlightEmptyRuns = v),
      "Instead of the crosshair: only used while it is switched off.",
      () => !s.crosshair.enabled,
    ),
  );

  const rl = s.runLength;
  const subcellGrid = h("div", { class: "subcells" });
  const subBtns: HTMLButtonElement[][] = [];
  for (let sr = 0; sr < 3; sr++) {
    subBtns.push([]);
    for (let sc = 0; sc < 3; sc++) {
      const b = h("button", { type: "button", class: "subcell" });
      b.addEventListener("click", () => {
        const order: SubcellKind[] = ["e", "h", "v"];
        rl.subcells[sr][sc] = order[(order.indexOf(rl.subcells[sr][sc]) + 1) % 3];
        changed();
      });
      subBtns[sr].push(b);
      subcellGrid.append(b);
    }
  }
  refreshers.push(() => {
    for (let sr = 0; sr < 3; sr++) {
      for (let sc = 0; sc < 3; sc++) {
        const k = rl.subcells[sr][sc];
        subBtns[sr][sc].textContent = k === "e" ? "" : k.toUpperCase();
        subBtns[sr][sc].dataset.kind = k;
      }
    }
  });

  const runLen = section(
    "Run-length indicators",
    h("p", { class: "note" }, "While hovering a run of filled (or blank) cells, show how long it is."),
    check("Show at run start", () => rl.showStart, (v) => (rl.showStart = v)),
    range("…if at least", 1, 20, 1, () => rl.startThreshold, (v) => (rl.startThreshold = v)),
    check("Show at run end", () => rl.showEnd, (v) => (rl.showEnd = v)),
    range("…if at least", 1, 20, 1, () => rl.endThreshold, (v) => (rl.endThreshold = v)),
    check("Show under the pointer", () => rl.showHover, (v) => (rl.showHover = v), "For long runs whose ends are off-screen."),
    range("…when at least this far from both ends", 1, 30, 1, () => rl.hoverThreshold, (v) => (rl.hoverThreshold = v)),
    check("Label the neighboring cell", () => rl.adjLabelEnabled, (v) => {
      rl.adjLabelEnabled = v;
      if (v) rl.fourDirLabels = false;
    }, "Puts the run length in the before the hovered cell instead."),
    check("…prefer the cell after (horizontal)", () => rl.adjLabelHPreferAfter, (v) => (rl.adjLabelHPreferAfter = v)),
    check("…prefer the cell after (vertical)", () => rl.adjLabelVPreferAfter, (v) => (rl.adjLabelVPreferAfter = v)),
    check("Four-direction counts", () => rl.fourDirLabels, (v) => {
      rl.fourDirLabels = v;
      if (v) rl.adjLabelEnabled = false;
    }, "Cells to the left / right / above / below the pointer, within its run."),
    h("div", { class: "row stack" }, h("span", {}, "Hover label placement (click: empty → H → V)"), subcellGrid),
    h("p", { class: "note" }, "Start/end labels always hug the outer edge of the run; this only positions the one under the pointer."),
    range("Label size", 6, 16, 1, () => rl.numSize, (v) => (rl.numSize = v), (v) => `${v}px`),
    range("Label contrast threshold", 0.2, 0.8, 0.01, () => rl.labelContrastThreshold, (v) => (rl.labelContrastThreshold = v), (v) => `${Math.round(v * 100)}%`),
    h("p", { class: "note" }, "Cell backgrounds lighter than this get dark labels; darker ones get light labels. Applies to Auto and to the custom colors below."),
    hueSatRow("Horizontal label color", () => rl.labelHColor, (v) => (rl.labelHColor = v)),
    hueSatRow("Vertical label color", () => rl.labelVColor, (v) => (rl.labelVColor = v)),
  );

  const keys = section(
    "Keyboard",
    selectRow("Fullscreen key", FULLSCREEN_KEYS.map((k) => [k, k.length === 1 ? k.toUpperCase() : k] as [string, string]), () => s.fullscreenKey, (v) => (s.fullscreenKey = v)),
    h(
      "dl",
      { class: "keys" },
      ...(
        [
          ["Ctrl+Z / Ctrl+Y", "Undo / redo"],
          ["X", "Swap what the left button does (fill ↔ mark)"],
          ["L", "Lock / unlock the puzzle's position"],
          ["T", "Enter trial (again: nested tier)"],
          ["A / R", "Accept / reject trial tier"],
          ["+ / − / 0", "Zoom in / out / fit"],
          ["Mouse wheel", "Zoom, or scroll the grid (see Mouse & zoom); Ctrl always zooms, Shift scrolls sideways"],
          ["Drag outside the grid", "Pan (clue strips, sums, blank space)"],
          ["Space + drag", "Pan from inside the grid"],
["Right-click / Shift", "Do the other action (mark ↔ fill)"],
          ["Click a clue", "Dim / undim it"],
          ["Fullscreen key / F11", "Toggle the browser's fullscreen"],
        ] as const
      ).flatMap(([k, d]) => [h("dt", {}, k), h("dd", {}, d)]),
    ),
  );

  const resetBtn = h("button", { type: "button", class: "btn danger" }, "Reset all settings");
  resetBtn.addEventListener("click", () => {
    closeColorPicker(); // it may be anchored to a swatch this rebuilds
    deps.onReset();
    refreshers.forEach((f) => f());
  });

  const el = h(
    "div",
    { class: "settings-body" },
    painting,
    appearance,
    assist,
    crosshair,
    runLen,
    keys,
    h("div", { class: "row" }, resetBtn),
  );

  refreshers.forEach((f) => f());
  return { el, refresh: () => refreshers.forEach((f) => f()) };
}

