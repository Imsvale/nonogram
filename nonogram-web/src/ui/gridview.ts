import type { Game, PaintAction } from "../state/game";
import { paletteFor, type ResolvedTheme, type Settings } from "../state/settings";
import { computeLayout, fitCellSize, hitTest, viewportOk, type Hit, type Layout } from "./geometry";
import { minimapGeometry, NO_HOVER, render, type HoverState } from "./render";

export interface GridViewDeps {
  getSettings(): Settings;
  getTheme(): ResolvedTheme;
  /** The bottom-right "sums include gaps" corner was clicked. */
  onToggleSumGaps(): void;
}

/** One zoom step: a +/- button press, a keyboard +/-, or one mouse-wheel notch with Ctrl held. */
const ZOOM_STEP = 1.18;

/** Pointer travel (px) before a press on a clue becomes a pan instead of a click. */
const PAN_SLOP = 4;

export const MIN_CELL = 10;
export const MAX_CELL = 96;
const HINTS: Partial<Record<Hit["kind"], string>> = {
  colClue: "Click to dim / undim this clue (drag to pan)",
  rowClue: "Click to dim / undim this clue (drag to pan)",
  colHead: "Drag to pan",
  rowHead: "Drag to pan",
  minimap: "Click or drag to move around the puzzle",
  sumToggle: "Click to include gaps in line sums",
};

interface Pt {
  x: number;
  y: number;
}

/**
 * Owns the canvas: viewport (cell size + pan), pointer/wheel/touch input and
 * render scheduling. Puzzle state lives in `Game`; this only translates input
 * into Game calls.
 */
export class GridView {
  private game: Game | null = null;
  private unsub: (() => void) | null = null;
  private ctx: CanvasRenderingContext2D;
  private W = 0;
  private H = 0;
  private dpr = 1;
  private C = 26;
  private panX = 0;
  private panY = 0;
  /** While true, the cell size tracks the window (until the user zooms). */
  autoFit = true;
  spaceHeld = false;
  /** Where the whole frame has been dragged to, relative to centred (see computeLayout). */
  private offX = 0;
  private offY = 0;

  private hover: HoverState = NO_HOVER;
  private hoverKey = "";
  private pointers = new Map<number, Pt & { type: string }>();
  private drag: "none" | "stroke" | "pan" | "minimap" = "none";
  /** `click` runs on release if the pointer never moved (a clue that was clicked rather than dragged). */
  private panStart: {
    px: number;
    py: number;
    ox: number;
    oy: number;
    x: number;
    y: number;
    /** Per axis: true = scroll the cells inside the frame, false = move the frame itself. */
    scrollX: boolean;
    scrollY: boolean;
    moved: boolean;
    click: (() => void) | null;
  } | null = null;
  private gesture: { d0: number; C0: number; wx: number; wy: number } | null = null;
  private rafId = 0;
  private autoScrollId = 0;
  private lastPointer: Pt | null = null;

  /** Fired when a stroke ends with a change, or a clue dim toggles — the app persists. */
  onUserChange: () => void = () => {};
  /** Fired when the cell size changes (for the zoom readout). */
  onViewChange: () => void = () => {};
  /** Fired after each layout is computed, so the footer can follow the frame. */
  onLayout: (L: Layout) => void = () => {};

  constructor(
    private canvas: HTMLCanvasElement,
    private host: HTMLElement,
    private deps: GridViewDeps,
  ) {
    const ctx = canvas.getContext("2d");
    if (!ctx) throw new Error("Canvas 2D is not available");
    this.ctx = ctx;

    canvas.addEventListener("pointerdown", (e) => this.onPointerDown(e));
    canvas.addEventListener("pointermove", (e) => this.onPointerMove(e));
    canvas.addEventListener("pointerup", (e) => this.onPointerUp(e));
    canvas.addEventListener("pointercancel", (e) => this.onPointerUp(e));
    canvas.addEventListener("pointerleave", (e) => {
      if (e.pointerType !== "touch" && this.drag === "none") this.setHover(NO_HOVER);
    });
    canvas.addEventListener("wheel", (e) => this.onWheel(e), { passive: false });
    canvas.addEventListener("contextmenu", (e) => e.preventDefault());

    new ResizeObserver(() => this.resize()).observe(host);
    window.addEventListener("keydown", (e) => this.onKey(e, true));
    window.addEventListener("keyup", (e) => this.onKey(e, false));
    window.addEventListener("blur", () => {
      this.spaceHeld = false;
      this.updateCursor();
    });
    this.resize();
  }

