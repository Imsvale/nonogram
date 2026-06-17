use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use directories::ProjectDirs;
use serde::{Deserialize, Serialize};

use iced::{
    alignment::{Horizontal, Vertical},
    keyboard, mouse,
    widget::{
        button, checkbox, column, container, horizontal_rule, mouse_area,
        pick_list, row, scrollable, slider, stack, text, vertical_rule, Space,
    },
    Color, Element, Event, Font, Length, Padding, Subscription, Task, Theme, Vector,
};
use iced_fonts::bootstrap::{self, Bootstrap};

use crate::pan_viewport::PanViewport;
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
    pub sum_bg:  Color,
}

impl Default for CellSettings {
    fn default() -> Self {
        Self {
            unknown: CellVisual { color: Color::from_rgb(0.72, 0.76, 0.82), icon: None },
            filled:  CellVisual { color: Color::from_rgb(0.10, 0.10, 0.15), icon: None },
            empty:   CellVisual { color: Color::WHITE, icon: None },
            clue_bg: Color::from_rgb(0.97, 0.97, 0.97),
            sum_bg:  Color::from_rgb(0.82, 0.82, 0.82),
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

// Master icon list — indices are stored in SavedSettings for persistence.
const ICON_OPTIONS: &[Option<Bootstrap>] = &[
    None,
    Some(Bootstrap::CircleFill),
    Some(Bootstrap::SquareFill),
    Some(Bootstrap::DiamondFill),
    Some(Bootstrap::CheckLg),
    Some(Bootstrap::XLg),
    Some(Bootstrap::DashLg),
    Some(Bootstrap::Dot),
];

// Subset shown in the Filled cell icon picker.
const FILLED_ICON_OPTIONS: &[Option<Bootstrap>] = &[
    None,
    Some(Bootstrap::CircleFill),
    Some(Bootstrap::SquareFill),
    Some(Bootstrap::DiamondFill),
    Some(Bootstrap::CheckLg),
    Some(Bootstrap::XLg),
    Some(Bootstrap::DashLg),
];

// Subset shown in the Empty cell icon picker (no blank, no checkmark, adds dot).
const EMPTY_ICON_OPTIONS: &[Option<Bootstrap>] = &[
    Some(Bootstrap::CircleFill),
    Some(Bootstrap::SquareFill),
    Some(Bootstrap::DiamondFill),
    Some(Bootstrap::XLg),
    Some(Bootstrap::DashLg),
    Some(Bootstrap::Dot),
];

// ---------------------------------------------------------------------------
// Assistance settings
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Default)]
pub struct AssistanceSettings {
    pub auto_dim: bool,
    pub auto_fill_empty: bool,
    pub clue_sums_with_gaps: bool,
}

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
fn default_sum_bg_channel()  -> f32 { 0.82 }

#[derive(Serialize, Deserialize)]
struct SavedSettings {
    unknown: SavedCellVisual,
    filled:  SavedCellVisual,
    empty:   SavedCellVisual,
    #[serde(default = "default_clue_bg_channel")] clue_bg_r: f32,
    #[serde(default = "default_clue_bg_channel")] clue_bg_g: f32,
    #[serde(default = "default_clue_bg_channel")] clue_bg_b: f32,
    #[serde(default = "default_sum_bg_channel")]  sum_bg_r:  f32,
    #[serde(default = "default_sum_bg_channel")]  sum_bg_g:  f32,
    #[serde(default = "default_sum_bg_channel")]  sum_bg_b:  f32,
    #[serde(default)] auto_dim: bool,
    #[serde(default)] auto_fill_empty: bool,
    #[serde(default)] clue_sums_with_gaps: bool,
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

fn solved_path() -> Option<PathBuf> {
    ProjectDirs::from("", "nonogram", "nonogram-gui")
        .map(|pd| pd.config_dir().join("solved.json"))
}

/// Returns `full_path` relative to the current working directory, using forward
/// slashes. Falls back to the original string if it isn't under the cwd.
fn relative_path(full_path: &str) -> String {
    if let Ok(cwd) = std::env::current_dir() {
        if let Ok(rel) = Path::new(full_path).strip_prefix(&cwd) {
            return rel.to_string_lossy().replace('\\', "/");
        }
    }
    full_path.replace('\\', "/")
}

#[derive(Serialize, Deserialize)]
struct SolvedEntry { path: String, name: String }

fn load_solved() -> HashSet<(String, String)> {
    let path = match solved_path() { Some(p) => p, None => return HashSet::new() };
    let content = match std::fs::read_to_string(&path) { Ok(s) => s, Err(_) => return HashSet::new() };
    let entries: Vec<SolvedEntry> = match serde_json::from_str(&content) { Ok(e) => e, Err(_) => return HashSet::new() };
    entries.into_iter().map(|e| (e.path, e.name)).collect()
}

fn save_solved(solved: &HashSet<(String, String)>) {
    let Some(path) = solved_path() else { return };
    if let Some(parent) = path.parent() { let _ = std::fs::create_dir_all(parent); }
    let mut entries: Vec<SolvedEntry> = solved.iter()
        .map(|(p, n)| SolvedEntry { path: p.clone(), name: n.clone() })
        .collect();
    entries.sort_by(|a, b| a.path.cmp(&b.path).then(a.name.cmp(&b.name)));
    if let Ok(json) = serde_json::to_string_pretty(&entries) {
        let _ = std::fs::write(&path, json);
    }
}

fn load_settings() -> (CellSettings, AssistanceSettings) {
    let path = match config_path() {
        Some(p) => p,
        None    => return (CellSettings::default(), AssistanceSettings::default()),
    };
    let content = match std::fs::read_to_string(&path) {
        Ok(s)  => s,
        Err(_) => return (CellSettings::default(), AssistanceSettings::default()),
    };
    let saved: SavedSettings = match serde_json::from_str(&content) {
        Ok(s)  => s,
        Err(_) => return (CellSettings::default(), AssistanceSettings::default()),
    };
    let cell = CellSettings {
        unknown: saved_to_visual(saved.unknown),
        filled:  saved_to_visual(saved.filled),
        empty:   saved_to_visual(saved.empty),
        clue_bg: Color { r: saved.clue_bg_r, g: saved.clue_bg_g, b: saved.clue_bg_b, a: 1.0 },
        sum_bg:  Color { r: saved.sum_bg_r,  g: saved.sum_bg_g,  b: saved.sum_bg_b,  a: 1.0 },
    };
    let assist = AssistanceSettings {
        auto_dim:             saved.auto_dim,
        auto_fill_empty:      saved.auto_fill_empty,
        clue_sums_with_gaps:  saved.clue_sums_with_gaps,
    };
    (cell, assist)
}

fn save_settings(settings: &CellSettings, assist: &AssistanceSettings) {
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
        sum_bg_r:  settings.sum_bg.r,
        sum_bg_g:  settings.sum_bg.g,
        sum_bg_b:  settings.sum_bg.b,
        auto_dim:            assist.auto_dim,
        auto_fill_empty:     assist.auto_fill_empty,
        clue_sums_with_gaps: assist.clue_sums_with_gaps,
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
// Export formats
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportFormat {
    PuzPreV3,
}

impl ExportFormat {
    pub const ALL: &'static [Self] = &[Self::PuzPreV3];
    pub fn label(self) -> &'static str {
        match self { Self::PuzPreV3 => "Puz-Pre v3" }
    }
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
    pan_offset: HashMap<Key, Vector>,
    // Manual clue dimming
    manual_dim_rows: HashMap<Key, HashSet<usize>>,
    manual_dim_cols: HashMap<Key, HashSet<usize>>,
    // Settings
    cell_settings: CellSettings,
    assistance: AssistanceSettings,
    show_settings: bool,
    show_export_menu: bool,
    // Answer reveal spoiler state
    revealed_answers: HashSet<Key>,
    // Manually-solved puzzles: (relative_file_path, puzzle_name) — persisted across sessions
    solved_manually: HashSet<(String, String)>,
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
    SettingSumBg(u8, f32),
    AssistToggle(u8),  // 0=auto_dim, 1=auto_fill_empty, 2=clue_sums_with_gaps
    ClueDimToggle(Key, bool, usize),  // (puzzle key, is_col, row_or_col_idx)

    // Trial mode
    TrialEnter,
    TrialReject,

    // Clipboard / export
    CopyPuzzleString(Key),
    ExportMenuToggled,
    ExportFormatSelected(ExportFormat),
    ExportSaved(String, Option<String>, Option<String>), // (content, path, error)

    // Grid pan
    PanOffsetChanged(Key, Vector),

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
        let (cell_settings, assistance) = load_settings();
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
            pan_offset: HashMap::new(),
            manual_dim_rows: HashMap::new(),
            manual_dim_cols: HashMap::new(),
            cell_settings,
            assistance,
            show_settings: false,
            show_export_menu: false,
            revealed_answers: HashSet::new(),
            solved_manually: load_solved(),
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

        let kbd_and_mouse = iced::event::listen_with(|event, _, _| match event {
            Event::Keyboard(keyboard::Event::ModifiersChanged(mods)) => {
                Some(Message::ModifiersChanged(mods))
            }
            Event::Mouse(mouse::Event::ButtonReleased(_)) => Some(Message::DragEnded),
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
                self.show_export_menu = false;
                let all = self.all_solutions.get(&(key, self.solver));
                self.solution_index = all.map(|a| a.solutions.len().saturating_sub(1)).unwrap_or(0);
                self.step_cursor = all
                    .and_then(|a| a.solutions.get(self.solution_index))
                    .or_else(|| self.results.get(&(key, self.solver)))
                    .map(|r| r.steps.len())
                    .unwrap_or(0);
                Task::none()
            }

            Message::Unfocus => {
                self.focused = None;
                self.replaying = false;
                self.show_export_menu = false;
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
                // Auto-fill empties when a line's clues become fulfilled.
                if self.assistance.auto_fill_empty && target != CellState::Unknown {
                    let no_solver = self.results.get(&(key, self.solver)).is_none()
                        && self.all_solutions.get(&(key, self.solver)).is_none();
                    if no_solver {
                        let clue_data = self.files.get(fi)
                            .and_then(|f| f.puzzles.get(pi))
                            .and_then(|e| e.as_puzzle())
                            .map(|p| (p.row_clues.clone(), p.col_clues.clone(), p.width, p.height));
                        if let Some((row_clues, col_clues, w, h)) = clue_data {
                            let mg = self.manual_grids.get_mut(&key).unwrap();
                            let row_cells: Vec<CellState> = (0..w).map(|c| mg[row * w + c]).collect();
                            if check_line_fulfilled(&row_clues[row], &row_cells) {
                                for c in 0..w {
                                    if mg[row * w + c] == CellState::Unknown {
                                        mg[row * w + c] = CellState::Empty;
                                    }
                                }
                            }
                            let col_cells: Vec<CellState> = (0..h).map(|r| mg[r * w + col]).collect();
                            if check_line_fulfilled(&col_clues[col], &col_cells) {
                                for r in 0..h {
                                    if mg[r * w + col] == CellState::Unknown {
                                        mg[r * w + col] = CellState::Empty;
                                    }
                                }
                            }
                        }
                    }
                }
                // Detect full manual solve.
                {
                    let solved = self.files.get(fi)
                        .and_then(|f| f.puzzles.get(pi).and_then(|e| e.as_puzzle())
                            .zip(self.manual_grids.get(&key))
                            .map(|(puzzle, grid)| (relative_path(&f.path), puzzle.name.clone(), is_puzzle_fully_solved(puzzle, grid))));
                    if let Some((rel, name, true)) = solved {
                        if self.solved_manually.insert((rel, name)) { save_solved(&self.solved_manually); }
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
                    // Auto-fill empties when a line's clues become fulfilled.
                    if self.assistance.auto_fill_empty && target != CellState::Unknown {
                        let no_solver = self.results.get(&(key, self.solver)).is_none()
                            && self.all_solutions.get(&(key, self.solver)).is_none();
                        if no_solver {
                            let clue_data = self.files.get(fi)
                                .and_then(|f| f.puzzles.get(pi))
                                .and_then(|e| e.as_puzzle())
                                .map(|p| (p.row_clues.clone(), p.col_clues.clone(), p.width, p.height));
                            if let Some((row_clues, col_clues, w, h)) = clue_data {
                                let mg = self.manual_grids.get_mut(&key).unwrap();
                                let row_cells: Vec<CellState> = (0..w).map(|c| mg[row * w + c]).collect();
                                if check_line_fulfilled(&row_clues[row], &row_cells) {
                                    for c in 0..w {
                                        if mg[row * w + c] == CellState::Unknown {
                                            mg[row * w + c] = CellState::Empty;
                                        }
                                    }
                                }
                                let col_cells: Vec<CellState> = (0..h).map(|r| mg[r * w + col]).collect();
                                if check_line_fulfilled(&col_clues[col], &col_cells) {
                                    for r in 0..h {
                                        if mg[r * w + col] == CellState::Unknown {
                                            mg[r * w + col] = CellState::Empty;
                                        }
                                    }
                                }
                            }
                        }
                    }
                    // Detect full manual solve.
                    let solved = self.files.get(fi)
                        .and_then(|f| f.puzzles.get(pi).and_then(|e| e.as_puzzle())
                            .zip(self.manual_grids.get(&key))
                            .map(|(puzzle, grid)| (relative_path(&f.path), puzzle.name.clone(), is_puzzle_fully_solved(puzzle, grid))));
                    if let Some((rel, name, true)) = solved {
                        if self.solved_manually.insert((rel, name)) { save_solved(&self.solved_manually); }
                    }
                }
                Task::none()
            }

            Message::DragEnded => {
                self.drag_state = None;
                Task::none()
            }

            Message::PanOffsetChanged(key, offset) => {
                self.pan_offset.insert(key, offset);
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

            Message::AssistToggle(idx) => {
                match idx {
                    0 => self.assistance.auto_dim             = !self.assistance.auto_dim,
                    1 => self.assistance.auto_fill_empty      = !self.assistance.auto_fill_empty,
                    2 => self.assistance.clue_sums_with_gaps  = !self.assistance.clue_sums_with_gaps,
                    _ => {}
                }
                save_settings(&self.cell_settings, &self.assistance);
                Task::none()
            }

            Message::ClueDimToggle(key, is_col, idx) => {
                let map = if is_col { &mut self.manual_dim_cols } else { &mut self.manual_dim_rows };
                let set = map.entry(key).or_default();
                if !set.remove(&idx) { set.insert(idx); }
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
                        let manual_solved = self.solved_manually
                            .contains(&(relative_path(&file.path), puzzle.name.clone()));
                        let badge: Option<Element<Message>> = if manual_solved {
                            Some(bi(Bootstrap::CheckCircleFill).size(11)
                                .color(Color::from_rgb(0.0, 0.62, 0.24)).into())
                        } else {
                            self.results.get(&(key, self.solver)).map(|r| {
                                let icon = match &r.state {
                                    SolutionState::Complete   => Bootstrap::CheckLg,
                                    SolutionState::Aborted    => Bootstrap::XCircleFill,
                                    SolutionState::Partial    => Bootstrap::DashLg,
                                    SolutionState::Unsolvable => Bootstrap::XLg,
                                    SolutionState::Invalid(_) => Bootstrap::ExclamationCircleFill,
                                };
                                bi(icon).size(11).into()
                            })
                        };
                        let btn_content: Element<Message> = if let Some(b) = badge {
                            row![text(puzzle.name.as_str()).size(13), b]
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
                let pan_off = self.pan_offset.get(&key).copied().unwrap_or(Vector::ZERO);
                return column![
                    header,
                    horizontal_rule(1),
                    container(reason_banner).padding([8, 16]).width(Length::Fill),
                    horizontal_rule(1),
                    container(
                        PanViewport::new(key, view_grid(puzzle, display_grid, key, &self.cell_settings, trial_info, &self.assistance, self.manual_dim_rows.get(&key), self.manual_dim_cols.get(&key), self.solved_manually.contains(&(relative_path(&file.path), name.clone()))), pan_off)
                            .on_pan(move |v| Message::PanOffsetChanged(key, v)),
                    )
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

        let export_active = self.show_export_menu;
        let export_btn = button(
            row![bi(Bootstrap::Download).size(12), text("Export").size(12)]
                .spacing(4).align_y(Vertical::Center),
        )
        .on_press(Message::ExportMenuToggled)
        .padding([2, 8])
        .style(move |theme: &Theme, status| {
            let p = theme.extended_palette();
            button::Style {
                background: if export_active {
                    Some(p.primary.base.color.into())
                } else {
                    match status {
                        button::Status::Hovered => Some(p.primary.weak.color.into()),
                        _ => Some(p.background.base.color.into()),
                    }
                },
                text_color: if export_active { p.primary.base.text } else { p.background.base.text },
                border: iced::Border {
                    radius: 4.0.into(),
                    color: p.primary.base.color,
                    width: if export_active { 1.0 } else { 0.0 },
                },
                shadow: iced::Shadow::default(),
            }
        });

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
                export_btn,
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

        let pan_off = self.pan_offset.get(&key).copied().unwrap_or(Vector::ZERO);
        let manually_solved = self.solved_manually.contains(&(relative_path(&file.path), puzzle.name.clone()));
        items.push(
            container(
                PanViewport::new(key, view_grid(puzzle, display_grid, key, &self.cell_settings, trial_info, &self.assistance, self.manual_dim_rows.get(&key), self.manual_dim_cols.get(&key), manually_solved), pan_off)
                    .on_pan(move |v| Message::PanOffsetChanged(key, v)),
            )
            .padding(Padding { left: 16.0, ..Padding::ZERO })
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
        );

        let detail: Element<Message> = column(items)
            .width(Length::Fill)
            .height(Length::Fill)
            .into();

        if self.show_export_menu {
            let popup_items: Vec<Element<Message>> = ExportFormat::ALL.iter().map(|&fmt| {
                button(text(fmt.label()).size(13))
                    .on_press(Message::ExportFormatSelected(fmt))
                    .width(Length::Fill)
                    .padding([5, 10])
                    .into()
            }).collect();
            let popup = container(column(popup_items).spacing(2).padding([4, 4]))
                .width(160)
                .style(|theme: &Theme| {
                    let p = theme.extended_palette();
                    container::Style {
                        background: Some(p.background.base.color.into()),
                        border: iced::Border {
                            radius: 4.0.into(),
                            color: p.background.strong.color,
                            width: 1.0,
                        },
                        shadow: iced::Shadow {
                            color: Color::from_rgba(0.0, 0.0, 0.0, 0.25),
                            offset: iced::Vector::new(0.0, 2.0),
                            blur_radius: 6.0,
                        },
                        ..Default::default()
                    }
                });
            // Position popup just below the header row (~44 px) and offset
            // left to roughly align with the export button.
            let popup_layer: Element<Message> = column![
                Space::with_height(Length::Fixed(44.0)),
                container(popup).padding(Padding { left: 16.0, ..Padding::ZERO }),
            ]
            .width(Length::Fill)
            .height(Length::Fill)
            .into();
            stack([detail, popup_layer]).into()
        } else {
            detail
        }
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

            // Icon picker — per-state option set; Unknown has no icon.
            let icon_opts: Option<&[Option<Bootstrap>]> = match idx {
                1 => Some(FILLED_ICON_OPTIONS),
                2 => Some(EMPTY_ICON_OPTIONS),
                _ => None,
            };
            let current_icon = vis.icon;
            let make_icon_btn = |opt: Option<Bootstrap>| -> Element<Message> {
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
            };

            let section_col: Element<Message> = if let Some(opts) = icon_opts {
                let icon_btns: Vec<Element<Message>> = opts.iter().map(|&opt| make_icon_btn(opt)).collect();
                column![
                    text(label).size(13),
                    color_row,
                    row![
                        text("Icon:").size(11),
                        row(icon_btns).spacing(3),
                    ].spacing(8).align_y(Vertical::Center),
                ]
                .spacing(8)
                .into()
            } else {
                column![text(label).size(13), color_row].spacing(8).into()
            };

            let section = container(section_col)
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

        // Clue sums background color section
        {
            let cb = self.cell_settings.sum_bg;
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
            let r_slider = slider(0.0_f32..=1.0, cb.r, move |v| Message::SettingSumBg(0, v))
                .step(0.01_f32).width(Length::Fixed(140.0));
            let g_slider = slider(0.0_f32..=1.0, cb.g, move |v| Message::SettingSumBg(1, v))
                .step(0.01_f32).width(Length::Fixed(140.0));
            let b_slider = slider(0.0_f32..=1.0, cb.b, move |v| Message::SettingSumBg(2, v))
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
                column![text("Clue sums background").size(13), color_row].spacing(8),
            )
            .style(style_panel)
            .padding(12)
            .width(Length::Fill);
            sections.push(section.into());
            sections.push(horizontal_rule(1).into());
        }

        // Assistance section
        {
            let section = container(
                column![
                    text("Assistance").size(13),
                    checkbox(
                        "Dim fulfilled row/col clues",
                        self.assistance.auto_dim,
                    )
                    .on_toggle(|_| Message::AssistToggle(0))
                    .size(14),
                    checkbox(
                        "Auto-fill empties when clues fulfilled (manual only)",
                        self.assistance.auto_fill_empty,
                    )
                    .on_toggle(|_| Message::AssistToggle(1))
                    .size(14),
                    checkbox(
                        "Clue sums with gaps",
                        self.assistance.clue_sums_with_gaps,
                    )
                    .on_toggle(|_| Message::AssistToggle(2))
                    .size(14),
                ]
                .spacing(8),
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
// Export formatters
// ---------------------------------------------------------------------------

/// Puz-Pre v3 format.
///
/// Layout: `(max_col_depth + height)` rows × `(max_row_width + width)` cols,
/// space-separated. Top-left corner is all dots. Top section holds column
/// clues (bottom-aligned). Left section holds row clues (right-aligned).
/// Bottom-right section is the puzzle grid (`.` = empty/unknown, `#` = filled).
/// Clues for fulfilled lines are prefixed with `c`.
fn export_puzprv3(
    row_clues: &[Vec<u32>],
    col_clues: &[Vec<u32>],
    w: usize,
    h: usize,
    grid: Option<&[CellState]>,
    fulfilled_rows: &[bool],
    fulfilled_cols: &[bool],
) -> String {
    let max_cd = (h + 1) / 2;
    let max_rd = (w + 1) / 2;
    let total_rows = max_cd + h;
    let total_cols = max_rd + w;

    let mut out = String::new();
    out.push_str("pzprv3\nnonogram\n");
    out.push_str(&h.to_string()); out.push('\n');
    out.push_str(&w.to_string()); out.push('\n');

    for row_idx in 0..total_rows {
        let mut tokens: Vec<String> = Vec::with_capacity(total_cols);
        for col_idx in 0..total_cols {
            let token = if row_idx < max_cd {
                // Column clue area
                if col_idx < max_rd {
                    // Top-left corner
                    ".".into()
                } else {
                    let c = col_idx - max_rd;
                    let clues = &col_clues[c];
                    let pad = max_cd - clues.len();
                    if row_idx >= pad {
                        let n = clues[row_idx - pad];
                        if fulfilled_cols.get(c).copied().unwrap_or(false) {
                            format!("c{n}")
                        } else {
                            n.to_string()
                        }
                    } else {
                        ".".into()
                    }
                }
            } else {
                // Puzzle rows
                let r = row_idx - max_cd;
                if col_idx < max_rd {
                    // Row clue area
                    let clues = &row_clues[r];
                    let pad = max_rd - clues.len();
                    if col_idx >= pad {
                        let n = clues[col_idx - pad];
                        if fulfilled_rows.get(r).copied().unwrap_or(false) {
                            format!("c{n}")
                        } else {
                            n.to_string()
                        }
                    } else {
                        ".".into()
                    }
                } else {
                    // Puzzle cell
                    let c = col_idx - max_rd;
                    match grid.map(|g| g[r * w + c]).unwrap_or(CellState::Unknown) {
                        CellState::Filled => "#".into(),
                        _ => ".".into(),
                    }
                }
            };
            tokens.push(token);
        }
        out.push_str(&tokens.join(" "));
        out.push('\n');
    }
    out
}

// ---------------------------------------------------------------------------
// Assistance helpers
// ---------------------------------------------------------------------------

/// Returns true when the manual grid has all rows and columns fulfilled,
/// meaning the Filled-cell pattern satisfies every clue. Unknown cells between
/// filled runs are treated as separators (consistent with `check_line_fulfilled`).
fn is_puzzle_fully_solved(puzzle: &Puzzle, grid: &[CellState]) -> bool {
    let w = puzzle.width;
    let h = puzzle.height;
    for r in 0..h {
        let cells: Vec<CellState> = (0..w).map(|c| grid[r * w + c]).collect();
        if !check_line_fulfilled(&puzzle.row_clues[r], &cells) { return false; }
    }
    for c in 0..w {
        let cells: Vec<CellState> = (0..h).map(|r| grid[r * w + c]).collect();
        if !check_line_fulfilled(&puzzle.col_clues[c], &cells) { return false; }
    }
    true
}

/// Returns true if the runs of Filled cells (Unknown/Empty both act as gaps)
/// exactly match `clues`. Unknown gaps between filled runs are treated as
/// separators, so `[F U F]` with clue `[1,1]` matches.
fn check_line_fulfilled(clues: &[u32], cells: &[CellState]) -> bool {
    let mut runs: Vec<u32> = Vec::new();
    let mut run = 0u32;
    for &c in cells {
        if c == CellState::Filled {
            run += 1;
        } else if run > 0 {
            runs.push(run);
            run = 0;
        }
    }
    if run > 0 { runs.push(run); }
    runs.as_slice() == clues
}

/// For each clue in `clues`, returns whether it is definitively matched to a
/// specific sealed Filled run in `cells`. Scans greedily from the left (for
/// leading clues) and from the right (for trailing clues). A run is "sealed"
/// only when bounded on both sides by Empty or a line boundary — any adjacent
/// Unknown means the run might extend, so the scan stops immediately.
fn individually_fulfilled_clues(clues: &[u32], cells: &[CellState]) -> Vec<bool> {
    let n = clues.len();
    let w = cells.len();
    let mut dim = vec![false; n];
    if n == 0 || w == 0 { return dim; }

    // Left scan: match clues[0..] to sealed runs from the left.
    let mut ci = 0usize;
    let mut gi = 0usize;
    while ci < n {
        while gi < w && cells[gi] == CellState::Empty { gi += 1; }
        if gi >= w || cells[gi] == CellState::Unknown { break; }
        let run_start = gi;
        while gi < w && cells[gi] == CellState::Filled { gi += 1; }
        let run_len = gi - run_start;
        if gi < w && cells[gi] == CellState::Unknown { break; } // run may extend right
        if run_len == clues[ci] as usize { dim[ci] = true; ci += 1; } else { break; }
    }
    let left_matched = ci;

    // Right scan: match clues[left_matched..] from the right.
    let mut ci = n as isize - 1;
    let mut gi = w as isize - 1;
    while ci >= left_matched as isize {
        while gi >= 0 && cells[gi as usize] == CellState::Empty { gi -= 1; }
        if gi < 0 || cells[gi as usize] == CellState::Unknown { break; }
        let run_end = gi;
        while gi >= 0 && cells[gi as usize] == CellState::Filled { gi -= 1; }
        let run_len = (run_end - gi) as usize;
        if gi >= 0 && cells[gi as usize] == CellState::Unknown { break; } // run may extend left
        if run_len == clues[ci as usize] as usize { dim[ci as usize] = true; ci -= 1; } else { break; }
    }

    dim
}

// ---------------------------------------------------------------------------
// Trial mode color palette
//
// tier 1–5 filled and empty colors; tiers above 5 cycle via modulo.
// ---------------------------------------------------------------------------

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

fn view_grid<'a>(
    puzzle: &'a Puzzle,
    grid: Option<Vec<CellState>>,
    key: Key,
    settings: &'a CellSettings,
    trial: &'a [(Vec<CellState>, Option<(usize, usize)>)],
    assistance: &'a AssistanceSettings,
    manual_dim_rows: Option<&'a HashSet<usize>>,
    manual_dim_cols: Option<&'a HashSet<usize>>,
    manually_solved: bool,
) -> Element<'a, Message> {
    const C: f32 = 26.0;  // cell size px
    const N: f32 = 22.0;  // clue-number cell px

    let clue_bg    = settings.clue_bg;
    let sum_bg     = settings.sum_bg;
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

    let line_sum = |clues: &[u32]| -> u32 {
        let s: u32 = clues.iter().sum();
        if assistance.clue_sums_with_gaps { s + clues.len().saturating_sub(1) as u32 } else { s }
    };

    // Grand totals — checked for mismatch (possible for Invalid puzzles). Always gapless.
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

    // Total pixel width of one grid row (used to give fixed width to Fill elements).
    let grid_total_w: f32 = row_clue_w + C * w as f32
        + if right_border { 2.0 } else { 0.0 }
        + sum_w;

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

    // Per-row/col fulfilled flags: auto-dim OR manually dimmed.
    let fulfilled_rows: Vec<bool> = (0..h).map(|r| {
        let auto = assistance.auto_dim && grid.as_ref().map(|g| {
            let cells: Vec<CellState> = (0..w).map(|c| g[r * w + c]).collect();
            check_line_fulfilled(&puzzle.row_clues[r], &cells)
        }).unwrap_or(false);
        auto || manual_dim_rows.map(|s| s.contains(&r)).unwrap_or(false)
    }).collect();

    let fulfilled_cols: Vec<bool> = (0..w).map(|c| {
        let auto = assistance.auto_dim && grid.as_ref().map(|g| {
            let cells: Vec<CellState> = (0..h).map(|r| g[r * w + c]).collect();
            check_line_fulfilled(&puzzle.col_clues[c], &cells)
        }).unwrap_or(false);
        auto || manual_dim_cols.map(|s| s.contains(&c)).unwrap_or(false)
    }).collect();

    let clue_bg_hover = Color {
        r: (clue_bg.r - 0.07).max(0.0),
        g: (clue_bg.g - 0.07).max(0.0),
        b: (clue_bg.b - 0.07).max(0.0),
        a: 1.0,
    };

    // Centered text label on clue_bg — used for clue numbers and sums.
    // `$dim` makes the text light gray when true.
    macro_rules! clue_text {
        ($n:expr, $w:expr, $h:expr, $dim:expr) => {{
            let bg = clue_bg;
            let tc = if $dim {
                Color::from_rgb(0.70, 0.70, 0.70)
            } else {
                Color::from_rgb(0.1, 0.1, 0.1)
            };
            container(text($n.to_string()).size(13).color(tc))
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
                .style(|_| container::Style { background: Some(Color::WHITE.into()), ..Default::default() })
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
            let col_indiv_dim: Vec<bool> = if assistance.auto_dim {
                grid.as_ref().map(|g| {
                    let col_cells: Vec<CellState> = (0..h).map(|r| g[r * w + c]).collect();
                    individually_fulfilled_clues(clues, &col_cells)
                }).unwrap_or_else(|| vec![false; clues.len()])
            } else { vec![false; clues.len()] };
            for (i, &n) in clues.iter().enumerate() {
                nums.push(clue_text!(n, C, N, fulfilled_cols[c] || col_indiv_dim[i]));
            }

            let col_inner = container(column(nums))
                .width(Length::Fill)
                .height(Length::Fill);
            cells.push(
                container(
                    button(col_inner)
                        .on_press(Message::ClueDimToggle(key, true, c))
                        .padding(Padding::ZERO)
                        .style(move |_, status| button::Style {
                            background: Some(if matches!(status, button::Status::Hovered) {
                                clue_bg_hover.into()
                            } else {
                                clue_bg.into()
                            }),
                            border: Default::default(),
                            shadow: Default::default(),
                            text_color: Color::BLACK,
                        })
                )
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
                    .style(move |_| container::Style { background: Some(sum_bg.into()), ..Default::default() })
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

        // Row clue area — top border only; click to manually dim
        {
            let clues = &puzzle.row_clues[r];
            let pad   = max_rd - clues.len();
            let mut rnums: Vec<Element<'a, Message>> = (0..pad)
                .map(|_| solid!(N, C, clue_bg))
                .collect();
            let row_indiv_dim: Vec<bool> = if assistance.auto_dim {
                grid.as_ref().map(|g| {
                    let row_cells: Vec<CellState> = (0..w).map(|c| g[r * w + c]).collect();
                    individually_fulfilled_clues(clues, &row_cells)
                }).unwrap_or_else(|| vec![false; clues.len()])
            } else { vec![false; clues.len()] };
            for (i, &n) in clues.iter().enumerate() {
                rnums.push(clue_text!(n, N, C, fulfilled_rows[r] || row_indiv_dim[i]));
            }
            let rclue_inner = container(row(rnums))
                .width(Length::Fill)
                .height(Length::Fill);
            cells.push(
                container(
                    button(rclue_inner)
                        .on_press(Message::ClueDimToggle(key, false, r))
                        .padding(Padding::ZERO)
                        .style(move |_, status| button::Style {
                            background: Some(if matches!(status, button::Status::Hovered) {
                                clue_bg_hover.into()
                            } else {
                                clue_bg.into()
                            }),
                            border: Default::default(),
                            shadow: Default::default(),
                            text_color: Color::BLACK,
                        })
                )
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
                // Trial cell (non-origin): colored background; empty cells show their icon
                // in the dark version of the tier color so it reads over the light bg.
                let empty_icon = if state == CellState::Empty { settings.empty.icon } else { None };
                if let Some(ic) = empty_icon {
                    let icon_col = trial_filled_color(tier);
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
            let row_sum: u32 = line_sum(&puzzle.row_clues[r]);
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
                    .style(move |_| container::Style { background: Some(sum_bg.into()), ..Default::default() })
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
                .width(Length::Fixed(grid_total_w))
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
                    .style(move |_| container::Style { background: Some(sum_bg.into()), ..Default::default() })
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
            let col_sum: u32 = line_sum(&puzzle.col_clues[c]);
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
                    .style(move |_| container::Style { background: Some(sum_bg.into()), ..Default::default() })
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

        // Bottom-right corner: click to toggle "clue sums with gaps".
        cells.push(
            button(Space::new(0.0, 0.0))
                .on_press(Message::AssistToggle(2))
                .width(Length::Fixed(sum_w))
                .height(Length::Fixed(N))
                .padding(Padding::ZERO)
                .style(move |_, status| button::Style {
                    background: Some(if matches!(status, button::Status::Hovered) {
                        Color { r: sum_bg.r * 0.88, g: sum_bg.g * 0.88, b: sum_bg.b * 0.88, a: 1.0 }.into()
                    } else {
                        sum_bg.into()
                    }),
                    border: Default::default(),
                    shadow: Default::default(),
                    text_color: Color::BLACK,
                })
                .into()
        );

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
        all_rows.push(
            container(horizontal_rule(1))
                .width(Length::Fixed(grid_total_w))
                .into()
        );
        all_rows.push(
            container(
                row![tier_label, Space::with_width(Length::Fill), enter_btn, reject_btn]
                    .spacing(6)
                    .padding([8, 0])
                    .align_y(Vertical::Center),
            )
            .width(Length::Fixed(grid_total_w))
            .into()
        );
    }

    let grid_content = container(column(all_rows))
        .padding(Padding { top: 16.0, right: 16.0, bottom: 16.0, left: 0.0 });

    if manually_solved {
        let badge = container(
            bi(Bootstrap::CheckCircleFill).size(48).color(Color::from_rgb(0.0, 0.62, 0.24)),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(Horizontal::Right)
        .align_y(Vertical::Top)
        .padding(Padding { top: 20.0, right: 20.0, ..Padding::ZERO });
        stack([grid_content.into(), badge.into()]).into()
    } else {
        grid_content.into()
    }
}
