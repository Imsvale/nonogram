mod app;
mod convert;
mod pan_viewport;
mod solver;

use iced::Size;

fn main() -> iced::Result {
    iced::application("Nonogram Solver", app::App::update, app::App::view)
        .window_size(Size::new(1200.0, 780.0))
        .font(iced_fonts::BOOTSTRAP_FONT_BYTES)
        .subscription(app::App::subscription)
        .theme(|app| app.theme())
        .run_with(app::App::new)
}
