use iced::{
    alignment::Vertical,
    widget::{
        button, column, container, horizontal_rule, mouse_area, row,
        scrollable, slider, stack, text, Space,
    },
    Color, Element, Length, Padding, Theme,
};
use iced_fonts::bootstrap::Bootstrap;
use nonogram_core::{CellState, ParsedPuzzle, SolutionState};

use crate::frozen_grid_viewport::FrozenGridViewport;
use super::{App, Key, Message};
use super::settings::{FocusKey, SecondaryFocusKey};
use super::style::{
    bi, icon_char, style_panel, style_header_row,
    status_row, warn_banner_style,
    WARN_COLOR, COLOR_SUCCESS, COLOR_ERROR, COLOR_AMBIGUOUS,
};
use super::settings::{CellVisual, FILLED_ICON_OPTIONS, EMPTY_ICON_OPTIONS};
use super::export::ExportFormat;
use super::persistence::relative_path;
use super::grid_view::{build_grid_regions, compute_hover_runs, HoverRuns};

impl App {
    pub(crate) fn view_puzzle_detail(&self, fi: usize, pi: usize) -> Element<'_, Message> {
        let Some(file) = self.files.get(fi) else {
            return text("(missing file)").into();
        };
        let entry = match file.puzzles.get(pi) {
            Some(e) => e,
            None => return text("(missing puzzle)").into(),
        };

        let key = (fi, pi);
        let sep_color = self.theme.extended_palette().background.strong.color;

        let back_btn = button(
            row![bi(Bootstrap::ArrowLeft).size(13), text("Back").size(13)]
                .spacing(4).align_y(Vertical::Center),
        )
        .on_press(Message::Unfocus);