  // ── Wiring ────────────────────────────────────────────────────────────────

  setGame(game: Game | null): void {
    this.unsub?.();
    this.unsub = null;
    this.game = game;
    this.hover = NO_HOVER;
    this.hoverKey = "";
    this.drag = "none";
    this.pointers.clear();
    this.panX = this.panY = 0;
    this.offX = this.offY = 0;
    this.autoFit = true;
    if (game) {
      this.unsub = game.subscribe(() => this.requestRender());
      this.C = fitCellSize(game.puzzle, this.W, this.H);
    }
    this.requestRender();
    this.onViewChange();
  }

  get cellSize(): number {
    return this.C;
  }

  layout(): Layout | null {
    if (!this.game) return null;
    const L = computeLayout(this.game.puzzle, this.C, this.W, this.H, this.panX, this.panY, this.offX, this.offY);
    this.panX = L.panX;
    this.panY = L.panY;
    this.offX = L.offX;
    this.offY = L.offY;
    return L;
  }

  requestRender(): void {
    if (this.rafId) return;
    this.rafId = requestAnimationFrame(() => {
      this.rafId = 0;
      this.draw();
    });
  }

  private resize(): void {
    const rect = this.host.getBoundingClientRect();
    this.W = Math.max(1, Math.floor(rect.width));
    this.H = Math.max(1, Math.floor(rect.height));
    this.dpr = window.devicePixelRatio || 1;
    this.canvas.width = Math.round(this.W * this.dpr);
    this.canvas.height = Math.round(this.H * this.dpr);
    this.canvas.style.width = `${this.W}px`;
    this.canvas.style.height = `${this.H}px`;
    if (this.game && this.autoFit) this.C = fitCellSize(this.game.puzzle, this.W, this.H);
    this.requestRender();
    this.onViewChange();
  }

  private draw(): void {
    const game = this.game;
    if (!game) return;
    // The device pixel ratio can change (zooming the page, moving between monitors).
    const dpr = window.devicePixelRatio || 1;
    if (dpr !== this.dpr) {
      this.dpr = dpr;
      this.canvas.width = Math.round(this.W * dpr);
      this.canvas.height = Math.round(this.H * dpr);
    }
    const settings = this.deps.getSettings();
    const theme = this.deps.getTheme();
    const L = this.layout()!;
    this.onLayout(L);
    render({
      ctx: this.ctx,
      dpr: this.dpr,
      layout: L,
      puzzle: game.puzzle,
      grid: game.displayGrid(),
      tierMap: game.tierMap(),
      trial: game.trial,
      derived: settings.assist.autoDim ? game.derived() : null,
      dimRows: game.dimRows,
      dimCols: game.dimCols,
      settings,
      theme,
      palette: paletteFor(settings, theme),
      hover: this.hover,
    });
  }

  // ── View control ──────────────────────────────────────────────────────────

  fit(): void {
    if (!this.game) return;
    this.autoFit = true;
    this.C = fitCellSize(this.game.puzzle, this.W, this.H);
    this.panX = this.panY = 0;
    this.offX = this.offY = 0;
    this.requestRender();
    this.onViewChange();
  }

  /** Zoom to an exact cell size (e.g. back to the 100% reference), keeping the view centre fixed. */
  setCellSize(px: number): void {
    const target = Math.round(px);
    if (!this.game || target === this.C) return;
    this.zoomBy(target / this.C);
  }

  /** Largest cell size the current puzzle + window allow (the frozen clue strips limit it). */
  get maxCellSize(): number {
    if (!this.game) return MAX_CELL;
    let c = MAX_CELL;
    while (c > this.C && !viewportOk(this.game.puzzle, c, this.W, this.H)) c--;
    return c;
  }

  zoomIn(): void {
    this.zoomBy(ZOOM_STEP);
  }
  zoomOut(): void {
    this.zoomBy(1 / ZOOM_STEP);
  }

  /** Multiply the cell size, keeping the point (sx, sy) — default: view centre — fixed. */
  zoomBy(factor: number, sx?: number, sy?: number): void {
    const L = this.layout();
    if (!this.game || !L) return;
    const px = sx ?? L.ox + L.cw / 2;
    const py = sy ?? L.oy + L.ch / 2;
    let next = Math.round(this.C * factor);
    if (next === this.C) next += factor > 1 ? 1 : -1;
    next = this.limitZoom(next);
    if (next === this.C) return;
    const wx = (px - L.ox + L.panX) / this.C;
    const wy = (py - L.oy + L.panY) / this.C;
    this.setCellSizeKeeping(next, wx, wy, px, py);
    this.autoFit = false;
  }

