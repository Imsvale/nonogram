use iced::{Font, Theme};
use iced::widget::{button, container};
use iced_fonts::bootstrap::{self, Bootstrap};

pub(crate) const BOOTSTRAP_FONT: Font = Font::with_name("bootstrap-icons");
pub(crate) const FORWARD_FONT: Font = Font::with_name("bootstrap-forward");

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
