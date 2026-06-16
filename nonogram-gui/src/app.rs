use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use directories::ProjectDirs;
use serde::{Deserialize, Serialize};

use iced::{
    alignment::{Horizontal, Vertical},
    keyboard, mouse,
    widget::{
        button, checkbox, column, container, horizontal_rule, mouse_area,
        pick_list, row, scrollable, self as widget, slider, text, vertical_rule, Space,
    },
    Color, Element, Event, Font, Length, Padding, Point, Subscription, Task, Theme,
};
use iced_fonts::bootstrap::{self, Bootstrap};
use nonogram_core::{
    parse_file, AllSolutions, CancelToken, CellState, ParsedPuzzle, Puzzle,
    SolveContext, SolveResult, SolutionState,
};

use crate::convert::convert_letter_content;
use crate::solver::SolverKind;

const BOOTSTRAP_FONT: Font = Font::with_name("bootstrap-icons");

fn bi(icon: Bootstrap) -> iced::widget::Text<'static> {
    text(bootstrap::icon_to_char(icon).to_string()).font(BOOTSTRAP_FONT)
}

fn icon_char(icon: Option<Bootstrap>) -> Option<char> {
    icon.map(bootstrap::icon_to_char)
}

// ---------------------------------------------------------------------------
// Cell visual settings
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct CellVisual {
    pub color: Color,
    pub icon: Option<Bootstrap>,
}

impl CellVisual {
    fn icon_color(&self) -> Color {
        let lum = 0.299 * self.color.r + 0.587 * self.color.g + 0.114 * self.color.b;
        if lum > 0.5 {
            Color::from_rgb(0.15, 0.15, 0.15)
        } else {
            Color::WHITE
        }
    }
}

#[derive(Clone, Debug)]
pub struct CellSettings {
    pub unknown: CellVisual,
    pub filled: CellVisual,
    pub empty: CellVisual,
    pub clue_bg: Color,
}

impl Default for CellSettings {
    fn default() -> Self {
        Self {
            unknown: CellVisual { color: Color::from_rgb(0.72, 0.76, 0.82), icon: None },
            filled:  CellVisual { color: Color::from_rgb(0.10, 0.10, 0.15), icon: None },
            empty:   CellVisual { color: Color::WHITE, icon: None },
            clue_bg: Color::from_rgb(0.97, 0.97, 0.97),
        }
    }
}

impl CellSettings {
    fn visual_for(&self, state: CellState) -> &CellVisual {
        match state {
            CellState::Unknown => &self.unknown,
            CellState::Filled  => &self.filled,
            CellState::Empty   => &self.empty,
        }
    }

    fn visual_for_mut(&mut self, state_idx: u8) -> Option<&mut CellVisual> {
        match state_idx {
            0 => Some(&mut self.unknown),
            1 => Some(&mut self.filled),
            2 => Some(&mut self.empty),
            _ => None,
        }
    }
}

// Icons available for cell states in settings.
const ICON_OPTIONS: &[Option<Bootstrap>] = &[
    None,
    Some(Bootstrap::CircleFill),
    Some(Bootstrap::SquareFill),
    Some(Bootstrap::DiamondFill),
    Some(Bootstrap::CheckLg),
    Some(Bootstrap::XLg),
    Some(Bootstrap::DashLg),
];

// ---------------------------------------------------------------------------
// Settings persistence
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize)]
struct SavedCellVisual {
    r: f32,
    g: f32,
    b: f32,
    /// Index into ICON_OPTIONS (0 = no icon).
    icon_idx: usize,
}

fn default_clue_bg_channel() -> f32 { 0.97 }

#[derive(Serialize, Deserialize)]
struct SavedSettings {
    unknown: SavedCellVisual,
    filled:  SavedCellVisual,
    empty:   SavedCellVisual,
    #[serde(default = "default_clue_bg_channel")] clue_bg_r: f32,
    #[serde(default = "default_clue_bg_channel")] clue_bg_g: f32,
    #[serde(default = "default_clue_bg_channel")] clue_bg_b: f32,
}

fn icon_to_idx(icon: Option<Bootstrap>) -> usize {
    let ch = icon_char(icon);
    ICON_OPTIONS.iter().position(|&opt| icon_char(opt) == ch).unwrap_or(0)
}

fn idx_to_icon(idx: usize) -> Option<Bootstrap> {
    ICON_OPTIONS.get(idx).copied().flatten()
}

fn visual_to_saved(vis: &CellVisual) -> SavedCellVisual {
    SavedCellVisual {
        r: vis.color.r,
        g: vis.color.g,
        b: vis.color.b,
        icon_idx: icon_to_idx(vis.icon),
    }
}

fn saved_to_visual(s: SavedCellVisual) -> CellVisual {
    CellVisual {
        color: Color { r: s.r, g: s.g, b: s.b, a: 1.0 },
        icon: idx_to_icon(s.icon_idx),
    }
}

fn config_path() -> Option<PathBuf> {
    ProjectDirs::from("", "nonogram", "nonogram-gui")
        .map(|pd| pd.config_dir().join("settings.json"))
}

fn load_settings() -> CellSettings {
    let path = match config_path() {
        Some(p) => p,
        None    => return CellSettings::default(),
    };
    let content = match std::fs::read_to_string(&path) {
        Ok(s)  => s,
        Err(_) => return CellSettings::default(),
    };
    let saved: SavedSettings = match serde_json::from_str(&content) {
        Ok(s)  => s,
        Err(_) => return CellSettings::default(),
    };
    CellSettings {
        unknown: saved_to_visual(saved.unknown),
        filled:  saved_to_visual(saved.filled),
        empty:   saved_to_visual(saved.empty),
        clue_bg: Color { r: saved.clue_bg_r, g: saved.clue_bg_g, b: saved.clue_bg_b, a: 1.0 },
    }
}

fn save_settings(settings: &CellSettings) {
    let Some(path) = config_path() else { return };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let saved = SavedSettings {
        unknown:  visual_to_saved(&settings.unknown),
        filled:   visual_to_saved(&settings.filled),
        empty:    visual_to_saved(&settings.empty),
        clue_bg_r: settings.clue_bg.r,
        clue_bg_g: settings.clue_bg.g,
        clue_bg_b: settings.clue_bg.b,
    };
    if let Ok(json) = serde_json::to_string_pretty(&saved) {
        let _ = std::fs::write(&path, json);
    }
}

// ---------------------------------------------------------------------------
// Theme-aware style helpers
// ---------------------------------------------------------------------------

fn style_panel(theme: &Theme) -> container::Style {
    container::Style {
        background: Some(theme.extended_palette().background.base.color.into()),
        ..Default::default()
    }
}

fn style_header_row(theme: &Theme) -> container::Style {
    container::Style {
        background: Some(theme.extended_palette().background.weak.color.into()),
        ..Default::default()
    }
}

fn style_chevron_btn(theme: &Theme, _: button::Status) -> button::Style {
    button::Style {
        background: None,
        text_color: theme.extended_palette().background.weak.text,
        border: iced::Border::default(),
        shadow: iced::Shadow::default(),
    }
}

fn style_list_row_btn(theme: &Theme, status: button::Status) -> button::Style {
    let p = theme.extended_palette();
    button::Style {
        background: match status {
            button::Status::Hovered => Some(p.primary.weak.color.into()),
            _ => None,
        },
        text_color: p.background.base.text,
        border: iced::Border::default(),
        shadow: iced::Shadow::default(),
    }
}

// ---------------------------------------------------------------------------
// Grid step helper
// ---------------------------------------------------------------------------

fn grid_at_step(puzzle: &Puzzle, result: &SolveResult, cursor: usize) -> Vec<CellState> {
    if result.steps.is_empty() || cursor >= result.steps.len() {
        return result.grid.clone();
    }
    let mut grid = vec![CellState::Unknown; puzzle.width * puzzle.height];
    for step in &result.steps[..cursor] {
        for &(r, c, state) in &step.cells_changed {
            grid[r * puzzle.width + c] = state;
        }
    }
    grid
}

// ---------------------------------------------------------------------------
// Natural sort (numbers in strings compared numerically, e.g. 5x5 < 10x10)
// ---------------------------------------------------------------------------

fn natural_cmp(a: &std::path::Path, b: &std::path::Path) -> std::cmp::Ordering {
    natural_str_cmp(&a.to_string_lossy(), &b.to_string_lossy())
}

fn natural_str_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    let mut ai = a.chars().peekable();
    let mut bi = b.chars().peekable();
    loop {
        match (ai.peek().copied(), bi.peek().copied()) {
            (None, None) => return std::cmp::Ordering::Equal,
            (None, _)    => return std::cmp::Ordering::Less,
            (_, None)    => return std::cmp::Ordering::Greater,
            (Some(ac), Some(bc)) if ac.is_ascii_digit() && bc.is_ascii_digit() => {
                let an = collect_digits(&mut ai);
                let bn = collect_digits(&mut bi);
                match an.cmp(&bn) {
                    std::cmp::Ordering::Equal => {}
                    ord => return ord,
                }
            }
            (Some(ac), Some(bc)) => {
                let ac = ac.to_ascii_lowercase();
                let bc = bc.to_ascii_lowercase();
                ai.next();
                bi.next();
                match ac.cmp(&bc) {
                    std::cmp::Ordering::Equal => {}
                    ord => return ord,
                }
            }
        }
    }
}

fn collect_digits(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) -> u64 {
    let mut n = 0u64;
    while let Some(&c) = chars.peek() {
        if c.is_ascii_digit() {
            n = n * 10 + (c as u64 - b'0' as u64);
            chars.next();
        } else {
            break;
        }
    }
    n
}

// ---------------------------------------------------------------------------
// Puzzle directory scanner (path discovery only — no parsing)
// ---------------------------------------------------------------------------

