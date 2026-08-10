use iced::{
    alignment::Vertical,
    widget::{
        button, checkbox, column, container, horizontal_rule, mouse_area,
        pick_list, row, scrollable, text, text_input, tooltip, Space,
    },
    Color, Element, Length, Padding, Theme,
};
use iced_fonts::bootstrap::Bootstrap;
use nonogram_core::{CellState, ParsedPuzzle, SolutionState};

use crate::solver::SolverKind;
use super::{App, LoadedFile, Message};
use super::style::{
    bi, primary_menu_btn, style_panel, style_header_row, style_chevron_btn, style_list_row_btn,
    status_row, WARN_COLOR, COLOR_SUCCESS, COLOR_ERROR, COLOR_AMBIGUOUS,
};
use super::persistence::relative_path;

impl App {
    pub(crate) fn view_toolbar(&self) -> Element<'_, Message> {
        const BTN_PAD: [u16; 2] = [5, 8];

        let all_selected_invalid = !self.selected.is_empty() && self.selected.iter().all(|(fi, pi)| {
            match self.files.get(*fi).and_then(|f| f.puzzles.get(*pi)) {
                Some(ParsedPuzzle::Valid(p)) => super::trivial_invalid_reason(p).is_some(),
                _ => true,
            }
        });

        let solve_abort: Element<Message> = if self.busy {
            button(
                row![bi(Bootstrap::XCircleFill).size(14), text("Abort").size(14)]
                    .spacing(4)
                    .align_y(Vertical::Center),
            )
            .on_press(Message::AbortClicked)
            .padding(BTN_PAD)
            .into()
        } else {
            let b = button(text("Solve").size(14)).padding(BTN_PAD);
            let can_solve = self.solver.is_machine() && !self.selected.is_empty() && !all_selected_invalid;
            let b = if can_solve { b.on_press(Message::SolveSelected) } else { b };
            if all_selected_invalid && self.solver.is_machine() && !self.selected.is_empty() {
                tooltip(b, "Puzzle is invalid – cannot be solved", tooltip::Position::Bottom).into()
            } else {
                b.into()
            }
        };

        let all_done = self.focused
            .and_then(|k| self.all_solutions.get(&(k, self.solver)))
            .map(|a| !a.aborted)
            .unwrap_or(false);
        let more_btn: Element<Message> = if self.solver.supports_exhaustive() {
            if all_done {
                tooltip(
                    button(text("Done").size(14)).padding(BTN_PAD),
                    "All solutions found",
                    tooltip::Position::Bottom,
                ).into()
            } else {
                let b = button(text("More").size(14)).padding(BTN_PAD);
                let b = if !self.selected.is_empty() && !self.busy {
                    b.on_press(Message::FindAllSelected)
                } else {
                    b
                };
                tooltip(b, "Search for more solutions", tooltip::Position::Bottom).into()
            }
        } else {
            button(text("More").size(14)).padding(BTN_PAD).into()
        };

        let import_btn = mouse_area(
            primary_menu_btn(Bootstrap::Upload, "Import", self.show_import_menu, 16.0, [5, 10])
                .on_press(Message::ImportMenuToggled),
        )
        .on_enter(Message::ImportBtnHovered);

        let focus_icon = if self.focus_mode { Bootstrap::FullscreenExit } else { Bootstrap::Fullscreen };
        let focus_btn = {
            let b = button(bi(focus_icon).size(14)).padding(BTN_PAD);
            if self.focused.is_some() || self.focus_mode { b.on_press(Message::FocusModeToggled) } else { b }
        };

        let theme_icon = match self.theme { Theme::Light => Bootstrap::MoonFill, _ => Bootstrap::SunFill };

        let settings_btn = button(bi(Bootstrap::GearFill).size(14))
            .on_press(if self.show_settings { Message::SettingsClosed } else { Message::SettingsOpened })
            .padding(BTN_PAD);