        if let ParsedPuzzle::Invalid { name, reason, puzzle: partial } = entry {
            let status_el = status_row(Bootstrap::ExclamationCircleFill, WARN_COLOR, "Invalid puzzle");

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
                    bi(Bootstrap::ExclamationCircleFill).size(13).color(WARN_COLOR),
                    text(reason.to_string()).size(13),
                ]
                .spacing(8)
                .align_y(Vertical::Center),
            )
            .style(|_| warn_banner_style())
            .padding([8, 16])
            .width(Length::Fill);

            if let Some(puzzle) = partial {
                let display_grid = self.manual_grids.get(&key).cloned();
                let trial_info: &[(Vec<CellState>, Option<(usize, usize)>)] =
                    self.trial_stack.get(&key).map(|v| v.as_slice()).unwrap_or(&[]);
                let pan_off = self.pan_offset;
                let hover_runs: HoverRuns = display_grid.as_deref()
                    .map(|g| compute_hover_runs(self.hover_cell, key, puzzle, g))
                    .unwrap_or_else(HoverRuns::none);
                let crosshair = if self.assistance.crosshair_enabled {
                    let xr = self.crosshair_row;
                    let xc = self.crosshair_col;
                    if xr.is_none() && xc.is_none() { None }
                    else { Some((xr, xc, self.assistance.crosshair_color)) }
                } else { None };
                let regions = build_grid_regions(puzzle, display_grid, key, &self.cell_settings, trial_info, &self.assistance, self.manual_dim_rows.get(&key), self.manual_dim_cols.get(&key), self.undo_stack.get(&key).map(|s| !s.is_empty()).unwrap_or(false), self.redo_stack.get(&key).map(|s| !s.is_empty()).unwrap_or(false), None, None, hover_runs, crosshair, true);
                let frozen = {
                    let f = FrozenGridViewport::from_regions(regions, key, pan_off, self.theme.extended_palette().background.base.color)
                        .on_pan(|v| Message::PanOffsetChanged(v))
                        .on_region(Message::GridRegionChanged)
                        .space_pan(self.space_held);
                    if !self.focus_mode { f.top_separator(sep_color) } else { f }
                };
                return column![
                    header,
                    horizontal_rule(1),
                    container(reason_banner).padding([8, 16]).width(Length::Fill),
                    Space::with_height(Length::Fixed(1.0)),
                    mouse_area(
                        container(frozen)
                            .padding(Padding { left: 16.0, ..Padding::ZERO })
                            .width(Length::Fill)
                            .height(Length::Fill),
                    )
                    .on_exit(Message::GridLeft),
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
        let manually_solved = self.solved_manually.contains(&(relative_path(&file.path), puzzle.name.clone()));

        let result = self.results.get(&(key, self.solver));
        let all    = self.all_solutions.get(&(key, self.solver));
        let active = all
            .and_then(|a| a.solutions.get(self.solution_index))
            .or(result);

        let grid_data: Option<Vec<CellState>> = if self.solver.is_machine() {
            let cursor = self.step_cursor;
            active.and_then(|r| {
                if r.grid.is_empty() { return None; }
                Some(if !r.steps.is_empty() && cursor < r.steps.len() {
                    super::grid_view::grid_at_step(puzzle, r, cursor)
                } else {
                    r.grid.clone()
                })
            })
            .or_else(|| self.solve_progress.get(&key).map(|u| u.grid.clone()))
        } else {
            None
        };

        let display_grid: Option<Vec<CellState>> = if self.solver.is_machine() {
            grid_data
        } else {
            self.manual_grids.get(&key).cloned()
        };

        let status_el: Element<Message> = if let Some(a) = all {
            let n      = a.solutions.len();
            let suffix = if a.aborted { " (aborted)" } else { "" };
            match n {
                0 => status_row(Bootstrap::XLg,    COLOR_ERROR,    format!("No solutions{suffix}")),
                1 => status_row(Bootstrap::CheckLg, COLOR_SUCCESS,  format!("Unique solution{suffix}")),
                _ => status_row(Bootstrap::DashLg,  COLOR_AMBIGUOUS, format!("{n} solutions — ambiguous{suffix}")),
            }
        } else if let Some(res) = result {
            match &res.state {
                SolutionState::Complete =>
                    status_row(Bootstrap::CheckLg, COLOR_SUCCESS, "Solved"),
                SolutionState::Aborted => {
                    let filled = res.grid.iter().filter(|&&c| c != CellState::Unknown).count();
                    status_row(Bootstrap::XCircleFill, WARN_COLOR, format!("Aborted ({filled}/{})", puzzle.width * puzzle.height))
                }
                SolutionState::Partial => {
                    let filled = res.grid.iter().filter(|&&c| c != CellState::Unknown).count();
                    status_row(Bootstrap::DashLg, COLOR_AMBIGUOUS, format!("Partial ({filled}/{})", puzzle.width * puzzle.height))
                }
                SolutionState::Unsolvable =>
                    status_row(Bootstrap::XLg, COLOR_ERROR, "No solution"),
                SolutionState::Invalid(reason) =>
                    status_row(Bootstrap::ExclamationCircleFill, WARN_COLOR, format!("Invalid — {reason}")),
            }
        } else if manually_solved {
            status_row(Bootstrap::CheckLg, COLOR_SUCCESS, "Solved")
        } else {
            text("Not yet solved").size(13).color(Color::from_rgb(0.45, 0.45, 0.45)).into()
        };

        let export_active = self.show_export_menu;
        let export_btn = mouse_area(
            button(
                row![bi(Bootstrap::Download).size(13), text("Export").size(13)]
                    .spacing(4).align_y(Vertical::Center),
            )
            .on_press(Message::ExportMenuToggled)
            .padding([2, 8])
            .style(move |theme: &Theme, status| {
                let mut s = button::Style::default().with_background(
                    theme.extended_palette().primary.base.color,
                );
                s.text_color = theme.extended_palette().primary.base.text;
                if export_active {
                    s.border = iced::Border {
                        radius: 4.0.into(),
                        color: theme.extended_palette().primary.strong.color,
                        width: 2.0,
                    };
                } else if matches!(status, button::Status::Hovered) {
                    s.background = Some(theme.extended_palette().primary.strong.color.into());
                }
                s
            })
        ).on_enter(Message::ExportBtnHovered);

        let answer_revealed = manually_solved || self.revealed_answers.contains(&key);
        let computer_complete = result.map(|r| matches!(&r.state, SolutionState::Complete)).unwrap_or(false);

        let mut header_items: Vec<Element<Message>> = vec![
            back_btn.into(),
            Space::with_width(Length::Fixed(12.0)).into(),
            text(puzzle.name.as_str()).size(16).into(),
        ];
        if let Some(answer) = &puzzle.answer {
            if answer_revealed {
                header_items.push(text(": ").size(16).into());
                header_items.push(
                    text(format!("\"{answer}\"")).size(16)
                        .color(Color::from_rgb(0.0, 0.62, 0.24))
                        .into()
                );
            } else if computer_complete {
                header_items.push(
                    button(text("Reveal answer").size(12))
                        .on_press(Message::RevealAnswer(key))
                        .padding([2, 8])
                        .into()
                );
            }
        }
        let timer = self.timers.get(&key).cloned().unwrap_or_default();
        let timer_label = {
            let s = timer.elapsed_secs;
            let h = s / 3600;
            let m = (s % 3600) / 60;
            let sec = s % 60;
            if h > 0 { format!("{h}:{m:02}:{sec:02}") } else { format!("{m}:{sec:02}") }
        };
        let play_pause_btn = if timer.running {
            button(bi(Bootstrap::PauseFill).size(13))
                .on_press(Message::TimerPause(key))
                .padding([2, 4])
        } else {
            button(bi(Bootstrap::PlayFill).size(13))
                .on_press(Message::TimerStart(key))
                .padding([2, 4])
        };
        let reset_btn = button(bi(Bootstrap::ArrowCounterclockwise).size(13))
            .on_press(Message::TimerReset(key))
            .padding([2, 4]);

        header_items.extend([
            Space::with_width(Length::Fixed(8.0)).into(),
            text(format!("{}x{}", puzzle.width, puzzle.height))
                .size(13)
                .color(Color::from_rgb(0.4, 0.4, 0.4))
                .into(),
            Space::with_width(Length::Fixed(6.0)).into(),
            text(timer_label).size(13).into(),
            play_pause_btn.into(),
            reset_btn.into(),
            Space::with_width(Length::Fixed(12.0)).into(),
            export_btn.into(),
            button(text("Dbg").size(13))
                .on_press(Message::DebugGridDump)
                .padding([2, 6])
                .into(),
            Space::with_width(Length::Fill).into(),
        ]);
        header_items.push(status_el);
        let header = container(
            row(header_items).spacing(4).align_y(Vertical::Center),
        )
        .padding([10, 16])
        .width(Length::Fill);

        let mut items: Vec<Element<Message>> = Vec::new();
        if !self.focus_mode {
            items.push(header.into());
        }

        if let Some(res) = result {
            if let SolutionState::Invalid(reason) = &res.state {
                let banner = container(
                    row![
                        bi(Bootstrap::ExclamationCircleFill).size(13).color(WARN_COLOR),
                        text(format!("Invalid — {reason}")).size(13),
                    ]
                    .spacing(8)
                    .align_y(Vertical::Center),
                )
                .style(|_| warn_banner_style())
                .padding([8, 16])
                .width(Length::Fill);

                items.push(horizontal_rule(1).into());
                items.push(container(banner).padding([8, 16]).width(Length::Fill).into());
            }
        }

        if self.solver.is_machine() {
            if let Some(a) = all {
                let n = a.solutions.len();
                if n > 0 {
                    let idx      = self.solution_index.min(n.saturating_sub(1));
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
                    items.push(horizontal_rule(1).into());
                    items.push(sol_nav.into());
                }
            }
        }

        if self.solver.is_machine() {
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

                    items.push(horizontal_rule(1).into());
                    items.push(step_nav.into());
                }
            }
        }

        if self.solver.is_machine() {
            if let Some(res) = result {
                if !res.grid.is_empty() {
                    let copy_btn = button(
                        row![bi(Bootstrap::ClipboardCheck).size(13), text("Copy to Manual").size(13)]
                            .spacing(4).align_y(Vertical::Center),
                    )
                    .on_press(Message::CopyToManual(key))
                    .padding([4, 10]);
                    items.push(horizontal_rule(1).into());
                    items.push(
                        container(row![copy_btn].padding([6, 12]))
                            .width(Length::Fill)
                            .into(),
                    );
                }
            }
        }

        let trial_info: &[(Vec<CellState>, Option<(usize, usize)>)] =
            self.trial_stack.get(&key).map(|v| v.as_slice()).unwrap_or(&[]);

        let all_keys: Vec<Key> = self.files.iter().enumerate()
            .flat_map(|(fi, f)| f.puzzles.iter().enumerate()
                .filter(|(_, e)| e.is_valid())
                .map(move |(pi, _)| (fi, pi)))
            .collect();
        let pos = all_keys.iter().position(|&k| k == key);
        let prev_key = pos.and_then(|i| i.checked_sub(1)).map(|i| all_keys[i]);
        let next_key = pos.and_then(|i| all_keys.get(i + 1).copied());

        let pan_off = self.pan_offset;
        let hover_runs: HoverRuns = display_grid.as_deref()
            .map(|g| compute_hover_runs(self.hover_cell, key, puzzle, g))
            .unwrap_or_else(HoverRuns::none);
        let crosshair = if self.assistance.crosshair_enabled {
            let xr = self.crosshair_row;
            let xc = self.crosshair_col;
            if xr.is_none() && xc.is_none() { None }
            else { Some((xr, xc, self.assistance.crosshair_color)) }
        } else { None };
        let regions = build_grid_regions(puzzle, display_grid, key, &self.cell_settings, trial_info, &self.assistance, self.manual_dim_rows.get(&key), self.manual_dim_cols.get(&key), self.undo_stack.get(&key).map(|s| !s.is_empty()).unwrap_or(false), self.redo_stack.get(&key).map(|s| !s.is_empty()).unwrap_or(false), prev_key, next_key, hover_runs, crosshair, !self.solver.is_machine());
        let frozen = {
            let f = FrozenGridViewport::from_regions(regions, key, pan_off, self.theme.extended_palette().background.base.color)
                .on_pan(|v| Message::PanOffsetChanged(v))
                .on_region(Message::GridRegionChanged)
                .space_pan(self.space_held);
            if !self.focus_mode { f.top_separator(sep_color) } else { f }
        };
        items.push(Space::with_height(Length::Fixed(1.0)).into());
        items.push(
            mouse_area(
                container(frozen)
                    .padding(Padding { left: 16.0, ..Padding::ZERO })
                    .width(Length::Fill)
                    .height(Length::Fill),
            )
            .on_exit(Message::GridLeft)
            .into()
        );

        let detail: Element<Message> = column(items)
            .width(Length::Fill)
            .height(Length::Fill)
            .into();

        if self.show_export_menu {
            let mut popup_items: Vec<Element<Message>> = vec![
                button(row![bi(Bootstrap::Clipboard).size(12), text("Copy string").size(13)]
                    .spacing(6).align_y(iced::alignment::Vertical::Center))
                    .on_press(Message::CopyPuzzleString(key))
                    .width(Length::Fill)
                    .padding([5, 10])
                    .into(),
            ];
            popup_items.extend(ExportFormat::ALL.iter().map(|&fmt| {
                button(text(fmt.label()).size(13))
                    .on_press(Message::ExportFormatSelected(fmt))
                    .width(Length::Fill)
                    .padding([5, 10])
                    .into()
            }));
            popup_items.push(
                button(row![bi(Bootstrap::LinkFourfivedeg).size(12), text("puzz.link").size(13)]
                    .spacing(6).align_y(iced::alignment::Vertical::Center))
                    .on_press(Message::CopyPuzzLink(key))
                    .width(Length::Fill)
                    .padding([5, 10])
                    .into()
            );
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
            let popup_layer: Element<Message> = column![
                Space::with_height(Length::Fixed(44.0)),
                container(popup).padding(Padding { left: self.export_popup_x, ..Padding::ZERO }),
            ]
            .width(Length::Fill)
            .height(Length::Fill)
            .into();
            stack([detail, popup_layer]).into()
        } else {
            detail
        }
    }

    pub(crate) fn view_settings_panel(&self) -> Element<'_, Message> {
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
            let color_row = color_editor_row(
                vis.color,
                move |v| Message::SettingColor(idx, 0, v),
                move |v| Message::SettingColor(idx, 1, v),
                move |v| Message::SettingColor(idx, 2, v),
            );

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

        {
            let cb = self.cell_settings.clue_bg;
            let color_row = color_editor_row(
                cb,
                |v| Message::SettingClueBg(0, v),
                |v| Message::SettingClueBg(1, v),
                |v| Message::SettingClueBg(2, v),
            );
            let section = container(
                column![text("Clue background").size(13), color_row].spacing(8),
            )
            .style(style_panel)
            .padding(12)
            .width(Length::Fill);
            sections.push(section.into());
            sections.push(horizontal_rule(1).into());
        }

        {
            let cb = self.cell_settings.sum_bg;
            let color_row = color_editor_row(
                cb,
                |v| Message::SettingSumBg(0, v),
                |v| Message::SettingSumBg(1, v),
                |v| Message::SettingSumBg(2, v),
            );
            let section = container(
                column![text("Clue sums background").size(13), color_row].spacing(8),
            )
            .style(style_panel)
            .padding(12)
            .width(Length::Fill);
            sections.push(section.into());
            sections.push(horizontal_rule(1).into());
        }

        {
            use iced::widget::{checkbox, pick_list};
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
                        "Auto-cross confirmed gaps from edges (manual only)",
                        self.assistance.auto_cross_edges,
                    )
                    .on_toggle(|_| Message::AssistToggle(3))
                    .size(14),
                    checkbox(
                        "Clue sums with gaps",
                        self.assistance.clue_sums_with_gaps,
                    )
                    .on_toggle(|_| Message::AssistToggle(2))
                    .size(14),
                    row![
                        text("Focus mode key:").size(13),
                        pick_list(
                            FocusKey::ALL,
                            Some(self.assistance.focus_key),
                            Message::FocusKeyChanged,
                        ).text_size(13),
                    ]
                    .spacing(8)
                    .align_y(Vertical::Center),
                    row![
                        text("Focus alt key:").size(13),
                        pick_list(
                            SecondaryFocusKey::ALL,
                            Some(self.assistance.focus_key2),
                            Message::FocusKey2Changed,
                        ).text_size(13),
                        text("  Esc: exit only (always)").size(11)
                            .color(iced::Color::from_rgb(0.5, 0.5, 0.5)),
                    ]
                    .spacing(8)
                    .align_y(Vertical::Center),
                ]
                .spacing(8),
            )
            .style(style_panel)
            .padding(12)
            .width(Length::Fill);
            sections.push(section.into());
            sections.push(horizontal_rule(1).into());
        }

        {
            use iced::widget::checkbox;
            let xc = self.assistance.crosshair_color;
            let color_row = color_alpha_editor_row(
                xc,
                |v| Message::CrosshairColor(0, v),
                |v| Message::CrosshairColor(1, v),
                |v| Message::CrosshairColor(2, v),
                |v| Message::CrosshairColor(3, v),
            );
            let section = container(
                column![
                    text("Crosshair highlight").size(13),
                    checkbox(
                        "Highlight hovered row and column",
                        self.assistance.crosshair_enabled,
                    )
                    .on_toggle(|_| Message::AssistToggle(4))
                    .size(14),
                    color_row,
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
}

// Color preview square + R/G/B/A sliders (for crosshair highlight).
fn color_alpha_editor_row<'a>(
    color: Color,
    mk_r: impl Fn(f32) -> Message + 'static,
    mk_g: impl Fn(f32) -> Message + 'static,
    mk_b: impl Fn(f32) -> Message + 'static,
    mk_a: impl Fn(f32) -> Message + 'static,
) -> Element<'a, Message> {
    let preview_color = Color { r: color.r, g: color.g, b: color.b, a: 1.0 };
    let preview = container(Space::new(Length::Fixed(32.0), Length::Fixed(32.0)))
        .style(move |_| container::Style {
            background: Some(preview_color.into()),
            border: iced::Border {
                radius: 4.0.into(),
                color: Color::from_rgb(0.4, 0.4, 0.4),
                width: 1.0,
            },
            ..Default::default()
        });
    let r_slider = slider(0.0_f32..=1.0, color.r, mk_r).step(0.01_f32).width(Length::Fixed(120.0));
    let g_slider = slider(0.0_f32..=1.0, color.g, mk_g).step(0.01_f32).width(Length::Fixed(120.0));
    let b_slider = slider(0.0_f32..=1.0, color.b, mk_b).step(0.01_f32).width(Length::Fixed(120.0));
    let a_slider = slider(0.0_f32..=1.0, color.a, mk_a).step(0.01_f32).width(Length::Fixed(120.0));
    row![
        preview,
        Space::with_width(Length::Fixed(12.0)),
        column![
            row![text("R").size(11).width(Length::Fixed(12.0)), r_slider,
                 text(format!("{:.2}", color.r)).size(11)].spacing(4).align_y(Vertical::Center),
            row![text("G").size(11).width(Length::Fixed(12.0)), g_slider,
                 text(format!("{:.2}", color.g)).size(11)].spacing(4).align_y(Vertical::Center),
            row![text("B").size(11).width(Length::Fixed(12.0)), b_slider,
                 text(format!("{:.2}", color.b)).size(11)].spacing(4).align_y(Vertical::Center),
            row![text("A").size(11).width(Length::Fixed(12.0)), a_slider,
                 text(format!("{:.2}", color.a)).size(11)].spacing(4).align_y(Vertical::Center),
        ].spacing(4),
    ]
    .align_y(Vertical::Center)
    .spacing(0)
    .into()
}

