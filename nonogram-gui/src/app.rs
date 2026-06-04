use std::collections::{HashMap, HashSet};
use std::path::Path;

use iced::{
    alignment::{Horizontal, Vertical},
    keyboard,
    widget::{
        button, checkbox, column, container, horizontal_rule, pick_list,
        row, scrollable, text, vertical_rule, Space,
    },
    Color, Element, Event, Font, Length, Subscription, Task, Theme,
};
use iced_fonts::bootstrap::{self, Bootstrap};
use nonogram_core::{parse_file, CancelToken, CellState, Outcome, Puzzle, SolveContext, SolveResult};

use crate::convert::convert_letter_content;
use crate::solver::SolverKind;

// Bootstrap Icons font, named to match the loaded font bytes.
const BOOTSTRAP_FONT: Font = Font::with_name("bootstrap-icons");

// Create a text widget displaying a Bootstrap icon glyph.
fn bi(icon: Bootstrap) -> iced::widget::Text<'static> {
    text(bootstrap::icon_to_char(icon).to_string()).font(BOOTSTRAP_FONT)
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
// Reconstruct the grid state after applying the first `cursor` steps.
// cursor=0 → all Unknown; cursor=steps.len() → same as result.grid.
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
// Types
// ---------------------------------------------------------------------------

pub type Key = (usize, usize); // (file_idx, puzzle_idx)

pub struct LoadedFile {
    pub path: String,
    pub name: String,
    pub puzzles: Vec<Puzzle>,
    pub collapsed: bool,
}

// ---------------------------------------------------------------------------
// App
// ---------------------------------------------------------------------------

pub struct App {
    files: Vec<LoadedFile>,
    selected: HashSet<Key>,
    results: HashMap<(Key, SolverKind), SolveResult>,
    solver: SolverKind,
    focused: Option<Key>,
    status: String,
    busy: bool,
    // Step replay state
    step_cursor: usize, // steps applied: 0=initial, N=after N steps
    replaying: bool,
    // Cancellation
    cancel: CancelToken,
    // Keyboard modifier tracking (for shift/ctrl selection)
    modifiers: keyboard::Modifiers,
    last_anchor: Option<Key>,
    // Theme
    theme: Theme,
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
    FileLoaded(String, String, Vec<Puzzle>),
    ConvertLoaded(String, Vec<Puzzle>),
    DefaultsLoaded(Vec<(String, String, Vec<Puzzle>)>),

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

    // Theme
    ThemeToggled,

    // Keyboard modifiers
    ModifiersChanged(keyboard::Modifiers),