        row![
            import_btn,
            Space::with_width(Length::Fill),
            text("Solver:").size(14),
            pick_list(SolverKind::ALL, Some(self.solver), Message::SolverChanged),
            {
                let label = if self.show_solve_progress { "Live ON" } else { "Live" };
                let b = button(text(label).size(12)).padding([3, 6]);
                if self.solver == SolverKind::Cuttlefish { b.on_press(Message::SolveProgressToggled) } else { b }
            },
            Space::with_width(Length::Fixed(12.0)),
            solve_abort,
            more_btn,
            focus_btn,
            button(bi(theme_icon).size(14)).on_press(Message::ThemeToggled).padding(BTN_PAD),
            settings_btn,
        ]
        .spacing(8)
        .padding(8)
        .align_y(Vertical::Center)
        .into()
    }

    pub(crate) fn render_file_rows<'a>(
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
        hdr.push(text(file.name.as_str()).size(13).width(Length::Fill).into());
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
                        let is_invalid = super::trivial_invalid_reason(puzzle).is_some();
                        let badge: Option<Element<Message>> = if is_invalid {
                            Some(bi(Bootstrap::ExclamationCircleFill).size(11).color(WARN_COLOR).into())
                        } else if manual_solved {
                            Some(bi(Bootstrap::CheckCircle).size(11)
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
                                    let text_color = if is_invalid { WARN_COLOR } else { p.background.base.text };
                                    button::Style { background: match status { button::Status::Hovered => Some(p.primary.weak.color.into()), _ => None }, text_color, border: iced::Border::default(), shadow: iced::Shadow::default() }
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
                            row![bi(Bootstrap::ExclamationCircleFill).size(11).color(WARN_COLOR), text(name.as_str()).size(13)]
                                .spacing(4).align_y(Vertical::Center),
                        )
                        .on_press(Message::PuzzleFocused(key))
                        .style(move |theme: &Theme, status| {
                            let p = theme.extended_palette();
                            if is_focused {
                                button::Style { background: Some(p.primary.strong.color.into()), text_color: p.primary.strong.text, border: iced::Border::default(), shadow: iced::Shadow::default() }
                            } else {
                                button::Style { background: match status { button::Status::Hovered => Some(p.primary.weak.color.into()), _ => None }, text_color: WARN_COLOR, border: iced::Border::default(), shadow: iced::Shadow::default() }
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

    pub(crate) fn view_left_panel(&self) -> Element<'_, Message> {
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

        let reload_btn = {
            let b = button(bi(Bootstrap::ArrowClockwise).size(11)).padding([3, 4]);
            if !self.busy { b.on_press(Message::RefreshClicked) } else { b }
        };

        let list_toolbar = container(
            row![
                checkbox("", all_selected)
                    .on_toggle(|v| if v { Message::SelectAll } else { Message::DeselectAll }),
                bi(Bootstrap::CheckAll).size(14),
                Space::with_width(Length::Fill),
                collapse_toggle,
                reload_btn,
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
                text(folder_name.clone()).size(13).width(Length::Fill),
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

        let url_row: Option<Element<Message>> = if self.show_url_import {
            let fetch_btn = {
                let b = button(text("Fetch").size(12)).padding([3, 8]);
                if !self.url_input.trim().is_empty() && !self.busy {
                    b.on_press(Message::UrlFetchClicked)
                } else {
                    b
                }
            };
            Some(
                container(
                    row![
                        text_input("https://puzzle-nonograms.com/?pl=…", &self.url_input)
                            .on_input(Message::UrlInputChanged)
                            .on_submit(Message::UrlFetchClicked)
                            .size(12)
                            .width(Length::Fill),
                        fetch_btn,
                    ]
                    .spacing(4)
                    .padding([4, 8])
                    .align_y(Vertical::Center),
                )
                .style(style_panel)
                .width(Length::Fill)
                .into(),
            )
        } else {
            None
        };

        let mut inner: Vec<Element<Message>> = vec![list_toolbar.into(), horizontal_rule(1).into()];
        if let Some(ur) = url_row {
            inner.push(ur);
            inner.push(horizontal_rule(1).into());
        }
        inner.push(scrollable(column(items).spacing(1)).height(Length::Fill).into());

        container(column(inner))
        .style(style_panel)
        .width(Length::Fixed(310.0))
        .height(Length::Fill)
        .into()
    }

    pub(crate) fn view_results_table(&self) -> Element<'_, Message> {
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
                        COLOR_SUCCESS,
                        String::from("Solved"),
                    ),
                    SolutionState::Aborted => {
                        let filled = result.grid.iter().filter(|&&c| c != CellState::Unknown).count();
                        (
                            Bootstrap::XCircleFill,
                            WARN_COLOR,
                            format!("Aborted ({}/{})", filled, puzzle.width * puzzle.height),
                        )
                    }
                    SolutionState::Partial => {
                        let filled = result.grid.iter().filter(|&&c| c != CellState::Unknown).count();
                        (
                            Bootstrap::DashLg,
                            COLOR_AMBIGUOUS,
                            format!("Partial ({}/{})", filled, puzzle.width * puzzle.height),
                        )
                    }
                    SolutionState::Unsolvable => (
                        Bootstrap::XLg,
                        COLOR_ERROR,
                        String::from("No solution"),
                    ),
                    SolutionState::Invalid(reason) => (
                        Bootstrap::ExclamationCircleFill,
                        WARN_COLOR,
                        format!("Invalid — {reason}"),
                    ),
                };

                let status_col: Element<Message> = container(status_row(status_icon, status_color, status_text))
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

    pub(crate) fn view_status_bar(&self) -> Element<'_, Message> {
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
}
