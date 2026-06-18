use std::collections::HashSet;

use iced::{
    alignment::{Horizontal, Vertical},
    widget::{button, column, container, horizontal_rule, mouse_area, row, text, Space},
    Color, Element, Length, Padding,
};
use iced_fonts::bootstrap::Bootstrap;
use nonogram_core::{CellState, Puzzle, SolveResult};

use super::{Key, Message};
use super::settings::{AssistanceSettings, CellSettings};
use super::style::{bi, fwd};

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

    let mut ci = 0usize;
    let mut gi = 0usize;
    while ci < n {
        while gi < w && cells[gi] == CellState::Empty { gi += 1; }
        if gi >= w || cells[gi] == CellState::Unknown { break; }
        let run_start = gi;
        while gi < w && cells[gi] == CellState::Filled { gi += 1; }
        let run_len = gi - run_start;
        if gi < w && cells[gi] == CellState::Unknown { break; }
        if run_len == clues[ci] as usize { dim[ci] = true; ci += 1; } else { break; }
    }
    let left_matched = ci;

    let mut ci = n as isize - 1;
    let mut gi = w as isize - 1;
    while ci >= left_matched as isize {
        while gi >= 0 && cells[gi as usize] == CellState::Empty { gi -= 1; }
        if gi < 0 || cells[gi as usize] == CellState::Unknown { break; }
        let run_end = gi;
        while gi >= 0 && cells[gi as usize] == CellState::Filled { gi -= 1; }
        let run_len = (run_end - gi) as usize;
        if gi >= 0 && cells[gi as usize] == CellState::Unknown { break; }
        if run_len == clues[ci as usize] as usize { dim[ci as usize] = true; ci -= 1; } else { break; }
    }

    dim
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
// Puzzle grid renderer (interactive)
// ---------------------------------------------------------------------------

