use std::collections::{HashMap, HashSet};
use std::path::Path;

use iced::{
    alignment::{Horizontal, Vertical},
    keyboard, mouse,
    widget::{button, column, container, horizontal_rule, row, Space, stack, text, vertical_rule},
    Color, Element, Event, Length, Padding, Point, Size, Subscription, Task, Theme, Vector,
    window,
};
use iced_fonts::bootstrap::Bootstrap;

use nonogram_core::{
    parse_file, AllSolutions, CancelToken, CellState, ParsedPuzzle, Puzzle,
    SolveContext, SolveResult, SolutionState,
};

use crate::solver::SolverKind;
use nonogram_graph_search::{GraphSearchSolver, ProgressConfig, ProgressUpdate};

pub(crate) mod style;
pub(crate) mod settings;
pub(crate) mod persistence;
pub(crate) mod scan;
pub(crate) mod export;
pub(crate) mod grid_view;
mod url_import;
mod view_panel;
mod view_detail;

use settings::{AssistFlag, CellSettings, AssistanceSettings, SecondaryFocusKey, SubcellKind, load_settings, save_settings};
use persistence::{
    load_solved, save_solved, save_session, load_session,
    save_solution_to_file, relative_path, save_window_state,
};
use scan::scan_puzzle_dirs;
use export::{ExportFormat, format_solve_log, puzzle_to_file_string, export_puzprv3, puzzle_to_puzzlink_url, parse_puzzlink_url, parse_pzprv3};
use grid_view::{grid_at_step, is_puzzle_fully_solved, check_line_fulfilled, forced_empty_from_edges};

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

pub type Key = (usize, usize); // (file_idx, puzzle_idx)

pub struct LoadedFile {
    pub path: String,
    pub name: String,
    pub folder: Option<String>,
    pub auto_loaded: bool,
    pub puzzles: Vec<ParsedPuzzle>,
    pub collapsed: bool,
}
// ---------------------------------------------------------------------------
// Drag axis-lock preview
// ---------------------------------------------------------------------------

/// Active during an axis-locked drag stroke. All grid writes are deferred to
/// `DragEnded` so the painted line can be recomputed on every cursor move.
pub(crate) struct DragPreview {
    pub(crate) key:     Key,
    pub(crate) anchor:  (usize, usize),  // cell where the drag started
    pub(crate) current: (usize, usize),  // cell currently under the cursor
    pub(crate) paint:   CellState,       // state to write on commit
}

/// Returns the line of cells to paint for an axis-locked drag.
/// Compares |Δcol| vs |Δrow| from `anchor` to `current`:
///  - Δcol ≥ Δrow → horizontal: anchor's row, cols from anchor to current
///  - Δrow  > Δcol → vertical:   anchor's col, rows from anchor to current
pub(crate) fn preview_cells(anchor: (usize, usize), current: (usize, usize)) -> Vec<(usize, usize)> {
    let (ar, ac) = anchor;
    let (cr, cc) = current;
    let dr = cr.abs_diff(ar);
    let dc = cc.abs_diff(ac);
    if dc >= dr {
        let (c0, c1) = if ac <= cc { (ac, cc) } else { (cc, ac) };
        (c0..=c1).map(|c| (ar, c)).collect()
    } else {
        let (r0, r1) = if ar <= cr { (ar, cr) } else { (cr, ar) };
        (r0..=r1).map(|r| (r, ac)).collect()
    }
}

// ---------------------------------------------------------------------------
// App
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default)]
pub struct TimerState {
    pub elapsed_secs: u64,
    pub running: bool,
}

pub struct App {
    files: Vec<LoadedFile>,
    selected: HashSet<Key>,
    results: HashMap<(Key, SolverKind), SolveResult>,
    all_solutions: HashMap<(Key, SolverKind), AllSolutions>,
    solution_index: usize,
    solver: SolverKind,
    focused: Option<Key>,
    status: String,
    busy: bool,
    step_cursor: usize,
    replaying: bool,
    cancel: CancelToken,
    modifiers: keyboard::Modifiers,
    last_anchor: Option<Key>,
    theme: Theme,
    // Interactive grid
    manual_grids: HashMap<Key, Vec<CellState>>,
    drag_state: Option<CellState>,
    drag_preview: Option<DragPreview>,
    // Undo/redo stacks per puzzle (each entry is a full grid snapshot)
    undo_stack: HashMap<Key, Vec<Vec<CellState>>>,
    redo_stack: HashMap<Key, Vec<Vec<CellState>>>,
    // Trial mode: each entry is (snapshot_before_tier, first_cell_changed_in_tier)
    trial_stack: HashMap<Key, Vec<(Vec<CellState>, Option<(usize, usize)>)>>,
    // Hover cell (for run highlight)
    hover_cell: Option<(Key, usize, usize)>,
    // Crosshair position — None on each axis hides that axis.
    // Updated by CellEntered (cells) and GridRegionChanged (clue areas).
    crosshair_row: Option<usize>,
    crosshair_col: Option<usize>,
    // Window geometry (updated by events, persisted on close)
    window_id: Option<window::Id>,
    startup_maximize: bool,
    window_size: Size,
    window_pos: Point,
    // Grid pan/drag
    pan_offset: Vector,
    // Manual clue dimming
    manual_dim_rows: HashMap<Key, HashSet<(usize, usize)>>,
    manual_dim_cols: HashMap<Key, HashSet<(usize, usize)>>,
    // Settings
    cell_settings: CellSettings,
    assistance: AssistanceSettings,
    show_settings: bool,
    show_export_menu: bool,
    export_popup_x: f32,  // cursor x within the detail panel when export menu opened
    show_import_menu: bool,
    import_popup_x: f32,  // cursor x when import button was hovered
    last_cursor: Point,
    // Answer reveal spoiler state
    revealed_answers: HashSet<Key>,
    // Manually-solved puzzles: (relative_file_path, puzzle_name) — persisted across sessions
    solved_manually: HashSet<(String, String)>,
    // Folder-level collapse state
    folder_collapsed: HashMap<String, bool>,
    // Puzzle timers (session-only, not persisted)
    timers: HashMap<Key, TimerState>,
    // Incremental scan tracking
    pending_scans: usize,
    scan_total: usize,
    // URL import
    url_input: String,
    show_url_import: bool,
    // Keyboard-driven modes
    space_held: bool,
    focus_mode: bool,
    // Auto-hide toolbar in fullscreen: visible while cursor is near top edge
    toolbar_visible: bool,
    // Live solve progress snapshots (only populated during GraphSearch+progress solves)
    solve_progress: HashMap<Key, ProgressUpdate>,
    show_solve_progress: bool,
    // Batch-solve progress counters (reset each SolveSelected)
    solving_total: usize,
    solving_done: usize,
}

// ---------------------------------------------------------------------------
// Message
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub(crate) enum RunLengthChange {
    ShowStart,          StartThreshold(u32),
    ShowEnd,            EndThreshold(u32),
    ShowHover,          HoverThreshold(u32),
    AdjLabelEnabled,
    AdjLabelHPreferAfter,
    AdjLabelVPreferAfter,
    FourDirLabels,
    // 3×3 subcell picker: cycle clicked cell through Empty→H→V→Empty
    SubcellToggle(usize, usize),
    // Label text size
    NumSize(f32),
    // Per-axis custom label colors
    LabelHCustom,
    LabelHR(f32), LabelHG(f32), LabelHB(f32),
    LabelVCustom,
    LabelVR(f32), LabelVG(f32), LabelVB(f32),
}

#[derive(Debug, Clone)]
pub enum Message {
    // File operations
    ImportMenuToggled,
    ImportBtnHovered,
    ImportFileClicked,
    FileChosen(Option<String>),
    FileLoaded(String, String, Vec<ParsedPuzzle>),
    // Incremental scan: discovery → per-file parse
    FilePathsDiscovered(Vec<(String, String, Option<String>)>),
    LoadFileFound(String, String, Option<String>, Vec<ParsedPuzzle>),
    LoadFileFailed, // parse failed — just decrements pending counter
    // Refresh
    RefreshClicked,

    // Selection
    PuzzleToggled(Key, bool),
    FileToggled(usize, bool),
    PuzzleFocused(Key),
    Unfocus,

    // Solving
    SolverChanged(SolverKind),
    SolveSelected,
    SolveOneDone(SolverKind, Key, SolveResult),
    SolveDone(String),
    SolveProgress(Key, ProgressUpdate),
    SolveProgressToggled,
    CopyToManual(Key),

    // Exhaustive search
    FindAllSelected,
    FindAllDone(SolverKind, Vec<(Key, AllSolutions)>),
    SolutionPrev,
    SolutionNext,

    // Step navigation
    StepFirst,
    StepBack,
    StepForward,
    StepLast,
    ReplayToggle,
    ReplayTick,

    // List toolbar
    SelectAll,
    DeselectAll,
    ExpandAll,
    CollapseAll,

    // Collapse
    FileCollapseToggled(usize),
    FolderCollapseToggled(String),
    FolderToggled(String, bool),

    // Theme
    ThemeToggled,

    // Keyboard modifiers
    ModifiersChanged(keyboard::Modifiers),

    // Abort
    AbortClicked,

    // Interactive grid
    CellClicked { key: Key, row: usize, col: usize, right: bool },
    CellEntered { key: Key, row: usize, col: usize },
    GridLeft,
    DragEnded,

    // Settings
    SettingsOpened,
    SettingsClosed,
    // state_idx: 0=Unknown, 1=Filled, 2=Empty; channel: 0=R, 1=G, 2=B
    SettingColor(u8, u8, f32),
    SettingIcon(u8, Option<Bootstrap>),
    SettingClueBg(u8, f32),
    SettingSumBg(u8, f32),
    AssistToggle(AssistFlag),
    RunLengthChanged(RunLengthChange),
    CrosshairHColor(u8, f32),  // channel: 0=R, 1=G, 2=B, 3=A (horizontal / row axis)
    CrosshairVColor(u8, f32),  // channel: 0=R, 1=G, 2=B, 3=A (vertical / col axis)
    ClueDimToggle(Key, bool, usize, usize),  // (puzzle key, is_col, line_idx, clue_idx)

