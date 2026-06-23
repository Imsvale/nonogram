mod app;
mod convert;
mod pan_viewport;
mod solver;

use iced::{Point, Size, window};

fn main() -> iced::Result {
    let (width, height, pos, _was_maximized) = app::persistence::load_window_state();
    let position = match pos {
        Some((x, y)) => window::Position::Specific(Point::new(x, y)),
        None => window::Position::Default,
    };
    iced::application("Nonogram Solver", app::App::update, app::App::view)
        .window(window::Settings {
            size: Size::new(width, height),
            position,
            exit_on_close_request: false,
            ..window::Settings::default()
        })
        .font(iced_fonts::BOOTSTRAP_FONT_BYTES)
        .font(include_bytes!("../assets/fonts/bootstrap-forward.ttf").as_slice())
        .subscription(app::App::subscription)
        .theme(|app| app.theme())
        .run_with(app::App::new)
}