async fn scan_puzzle_dirs() -> Vec<(String, String, Option<String>)> {
    let mut out = Vec::new();
    let dir = "puzzles";
    if let Ok(entries) = std::fs::read_dir(dir) {
        let mut paths: Vec<_> = entries.flatten().map(|e| e.path()).collect();
        paths.sort_by(|a, b| natural_cmp(a, b));
        for path in paths {
            if path.is_file() && path.extension().map(|e| e == "txt").unwrap_or(false) {
                let path_str = path.to_string_lossy().into_owned();
                let name = path.file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| path_str.clone());
                out.push((path_str, name, None));
            } else if path.is_dir() {
                let folder_name = path.file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                if let Ok(sub_entries) = std::fs::read_dir(&path) {
                    let mut sub_paths: Vec<_> = sub_entries
                        .flatten()
                        .map(|e| e.path())
                        .filter(|p| p.is_file() && p.extension().map(|e| e == "txt").unwrap_or(false))
                        .collect();
                    sub_paths.sort_by(|a, b| natural_cmp(a, b));
                    for sub_path in sub_paths {
                        let sub_str = sub_path.to_string_lossy().into_owned();
                        let name = sub_path.file_name()
                            .map(|n| n.to_string_lossy().into_owned())
                            .unwrap_or_else(|| sub_str.clone());
                        out.push((sub_str, name, Some(folder_name.clone())));
                    }
                }
            }
        }
    }
    out
}

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
// App
// ---------------------------------------------------------------------------

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
    // Trial mode: each entry is (snapshot_before_tier, first_cell_changed_in_tier)
    trial_stack: HashMap<Key, Vec<(Vec<CellState>, Option<(usize, usize)>)>>,
    // Grid pan/drag
    pan_dragging: bool,
    last_cursor_pos: Option<Point>,
    grid_scroll_offset: HashMap<Key, widget::scrollable::AbsoluteOffset>,
    // Settings
    cell_settings: CellSettings,
    show_settings: bool,
    // Answer reveal spoiler state
    revealed_answers: HashSet<Key>,
    // Folder-level collapse state
    folder_collapsed: HashMap<String, bool>,
    // Incremental scan tracking
    pending_scans: usize,
    scan_total: usize,
}

// ---------------------------------------------------------------------------
// Message
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub enum Message {
    // File operations
    ImportClicked,
    ConvertClicked,
    FileChosen(Option<String>),
    ConvertChosen(Option<String>),
    FileLoaded(String, String, Vec<ParsedPuzzle>),
    ConvertLoaded(String, Vec<ParsedPuzzle>),
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
    SolveAll,
    SolveDone(SolverKind, Vec<(Key, SolveResult)>),

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
    DragEnded,

    // Settings
    SettingsOpened,
    SettingsClosed,
    // state_idx: 0=Unknown, 1=Filled, 2=Empty; channel: 0=R, 1=G, 2=B
    SettingColor(u8, u8, f32),
    SettingIcon(u8, Option<Bootstrap>),
    SettingClueBg(u8, f32),

    // Trial mode
    TrialEnter,
    TrialReject,

    // Clipboard
    CopyPuzzleString(Key),

    // Grid pan
    PanStarted,
    CursorMoved(Point),
    GridScrolled(widget::scrollable::AbsoluteOffset),

    // Spoiler reveal
    RevealAnswer(Key),

    // Errors
    Error(String),
}

// ---------------------------------------------------------------------------
// impl App
// ---------------------------------------------------------------------------

impl App {
    pub fn new() -> (Self, Task<Message>) {
        let app = App {
            files: Vec::new(),
            selected: HashSet::new(),
            results: HashMap::new(),
            all_solutions: HashMap::new(),
            solution_index: 0,
            solver: SolverKind::GraphSearch,
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
            trial_stack: HashMap::new(),
            pan_dragging: false,
            last_cursor_pos: None,
            grid_scroll_offset: HashMap::new(),
            cell_settings: load_settings(),
            show_settings: false,
            revealed_answers: HashSet::new(),
            folder_collapsed: HashMap::new(),
            pending_scans: 0,
            scan_total: 0,
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

        let kbd_and_mouse = iced::event::listen_with(|event, status, _| match event {
            Event::Keyboard(keyboard::Event::ModifiersChanged(mods)) => {
                Some(Message::ModifiersChanged(mods))
            }
            Event::Mouse(mouse::Event::ButtonReleased(_)) => Some(Message::DragEnded),
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left))
                if status == iced::event::Status::Ignored =>
            {
                Some(Message::PanStarted)
            }
            Event::Mouse(mouse::Event::CursorMoved { position }) => {
                Some(Message::CursorMoved(position))
            }
            _ => None,
        });

