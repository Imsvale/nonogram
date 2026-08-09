use std::collections::HashSet;

use iced::{
    Border, Color, Element, Font, Length, Padding, alignment::{Horizontal, Vertical}, font::Weight, widget::{Space, button, column, container, mouse_area, row, text}
};
use iced_fonts::bootstrap::Bootstrap;
use nonogram_core::{CellState, Puzzle, SolveResult};

use super::{Key, Message};
use super::settings::{AssistFlag, AssistanceSettings, CellSettings};
use super::style::{bi, fwd, warn_inline_style, WARN_COLOR, COLOR_SUCCESS, COLOR_ERROR};

// ---------------------------------------------------------------------------
// Grid step helper
// ---------------------------------------------------------------------------

pub(crate) fn grid_at_step(puzzle: &Puzzle, result: &SolveResult, cursor: usize) -> Vec<CellState> {
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
// Assistance helpers
// ---------------------------------------------------------------------------

pub(crate) fn is_puzzle_fully_solved(puzzle: &Puzzle, grid: &[CellState]) -> bool {
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

pub(crate) fn check_line_fulfilled(clues: &[u32], cells: &[CellState]) -> bool {
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

fn individually_fulfilled_clues(clues: &[u32], cells: &[CellState]) -> Vec<bool> {
    let n = clues.len();
    let w = cells.len();
    let mut dim = vec![false; n];
    if n == 0 || w == 0 { return dim; }

    // Left-to-right: a run is dimmed if its left boundary is confirmed (edge or Empty).
    // An Unknown to the RIGHT of the run no longer blocks — the right boundary will be
    // checked by the next iteration's pre-scan (which stops at Unknown gaps).
    let mut ci = 0usize;
    let mut gi = 0usize;
    while ci < n {
        while gi < w && cells[gi] == CellState::Empty { gi += 1; }
        if gi >= w || cells[gi] == CellState::Unknown { break; }
        let run_start = gi;
        while gi < w && cells[gi] == CellState::Filled { gi += 1; }
        let run_len = gi - run_start;
        if run_len == clues[ci] as usize { dim[ci] = true; ci += 1; } else { break; }
    }
    let left_matched = ci;

    // Right-to-left: symmetric — confirmed right boundary is enough.
    let mut ci = n as isize - 1;
    let mut gi = w as isize - 1;
    while ci >= left_matched as isize {
        while gi >= 0 && cells[gi as usize] == CellState::Empty { gi -= 1; }
        if gi < 0 || cells[gi as usize] == CellState::Unknown { break; }
        let run_end = gi;
        while gi >= 0 && cells[gi as usize] == CellState::Filled { gi -= 1; }
        let run_len = (run_end - gi) as usize;
        if run_len == clues[ci as usize] as usize { dim[ci as usize] = true; ci -= 1; } else { break; }
    }

    dim
}

// ---------------------------------------------------------------------------
// Forced-empty helpers (used by auto-cross-edges assist)
// ---------------------------------------------------------------------------

// Strict left scan: returns (start, end_exclusive) for each confirmed run where
// BOTH boundaries are confirmed (Empty or edge). Runs abutting Unknown are not
// included, because their length might grow as the solver fills in cells.
fn scan_confirmed_left(clues: &[u32], cells: &[CellState]) -> Vec<(usize, usize)> {
    let n = clues.len();
    let w = cells.len();
    let mut runs = Vec::new();
    let (mut ci, mut gi) = (0usize, 0usize);
    while ci < n {
        while gi < w && cells[gi] == CellState::Empty { gi += 1; }
        if gi >= w || cells[gi] == CellState::Unknown { break; }
        let start = gi;
        while gi < w && cells[gi] == CellState::Filled { gi += 1; }
        let end_excl = gi;
        if gi < w && cells[gi] == CellState::Unknown { break; }  // right boundary uncertain
        if end_excl - start == clues[ci] as usize { runs.push((start, end_excl)); ci += 1; } else { break; }
    }
    runs
}

// Strict right scan: returns (start, end_exclusive) rightmost-first.
fn scan_confirmed_right(clues: &[u32], cells: &[CellState], skip_left: usize) -> Vec<(usize, usize)> {
    let n = clues.len();
    let w = cells.len();
    let mut runs = Vec::new();
    let (mut ci, mut gi) = (n as isize - 1, w as isize - 1);
    while ci >= skip_left as isize {
        while gi >= 0 && cells[gi as usize] == CellState::Empty { gi -= 1; }
        if gi < 0 || cells[gi as usize] == CellState::Unknown { break; }
        let end_incl = gi as usize;
        while gi >= 0 && cells[gi as usize] == CellState::Filled { gi -= 1; }
        let start = (gi + 1) as usize;
        if gi >= 0 && cells[gi as usize] == CellState::Unknown { break; }  // left boundary uncertain
        if end_incl - start + 1 == clues[ci as usize] as usize {
            runs.push((start, end_incl + 1));
            ci -= 1;
        } else { break; }
    }
    runs
}

fn mark_unknown_range(cells: &[CellState], a: usize, b: usize, out: &mut Vec<usize>) {
    for i in a..b {
        if cells[i] == CellState::Unknown { out.push(i); }
    }
}

/// Returns indices of Unknown cells that are definitively Empty, derived from
/// runs confirmed on both sides from the left and right edges.
///
/// Identifies forced empties in three situations:
///   • Cells to the left of the leftmost edge-confirmed run.
///   • Gaps between consecutive edge-confirmed runs (same direction).
///   • Cells to the right of the rightmost edge-confirmed run.
///   • The middle gap between left and right scans, when together they account
///     for every clue (so no unconfirmed clue can occupy the space between them).
pub(crate) fn forced_empty_from_edges(clues: &[u32], cells: &[CellState]) -> Vec<usize> {
    let n = clues.len();
    let len = cells.len();
    if len == 0 { return vec![]; }

    let left  = scan_confirmed_left(clues, cells);
    let right = scan_confirmed_right(clues, cells, left.len());
    let mut out: Vec<usize> = Vec::new();

    // Cells before the first left-confirmed run.
    if let Some(&(s, _)) = left.first() { mark_unknown_range(cells, 0, s, &mut out); }
    // Gaps between consecutive left-confirmed runs.
    for pair in left.windows(2) { mark_unknown_range(cells, pair[0].1, pair[1].0, &mut out); }
    // Middle gap — only when left + right together cover all clues.
    if left.len() + right.len() == n && !left.is_empty() && !right.is_empty() {
        // right is rightmost-first; right.last() is the leftmost right-confirmed run.
        mark_unknown_range(cells, left.last().unwrap().1, right.last().unwrap().0, &mut out);
    }
    // Gaps between consecutive right-confirmed runs (pair[0] is the more-rightward one).
    for pair in right.windows(2) { mark_unknown_range(cells, pair[1].1, pair[0].0, &mut out); }
    // Cells after the last right-confirmed run (right[0] is rightmost).
    if let Some(&(_, e)) = right.first() { mark_unknown_range(cells, e, len, &mut out); }

    out.sort_unstable();
    out.dedup();
    out
}

// ---------------------------------------------------------------------------
// Trial mode color palette
// ---------------------------------------------------------------------------

const TRIAL_FILLED: &[(f32, f32, f32)] = &[
    (0.25, 0.32, 0.58),
    (0.45, 0.22, 0.55),
    (0.18, 0.48, 0.38),
    (0.55, 0.38, 0.18),
    (0.50, 0.20, 0.28),
];

const TRIAL_EMPTY: &[(f32, f32, f32)] = &[
    (0.86, 0.91, 0.99),
    (0.94, 0.88, 0.99),
    (0.88, 0.98, 0.93),
    (0.99, 0.95, 0.84),
    (0.99, 0.88, 0.91),
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
// Hover run
// ---------------------------------------------------------------------------

// horizontal: fixed=row, start/end=col range
// vertical:   fixed=col, start/end=row range
pub(crate) struct HoverRun {
    pub fixed: usize,
    pub start: usize,
    pub end: usize,
}

pub(crate) struct HoverRuns {
    pub h: Option<HoverRun>,
    pub v: Option<HoverRun>,
}

impl HoverRuns {
    pub fn none() -> Self { Self { h: None, v: None } }
}

pub(crate) fn compute_hover_runs(
    hover_cell: Option<(Key, usize, usize)>,
    for_key: Key,
    puzzle: &Puzzle,
    grid: &[CellState],
) -> HoverRuns {
    let (hkey, hr, hc) = match hover_cell {
        Some(v) => v,
        None => return HoverRuns::none(),
    };
    if hkey != for_key { return HoverRuns::none(); }
    let w = puzzle.width;
    let h = puzzle.height;
    if hr >= h || hc >= w || grid[hr * w + hc] != CellState::Filled {
        return HoverRuns::none();
    }

    // Horizontal run
    let mut hs = hc;
    while hs > 0 && grid[hr * w + hs - 1] == CellState::Filled { hs -= 1; }
    let mut he = hc;
    while he + 1 < w && grid[hr * w + he + 1] == CellState::Filled { he += 1; }

    // Vertical run
    let mut vs = hr;
    while vs > 0 && grid[(vs - 1) * w + hc] == CellState::Filled { vs -= 1; }
    let mut ve = hr;
    while ve + 1 < h && grid[(ve + 1) * w + hc] == CellState::Filled { ve += 1; }

    HoverRuns {
        h: if he > hs { Some(HoverRun { fixed: hr, start: hs, end: he }) } else { None },
        v: if ve > vs { Some(HoverRun { fixed: hc, start: vs, end: ve }) } else { None },
    }
}

// ---------------------------------------------------------------------------
// Color blend utility
// ---------------------------------------------------------------------------

fn blend_color(base: Color, overlay: Color) -> Color {
    let a = overlay.a;
    Color {
        r: base.r * (1.0 - a) + overlay.r * a,
        g: base.g * (1.0 - a) + overlay.g * a,
        b: base.b * (1.0 - a) + overlay.b * a,
        a: 1.0,
    }
}

// ---------------------------------------------------------------------------
// Grid regions
// ---------------------------------------------------------------------------

pub(crate) struct GridRegions<'a> {
    pub corner:       Element<'a, Message>,
    pub col_clues:    Element<'a, Message>,
    pub top_right:    Element<'a, Message>,
    pub row_clues:    Element<'a, Message>,
    pub cells:        Element<'a, Message>,
    pub row_sums:     Element<'a, Message>,
    pub col_sums:     Element<'a, Message>,
    pub bottom_left:  Element<'a, Message>,
    pub bottom_right: Element<'a, Message>,
    pub controls:     Element<'a, Message>,
    pub nav:          Element<'a, Message>,
    pub corner_w:  f32,
    pub corner_h:  f32,
    pub sum_w:     f32,
    pub sum_h:     f32,
    pub puzzle_w:  usize,
    pub puzzle_h:  usize,
}

// ---------------------------------------------------------------------------
// Puzzle grid renderer (interactive)
// ---------------------------------------------------------------------------

pub(crate) fn build_grid_regions<'a>(
    puzzle: &'a Puzzle,
    grid: Option<Vec<CellState>>,
    key: Key,
    settings: &'a CellSettings,
    trial: &'a [(Vec<CellState>, Option<(usize, usize)>)],
    assistance: &'a AssistanceSettings,
    manual_dim_rows: Option<&'a HashSet<(usize, usize)>>,
    manual_dim_cols: Option<&'a HashSet<(usize, usize)>>,
    has_undo: bool,
    has_redo: bool,
    prev_key: Option<Key>,
    next_key: Option<Key>,
    hover_runs: HoverRuns,
    crosshair: Option<(Option<usize>, Option<usize>, Color)>,
    machine_nav: Option<Element<'a, Message>>,
) -> GridRegions<'a> {
    const C: f32 = 26.0;
    const N: f32 = 22.0;

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

    let line_sum = |clues: &[u32]| -> u32 {
        let s: u32 = clues.iter().sum();
        if assistance.clue_sums_with_gaps { s + clues.len().saturating_sub(1) as u32 } else { s }
    };

    let total_row: u32 = puzzle.row_clues.iter().flat_map(|r| r.iter()).sum();
    let total_col: u32 = puzzle.col_clues.iter().flat_map(|c| c.iter()).sum();
    let total_mismatch = total_row != total_col;

    let row_infeasible: Vec<bool> = puzzle.row_clues.iter().map(|clues| {
        let span: u32 = clues.iter().sum::<u32>() + clues.len().saturating_sub(1) as u32;
        span > puzzle.width as u32
    }).collect();
    let col_infeasible: Vec<bool> = puzzle.col_clues.iter().map(|clues| {
        let span: u32 = clues.iter().sum::<u32>() + clues.len().saturating_sub(1) as u32;
        span > puzzle.height as u32
    }).collect();

    let sum_w: f32 = match total_row.max(total_col) {
        0..=9   => N,
        10..=99 => N + 4.0,
        _       => N + 10.0,
    };

    let warn_text = WARN_COLOR;

    macro_rules! solid {
        ($w:expr, $h:expr, $c:expr) => {
            container(Space::new(0.0, 0.0))
                .width(Length::Fixed($w))
                .height(Length::Fixed($h))
                .style(move |_| container::Style { background: Some($c.into()), ..Default::default() })
                .into()
        };
    }

    let fulfilled_rows: Vec<bool> = (0..h).map(|r| {
        assistance.auto_dim && grid.as_ref().map(|g| {
            let cells: Vec<CellState> = (0..w).map(|c| g[r * w + c]).collect();
            check_line_fulfilled(&puzzle.row_clues[r], &cells)
        }).unwrap_or(false)
    }).collect();

    let fulfilled_cols: Vec<bool> = (0..w).map(|c| {
        assistance.auto_dim && grid.as_ref().map(|g| {
            let cells: Vec<CellState> = (0..h).map(|r| g[r * w + c]).collect();
            check_line_fulfilled(&puzzle.col_clues[c], &cells)
        }).unwrap_or(false)
    }).collect();

    let clue_bg_hover = Color {
        r: (clue_bg.r - 0.07).max(0.0),
        g: (clue_bg.g - 0.07).max(0.0),
        b: (clue_bg.b - 0.07).max(0.0),
        a: 1.0,
    };

    let (xhair_row, xhair_col, xhair_color): (Option<usize>, Option<usize>, Option<Color>) =
        match crosshair {
            Some((xr, xc, xc_col)) => (xr, xc, Some(xc_col)),
            None => (None, None, None),
        };

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

    // ── Region accumulators ───────────────────────────────────────────────────
    let mut col_clue_cells: Vec<Element<'a, Message>> = Vec::new();
    let mut row_clue_vec:   Vec<Element<'a, Message>> = Vec::new();
    let mut cells_rows:     Vec<Element<'a, Message>> = Vec::new();
    let mut row_sum_vec:    Vec<Element<'a, Message>> = Vec::new();
    let mut col_sum_cells:  Vec<Element<'a, Message>> = Vec::new();

    // ── Corner: minimap ───────────────────────────────────────────────────────
    let corner_el: Element<'a, Message> = {
        let cell_size = (row_clue_w / (w + 2) as f32).min(col_clue_h / (h + 2) as f32).max(1.0);
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
        column(vec![
            row(vec![
                container(
                    container(column(mini_rows))
                        .padding(Padding { top: pad_top, left: pad_left, ..Padding::ZERO }),
                )
                .width(Length::Fixed(row_clue_w))
                .height(Length::Fixed(col_clue_h))
                .style(|_| container::Style { background: Some(Color::WHITE.into()), ..Default::default() })
                .into(),
                solid!(2.0, col_clue_h, border_maj),
            ]).into(),
            solid!(row_clue_w + 2.0, 2.0, border_maj),
        ])
        .into()
    };

    // ── Col clue columns ──────────────────────────────────────────────────────
    for c in 0..w {
        let lp = if c == 0 { 0.0_f32 } else if c % 5 == 0 { 2.0 } else { 1.0 };
        let vc = if c == 0 { border_min } else if c % 5 == 0 { border_maj } else { border_min };

        let col_bg = match (xhair_col == Some(c), xhair_color) {
            (true, Some(xc)) => blend_color(clue_bg, xc),
            _ => clue_bg,
        };
        let col_bg_hover = match (xhair_col == Some(c), xhair_color) {
            (true, Some(xc)) => blend_color(clue_bg_hover, xc),
            _ => clue_bg_hover,
        };

        let clues = &puzzle.col_clues[c];
        let pad   = max_cd - clues.len();
        let mut nums: Vec<Element<'a, Message>> = (0..pad)
            .map(|_| solid!(C, N, col_bg))
            .collect();
        let col_indiv_dim: Vec<bool> = if assistance.auto_dim {
            grid.as_ref().map(|g| {
                let col_cells: Vec<CellState> = (0..h).map(|r| g[r * w + c]).collect();
                individually_fulfilled_clues(clues, &col_cells)
            }).unwrap_or_else(|| vec![false; clues.len()])
        } else { vec![false; clues.len()] };
        for (i, &n) in clues.iter().enumerate() {
            let is_dim = fulfilled_cols[c]
                || col_indiv_dim[i]
                || manual_dim_cols.map(|s| s.contains(&(c, i))).unwrap_or(false);
            let tc = if is_dim { Color::from_rgb(0.70, 0.70, 0.70) } else { Color::from_rgb(0.1, 0.1, 0.1) };
            let bg = col_bg;
            let bg_h = col_bg_hover;
            nums.push(
                button(
                    container(text(n.to_string()).size(13).color(tc)
                        .font(Font { weight: Weight::Bold, ..Font::DEFAULT }))
                        .width(Length::Fixed(C))
                        .height(Length::Fixed(N))
                        .align_x(Horizontal::Center)
                        .align_y(Vertical::Center)
                )
                .on_press(Message::ClueDimToggle(key, true, c, i))
                .padding(Padding::ZERO)
                .style(move |_, status| button::Style {
                    background: Some(if matches!(status, button::Status::Hovered) {
                        bg_h.into()
                    } else {
                        bg.into()
                    }),
                    border: Default::default(),
                    shadow: Default::default(),
                    text_color: Color::BLACK,
                })
                .into()
            );
        }
        col_clue_cells.push(
            container(column(nums))
                .width(Length::Fixed(C))
                .height(Length::Fixed(col_clue_h))
                .padding(Padding { left: lp, ..Padding::ZERO })
                .style(move |_| container::Style { background: Some(vc.into()), ..Default::default() })
                .into(),
        );
    }
    col_clue_cells.push(solid!(2.0, col_clue_h, border_maj));

    // ── Top-right corner: col grand total ─────────────────────────────────────
    let top_right_el: Element<'a, Message> = {
        let txt_color = if total_mismatch { warn_text } else { Color::from_rgb(0.1, 0.1, 0.1) };
        let corner_inner = if total_mismatch {
            container(
                container(text(total_col.to_string()).size(13).color(txt_color))
                    .padding([2, 5])
                    .style(|_| warn_inline_style())
            )
            .width(Length::Fill).height(Length::Fill)
            .align_x(Horizontal::Center).align_y(Vertical::Bottom)
            .padding(Padding { bottom: 2.0, ..Padding::ZERO })
            .style(move |_| container::Style { background: Some(sum_bg.into()), ..Default::default() })
        } else {
            container(text(total_col.to_string()).size(13).color(txt_color))
                .width(Length::Fill).height(Length::Fill)
                .align_x(Horizontal::Center).align_y(Vertical::Bottom)
                .padding(Padding { bottom: 2.0, ..Padding::ZERO })
                .style(move |_| container::Style { background: Some(sum_bg.into()), ..Default::default() })
        };
        column(vec![
            container(corner_inner)
                .width(Length::Fixed(sum_w))
                .height(Length::Fixed(col_clue_h))
                .into(),
            solid!(sum_w, 2.0, border_maj),
        ])
        .into()
    };

    // ── Cell rows ─────────────────────────────────────────────────────────────
    for r in 0..h {
        let tp = if r == 0 { 0.0_f32 } else if r % 5 == 0 { 2.0 } else { 1.0 };
        let hc = if r == 0 { border_min } else if r % 5 == 0 { border_maj } else { border_min };

        // Row clue
        {
            let row_bg = match (xhair_row == Some(r), xhair_color) {
                (true, Some(xc)) => blend_color(clue_bg, xc),
                _ => clue_bg,
            };
            let row_bg_hover = match (xhair_row == Some(r), xhair_color) {
                (true, Some(xc)) => blend_color(clue_bg_hover, xc),
                _ => clue_bg_hover,
            };

            let clues = &puzzle.row_clues[r];
            let pad   = max_rd - clues.len();
            let mut rnums: Vec<Element<'a, Message>> = (0..pad)
                .map(|_| solid!(N, C, row_bg))
                .collect();
            let row_indiv_dim: Vec<bool> = if assistance.auto_dim {
                grid.as_ref().map(|g| {
                    let row_cells: Vec<CellState> = (0..w).map(|c| g[r * w + c]).collect();
                    individually_fulfilled_clues(clues, &row_cells)
                }).unwrap_or_else(|| vec![false; clues.len()])
            } else { vec![false; clues.len()] };
            for (i, &n) in clues.iter().enumerate() {
                let is_dim = fulfilled_rows[r]
                    || row_indiv_dim[i]
                    || manual_dim_rows.map(|s| s.contains(&(r, i))).unwrap_or(false);
                let tc = if is_dim { Color::from_rgb(0.70, 0.70, 0.70) } else { Color::from_rgb(0.1, 0.1, 0.1) };
                let bg = row_bg;
                let bg_h = row_bg_hover;
                rnums.push(
                    button(
                        container(text(n.to_string()).size(13).color(tc)
                            .font(Font { weight: Weight::Bold, ..Font::DEFAULT }))
                            .width(Length::Fixed(N))
                            .height(Length::Fixed(C))
                            .align_x(Horizontal::Center)
                            .align_y(Vertical::Center)
                    )
                    .on_press(Message::ClueDimToggle(key, false, r, i))
                    .padding(Padding::ZERO)
                    .style(move |_, status| button::Style {
                        background: Some(if matches!(status, button::Status::Hovered) {
                            bg_h.into()
                        } else {
                            bg.into()
                        }),
                        border: Default::default(),
                        shadow: Default::default(),
                        text_color: Color::BLACK,
                    })
                    .into()
                );
            }
            row_clue_vec.push(
                container(row(rnums))
                    .width(Length::Fixed(row_clue_w))
                    .height(Length::Fixed(C))
                    .padding(Padding { top: tp, ..Padding::ZERO })
                    .style(move |_| container::Style { background: Some(hc.into()), ..Default::default() })
                    .into(),
            );
        }

        // Grid cells for this row
        let mut row_cells: Vec<Element<'a, Message>> = Vec::new();
        for c in 0..w {
            let lp = if c == 0 { 0.0_f32 } else if c % 5 == 0 { 2.0 } else { 1.0 };
            let vc = if c == 0 { border_min } else if c % 5 == 0 { border_maj } else { border_min };

            let state = grid.as_ref().map(|g| g[r * w + c]).unwrap_or(CellState::Unknown);
            let idx   = r * w + c;

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

            let bg = match (xhair_row == Some(r) || xhair_col == Some(c), xhair_color) {
                (true, Some(xc)) => blend_color(bg, xc),
                _ => bg,
            };

            let in_h_run = hover_runs.h.as_ref().map_or(false, |hr| r == hr.fixed && c >= hr.start && c <= hr.end);
            let in_v_run = hover_runs.v.as_ref().map_or(false, |hr| c == hr.fixed && r >= hr.start && r <= hr.end);
            let in_hover_run = in_h_run || in_v_run;
            let is_h_head = in_h_run && hover_runs.h.as_ref().map_or(false, |hr| c == hr.start);
            let is_v_head = in_v_run && hover_runs.v.as_ref().map_or(false, |hr| r == hr.start);
            let h_run_len = hover_runs.h.as_ref().map_or(0, |hr| hr.end - hr.start + 1);
            let v_run_len = hover_runs.v.as_ref().map_or(0, |hr| hr.end - hr.start + 1);

            let cell_face: Element<'a, Message> = if let Some(t) = origin_tier {
                match state {
                    CellState::Empty => {
                        let marker = container(
                            text(t.to_string()).size(9).color(Color::from_rgb(0.3, 0.3, 0.45))
                        )
                        .align_x(Horizontal::Right)
                        .align_y(Vertical::Bottom)
                        .width(Length::Fill)
                        .height(Length::Fill)
                        .padding(Padding { right: 2.0, bottom: 1.0, ..Padding::ZERO });

                        if let Some(ic) = settings.empty.icon {
                            let icon_col = trial_filled_color(tier);
                            let icon_layer = container(bi(ic).size(C * 0.55).color(icon_col))
                                .width(Length::Fill)
                                .height(Length::Fill)
                                .align_x(Horizontal::Center)
                                .align_y(Vertical::Center)
                                .style(move |_| container::Style { background: Some(bg.into()), ..Default::default() });
                            iced::widget::stack![icon_layer, marker]
                                .width(Length::Fill)
                                .height(Length::Fill)
                                .into()
                        } else {
                            container(marker)
                                .width(Length::Fill)
                                .height(Length::Fill)
                                .style(move |_| container::Style { background: Some(bg.into()), ..Default::default() })
                                .into()
                        }
                    }
                    _ => {
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

            let cell_face: Element<'a, Message> = if in_hover_run && state == CellState::Filled {
                let highlight = container(Space::new(0.0, 0.0))
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .style(|_| container::Style {
                        background: Some(Color::from_rgba(1.0, 1.0, 1.0, 0.10).into()),
                        ..Default::default()
                    });
                let mut layers: Vec<Element<'a, Message>> = vec![cell_face, highlight.into()];
                if is_h_head {
                    layers.push(
                        container(text(h_run_len.to_string()).size(9).color(Color::WHITE))
                            .align_x(Horizontal::Left)
                            .align_y(Vertical::Bottom)
                            .width(Length::Fill)
                            .height(Length::Fill)
                            .padding(Padding { bottom: 2.0, left: 2.0, ..Padding::ZERO })
                            .into(),
                    );
                }
                if is_v_head {
                    layers.push(
                        container(text(v_run_len.to_string()).size(9).color(Color::WHITE))
                            .align_x(Horizontal::Right)
                            .align_y(Vertical::Top)
                            .width(Length::Fill)
                            .height(Length::Fill)
                            .padding(Padding { top: 1.0, right: 2.0, ..Padding::ZERO })
                            .into(),
                    );
                }
                iced::widget::stack(layers)
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .into()
            } else {
                cell_face
            };

            let bordered_cell = bordered!(
                content: cell_face,
                w: C, h: C,
                top_pad: tp, top_color: hc,
                left_pad: lp, left_color: vc
            );

            row_cells.push(
                mouse_area(bordered_cell)
                    .on_press(Message::CellClicked { key, row: r, col: c, right: false })
                    .on_right_press(Message::CellClicked { key, row: r, col: c, right: true })
                    .on_enter(Message::CellEntered { key, row: r, col: c })
                    .into(),
            );
        }
        row_cells.push(solid!(2.0, C, border_maj));
        cells_rows.push(row(row_cells).into());

        // Row sum
        {
            let row_sum: u32 = line_sum(&puzzle.row_clues[r]);
            let warn = row_infeasible[r];
            let txt_color = if warn { warn_text } else { Color::from_rgb(0.1, 0.1, 0.1) };
            let rsum_inner = if warn {
                container(
                    container(text(row_sum.to_string()).size(13).color(txt_color))
                        .padding([2, 4])
                        .style(|_| warn_inline_style())
                )
                .width(Length::Fill).height(Length::Fill)
                .align_x(Horizontal::Center).align_y(Vertical::Center)
                .style(move |_| container::Style { background: Some(sum_bg.into()), ..Default::default() })
            } else {
                container(text(row_sum.to_string()).size(13).color(txt_color))
                    .width(Length::Fill).height(Length::Fill)
                    .align_x(Horizontal::Center).align_y(Vertical::Center)
                    .style(move |_| container::Style { background: Some(sum_bg.into()), ..Default::default() })
            };
            row_sum_vec.push(
                container(rsum_inner)
                    .width(Length::Fixed(sum_w))
                    .height(Length::Fixed(C))
                    .padding(Padding { top: tp, ..Padding::ZERO })
                    .style(move |_| container::Style { background: Some(hc.into()), ..Default::default() })
                    .into(),
            );
        }
    }

    let cells_row_w = C * w as f32 + 2.0;
    // No bottom border on the scrollable regions — the separator belongs to the
    // frozen footer (col_sums / bottom_left / bottom_right) so it stays visible
    // while panning and when grid height is not a multiple of 5.

    // ── Col sum row ───────────────────────────────────────────────────────────
    let bottom_left_el: Element<'a, Message> = {
        let txt_color = if total_mismatch { warn_text } else { Color::from_rgb(0.1, 0.1, 0.1) };
        let corner_inner = if total_mismatch {
            container(
                container(text(total_row.to_string()).size(13).color(txt_color))
                    .padding([2, 4])
                    .style(|_| warn_inline_style())
            )
            .width(Length::Fill).height(Length::Fill)
            .align_x(Horizontal::Right).align_y(Vertical::Center)
            .padding(Padding { right: 4.0, ..Padding::ZERO })
            .style(move |_| container::Style { background: Some(sum_bg.into()), ..Default::default() })
        } else {
            container(text(total_row.to_string()).size(13).color(txt_color))
                .width(Length::Fill).height(Length::Fill)
                .align_x(Horizontal::Right).align_y(Vertical::Center)
                .padding(Padding { right: 4.0, ..Padding::ZERO })
                .style(move |_| container::Style { background: Some(sum_bg.into()), ..Default::default() })
        };
        column(vec![
            solid!(row_clue_w + 2.0, 2.0, border_maj),
            container(
                container(corner_inner)
                    .width(Length::Fixed(row_clue_w))
                    .height(Length::Fixed(N)),
            )
            .width(Length::Fixed(row_clue_w + 2.0))
            .height(Length::Fixed(N))
            .style(move |_| container::Style { background: Some(border_maj.into()), ..Default::default() })
            .into(),
        ])
        .into()
    };

    for c in 0..w {
        let lp = if c == 0 { 0.0_f32 } else if c % 5 == 0 { 2.0 } else { 1.0 };
        let vc = if c == 0 { border_min } else if c % 5 == 0 { border_maj } else { border_min };
        let col_sum: u32 = line_sum(&puzzle.col_clues[c]);
        let warn = col_infeasible[c];
        let txt_color = if warn { warn_text } else { Color::from_rgb(0.1, 0.1, 0.1) };
        let csum_inner = if warn {
            container(
                container(text(col_sum.to_string()).size(13).color(txt_color))
                    .padding([2, 4])
                    .style(|_| warn_inline_style())
            )
            .width(Length::Fill).height(Length::Fill)
            .align_x(Horizontal::Center).align_y(Vertical::Center)
            .style(move |_| container::Style { background: Some(sum_bg.into()), ..Default::default() })
        } else {
            container(text(col_sum.to_string()).size(13).color(txt_color))
                .width(Length::Fill).height(Length::Fill)
                .align_x(Horizontal::Center).align_y(Vertical::Center)
                .style(move |_| container::Style { background: Some(sum_bg.into()), ..Default::default() })
        };
        col_sum_cells.push(
            container(csum_inner)
                .width(Length::Fixed(C))
                .height(Length::Fixed(N))
                .padding(Padding { left: lp, ..Padding::ZERO })
                .style(move |_| container::Style { background: Some(vc.into()), ..Default::default() })
                .into(),
        );
    }
    col_sum_cells.push(solid!(2.0, N, border_maj));
    // Top separator belongs to the footer so it stays frozen while the grid scrolls.
    let col_sums_el: Element<'a, Message> = column(vec![
        solid!(cells_row_w, 2.0, border_maj),
        row(col_sum_cells).into(),
    ])
    .into();

    let bottom_right_el: Element<'a, Message> = column(vec![
        solid!(sum_w, 2.0, border_maj),
        button(Space::new(0.0, 0.0))
            .on_press(Message::AssistToggle(AssistFlag::ClueSumsWithGaps))
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
            .into(),
    ])
    .into();

    // ── Controls ──────────────────────────────────────────────────────────────
    let controls_el: Element<'a, Message> = if let Some(el) = machine_nav { el } else {
        let trial_tier = trial.len();

        let clear_btn = button(text("Clear answer").size(12))
            .on_press(Message::ClearGrid(key))
            .padding([2, 8]);
        let undo_btn_base = button(bi(Bootstrap::ReplyFill).size(15)).padding([2, 8]);
        let undo_btn: Element<Message> = if has_undo {
            undo_btn_base.on_press(Message::UndoGrid(key)).into()
        } else { undo_btn_base.into() };
        let redo_btn_base = button(fwd('\u{E003}').size(15)).padding([2, 8]);
        let redo_btn: Element<Message> = if has_redo {
            redo_btn_base.on_press(Message::RedoGrid(key)).into()
        } else { redo_btn_base.into() };

        let enter_label = if trial_tier == 0 { "Trial" } else { "+1" };
        let enter_btn: Element<Message> = button(
            text(enter_label)
                .size(12)
                .align_x(Horizontal::Center)
            )
            .on_press(Message::TrialEnter)
            .width(Length::Fixed(40.0))
            .padding([2, 4])
            .into();

        let active = trial_tier > 0;
        let icon_radius = Border { radius: 3.0.into(), ..Default::default() };

        let reject_btn_base = button(
            bi(Bootstrap::XLg)
                .size(14)
                .color(Color { r: 0.3, g: 0.1, b: 0.1, a: 1.0 })
                .align_y(Vertical::Center)
                .align_x(Horizontal::Center)
            )
            .padding([1, 3])
            .style(move |_, status: button::Status| {
                let (bg, border_col) = if !active {
                    (Color { r: 0.50, g: 0.50, b: 0.50, a: 1.0 },
                     Color { r: 0.38, g: 0.38, b: 0.38, a: 1.0 })
                } else {
                    let bg = match status {
                        button::Status::Hovered | button::Status::Pressed =>
                            Color { r: 0.90, g: 0.20, b: 0.20, a: 1.0 },
                        _ => COLOR_ERROR,
                    };
                    (bg, Color { r: 0.55, g: 0.04, b: 0.04, a: 1.0 })
                };
                button::Style {
                    background: Some(bg.into()),
                    border: Border { color: border_col, width: 1.5, ..icon_radius },
                    shadow: Default::default(),
                    text_color: Color::WHITE,
                }
            });
        let reject_btn: Element<Message> = if active {
            reject_btn_base.on_press(Message::TrialReject).into()
        } else { reject_btn_base.into() };

        let accept_btn_base = button(
            bi(Bootstrap::CheckLg)
                .size(14)
                .color(Color { r: 0.1, g: 0.3, b: 0.1, a: 1.0 })
                .align_y(Vertical::Center)
                .align_x(Horizontal::Center)
            )
            .padding([1, 3])
            .style(move |_, status: button::Status| {
                let (bg, border_col) = if !active {
                    (Color { r: 0.50, g: 0.50, b: 0.50, a: 1.0 },
                     Color { r: 0.38, g: 0.38, b: 0.38, a: 1.0 })
                } else {
                    let bg = match status {
                        button::Status::Hovered | button::Status::Pressed =>
                            Color { r: 0.12, g: 0.68, b: 0.12, a: 1.0 },
                        _ => COLOR_SUCCESS,
                    };
                    (bg, Color { r: 0.04, g: 0.40, b: 0.04, a: 1.0 })
                };
                button::Style {
                    background: Some(bg.into()),
                    border: Border { color: border_col, width: 1.5, ..icon_radius },
                    shadow: Default::default(),
                    text_color: Color::WHITE,
                }
            });
        let accept_btn: Element<Message> = if active {
            accept_btn_base.on_press(Message::TrialAccept).into()
        } else { accept_btn_base.into() };

        let mut tr_items: Vec<Element<Message>> = Vec::new();
        if trial_tier > 0 {
            let (r, g, b) = TRIAL_FILLED[(trial_tier - 1) % TRIAL_FILLED.len()];
            tr_items.push(text(format!("Tier {trial_tier}")).size(12).color(Color::from_rgb(r, g, b)).into());
            tr_items.push(Space::with_width(Length::Fixed(6.0)).into());
        }
        tr_items.push(reject_btn);
        tr_items.push(Space::with_width(Length::Fixed(4.0)).into());
        tr_items.push(enter_btn);
        tr_items.push(Space::with_width(Length::Fixed(4.0)).into());
        tr_items.push(accept_btn);
        let tr_group: Element<Message> = row(tr_items).align_y(Vertical::Center).into();

        const CLEAR_W: f32 = 100.0;
        const UR_W:    f32 =  64.0;
        let grid_center = row_clue_w + (C * w as f32) / 2.0;
        let left_gap = grid_center - CLEAR_W - UR_W / 2.0;

        if left_gap >= 4.0 {
            row![
                clear_btn,
                Space::with_width(Length::Fixed(left_gap)),
                undo_btn,
                Space::with_width(Length::Fixed(6.0)),
                redo_btn,
                Space::with_width(Length::Fill),
                tr_group,
            ].align_y(Vertical::Center).into()
        } else {
            row![
                clear_btn,
                Space::with_width(Length::Fill),
                undo_btn,
                Space::with_width(Length::Fixed(6.0)),
                redo_btn,
                Space::with_width(Length::Fill),
                tr_group,
            ].align_y(Vertical::Center).into()
        }
    };

    // ── Nav ───────────────────────────────────────────────────────────────────
    let nav_el: Element<'a, Message> = {
        let prev_btn: Element<Message> = if let Some(pk) = prev_key {
            button(row![bi(Bootstrap::ChevronLeft).size(13), text("Previous").size(12)].spacing(4).align_y(Vertical::Center))
                .on_press(Message::PuzzleFocused(pk))
                .padding([4, 10])
                .into()
        } else {
            button(row![bi(Bootstrap::ChevronLeft).size(13), text("Previous").size(12)].spacing(4).align_y(Vertical::Center))
                .padding([4, 10])
                .into()
        };
        let next_btn: Element<Message> = if let Some(nk) = next_key {
            button(row![text("Next").size(12), bi(Bootstrap::ChevronRight).size(13)].spacing(4).align_y(Vertical::Center))
                .on_press(Message::PuzzleFocused(nk))
                .padding([4, 10])
                .into()
        } else {
            button(row![text("Next").size(12), bi(Bootstrap::ChevronRight).size(13)].spacing(4).align_y(Vertical::Center))
                .padding([4, 10])
                .into()
        };
        row![prev_btn, Space::with_width(Length::Fill), next_btn]
            .align_y(Vertical::Center)
            .into()
    };

    // The 2-px separator between clue strips and the cell grid is owned by the
    // frozen header regions, not by the scrollable cells.  corner_w/corner_h are
    // bumped by 2 so the layout engine positions cells correctly.
    let col_clues_el = column(vec![
        row(col_clue_cells).into(),
        solid!(cells_row_w, 2.0, border_maj),
    ]);
    let row_clues_el = row(vec![
        column(row_clue_vec).into(),
        solid!(2.0, C * h as f32, border_maj),
    ]);
    GridRegions {
        corner:       corner_el,
        col_clues:    col_clues_el.into(),
        top_right:    top_right_el,
        row_clues:    row_clues_el.into(),
        cells:        column(cells_rows).into(),
        row_sums:     column(row_sum_vec).into(),
        col_sums:     col_sums_el,
        bottom_left:  bottom_left_el,
        bottom_right: bottom_right_el,
        controls:     controls_el,
        nav:          nav_el,
        corner_w:     row_clue_w + 2.0,
        corner_h:     col_clue_h + 2.0,
        sum_w,
        sum_h:        N + 2.0,
        puzzle_w:     w,
        puzzle_h:     h,
    }
}