  /** Clamp a requested cell size to the allowed range and to what the window can usefully show. */
  private limitZoom(next: number): number {
    next = Math.min(MAX_CELL, Math.max(MIN_CELL, next));
    if (!this.game) return next;
    // Only zoom-in is restricted; never trap the user at a size they can't leave.
    while (next > this.C && !viewportOk(this.game.puzzle, next, this.W, this.H)) next--;
    return next;
  }

  private setCellSizeKeeping(next: number, wx: number, wy: number, px: number, py: number): void {
    if (!this.game) return;
    this.C = next;
    const L1 = computeLayout(this.game.puzzle, next, this.W, this.H, 0, 0, this.offX, this.offY);
    this.panX = wx * next - (px - L1.ox);
    this.panY = wy * next - (py - L1.oy);
    this.requestRender();
    this.onViewChange();
  }

  private panBy(dx: number, dy: number): void {
    this.panX += dx;
    this.panY += dy;
    this.requestRender();
  }

  // ── Pointer input ─────────────────────────────────────────────────────────

  private local(e: { clientX: number; clientY: number }): Pt {
    const r = this.canvas.getBoundingClientRect();
    return { x: e.clientX - r.left, y: e.clientY - r.top };
  }

  private paintAction(e: PointerEvent): PaintAction {
    const alt = e.button === 2 || e.shiftKey;
    const primary = this.deps.getSettings().primaryMode;
    return alt ? (primary === "fill" ? "mark" : "fill") : primary;
  }

  private onPointerDown(e: PointerEvent): void {
    const game = this.game;
    if (!game) return;
    const pt = this.local(e);
    this.pointers.set(e.pointerId, { ...pt, type: e.pointerType });

    if (this.pointers.size === 2) {
      this.beginGesture();
      return;
    }
    if (this.pointers.size > 2) return;

    this.canvas.setPointerCapture(e.pointerId);
    const L = this.layout()!;
    const hit = hitTest(L, game.puzzle, pt.x, pt.y);
    this.lastPointer = pt;

    if (e.button === 1 || (e.button === 0 && this.spaceHeld)) {
      e.preventDefault();
      this.beginPanDrag(pt, null);
      return;
    }
    if (e.button !== 0 && e.button !== 2) return;

    switch (hit.kind) {
      case "cell": {
        const settings = this.deps.getSettings();
        if (settings.autoStartTimer && !game.timerRunning && !game.solvedNow) game.startTimer();
        game.startStroke(hit.r, hit.c, this.paintAction(e), settings.assist.axisLock);
        this.drag = "stroke";
        this.setHover(this.hoverFor(hit));
        break;
      }
      case "minimap":
        if (e.button === 0) {
          this.drag = "minimap";
          this.minimapJump(hit.x, hit.y);
        }
        break;
      default:
        // Anywhere outside the paintable grid, dragging pans. A press that never
        // moves keeps its old meaning (dim a clue / toggle gaps in sums).
        if (e.button === 0) {
          const click =
            hit.kind === "colClue"
              ? () => {
                  game.toggleDim(true, hit.c, hit.i);
                  this.onUserChange();
                }
              : hit.kind === "rowClue"
                ? () => {
                    game.toggleDim(false, hit.r, hit.i);
                    this.onUserChange();
                  }
                : hit.kind === "sumToggle"
                  ? () => this.deps.onToggleSumGaps()
                  : null;
          this.beginPanDrag(pt, click);
        }
        break;
    }
  }

  private onPointerMove(e: PointerEvent): void {
    const game = this.game;
    if (!game) return;
    const pt = this.local(e);
    if (this.pointers.has(e.pointerId)) this.pointers.set(e.pointerId, { ...pt, type: e.pointerType });
    this.lastPointer = pt;

    if (this.gesture) {
      this.updateGesture();
      return;
    }

    switch (this.drag) {
      case "stroke": {
        const L = this.layout()!;
        const [r, c] = this.clampedCell(L, pt.x, pt.y);
        game.strokeTo(r, c);
        this.setHover({ ...NO_HOVER, cell: [r, c] });
        this.maybeAutoScroll();
        return;
      }
      case "pan":
        this.dragPanTo(pt);
        return;
      case "minimap": {
        const L = this.layout()!;
        this.minimapJump(pt.x - L.originX, pt.y - L.originY);
        return;
      }
      default:
        break;
    }

    if (e.pointerType === "touch") return;
    const L = this.layout()!;
    const hit = hitTest(L, game.puzzle, pt.x, pt.y);
    this.setHover(this.hoverFor(hit));
    this.updateCursor(hit);
    this.canvas.title = HINTS[hit.kind] ?? "";
  }