        Subscription::batch([timer, kbd_and_mouse])
    }

    pub fn update(&mut self, msg: Message) -> Task<Message> {
        match msg {
            // ── File loading ──────────────────────────────────────────────
            Message::ImportClicked => Task::perform(
                async {
                    rfd::AsyncFileDialog::new()
                        .set_title("Import puzzle file")
                        .add_filter("Nonogram puzzles", &["txt"])
                        .pick_file()
                        .await
                        .map(|h| h.path().to_string_lossy().into_owned())
                },
                Message::FileChosen,
            ),

            Message::ConvertClicked => Task::perform(
                async {
                    rfd::AsyncFileDialog::new()
                        .set_title("Convert letter-encoded puzzle file")
                        .add_filter("Text files", &["txt"])
                        .pick_file()
                        .await
                        .map(|h| h.path().to_string_lossy().into_owned())
                },
                Message::ConvertChosen,
            ),

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
                        parse_file(&p2).map(|p| (p2, n2, p)).map_err(|e| e.to_string())
                    },
                    |r| match r {
                        Ok((path, name, puzzles)) => Message::FileLoaded(path, name, puzzles),
                        Err(e) => Message::Error(e),
                    },
                )
            }
            Message::FileChosen(None) => Task::none(),

            Message::ConvertChosen(Some(path)) => {
                let p2 = path.clone();
                let name = Path::new(&path)
                    .file_stem()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| path.clone());
                self.status = format!("Converting {name}…");
                self.busy = true;
                Task::perform(
                    async move {
                        let content = std::fs::read_to_string(&p2)
                            .map_err(|e| e.to_string())?;
                        let puzzles = convert_letter_content(&content, &name)
                            .into_iter().map(ParsedPuzzle::Valid).collect();
                        Ok::<_, String>((name, puzzles))
                    },
                    |r| match r {
                        Ok((name, puzzles)) => Message::ConvertLoaded(name, puzzles),
                        Err(e) => Message::Error(e),
                    },
                )
            }
            Message::ConvertChosen(None) => Task::none(),

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

            Message::ConvertLoaded(name, puzzles) => {
                let n_valid = puzzles.iter().filter(|e| e.is_valid()).count();
                self.status = format!("Converted {n_valid} puzzle(s) as \"{name}\"");
                self.busy = false;
                let display = format!("{name} (converted)");
                self.files.push(LoadedFile {
                    path: format!("__converted__{name}"),
                    name: display,
                    folder: None,
                    auto_loaded: false,
                    puzzles,
                    collapsed: true,
                });
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
                let all = self.all_solutions.get(&(key, self.solver));
                self.solution_index = all.map(|a| a.solutions.len().saturating_sub(1)).unwrap_or(0);
                self.step_cursor = all
                    .and_then(|a| a.solutions.get(self.solution_index))
                    .or_else(|| self.results.get(&(key, self.solver)))
                    .map(|r| r.steps.len())
                    .unwrap_or(0);
                let init = widget::scrollable::AbsoluteOffset { x: PAN_CENTER, y: PAN_CENTER };
                let offset = *self.grid_scroll_offset.entry(key).or_insert(init);
                widget::scrollable::scroll_to(grid_scroll_id(), offset)
            }

            Message::Unfocus => {
                self.focused = None;
                self.replaying = false;
                Task::none()
            }

            // ── Solver ────────────────────────────────────────────────────
            Message::SolverChanged(kind) => {
                self.solver = kind;
                self.replaying = false;
                let all = self.focused.and_then(|key| self.all_solutions.get(&(key, kind)));
                self.solution_index = all.map(|a| a.solutions.len().saturating_sub(1)).unwrap_or(0);
                self.step_cursor = all
                    .and_then(|a| a.solutions.get(self.solution_index))
                    .or_else(|| self.focused.and_then(|key| self.results.get(&(key, kind))))
                    .map(|r| r.steps.len())
                    .unwrap_or(0);
                Task::none()
            }

            Message::SolveSelected => {
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
                self.status = format!("Solving {} puzzle(s)…", to_solve.len());
                Task::perform(
                    async move {
                        tokio::task::spawn_blocking(move || {
                            to_solve.into_iter()
                                .map(|(key, puzzle)| (key, solver.solve(&puzzle, &ctx)))
                                .collect::<Vec<_>>()
                        })
                        .await
                        .unwrap_or_default()
                    },
                    move |results| Message::SolveDone(solver, results),
                )
            }

            Message::SolveAll => {
                let to_solve: Vec<(Key, Puzzle)> = self.files.iter().enumerate()
                    .flat_map(|(fi, f)| {
                        f.puzzles.iter().enumerate()
                            .filter_map(move |(pi, e)| e.as_puzzle().map(|p| ((fi, pi), p.clone())))
                    })
                    .collect();
                if to_solve.is_empty() { return Task::none(); }
                let solver = self.solver;
                self.cancel.reset();
                let ctx = SolveContext { cancel: self.cancel.clone() };
                self.busy = true;
                self.status = format!("Solving {} puzzle(s)…", to_solve.len());
                Task::perform(
                    async move {
                        tokio::task::spawn_blocking(move || {
                            to_solve.into_iter()
                                .map(|(key, puzzle)| (key, solver.solve(&puzzle, &ctx)))
                                .collect::<Vec<_>>()
                        })
                        .await
                        .unwrap_or_default()
                    },
                    move |results| Message::SolveDone(solver, results),
                )
            }

            Message::SolveDone(solver_kind, results) => {
                let n = results.len();
                let solved  = results.iter().filter(|(_, r)| r.state == SolutionState::Complete).count();
                let aborted = results.iter().filter(|(_, r)| r.state == SolutionState::Aborted).count();
                for (key, result) in results {
                    if Some(key) == self.focused && solver_kind == self.solver {
                        self.step_cursor = result.steps.len();
                    }
                    self.results.insert((key, solver_kind), result);
                }
                self.busy = false;
                self.status = if aborted > 0 {
                    format!("Done: {solved}/{n} solved, {aborted} aborted")
                } else {
                    format!("Done: {solved}/{n} fully solved")
                };
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

                let mg = self.manual_grids.get_mut(&key).unwrap();
                let current = mg[row * w + col];
                let target = if right {
                    match current {
                        CellState::Empty => CellState::Unknown,
                        _                => CellState::Empty,
                    }
                } else {
                    match current {
                        CellState::Filled => CellState::Unknown,
                        _                 => CellState::Filled,
                    }
                };
                mg[row * w + col] = target;
                self.drag_state = Some(target);
                // Record origin cell for the current trial tier (first click only)
                if let Some(stack) = self.trial_stack.get_mut(&key) {
                    if let Some((_, origin)) = stack.last_mut() {
                        if origin.is_none() {
                            *origin = Some((row, col));
                        }
                    }
                }
                Task::none()
            }

            Message::CellEntered { key, row, col } => {
                if let Some(target) = self.drag_state {
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
                }
                Task::none()
            }

            Message::DragEnded => {
                self.drag_state = None;
                self.pan_dragging = false;
                Task::none()
            }

            Message::PanStarted => {
                self.pan_dragging = true;
                Task::none()
            }

            Message::CursorMoved(pos) => {
                let delta = self.last_cursor_pos.map(|last| (last.x - pos.x, last.y - pos.y));
                self.last_cursor_pos = Some(pos);
                if self.pan_dragging {
                    if let Some((dx, dy)) = delta {
                        if let Some(key) = self.focused {
                            let init = widget::scrollable::AbsoluteOffset { x: PAN_CENTER, y: PAN_CENTER };
                            let entry = self.grid_scroll_offset.entry(key).or_insert(init);
                            entry.x = (entry.x + dx).max(0.0);
                            entry.y = (entry.y + dy).max(0.0);
                            let offset = *entry;
                            return widget::scrollable::scroll_to(grid_scroll_id(), offset);
                        }
                    }
                }
                Task::none()
            }

            Message::GridScrolled(offset) => {
                if !self.pan_dragging {
                    if let Some(key) = self.focused {
                        self.grid_scroll_offset.insert(key, offset);
                    }
                }
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
                save_settings(&self.cell_settings);
                Task::none()
            }
            Message::SettingIcon(state_idx, icon) => {
                if let Some(vis) = self.cell_settings.visual_for_mut(state_idx) {
                    vis.icon = icon;
                }
                save_settings(&self.cell_settings);
                Task::none()
            }
            Message::SettingClueBg(channel, value) => {
                match channel {
                    0 => self.cell_settings.clue_bg.r = value,
                    1 => self.cell_settings.clue_bg.g = value,
                    2 => self.cell_settings.clue_bg.b = value,
                    _ => {}
                }
                save_settings(&self.cell_settings);
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

            Message::Error(e) => {
                self.status = format!("Error: {e}");
                self.busy = false;
                Task::none()
            }
        }
    }

    // ── Top-level view ───────────────────────────────────────────────────

    pub fn view(&self) -> Element<'_, Message> {
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
    }

    // ── Toolbar ──────────────────────────────────────────────────────────

    fn view_toolbar(&self) -> Element<'_, Message> {
        let solve_sel = {
            let b = button("Solve Selected");
            if !self.selected.is_empty() && !self.busy {
                b.on_press(Message::SolveSelected)
            } else {
                b
            }
        };
        let solve_all = {
            let b = button("Solve All");
            let has = self.files.iter().any(|f| f.puzzles.iter().any(|e| e.is_valid()));
            if has && !self.busy { b.on_press(Message::SolveAll) } else { b }
        };
        let abort = {
            let b = button(
                row![bi(Bootstrap::XCircleFill).size(13), text("Abort").size(13)]
                    .spacing(4)
                    .align_y(Vertical::Center),
            );
            if self.busy { b.on_press(Message::AbortClicked) } else { b }
        };

        let theme_icon = match self.theme {
            Theme::Light => Bootstrap::MoonFill,
            _            => Bootstrap::SunFill,
        };

        let settings_btn = button(bi(Bootstrap::GearFill).size(14))
            .on_press(if self.show_settings {
                Message::SettingsClosed
            } else {
                Message::SettingsOpened
            })
            .padding([4, 8]);

        let refresh_btn = {
            let b = button(bi(Bootstrap::ArrowClockwise).size(14)).padding([4, 8]);
            if !self.busy { b.on_press(Message::RefreshClicked) } else { b }
        };

        row![
            button("Import File").on_press(Message::ImportClicked),
            button("Convert File").on_press(Message::ConvertClicked),
            refresh_btn,
            Space::with_width(Length::Fill),
            button(bi(theme_icon).size(14))
                .on_press(Message::ThemeToggled)
                .padding([4, 8]),
            settings_btn,
            text("Solver:").size(14),
            pick_list(SolverKind::ALL, Some(self.solver), Message::SolverChanged),
            Space::with_width(Length::Fixed(12.0)),
            solve_sel,
            solve_all,
            {
                let b = button("Find All");
                if self.solver.supports_exhaustive() && !self.selected.is_empty() && !self.busy {
                    b.on_press(Message::FindAllSelected)
                } else {
                    b
                }
            },
            abort,
        ]
        .spacing(8)
        .padding(8)
        .align_y(Vertical::Center)
        .into()
    }

    // ── File row renderer (shared by top-level and folder-nested files) ────

    fn render_file_rows<'a>(
        &'a self,
        fi: usize,
        file: &'a LoadedFile,
        indent: f32,
        items: &mut Vec<Element<'a, Message>>,
    ) {
        let valid_count = file.puzzles.iter().filter(|e| e.is_valid()).count();
        let sel_count = file.puzzles.iter().enumerate()
            .filter(|(pi, e)| e.is_valid() && self.selected.contains(&(fi, *pi)))
            .count();
        let all_sel = valid_count > 0 && sel_count == valid_count;
        let chevron = if file.collapsed { Bootstrap::ChevronRight } else { Bootstrap::ChevronDown };
        let count_label = if file.puzzles.len() > valid_count {
            format!("{sel_count}/{valid_count} (+{} invalid)", file.puzzles.len() - valid_count)
        } else {
            format!("{sel_count}/{valid_count}")
        };

        let mut hdr: Vec<Element<Message>> = Vec::new();
        if indent > 0.0 { hdr.push(Space::with_width(Length::Fixed(indent)).into()); }
        hdr.push(button(bi(chevron).size(11)).on_press(Message::FileCollapseToggled(fi)).padding([2, 4]).style(style_chevron_btn).into());
        hdr.push(checkbox("", all_sel).on_toggle(move |c| Message::FileToggled(fi, c)).into());
        hdr.push(text(file.name.as_str()).size(13).into());
        hdr.push(Space::with_width(Length::Fill).into());
        hdr.push(text(count_label).size(11).into());
        items.push(container(row(hdr).spacing(4).padding(Padding { top: 4.0, right: 20.0, bottom: 4.0, left: 8.0 }).align_y(Vertical::Center)).style(style_header_row).into());

        if !file.collapsed {
            for (pi, entry) in file.puzzles.iter().enumerate() {
                let key = (fi, pi);
                let is_focused = self.focused == Some(key);

                match entry {
                    ParsedPuzzle::Valid(puzzle) => {
                        let is_sel = self.selected.contains(&key);
                        let badge_icon: Option<Bootstrap> = self.results.get(&(key, self.solver)).map(|r| match &r.state {
                            SolutionState::Complete   => Bootstrap::CheckLg,
                            SolutionState::Aborted    => Bootstrap::XCircleFill,
                            SolutionState::Partial    => Bootstrap::DashLg,
                            SolutionState::Unsolvable => Bootstrap::XLg,
                            SolutionState::Invalid(_) => Bootstrap::ExclamationCircleFill,
                        });
                        let btn_content: Element<Message> = if let Some(icon) = badge_icon {
                            row![text(puzzle.name.as_str()).size(13), bi(icon).size(11)]
                                .spacing(4).align_y(Vertical::Center).into()
                        } else {
                            text(puzzle.name.as_str()).size(13).into()
                        };
                        let name_btn = button(btn_content)
                            .on_press(Message::PuzzleFocused(key))
                            .style(move |theme: &Theme, status| {
                                let p = theme.extended_palette();
                                if is_focused {
                                    button::Style { background: Some(p.primary.strong.color.into()), text_color: p.primary.strong.text, border: iced::Border::default(), shadow: iced::Shadow::default() }
                                } else {
                                    button::Style { background: match status { button::Status::Hovered => Some(p.primary.weak.color.into()), _ => None }, text_color: p.background.base.text, border: iced::Border::default(), shadow: iced::Shadow::default() }
                                }
                            })
                            .padding([2, 6]);
                        let mut prow: Vec<Element<Message>> = Vec::new();
                        if indent > 0.0 { prow.push(Space::with_width(Length::Fixed(indent)).into()); }
                        prow.push(Space::with_width(Length::Fixed(20.0)).into());
                        prow.push(checkbox("", is_sel).on_toggle(move |c| Message::PuzzleToggled(key, c)).into());
                        prow.push(name_btn.into());
                        prow.push(Space::with_width(Length::Fill).into());
                        prow.push(text(format!("{}x{}", puzzle.width, puzzle.height)).size(10).into());
                        items.push(container(row(prow).spacing(4).padding(Padding { top: 2.0, right: 20.0, bottom: 2.0, left: 6.0 }).align_y(Vertical::Center)).style(style_panel).into());
                    }
                    ParsedPuzzle::Invalid { name, .. } => {
                        let name_btn = button(
                            row![bi(Bootstrap::ExclamationCircleFill).size(11).color(Color::from_rgb(0.75, 0.38, 0.0)), text(name.as_str()).size(13)]
                                .spacing(4).align_y(Vertical::Center),
                        )
                        .on_press(Message::PuzzleFocused(key))
                        .style(move |theme: &Theme, status| {
                            let p = theme.extended_palette();
                            if is_focused {
                                button::Style { background: Some(p.primary.strong.color.into()), text_color: p.primary.strong.text, border: iced::Border::default(), shadow: iced::Shadow::default() }
                            } else {
                                button::Style { background: match status { button::Status::Hovered => Some(p.primary.weak.color.into()), _ => None }, text_color: Color::from_rgb(0.75, 0.38, 0.0), border: iced::Border::default(), shadow: iced::Shadow::default() }
                            }
                        })
                        .padding([2, 6]);
                        let mut prow: Vec<Element<Message>> = Vec::new();
                        if indent > 0.0 { prow.push(Space::with_width(Length::Fixed(indent)).into()); }
                        prow.push(Space::with_width(Length::Fixed(20.0)).into());
                        prow.push(Space::with_width(Length::Fixed(22.0)).into());
                        prow.push(name_btn.into());
                        items.push(container(row(prow).spacing(4).padding(Padding { top: 2.0, right: 20.0, bottom: 2.0, left: 6.0 }).align_y(Vertical::Center)).style(style_panel).into());
                    }
                }
            }
        }
        items.push(horizontal_rule(1).into());
    }

    // ── Left panel (file/puzzle list) ────────────────────────────────────

    fn view_left_panel(&self) -> Element<'_, Message> {
        let total_valid: usize = self.files.iter()
            .map(|f| f.puzzles.iter().filter(|e| e.is_valid()).count())
            .sum();
        let all_selected = total_valid > 0
            && self.files.iter().enumerate().all(|(fi, f)| {
                f.puzzles.iter().enumerate()
                    .filter(|(_, e)| e.is_valid())
                    .all(|(pi, _)| self.selected.contains(&(fi, pi)))
            });
        let any_collapsed = self.files.iter().any(|f| f.collapsed)
            || self.folder_collapsed.values().any(|&v| v);

        let collapse_toggle: Element<Message> = if any_collapsed {
            button(bi(Bootstrap::ChevronDoubleRight).size(11))
                .on_press(Message::ExpandAll)
                .padding([3, 6])
                .into()
        } else {
            button(bi(Bootstrap::ChevronDoubleDown).size(11))
                .on_press(Message::CollapseAll)
                .padding([3, 6])
                .into()
        };

        let list_toolbar = container(
            row![
                checkbox("", all_selected)
                    .on_toggle(|v| if v { Message::SelectAll } else { Message::DeselectAll }),
                bi(Bootstrap::CheckAll).size(14),
                Space::with_width(Length::Fill),
                collapse_toggle,
            ]
            .padding([4, 8])
            .align_y(Vertical::Center),
        )
        .style(style_header_row)
        .width(Length::Fill);

        let mut items: Vec<Element<Message>> = Vec::new();

        if self.files.is_empty() {
            items.push(
                container(text("No files loaded").size(13))
                    .padding([16, 12])
                    .into(),
            );
        }

        // Separate into top-level files and folder groups (sorted by folder name).
        // Store only indices to avoid lifetime confusion with &LoadedFile in a local map.
        let mut top_level: Vec<usize> = Vec::new();
        let mut by_folder: std::collections::BTreeMap<String, Vec<usize>> =
            std::collections::BTreeMap::new();
        for (fi, file) in self.files.iter().enumerate() {
            match &file.folder {
                None         => top_level.push(fi),
                Some(folder) => by_folder.entry(folder.clone()).or_default().push(fi),
            }
        }

        for (folder_name, fis) in &by_folder {
            let folder_collapsed = *self.folder_collapsed.get(folder_name).unwrap_or(&true);

            let folder_valid: usize = fis.iter()
                .flat_map(|&fi| self.files[fi].puzzles.iter().enumerate().map(move |(pi, e)| ((fi, pi), e)))
                .filter(|(_, e)| e.is_valid())
                .count();
            let folder_sel: usize = fis.iter()
                .flat_map(|&fi| self.files[fi].puzzles.iter().enumerate().map(move |(pi, e)| ((fi, pi), e)))
                .filter(|(key, e)| e.is_valid() && self.selected.contains(key))
                .count();
            let folder_all_sel = folder_valid > 0 && folder_sel == folder_valid;

            let chevron = if folder_collapsed { Bootstrap::ChevronRight } else { Bootstrap::ChevronDown };
            let fn_toggle = folder_name.clone();
            let fn_select = folder_name.clone();

            let folder_hdr = row![
                button(bi(chevron).size(11))
                    .on_press(Message::FolderCollapseToggled(fn_toggle))
                    .padding([2, 4])
                    .style(style_chevron_btn),
                checkbox("", folder_all_sel)
                    .on_toggle(move |c| Message::FolderToggled(fn_select.clone(), c)),
                bi(Bootstrap::Folder).size(12),
                text(folder_name.clone()).size(13),
                Space::with_width(Length::Fill),
                text(format!("{folder_sel}/{folder_valid}")).size(11),
            ]
            .spacing(4)
            .padding(Padding { top: 4.0, right: 20.0, bottom: 4.0, left: 8.0 })
            .align_y(Vertical::Center);

            items.push(container(folder_hdr).style(style_header_row).width(Length::Fill).into());

            if !folder_collapsed {
                for &fi in fis {
                    self.render_file_rows(fi, &self.files[fi], 12.0, &mut items);
                }
            }
        }

        for &fi in &top_level {
            self.render_file_rows(fi, &self.files[fi], 0.0, &mut items);
        }

        container(
            column![
                list_toolbar,
                horizontal_rule(1),
                scrollable(
                    column(items).spacing(1)
                ).height(Length::Fill),
            ]
        )
        .style(style_panel)
        .width(Length::Fixed(310.0))
        .height(Length::Fill)
        .into()
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

    fn view_puzzle_detail(&self, fi: usize, pi: usize) -> Element<'_, Message> {
        let Some(file) = self.files.get(fi) else {
            return text("(missing file)").into();
        };
        let entry = match file.puzzles.get(pi) {
            Some(e) => e,
            None => return text("(missing puzzle)").into(),
        };

        let key = (fi, pi);

        // ── Build shared header elements ──────────────────────────────────
        let back_btn = button(
            row![bi(Bootstrap::ArrowLeft).size(13), text("Back").size(13)]
                .spacing(4).align_y(Vertical::Center),
        )
        .on_press(Message::Unfocus);

        // ── Invalid puzzle ────────────────────────────────────────────────
        // ParsedPuzzle::Invalid carries an optional Puzzle when the error is a
        // clue-total mismatch (the structure is fully parseable, just inconsistent).
        // Hard parse errors (bad tokens, missing sections) have puzzle = None.
        if let ParsedPuzzle::Invalid { name, reason, puzzle: partial } = entry {
            let status_el = row![
                bi(Bootstrap::ExclamationCircleFill).size(13)
                    .color(Color::from_rgb(0.75, 0.38, 0.0)),
                text("Invalid puzzle").size(13),
            ]
            .spacing(4)
            .align_y(Vertical::Center);

            let header = container(
                row![
                    back_btn,
                    Space::with_width(Length::Fixed(12.0)),
                    text(name.as_str()).size(16),
                    Space::with_width(Length::Fill),
                    status_el,
                ]
                .spacing(4)
                .align_y(Vertical::Center),
            )
            .padding([10, 16])
            .width(Length::Fill);

            let reason_banner = container(
                row![
                    bi(Bootstrap::ExclamationCircleFill).size(13)
                        .color(Color::from_rgb(0.75, 0.38, 0.0)),
                    text(reason.to_string()).size(13),
                ]
                .spacing(8)
                .align_y(Vertical::Center),
            )
            .style(|_| container::Style {
                background: Some(Color::from_rgba(0.75, 0.38, 0.0, 0.12).into()),
                border: iced::Border {
                    radius: 4.0.into(),
                    color: Color::from_rgb(0.75, 0.38, 0.0),
                    width: 1.0,
                },
                ..Default::default()
            })
            .padding([8, 16])
            .width(Length::Fill);

            if let Some(puzzle) = partial {
                // Clue-total mismatch: we have the full puzzle structure, so
                // show the grid below the warning (sums will highlight in orange).
                let display_grid = self.manual_grids.get(&key).cloned();
                let trial_info: &[(Vec<CellState>, Option<(usize, usize)>)] =
                    self.trial_stack.get(&key).map(|v| v.as_slice()).unwrap_or(&[]);
                return column![
                    header,
                    horizontal_rule(1),
                    container(reason_banner).padding([8, 16]).width(Length::Fill),
                    horizontal_rule(1),
                    container(view_grid(puzzle, display_grid, key, &self.cell_settings, trial_info))
                        .padding(Padding { left: 16.0, ..Padding::ZERO })
                        .width(Length::Fill)
                        .height(Length::Fill),
                ]
                .width(Length::Fill)
                .height(Length::Fill)
                .into();
            }

            return column![
                header,
                horizontal_rule(1),
                container(reason_banner)
                    .padding(16)
                    .width(Length::Fill)
                    .height(Length::Fill),
            ]
            .width(Length::Fill)
            .height(Length::Fill)
            .into();
        }

        let ParsedPuzzle::Valid(puzzle) = entry else { unreachable!() };

        let result = self.results.get(&(key, self.solver));
        let all    = self.all_solutions.get(&(key, self.solver));
        let active: Option<&SolveResult> = all
            .and_then(|a| a.solutions.get(self.solution_index))
            .or(result);

        // Solver-derived grid state (respects step cursor).
        let grid_data: Option<Vec<CellState>> = {
            let cursor = self.step_cursor;
            active.and_then(|r| {
                if r.grid.is_empty() { return None; }
                Some(if !r.steps.is_empty() && cursor < r.steps.len() {
                    grid_at_step(puzzle, r, cursor)
                } else {
                    r.grid.clone()
                })
            })
        };

        // Use manual grid when we're at the end of step replay (or no steps).
        let at_end = active
            .map(|r| r.steps.is_empty() || self.step_cursor >= r.steps.len())
            .unwrap_or(true);
        let display_grid: Option<Vec<CellState>> = if at_end {
            self.manual_grids.get(&key).cloned().or(grid_data)
        } else {
            grid_data
        };

        // ── Status indicator ──────────────────────────────────────────────
        let status_el: Element<Message> = if let Some(a) = all {
            let n      = a.solutions.len();
            let suffix = if a.aborted { " (aborted)" } else { "" };
            match n {
                0 => row![
                    bi(Bootstrap::XLg).size(13).color(Color::from_rgb(0.78, 0.08, 0.08)),
                    text(format!("No solutions{suffix}")).size(13),
                ].spacing(4).align_y(Vertical::Center).into(),
                1 => row![
                    bi(Bootstrap::CheckLg).size(13).color(Color::from_rgb(0.08, 0.55, 0.08)),
                    text(format!("Unique solution{suffix}")).size(13),
                ].spacing(4).align_y(Vertical::Center).into(),
                _ => row![
                    bi(Bootstrap::DashLg).size(13).color(Color::from_rgb(0.65, 0.45, 0.0)),
                    text(format!("{n} solutions — ambiguous{suffix}")).size(13),
                ].spacing(4).align_y(Vertical::Center).into(),
            }
        } else if let Some(res) = result {
            match &res.state {
                SolutionState::Complete => {
                    let mut elems: Vec<Element<Message>> = vec![
                        bi(Bootstrap::CheckLg).size(13).color(Color::from_rgb(0.08, 0.55, 0.08)).into(),
                        text("Solved").size(13).into(),
                    ];
                    if let Some(answer) = &puzzle.answer {
                        elems.push(Space::with_width(Length::Fixed(8.0)).into());
                        if self.revealed_answers.contains(&key) {
                            elems.push(
                                text(format!("\"{answer}\"")).size(13)
                                    .color(Color::from_rgb(0.08, 0.55, 0.08))
                                    .into()
                            );
                        } else {
                            elems.push(
                                button(text("Reveal answer").size(12))
                                    .on_press(Message::RevealAnswer(key))
                                    .padding([1, 8])
                                    .into()
                            );
                        }
                    }
                    row(elems).spacing(4).align_y(Vertical::Center).into()
                }
                SolutionState::Aborted => {
                    let filled = res.grid.iter().filter(|&&c| c != CellState::Unknown).count();
                    row![
                        bi(Bootstrap::XCircleFill).size(13).color(Color::from_rgb(0.75, 0.38, 0.0)),
                        text(format!("Aborted ({filled}/{})", puzzle.width * puzzle.height)).size(13),
                    ].spacing(4).align_y(Vertical::Center).into()
                }
                SolutionState::Partial => {
                    let filled = res.grid.iter().filter(|&&c| c != CellState::Unknown).count();
                    row![
                        bi(Bootstrap::DashLg).size(13).color(Color::from_rgb(0.65, 0.45, 0.0)),
                        text(format!("Partial ({filled}/{})", puzzle.width * puzzle.height)).size(13),
                    ].spacing(4).align_y(Vertical::Center).into()
                }
                SolutionState::Unsolvable => row![
                    bi(Bootstrap::XLg).size(13).color(Color::from_rgb(0.78, 0.08, 0.08)),
                    text("No solution").size(13),
                ].spacing(4).align_y(Vertical::Center).into(),
                SolutionState::Invalid(reason) => row![
                    bi(Bootstrap::ExclamationCircleFill).size(13)
                        .color(Color::from_rgb(0.75, 0.38, 0.0)),
                    text(format!("Invalid — {reason}")).size(13),
                ].spacing(4).align_y(Vertical::Center).into(),
            }
        } else {
            text("Not yet solved").size(13).color(Color::from_rgb(0.45, 0.45, 0.45)).into()
        };

        let copy_btn = button(
            row![bi(Bootstrap::Clipboard).size(12), text("Copy").size(12)]
                .spacing(4).align_y(Vertical::Center),
        )
        .on_press(Message::CopyPuzzleString(key))
        .padding([2, 8]);

        let header = container(
            row![
                back_btn,
                Space::with_width(Length::Fixed(12.0)),
                text(puzzle.name.as_str()).size(16),
                Space::with_width(Length::Fixed(8.0)),
                text(format!("{}x{}", puzzle.width, puzzle.height))
                    .size(13)
                    .color(Color::from_rgb(0.4, 0.4, 0.4)),
                copy_btn,
                Space::with_width(Length::Fill),
                status_el,
            ]
            .spacing(4)
            .align_y(Vertical::Center),
        )
        .padding([10, 16])
        .width(Length::Fill);

        let mut items: Vec<Element<Message>> = Vec::new();
        items.push(header.into());
        items.push(horizontal_rule(1).into());

        // ── Inline banner for solver-detected invalid state ────────────────
        if let Some(res) = result {
            if let SolutionState::Invalid(reason) = &res.state {
                let banner = container(
                    row![
                        bi(Bootstrap::ExclamationCircleFill).size(13)
                            .color(Color::from_rgb(0.75, 0.38, 0.0)),
                        text(format!("Invalid — {reason}")).size(13),
                    ]
                    .spacing(8)
                    .align_y(Vertical::Center),
                )
                .style(|_| container::Style {
                    background: Some(Color::from_rgba(0.75, 0.38, 0.0, 0.12).into()),
                    border: iced::Border {
                        radius: 4.0.into(),
                        color: Color::from_rgb(0.75, 0.38, 0.0),
                        width: 1.0,
                    },
                    ..Default::default()
                })
                .padding([8, 16])
                .width(Length::Fill);

                items.push(container(banner).padding([8, 16]).width(Length::Fill).into());
                items.push(horizontal_rule(1).into());
            }
        }

        // ── Solution navigation bar ───────────────────────────────────────
        if let Some(a) = all {
            let n = a.solutions.len();
            if n > 0 {
                let idx     = self.solution_index.min(n.saturating_sub(1));
                let can_prev = idx > 0;
                let can_next = idx + 1 < n;
                let sol_nav = row![
                    {
                        let b = button(bi(Bootstrap::ChevronLeft).size(13)).padding([2, 5]);
                        if can_prev { b.on_press(Message::SolutionPrev) } else { b }
                    },
                    text(format!("Solution {} / {n}", idx + 1)).size(12),
                    {
                        let b = button(bi(Bootstrap::ChevronRight).size(13)).padding([2, 5]);
                        if can_next { b.on_press(Message::SolutionNext) } else { b }
                    },
                    Space::with_width(Length::Fixed(12.0)),
                    text(format!("nodes expanded: {}", a.nodes_expanded))
                        .size(11)
                        .color(Color::from_rgb(0.45, 0.45, 0.45)),
                ]
                .spacing(4)
                .padding([5, 12])
                .align_y(Vertical::Center);
                items.push(sol_nav.into());
                items.push(horizontal_rule(1).into());
            }
        }

        // ── Step navigation bar ───────────────────────────────────────────
        if let Some(res) = active {
            if !res.steps.is_empty() {
                let total    = res.steps.len();
                let cursor   = self.step_cursor.min(total);
                let can_back = cursor > 0;
                let can_fwd  = cursor < total;

                let step_desc: String = if cursor > 0 {
                    res.steps[cursor - 1].description.clone()
                } else {
                    String::from("Initial state")
                };

                let play_icon = if self.replaying { Bootstrap::PauseFill } else { Bootstrap::PlayFill };

                let first_btn = {
                    let b = button(bi(Bootstrap::SkipStartFill).size(13)).padding([2, 5]);
                    if can_back { b.on_press(Message::StepFirst) } else { b }
                };
                let back_btn = {
                    let b = button(bi(Bootstrap::ChevronLeft).size(13)).padding([2, 5]);
                    if can_back { b.on_press(Message::StepBack) } else { b }
                };
                let fwd_btn = {
                    let b = button(bi(Bootstrap::ChevronRight).size(13)).padding([2, 5]);
                    if can_fwd { b.on_press(Message::StepForward) } else { b }
                };
                let last_btn = {
                    let b = button(bi(Bootstrap::SkipEndFill).size(13)).padding([2, 5]);
                    if can_fwd { b.on_press(Message::StepLast) } else { b }
                };
                let play_btn = {
                    let b = button(bi(play_icon).size(13)).padding([2, 5]);
                    if can_fwd || self.replaying { b.on_press(Message::ReplayToggle) } else { b }
                };

                let step_nav = row![
                    first_btn, back_btn,
                    text(format!("Step {cursor} / {total}")).size(12),
                    fwd_btn, last_btn,
                    Space::with_width(Length::Fixed(8.0)),
                    play_btn,
                    Space::with_width(Length::Fixed(12.0)),
                    text(step_desc).size(12).color(Color::from_rgb(0.35, 0.35, 0.35)),
                ]
                .spacing(4)
                .padding([6, 12])
                .align_y(Vertical::Center);

                items.push(step_nav.into());
                items.push(horizontal_rule(1).into());
            }
        }

        let trial_info: &[(Vec<CellState>, Option<(usize, usize)>)] =
            self.trial_stack.get(&key).map(|v| v.as_slice()).unwrap_or(&[]);

        items.push(
            container(view_grid(puzzle, display_grid, key, &self.cell_settings, trial_info))
                .padding(Padding { left: 16.0, ..Padding::ZERO })
                .width(Length::Fill)
                .height(Length::Fill)
                .into()
        );

        column(items)
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }

    fn view_results_table(&self) -> Element<'_, Message> {
        let mut rows: Vec<Element<Message>> = Vec::new();

        rows.push(
            container(
                row![
                    text("Puzzle").size(12).width(Length::Fixed(220.0)),
                    text("Size").size(12).width(Length::Fixed(70.0)),
                    text("Status").size(12).width(Length::Fill),
                ]
                .padding([6, 12]),
            )
            .style(style_header_row)
            .into(),
        );
        rows.push(horizontal_rule(1).into());

        for (fi, file) in self.files.iter().enumerate() {
            for (pi, entry) in file.puzzles.iter().enumerate() {
                let Some(puzzle) = entry.as_puzzle() else { continue };
                let key = (fi, pi);
                let Some(result) = self.results.get(&(key, self.solver)) else { continue };

                let (status_icon, status_color, status_text) = match &result.state {
                    SolutionState::Complete => (
                        Bootstrap::CheckLg,
                        Color::from_rgb(0.08, 0.55, 0.08),
                        String::from("Solved"),
                    ),
                    SolutionState::Aborted => {
                        let filled = result.grid.iter().filter(|&&c| c != CellState::Unknown).count();
                        (
                            Bootstrap::XCircleFill,
                            Color::from_rgb(0.75, 0.38, 0.0),
                            format!("Aborted ({}/{})", filled, puzzle.width * puzzle.height),
                        )
                    }
                    SolutionState::Partial => {
                        let filled = result.grid.iter().filter(|&&c| c != CellState::Unknown).count();
                        (
                            Bootstrap::DashLg,
                            Color::from_rgb(0.65, 0.45, 0.0),
                            format!("Partial ({}/{})", filled, puzzle.width * puzzle.height),
                        )
                    }
                    SolutionState::Unsolvable => (
                        Bootstrap::XLg,
                        Color::from_rgb(0.78, 0.08, 0.08),
                        String::from("No solution"),
                    ),
                    SolutionState::Invalid(reason) => (
                        Bootstrap::ExclamationCircleFill,
                        Color::from_rgb(0.75, 0.38, 0.0),
                        format!("Invalid — {reason}"),
                    ),
                };

                let status_col: Element<Message> = row![
                    bi(status_icon).size(13).color(status_color),
                    text(status_text).size(13),
                ]
                .spacing(4)
                .align_y(Vertical::Center)
                .width(Length::Fill)
                .into();

                let row_el = button(
                    row![
                        text(puzzle.name.as_str()).size(13).width(Length::Fixed(220.0)),
                        text(format!("{}x{}", puzzle.width, puzzle.height))
                            .size(13).width(Length::Fixed(70.0)),
                        status_col,
                    ]
                    .padding([4, 12]),
                )
                .on_press(Message::PuzzleFocused(key))
                .style(style_list_row_btn)
                .width(Length::Fill);

                rows.push(row_el.into());
                rows.push(horizontal_rule(1).into());
            }
        }

        container(scrollable(column(rows).spacing(0)).width(Length::Fill).height(Length::Fill))
            .style(style_panel)
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }

    // ── Status bar ───────────────────────────────────────────────────────

    fn view_status_bar(&self) -> Element<'_, Message> {
        let content: Element<Message> = if self.busy {
            row![
                bi(Bootstrap::HourglassSplit).size(12),
                text(self.status.as_str()).size(12),
            ]
            .spacing(6)
            .align_y(Vertical::Center)
            .into()
        } else {
            text(self.status.as_str()).size(12).into()
        };

        container(content).padding([4, 12]).into()
    }

    // ── Settings panel ───────────────────────────────────────────────────

    fn view_settings_panel(&self) -> Element<'_, Message> {
        let title_row = container(
            row![
                text("Settings").size(15),
                Space::with_width(Length::Fill),
                button(bi(Bootstrap::XLg).size(13))
                    .on_press(Message::SettingsClosed)
                    .padding([2, 6]),
            ]
            .align_y(Vertical::Center),
        )
        .style(style_header_row)
        .padding([8, 12])
        .width(Length::Fill);

        let states: &[(&str, u8, &CellVisual)] = &[
            ("Unknown", 0, &self.cell_settings.unknown),
            ("Filled",  1, &self.cell_settings.filled),
            ("Empty",   2, &self.cell_settings.empty),
        ];

        let mut sections: Vec<Element<Message>> = Vec::new();

        for &(label, idx, vis) in states {
            let color_preview = container(Space::new(Length::Fixed(32.0), Length::Fixed(32.0)))
                .style(move |_| container::Style {
                    background: Some(vis.color.into()),
                    border: iced::Border {
                        radius: 4.0.into(),
                        color: Color::from_rgb(0.4, 0.4, 0.4),
                        width: 1.0,
                    },
                    ..Default::default()
                });

            let r_slider = slider(0.0_f32..=1.0, vis.color.r, move |v| Message::SettingColor(idx, 0, v))
                .step(0.01_f32).width(Length::Fixed(140.0));
            let g_slider = slider(0.0_f32..=1.0, vis.color.g, move |v| Message::SettingColor(idx, 1, v))
                .step(0.01_f32).width(Length::Fixed(140.0));
            let b_slider = slider(0.0_f32..=1.0, vis.color.b, move |v| Message::SettingColor(idx, 2, v))
                .step(0.01_f32).width(Length::Fixed(140.0));

            let color_row = row![
                color_preview,
                Space::with_width(Length::Fixed(12.0)),
                column![
                    row![text("R").size(11).width(Length::Fixed(12.0)), r_slider,
                         text(format!("{:.2}", vis.color.r)).size(11)].spacing(4).align_y(Vertical::Center),
                    row![text("G").size(11).width(Length::Fixed(12.0)), g_slider,
                         text(format!("{:.2}", vis.color.g)).size(11)].spacing(4).align_y(Vertical::Center),
                    row![text("B").size(11).width(Length::Fixed(12.0)), b_slider,
                         text(format!("{:.2}", vis.color.b)).size(11)].spacing(4).align_y(Vertical::Center),
                ].spacing(4),
            ]
            .align_y(Vertical::Center)
            .spacing(0);

            // Icon picker — row of small buttons
            let current_icon = vis.icon;
            let icon_btns: Vec<Element<Message>> = ICON_OPTIONS.iter().map(|&opt| {
                let is_sel = icon_char(opt) == icon_char(current_icon);
                let btn_content: Element<Message> = match opt {
                    None => text("—").size(12).into(),
                    Some(ic) => bi(ic).size(13).into(),
                };
                button(btn_content)
                    .on_press(Message::SettingIcon(idx, opt))
                    .padding([3, 7])
                    .style(move |theme: &Theme, status| {
                        let p = theme.extended_palette();
                        button::Style {
                            background: if is_sel {
                                Some(p.primary.base.color.into())
                            } else {
                                match status {
                                    button::Status::Hovered => Some(p.primary.weak.color.into()),
                                    _ => None,
                                }
                            },
                            text_color: if is_sel { p.primary.base.text } else { p.background.base.text },
                            border: iced::Border {
                                radius: 3.0.into(),
                                color: p.primary.base.color,
                                width: if is_sel { 1.0 } else { 0.0 },
                            },
                            shadow: iced::Shadow::default(),
                        }
                    })
                    .into()
            }).collect();

            let section = container(
                column![
                    text(label).size(13),
                    color_row,
                    row![
                        text("Icon:").size(11),
                        row(icon_btns).spacing(3),
                    ].spacing(8).align_y(Vertical::Center),
                ]
                .spacing(8),
            )
            .style(style_panel)
            .padding(12)
            .width(Length::Fill);

            sections.push(section.into());
            sections.push(horizontal_rule(1).into());
        }

        // Clue background color section
        {
            let cb = self.cell_settings.clue_bg;
            let color_preview = container(Space::new(Length::Fixed(32.0), Length::Fixed(32.0)))
                .style(move |_| container::Style {
                    background: Some(cb.into()),
                    border: iced::Border {
                        radius: 4.0.into(),
                        color: Color::from_rgb(0.4, 0.4, 0.4),
                        width: 1.0,
                    },
                    ..Default::default()
                });
            let r_slider = slider(0.0_f32..=1.0, cb.r, move |v| Message::SettingClueBg(0, v))
                .step(0.01_f32).width(Length::Fixed(140.0));
            let g_slider = slider(0.0_f32..=1.0, cb.g, move |v| Message::SettingClueBg(1, v))
                .step(0.01_f32).width(Length::Fixed(140.0));
            let b_slider = slider(0.0_f32..=1.0, cb.b, move |v| Message::SettingClueBg(2, v))
                .step(0.01_f32).width(Length::Fixed(140.0));
            let color_row = row![
                color_preview,
                Space::with_width(Length::Fixed(12.0)),
                column![
                    row![text("R").size(11).width(Length::Fixed(12.0)), r_slider,
                         text(format!("{:.2}", cb.r)).size(11)].spacing(4).align_y(Vertical::Center),
                    row![text("G").size(11).width(Length::Fixed(12.0)), g_slider,
                         text(format!("{:.2}", cb.g)).size(11)].spacing(4).align_y(Vertical::Center),
                    row![text("B").size(11).width(Length::Fixed(12.0)), b_slider,
                         text(format!("{:.2}", cb.b)).size(11)].spacing(4).align_y(Vertical::Center),
                ].spacing(4),
            ]
            .align_y(Vertical::Center)
            .spacing(0);
            let section = container(
                column![text("Clue background").size(13), color_row].spacing(8),
            )
            .style(style_panel)
            .padding(12)
            .width(Length::Fill);
            sections.push(section.into());
            sections.push(horizontal_rule(1).into());
        }

        let content = column(sections).spacing(0);

        container(
            column![
                title_row,
                horizontal_rule(1),
                scrollable(content).height(Length::Fill),
            ]
        )
        .style(style_panel)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
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
    }
}