    // Abort
    AbortClicked,

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
            solver: SolverKind::Propagation,
            focused: None,
            status: String::from("Loading default puzzle files…"),
            busy: true,
            step_cursor: 0,
            replaying: false,
            cancel: CancelToken::default(),
            modifiers: keyboard::Modifiers::default(),
            last_anchor: None,
            theme: Theme::Light,
        };

        let task = Task::perform(
            async {
                let mut out = Vec::new();
                let dir = "puzzles";
                if let Ok(entries) = std::fs::read_dir(dir) {
                    let mut paths: Vec<_> = entries
                        .flatten()
                        .map(|e| e.path())
                        .filter(|p| p.extension().map(|e| e == "txt").unwrap_or(false))
                        .collect();
                    paths.sort();
                    for path in paths {
                        let path_str = path.to_string_lossy().into_owned();
                        if let Ok(puzzles) = parse_file(&path_str) {
                            let name = path
                                .file_name()
                                .map(|n| n.to_string_lossy().into_owned())
                                .unwrap_or_else(|| path_str.clone());
                            out.push((path_str, name, puzzles));
                        }
                    }
                }
                out
            },
            Message::DefaultsLoaded,
        );

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

        let modifiers = iced::event::listen_with(|event, _, _| {
            if let Event::Keyboard(keyboard::Event::ModifiersChanged(mods)) = event {
                Some(Message::ModifiersChanged(mods))
            } else {
                None
            }
        });

        Subscription::batch([timer, modifiers])
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
                        let puzzles = convert_letter_content(&content, &name);
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
                let n = puzzles.len();
                self.status = format!("Loaded {n} puzzle(s) from {name}");
                self.busy = false;
                if !self.files.iter().any(|f| f.path == path) {
                    self.files.push(LoadedFile { path, name, puzzles, collapsed: false });
                }
                Task::none()
            }

            Message::ConvertLoaded(name, puzzles) => {
                let n = puzzles.len();
                self.status = format!("Converted {n} puzzle(s) as \"{name}\"");
                self.busy = false;
                let display = format!("{name} (converted)");
                self.files.push(LoadedFile {
                    path: format!("__converted__{name}"),
                    name: display,
                    puzzles,
                    collapsed: false,
                });
                Task::none()
            }

            Message::DefaultsLoaded(list) => {
                let total: usize = list.iter().map(|(_, _, p)| p.len()).sum();
                self.status = if total > 0 {
                    format!("Loaded {total} puzzle(s) from puzzles/")
                } else {
                    "No default puzzles found in puzzles/".into()
                };
                self.busy = false;
                for (path, name, puzzles) in list {
                    if !self.files.iter().any(|f| f.path == path) {
                        self.files.push(LoadedFile { path, name, puzzles, collapsed: false });
                    }
                }
                Task::none()
            }

            // ── List toolbar ──────────────────────────────────────────────
            Message::SelectAll => {
                for fi in 0..self.files.len() {
                    for pi in 0..self.files[fi].puzzles.len() {
                        self.selected.insert((fi, pi));
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
                Task::none()
            }
            Message::CollapseAll => {
                for file in &mut self.files { file.collapsed = true; }
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

            // ── Collapse ──────────────────────────────────────────────────
            Message::FileCollapseToggled(fi) => {
                if let Some(file) = self.files.get_mut(fi) {
                    file.collapsed = !file.collapsed;
                }
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
                    let n = self.files[fi].puzzles.len();
                    for pi in 0..n {
                        if checked { self.selected.insert((fi, pi)); }
                        else { self.selected.remove(&(fi, pi)); }
                    }
                }
                Task::none()
            }

            Message::PuzzleFocused(key) => {
                let shift = self.modifiers.shift();
                let ctrl  = self.modifiers.control() || self.modifiers.logo();
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
                self.focused = Some(key);
                self.replaying = false;
                self.step_cursor = self.results.get(&(key, self.solver))
                    .map(|r| r.steps.len())
                    .unwrap_or(0);
                Task::none()
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
                self.step_cursor = self.focused
                    .and_then(|key| self.results.get(&(key, kind)))
                    .map(|r| r.steps.len())
                    .unwrap_or(0);
                Task::none()
            }

            Message::SolveSelected => {
                let to_solve: Vec<(Key, Puzzle)> = self.selected.iter()
                    .filter_map(|&(fi, pi)| {
                        self.files.get(fi)?.puzzles.get(pi).map(|p| ((fi, pi), p.clone()))
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
                        f.puzzles.iter().enumerate().map(move |(pi, p)| ((fi, pi), p.clone()))
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
                let solved = results.iter().filter(|(_, r)| r.outcome == Outcome::Solved).count();
                let aborted = results.iter().filter(|(_, r)| r.aborted).count();
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
                    let max = self.results.get(&(key, self.solver)).map(|r| r.steps.len()).unwrap_or(0);
                    self.step_cursor = (self.step_cursor + 1).min(max);
                }
                Task::none()
            }
            Message::StepLast => {
                if let Some(key) = self.focused {
                    let max = self.results.get(&(key, self.solver)).map(|r| r.steps.len()).unwrap_or(0);
                    self.step_cursor = max;
                }
                Task::none()
            }
            Message::ReplayToggle => {
                self.replaying = !self.replaying;
                if self.replaying {
                    if let Some(key) = self.focused {
                        let max = self.results.get(&(key, self.solver)).map(|r| r.steps.len()).unwrap_or(0);
                        if self.step_cursor >= max {
                            self.step_cursor = 0;
                        }
                    } else {
                        self.replaying = false;
                    }
                }
                Task::none()
            }
            Message::ReplayTick => {
                if let Some(key) = self.focused {
                    let max = self.results.get(&(key, self.solver)).map(|r| r.steps.len()).unwrap_or(0);
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
            let has = self.files.iter().any(|f| !f.puzzles.is_empty());
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

        row![
            button("Import File").on_press(Message::ImportClicked),
            button("Convert File").on_press(Message::ConvertClicked),
            Space::with_width(Length::Fill),
            button(bi(theme_icon).size(14))
                .on_press(Message::ThemeToggled)
                .padding([4, 8]),
            text("Solver:").size(14),
            pick_list(SolverKind::ALL, Some(self.solver), Message::SolverChanged),
            Space::with_width(Length::Fixed(12.0)),
            solve_sel,
            solve_all,
            abort,
        ]
        .spacing(8)
        .padding(8)
        .align_y(Vertical::Center)
        .into()
    }

    // ── Left panel (file/puzzle list) ────────────────────────────────────

    fn view_left_panel(&self) -> Element<'_, Message> {
        // ── List toolbar ──
        let list_toolbar = container(
            row![
                button(
                    row![bi(Bootstrap::CheckLg).size(11), text("All").size(12)]
                        .spacing(3).align_y(Vertical::Center)
                ).on_press(Message::SelectAll).padding([3, 6]),
                button(text("None").size(12)).on_press(Message::DeselectAll).padding([3, 6]),
                Space::with_width(Length::Fill),
                button(bi(Bootstrap::ChevronDown).size(11))
                    .on_press(Message::ExpandAll).padding([3, 6]),
                button(bi(Bootstrap::ChevronRight).size(11))
                    .on_press(Message::CollapseAll).padding([3, 6]),
            ]
            .spacing(4)
            .padding([4, 8])
            .align_y(Vertical::Center),
        )
        .style(style_header_row)
        .width(Length::Fill);

        // ── File/puzzle list ──
        let mut items: Vec<Element<Message>> = Vec::new();

        if self.files.is_empty() {
            items.push(
                container(text("No files loaded").size(13))
                    .padding([16, 12])
                    .into(),
            );
        }

        for (fi, file) in self.files.iter().enumerate() {
            let sel_count = (0..file.puzzles.len())
                .filter(|&pi| self.selected.contains(&(fi, pi)))
                .count();
            let all_sel = sel_count == file.puzzles.len() && !file.puzzles.is_empty();

            let chevron = if file.collapsed { Bootstrap::ChevronRight } else { Bootstrap::ChevronDown };
            let file_row = row![
                button(bi(chevron).size(11))
                    .on_press(Message::FileCollapseToggled(fi))
                    .padding([2, 4])
                    .style(style_chevron_btn),
                checkbox("", all_sel).on_toggle(move |c| Message::FileToggled(fi, c)),
                text(file.name.as_str()).size(13),
                Space::with_width(Length::Fill),
                text(format!("{sel_count}/{}", file.puzzles.len())).size(11),
            ]
            .spacing(4)
            .padding([4, 8])
            .align_y(Vertical::Center);

            items.push(container(file_row).style(style_header_row).into());

            if !file.collapsed {
                for (pi, puzzle) in file.puzzles.iter().enumerate() {
                    let key = (fi, pi);
                    let is_sel = self.selected.contains(&key);
                    let is_focused = self.focused == Some(key);

                    let badge_icon: Option<Bootstrap> = self.results.get(&(key, self.solver)).map(|r| match (r.outcome, r.aborted) {
                        (Outcome::Solved, _)     => Bootstrap::CheckLg,
                        (_, true)                => Bootstrap::XCircleFill,
                        (Outcome::Stuck, _)      => Bootstrap::DashLg,
                        (Outcome::NoSolution, _) => Bootstrap::XLg,
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
                                button::Style {
                                    background: Some(p.primary.strong.color.into()),
                                    text_color: p.primary.strong.text,
                                    border: iced::Border::default(),
                                    shadow: iced::Shadow::default(),
                                }
                            } else {
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
                        })
                        .padding([2, 6]);

                    let puzzle_row = row![
                        Space::with_width(Length::Fixed(20.0)),
                        checkbox("", is_sel).on_toggle(move |c| Message::PuzzleToggled(key, c)),
                        name_btn,
                        Space::with_width(Length::Fill),
                        text(format!("{}x{}", puzzle.width, puzzle.height)).size(10),
                    ]
                    .spacing(4)
                    .padding([2, 6])
                    .align_y(Vertical::Center);

                    items.push(container(puzzle_row).style(style_panel).into());
                }
            }

            items.push(horizontal_rule(1).into());
        }

        container(
            column![
                list_toolbar,
                horizontal_rule(1),
                scrollable(column(items).spacing(1)).height(Length::Fill),
            ]
        )
        .style(style_panel)
        .width(Length::Fixed(310.0))
        .height(Length::Fill)
        .into()
    }

    // ── Right panel ──────────────────────────────────────────────────────

    fn view_right_panel(&self) -> Element<'_, Message> {
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
        let Some(puzzle) = file.puzzles.get(pi) else {
            return text("(missing puzzle)").into();
        };

        let key = (fi, pi);
        let result = self.results.get(&(key, self.solver));

        // Determine the grid state to display (owned so it outlives the closures below)
        let grid_data: Option<Vec<CellState>> = {
            let cursor = self.step_cursor;
            result.map(|r| {
                if !r.steps.is_empty() && cursor < r.steps.len() {
                    grid_at_step(puzzle, r, cursor)
                } else {
                    r.grid.clone()
                }
            })
        };

        // Status indicator for the header
        let status_el: Element<Message> = if let Some(res) = result {
            match (res.outcome, res.aborted) {
                (Outcome::Solved, _) => row![
                    bi(Bootstrap::CheckLg).size(13).color(Color::from_rgb(0.08, 0.55, 0.08)),
                    text("Solved").size(13),
                ]
                .spacing(4)
                .align_y(Vertical::Center)
                .into(),
                (_, true) => {
                    let filled = res.grid.iter().filter(|&&c| c != CellState::Unknown).count();
                    let total = puzzle.width * puzzle.height;
                    row![
                        bi(Bootstrap::XCircleFill).size(13).color(Color::from_rgb(0.75, 0.38, 0.0)),
                        text(format!("Aborted ({filled}/{total})")).size(13),
                    ]
                    .spacing(4)
                    .align_y(Vertical::Center)
                    .into()
                }
                (Outcome::Stuck, _) => {
                    let filled = res.grid.iter().filter(|&&c| c != CellState::Unknown).count();
                    let total = puzzle.width * puzzle.height;
                    row![
                        bi(Bootstrap::DashLg).size(13).color(Color::from_rgb(0.65, 0.45, 0.0)),
                        text(format!("Partial ({filled}/{total})")).size(13),
                    ]
                    .spacing(4)
                    .align_y(Vertical::Center)
                    .into()
                }
                (Outcome::NoSolution, _) => row![
                    bi(Bootstrap::XLg).size(13).color(Color::from_rgb(0.78, 0.08, 0.08)),
                    text("No solution").size(13),
                ]
                .spacing(4)
                .align_y(Vertical::Center)
                .into(),
            }
        } else {
            text("Not yet solved")
                .size(13)
                .color(Color::from_rgb(0.45, 0.45, 0.45))
                .into()
        };

        let header = row![
            button(
                row![
                    bi(Bootstrap::ArrowLeft).size(13),
                    text("Back").size(13),
                ]
                .spacing(4)
                .align_y(Vertical::Center),
            )
            .on_press(Message::Unfocus),
            Space::with_width(Length::Fixed(12.0)),
            text(puzzle.name.as_str()).size(16),
            Space::with_width(Length::Fixed(8.0)),
            text(format!("{}x{}", puzzle.width, puzzle.height))
                .size(13)
                .color(Color::from_rgb(0.4, 0.4, 0.4)),
            Space::with_width(Length::Fill),
            status_el,
        ]
        .spacing(4)
        .padding(10)
        .align_y(Vertical::Center);

        let mut items: Vec<Element<Message>> = Vec::new();
        items.push(header.into());
        items.push(horizontal_rule(1).into());

        // Step navigation bar (only when a result with steps exists)
        if let Some(res) = result {
            if !res.steps.is_empty() {
                let total = res.steps.len();
                let cursor = self.step_cursor.min(total);
                let can_back = cursor > 0;
                let can_fwd = cursor < total;

                let step_desc: String = if cursor > 0 {
                    res.steps[cursor - 1].description.clone()
                } else {
                    String::from("Initial state")
                };

                let play_icon = if self.replaying {
                    Bootstrap::PauseFill
                } else {
                    Bootstrap::PlayFill
                };

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
                    if can_fwd || self.replaying {
                        b.on_press(Message::ReplayToggle)
                    } else {
                        b
                    }
                };

                let step_nav = row![
                    first_btn,
                    back_btn,
                    text(format!("Step {cursor} / {total}")).size(12),
                    fwd_btn,
                    last_btn,
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

        items.push(view_grid(puzzle, grid_data).into());

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
            for (pi, puzzle) in file.puzzles.iter().enumerate() {
                let key = (fi, pi);
                let Some(result) = self.results.get(&(key, self.solver)) else { continue };

                let (status_icon, status_color, status_text) = match (result.outcome, result.aborted) {
                    (Outcome::Solved, _) => (
                        Bootstrap::CheckLg,
                        Color::from_rgb(0.08, 0.55, 0.08),
                        String::from("Solved"),
                    ),
                    (_, true) => {
                        let filled = result.grid.iter()
                            .filter(|&&c| c != CellState::Unknown).count();
                        (
                            Bootstrap::XCircleFill,
                            Color::from_rgb(0.75, 0.38, 0.0),
                            format!("Aborted ({}/{})", filled, puzzle.width * puzzle.height),
                        )
                    }
                    (Outcome::Stuck, _) => {
                        let filled = result.grid.iter()
                            .filter(|&&c| c != CellState::Unknown).count();
                        (
                            Bootstrap::DashLg,
                            Color::from_rgb(0.65, 0.45, 0.0),
                            format!("Partial ({}/{})", filled, puzzle.width * puzzle.height),
                        )
                    }
                    (Outcome::NoSolution, _) => (
                        Bootstrap::XLg,
                        Color::from_rgb(0.78, 0.08, 0.08),
                        String::from("No solution"),
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
                        text(puzzle.name.as_str())
                            .size(13)
                            .width(Length::Fixed(220.0)),
                        text(format!("{}x{}", puzzle.width, puzzle.height))
                            .size(13)
                            .width(Length::Fixed(70.0)),
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

    // ── Helpers ──────────────────────────────────────────────────────────

    // Select all puzzles between `from` and `to` (inclusive) in document order.
    fn select_range(&mut self, from: Key, to: Key) {
        let all: Vec<Key> = self.files.iter().enumerate()
            .flat_map(|(fi, f)| f.puzzles.iter().enumerate().map(move |(pi, _)| (fi, pi)))
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
}

// ---------------------------------------------------------------------------
// Puzzle grid renderer
// ---------------------------------------------------------------------------

fn view_grid<'a>(puzzle: &'a Puzzle, grid: Option<Vec<CellState>>) -> Element<'a, Message> {
    const C: f32 = 26.0; // cell px
    const N: f32 = 22.0; // clue number cell px

    // Clue area gets the weak-background shade so it reads as distinct from cells.
    let clue_bg = Color::from_rgb(0.97, 0.97, 0.97); // kept light: clue numbers are semantic

    let max_cd = puzzle.col_clues.iter().map(|v| v.len()).max().unwrap_or(0).max(1);
    let max_rd = puzzle.row_clues.iter().map(|v| v.len()).max().unwrap_or(0).max(1);
    let row_clue_w = max_rd as f32 * N;

    // ── Column clue area ─────────────────────────────────────────────────
    let mut col_clue_cols: Vec<Element<Message>> = vec![
        // Corner spacer aligned with the row-clue column, on the clue background
        container(Space::new(Length::Fixed(row_clue_w), Length::Fixed(1.0)))
            .style(move |_| container::Style {
                background: Some(clue_bg.into()),
                ..Default::default()
            })
            .into(),
    ];
    for c in 0..puzzle.width {
        let clues = &puzzle.col_clues[c];
        let pad   = max_cd - clues.len();
        let mut nums: Vec<Element<Message>> = (0..pad)
            .map(|_| {
                container(Space::new(Length::Fixed(C), Length::Fixed(N)))
                    .style(move |_| container::Style {
                        background: Some(clue_bg.into()),
                        ..Default::default()
                    })
                    .into()
            })
            .collect();
        for &n in clues {
            nums.push(
                container(text(n.to_string()).size(11).color(Color::from_rgb(0.1, 0.1, 0.1)))
                    .width(Length::Fixed(C))
                    .height(Length::Fixed(N))
                    .align_x(Horizontal::Center)
                    .align_y(Vertical::Center)
                    .style(move |_| container::Style {
                        background: Some(clue_bg.into()),
                        ..Default::default()
                    })
                    .into(),
            );
        }
        col_clue_cols.push(column(nums).into());
    }

    // ── Grid rows ────────────────────────────────────────────────────────
    let mut grid_rows: Vec<Element<Message>> = vec![row(col_clue_cols).spacing(1).into()];

    for r in 0..puzzle.height {
        let mut cells: Vec<Element<Message>> = Vec::new();

        // Row clue cells
        let clues = &puzzle.row_clues[r];
        let pad   = max_rd - clues.len();
        let mut rnums: Vec<Element<Message>> = (0..pad)
            .map(|_| {
                container(Space::new(Length::Fixed(N), Length::Fixed(C)))
                    .style(move |_| container::Style {
                        background: Some(clue_bg.into()),
                        ..Default::default()
                    })
                    .into()
            })
            .collect();
        for &n in clues {
            rnums.push(
                container(text(n.to_string()).size(11).color(Color::from_rgb(0.1, 0.1, 0.1)))
                    .width(Length::Fixed(N))
                    .height(Length::Fixed(C))
                    .align_x(Horizontal::Center)
                    .align_y(Vertical::Center)
                    .style(move |_| container::Style {
                        background: Some(clue_bg.into()),
                        ..Default::default()
                    })
                    .into(),
            );
        }
        cells.push(row(rnums).into());

        // Grid cells
        for c in 0..puzzle.width {
            let state = grid
                .as_ref()
                .map(|g| g[r * puzzle.width + c])
                .unwrap_or(CellState::Unknown);
            let bg = match state {
                CellState::Filled  => Color::from_rgb(0.10, 0.10, 0.15),
                CellState::Empty   => Color::WHITE,
                CellState::Unknown => Color::from_rgb(0.72, 0.76, 0.82),
            };
            cells.push(
                container(Space::new(Length::Fill, Length::Fill))
                    .width(Length::Fixed(C))
                    .height(Length::Fixed(C))
                    .style(move |_| container::Style {
                        background: Some(bg.into()),
                        ..Default::default()
                    })
                    .into(),
            );
        }

        grid_rows.push(row(cells).spacing(1).into());
    }

    // Border container only wraps the grid rows (clue area has its own backgrounds)
    let grid_widget = container(column(grid_rows).spacing(2))
        .style(|_| container::Style {
            background: Some(Color::from_rgb(0.50, 0.53, 0.58).into()),
            ..Default::default()
        })
        .padding(1);

    scrollable(container(grid_widget).padding(16))
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}