    // Grid editing
    ClearGrid(Key),
    UndoGrid(Key),
    RedoGrid(Key),

    // Trial mode
    TrialEnter,
    TrialAccept,
    TrialReject,

    // Puzzle timer
    TimerTick,
    TimerStart(Key),
    TimerPause(Key),
    TimerReset(Key),

    // Clipboard / export
    CopyPuzzleString(Key),
    CopyPuzzLink(Key),
    LogExportCopy(Key),
    LogExportSave(Key),
    LogFileSaved(Option<String>, Option<String>),
    ExportMenuToggled,
    ExportBtnHovered,
    CursorMoved(Point),
    ExportFormatSelected(ExportFormat),
    ExportSaved(String, Option<String>, Option<String>), // (content, path, error)

    // Grid pan
    PanOffsetChanged(Vector),
    // Crosshair axis visibility (fired by FrozenGridViewport on CursorMoved)
    GridRegionChanged(crate::frozen_grid_viewport::FrozenRegion),

    // Temporary debug
    DebugGridDump,

    // Window geometry / lifecycle
    WindowOpened(window::Id),
    WindowResized(Size),
    WindowMoved(Point),
    CloseRequested(window::Id),
    FinalizeClose { id: window::Id, maximized: bool },

    // Spoiler reveal
    RevealAnswer(Key),

    // URL import
    UrlImportToggled,
    UrlInputChanged(String),
    UrlFetchClicked,
    UrlFetched(Result<(String, Vec<ParsedPuzzle>), String>),

    // Keyboard-driven modes
    NamedKeyPressed(keyboard::key::Named),
    CharKeyPressed(String),
    SpaceReleased,
    FocusModeToggled,

    // Focus-key bindings changed in settings
    FocusKeyChanged(crate::app::settings::FocusKey),
    FocusKey2Changed(SecondaryFocusKey),

    // Errors
    Error(String),
}

// ---------------------------------------------------------------------------
// impl App
// ---------------------------------------------------------------------------

impl App {
    pub fn new() -> (Self, Task<Message>) {
        let (cell_settings, assistance) = load_settings();
        let (_, _, _, was_maximized) = persistence::load_window_state();
        let app = App {
            files: Vec::new(),
            selected: HashSet::new(),
            results: HashMap::new(),
            all_solutions: HashMap::new(),
            solution_index: 0,
            solver: SolverKind::Manual,
            focused: None,
            status: String::from("Loading default puzzle files…"),
            busy: true,
            step_cursor: 0,
            replaying: false,
            cancel: CancelToken::default(),
            modifiers: keyboard::Modifiers::default(),
            last_anchor: None,
            theme: Theme::Dark,
            manual_grids: HashMap::new(),
            drag_state: None,
            drag_preview: None,
            undo_stack: HashMap::new(),
            redo_stack: HashMap::new(),
            trial_stack: HashMap::new(),
            hover_cell: None,
            crosshair_row: None,
            crosshair_col: None,
            window_id: None,
            startup_maximize: was_maximized,
            window_size: Size::new(1200.0, 780.0),
            window_pos: Point::ORIGIN,
            pan_offset: Vector::ZERO,
            manual_dim_rows: HashMap::new(),
            manual_dim_cols: HashMap::new(),
            cell_settings,
            assistance,
            show_settings: false,
            show_export_menu: false,
            export_popup_x: 0.0,
            show_import_menu: false,
            import_popup_x: 0.0,
            last_cursor: Point::ORIGIN,
            revealed_answers: HashSet::new(),
            solved_manually: load_solved(),
            folder_collapsed: HashMap::new(),
            timers: HashMap::new(),
            pending_scans: 0,
            scan_total: 0,
            url_input: String::new(),
            show_url_import: false,
            space_held: false,
            focus_mode: false,
            toolbar_visible: true,
            solve_progress: HashMap::new(),
            show_solve_progress: false,
            solving_total: 0,
            solving_done: 0,
        };

        let task = Task::perform(scan_puzzle_dirs(), Message::FilePathsDiscovered);

        (app, task)
    }

    pub fn theme(&self) -> Theme {
        self.theme.clone()
    }

    pub fn subscription(&self) -> Subscription<Message> {
        let timer = if self.replaying {
            iced::time::every(std::time::Duration::from_millis(400))
                .map(|_| Message::ReplayTick)
        } else {
            Subscription::none()
        };

        let puzzle_timer = if self.timers.values().any(|t| t.running) {
            iced::time::every(std::time::Duration::from_secs(1))
                .map(|_| Message::TimerTick)
        } else {
            Subscription::none()
        };

        let kbd_and_mouse = iced::event::listen_with(|event, _, id| match event {
            Event::Keyboard(keyboard::Event::ModifiersChanged(mods)) => {
                Some(Message::ModifiersChanged(mods))
            }
            Event::Keyboard(keyboard::Event::KeyPressed { key: keyboard::Key::Named(n), .. }) => {
                Some(Message::NamedKeyPressed(n))
            }
            Event::Keyboard(keyboard::Event::KeyPressed { key: keyboard::Key::Character(c), .. }) => {
                Some(Message::CharKeyPressed(c.to_string()))
            }
            Event::Keyboard(keyboard::Event::KeyReleased { key: keyboard::Key::Named(keyboard::key::Named::Space), .. }) => {
                Some(Message::SpaceReleased)
            }
            Event::Mouse(mouse::Event::ButtonReleased(_)) => Some(Message::DragEnded),
            Event::Mouse(mouse::Event::CursorMoved { position }) => Some(Message::CursorMoved(position)),
            Event::Window(window::Event::Opened { .. }) => Some(Message::WindowOpened(id)),
            Event::Window(window::Event::Resized(sz)) => Some(Message::WindowResized(sz)),
            Event::Window(window::Event::Moved(pt)) => Some(Message::WindowMoved(pt)),
            Event::Window(window::Event::CloseRequested) => Some(Message::CloseRequested(id)),
            _ => None,
        });

        Subscription::batch([timer, puzzle_timer, kbd_and_mouse])
    }