// ---------------------------------------------------------------------------
// Puzzle → canonical file-format string
// ---------------------------------------------------------------------------

fn puzzle_to_file_string(puzzle: &Puzzle) -> String {
    let encode = |clues: &[Vec<u32>]| -> String {
        clues.iter()
            .map(|g| if g.is_empty() { "0".into() } else { g.iter().map(|n| n.to_string()).collect::<Vec<_>>().join(" ") })
            .collect::<Vec<_>>()
            .join("|")
    };
    let mut s = format!("{};C:{}/R:{}", puzzle.name, encode(&puzzle.col_clues), encode(&puzzle.row_clues));
    match (&puzzle.answer, &puzzle.solution) {
        (Some(ans), Some(sol)) => {
            s.push(';'); s.push_str(ans);
            s.push(';');
            for st in sol { s.push(if *st == CellState::Filled { '1' } else { '0' }); }
        }
        (Some(ans), None) => { s.push(';'); s.push_str(ans); }
        (None, Some(sol))  => {
            s.push_str(";;");
            for st in sol { s.push(if *st == CellState::Filled { '1' } else { '0' }); }
        }
        (None, None) => {}
    }
    s
}

// ---------------------------------------------------------------------------
// Trial mode color palette
//
// tier 1–5 filled and empty colors; tiers above 5 cycle via modulo.
// ---------------------------------------------------------------------------