pub(crate) fn view_grid<'a>(
    puzzle: &'a Puzzle,
    grid: Option<Vec<CellState>>,
    key: Key,
    settings: &'a CellSettings,
    trial: &'a [(Vec<CellState>, Option<(usize, usize)>)],
    assistance: &'a AssistanceSettings,
    manual_dim_rows: Option<&'a HashSet<usize>>,
    manual_dim_cols: Option<&'a HashSet<usize>>,
    has_undo: bool,
    has_redo: bool,
    prev_key: Option<Key>,
    next_key: Option<Key>,
) -> Element<'a, Message> {
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

    let right_border = w % 5 == 0;
    let bot_border   = h % 5 == 0;

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
        10..=99  => N + 4.0,
        _ => N + 10.0,
    };

    let grid_total_w: f32 = row_clue_w + C * w as f32
        + if right_border { 2.0 } else { 0.0 }
        + sum_w;

    let warn_text = Color::from_rgb(0.75, 0.38, 0.0);

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

        // Corner: minimap
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

        // Top-right corner: col grand total
        {
            let txt_color = if total_mismatch { warn_text } else { Color::from_rgb(0.1, 0.1, 0.1) };
            let corner_inner = if total_mismatch {
                container(text(total_col.to_string()).size(13).color(txt_color))
                    .width(Length::Fill).height(Length::Fill)
                    .align_x(Horizontal::Center).align_y(Vertical::Bottom)
                    .padding(Padding { bottom: 2.0, ..Padding::ZERO })
                    .style(|_| container::Style {
                        background: Some(Color::from_rgba(0.75, 0.38, 0.0, 0.15).into()),
                        border: iced::Border { radius: 3.0.into(), color: Color::from_rgb(0.75, 0.38, 0.0), width: 1.0 },
                        ..Default::default()
                    })
            } else {
                container(text(total_col.to_string()).size(13).color(txt_color))
                    .width(Length::Fill).height(Length::Fill)
                    .align_x(Horizontal::Center).align_y(Vertical::Bottom)
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

        // Row clue area
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

        // Grid cells
        for c in 0..w {
            let lp = if c % 5 == 0 { 2.0_f32 } else { 1.0 };
            let vc = if c % 5 == 0 { border_maj } else { border_min };

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

            let cell_face: Element<'a, Message> = if let Some(t) = origin_tier {
                match state {
                    CellState::Empty => {
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

        // Row sum
        {
            let row_sum: u32 = line_sum(&puzzle.row_clues[r]);
            let warn = row_infeasible[r];
            let txt_color = if warn { warn_text } else { Color::from_rgb(0.1, 0.1, 0.1) };
            let rsum_inner = if warn {
                container(text(row_sum.to_string()).size(13).color(txt_color))
                    .width(Length::Fill).height(Length::Fill)
                    .align_x(Horizontal::Center).align_y(Vertical::Center)
                    .style(|_| container::Style {
                        background: Some(Color::from_rgba(0.75, 0.38, 0.0, 0.15).into()),
                        border: iced::Border { radius: 3.0.into(), color: Color::from_rgb(0.75, 0.38, 0.0), width: 1.0 },
                        ..Default::default()
                    })
            } else {
                container(text(row_sum.to_string()).size(13).color(txt_color))
                    .width(Length::Fill).height(Length::Fill)
                    .align_x(Horizontal::Center).align_y(Vertical::Center)
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

    // Outer bottom border
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

        // Bottom-left corner: row grand total
        {
            let txt_color = if total_mismatch { warn_text } else { Color::from_rgb(0.1, 0.1, 0.1) };
            let corner_inner = if total_mismatch {
                container(text(total_row.to_string()).size(13).color(txt_color))
                    .width(Length::Fill).height(Length::Fill)
                    .align_x(Horizontal::Right).align_y(Vertical::Center)
                    .padding(Padding { right: 4.0, ..Padding::ZERO })
                    .style(|_| container::Style {
                        background: Some(Color::from_rgba(0.75, 0.38, 0.0, 0.15).into()),
                        border: iced::Border { radius: 3.0.into(), color: Color::from_rgb(0.75, 0.38, 0.0), width: 1.0 },
                        ..Default::default()
                    })
            } else {
                container(text(total_row.to_string()).size(13).color(txt_color))
                    .width(Length::Fill).height(Length::Fill)
                    .align_x(Horizontal::Right).align_y(Vertical::Center)
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
                    .width(Length::Fill).height(Length::Fill)
                    .align_x(Horizontal::Center).align_y(Vertical::Center)
                    .style(|_| container::Style {
                        background: Some(Color::from_rgba(0.75, 0.38, 0.0, 0.15).into()),
                        border: iced::Border { radius: 3.0.into(), color: Color::from_rgb(0.75, 0.38, 0.0), width: 1.0 },
                        ..Default::default()
                    })
            } else {
                container(text(col_sum.to_string()).size(13).color(txt_color))
                    .width(Length::Fill).height(Length::Fill)
                    .align_x(Horizontal::Center).align_y(Vertical::Center)
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

        // Bottom-right corner: toggle clue sums with gaps
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

    // ── Controls below the grid ───────────────────────────────────────────
    {
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

        let enter_label = if trial_tier == 0 { "Trial" } else { "Deeper" };
        let enter_btn = button(text(enter_label).size(12))
            .on_press(Message::TrialEnter)
            .padding([2, 8]);
        let reject_btn = if trial_tier > 0 {
            button(text("Reject trial").size(12)).on_press(Message::TrialReject).padding([2, 8])
        } else {
            button(text("Reject trial").size(12)).padding([2, 8])
        };

        let mut tr_items: Vec<Element<Message>> = Vec::new();
        if trial_tier > 0 {
            let (r, g, b) = TRIAL_FILLED[(trial_tier - 1) % TRIAL_FILLED.len()];
            tr_items.push(text(format!("Tier {trial_tier}")).size(12).color(Color::from_rgb(r, g, b)).into());
            tr_items.push(Space::with_width(Length::Fixed(6.0)).into());
        }
        tr_items.push(enter_btn.into());
        tr_items.push(Space::with_width(Length::Fixed(6.0)).into());
        tr_items.push(reject_btn.into());
        let tr_group: Element<Message> = row(tr_items).align_y(Vertical::Center).into();

        const CLEAR_W: f32 = 100.0;
        const UR_W:    f32 =  64.0;
        const TR_W:    f32 = 160.0;
        let grid_center = row_clue_w + (C * w as f32) / 2.0;
        let left_gap  = grid_center - CLEAR_W - UR_W / 2.0;
        let right_gap = grid_total_w - grid_center - UR_W / 2.0 - TR_W;

        let btn_row: Element<Message> = if left_gap >= 4.0 && right_gap >= 4.0 {
            row![
                clear_btn,
                Space::with_width(Length::Fixed(left_gap)),
                undo_btn,
                Space::with_width(Length::Fixed(6.0)),
                redo_btn,
                Space::with_width(Length::Fixed(right_gap)),
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
        };

        all_rows.push(
            container(horizontal_rule(1))
                .width(Length::Fixed(grid_total_w))
                .into()
        );
        all_rows.push(
            container(btn_row)
                .padding([6, 0])
                .width(Length::Fixed(grid_total_w))
                .into()
        );
    }

    // ── Prev / Next navigation ────────────────────────────────────────────
    {
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
        all_rows.push(
            container(horizontal_rule(1))
                .width(Length::Fixed(grid_total_w))
                .into()
        );
        all_rows.push(
            container(
                row![prev_btn, Space::with_width(Length::Fill), next_btn]
                    .align_y(Vertical::Center),
            )
            .padding([6, 0])
            .width(Length::Fixed(grid_total_w))
            .into()
        );
    }

    container(column(all_rows))
        .padding(Padding { top: 16.0, right: 16.0, bottom: 16.0, left: 0.0 })
        .into()
}