    pub fn update(&mut self, msg: Message) -> Task<Message> {
        match msg {
            // ── File loading ──────────────────────────────────────────────
            Message::ImportMenuToggled => {
                self.show_import_menu = !self.show_import_menu;
                Task::none()
            }

            Message::ImportBtnHovered => {
                if !self.show_import_menu {
                    self.import_popup_x = self.last_cursor.x.max(0.0);
                }
                Task::none()
            }

            Message::ImportFileClicked => {
                self.show_import_menu = false;
                Task::perform(
                    async {
                        rfd::AsyncFileDialog::new()
                            .set_title("Import puzzle file")
                            .pick_file()
                            .await
                            .map(|h| h.path().to_string_lossy().into_owned())
                    },
                    Message::FileChosen,
                )
            }

            Message::FileChosen(Some(path)) => {
                let p2 = path.clone();
                let name = Path::new(&path)
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| path.clone());
                let n2 = name.clone();
                self.status = format!("Loading {name}…");
                self.busy = true;
                Task::perform(
                    async move {
                        let content = std::fs::read_to_string(&p2).map_err(|e| e.to_string())?;
                        let puzzles = if content.trim_start().starts_with("pzprv3") {
                            parse_pzprv3(&content)?
                        } else {
                            parse_file(&p2).map_err(|e| e.to_string())?
                        };
                        Ok((p2, n2, puzzles))
                    },
                    |r| match r {
                        Ok((path, name, puzzles)) => Message::FileLoaded(path, name, puzzles),
                        Err(e) => Message::Error(e),
                    },
                )
            }
            Message::FileChosen(None) => Task::none(),

            Message::FileLoaded(path, name, puzzles) => {
                let n_valid = puzzles.iter().filter(|e| e.is_valid()).count();
                let n_invalid = puzzles.len() - n_valid;
                self.status = if n_invalid > 0 {
                    format!("Loaded {n_valid} puzzle(s) from {name} ({n_invalid} invalid)")
                } else {
                    format!("Loaded {n_valid} puzzle(s) from {name}")
                };
                self.busy = false;
                if !self.files.iter().any(|f| f.path == path) {
                    self.files.push(LoadedFile { path, name, folder: None, auto_loaded: false, puzzles, collapsed: true });
                }
                Task::none()
            }

            Message::FilePathsDiscovered(paths) => {
                if paths.is_empty() {
                    self.busy = false;
                    self.status = "No default puzzles found in puzzles/".into();
                    return Task::none();
                }
                let n = paths.len();
                self.scan_total = n;
                self.pending_scans = n;
                self.status = format!("Loading 0 / {n} files…");
                Task::batch(paths.into_iter().map(|(path, name, folder)| {
                    Task::perform(
                        async move {
                            parse_file(&path).ok().map(|puzzles| (path, name, folder, puzzles))
                        },
                        |opt| match opt {
                            Some((p, n, f, pz)) => Message::LoadFileFound(p, n, f, pz),
                            None => Message::LoadFileFailed,
                        },
                    )
                }))
            }

            Message::LoadFileFound(path, name, folder, puzzles) => {
                self.pending_scans = self.pending_scans.saturating_sub(1);
                if let Some(existing) = self.files.iter_mut().find(|f| f.path == path) {
                    existing.puzzles = puzzles;
                    existing.folder = folder;
                } else {
                    if let Some(ref f) = folder {
                        self.folder_collapsed.entry(f.clone()).or_insert(true);
                    }
                    self.files.push(LoadedFile { path, name, folder, auto_loaded: true, puzzles, collapsed: true });
                }
                if self.pending_scans == 0 {
                    self.busy = false;
                    let (v, iv) = self.files.iter().filter(|f| f.auto_loaded).fold((0usize, 0usize), |(v, iv), f| {
                        let valid = f.puzzles.iter().filter(|e| e.is_valid()).count();
                        (v + valid, iv + f.puzzles.len() - valid)
                    });
                    self.status = if iv > 0 {
                        format!("Loaded {v} puzzle(s) from puzzles/ ({iv} invalid)")
                    } else {
                        format!("Loaded {v} puzzle(s) from puzzles/")
                    };
                    self.restore_session();
                } else {
                    let done = self.scan_total - self.pending_scans;
                    self.status = format!("Loading {done} / {} files…", self.scan_total);
                }
                Task::none()
            }

            Message::LoadFileFailed => {
                self.pending_scans = self.pending_scans.saturating_sub(1);
                if self.pending_scans == 0 {
                    self.busy = false;
                    let (v, iv) = self.files.iter().filter(|f| f.auto_loaded).fold((0usize, 0usize), |(v, iv), f| {
                        let valid = f.puzzles.iter().filter(|e| e.is_valid()).count();
                        (v + valid, iv + f.puzzles.len() - valid)
                    });
                    self.status = if v > 0 {
                        if iv > 0 { format!("Loaded {v} puzzle(s) from puzzles/ ({iv} invalid)") }
                        else { format!("Loaded {v} puzzle(s) from puzzles/") }
                    } else {
                        "No default puzzles found in puzzles/".into()
                    };
                    self.restore_session();
                }
                Task::none()
            }

            Message::RefreshClicked => {
                if self.busy { return Task::none(); }
                self.busy = true;
                self.status = "Rescanning puzzles/…".into();
                Task::perform(scan_puzzle_dirs(), Message::FilePathsDiscovered)
            }

            // ── List toolbar ──────────────────────────────────────────────
            Message::SelectAll => {
                for fi in 0..self.files.len() {
                    for (pi, entry) in self.files[fi].puzzles.iter().enumerate() {
                        if entry.is_valid() { self.selected.insert((fi, pi)); }
                    }
                }
                Task::none()
            }
            Message::DeselectAll => {
                self.selected.clear();
                Task::none()
            }
            Message::ExpandAll => {
                for file in &mut self.files { file.collapsed = false; }
                for v in self.folder_collapsed.values_mut() { *v = false; }
                Task::none()
            }
            Message::CollapseAll => {
                for file in &mut self.files { file.collapsed = true; }
                for v in self.folder_collapsed.values_mut() { *v = true; }
                Task::none()
            }

            Message::FileCollapseToggled(fi) => {
                if let Some(file) = self.files.get_mut(fi) {
                    file.collapsed = !file.collapsed;
                }
                Task::none()
            }

            Message::FolderCollapseToggled(folder) => {
                let entry = self.folder_collapsed.entry(folder).or_insert(true);
                *entry = !*entry;
                Task::none()
            }

            Message::FolderToggled(folder, checked) => {
                for (fi, file) in self.files.iter().enumerate() {
                    if file.folder.as_deref() == Some(folder.as_str()) {
                        for (pi, entry) in file.puzzles.iter().enumerate() {
                            if !entry.is_valid() { continue; }
                            if checked { self.selected.insert((fi, pi)); }
                            else { self.selected.remove(&(fi, pi)); }
                        }
                    }
                }
                Task::none()
            }

            // ── Theme ─────────────────────────────────────────────────────
            Message::ThemeToggled => {
                self.theme = match self.theme {
                    Theme::Light => Theme::Dark,
                    _            => Theme::Light,
                };
                Task::none()
            }

            // ── Modifiers ─────────────────────────────────────────────────
            Message::ModifiersChanged(mods) => {
                self.modifiers = mods;
                Task::none()
            }

            // ── Selection ─────────────────────────────────────────────────
            Message::PuzzleToggled(key, checked) => {
                if checked { self.selected.insert(key); } else { self.selected.remove(&key); }
                Task::none()
            }

            Message::FileToggled(fi, checked) => {
                if fi < self.files.len() {
                    for (pi, entry) in self.files[fi].puzzles.iter().enumerate() {
                        if !entry.is_valid() { continue; }
                        if checked { self.selected.insert((fi, pi)); }
                        else { self.selected.remove(&(fi, pi)); }
                    }
                }
                Task::none()
            }

            Message::PuzzleFocused(key) => {
                let (fi, pi) = key;
                let is_valid = self.files.get(fi)
                    .and_then(|f| f.puzzles.get(pi))
                    .map(|e| e.is_valid())
                    .unwrap_or(false);
                let shift = self.modifiers.shift();
                let ctrl  = self.modifiers.control() || self.modifiers.logo();
                if is_valid {
                    match (shift, ctrl) {
                        (false, false) => {
                            self.selected.clear();
                            self.selected.insert(key);
                            self.last_anchor = Some(key);
                        }
                        (false, true) => {
                            if self.selected.contains(&key) {
                                self.selected.remove(&key);
                            } else {
                                self.selected.insert(key);
                                self.last_anchor = Some(key);
                            }
                        }
                        (true, _) => {
                            let anchor = self.last_anchor.unwrap_or(key);
                            if !ctrl { self.selected.clear(); }
                            self.select_range(anchor, key);
                        }
                    }
                }
                self.focused = Some(key);
                self.replaying = false;
                self.show_export_menu = false;
                self.crosshair_row = None;
                self.crosshair_col = None;

                // Persist this puzzle as the last focused for next-session restore.
                if let Some(file) = self.files.get(fi) {
                    if let Some(puzzle) = file.puzzles.get(pi).and_then(|e| e.as_puzzle()) {
                        save_session(&relative_path(&file.path), &puzzle.name);
                    }
                }

                // Seed manual grid from persisted solution if we have no grid yet.
                if !self.manual_grids.contains_key(&key) {
                    let (fi, pi) = key;
                    if let Some(sol) = self.files.get(fi)
                        .and_then(|f| f.puzzles.get(pi))
                        .and_then(|e| e.as_puzzle())
                        .and_then(|p| p.solution.as_ref())
                    {
                        self.manual_grids.insert(key, sol.clone());
                    }
                }

                self.sync_step_cursor();
                Task::none()
            }

            Message::Unfocus => {
                self.focused = None;
                self.replaying = false;
                self.show_export_menu = false;
                self.crosshair_row = None;
                self.crosshair_col = None;
                Task::none()
            }

            // ── Solver ────────────────────────────────────────────────────
            Message::SolverChanged(kind) => {
                self.solver = kind;
                self.replaying = false;
                self.sync_step_cursor();
                Task::none()
            }

            Message::SolveSelected => {
                if !self.solver.is_machine() { return Task::none(); }
                let to_solve: Vec<(Key, Puzzle)> = self.selected.iter()
                    .filter_map(|&(fi, pi)| {
                        self.files.get(fi)?.puzzles.get(pi)?.as_puzzle().map(|p| ((fi, pi), p.clone()))
                    })
                    .collect();
                if to_solve.is_empty() { return Task::none(); }
                let solver = self.solver;
                let with_progress = solver == SolverKind::Cuttlefish && self.show_solve_progress;
                self.solving_total = to_solve.len();
                self.solving_done = 0;
                self.cancel.reset();
                self.busy = true;
                self.status = format!("Solving {} puzzle(s)…", to_solve.len());
                Task::run(solve_batch_stream(to_solve, solver, self.cancel.clone(), with_progress, 350), |msg| msg)
            }

            Message::SolveOneDone(solver_kind, key, result) => {
                if result.state == SolutionState::Complete {
                    if let Some(t) = self.timers.get_mut(&key) { t.running = false; }
                }
                if Some(key) == self.focused && solver_kind == self.solver {
                    self.step_cursor = result.steps.len();
                }
                self.results.insert((key, solver_kind), result);
                self.solving_done += 1;
                self.status = format!("Solving… {}/{} done", self.solving_done, self.solving_total);
                Task::none()
            }

            Message::SolveDone(status) => {
                self.solve_progress.clear();
                self.busy = false;
                self.status = status;
                Task::none()
            }

            Message::SolveProgress(key, update) => {
                if Some(key) == self.focused && self.solver == SolverKind::Cuttlefish {
                    self.status = format!(
                        "Solving… {}/{} cells ({:.0}%)  nodes expanded: {}  heap: {}",
                        update.cells_known, update.cells_total,
                        100.0 * update.cells_known as f32 / update.cells_total.max(1) as f32,
                        update.nodes_expanded,
                        update.heap_len,
                    );
                }
                self.solve_progress.insert(key, update);
                Task::none()
            }

            Message::SolveProgressToggled => {
                self.show_solve_progress = !self.show_solve_progress;
                Task::none()
            }

            Message::CopyToManual(key) => {
                let solver = self.solver;
                let Some(puzzle) = self.files.get(key.0)
                    .and_then(|f| f.puzzles.get(key.1))
                    .and_then(|e| e.as_puzzle())
                else { return Task::none(); };
                let Some(result) = self.results.get(&(key, solver)) else {
                    return Task::none();
                };
                let final_grid = result.grid.clone();
                // Cursor 0..n-1: blank through penultimate step. final_grid becomes the
                // new current state and must not also sit on top of the undo stack.
                let steps: Vec<_> = (0..result.steps.len())
                    .map(|c| grid_at_step(puzzle, result, c))
                    .collect();
                if let Some(current) = self.manual_grids.get(&key) {
                    self.undo_stack.entry(key).or_default().push(current.clone());
                }
                let undo = self.undo_stack.entry(key).or_default();
                for step_grid in steps {
                    undo.push(step_grid);
                }
                self.manual_grids.insert(key, final_grid);
                self.redo_stack.remove(&key);
                self.solver = SolverKind::Manual;
                Task::none()
            }

            Message::AbortClicked => {
                self.cancel.cancel();
                Task::none()
            }

            // ── Exhaustive search ─────────────────────────────────────────
            Message::FindAllSelected => {
                if !self.solver.supports_exhaustive() { return Task::none(); }
                let to_solve: Vec<(Key, Puzzle)> = self.selected.iter()
                    .filter_map(|&(fi, pi)| {
                        self.files.get(fi)?.puzzles.get(pi)?.as_puzzle().map(|p| ((fi, pi), p.clone()))
                    })
                    .collect();
                if to_solve.is_empty() { return Task::none(); }
                let solver = self.solver;
                self.cancel.reset();
                let ctx = SolveContext { cancel: self.cancel.clone() };
                self.busy = true;
                self.status = format!("Finding all solutions for {} puzzle(s)…", to_solve.len());
                Task::perform(
                    async move {
                        tokio::task::spawn_blocking(move || {
                            to_solve.into_iter()
                                .filter_map(|(key, puzzle)| {
                                    solver.solve_all(&puzzle, &ctx).map(|r| (key, r))
                                })
                                .collect::<Vec<_>>()
                        })
                        .await
                        .unwrap_or_default()
                    },
                    move |results| Message::FindAllDone(solver, results),
                )
            }

            Message::FindAllDone(solver_kind, results) => {
                let total    = results.iter().map(|(_, r)| r.solutions.len()).sum::<usize>();
                let expanded = results.iter().map(|(_, r)| r.nodes_expanded).sum::<usize>();
                let pushed   = results.iter().map(|(_, r)| r.nodes_pushed).sum::<usize>();
                let aborted  = results.iter().any(|(_, r)| r.aborted);
                for (key, result) in results {
                    if Some(key) == self.focused && solver_kind == self.solver {
                        self.solution_index = result.solutions.len().saturating_sub(1);
                        self.step_cursor = result.solutions
                            .get(self.solution_index)
                            .map(|r| r.steps.len())
                            .unwrap_or(0);
                    }
                    self.all_solutions.insert((key, solver_kind), result);
                }
                self.busy = false;
                self.status = if aborted {
                    format!("{total} solution(s) found (aborted) — expanded: {expanded}, pushed: {pushed}")
                } else {
                    format!("{total} solution(s) found — expanded: {expanded}, pushed: {pushed}")
                };
                Task::none()
            }

            Message::SolutionPrev => {
                if self.solution_index > 0 {
                    self.solution_index -= 1;
                    self.replaying = false;
                    if let Some(key) = self.focused {
                        self.step_cursor = self.all_solutions.get(&(key, self.solver))
                            .and_then(|a| a.solutions.get(self.solution_index))
                            .map(|r| r.steps.len())
                            .unwrap_or(0);
                    }
                }
                Task::none()
            }

            Message::SolutionNext => {
                if let Some(key) = self.focused {
                    let max_idx = self.all_solutions.get(&(key, self.solver))
                        .map(|a| a.solutions.len().saturating_sub(1))
                        .unwrap_or(0);
                    if self.solution_index < max_idx {
                        self.solution_index += 1;
                        self.replaying = false;
                        self.step_cursor = self.all_solutions.get(&(key, self.solver))
                            .and_then(|a| a.solutions.get(self.solution_index))
                            .map(|r| r.steps.len())
                            .unwrap_or(0);
                    }
                }
                Task::none()
            }

            // ── Step navigation ───────────────────────────────────────────
            Message::StepFirst => {
                self.step_cursor = 0;
                self.replaying = false;
                Task::none()
            }
            Message::StepBack => {
                self.step_cursor = self.step_cursor.saturating_sub(1);
                Task::none()
            }
            Message::StepForward => {
                if let Some(key) = self.focused {
                    let max = self.active_steps_len(key);
                    self.step_cursor = (self.step_cursor + 1).min(max);
                }
                Task::none()
            }
            Message::StepLast => {
                if let Some(key) = self.focused {
                    let max = self.active_steps_len(key);
                    self.step_cursor = max;
                }
                Task::none()
            }
            Message::ReplayToggle => {
                self.replaying = !self.replaying;
                if self.replaying {
                    if let Some(key) = self.focused {
                        let max = self.active_steps_len(key);
                        if self.step_cursor >= max { self.step_cursor = 0; }
                    } else {
                        self.replaying = false;
                    }
                }
                Task::none()
            }
            Message::ReplayTick => {
                if let Some(key) = self.focused {
                    let max = self.active_steps_len(key);
                    if self.step_cursor >= max {
                        self.replaying = false;
                    } else {
                        self.step_cursor += 1;
                    }
                } else {
                    self.replaying = false;
                }
                Task::none()
            }

            // ── Interactive grid ──────────────────────────────────────────
            Message::CellClicked { key, row, col, right } => {
                let (fi, pi) = key;
                let Some((w, h)) = self.files.get(fi)
                    .and_then(|f| f.puzzles.get(pi))
                    .and_then(|e| e.as_puzzle())
                    .map(|p| (p.width, p.height))
                else { return Task::none(); };

                // Initialize manual grid from current display if not yet present.
                if !self.manual_grids.contains_key(&key) {
                    let init = self.compute_display_grid(key).unwrap_or_else(|| {
                        vec![CellState::Unknown; w * h]
                    });
                    self.manual_grids.insert(key, init);
                }

                let target = {
                    let current = self.manual_grids[&key][row * w + col];
                    if right {
                        match current { CellState::Empty => CellState::Unknown, _ => CellState::Empty }
                    } else {
                        match current { CellState::Filled => CellState::Unknown, _ => CellState::Filled }
                    }
                };

                // Record origin cell for the current trial tier (first click only).
                if let Some(stack) = self.trial_stack.get_mut(&key) {
                    if let Some((_, origin)) = stack.last_mut() {
                        if origin.is_none() {
                            *origin = Some((row, col));
                        }
                    }
                }

                if self.assistance.axis_lock {
                    // Preview mode: defer all grid writes to DragEnded.
                    self.drag_preview = Some(DragPreview { key, anchor: (row, col), current: (row, col), paint: target });
                    self.drag_state = Some(target);
                } else {
                    // Immediate mode: write now.
                    {
                        let snapshot = self.manual_grids[&key].clone();
                        self.undo_stack.entry(key).or_default().push(snapshot);
                        self.redo_stack.remove(&key);
                    }
                    {
                        let mg = self.manual_grids.get_mut(&key).unwrap();
                        mg[row * w + col] = target;
                    }
                    self.drag_state = Some(target);
                    self.apply_line_assistance(key, row, col, target);
                    self.check_and_record_manual_solve(key);
                }
                Task::none()
            }

            Message::GridLeft => {
                self.hover_cell = None;
                self.crosshair_row = None;
                self.crosshair_col = None;
                Task::none()
            }

            Message::GridRegionChanged(region) => {
                use crate::frozen_grid_viewport::FrozenRegion;
                match region {
                    FrozenRegion::Cells => {
                        // crosshair_row/col kept current — updated by CellEntered
                    }
                    FrozenRegion::RowAxis(row) => {
                        self.hover_cell = None;
                        self.crosshair_row = Some(row);
                        self.crosshair_col = None;
                    }
                    FrozenRegion::ColAxis(col) => {
                        self.hover_cell = None;
                        self.crosshair_row = None;
                        self.crosshair_col = Some(col);
                    }
                    FrozenRegion::Other => {
                        self.hover_cell = None;
                        self.crosshair_row = None;
                        self.crosshair_col = None;
                    }
                }
                Task::none()
            }

            Message::CellEntered { key, row, col } => {
                self.hover_cell = Some((key, row, col));
                self.crosshair_row = Some(row);
                self.crosshair_col = Some(col);

                if let Some(target) = self.drag_state {
                    if self.assistance.axis_lock {
                        // Preview mode: track cursor position; actual paint deferred to DragEnded.
                        if let Some(ref mut p) = self.drag_preview {
                            if p.key == key { p.current = (row, col); }
                        }
                    } else {
                    let (fi, pi) = key;
                    let w = self.files.get(fi)
                        .and_then(|f| f.puzzles.get(pi))
                        .and_then(|e| e.as_puzzle())
                        .map(|p| p.width)
                        .unwrap_or(0);
                    if let Some(mg) = self.manual_grids.get_mut(&key) {
                        if row * w + col < mg.len() {
                            mg[row * w + col] = target;
                        }
                    }
                    self.apply_line_assistance(key, row, col, target);
                    self.check_and_record_manual_solve(key);
                    } // else (immediate mode)
                }
                Task::none()
            }

            Message::DragEnded => {
                if let Some(preview) = self.drag_preview.take() {
                    let DragPreview { key, anchor, current: cur, paint } = preview;
                    let (fi, pi) = key;
                    if let Some((w, h)) = self.files.get(fi)
                        .and_then(|f| f.puzzles.get(pi))
                        .and_then(|e| e.as_puzzle())
                        .map(|p| (p.width, p.height))
                    {
                        if !self.manual_grids.contains_key(&key) {
                            let init = self.compute_display_grid(key)
                                .unwrap_or_else(|| vec![CellState::Unknown; w * h]);
                            self.manual_grids.insert(key, init);
                        }
                        {
                            let snapshot = self.manual_grids[&key].clone();
                            self.undo_stack.entry(key).or_default().push(snapshot);
                            self.redo_stack.remove(&key);
                        }
                        let cells = preview_cells(anchor, cur);
                        {
                            let mg = self.manual_grids.get_mut(&key).unwrap();
                            for &(r, c) in &cells {
                                if r * w + c < mg.len() { mg[r * w + c] = paint; }
                            }
                        }
                        for &(r, c) in &cells {
                            self.apply_line_assistance(key, r, c, paint);
                        }
                        self.check_and_record_manual_solve(key);
                    }
                }
                self.drag_state = None;
                Task::none()
            }

            Message::TimerTick => {
                for t in self.timers.values_mut() {
                    if t.running { t.elapsed_secs += 1; }
                }
                Task::none()
            }
            Message::TimerStart(key) => {
                self.timers.entry(key).or_default().running = true;
                Task::none()
            }
            Message::TimerPause(key) => {
                if let Some(t) = self.timers.get_mut(&key) { t.running = false; }
                Task::none()
            }
            Message::TimerReset(key) => {
                self.timers.insert(key, TimerState::default());
                Task::none()
            }

            Message::PanOffsetChanged(offset) => {
                self.pan_offset = offset;
                self.hover_cell = None;
                self.crosshair_row = None;
                self.crosshair_col = None;
                Task::none()
            }

            Message::DebugGridDump => {
                crate::frozen_grid_viewport::request_debug_dump();
                Task::none()
            }

            // ── Settings ──────────────────────────────────────────────────
            Message::SettingsOpened => {
                self.show_settings = true;
                Task::none()
            }
            Message::SettingsClosed => {
                self.show_settings = false;
                Task::none()
            }
            Message::SettingColor(state_idx, channel, value) => {
                if let Some(vis) = self.cell_settings.visual_for_mut(state_idx) {
                    match channel {
                        0 => vis.color.r = value,
                        1 => vis.color.g = value,
                        2 => vis.color.b = value,
                        _ => {}
                    }
                }
                save_settings(&self.cell_settings, &self.assistance);
                Task::none()
            }
            Message::SettingIcon(state_idx, icon) => {
                if let Some(vis) = self.cell_settings.visual_for_mut(state_idx) {
                    vis.icon = icon;
                }
                save_settings(&self.cell_settings, &self.assistance);
                Task::none()
            }
            Message::SettingClueBg(channel, value) => {
                match channel {
                    0 => self.cell_settings.clue_bg.r = value,
                    1 => self.cell_settings.clue_bg.g = value,
                    2 => self.cell_settings.clue_bg.b = value,
                    _ => {}
                }
                save_settings(&self.cell_settings, &self.assistance);
                Task::none()
            }
            Message::SettingSumBg(channel, value) => {
                match channel {
                    0 => self.cell_settings.sum_bg.r = value,
                    1 => self.cell_settings.sum_bg.g = value,
                    2 => self.cell_settings.sum_bg.b = value,
                    _ => {}
                }
                save_settings(&self.cell_settings, &self.assistance);
                Task::none()
            }

            Message::AssistToggle(flag) => {
                match flag {
                    AssistFlag::AutoDim          => self.assistance.auto_dim            = !self.assistance.auto_dim,
                    AssistFlag::AutoFillEmpty    => self.assistance.auto_fill_empty     = !self.assistance.auto_fill_empty,
                    AssistFlag::ClueSumsWithGaps => self.assistance.clue_sums_with_gaps = !self.assistance.clue_sums_with_gaps,
                    AssistFlag::AutoCrossEdges   => self.assistance.auto_cross_edges    = !self.assistance.auto_cross_edges,
                    AssistFlag::CrosshairEnabled   => self.assistance.crosshair_enabled   = !self.assistance.crosshair_enabled,
                    AssistFlag::CrosshairSkipHover => self.assistance.crosshair_skip_hover = !self.assistance.crosshair_skip_hover,
                    AssistFlag::AxisLock           => self.assistance.axis_lock           = !self.assistance.axis_lock,
                }
                save_settings(&self.cell_settings, &self.assistance);
                Task::none()
            }

            Message::RunLengthChanged(change) => {
                let rl = &mut self.assistance.run_length;
                match change {
                    RunLengthChange::ShowStart          => rl.show_start      = !rl.show_start,
                    RunLengthChange::StartThreshold(v)  => rl.start_threshold = v,
                    RunLengthChange::ShowEnd            => rl.show_end        = !rl.show_end,
                    RunLengthChange::EndThreshold(v)    => rl.end_threshold   = v,
                    RunLengthChange::ShowHover          => rl.show_hover      = !rl.show_hover,
                    RunLengthChange::HoverThreshold(v)  => rl.hover_threshold = v,
                    RunLengthChange::AdjLabelEnabled => {
                        rl.adj_label_enabled = !rl.adj_label_enabled;
                        if rl.adj_label_enabled { rl.four_dir_labels_enabled = false; }
                    }
                    RunLengthChange::AdjLabelHPreferAfter => rl.adj_label_h_prefer_after = !rl.adj_label_h_prefer_after,
                    RunLengthChange::AdjLabelVPreferAfter => rl.adj_label_v_prefer_after = !rl.adj_label_v_prefer_after,
                    RunLengthChange::FourDirLabels => {
                        rl.four_dir_labels_enabled = !rl.four_dir_labels_enabled;
                        if rl.four_dir_labels_enabled { rl.adj_label_enabled = false; }
                    }
                    RunLengthChange::SubcellToggle(sr, sc) => {
                        use settings::SubcellKind;
                        rl.subcells[sr][sc] = match rl.subcells[sr][sc] {
                            SubcellKind::Empty => SubcellKind::H,
                            SubcellKind::H     => SubcellKind::V,
                            SubcellKind::V     => SubcellKind::Empty,
                        };
                    }
                    RunLengthChange::NumSize(v)      => rl.num_size           = v,
                    RunLengthChange::LabelHCustom    => rl.label_h_custom     = !rl.label_h_custom,
                    RunLengthChange::LabelHR(v)      => rl.label_h_r          = v,
                    RunLengthChange::LabelHG(v)      => rl.label_h_g          = v,
                    RunLengthChange::LabelHB(v)      => rl.label_h_b          = v,
                    RunLengthChange::LabelVCustom    => rl.label_v_custom     = !rl.label_v_custom,
                    RunLengthChange::LabelVR(v)      => rl.label_v_r          = v,
                    RunLengthChange::LabelVG(v)      => rl.label_v_g          = v,
                    RunLengthChange::LabelVB(v)      => rl.label_v_b          = v,
                }
                save_settings(&self.cell_settings, &self.assistance);
                Task::none()
            }

            Message::CrosshairHColor(channel, value) => {
                let c = &mut self.assistance.crosshair_h_color;
                match channel { 0 => c.r = value, 1 => c.g = value, 2 => c.b = value, 3 => c.a = value, _ => {} }
                save_settings(&self.cell_settings, &self.assistance);
                Task::none()
            }

            Message::CrosshairVColor(channel, value) => {
                let c = &mut self.assistance.crosshair_v_color;
                match channel { 0 => c.r = value, 1 => c.g = value, 2 => c.b = value, 3 => c.a = value, _ => {} }
                save_settings(&self.cell_settings, &self.assistance);
                Task::none()
            }

            Message::ClueDimToggle(key, is_col, line_idx, clue_idx) => {
                let map = if is_col { &mut self.manual_dim_cols } else { &mut self.manual_dim_rows };
                let set = map.entry(key).or_default();
                let entry = (line_idx, clue_idx);
                if !set.remove(&entry) { set.insert(entry); }
                Task::none()
            }

            Message::RevealAnswer(key) => {
                self.revealed_answers.insert(key);
                Task::none()
            }

            Message::TrialEnter => {
                if let Some(key) = self.focused {
                    let (fi, pi) = key;
                    let Some((w, h)) = self.files.get(fi)
                        .and_then(|f| f.puzzles.get(pi))
                        .and_then(|e| e.as_puzzle())
                        .map(|p| (p.width, p.height))
                    else { return Task::none(); };
                    // Snapshot = current displayed state
                    let snapshot = self.manual_grids.get(&key).cloned()
                        .or_else(|| self.compute_display_grid(key))
                        .unwrap_or_else(|| vec![CellState::Unknown; w * h]);
                    // Ensure manual_grids has this state so future clicks go into it
                    self.manual_grids.entry(key).or_insert_with(|| snapshot.clone());
                    self.trial_stack.entry(key).or_default().push((snapshot, None));
                }
                Task::none()
            }

            Message::TrialAccept => {
                if let Some(key) = self.focused {
                    if let Some(stack) = self.trial_stack.get_mut(&key) {
                        stack.pop(); // discard saved snapshot, keep current manual_grid as promoted state
                        if stack.is_empty() {
                            self.trial_stack.remove(&key);
                        }
                    }
                }
                Task::none()
            }

            Message::WindowOpened(id) => {
                self.window_id = Some(id);
                if self.startup_maximize {
                    self.startup_maximize = false;
                    window::maximize(id, true)
                } else {
                    Task::none()
                }
            }

            Message::WindowResized(size) => {
                self.window_size = size;
                Task::none()
            }

            Message::WindowMoved(pos) => {
                self.window_pos = pos;
                Task::none()
            }

            Message::CloseRequested(id) => {
                window::get_maximized(id).map(move |m| Message::FinalizeClose { id, maximized: m })
            }

            Message::FinalizeClose { id, maximized } => {
                save_window_state(
                    self.window_size.width, self.window_size.height,
                    self.window_pos.x, self.window_pos.y,
                    maximized,
                );
                window::close(id)
            }

            Message::CopyPuzzleString(key) => {
                let (fi, pi) = key;
                let Some(puzzle) = self.files.get(fi)
                    .and_then(|f| f.puzzles.get(pi))
                    .and_then(|e| match e {
                        nonogram_core::ParsedPuzzle::Valid(p) => Some(p),
                        nonogram_core::ParsedPuzzle::Invalid { puzzle: Some(p), .. } => Some(p),
                        _ => None,
                    })
                else { return Task::none(); };
                iced::clipboard::write(puzzle_to_file_string(puzzle))
            }

            Message::CopyPuzzLink(key) => {
                self.show_export_menu = false;
                let (fi, pi) = key;
                let Some(puzzle) = self.files.get(fi)
                    .and_then(|f| f.puzzles.get(pi))
                    .and_then(|e| match e {
                        nonogram_core::ParsedPuzzle::Valid(p) => Some(p),
                        nonogram_core::ParsedPuzzle::Invalid { puzzle: Some(p), .. } => Some(p),
                        _ => None,
                    })
                else { return Task::none(); };
                let url = puzzle_to_puzzlink_url(puzzle);
                self.status = "puzz.link URL copied to clipboard".to_string();
                iced::clipboard::write(url)
            }

            Message::LogExportCopy(key) => {
                let (fi, pi) = key;
                let Some(puzzle) = self.files.get(fi)
                    .and_then(|f| f.puzzles.get(pi))
                    .and_then(|e| e.as_puzzle())
                else { return Task::none(); };
                let Some(result) = self.results.get(&(key, self.solver)) else { return Task::none(); };
                let log = format_solve_log(&puzzle.name, puzzle.width, puzzle.height, &result.steps);
                self.status = "Solve log copied to clipboard".to_string();
                self.show_export_menu = false;
                iced::clipboard::write(log)
            }

            Message::LogExportSave(key) => {
                let (fi, pi) = key;
                let Some(puzzle) = self.files.get(fi)
                    .and_then(|f| f.puzzles.get(pi))
                    .and_then(|e| e.as_puzzle())
                else { return Task::none(); };
                let Some(result) = self.results.get(&(key, self.solver)) else { return Task::none(); };
                let log = format_solve_log(&puzzle.name, puzzle.width, puzzle.height, &result.steps);
                self.show_export_menu = false;
                Task::perform(
                    async move {
                        let handle = rfd::AsyncFileDialog::new()
                            .set_title("Save solve log")
                            .set_file_name("solve-log.txt")
                            .add_filter("Text files", &["txt"])
                            .save_file()
                            .await;
                        match handle {
                            Some(h) => {
                                let path = h.path().to_string_lossy().into_owned();
                                let err = std::fs::write(&path, &log).err().map(|e| e.to_string());
                                (Some(path), err)
                            }
                            None => (None, None),
                        }
                    },
                    |(path, err)| Message::LogFileSaved(path, err),
                )
            }

            Message::LogFileSaved(path, err) => {
                match (&path, &err) {
                    (Some(p), None)  => self.status = format!("Log saved to {p}"),
                    (_, Some(e))     => self.status = format!("Save failed: {e}"),
                    (None, None)     => {}
                }
                Task::none()
            }

            Message::CursorMoved(pos) => {
                self.last_cursor = pos;
                if self.focus_mode {
                    if pos.y <= 4.0 {
                        self.toolbar_visible = true;
                    } else if pos.y > 55.0 {
                        self.toolbar_visible = false;
                    }
                    // Between 4 and 55: hysteresis — keep current state so toolbar
                    // stays visible while the cursor is over it.
                }
                Task::none()
            }

            Message::ExportBtnHovered => {
                // Anchor the popup when the cursor enters the button (before the click).
                // Entering from the left positions cursor near the button's left edge.
                if !self.show_export_menu {
                    self.export_popup_x = (self.last_cursor.x - 311.0 - 10.0).max(0.0);
                }
                Task::none()
            }

            Message::ExportMenuToggled => {
                self.show_export_menu = !self.show_export_menu;
                Task::none()
            }

            Message::ExportFormatSelected(fmt) => {
                let Some(key) = self.focused else { return Task::none(); };
                let (fi, pi) = key;
                // Extract clue data (ends immutable borrow of self.files).
                let Some((row_clues, col_clues, w, h)) = self.files.get(fi)
                    .and_then(|f| f.puzzles.get(pi))
                    .and_then(|e| e.as_puzzle())
                    .map(|p| (p.row_clues.clone(), p.col_clues.clone(), p.width, p.height))
                else { return Task::none(); };

                // Current display grid: manual overrides solver.
                let display_grid = self.manual_grids.get(&key).cloned()
                    .or_else(|| self.compute_display_grid(key));

                // Fulfilled flags — only meaningful when auto_dim is on.
                let fulfilled_rows: Vec<bool> = if self.assistance.auto_dim {
                    if let Some(g) = &display_grid {
                        (0..h).map(|r| {
                            let cells: Vec<CellState> = (0..w).map(|c| g[r * w + c]).collect();
                            check_line_fulfilled(&row_clues[r], &cells)
                        }).collect()
                    } else { vec![false; h] }
                } else { vec![false; h] };

                let fulfilled_cols: Vec<bool> = if self.assistance.auto_dim {
                    if let Some(g) = &display_grid {
                        (0..w).map(|c| {
                            let cells: Vec<CellState> = (0..h).map(|r| g[r * w + c]).collect();
                            check_line_fulfilled(&col_clues[c], &cells)
                        }).collect()
                    } else { vec![false; w] }
                } else { vec![false; w] };

                let content = match fmt {
                    ExportFormat::PuzPreV3 => export_puzprv3(
                        &row_clues, &col_clues, w, h,
                        display_grid.as_deref(),
                        &fulfilled_rows,
                        &fulfilled_cols,
                    ),
                };

                self.show_export_menu = false;

                Task::perform(
                    async move {
                        let handle = rfd::AsyncFileDialog::new()
                            .set_title("Export puzzle")
                            .set_file_name("nonogram.txt")
                            .add_filter("Text files", &["txt"])
                            .save_file()
                            .await;
                        match handle {
                            Some(h) => {
                                let path = h.path().to_string_lossy().into_owned();
                                let err = std::fs::write(&path, &content)
                                    .err().map(|e| e.to_string());
                                (content, Some(path), err)
                            }
                            None => (content, None, None),
                        }
                    },
                    |(content, path, err)| Message::ExportSaved(content, path, err),
                )
            }

            Message::ExportSaved(content, path, err) => {
                match (&path, &err) {
                    (Some(p), None)  => self.status = format!("Exported to {p}"),
                    (_, Some(e))     => self.status = format!("Export failed: {e}"),
                    (None, None)     => {}  // user cancelled
                }
                if path.is_some() {
                    iced::clipboard::write(content)
                } else {
                    Task::none()
                }
            }

            Message::TrialReject => {
                if let Some(key) = self.focused {
                    if let Some(stack) = self.trial_stack.get_mut(&key) {
                        if let Some((snapshot, _)) = stack.pop() {
                            self.manual_grids.insert(key, snapshot);
                        }
                        if stack.is_empty() {
                            self.trial_stack.remove(&key);
                        }
                    }
                }
                Task::none()
            }

            Message::ClearGrid(key) => {
                if let Some(grid) = self.manual_grids.get(&key) {
                    let snapshot = grid.clone();
                    let stack = self.undo_stack.entry(key).or_default();
                    stack.push(snapshot);
                    if stack.len() > 500 { stack.remove(0); }
                    self.redo_stack.remove(&key);
                }
                let size = self.files.get(key.0)
                    .and_then(|f| f.puzzles.get(key.1))
                    .and_then(|e| e.as_puzzle())
                    .map(|p| p.width * p.height)
                    .unwrap_or(0);
                if size > 0 {
                    self.manual_grids.insert(key, vec![CellState::Unknown; size]);
                }
                Task::none()
            }

            Message::UndoGrid(key) => {
                if let Some(prev) = self.undo_stack.entry(key).or_default().pop() {
                    if let Some(current) = self.manual_grids.get(&key).cloned() {
                        self.redo_stack.entry(key).or_default().push(current);
                    }
                    self.manual_grids.insert(key, prev);
                }
                Task::none()
            }

            Message::RedoGrid(key) => {
                if let Some(next) = self.redo_stack.entry(key).or_default().pop() {
                    if let Some(current) = self.manual_grids.get(&key).cloned() {
                        self.undo_stack.entry(key).or_default().push(current);
                    }
                    self.manual_grids.insert(key, next);
                }
                Task::none()
            }

            // ── Keyboard modes ────────────────────────────────────────────
            Message::NamedKeyPressed(named) => {
                use keyboard::key::Named;
                match named {
                    Named::Space if !self.show_url_import => {
                        self.space_held = true;
                        Task::none()
                    }
                    // Esc always exits fullscreen (non-rebindable, exit-only).
                    Named::Escape if self.focus_mode => {
                        self.focus_mode = false;
                        self.apply_window_mode()
                    }
                    n if self.assistance.focus_key.matches(&n) => {
                        self.focus_mode = !self.focus_mode;
                        if self.focus_mode {
                            self.toolbar_visible = false;
                            self.pan_offset = self.center_pan_for_fullscreen();
                        }
                        self.apply_window_mode()
                    }
                    _ => Task::none()
                }
            }
            Message::CharKeyPressed(c) => {
                if self.modifiers.alt() || self.modifiers.control() || self.modifiers.logo() {
                    return Task::none();
                }
                if !self.show_url_import && self.assistance.focus_key2.matches_str(&c) {
                    self.focus_mode = !self.focus_mode;
                    if self.focus_mode {
                        self.toolbar_visible = false;
                        self.pan_offset = self.center_pan_for_fullscreen();
                    }
                    self.apply_window_mode()
                } else {
                    Task::none()
                }
            }
            Message::SpaceReleased => {
                self.space_held = false;
                Task::none()
            }
            Message::FocusModeToggled => {
                self.focus_mode = !self.focus_mode;
                if self.focus_mode {
                    self.toolbar_visible = false;
                    self.pan_offset = self.center_pan_for_fullscreen();
                }
                self.apply_window_mode()
            }
            Message::FocusKeyChanged(key) => {
                self.assistance.focus_key = key;
                save_settings(&self.cell_settings, &self.assistance);
                Task::none()
            }
            Message::FocusKey2Changed(key) => {
                self.assistance.focus_key2 = key;
                save_settings(&self.cell_settings, &self.assistance);
                Task::none()
            }

            // ── URL import ────────────────────────────────────────────────
            Message::UrlImportToggled => {
                self.show_url_import = !self.show_url_import;
                self.show_import_menu = false;
                Task::none()
            }

            Message::UrlInputChanged(s) => {
                self.url_input = s;
                Task::none()
            }

            Message::UrlFetchClicked => {
                let url = self.url_input.trim().to_string();
                if url.is_empty() || self.busy { return Task::none(); }
                if url.contains("puzz.link/p?nonogram") {
                    // Decode locally — no HTTP.
                    match parse_puzzlink_url(&url) {
                        Ok((name, puzzles)) => {
                            let n_valid = puzzles.iter().filter(|e| e.is_valid()).count();
                            self.status = format!("Imported {n_valid} puzzle(s) from puzz.link as \"{name}\"");
                            let path = format!("__url__{name}");
                            if !self.files.iter().any(|f| f.path == path) {
                                self.files.push(LoadedFile {
                                    path,
                                    name,
                                    folder: None,
                                    auto_loaded: false,
                                    puzzles,
                                    collapsed: false,
                                });
                            }
                            self.show_url_import = false;
                        }
                        Err(e) => {
                            self.status = format!("Import failed: {e}");
                        }
                    }
                    Task::none()
                } else {
                    self.status = format!("Fetching {url}…");
                    self.busy = true;
                    Task::perform(
                        url_import::fetch_puzzle_from_url(url),
                        Message::UrlFetched,
                    )
                }
            }

            Message::UrlFetched(Ok((name, puzzles))) => {
                let n_valid = puzzles.iter().filter(|e| e.is_valid()).count();
                self.status = format!("Imported {n_valid} puzzle(s) from URL as \"{name}\"");
                self.busy = false;
                let path = format!("__url__{name}");
                if !self.files.iter().any(|f| f.path == path) {
                    self.files.push(LoadedFile {
                        path,
                        name,
                        folder: None,
                        auto_loaded: false,
                        puzzles,
                        collapsed: false,
                    });
                }
                self.show_url_import = false;
                Task::none()
            }

            Message::UrlFetched(Err(e)) => {
                self.status = format!("Import failed: {e}");
                self.busy = false;
                Task::none()
            }

            Message::Error(e) => {
                self.status = format!("Error: {e}");
                self.busy = false;
                Task::none()
            }
        }
    }

    // ── Top-level view ───────────────────────────────────────────────────

    pub fn view(&self) -> Element<'_, Message> {
        let base: Element<Message> = if self.focus_mode {
            let puzzle = row![self.view_right_panel()].height(Length::Fill);
            if self.toolbar_visible {
                let overlay = container(column![self.view_toolbar(), horizontal_rule(1)])
                    .width(Length::Fill)
                    .style(|theme: &Theme| container::Style {
                        background: Some(theme.palette().background.into()),
                        ..Default::default()
                    });
                stack![puzzle, overlay].into()
            } else {
                puzzle.into()
            }
        } else {
            column![
                self.view_toolbar(),
                horizontal_rule(1),
                row![
                    self.view_left_panel(),
                    vertical_rule(1),
                    self.view_right_panel(),
                ]
                .height(Length::Fill),
                horizontal_rule(1),
                self.view_status_bar(),
            ]
            .into()
        };

        if self.show_import_menu {
            let items: Vec<Element<Message>> = vec![
                button(row![style::bi(Bootstrap::Folder).size(12), text("File").size(13)]
                    .spacing(6).align_y(Vertical::Center))
                    .on_press(Message::ImportFileClicked)
                    .width(Length::Fill)
                    .padding([5, 10])
                    .into(),
                button(row![style::bi(Bootstrap::Globe).size(12), text("URL").size(13)]
                    .spacing(6).align_y(Vertical::Center))
                    .on_press(Message::UrlImportToggled)
                    .width(Length::Fill)
                    .padding([5, 10])
                    .into(),
            ];
            let popup_layer: Element<Message> = column![
                Space::with_height(Length::Fixed(45.0)),
                container(style::popup_menu(items))
                    .padding(Padding { left: self.import_popup_x, ..Padding::ZERO }),
            ]
            .width(Length::Fill)
            .height(Length::Fill)
            .into();
            stack![base, popup_layer].into()
        } else {
            base
        }
    }


    // ── Right panel ──────────────────────────────────────────────────────

    fn view_right_panel(&self) -> Element<'_, Message> {
        if self.show_settings {
            return self.view_settings_panel();
        }
        if let Some((fi, pi)) = self.focused {
            self.view_puzzle_detail(fi, pi)
        } else if self.results.keys().any(|(_, sk)| *sk == self.solver) {
            self.view_results_table()
        } else {
            container(
                text("Select puzzles and press Solve, or click a puzzle to inspect it.")
                    .size(14)
                    .color(Color::from_rgb(0.45, 0.45, 0.45)),
            )
            .width(Length::Fill)
            .height(Length::Fill)
            .align_x(Horizontal::Center)
            .align_y(Vertical::Center)
            .into()
        }
    }

    // ── Helpers ──────────────────────────────────────────────────────────

    fn active_steps_len(&self, key: Key) -> usize {
        self.all_solutions.get(&(key, self.solver))
            .and_then(|a| a.solutions.get(self.solution_index))
            .or_else(|| self.results.get(&(key, self.solver)))
            .map(|r| r.steps.len())
            .unwrap_or(0)
    }

    fn select_range(&mut self, from: Key, to: Key) {
        let all: Vec<Key> = self.files.iter().enumerate()
            .flat_map(|(fi, f)| {
                f.puzzles.iter().enumerate()
                    .filter(|(_, e)| e.is_valid())
                    .map(move |(pi, _)| (fi, pi))
            })
            .collect();
        let a = all.iter().position(|&k| k == from);
        let b = all.iter().position(|&k| k == to);
        if let (Some(a), Some(b)) = (a, b) {
            let (lo, hi) = if a <= b { (a, b) } else { (b, a) };
            for &k in &all[lo..=hi] {
                self.selected.insert(k);
            }
        }
    }

    // Compute the solver-derived display grid for a key at the current step cursor.
    /// Called once when all default files finish loading. Looks up the saved
    /// session and, if the referenced puzzle still exists, focuses it and
    /// expands its folder/file in the left panel.
    fn restore_session(&mut self) {
        let Some((session_rel, session_name)) = load_session() else { return };
        let found = self.files.iter().enumerate().find_map(|(fi, file)| {
            if relative_path(&file.path) != session_rel { return None; }
            file.puzzles.iter().enumerate().find_map(|(pi, entry)| {
                if let ParsedPuzzle::Valid(p) = entry {
                    if p.name == session_name { Some((fi, pi)) } else { None }
                } else { None }
            })
        });
        let Some((fi, pi)) = found else { return };
        let key = (fi, pi);

        // Expand folder and file so the puzzle is visible in the left panel.
        if let Some(folder) = self.files[fi].folder.clone() {
            self.folder_collapsed.insert(folder, false);
        }
        self.files[fi].collapsed = false;

        // Focus the puzzle (mirrors the core of PuzzleFocused without modifier logic).
        self.selected.clear();
        self.selected.insert(key);
        self.last_anchor = Some(key);
        self.focused = Some(key);
        self.replaying = false;

        // Seed manual grid from persisted solution if needed.
        if !self.manual_grids.contains_key(&key) {
            if let Some(sol) = self.files.get(fi)
                .and_then(|f| f.puzzles.get(pi))
                .and_then(|e| e.as_puzzle())
                .and_then(|p| p.solution.as_ref())
            {
                self.manual_grids.insert(key, sol.clone());
            }
        }

        self.sync_step_cursor();
    }

    fn apply_window_mode(&self) -> Task<Message> {
        let mode = if self.focus_mode { window::Mode::Fullscreen } else { window::Mode::Windowed };
        if let Some(id) = self.window_id {
            window::change_mode(id, mode)
        } else {
            Task::none()
        }
    }

    /// Compute a pan_offset that centers the focused puzzle in the fullscreen
    /// viewport. Returns Vector::ZERO when no puzzle is focused or it is too
    /// large to center (content exceeds viewport on that axis).
    fn center_pan_for_fullscreen(&self) -> Vector {
        const C: f32 = 26.0; // cell px
        const N: f32 = 22.0; // clue-number cell px

        let Some((fi, pi)) = self.focused else { return Vector::ZERO };
        let Some(puzzle) = self.files.get(fi)
            .and_then(|f| f.puzzles.get(pi))
            .and_then(|e| e.as_puzzle())
        else { return Vector::ZERO };

        let max_rd = puzzle.row_clues.iter().map(|v| v.len()).max().unwrap_or(0).max(1);
        let max_cd = puzzle.col_clues.iter().map(|v| v.len()).max().unwrap_or(0).max(1);

        let corner_w = max_rd as f32 * N + 2.0;
        let corner_h = max_cd as f32 * N + 2.0;

        // cells area: dim * C + (dim-1) * 1px spacing
        let cells_w = puzzle.width  as f32 * (C + 1.0) - 1.0;
        let cells_h = puzzle.height as f32 * (C + 1.0) - 1.0;

        // sum panel width (clue-sum column appended to the right)
        let total_clue: u32 = puzzle.col_clues.iter().flat_map(|c| c.iter()).sum();
        let sum_w: f32 = match total_clue { 0..=9 => N, 10..=99 => N + 4.0, _ => N + 10.0 };

        // In fullscreen the detail panel fills the whole window; the
        // FrozenGridViewport container has 16px left padding.
        // Subtract ~40px vertically for the puzzle title header + separator.
        let vp_w = self.window_size.width  - 16.0;
        let vp_h = self.window_size.height - 40.0;

        // Round to whole pixels — fractional positions cause anti-aliased borders
        // that render 1-2px thicker and shift sub-pixel-aligned content.
        Vector::new(
            ((vp_w - corner_w - cells_w - sum_w) / 2.0).max(0.0).round(),
            ((vp_h - corner_h - cells_h)          / 2.0).max(0.0).round(),
        )
    }

    // ── Shared helpers ────────────────────────────────────────────────────

    /// Synchronise `solution_index` and `step_cursor` for the current focused
    /// puzzle and solver. Call after changing either `self.focused` or `self.solver`.
    fn sync_step_cursor(&mut self) {
        let Some(key) = self.focused else {
            self.solution_index = 0;
            self.step_cursor = 0;
            return;
        };
        let solver = self.solver;
        let all = self.all_solutions.get(&(key, solver));
        self.solution_index = all.map(|a| a.solutions.len().saturating_sub(1)).unwrap_or(0);
        self.step_cursor = all
            .and_then(|a| a.solutions.get(self.solution_index))
            .or_else(|| self.results.get(&(key, solver)))
            .map(|r| r.steps.len())
            .unwrap_or(0);
    }

    /// Apply auto-fill-empty and auto-cross-edges to the row and column
    /// touched by a single painted cell. No-op when a machine result exists
    /// or when neither assistance flag is enabled.
    fn apply_line_assistance(&mut self, key: Key, row: usize, col: usize, paint: CellState) {
        if paint == CellState::Unknown { return; }
        let auto_fill  = self.assistance.auto_fill_empty;
        let auto_cross = self.assistance.auto_cross_edges;
        if !auto_fill && !auto_cross { return; }
        let no_solver = self.results.get(&(key, self.solver)).is_none()
            && self.all_solutions.get(&(key, self.solver)).is_none();
        if !no_solver { return; }

        let (fi, pi) = key;
        let Some((row_clues, col_clues, w, h)) = self.files.get(fi)
            .and_then(|f| f.puzzles.get(pi))
            .and_then(|e| e.as_puzzle())
            .map(|p| (p.row_clues.clone(), p.col_clues.clone(), p.width, p.height))
        else { return; };
        let Some(mg) = self.manual_grids.get_mut(&key) else { return };

        if auto_fill {
            let row_cells: Vec<CellState> = (0..w).map(|c| mg[row * w + c]).collect();
            if check_line_fulfilled(&row_clues[row], &row_cells) {
                for c in 0..w { if mg[row * w + c] == CellState::Unknown { mg[row * w + c] = CellState::Empty; } }
            }
            let col_cells: Vec<CellState> = (0..h).map(|r| mg[r * w + col]).collect();
            if check_line_fulfilled(&col_clues[col], &col_cells) {
                for r in 0..h { if mg[r * w + col] == CellState::Unknown { mg[r * w + col] = CellState::Empty; } }
            }
        }
        if auto_cross {
            let row_cells: Vec<CellState> = (0..w).map(|c| mg[row * w + c]).collect();
            for c in forced_empty_from_edges(&row_clues[row], &row_cells) { mg[row * w + c] = CellState::Empty; }
            let col_cells: Vec<CellState> = (0..h).map(|r| mg[r * w + col]).collect();
            for r in forced_empty_from_edges(&col_clues[col], &col_cells) { mg[r * w + col] = CellState::Empty; }
        }
    }

    /// Check whether the current manual grid fully solves the puzzle and, if
    /// so, record it in `solved_manually`, stop the timer, and persist the
    /// solution back to the puzzle file.
    fn check_and_record_manual_solve(&mut self, key: Key) {
        let (fi, pi) = key;
        // Two separate lookups so neither borrow lives inside a closure that also
        // captures self — RA's closure borrow checker can't handle that combination.
        let meta = self.files.get(fi)
            .and_then(|f| f.puzzles.get(pi).and_then(|e| e.as_puzzle())
                .map(|p| (f.path.clone(), relative_path(&f.path), p.name.clone(), p.solution.is_none())));
        let Some((file_path, rel, name, no_file_solution)) = meta else { return };
        let fully_solved = {
            let puzzle = self.files.get(fi).and_then(|f| f.puzzles.get(pi)).and_then(|e| e.as_puzzle());
            let grid   = self.manual_grids.get(&key);
            matches!((puzzle, grid), (Some(p), Some(g)) if is_puzzle_fully_solved(p, g))
        };
        if !fully_solved { return; }
        if let Some(t) = self.timers.get_mut(&key) { t.running = false; }
        let newly_tracked = self.solved_manually.insert((rel, name.clone()));
        if newly_tracked {
            save_solved(&self.solved_manually);
            self.revealed_answers.insert(key);
        }
        if newly_tracked || no_file_solution {
            if let Some(grid) = self.manual_grids.get(&key) {
                let sol: String = grid.iter()
                    .map(|&c| if c == CellState::Filled { '1' } else { '0' })
                    .collect();
                save_solution_to_file(&file_path, &name, &sol);
            }
        }
    }

    fn compute_display_grid(&self, key: Key) -> Option<Vec<CellState>> {
        let (fi, pi) = key;
        let puzzle = self.files.get(fi)?.puzzles.get(pi)?.as_puzzle()?;
        let all    = self.all_solutions.get(&(key, self.solver));
        let result = self.results.get(&(key, self.solver));
        let active: Option<&SolveResult> = all
            .and_then(|a| a.solutions.get(self.solution_index))
            .or(result);
        let cursor = self.step_cursor;
        active.and_then(|r| {
            if r.grid.is_empty() { return None; }
            Some(if !r.steps.is_empty() && cursor < r.steps.len() {
                grid_at_step(puzzle, r, cursor)
            } else {
                r.grid.clone()
            })
        })
        .or_else(|| self.solve_progress.get(&key).map(|u| u.grid.clone()))
    }
}