// Pixels of padding added on every side of the puzzle content so the user can
// pan the grid in any direction (initial scroll is set to this value so the
// puzzle appears flush with the viewport edge, with PAN_CENTER pixels of
// available movement in the inward directions).
const PAN_CENTER: f32 = 400.0;

const TRIAL_FILLED: &[(f32, f32, f32)] = &[
    (0.25, 0.32, 0.58), // tier 1: steel blue
    (0.45, 0.22, 0.55), // tier 2: purple
    (0.18, 0.48, 0.38), // tier 3: teal
    (0.55, 0.38, 0.18), // tier 4: amber
    (0.50, 0.20, 0.28), // tier 5: crimson
];

const TRIAL_EMPTY: &[(f32, f32, f32)] = &[
    (0.86, 0.91, 0.99), // tier 1: light blue
    (0.94, 0.88, 0.99), // tier 2: light lavender
    (0.88, 0.98, 0.93), // tier 3: light teal
    (0.99, 0.95, 0.84), // tier 4: light amber
    (0.99, 0.88, 0.91), // tier 5: light pink
];

fn trial_filled_color(tier: usize) -> Color {
    let (r, g, b) = TRIAL_FILLED[(tier - 1) % TRIAL_FILLED.len()];
    Color::from_rgb(r, g, b)
}