// Color preview square + R/G/B sliders.
fn color_editor_row<'a>(
    color: Color,
    mk_r: impl Fn(f32) -> Message + 'static,
    mk_g: impl Fn(f32) -> Message + 'static,
    mk_b: impl Fn(f32) -> Message + 'static,
) -> Element<'a, Message> {
    let preview = container(Space::new(Length::Fixed(32.0), Length::Fixed(32.0)))
        .style(move |_| container::Style {
            background: Some(color.into()),
            border: iced::Border {
                radius: 4.0.into(),
                color: Color::from_rgb(0.4, 0.4, 0.4),
                width: 1.0,
            },
            ..Default::default()
        });
    let r_slider = slider(0.0_f32..=1.0, color.r, mk_r).step(0.01_f32).width(Length::Fixed(140.0));
    let g_slider = slider(0.0_f32..=1.0, color.g, mk_g).step(0.01_f32).width(Length::Fixed(140.0));
    let b_slider = slider(0.0_f32..=1.0, color.b, mk_b).step(0.01_f32).width(Length::Fixed(140.0));
    row![
        preview,
        Space::with_width(Length::Fixed(12.0)),
        column![
            row![text("R").size(11).width(Length::Fixed(12.0)), r_slider,
                 text(format!("{:.2}", color.r)).size(11)].spacing(4).align_y(Vertical::Center),
            row![text("G").size(11).width(Length::Fixed(12.0)), g_slider,
                 text(format!("{:.2}", color.g)).size(11)].spacing(4).align_y(Vertical::Center),
            row![text("B").size(11).width(Length::Fixed(12.0)), b_slider,
                 text(format!("{:.2}", color.b)).size(11)].spacing(4).align_y(Vertical::Center),
        ].spacing(4),
    ]
    .align_y(Vertical::Center)
    .spacing(0)
    .into()
}