// ---------------------------------------------------------------------------
// Returns a human-readable reason if the puzzle is trivially unsolvable,
// without invoking any solver.  pub(crate) so view modules can gate on it.
pub(crate) fn trivial_invalid_reason(puzzle: &Puzzle) -> Option<String> {
    let row_sum: u32 = puzzle.row_clues.iter().flat_map(|c| c.iter().copied()).sum();
    let col_sum: u32 = puzzle.col_clues.iter().flat_map(|c| c.iter().copied()).sum();
    if row_sum != col_sum {
        return Some(format!(
            "clue sums don't match (rows sum to {row_sum}, cols sum to {col_sum})"
        ));
    }
    let w = puzzle.col_clues.len() as u32;
    let h = puzzle.row_clues.len() as u32;
    for (i, clues) in puzzle.row_clues.iter().enumerate() {
        let min: u32 = clues.iter().sum::<u32>() + clues.len().saturating_sub(1) as u32;
        if min > w {
            return Some(format!(
                "row {} requires at least {min} cells but the grid is only {w} wide",
                i + 1
            ));
        }
    }
    for (i, clues) in puzzle.col_clues.iter().enumerate() {
        let min: u32 = clues.iter().sum::<u32>() + clues.len().saturating_sub(1) as u32;
        if min > h {
            return Some(format!(
                "col {} requires at least {min} cells but the grid is only {h} tall",
                i + 1
            ));
        }
    }
    None
}

