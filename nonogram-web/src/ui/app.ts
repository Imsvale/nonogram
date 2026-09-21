import { puzzleToNative, puzzleToPzprv3 } from "../core/export";
import { trivialInvalidReason } from "../core/lines";
import { importFromText } from "../core/parse";
import { puzzleToPuzzlinkUrl } from "../core/puzzlink";
import { SAMPLES } from "../core/samples";
import { buildShareLink, puzzleFromLocation, puzzleHash } from "../core/share";
import type { Puzzle } from "../core/types";
import { Game } from "../state/game";
import { deleteProgress, listRecent, loadProgress, progressFraction, resumeCandidate, saveProgress, setLastOpen } from "../state/progress";
import { defaultSettings, loadSettings, resolveTheme, saveSettings, saveSettingsNow, type ResolvedTheme, type Settings } from "../state/settings";
import { append, clear, copyText, downloadText, formatTime, h, icon } from "./dom";
import { HEAVY_LINE_MIN_CELL } from "./geometry";
import { GridView, MAX_CELL, MIN_CELL } from "./gridview";
import { buildSettingsPanel } from "./settingsPanel";

const APP_NAME = "Nonogram";

export interface AppHandle {
  gridView: GridView;
  game(): Game | null;
  settings(): Settings;
}

export function startApp(root: HTMLElement): AppHandle {
  let settings = loadSettings();
  let theme: ResolvedTheme = resolveTheme(settings.theme);
  let game: Game | null = null;
  let unsubGame: (() => void) | null = null;
  let wasSolved = false;
  let saveTimer: ReturnType<typeof setTimeout> | undefined;
  let focusMode = false;

  // ── Static DOM ───────────────────────────────────────────────────────────

  const canvas = h("canvas", { id: "grid", role: "img", "aria-label": "Nonogram grid", tabindex: "-1" });
  const canvasHost = h("div", { id: "canvasHost" }, canvas);
  const landing = h("section", { id: "landing" });
  // Overlay (never in flow): an in-flow banner would shift the grid under the
  // pointer when it appears mid-stroke, e.g. on the stroke that solves the puzzle.
  const notice = h("div", { id: "notice", hidden: true, role: "status" });
  const warnEl = h("span", { class: "warn-pill", hidden: true });
  const toastEl = h("div", { id: "toast", role: "status", "aria-live": "polite" });

  // Header
  const nameEl = h("span", { class: "name" });
  const answerEl = h("span", { class: "answer" });
  const sizeEl = h("span", { class: "size" });
  const timeEl = h("span", { class: "time", title: "Elapsed time" }, "0:00");
  const playBtn = iconButton("play", "Start / pause timer", () => {
    if (!game) return;
    game.timerRunning ? game.pauseTimer() : game.startTimer();
  });
  const resetTimerBtn = iconButton("reset", "Reset timer", () => game?.resetTimer());
  const statusEl = h("span", { class: "status" });
  const shareMenu = h("div", { class: "menu", hidden: true, role: "menu" });
  const shareBtn = h("button", { class: "btn", type: "button", "aria-haspopup": "menu" }, icon("share", 16), h("span", {}, "Share"), icon("chevron", 14));
  const importBtn = h("button", { class: "btn", type: "button", title: "Open a puzzle from a link, text or file" }, icon("import", 16), h("span", {}, "Open"));
  const settingsBtn = iconButton("sliders", "Settings", () => toggleSettings());
  const focusBtn = iconButton("focus", "Focus mode: hide the header (F11 for browser fullscreen)", () => setFocusMode(!focusMode));
  const homeBtn = h("button", { class: "brand", type: "button", title: "Start page" }, icon("fit", 20), h("span", {}, APP_NAME));

  const topbar = h(
    "header",
    { id: "topbar" },
    homeBtn,
    h("div", { class: "title" }, nameEl, answerEl, sizeEl),
    h("div", { class: "timer" }, timeEl, playBtn, resetTimerBtn),
    statusEl,
    warnEl,
    h("div", { class: "spacer" }),
    importBtn,
    h("div", { class: "menu-wrap" }, shareBtn, shareMenu),
    settingsBtn,
    focusBtn,
  );

  // Controls
  const clearBtn = h("button", { class: "btn", type: "button", title: "Clear the whole grid (undoable)" }, icon("trash", 16), h("span", { class: "lbl" }, "Clear"));
  const undoBtn = h("button", { class: "btn", type: "button", title: "Undo (Ctrl+Z)" }, icon("undo", 16), h("span", { class: "lbl" }, "Undo"));
  const redoBtn = h("button", { class: "btn", type: "button", title: "Redo (Ctrl+Y)" }, icon("redo", 16), h("span", { class: "lbl" }, "Redo"));
  const fillBtn = h("button", { class: "seg", type: "button", title: "Left click / tap fills; right click or Shift marks (X to swap)" }, icon("fill", 15), h("span", {}, "Fill"));
  const markBtn = h("button", { class: "seg", type: "button", title: "Left click / tap marks empty; right click or Shift fills (X to swap)" }, icon("mark", 15), h("span", {}, "Mark"));
  const zoomOut = iconButton("minus", "Zoom out (−)", () => gridView.zoomOut());
  const zoomFit = iconButton("fit", "Fit puzzle to window (0)", () => gridView.fit());
  const zoomIn = iconButton("plus", "Zoom in (+)", () => gridView.zoomIn());
  const zoomText = h("span", { class: "zoom-now" });
  const zoomRange = h("span", { class: "zoom-range" });
  const zoomReadout = h("span", { class: "zoom-readout" }, zoomText, zoomRange);
  const tierEl = h("span", { class: "tier" });
  const trialReject = h("button", { class: "btn reject", type: "button", title: "Reject: discard this trial tier (R)" }, icon("x", 16));
  const trialEnter = h("button", { class: "btn", type: "button", title: "Start a trial: guess, and keep or discard it later (T)" }, "Trial");
  const trialAccept = h("button", { class: "btn accept", type: "button", title: "Accept: keep this trial tier's work (A)" }, icon("check", 16));

  const controls = h(
    "footer",
    { id: "controls" },
    h("div", { class: "group" }, clearBtn, undoBtn, redoBtn),
    h("div", { class: "group seg-group", role: "group", "aria-label": "Paint mode" }, fillBtn, markBtn),
    h("div", { class: "group" }, zoomOut, zoomFit, zoomIn, zoomReadout),
    h("div", { class: "spacer" }),
    h("div", { class: "group trial" }, tierEl, trialReject, trialEnter, trialAccept),
  );

  // Settings drawer
  const settingsDrawer = h("aside", { id: "settings", hidden: true, "aria-label": "Settings" });
  const settingsBody = h("div", { class: "drawer-body" });
  settingsDrawer.append(
    h("div", { class: "drawer-head" }, h("h2", {}, "Settings"), iconButton("x", "Close settings", () => toggleSettings(false))),
    settingsBody,
  );

  // Import dialog
  const importText = h("textarea", {
    id: "importText",
    rows: "5",
    placeholder: "Paste a puzz.link URL, a link to this page, puzzle text (name;C:1 1|2/R:…), or Puz-Pre v3…",
    spellcheck: "false",
  });
  const importError = h("div", { class: "error", hidden: true });
  const importChoices = h("div", { class: "choices", hidden: true });
  const fileInput = h("input", { type: "file", accept: ".txt,.pzprv3,.nono,text/plain", hidden: true });
  const importDialog = h("dialog", { id: "importDialog" });
  const dropVeil = h("div", { id: "dropVeil" }, "Drop a puzzle file or link to open it");

  const stage = h("main", { id: "stage" }, canvasHost, landing, notice);
  root.append(topbar, stage, controls, settingsDrawer, importDialog, toastEl, dropVeil);

  // ── Grid view ────────────────────────────────────────────────────────────

  const gridView: GridView = new GridView(canvas, canvasHost, {
    getSettings: () => settings,
    getTheme: () => theme,
    onToggleSumGaps: () => {
      settings.assist.clueSumsWithGaps = !settings.assist.clueSumsWithGaps;
      settingsChanged();
      settingsPanel.refresh();
    },
  });
  gridView.onUserChange = () => scheduleSave();
  gridView.onViewChange = () => refreshControls();

  // ── Settings ─────────────────────────────────────────────────────────────

  let settingsPanel = mountSettingsPanel();

  function mountSettingsPanel() {
    const panel = buildSettingsPanel({
      settings,
      getTheme: () => theme,
      onChange: settingsChanged,
      onReset: () => {
        settings = defaultSettings();
        settingsChanged();
        settingsPanel = mountSettingsPanel();
      },
    });
    clear(settingsBody);
    settingsBody.append(panel.el);
    return panel;
  }

  function settingsChanged(): void {
    saveSettings(settings);
    applyTheme();
    if (game) {
      game.assist.autoFillEmpty = settings.assist.autoFillEmpty;
      game.assist.autoCrossEdges = settings.assist.autoCrossEdges;
    }
    gridView.requestRender();
  }

  function applyTheme(): void {
    theme = resolveTheme(settings.theme);
    document.documentElement.dataset.theme = theme;
    const meta = document.querySelector('meta[name="theme-color"]');
    if (meta) meta.setAttribute("content", theme === "dark" ? "#181b22" : "#ffffff");
  }
  matchMedia("(prefers-color-scheme: dark)").addEventListener("change", () => {
    if (settings.theme === "system") {
      applyTheme();
      gridView.requestRender();
      settingsPanel.refresh();
    }
  });
  applyTheme();

  function toggleSettings(force?: boolean): void {
    const open = force ?? !!settingsDrawer.hidden;
    settingsDrawer.hidden = !open;
    document.body.classList.toggle("drawer-open", open);
    if (open) settingsPanel.refresh();
  }

  // ── Puzzle lifecycle ─────────────────────────────────────────────────────

  function openPuzzle(puzzle: Puzzle, how: "push" | "replace" | "none" = "push"): void {
    flushSave();
    game?.pauseTimer();
    unsubGame?.();

    const g = new Game(puzzle);
    g.assist.autoFillEmpty = settings.assist.autoFillEmpty;
    g.assist.autoCrossEdges = settings.assist.autoCrossEdges;
    const saved = loadProgress(puzzle);
    if (saved) g.restore(saved);
    game = g;
    wasSolved = g.solvedNow;
    unsubGame = g.subscribe(onGameChange);

    if (how !== "none") {
      const target = window.location.pathname + puzzleHash(puzzle);
      if (how === "push") history.pushState(null, "", target);
      else history.replaceState(null, "", target);
    }

    document.title = `${puzzle.name} — ${APP_NAME}`;
    canvas.setAttribute("aria-label", `${puzzle.name}: ${puzzle.width} by ${puzzle.height} nonogram`);
    root.classList.remove("no-puzzle");
    landing.hidden = true;
    canvasHost.hidden = false;
    closeImport();
    gridView.setGame(g);
    hideNotice();
    showWarning(trivialInvalidReason(puzzle));
    if (g.solvedNow) showSolved();
    saveProgress(g.toProgress());
    setLastOpen(g.id);
    refreshAll();
  }

  function showLanding(error?: string): void {
    flushSave();
    game?.pauseTimer();
    unsubGame?.();
    unsubGame = null;
    game = null;
    gridView.setGame(null);
    document.title = `${APP_NAME} — solve nonograms in your browser`;
    root.classList.add("no-puzzle");
    canvasHost.hidden = true;
    landing.hidden = false;
    setFocusMode(false);
    hideNotice();
    showWarning(null);
    buildLanding(error);
    refreshAll();
  }

  function loadFromLocation(initial: boolean): void {
    const res = puzzleFromLocation(window.location);
    if (res.kind === "puzzle") {
      // Same puzzle already open (e.g. we just replaced the URL ourselves): nothing to do.
      const next = new Game(res.puzzle);
      if (game && game.id === next.id) return;
      openPuzzle(res.puzzle, initial ? "replace" : "none");
    } else if (res.kind === "error") {
      showLanding(`That link couldn't be read: ${res.message}`);
    } else if (game || initial) {
      showLanding();
    }
  }
  window.addEventListener("hashchange", () => loadFromLocation(false));
  window.addEventListener("popstate", () => loadFromLocation(false));

  // ── Persistence ──────────────────────────────────────────────────────────

  function scheduleSave(): void {
    clearTimeout(saveTimer);
    saveTimer = setTimeout(flushSave, 500);
  }
  function flushSave(): void {
    clearTimeout(saveTimer);
    if (game) saveProgress(game.toProgress());
  }
  const flushAll = () => {
    flushSave();
    saveSettingsNow(settings);
  };
  window.addEventListener("pagehide", flushAll);
  document.addEventListener("visibilitychange", () => {
    if (document.visibilityState === "hidden") flushAll();
  });

  function onGameChange(): void {
    if (!game) return;
    refreshControls();
    refreshHeader();
    scheduleSave();
    if (game.solvedNow && !wasSolved) showSolved();
    else if (!game.solvedNow && wasSolved) hideNotice();
    wasSolved = game.solvedNow;
  }

  // ── Header / controls refresh ────────────────────────────────────────────

  function refreshAll(): void {
    refreshHeader();
    refreshControls();
    refreshTimer();
  }

  function refreshHeader(): void {
    const g = game;
    if (!g) return;
    nameEl.textContent = g.puzzle.name;
    answerEl.textContent = g.puzzle.answer && g.everSolved ? `“${g.puzzle.answer}”` : "";
    sizeEl.textContent = `${g.puzzle.width}×${g.puzzle.height}`;
    statusEl.className = "status " + (g.solvedNow ? "solved" : "");
    clear(statusEl);
    if (g.solvedNow) append(statusEl, [icon("check", 15), "Solved"]);
    else {
      let known = 0;
      for (const c of g.grid) if (c !== 0) known++;
      statusEl.textContent = known ? `${Math.round((100 * known) / g.grid.length)}% marked` : "Not yet solved";
    }
    const playing = g.timerRunning;
    clear(playBtn);
    playBtn.append(icon(playing ? "pause" : "play", 18));
    playBtn.title = playing ? "Pause timer" : "Start timer";
  }

  function refreshTimer(): void {
    if (game) timeEl.textContent = formatTime(game.timerElapsedMs());
  }
  setInterval(() => {
    if (game?.timerRunning) refreshTimer();
  }, 250);

  function refreshControls(): void {
    const g = game;
    undoBtn.disabled = !g?.canUndo;
    redoBtn.disabled = !g?.canRedo;
    clearBtn.disabled = !g;
    const tiers = g?.trial.length ?? 0;
    trialEnter.textContent = tiers === 0 ? "Trial" : "+1";
    trialReject.disabled = tiers === 0;
    trialAccept.disabled = tiers === 0;
    tierEl.textContent = tiers ? `Tier ${tiers}` : "";
    tierEl.style.color = tiers ? `var(--tier-${((tiers - 1) % 5) + 1})` : "";
    fillBtn.classList.toggle("on", gridView.primaryMode === "fill");
    markBtn.classList.toggle("on", gridView.primaryMode === "mark");
    fillBtn.setAttribute("aria-pressed", String(gridView.primaryMode === "fill"));
    markBtn.setAttribute("aria-pressed", String(gridView.primaryMode === "mark"));
    const cell = gridView.cellSize;
    const max = gridView.maxCellSize;
    zoomText.textContent = `${cell} px`;
    zoomRange.textContent = `${MIN_CELL}–${max}`;
    zoomReadout.title =
      `Cell size: ${cell} px. Range ${MIN_CELL}–${MAX_CELL} px` +
      (max < MAX_CELL ? ` (up to ${max} px for this puzzle and window)` : "") +
      `.
Grid lines are 1 px below ${HEAVY_LINE_MIN_CELL} px; the 5-cell lines and frame get heavier from ${HEAVY_LINE_MIN_CELL} px.`;
    zoomFit.title = "Fit puzzle to window (0)";
  }

  // ── Notices & toasts ─────────────────────────────────────────────────────

  function showWarning(text: string | null): void {
    clear(warnEl);
    warnEl.hidden = !text;
    if (!text) return;
    warnEl.title = text;
    append(warnEl, [icon("x", 14), h("span", {}, text)]);
  }

  let noticeTimer: ReturnType<typeof setTimeout> | undefined;
  function hideNotice(): void {
    clearTimeout(noticeTimer);
    notice.hidden = true;
  }

  function showSolved(): void {
    if (!game) return;
    const g = game;
    const t = formatTime(g.timerElapsedMs());
    const msg = g.timerElapsedMs() > 0 ? `Solved in ${t}!` : "Solved!";
    const close = h("button", { class: "icon-btn", type: "button", title: "Dismiss", "aria-label": "Dismiss" }, icon("x", 14));
    close.addEventListener("click", hideNotice);
    clear(notice);
    append(notice, [icon("check", 18), h("span", {}, g.puzzle.answer ? `${msg}  It's “${g.puzzle.answer}”.` : msg), close]);
    notice.hidden = false;
    clearTimeout(noticeTimer);
    noticeTimer = setTimeout(hideNotice, 12000);
  }

  let toastTimer: ReturnType<typeof setTimeout> | undefined;
  function toast(msg: string): void {
    toastEl.textContent = msg;
    toastEl.classList.add("show");
    clearTimeout(toastTimer);
    toastTimer = setTimeout(() => toastEl.classList.remove("show"), 2200);
  }

  // ── Buttons ──────────────────────────────────────────────────────────────

  clearBtn.addEventListener("click", () => game?.clear());
  undoBtn.addEventListener("click", () => game?.undo());
  redoBtn.addEventListener("click", () => game?.redo());
  fillBtn.addEventListener("click", () => setMode("fill"));
  markBtn.addEventListener("click", () => setMode("mark"));
  trialEnter.addEventListener("click", () => game?.enterTrial());
  trialAccept.addEventListener("click", () => game?.acceptTrial());
  trialReject.addEventListener("click", () => game?.rejectTrial());
  homeBtn.addEventListener("click", () => {
    if (!game) return;
    history.pushState(null, "", window.location.pathname);
    showLanding();
  });
  importBtn.addEventListener("click", () => openImport());

  function setMode(m: "fill" | "mark"): void {
    gridView.primaryMode = m;
    refreshControls();
  }

  // ── Share / export menu ──────────────────────────────────────────────────

  function buildShareMenu(): void {
    clear(shareMenu);
    const g = game;
    if (!g) return;
    const base = window.location.origin + window.location.pathname;
    const item = (ic: string, label: string, fn: () => void | Promise<void>) => {
      const b = h("button", { type: "button", role: "menuitem", class: "menu-item" }, icon(ic, 16), h("span", {}, label));
      b.addEventListener("click", async () => {
        closeShare();
        await fn();
      });
      return b;
    };
    const copy = (what: string, text: string) => async () => toast((await copyText(text)) ? `${what} copied` : "Couldn't copy — check clipboard permission");
    const fulfilled = () => {
      const d = g.derived();
      return puzzleToPzprv3(g.puzzle, g.grid, d.rowFulfilled, d.colFulfilled);
    };
    const fileBase = g.puzzle.name.replace(/[^\w.-]+/g, "_") || "puzzle";

    append(shareMenu, [
      item("link", "Copy link to this puzzle", copy("Link", buildShareLink(base, g.puzzle))),
      g.puzzle.answer ? item("link", "Copy link (includes the answer)", copy("Link", buildShareLink(base, g.puzzle, { includeAnswer: true }))) : null,
      h("hr", {}),
      item("copy", "Copy puzz.link URL", copy("puzz.link URL", puzzleToPuzzlinkUrl(g.puzzle))),
      item("copy", "Copy puzzle text", copy("Puzzle text", puzzleToNative(g.puzzle))),
      h("hr", {}),
      item("download", "Download Puz-Pre v3 (with progress)", () => downloadText(`${fileBase}.txt`, fulfilled())),
      item("file", "Download puzzle file", () => downloadText(`${fileBase}.txt`, puzzleToNative(g.puzzle) + "\n")),
    ]);
  }
  function closeShare(): void {
    shareMenu.hidden = true;
    shareBtn.setAttribute("aria-expanded", "false");
  }
  shareBtn.addEventListener("click", (e) => {
    e.stopPropagation();
    if (!game) return;
    if (!shareMenu.hidden) return closeShare();
    buildShareMenu();
    shareMenu.hidden = false;
    shareBtn.setAttribute("aria-expanded", "true");
  });
  document.addEventListener("click", (e) => {
    if (!shareMenu.hidden && !shareMenu.contains(e.target as Node)) closeShare();
  });

  // ── Import ───────────────────────────────────────────────────────────────

  importDialog.append(
    h(
      "form",
      { method: "dialog", class: "import-form" },
      h("div", { class: "drawer-head" }, h("h2", {}, "Open a puzzle"), iconButton("x", "Close", () => closeImport())),
      importText,
      importError,
      importChoices,
      h(
        "div",
        { class: "row-actions" },
        (() => {
          const b = h("button", { class: "btn", type: "button" }, icon("file", 16), "Choose file…");
          b.addEventListener("click", () => fileInput.click());
          return b;
        })(),
        h("div", { class: "spacer" }),
        (() => {
          const b = h("button", { class: "btn primary", type: "button" }, "Open");
          b.addEventListener("click", () => submitImport(importText.value));
          return b;
        })(),
      ),
      h("p", { class: "note" }, "Accepts puzz.link nonogram URLs, links to this page, the native text format, and Puz-Pre v3. You can also just paste (Ctrl+V) or drop a file anywhere on the page."),
      fileInput,
    ),
  );
  importDialog.addEventListener("close", () => importText.blur());
  importDialog.addEventListener("click", (e) => {
    if (e.target === importDialog) closeImport();
  });
  importText.addEventListener("keydown", (e) => {
    if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) submitImport(importText.value);
  });
  fileInput.addEventListener("change", async () => {
    const f = fileInput.files?.[0];
    fileInput.value = "";
    if (f) submitImport(await f.text());
  });

  function openImport(prefill = ""): void {
    importText.value = prefill;
    importError.hidden = true;
    importChoices.hidden = true;
    if (!importDialog.open) importDialog.showModal();
    importText.focus();
  }
  function closeImport(): void {
    if (importDialog.open) importDialog.close();
  }

  /** Returns true if something was opened (or a chooser shown). */
  function submitImport(text: string, quiet = false): boolean {
    const res = importFromText(text);
    if (res.puzzles.length === 1) {
      openPuzzle(res.puzzles[0]);
      if (res.errors.length) toast(`Opened; ${res.errors.length} other line(s) skipped`);
      return true;
    }
    if (res.puzzles.length > 1) {
      if (!importDialog.open) openImport(text);
      showChoices(res.puzzles, res.errors);
      return true;
    }
    if (!quiet) {
      if (!importDialog.open) openImport(text);
      importChoices.hidden = true;
      importError.hidden = false;
      importError.textContent = res.errors.slice(0, 4).join("\n") || "Nothing recognisable to open.";
    }
    return false;
  }

  function showChoices(puzzles: Puzzle[], errors: string[]): void {
    importError.hidden = errors.length === 0;
    importError.textContent = errors.length ? `${errors.length} line(s) skipped: ${errors.slice(0, 2).join("; ")}` : "";
    clear(importChoices);
    importChoices.hidden = false;
    append(importChoices, [h("p", {}, `${puzzles.length} puzzles found — pick one:`)]);
    for (const p of puzzles) {
      const b = h("button", { type: "button", class: "choice" }, h("span", { class: "grow" }, p.name), h("small", {}, `${p.width}×${p.height}`));
      b.addEventListener("click", () => openPuzzle(p));
      importChoices.append(b);
    }
  }

  // Paste / drop anywhere
  window.addEventListener("paste", (e) => {
    const t = e.target as HTMLElement | null;
    if (t && /^(INPUT|TEXTAREA|SELECT)$/.test(t.tagName)) return;
    const text = e.clipboardData?.getData("text") ?? "";
    if (/nonogram\/\d+\/\d+\/|;\s*[CR]:|^\s*pzprv3/.test(text) && submitImport(text, true)) e.preventDefault();
  });
  let dragDepth = 0;
  window.addEventListener("dragenter", (e) => {
    if (e.dataTransfer?.types.includes("Files") || e.dataTransfer?.types.includes("text/plain")) {
      dragDepth++;
      dropVeil.classList.add("show");
    }
  });
  window.addEventListener("dragleave", () => {
    dragDepth = Math.max(0, dragDepth - 1);
    if (!dragDepth) dropVeil.classList.remove("show");
  });
  window.addEventListener("dragover", (e) => e.preventDefault());
  window.addEventListener("drop", async (e) => {
    e.preventDefault();
    dragDepth = 0;
    dropVeil.classList.remove("show");
    const file = e.dataTransfer?.files?.[0];
    const text = file ? await file.text() : (e.dataTransfer?.getData("text/plain") ?? "");
    if (text) submitImport(text);
  });

  // ── Landing page ─────────────────────────────────────────────────────────

  function buildLanding(error?: string): void {
    clear(landing);
    const box = h("div", { class: "landing-inner" });
    box.append(
      h("h1", {}, APP_NAME),
      h("p", { class: "lead" }, "A nonogram (picross) solver's workbench. Paint, cross out, try guesses in trial tiers, and let the assists do the busywork."),
    );
    if (error) box.append(h("div", { class: "error" }, error));

    const resume = resumeCandidate();
    if (resume) {
      const frac = Math.round(progressFraction(resume.entry) * 100);
      const time = resume.entry.elapsedMs > 0 ? ` · ${formatTime(resume.entry.elapsedMs)}` : "";
      const resumeBtn = h("button", { class: "btn primary", type: "button" }, "Resume");
      resumeBtn.addEventListener("click", () => openPuzzle(resume.puzzle));
      box.append(
        h(
          "div",
          { class: "card resume" },
          h("h2", {}, "Continue where you left off"),
          h(
            "div",
            { class: "row-actions" },
            h("div", { class: "grow" }, h("div", { class: "name" }, resume.puzzle.name), h("small", {}, `${resume.puzzle.width}×${resume.puzzle.height} · ${frac}% marked${time}`)),
            resumeBtn,
          ),
        ),
      );
    }

    const openCard = h("div", { class: "card" });
    const paste = h("textarea", { rows: "3", placeholder: "Paste a puzz.link URL or puzzle text…", spellcheck: "false" });
    const go = h("button", { class: "btn primary", type: "button" }, "Open");
    const file = h("button", { class: "btn", type: "button" }, icon("file", 16), "Choose file…");
    go.addEventListener("click", () => {
      if (paste.value.trim()) submitImport(paste.value);
    });
    file.addEventListener("click", () => fileInput.click());
    paste.addEventListener("keydown", (e) => {
      if (e.key === "Enter" && !e.shiftKey && paste.value.trim()) {
        e.preventDefault();
        submitImport(paste.value);
      }
    });
    openCard.append(h("h2", {}, resume ? "Start a new puzzle" : "Open a puzzle"), paste, h("div", { class: "row-actions" }, file, h("div", { class: "spacer" }), go));
    box.append(openCard);

    const recent = listRecent().filter((r) => r.entry.id !== resume?.entry.id).slice(0, 8);
    if (recent.length) {
      const list = h("div", { class: "list" });
      for (const { entry, puzzle } of recent) {
        const frac = progressFraction(entry);
        const item = h("div", { class: "list-item" });
        const open = h(
          "button",
          { type: "button", class: "list-open" },
          h("span", { class: "grow name" }, puzzle.name, entry.everSolved ? h("span", { class: "solved-dot", title: "Solved" }, "✓") : null),
          h("small", {}, `${puzzle.width}×${puzzle.height}`),
          h("span", { class: "bar", title: `${Math.round(frac * 100)}% marked` }, h("i", { style: `width:${Math.round(frac * 100)}%` })),
        );
        open.addEventListener("click", () => openPuzzle(puzzle));
        const del = iconButton("x", "Forget this puzzle's saved progress", () => {
          deleteProgress(entry.id);
          buildLanding();
        });
        item.append(open, del);
        list.append(item);
      }
      box.append(h("div", { class: "card" }, h("h2", {}, resume ? "Other recent puzzles" : "Recent puzzles"), list));
    }

    const samples = h("div", { class: "list" });
    for (const p of SAMPLES) {
      const b = h("button", { type: "button", class: "list-item list-open" }, h("span", { class: "grow name" }, p.name), h("small", {}, `${p.width}×${p.height}`));
      b.addEventListener("click", () => openPuzzle({ ...p, solution: undefined }));
      samples.append(b);
    }
    box.append(h("div", { class: "card" }, h("h2", {}, "Try one"), samples));
    box.append(
      h(
        "p",
        { class: "note" },
        "Links to this page carry the puzzle in the address (after the #), so you can share or bookmark any puzzle. Progress is saved in this browser only.",
      ),
    );
    landing.append(box);
  }

  // ── Focus mode ───────────────────────────────────────────────────────────

  // Focus mode only hides the header. Fullscreen is the browser's own (F11);
  // we deliberately don't call the Fullscreen API, so the two can't fight.
  function setFocusMode(on: boolean): void {
    if (on && !game) return;
    focusMode = on;
    document.body.classList.toggle("focus", on);
    focusBtn.classList.toggle("on", on);
    gridView.requestRender();
  }
  const exitFocus = h("button", { id: "exitFocus", type: "button", title: "Leave focus mode (Esc)" }, icon("focus", 18));
  exitFocus.addEventListener("click", () => setFocusMode(false));
  // Inside the stage (not the viewport) so it stays clear of the docked settings drawer.
  stage.append(exitFocus);

  // ── Keyboard ─────────────────────────────────────────────────────────────

  window.addEventListener("keydown", (e) => {
    const t = e.target as HTMLElement | null;
    if (t && (/^(INPUT|TEXTAREA|SELECT)$/.test(t.tagName) || t.isContentEditable)) return;
    if (importDialog.open) return;

    if (e.key === "Escape") {
      if (!shareMenu.hidden) closeShare();
      else if (!settingsDrawer.hidden) toggleSettings(false);
      else if (focusMode) setFocusMode(false);
      return;
    }
    if (!game) return;
    const mod = e.ctrlKey || e.metaKey;
    const key = e.key.length === 1 ? e.key.toLowerCase() : e.key;

    if (mod && !e.altKey) {
      if (key === "z") {
        e.preventDefault();
        e.shiftKey ? game.redo() : game.undo();
      } else if (key === "y") {
        e.preventDefault();
        game.redo();
      }
      return;
    }
    if (e.altKey) return;

    if (key === settings.focusKey.toLowerCase() || key === settings.focusKey) {
      e.preventDefault();
      setFocusMode(!focusMode);
      return;
    }
    switch (key) {
      case "x":
        setMode(gridView.primaryMode === "fill" ? "mark" : "fill");
        break;
      case "t":
        game.enterTrial();
        break;
      case "a":
        game.acceptTrial();
        break;
      case "r":
        game.rejectTrial();
        break;
      case "+":
      case "=":
        gridView.zoomIn();
        break;
      case "-":
      case "_":
        gridView.zoomOut();
        break;
      case "0":
        gridView.fit();
        break;
      default:
        return;
    }
    e.preventDefault();
  });

  // ── Boot ─────────────────────────────────────────────────────────────────

  root.classList.add("no-puzzle");
  canvasHost.hidden = true;
  buildLanding();
  refreshAll();
  loadFromLocation(true);

  return { gridView, game: () => game, settings: () => settings };
}

function iconButton(name: string, title: string, onClick: () => void): HTMLButtonElement {
  const b = h("button", { class: "icon-btn", type: "button", title, "aria-label": title }, icon(name, 18));
  b.addEventListener("click", onClick);
  return b;
}