fn trial_empty_color(tier: usize) -> Color {
    let (r, g, b) = TRIAL_EMPTY[(tier - 1) % TRIAL_EMPTY.len()];
    Color::from_rgb(r, g, b)
}

// ---------------------------------------------------------------------------
// Puzzle grid renderer (interactive)
//
// Layout:
//   [corner]       [col clues…]  [corner]
//   [row clues…]   [cells…]      [row sum]
//   [bottom-border row if H%5==0]
//   [corner]       [col sums…]   [corner]
//
// Borders are drawn INTO cells (not as spacers between them):
//   - Each cell has a top and left inner border via container padding.
//   - Minor borders: 1 px.  Major borders: 2 px (at multiples of 5).
//   - Outer left (c=0) and top (r=0) borders are always major.
//   - Outer right / bottom borders added only when W%5==0 / H%5==0.
// ---------------------------------------------------------------------------

fn grid_scroll_id() -> widget::scrollable::Id {
    widget::scrollable::Id::new("puzzle-grid")
}

fn view_grid<'a>(
    puzzle: &'a Puzzle,
    grid: Option<Vec<CellState>>,
    key: Key,
    settings: &'a CellSettings,
    trial: &'a [(Vec<CellState>, Option<(usize, usize)>)],
) -> Element<'a, Message> {
    const C: f32 = 26.0;  // cell size px
    const N: f32 = 22.0;  // clue-number cell px

    let clue_bg    = settings.clue_bg;
    let border_min = Color::from_rgb(0.50, 0.53, 0.58);
    let border_maj = Color::from_rgb(0.28, 0.32, 0.44);

    let max_cd     = puzzle.col_clues.iter().map(|v| v.len()).max().unwrap_or(0).max(1);
    let max_rd     = puzzle.row_clues.iter().map(|v| v.len()).max().unwrap_or(0).max(1);
    let row_clue_w = max_rd as f32 * N;
    let col_clue_h = max_cd as f32 * N;

    let w = puzzle.width;
    let h = puzzle.height;

    let right_border = w % 5 == 0;
    let bot_border   = h % 5 == 0;

    // Grand totals — checked for mismatch (possible for Invalid puzzles).
    let total_row: u32 = puzzle.row_clues.iter().flat_map(|r| r.iter()).sum();
    let total_col: u32 = puzzle.col_clues.iter().flat_map(|c| c.iter()).sum();
    let total_mismatch = total_row != total_col;

    // Per-row infeasibility: minimum span (sum + gaps) exceeds grid width.
    let row_infeasible: Vec<bool> = puzzle.row_clues.iter().map(|clues| {
        let span: u32 = clues.iter().sum::<u32>() + clues.len().saturating_sub(1) as u32;
        span > puzzle.width as u32
    }).collect();
    // Per-col infeasibility: minimum span exceeds grid height.
    let col_infeasible: Vec<bool> = puzzle.col_clues.iter().map(|clues| {
        let span: u32 = clues.iter().sum::<u32>() + clues.len().saturating_sub(1) as u32;
        span > puzzle.height as u32
    }).collect();

    // Width of the sum column (far-right); expands for large numbers.
    let sum_w: f32 = match total_row.max(total_col) {
        0..=9   => N,
        10..=99  => N + 4.0,
        _ => N + 10.0,
    };

    // Orange-red style used on infeasible/mismatched sum cells.
    let warn_text = Color::from_rgb(0.75, 0.38, 0.0);

    // Solid-color box of fixed size (used for corners, outer borders).
    macro_rules! solid {
        ($w:expr, $h:expr, $c:expr) => {
            container(Space::new(0.0, 0.0))
                .width(Length::Fixed($w))
                .height(Length::Fixed($h))
                .style(move |_| container::Style { background: Some($c.into()), ..Default::default() })
                .into()
        };
    }

    // Centered text label on clue_bg — used for clue numbers and sums.
    macro_rules! clue_text {
        ($n:expr, $w:expr, $h:expr) => {{
            let bg = clue_bg;
            container(text($n.to_string()).size(13).color(Color::from_rgb(0.1, 0.1, 0.1)))
                .width(Length::Fixed($w))
                .height(Length::Fixed($h))
                .align_x(Horizontal::Center)
                .align_y(Vertical::Center)
                .style(move |_| container::Style { background: Some(bg.into()), ..Default::default() })
                .into()
        }};
    }

    // Wrap `content` with an independent left border (vc, left_pad) and top border (hc, top_pad),
    // both drawn INTO the cell so the outer size remains w_fixed × h_fixed.
    macro_rules! bordered {
        (
            content: $content:expr,
            w: $w:expr, h: $h:expr,
            top_pad: $tp:expr, top_color: $hc:expr,
            left_pad: $lp:expr, left_color: $vc:expr
        ) => {{
            let hc = $hc; let vc = $vc;
            let tp: f32 = $tp; let lp: f32 = $lp;
            let with_left = container($content)
                .width(Length::Fill)
                .height(Length::Fill)
                .padding(Padding { left: lp, ..Padding::ZERO })
                .style(move |_| container::Style { background: Some(vc.into()), ..Default::default() });
            container(with_left)
                .width(Length::Fixed($w))
                .height(Length::Fixed($h))
                .padding(Padding { top: tp, ..Padding::ZERO })
                .style(move |_| container::Style { background: Some(hc.into()), ..Default::default() })
        }};
    }

    let mut all_rows: Vec<Element<'a, Message>> = Vec::new();

    // ── Col clue header row ───────────────────────────────────────────────
    {
        let mut cells: Vec<Element<'a, Message>> = Vec::new();

        // Corner: minimap of current grid state
        {
            let cell_size = (row_clue_w / w as f32).min(col_clue_h / h as f32).max(1.0);
            let map_w = cell_size * w as f32;
            let map_h = cell_size * h as f32;
            let pad_left = ((row_clue_w - map_w) / 2.0).max(0.0);
            let pad_top  = ((col_clue_h - map_h) / 2.0).max(0.0);

            let mut mini_rows: Vec<Element<'a, Message>> = Vec::new();
            for mr in 0..h {
                let mut mini_cells: Vec<Element<'a, Message>> = Vec::new();
                for mc in 0..w {
                    let state = grid.as_ref().map(|g| g[mr * w + mc]).unwrap_or(CellState::Unknown);
                    let idx   = mr * w + mc;
                    let tier: usize = if !trial.is_empty() && state != CellState::Unknown {
                        trial.iter().enumerate().rev()
                            .find_map(|(i, (snap, _))| {
                                if snap.get(idx).copied().unwrap_or(CellState::Unknown) != state {
                                    Some(i + 1)
                                } else {
                                    None
                                }
                            })
                            .unwrap_or(0)
                    } else {
                        0
                    };
                    let cell_color = if tier > 0 {
                        match state {
                            CellState::Filled  => trial_filled_color(tier),
                            CellState::Empty   => trial_empty_color(tier),
                            CellState::Unknown => settings.visual_for(state).color,
                        }
                    } else {
                        settings.visual_for(state).color
                    };
                    mini_cells.push(
                        container(Space::new(0.0, 0.0))
                            .width(Length::Fixed(cell_size))
                            .height(Length::Fixed(cell_size))
                            .style(move |_| container::Style {
                                background: Some(cell_color.into()),
                                ..Default::default()
                            })
                            .into(),
                    );
                }
                mini_rows.push(row(mini_cells).into());
            }

            cells.push(
                container(
                    container(column(mini_rows))
                        .padding(Padding { top: pad_top, left: pad_left, ..Padding::ZERO }),
                )
                .width(Length::Fixed(row_clue_w))
                .height(Length::Fixed(col_clue_h))
                .style(move |_| container::Style { background: Some(clue_bg.into()), ..Default::default() })
                .into(),
            );
        }

        for c in 0..w {
            let lp = if c % 5 == 0 { 2.0_f32 } else { 1.0 };
            let vc = if c % 5 == 0 { border_maj } else { border_min };

            let clues = &puzzle.col_clues[c];
            let pad   = max_cd - clues.len();
            let mut nums: Vec<Element<'a, Message>> = (0..pad)
                .map(|_| solid!(C, N, clue_bg))
                .collect();
            for &n in clues {
                nums.push(clue_text!(n, C, N));
            }

            let col_content = container(column(nums))
                .width(Length::Fill)
                .height(Length::Fill)
                .style(move |_| container::Style { background: Some(clue_bg.into()), ..Default::default() });
            cells.push(
                container(col_content)
                    .width(Length::Fixed(C))
                    .height(Length::Fixed(col_clue_h))
                    .padding(Padding { left: lp, ..Padding::ZERO })
                    .style(move |_| container::Style { background: Some(vc.into()), ..Default::default() })
                    .into(),
            );
        }

        if right_border { cells.push(solid!(2.0, col_clue_h, border_maj)); }
        // Top-right corner: col grand total, bottom-aligned like col clues.
        {
            let txt_color = if total_mismatch { warn_text } else { Color::from_rgb(0.1, 0.1, 0.1) };
            let corner_inner = if total_mismatch {
                container(text(total_col.to_string()).size(13).color(txt_color))
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .align_x(Horizontal::Center)
                    .align_y(Vertical::Bottom)
                    .padding(Padding { bottom: 2.0, ..Padding::ZERO })
                    .style(|_| container::Style {
                        background: Some(Color::from_rgba(0.75, 0.38, 0.0, 0.15).into()),
                        border: iced::Border { radius: 3.0.into(), color: Color::from_rgb(0.75, 0.38, 0.0), width: 1.0 },
                        ..Default::default()
                    })
            } else {
                container(text(total_col.to_string()).size(13).color(txt_color))
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .align_x(Horizontal::Center)
                    .align_y(Vertical::Bottom)
                    .padding(Padding { bottom: 2.0, ..Padding::ZERO })
                    .style(move |_| container::Style { background: Some(clue_bg.into()), ..Default::default() })
            };
            cells.push(
                container(corner_inner)
                    .width(Length::Fixed(sum_w))
                    .height(Length::Fixed(col_clue_h))
                    .into(),
            );
        }

        all_rows.push(row(cells).into());
    }

    // ── Cell rows ─────────────────────────────────────────────────────────
    for r in 0..h {
        let tp = if r % 5 == 0 { 2.0_f32 } else { 1.0 };
        let hc = if r % 5 == 0 { border_maj } else { border_min };

        let mut cells: Vec<Element<'a, Message>> = Vec::new();

        // Row clue area — top border only
        {
            let clues = &puzzle.row_clues[r];
            let pad   = max_rd - clues.len();
            let mut rnums: Vec<Element<'a, Message>> = (0..pad)
                .map(|_| solid!(N, C, clue_bg))
                .collect();
            for &n in clues {
                rnums.push(clue_text!(n, N, C));
            }
            let rclue_inner = container(row(rnums))
                .width(Length::Fill)
                .height(Length::Fill)
                .style(move |_| container::Style { background: Some(clue_bg.into()), ..Default::default() });
            cells.push(
                container(rclue_inner)
                    .width(Length::Fixed(row_clue_w))
                    .height(Length::Fixed(C))
                    .padding(Padding { top: tp, ..Padding::ZERO })
                    .style(move |_| container::Style { background: Some(hc.into()), ..Default::default() })
                    .into(),
            );
        }

        // Grid cells — top + left inner borders, interactive
        for c in 0..w {
            let lp = if c % 5 == 0 { 2.0_f32 } else { 1.0 };
            let vc = if c % 5 == 0 { border_maj } else { border_min };

            let state = grid.as_ref().map(|g| g[r * w + c]).unwrap_or(CellState::Unknown);
            let idx   = r * w + c;

            // Deepest trial tier that changed this cell (0 = base).
            let tier: usize = if !trial.is_empty() && state != CellState::Unknown {
                trial.iter().enumerate().rev()
                    .find_map(|(i, (snap, _))| {
                        if snap.get(idx).copied().unwrap_or(CellState::Unknown) != state {
                            Some(i + 1)
                        } else {
                            None
                        }
                    })
                    .unwrap_or(0)
            } else {
                0
            };

            // Is this cell the first-changed origin of a trial tier?
            let origin_tier: Option<usize> = trial.iter().enumerate()
                .find_map(|(i, (_, orig))| if *orig == Some((r, c)) { Some(i + 1) } else { None });

            let bg = if tier > 0 {
                match state {
                    CellState::Filled  => trial_filled_color(tier),
                    CellState::Empty   => trial_empty_color(tier),
                    CellState::Unknown => settings.visual_for(state).color,
                }
            } else {
                settings.visual_for(state).color
            };

            let cell_face: Element<'a, Message> = if let Some(t) = origin_tier {
                match state {
                    CellState::Empty => {
                        // Small tier number in bottom-right corner
                        container(
                            container(text(t.to_string()).size(9).color(Color::from_rgb(0.3, 0.3, 0.45)))
                                .align_x(Horizontal::Right)
                                .align_y(Vertical::Bottom)
                                .width(Length::Fill)
                                .height(Length::Fill)
                                .padding(Padding { right: 2.0, bottom: 1.0, ..Padding::ZERO })
                        )
                        .width(Length::Fill)
                        .height(Length::Fill)
                        .style(move |_| container::Style { background: Some(bg.into()), ..Default::default() })
                        .into()
                    }
                    _ => {
                        // Centered white tier number
                        container(text(t.to_string()).size(13).color(Color::WHITE))
                            .width(Length::Fill)
                            .height(Length::Fill)
                            .align_x(Horizontal::Center)
                            .align_y(Vertical::Center)
                            .style(move |_| container::Style { background: Some(bg.into()), ..Default::default() })
                            .into()
                    }
                }
            } else if tier == 0 {
                let vis = settings.visual_for(state);
                if let Some(ic) = vis.icon {
                    let icon_col = vis.icon_color();
                    container(bi(ic).size(C * 0.55).color(icon_col))
                        .width(Length::Fill)
                        .height(Length::Fill)
                        .align_x(Horizontal::Center)
                        .align_y(Vertical::Center)
                        .style(move |_| container::Style { background: Some(bg.into()), ..Default::default() })
                        .into()
                } else {
                    container(Space::new(0.0, 0.0))
                        .width(Length::Fill)
                        .height(Length::Fill)
                        .style(move |_| container::Style { background: Some(bg.into()), ..Default::default() })
                        .into()
                }
            } else {
                // Trial cell (non-origin): colored background only
                container(Space::new(0.0, 0.0))
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .style(move |_| container::Style { background: Some(bg.into()), ..Default::default() })
                    .into()
            };

            let bordered_cell = bordered!(
                content: cell_face,
                w: C, h: C,
                top_pad: tp, top_color: hc,
                left_pad: lp, left_color: vc
            );

            cells.push(
                mouse_area(bordered_cell)
                    .on_press(Message::CellClicked { key, row: r, col: c, right: false })
                    .on_right_press(Message::CellClicked { key, row: r, col: c, right: true })
                    .on_enter(Message::CellEntered { key, row: r, col: c })
                    .into(),
            );
        }

        if right_border { cells.push(solid!(2.0, C, border_maj)); }

        // Row sum — top border only; orange-red if infeasible
        {
            let row_sum: u32 = puzzle.row_clues[r].iter().sum();
            let warn = row_infeasible[r];
            let txt_color = if warn { warn_text } else { Color::from_rgb(0.1, 0.1, 0.1) };
            let rsum_inner = if warn {
                container(text(row_sum.to_string()).size(13).color(txt_color))
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .align_x(Horizontal::Center)
                    .align_y(Vertical::Center)
                    .style(|_| container::Style {
                        background: Some(Color::from_rgba(0.75, 0.38, 0.0, 0.15).into()),
                        border: iced::Border {
                            radius: 3.0.into(),
                            color: Color::from_rgb(0.75, 0.38, 0.0),
                            width: 1.0,
                        },
                        ..Default::default()
                    })
            } else {
                container(text(row_sum.to_string()).size(13).color(txt_color))
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .align_x(Horizontal::Center)
                    .align_y(Vertical::Center)
                    .style(move |_| container::Style { background: Some(clue_bg.into()), ..Default::default() })
            };
            cells.push(
                container(rsum_inner)
                    .width(Length::Fixed(sum_w))
                    .height(Length::Fixed(C))
                    .padding(Padding { top: tp, ..Padding::ZERO })
                    .style(move |_| container::Style { background: Some(hc.into()), ..Default::default() })
                    .into(),
            );
        }

        all_rows.push(row(cells).into());
    }

    // Outer bottom border (only when H%5==0)
    if bot_border {
        all_rows.push(
            container(Space::new(0.0, 0.0))
                .width(Length::Fill)
                .height(Length::Fixed(2.0))
                .style(move |_| container::Style { background: Some(border_maj.into()), ..Default::default() })
                .into(),
        );
    }

    // ── Col sum row ───────────────────────────────────────────────────────
    {
        let mut cells: Vec<Element<'a, Message>> = Vec::new();

        // Bottom-left corner: row grand total, right-aligned like row clues.
        {
            let txt_color = if total_mismatch { warn_text } else { Color::from_rgb(0.1, 0.1, 0.1) };
            let corner_inner = if total_mismatch {
                container(text(total_row.to_string()).size(13).color(txt_color))
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .align_x(Horizontal::Right)
                    .align_y(Vertical::Center)
                    .padding(Padding { right: 4.0, ..Padding::ZERO })
                    .style(|_| container::Style {
                        background: Some(Color::from_rgba(0.75, 0.38, 0.0, 0.15).into()),
                        border: iced::Border { radius: 3.0.into(), color: Color::from_rgb(0.75, 0.38, 0.0), width: 1.0 },
                        ..Default::default()
                    })
            } else {
                container(text(total_row.to_string()).size(13).color(txt_color))
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .align_x(Horizontal::Right)
                    .align_y(Vertical::Center)
                    .padding(Padding { right: 4.0, ..Padding::ZERO })
                    .style(move |_| container::Style { background: Some(clue_bg.into()), ..Default::default() })
            };
            cells.push(
                container(corner_inner)
                    .width(Length::Fixed(row_clue_w))
                    .height(Length::Fixed(N))
                    .into(),
            );
        }

        for c in 0..w {
            let lp = if c % 5 == 0 { 2.0_f32 } else { 1.0 };
            let vc = if c % 5 == 0 { border_maj } else { border_min };
            let col_sum: u32 = puzzle.col_clues[c].iter().sum();
            let warn = col_infeasible[c];
            let txt_color = if warn { warn_text } else { Color::from_rgb(0.1, 0.1, 0.1) };
            let csum_inner = if warn {
                container(text(col_sum.to_string()).size(13).color(txt_color))
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .align_x(Horizontal::Center)
                    .align_y(Vertical::Center)
                    .style(|_| container::Style {
                        background: Some(Color::from_rgba(0.75, 0.38, 0.0, 0.15).into()),
                        border: iced::Border {
                            radius: 3.0.into(),
                            color: Color::from_rgb(0.75, 0.38, 0.0),
                            width: 1.0,
                        },
                        ..Default::default()
                    })
            } else {
                container(text(col_sum.to_string()).size(13).color(txt_color))
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .align_x(Horizontal::Center)
                    .align_y(Vertical::Center)
                    .style(move |_| container::Style { background: Some(clue_bg.into()), ..Default::default() })
            };
            cells.push(
                container(csum_inner)
                    .width(Length::Fixed(C))
                    .height(Length::Fixed(N))
                    .padding(Padding { left: lp, ..Padding::ZERO })
                    .style(move |_| container::Style { background: Some(vc.into()), ..Default::default() })
                    .into(),
            );
        }

        if right_border { cells.push(solid!(2.0, N, border_maj)); }

        cells.push(solid!(sum_w, N, clue_bg)); // bottom-right corner

        all_rows.push(row(cells).into());
    }

    // ── Trial controls, appended inside the scrollable immediately below grid
    {
        let trial_tier = trial.len();
        let enter_label = if trial_tier == 0 { "Trial" } else { "Deeper" };
        let enter_btn = button(text(enter_label).size(12))
            .on_press(Message::TrialEnter)
            .padding([2, 8]);
        let reject_btn = if trial_tier > 0 {
            button(text("Reject trial").size(12))
                .on_press(Message::TrialReject)
                .padding([2, 8])
        } else {
            button(text("Reject trial").size(12)).padding([2, 8])
        };
        let tier_label: Element<'a, Message> = if trial_tier > 0 {
            let (r, g, b) = TRIAL_FILLED[(trial_tier - 1) % TRIAL_FILLED.len()];
            text(format!("Tier {trial_tier}")).size(12)
                .color(Color::from_rgb(r, g, b))
                .into()
        } else {
            Space::with_width(Length::Shrink).into()
        };
        all_rows.push(horizontal_rule(1).into());
        all_rows.push(
            container(
                row![tier_label, Space::with_width(Length::Fill), enter_btn, reject_btn]
                    .spacing(6)
                    .padding([8, 0])
                    .align_y(Vertical::Center),
            )
            .width(Length::Fill)
            .into()
        );
    }

    scrollable(container(column(all_rows)).padding(Padding { top: PAN_CENTER + 16.0, right: PAN_CENTER + 16.0, bottom: PAN_CENTER + 16.0, left: PAN_CENTER }))
        .id(grid_scroll_id())
        .on_scroll(|vp| Message::GridScrolled(vp.absolute_offset()))
        .direction(widget::scrollable::Direction::Both {
            vertical:   widget::scrollable::Scrollbar::default(),
            horizontal: widget::scrollable::Scrollbar::default(),
        })
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}