// Streaming batch solve — emits SolveOneDone per puzzle, SolveDone at end.
// ---------------------------------------------------------------------------

fn solve_batch_stream(
    to_solve: Vec<(Key, Puzzle)>,
    solver: SolverKind,
    cancel: CancelToken,
    with_progress: bool,
    snapshot_ms: u64,
) -> impl iced::futures::Stream<Item = Message> {
    use std::time::Duration;
    use iced::futures::SinkExt as _;
    iced::stream::channel(128, move |mut sender| async move {
        let n = to_solve.len();
        let mut n_solved = 0usize;
        let mut n_aborted = 0usize;
        let mut n_invalid = 0usize;
        let cfg = with_progress.then(|| ProgressConfig {
            snapshot_interval: Some(Duration::from_millis(snapshot_ms)),
            emit_start_snapshot: true,
            ..Default::default()
        });
        for (key, puzzle) in to_solve {
            if let Some(reason) = trivial_invalid_reason(&puzzle) {
                n_invalid += 1;
                let result = SolveResult {
                    state: SolutionState::Invalid(reason),
                    grid: vec![],
                    steps: vec![],
                };
                let _ = sender.send(Message::SolveOneDone(solver, key, result)).await;
                continue;
            }
            let s2 = sender.clone();
            let ctx = SolveContext { cancel: cancel.clone() };
            let cfg2 = cfg.clone();
            let result = tokio::task::spawn_blocking(move || {
                match cfg2 {
                    Some(cfg) => {
                        let mut s3 = s2;
                        let mut cb = move |u: ProgressUpdate| {
                            let _ = s3.try_send(Message::SolveProgress(key, u));
                        };
                        GraphSearchSolver.solve_with_progress(&puzzle, &ctx, &cfg, Some(&mut cb))
                    }
                    None => solver.solve(&puzzle, &ctx),
                }
            })
            .await
            .unwrap_or_else(|_| SolveResult {
                state: SolutionState::Aborted,
                grid: vec![],
                steps: vec![],
            });
            if result.state == SolutionState::Aborted { n_aborted += 1; } else { n_solved += 1; }
            let _ = sender.send(Message::SolveOneDone(solver, key, result)).await;
        }
        let n_attempted = n - n_invalid;
        let mut parts: Vec<String> = vec![format!("{n_solved}/{n_attempted} solved")];
        if n_invalid > 0 { parts.push(format!("{n_invalid} invalid")); }
        if n_aborted > 0 { parts.push(format!("{n_aborted} aborted")); }
        let status = format!("Done: {}", parts.join(", "));
        let _ = sender.send(Message::SolveDone(status)).await;
    })
}