  private onPointerUp(e: PointerEvent): void {
    this.pointers.delete(e.pointerId);
    if (this.canvas.hasPointerCapture(e.pointerId)) this.canvas.releasePointerCapture(e.pointerId);

    if (this.gesture) {
      if (this.pointers.size < 2) this.gesture = null;
      if (this.pointers.size === 0) this.drag = "none";
      return;
    }
    if (this.pointers.size > 0) return;

    if (this.drag === "stroke" && this.game) {
      const changed = this.game.endStroke();
      if (changed) this.onUserChange();
    }
    if (this.drag === "pan" && this.panStart && !this.panStart.moved) this.panStart.click?.();
    this.drag = "none";
    this.panStart = null;
    this.stopAutoScroll();
    if (e.pointerType === "touch") this.setHover(NO_HOVER);
    this.updateCursor();
  }

  private hoverFor(hit: Hit): HoverState {
    switch (hit.kind) {
      case "cell":
        return { ...NO_HOVER, cell: [hit.r, hit.c] };
      case "colClue":
        return { ...NO_HOVER, colHead: hit.c, clue: { isCol: true, line: hit.c, idx: hit.i } };
      case "rowClue":
        return { ...NO_HOVER, rowHead: hit.r, clue: { isCol: false, line: hit.r, idx: hit.i } };
      case "colHead":
        return { ...NO_HOVER, colHead: hit.c };
      case "rowHead":
        return { ...NO_HOVER, rowHead: hit.r };
      case "sumToggle":
        return { ...NO_HOVER, sumToggle: true };
      default:
        return NO_HOVER;
    }
  }

  private setHover(h: HoverState): void {
    const key = JSON.stringify(h);
    if (key === this.hoverKey) return;
    this.hoverKey = key;
    this.hover = h;
    this.requestRender();
  }

  private updateCursor(hit?: Hit): void {
    let cursor = "default";
    if (this.drag === "pan") cursor = "grabbing";
    else if (this.spaceHeld) cursor = "grab";
    else if (hit) {
      if (hit.kind === "cell") cursor = "cell";
      else if (hit.kind === "colClue" || hit.kind === "rowClue" || hit.kind === "minimap" || hit.kind === "sumToggle") cursor = "pointer";
      else if (hit.kind === "colHead" || hit.kind === "rowHead" || hit.kind === "none") cursor = "grab";
    }
    this.canvas.style.cursor = cursor;
  }

  private clampedCell(L: Layout, x: number, y: number): [number, number] {
    const p = this.game!.puzzle;
    const c = Math.floor((x - L.ox + L.panX) / L.m.C);
    const r = Math.floor((y - L.oy + L.panY) / L.m.C);
    return [Math.min(p.height - 1, Math.max(0, r)), Math.min(p.width - 1, Math.max(0, c))];
  }

  // ── Panning, minimap, edge auto-scroll ────────────────────────────────────

  private beginPanDrag(pt: Pt, click: (() => void) | null): void {
    const L = this.layout();
    this.drag = "pan";
    // Each axis either scrolls the cells (if the puzzle overflows the window there)
    // or moves the whole frame around the window (if there is slack).
    this.panStart = {
      px: this.panX,
      py: this.panY,
      ox: this.offX,
      oy: this.offY,
      x: pt.x,
      y: pt.y,
      scrollX: !!L && L.fullW > L.cw,
      scrollY: !!L && L.fullH > L.ch,
      // Space / middle-button pans start moving immediately; clue presses wait for a small slop.
      moved: click === null,
      click,
    };
    this.updateCursor();
  }

  private dragPanTo(pt: Pt): void {
    const s = this.panStart;
    if (!s) return;
    const dx = pt.x - s.x;
    const dy = pt.y - s.y;
    if (!s.moved && Math.hypot(dx, dy) < PAN_SLOP) return;
    s.moved = true;
    if (s.scrollX) this.panX = s.px - dx;
    else this.offX = s.ox + dx;
    if (s.scrollY) this.panY = s.py - dy;
    else this.offY = s.oy + dy;
    this.requestRender();
  }

