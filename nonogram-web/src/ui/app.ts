import { puzzleToNative, puzzleToPzprv3 } from "../core/export";
import { trivialInvalidReason } from "../core/lines";
import { importFromText, parseNativeLine } from "../core/parse";
import { puzzleToPuzzlinkUrl } from "../core/puzzlink";
import { SAMPLES } from "../core/samples";
import { buildShareLink, puzzleFromLocation, puzzleHash } from "../core/share";
import type { Puzzle } from "../core/types";
import { Game } from "../state/game";
import { deleteProgress, listRecent, loadProgress, progressFraction, resumeCandidate, saveProgress, setLastOpen } from "../state/progress";
import { defaultSettings, loadSettings, resolveTheme, saveSettings, saveSettingsNow, type ResolvedTheme, type Settings } from "../state/settings";
import { append, clear, copyText, downloadText, formatTime, h, icon } from "./dom";
import { HEAVY_LINE_MIN_CELL, type Layout } from "./geometry";
import { closeColorPicker } from "./colorPicker";
import { GridView } from "./gridview";
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

  // ── Static DOM ───────────────────────────────────────────────────────────

  const canvas = h("canvas", { id: "grid", role: "img", "aria-label": "Nonogram grid", tabindex: "-1" });
  const canvasHost = h("div", { id: "canvasHost" }, canvas);
  const landing = h("section", { id: "landing" });
  // Overlay (never in flow): an in-flow banner would shift the grid under the
  // pointer when it appears mid-stroke, e.g. on the stroke that solves the puzzle.
  const notice = h("div", { id: "notice", hidden: true, role: "status" });
  const warnEl = h("span", { class: "warn-pill", hidden: true });
  const toastEl = h("div", { id: "toast", role: "status", "aria-live": "polite" });

  // Header: title, size, timer, zoom — then the actions.
  const nameEl = h("span", { class: "name" });
  const answerEl = h("span", { class: "answer" });
  const sizeEl = h("span", { class: "size" });
  const timeEl = h("span", { class: "time" }, "0:00");
  // The whole pill is the pause / resume button (time + a play/pause glyph).
  const timerIcon = h("span", { class: "timer-icon" });
  const timerBtn = h("button", { class: "pill timer", type: "button" }, timeEl, timerIcon);
  timerBtn.addEventListener("click", () => {
    if (!game) return;
    game.timerRunning ? game.pauseTimer() : game.startTimer();
  });
  const RESTART_TIP = "Restart: blank the grid, un-dim every clue, clear the undo history and reset the timer";
  const restartBtn = h("button", { id: "restartBtn", class: "btn", type: "button", title: RESTART_TIP }, icon("reset", 16), h("span", {}, "Restart"));
  const zoomOut = iconButton("minus", "Zoom out (−)", () => gridView.zoomOut());
  const zoomIn = iconButton("plus", "Zoom in (+)", () => gridView.zoomIn());
  const zoomFit = iconButton("fit", "Fit the puzzle to the window (0)", () => gridView.fit());
  const zoomReadout = h("button", { class: "zoom-readout", type: "button" });
  const shareMenu = h("div", { class: "menu", hidden: true, role: "menu" });
  const shareBtn = h("button", { class: "btn", type: "button", "aria-haspopup": "menu" }, icon("share", 16), h("span", {}, "Share"), icon("chevron", 14));
  const importBtn = h("button", { class: "btn", type: "button", title: "Open a puzzle from a link, text or file" }, icon("import", 16), h("span", {}, "Open"));
  const settingsBtn = iconButton("sliders", "Settings", () => toggleSettings());
  const homeBtn = h("button", { class: "brand", type: "button", title: "Start page" }, icon("fit", 20), h("span", {}, APP_NAME));

  // Three columns so the title is truly centered whatever the side groups' widths.
  const topbar = h(
    "header",
    { id: "topbar" },
    h("div", { class: "hd-left" }, homeBtn, timerBtn, restartBtn, warnEl),
    h("div", { class: "title" }, nameEl, sizeEl, answerEl),
    h(
      "div",
      { class: "hd-right" },
      h("div", { class: "pill zoom" }, zoomOut, zoomReadout, zoomIn, zoomFit),
      importBtn,
      h("div", { class: "menu-wrap" }, shareBtn, shareMenu),
      settingsBtn,
    ),
  );

  // A small tab under the header's right end: hides / shows the header.
  const headerTab = h("button", { id: "headerTab", type: "button" }, icon("chevron", 14));

  // Footer: sits centered under the puzzle frame (positioned from the layout).
  const undoBtn = h("button", { class: "btn sm icon-only", type: "button", title: "Undo (Ctrl+Z)", "aria-label": "Undo" }, icon("undo", 16));
  const redoBtn = h("button", { class: "btn sm icon-only", type: "button", title: "Redo (Ctrl+Y)", "aria-label": "Redo" }, icon("redo", 16));
  const tierEl = h("span", { class: "tier" });
  const trialReject = h("button", { class: "btn sm reject icon-only", type: "button", title: "Reject: discard this trial tier (R)", "aria-label": "Reject trial" }, icon("x", 15));
  const trialEnter = h("button", { class: "btn sm", type: "button", title: "Start a trial: guess, and keep or discard it later (T)" }, "Trial");
  const trialAccept = h("button", { class: "btn sm accept icon-only", type: "button", title: "Accept: keep this trial tier's work (A)", "aria-label": "Accept trial" }, icon("check", 15));

  const footer = h(
    "div",
    { id: "footer" },
    h("div", { class: "group" }, undoBtn, redoBtn),
    h("div", { class: "group trial" }, tierEl, trialReject, trialEnter, trialAccept),
  );

  // Shown while the timer is paused: blurs the puzzle so a pause can't be used to think for free.
  const pauseTime = h("div", { class: "pause-time" });
  const resumeBtn = h("button", { class: "resume-btn", type: "button" }, icon("play", 40), h("span", {}, "Resume"));
  const pauseOverlay = h(
    "div",
    { id: "pauseOverlay", hidden: true, role: "dialog", "aria-label": "Paused" },
    h("div", { class: "pause-card" }, h("div", { class: "pause-title" }, "Paused"), resumeBtn, pauseTime),
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

  const stage = h("main", { id: "stage" }, canvasHost, landing, notice, footer, pauseOverlay, headerTab);
  root.append(topbar, stage, settingsDrawer, importDialog, toastEl, dropVeil);

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
  gridView.onLayout = positionFooter;

  /** Center the footer under the puzzle frame, keeping it inside the window. */
  function positionFooter(L: Layout): void {
    if (!game) return;
    const fw = footer.offsetWidth;
    const left = Math.min(Math.max(4, L.originX + L.frameW / 2 - fw / 2), Math.max(4, L.W - fw - 4));
    footer.style.transform = `translate(${Math.round(left)}px, ${Math.round(L.footerTop)}px)`;
  }

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
      game.assist.autoCrossMatched = settings.assist.autoCrossMatched;
    }
    gridView.requestRender();
    refreshControls();
    applyTimerVisibility();
    refreshPause();
  }

  function applyTimerVisibility(): void {
    document.body.classList.toggle("no-timer", !settings.showTimer);
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
  applyTimerVisibility();

  function toggleSettings(force?: boolean): void {
    const open = force ?? !!settingsDrawer.hidden;
    settingsDrawer.hidden = !open;
    document.body.classList.toggle("drawer-open", open);
    if (!open) closeColorPicker();
    // A control inside a just-closed drawer must not keep swallowing keyboard shortcuts.
    if (!open && settingsDrawer.contains(document.activeElement)) (document.activeElement as HTMLElement).blur();
    if (open) settingsPanel.refresh();
  }

  // ── Puzzle lifecycle ─────────────────────────────────────────────────────

  function openPuzzle(puzzle: Puzzle, how: "push" | "replace" | "none" = "push"): void {
    flushSave();
    game?.pauseTimer();
    unsubGame?.();

    const saved = loadProgress(puzzle);
    // A puzzle opened without its name/answer (a Puz-Pre v3 "with progress" file can't carry
    // either) picks them back up from this browser's saved progress, if any: same clues mean the
    // same puzzle, and the saved spec keeps the metadata this particular import couldn't.
    if (saved) {
      try {
        const stored = parseNativeLine(saved.spec);
        if (!puzzle.name || puzzle.name === `${puzzle.width}×${puzzle.height}`) puzzle = { ...puzzle, name: stored.name };
        if (puzzle.answer === undefined && stored.answer !== undefined) puzzle = { ...puzzle, answer: stored.answer };
      } catch {
        /* corrupt saved spec; keep what we have */
      }
    }

    const g = new Game(puzzle);
    g.assist.autoFillEmpty = settings.assist.autoFillEmpty;
    g.assist.autoCrossEdges = settings.assist.autoCrossEdges;
    g.assist.autoCrossMatched = settings.assist.autoCrossMatched;
    if (saved) g.restore(saved);
    else if (puzzle.progress) g.seedGrid(puzzle.progress);
    game = g;
    wasSolved = g.solvedNow;
    unsubGame = g.subscribe(onGameChange);
    if (settings.showTimer && !g.solvedNow) g.startTimer();

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
    pauseOverlay.hidden = true;
    gridView.setGame(null);
    document.title = `${APP_NAME} — solve nonograms in your browser`;
    root.classList.add("no-puzzle");
    canvasHost.hidden = true;
    landing.hidden = false;
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
    const playing = g.timerRunning;
    clear(timerIcon);
    timerIcon.append(icon(playing ? "pause" : "play", 18));
    timerBtn.title = playing ? "Pause timer" : "Start timer";
    timerBtn.setAttribute("aria-label", timerBtn.title);
    refreshTimer();
    refreshPause();
  }

  /** Paused = the timer is stopped on an unsolved puzzle (only possible with the timer shown). */
  function isPaused(): boolean {
    return !!game && settings.showTimer && !game.timerRunning && !game.solvedNow;
  }

  function refreshPause(): void {
    const paused = isPaused();
    if (paused && pauseOverlay.hidden) {
      pauseOverlay.hidden = false;
      resumeBtn.focus(); // so Enter / Space resume
    } else if (!paused && !pauseOverlay.hidden) {
      pauseOverlay.hidden = true;
    }
  }
  resumeBtn.addEventListener("click", () => game?.startTimer());

  function refreshTimer(): void {
    if (game) {
      timeEl.textContent = formatTime(game.timerElapsedMs());
      pauseTime.textContent = timeEl.textContent;
    }
  }
  setInterval(() => {
    if (game?.timerRunning) refreshTimer();
  }, 250);
  setInterval(() => {
    if (game?.timerRunning) flushSave();
  }, 15000);

  function refreshControls(): void {
    const g = game;
    undoBtn.disabled = !g?.canUndo;
    redoBtn.disabled = !g?.canRedo;
    const tiers = g?.trial.length ?? 0;
    trialEnter.textContent = tiers === 0 ? "Trial" : "+1";
    trialReject.disabled = tiers === 0;
    trialAccept.disabled = tiers === 0;
    tierEl.textContent = tiers ? `Tier ${tiers}` : "";
    tierEl.style.color = tiers ? `var(--tier-${((tiers - 1) % 5) + 1})` : "";
    const wheel = settings.wheelMode === "zoom" ? "mouse wheel" : "Ctrl + mouse wheel";
    zoomOut.title = `Zoom out (−, or ${wheel} down)`;
    zoomIn.title = `Zoom in (+, or ${wheel} up)`;
    const cell = gridView.cellSize;
    const ref = settings.zoomReference;
    zoomReadout.textContent = `${Math.round((cell / ref) * 100)}%`;
    zoomReadout.title =
      `Zoom ${Math.round((cell / ref) * 100)}%: cells are ${cell} px (100% = ${ref} px, change it in Settings).
` +
      `Click to return to 100%. Grid lines are 1 px below ${HEAVY_LINE_MIN_CELL} px cells.`;
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
    const msg = settings.showTimer && g.timerElapsedMs() > 0 ? `Solved in ${t}!` : "Solved!";
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

  // Restart throws everything away (grid, undo history, dimming, timer) and sits in the header,
  // so it takes two clicks.
  let restartArmed: ReturnType<typeof setTimeout> | undefined;
  function disarmRestart(): void {
    clearTimeout(restartArmed);
    restartArmed = undefined;
    restartBtn.classList.remove("armed");
    restartBtn.title = RESTART_TIP;
    (restartBtn.querySelector("span") as HTMLElement).textContent = "Restart";
  }
  restartBtn.addEventListener("click", () => {
    if (!game) return;
    if (restartArmed === undefined) {
      restartBtn.classList.add("armed");
      restartBtn.title = "Click again to restart this puzzle";
      (restartBtn.querySelector("span") as HTMLElement).textContent = "Sure?";
      restartArmed = setTimeout(disarmRestart, 3000);
      return;
    }
    disarmRestart();
    game.restart();
    if (settings.showTimer && !game.timerRunning) game.startTimer();
    toast("Puzzle restarted");
  });
  restartBtn.addEventListener("blur", disarmRestart);
  undoBtn.addEventListener("click", () => game?.undo());
  redoBtn.addEventListener("click", () => game?.redo());
  zoomReadout.addEventListener("click", () => gridView.setCellSize(settings.zoomReference));
  headerTab.addEventListener("click", () => setHeaderHidden(!settings.headerHidden));
  trialEnter.addEventListener("click", () => game?.enterTrial());
  trialAccept.addEventListener("click", () => game?.acceptTrial());
  trialReject.addEventListener("click", () => game?.rejectTrial());
  homeBtn.addEventListener("click", () => {
    if (!game) return;
    history.pushState(null, "", window.location.pathname);
    showLanding();
  });
  importBtn.addEventListener("click", () => openImport());

  function swapPaintMode(): void {
    settings.primaryMode = settings.primaryMode === "fill" ? "mark" : "fill";
    settingsChanged();
    settingsPanel.refresh();
    toast(`Left button now ${settings.primaryMode === "fill" ? "fills" : "marks"}`);
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
      // The answer (if the puzzle has one) rides along scrambled: it's only a spoiler.
      item("link", "Copy link to this puzzle", copy("Link", buildShareLink(base, g.puzzle, { includeAnswer: true }))),
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
      importError.textContent = res.errors.slice(0, 4).join("\n") || "Nothing recognizable to open.";
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

  // ── Fullscreen & header visibility ───────────────────────────────────────

  // Fullscreen is the browser's own; F just asks for it (Esc / F11 leave it as usual).
  function toggleFullscreen(): void {
    if (document.fullscreenElement) document.exitFullscreen?.().catch(() => {});
    else document.documentElement.requestFullscreen?.().catch(() => {});
  }

  function setHeaderHidden(hidden: boolean): void {
    settings.headerHidden = hidden;
    document.body.classList.toggle("header-hidden", hidden);
    headerTab.title = hidden ? "Show the header" : "Hide the header";
    headerTab.setAttribute("aria-label", headerTab.title);
    headerTab.setAttribute("aria-expanded", String(!hidden));
    saveSettings(settings);
  }
  setHeaderHidden(settings.headerHidden);

  // ── Keyboard ─────────────────────────────────────────────────────────────

  window.addEventListener("keydown", (e) => {
    const t = e.target as HTMLElement | null;
    if (t && (/^(INPUT|TEXTAREA|SELECT)$/.test(t.tagName) || t.isContentEditable)) return;
    if (importDialog.open) return;

    if (e.key === "Escape") {
      if (!shareMenu.hidden) closeShare();
      else if (!settingsDrawer.hidden) toggleSettings(false);
      return;
    }
    if (!game) return;
    const mod = e.ctrlKey || e.metaKey;
    const key = e.key.length === 1 ? e.key.toLowerCase() : e.key;

    if (isPaused()) {
      if (!mod && (key === settings.fullscreenKey.toLowerCase() || key === settings.fullscreenKey)) {
        e.preventDefault();
        toggleFullscreen();
      }
      return; // Enter / Space still activate the focused Resume button
    }

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

    if (key === settings.fullscreenKey.toLowerCase() || key === settings.fullscreenKey) {
      e.preventDefault();
      toggleFullscreen();
      return;
    }
    switch (key) {
      case "x":
        swapPaintMode();
        break;
      case "l":
        settings.lockFrame = !settings.lockFrame;
        settingsChanged();
        settingsPanel.refresh();
        toast(settings.lockFrame ? "Puzzle position locked" : "Puzzle position unlocked");
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
