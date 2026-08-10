use iced::{alignment::Vertical, Color, Element, Font, Padding, Theme};
use iced::widget::{button, container};
use iced_fonts::bootstrap::{self, Bootstrap};

pub(crate) const BOOTSTRAP_FONT: Font = Font::with_name("bootstrap-icons");
pub(crate) const FORWARD_FONT:   Font = Font::with_name("bootstrap-forward");

// Status / warning colors used across view modules.
pub(crate) const WARN_COLOR:      Color = Color { r: 0.75, g: 0.38, b: 0.00, a: 1.0 };
pub(crate) const COLOR_SUCCESS:   Color = Color { r: 0.08, g: 0.55, b: 0.08, a: 1.0 };
pub(crate) const COLOR_ERROR:     Color = Color { r: 0.78, g: 0.08, b: 0.08, a: 1.0 };
pub(crate) const COLOR_AMBIGUOUS: Color = Color { r: 0.65, g: 0.45, b: 0.00, a: 1.0 };

pub(crate) fn bi(icon: Bootstrap) -> iced::widget::Text<'static> {
    iced::widget::text(bootstrap::icon_to_char(icon).to_string()).font(BOOTSTRAP_FONT)
}

pub(crate) fn fwd(cp: char) -> iced::widget::Text<'static> {
    iced::widget::text(cp.to_string()).font(FORWARD_FONT)
}

pub(crate) fn icon_char(icon: Option<Bootstrap>) -> Option<char> {
    icon.map(bootstrap::icon_to_char)
}

pub(crate) fn style_panel(theme: &Theme) -> container::Style {
    container::Style {
        background: Some(theme.extended_palette().background.base.color.into()),
        ..Default::default()
    }
}

pub(crate) fn style_header_row(theme: &Theme) -> container::Style {
    container::Style {
        background: Some(theme.extended_palette().background.weak.color.into()),
        ..Default::default()
    }
}

pub(crate) fn style_chevron_btn(theme: &Theme, _: button::Status) -> button::Style {
    button::Style {
        background: None,
        text_color: theme.extended_palette().background.weak.text,
        border: iced::Border::default(),
        shadow: iced::Shadow::default(),
    }
}

pub(crate) fn style_list_row_btn(theme: &Theme, status: button::Status) -> button::Style {
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

// Warning container style used for full-width error banners in view_detail.
pub(crate) fn warn_banner_style() -> container::Style {
    container::Style {
        background: Some(Color::from_rgba(0.75, 0.38, 0.0, 0.12).into()),
        border: iced::Border { radius: 4.0.into(), color: WARN_COLOR, width: 1.0 },
        ..Default::default()
    }
}

// Warning container style used for inline clue-sum cells in the grid.
pub(crate) fn warn_inline_style() -> container::Style {
    container::Style {
        background: Some(Color::from_rgba(0.75, 0.38, 0.0, 0.15).into()),
        border: iced::Border { radius: 3.0.into(), color: WARN_COLOR, width: 1.0 },
        ..Default::default()
    }
}

// Icon + text row for solver-result status lines.
pub(crate) fn status_row<'a>(icon: Bootstrap, color: Color, msg: impl Into<String>) -> Element<'a, super::Message> {
    use iced::widget::{row, text};
    row![bi(icon).size(13).color(color), text(msg.into()).size(13)]
        .spacing(4)
        .align_y(Vertical::Center)
        .into()
}

// Styled primary-colour button used to open a popup menu (Import / Export / etc.).
// Pass the icon, label, whether the menu is currently open, the icon+text size,
// and the button padding.  Caller wraps in mouse_area and sets .on_press().
pub(crate) fn primary_menu_btn(
    icon: Bootstrap,
    label: &'static str,
    active: bool,
    text_size: f32,
    padding: impl Into<Padding>,
) -> iced::widget::Button<'static, super::Message> {
    use iced::widget::{button, row, text};
    button(
        row![bi(icon).size(text_size), text(label).size(text_size)]
            .spacing(4)
            .align_y(Vertical::Center),
    )
    .padding(padding)
    .style(move |theme: &Theme, status| {
        let mut s = button::primary(theme, status);
        if active {
            s.border = iced::Border {
                radius: 4.0.into(),
                color: theme.extended_palette().primary.strong.color,
                width: 2.0,
            };
        }
        s
    })
}

// Floating dropdown popup container shared by Import and Export menus.
pub(crate) fn popup_menu<'a>(items: Vec<Element<'a, super::Message>>) -> Element<'a, super::Message> {
    use iced::widget::{column, container};
    container(column(items).spacing(2).padding([4, 4]))
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
        })
        .into()
}
