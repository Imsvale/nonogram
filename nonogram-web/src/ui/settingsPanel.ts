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

  const check = (label: string, get: () => boolean, set: (v: boolean) => void, hint?: string) => {
    const input = h("input", { type: "checkbox" });
    input.addEventListener("change", () => {
      set(input.checked);
      changed();
    });
    refreshers.push(() => (input.checked = get()));
    return h("label", { class: "row check" }, input, h("span", { class: "grow" }, label, hint ? h("small", {}, hint) : null));
  };

  const range = (label: string, min: number, max: number, step: number, get: () => number, set: (v: number) => void, fmt: (v: number) => string = String) => {
    const input = h("input", { type: "range", min, max, step });
    const out = h("output", {});
    input.addEventListener("input", () => {
      set(Number(input.value));
      out.textContent = fmt(get());
      deps.onChange();
    });
    refreshers.push(() => {
      input.value = String(get());
      out.textContent = fmt(get());
    });
    return h("label", { class: "row range" }, h("span", { class: "grow" }, label), input, out);
  };

  const colorRow = (label: string, get: () => string | null, set: (v: string | null) => void, fallback: () => string, resettable = true) => {
    const input = h("input", { type: "color" });
    const reset = h("button", { class: "mini", type: "button", title: "Use the theme default" }, "Reset");
    input.addEventListener("input", () => {
      set(input.value);
      deps.onChange();
      reset.hidden = !resettable;
    });
    reset.addEventListener("click", () => {
      set(null);
      changed();
    });
    refreshers.push(() => {
      input.value = get() ?? fallback();
      reset.hidden = !resettable || get() === null;
    });
    return h("div", { class: "row" }, h("span", { class: "grow" }, label), reset, input);
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

  const iconPicker = (label: string, target: "filled" | "empty", get: () => IconKind, set: (v: IconKind) => void) => {
    const wrap = h("div", { class: "icon-picker" });
    const buttons: [IconKind, HTMLButtonElement][] = ICON_KINDS.map((kind) => {
      const cv = h("canvas", { width: 44, height: 44 });
      cv.style.width = "22px";
      cv.style.height = "22px";
      const b = h("button", { type: "button", class: "icon-opt", title: kind === "none" ? "No marker" : kind }, cv);
      b.addEventListener("click", () => {
        set(kind);
        changed();
      });
      wrap.append(b);
      return [kind, b];
    });
    const paint = () => {
      const pal = paletteFor(s, deps.getTheme());
      for (const [kind, b] of buttons) {
        const cv = b.firstElementChild as HTMLCanvasElement;
        const ctx = cv.getContext("2d")!;
        ctx.setTransform(2, 0, 0, 2, 0, 0);
        ctx.clearRect(0, 0, 22, 22);
        const bg = target === "filled" ? pal.filled : pal.empty;
        ctx.fillStyle = bg;
        ctx.fillRect(1, 1, 20, 20);
        const lum = (parseInt(bg.slice(1, 3), 16) * 0.299 + parseInt(bg.slice(3, 5), 16) * 0.587 + parseInt(bg.slice(5, 7), 16) * 0.114) / 255;
        if (kind === "none") {
          ctx.strokeStyle = lum > 0.5 ? "#999" : "#888";
          ctx.setLineDash([2, 2]);
          ctx.strokeRect(4, 4, 14, 14);
        } else drawIcon(ctx, kind, 11, 11, 14, lum > 0.5 ? "#262626" : "#ffffff");
        b.classList.toggle("on", get() === kind);
      }
    };
    refreshers.push(paint);
    return h("div", { class: "row stack" }, h("span", {}, label), wrap);
  };

  const section = (title: string, ...body: HTMLElement[]) => {
    const d = h("details", { open: true }, h("summary", {}, title), ...body);
    return d;
  };

  // ── Sections ─────────────────────────────────────────────────────────────

  const appearance = section(
    "Appearance",
    selectRow<ThemeChoice>("Theme", [["system", "Match system"], ["light", "Light"], ["dark", "Dark"]], () => s.theme, (v) => (s.theme = v)),
    colorRow("Unknown cell", () => s.colors.unknown, (v) => (s.colors.unknown = v), () => paletteFor(s, deps.getTheme()).unknown),
    colorRow("Filled cell", () => s.colors.filled, (v) => (s.colors.filled = v), () => paletteFor(s, deps.getTheme()).filled),
    colorRow("Empty (crossed) cell", () => s.colors.empty, (v) => (s.colors.empty = v), () => paletteFor(s, deps.getTheme()).empty),
    iconPicker("Filled marker", "filled", () => s.icons.filled, (v) => (s.icons.filled = v)),
    iconPicker("Empty marker", "empty", () => s.icons.empty, (v) => (s.icons.empty = v)),
    colorRow("Clue background", () => s.colors.clueBg, (v) => (s.colors.clueBg = v), () => paletteFor(s, deps.getTheme()).clueBg),
    colorRow("Line-sum background", () => s.colors.sumBg, (v) => (s.colors.sumBg = v), () => paletteFor(s, deps.getTheme()).sumBg),
  );

  const painting = section(
    "Mouse & zoom",
    selectRow<"fill" | "mark">("Left button / tap", [["fill", "Fills cells"], ["mark", "Marks cells empty"]], () => s.primaryMode, (v) => (s.primaryMode = v)),
    h("p", { class: "note" }, "The right button (or holding Shift) does the other one. Press X to swap."),
    range("100% zoom is a cell size of", 12, 60, 1, () => s.zoomReference, (v) => (s.zoomReference = v), (v) => `${v} px`),
  );

  const assist = section(
    "Assistance",
    check("Dim fulfilled clues", () => s.assist.autoDim, (v) => (s.assist.autoDim = v), "Grey out clues that the grid already satisfies."),
    check("Auto-fill empty", () => s.assist.autoFillEmpty, (v) => (s.assist.autoFillEmpty = v), "Cross out the rest of a line once its clues are met."),
    check("Auto-cross from edges", () => s.assist.autoCrossEdges, (v) => (s.assist.autoCrossEdges = v), "Cross out cells that lie between edge-confirmed runs."),
    check("Line sums include gaps", () => s.assist.clueSumsWithGaps, (v) => (s.assist.clueSumsWithGaps = v), "Shows the minimum span each line needs."),
    check("Axis-lock dragging", () => s.assist.axisLock, (v) => (s.assist.axisLock = v), "Drags paint a straight line, previewed until you release."),
    check("Start timer on first move", () => s.autoStartTimer, (v) => (s.autoStartTimer = v)),
  );

  const crosshair = section(
    "Crosshair highlight",
    check("Highlight hovered row & column", () => s.crosshair.enabled, (v) => (s.crosshair.enabled = v)),
    check("Skip the hovered cell", () => s.crosshair.skipIntersection, (v) => (s.crosshair.skipIntersection = v)),
    colorRow("Row colour", () => s.crosshair.rowColor, (v) => v && (s.crosshair.rowColor = v), () => s.crosshair.rowColor, false),
    range("Row opacity", 0.05, 0.6, 0.01, () => s.crosshair.rowAlpha, (v) => (s.crosshair.rowAlpha = v), (v) => `${Math.round(v * 100)}%`),
    colorRow("Column colour", () => s.crosshair.colColor, (v) => v && (s.crosshair.colColor = v), () => s.crosshair.colColor, false),
    range("Column opacity", 0.05, 0.6, 0.01, () => s.crosshair.colAlpha, (v) => (s.crosshair.colAlpha = v), (v) => `${Math.round(v * 100)}%`),
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
    range("…when at least this far from either end", 0, 10, 1, () => rl.hoverThreshold, (v) => (rl.hoverThreshold = v)),
    check("Label the neighbouring cell", () => rl.adjLabelEnabled, (v) => {
      rl.adjLabelEnabled = v;
      if (v) rl.fourDirLabels = false;
    }, "Puts the run length just beside the pointer instead of on it."),
    check("…prefer the cell after (horizontal)", () => rl.adjLabelHPreferAfter, (v) => (rl.adjLabelHPreferAfter = v)),
    check("…prefer the cell after (vertical)", () => rl.adjLabelVPreferAfter, (v) => (rl.adjLabelVPreferAfter = v)),
    check("Four-direction counts", () => rl.fourDirLabels, (v) => {
      rl.fourDirLabels = v;
      if (v) rl.adjLabelEnabled = false;
    }, "Cells to the left / right / above / below the pointer, within its run."),
    h("div", { class: "row stack" }, h("span", {}, "Label placement (click: empty → H → V)"), subcellGrid),
    range("Label size", 6, 16, 1, () => rl.numSize, (v) => (rl.numSize = v), (v) => `${v}px`),
    colorRow("Horizontal label colour", () => rl.labelHColor, (v) => (rl.labelHColor = v), () => "#66b8ff"),
    colorRow("Vertical label colour", () => rl.labelVColor, (v) => (rl.labelVColor = v), () => "#66b8ff"),
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
          ["T", "Enter trial (again: nested tier)"],
          ["A / R", "Accept / reject trial tier"],
          ["+ / − / 0", "Zoom in / out / fit"],
          ["Wheel", "Zoom"],
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