  /** Centre the viewport on a point given in minimap-area coordinates. */
  private minimapJump(mx: number, my: number): void {
    const L = this.layout();
    if (!L || !this.game) return;
    const g = minimapGeometry(L, this.game.puzzle);
    const wx = ((mx - g.px) / g.mw) * L.fullW;
    const wy = ((my - g.py) / g.mh) * L.fullH;
    this.panX = wx - L.cw / 2;
    this.panY = wy - L.ch / 2;
    this.requestRender();
  }

  private maybeAutoScroll(): void {
    if (this.autoScrollId || this.drag !== "stroke") return;
    const tick = () => {
      this.autoScrollId = 0;
      if (this.drag !== "stroke" || !this.lastPointer || !this.game) return;
      const L = this.layout()!;
      const { x, y } = this.lastPointer;
      const edge = (v: number, lo: number, hi: number) => (v < lo ? v - lo : v > hi ? v - hi : 0);
      const dx = edge(x, L.ox, L.ox + L.cw);
      const dy = edge(y, L.oy, L.oy + L.ch);
      if (dx === 0 && dy === 0) return;
      const speed = (d: number) => Math.sign(d) * Math.min(28, 2 + Math.abs(d) * 0.35);
      this.panBy(dx ? speed(dx) : 0, dy ? speed(dy) : 0);
      const L2 = this.layout()!;
      const [r, c] = this.clampedCell(L2, x, y);
      this.game.strokeTo(r, c);
      this.autoScrollId = requestAnimationFrame(tick);
    };
    this.autoScrollId = requestAnimationFrame(tick);
  }

  private stopAutoScroll(): void {
    if (this.autoScrollId) cancelAnimationFrame(this.autoScrollId);
    this.autoScrollId = 0;
  }

  // ── Wheel, touch gestures, keys ───────────────────────────────────────────

  private onWheel(e: WheelEvent): void {
    if (!this.game) return;
    e.preventDefault();
    const pt = this.local(e);
    const unit = e.deltaMode === 1 ? 16 : e.deltaMode === 2 ? this.H : 1;
    const dy = e.deltaY * unit;
    if (dy === 0) return;
    // The wheel zooms (panning is by dragging). A mouse notch is exactly one zoom
    // step, like the +/- buttons; the many small deltas of a trackpad zoom smoothly.
    if (e.deltaMode !== 0 || Math.abs(dy) >= 50) this.zoomBy(dy < 0 ? ZOOM_STEP : 1 / ZOOM_STEP, pt.x, pt.y);
    else this.zoomBy(Math.exp(-dy / 100), pt.x, pt.y);
  }

  private beginGesture(): void {
    const game = this.game;
    if (!game) return;
    // A second finger means "navigate", not "paint": undo whatever the first one started.
    if (this.drag === "stroke") game.cancelStroke();
    this.stopAutoScroll();
    this.drag = "none";
    const [a, b] = [...this.pointers.values()];
    const L = this.layout()!;
    const mid = { x: (a.x + b.x) / 2, y: (a.y + b.y) / 2 };
    this.gesture = {
      d0: Math.max(1, Math.hypot(a.x - b.x, a.y - b.y)),
      C0: this.C,
      wx: (mid.x - L.ox + L.panX) / this.C,
      wy: (mid.y - L.oy + L.panY) / this.C,
    };
    this.setHover(NO_HOVER);
  }

  private updateGesture(): void {
    const g = this.gesture;
    if (!g || !this.game || this.pointers.size < 2) return;
    const [a, b] = [...this.pointers.values()];
    const mid = { x: (a.x + b.x) / 2, y: (a.y + b.y) / 2 };
    const d = Math.hypot(a.x - b.x, a.y - b.y);
    const next = this.limitZoom(Math.round(g.C0 * (d / g.d0)));
    this.autoFit = false;
    this.setCellSizeKeeping(next, g.wx, g.wy, mid.x, mid.y);
  }

  private onKey(e: KeyboardEvent, down: boolean): void {
    if (e.code !== "Space") return;
    const t = e.target as HTMLElement | null;
    if (down && t && /^(INPUT|TEXTAREA|SELECT|BUTTON)$/.test(t.tagName)) return;
    if (down && t?.isContentEditable) return;
    if (!this.game) return;
    if (down) e.preventDefault();
    if (this.spaceHeld !== down) {
      this.spaceHeld = down;
      this.updateCursor();
    }
  }

  destroy(): void {
    this.unsub?.();
  }
}
