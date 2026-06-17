use iced::{
    advanced::{
        layout, renderer,
        widget::{self, Tree},
        Clipboard, Layout, Shell, Widget,
    },
    event, mouse, Background, Color, Element, Event, Length, Point, Rectangle, Size, Vector,
};

type Key = (usize, usize);

// ── Pan state stored in the widget tree ─────────────────────────────────────

#[derive(Debug)]
struct State {
    drag_start: Option<(Point, Vector)>, // (screen pos, offset) when drag started
    puzzle_key: Key,
}

impl Default for State {
    fn default() -> Self {
        Self { drag_start: None, puzzle_key: (usize::MAX, usize::MAX) }
    }
}

// ── PanViewport widget ───────────────────────────────────────────────────────

/// A viewport that clips its child content and allows drag-to-pan in any
/// direction. The child is laid out at its natural pixel size; the viewport
/// clips it to the available area and applies the pan offset via layout
/// translation (no renderer transform needed).
pub struct PanViewport<'a, Message, Theme = iced::Theme, Renderer = iced::Renderer> {
    content: Element<'a, Message, Theme, Renderer>,
    offset: Vector,
    puzzle_key: Key,
    on_pan: Option<Box<dyn Fn(Vector) -> Message + 'a>>,
}

impl<'a, Message, Theme, Renderer> PanViewport<'a, Message, Theme, Renderer> {
    pub fn new(
        puzzle_key: Key,
        content: impl Into<Element<'a, Message, Theme, Renderer>>,
        offset: Vector,
    ) -> Self {
        Self {
            content: content.into(),
            offset,
            puzzle_key,
            on_pan: None,
        }
    }

    /// Register a callback that fires with the new offset whenever the user pans.
    pub fn on_pan(mut self, f: impl Fn(Vector) -> Message + 'a) -> Self {
        self.on_pan = Some(Box::new(f));
        self
    }
}

// ── Widget impl ──────────────────────────────────────────────────────────────

impl<Message, Theme, Renderer> Widget<Message, Theme, Renderer>
    for PanViewport<'_, Message, Theme, Renderer>
where
    Renderer: iced::advanced::Renderer,
    Message: Clone,
{
    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Fill)
    }

    fn layout(
        &self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let viewport_size = limits.max();

        // Child gets its natural (unlimited) size so we can measure overflow.
        let child_limits = layout::Limits::new(Size::ZERO, Size::INFINITY);
        let child_node = self.content.as_widget().layout(
            &mut tree.children[0],
            renderer,
            &child_limits,
        );

        // Position the child at the pan offset within the viewport so that
        // event hit-testing naturally follows the visual position without
        // needing cursor adjustment.
        let positioned = child_node.translate(self.offset);

        layout::Node::with_children(viewport_size, vec![positioned])
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        let bounds = layout.bounds();
        let child_layout = layout.children().next().unwrap();

        // Clip child drawing to the viewport bounds.
        renderer.with_layer(bounds, |renderer| {
            self.content.as_widget().draw(
                &tree.children[0],
                renderer,
                theme,
                style,
                child_layout,
                cursor,
                viewport,
            );
        });

        // Scrollbar indicators when content overflows the viewport.
        draw_scrollbars(renderer, bounds, child_layout.bounds(), self.offset);
    }

    fn on_event(
        &mut self,
        tree: &mut Tree,
        event: Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) -> event::Status {
        let bounds = layout.bounds();
        let child_layout = layout.children().next().unwrap();

        // Child gets first priority (cell clicks, buttons, etc.).
        let child_status = self.content.as_widget_mut().on_event(
            &mut tree.children[0],
            event.clone(),
            child_layout,
            cursor,
            renderer,
            clipboard,
            shell,
            viewport,
        );

        // Handle pan gesture only when child did not capture the event.
        if child_status == event::Status::Ignored {
            let state = tree.state.downcast_mut::<State>();
            let content_size = child_layout.bounds().size();

            match event {
                Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left))
                    if cursor.is_over(bounds) =>
                {
                    if let Some(pos) = cursor.position() {
                        state.drag_start = Some((pos, self.offset));
                        return event::Status::Captured;
                    }
                }

                Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                    state.drag_start = None;
                    // Leave status as Ignored so child can also see the release.
                }

                Event::Mouse(mouse::Event::CursorMoved { position }) => {
                    if let Some((start_pos, start_offset)) = state.drag_start {
                        let delta = Vector::new(
                            position.x - start_pos.x,
                            position.y - start_pos.y,
                        );
                        let raw = Vector::new(
                            start_offset.x + delta.x,
                            start_offset.y + delta.y,
                        );
                        let new_offset =
                            clamp_offset(raw, bounds.size(), content_size);
                        if let Some(on_pan) = &self.on_pan {
                            shell.publish(on_pan(new_offset));
                        }
                        return event::Status::Captured;
                    }
                }

                _ => {}
            }
        }

        child_status
    }

    fn state(&self) -> widget::tree::State {
        widget::tree::State::new(State {
            drag_start: None,
            puzzle_key: self.puzzle_key,
        })
    }

    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.content)]
    }

    fn diff(&self, tree: &mut Tree) {
        // If the focused puzzle changed, discard any in-progress drag.
        {
            let state = tree.state.downcast_mut::<State>();
            if state.puzzle_key != self.puzzle_key {
                state.drag_start = None;
                state.puzzle_key = self.puzzle_key;
            }
        }
        tree.diff_children(std::slice::from_ref(&self.content));
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        let state = tree.state.downcast_ref::<State>();
        if state.drag_start.is_some() {
            return mouse::Interaction::Grabbing;
        }
        let child_layout = layout.children().next().unwrap();
        self.content.as_widget().mouse_interaction(
            &tree.children[0],
            child_layout,
            cursor,
            viewport,
            renderer,
        )
    }
}

// ── Element conversion ───────────────────────────────────────────────────────

impl<'a, Message, Theme, Renderer> From<PanViewport<'a, Message, Theme, Renderer>>
    for Element<'a, Message, Theme, Renderer>
where
    Renderer: iced::advanced::Renderer + 'a,
    Message: Clone + 'a,
    Theme: 'a,
{
    fn from(widget: PanViewport<'a, Message, Theme, Renderer>) -> Self {
        Element::new(widget)
    }
}

// ── Clamping ─────────────────────────────────────────────────────────────────

/// Clamp `offset` so the content stays strictly within the viewport.
///
/// - When content fits: offset stays in [0, vp - content], keeping it within bounds.
/// - When content overflows: offset stays in [vp - content, 0], allowing panning
///   to see all parts while never pushing either edge off-screen.
fn clamp_offset(offset: Vector, viewport: Size, content: Size) -> Vector {
    let clamp_axis = |o: f32, vp: f32, co: f32| -> f32 {
        if co <= vp {
            o.clamp(0.0, vp - co)
        } else {
            o.clamp(vp - co, 0.0)
        }
    };
    Vector::new(
        clamp_axis(offset.x, viewport.width, content.width),
        clamp_axis(offset.y, viewport.height, content.height),
    )
}

// ── Scrollbar indicators ─────────────────────────────────────────────────────

/// Draws thin translucent scrollbar indicators when content overflows the
/// viewport. The indicators are read-only (not interactive).
fn draw_scrollbars<Renderer: iced::advanced::Renderer>(
    renderer: &mut Renderer,
    bounds: Rectangle,  // viewport screen bounds
    content_bounds: Rectangle,  // child layout bounds (includes offset)
    offset: Vector,
) {
    const THICKNESS: f32 = 5.0;
    const MARGIN: f32 = 2.0;
    const TRACK: Color = Color { r: 0.5, g: 0.5, b: 0.5, a: 0.12 };
    const THUMB: Color = Color { r: 0.45, g: 0.48, b: 0.55, a: 0.55 };

    let content_w = content_bounds.width;
    let content_h = content_bounds.height;
    let view_w = bounds.width;
    let view_h = bounds.height;

    // Horizontal indicator
    if content_w > view_w + 1.0 {
        let track_x = bounds.x + MARGIN;
        let track_y = bounds.y + view_h - THICKNESS - MARGIN;
        let track_w = view_w - 2.0 * MARGIN;

        renderer.fill_quad(
            renderer::Quad {
                bounds: Rectangle { x: track_x, y: track_y, width: track_w, height: THICKNESS },
                border: Default::default(),
                shadow: Default::default(),
            },
            Background::Color(TRACK),
        );

        let thumb_w = (track_w * view_w / content_w).max(20.0).min(track_w);
        let scroll_x = (-offset.x).max(0.0);
        let max_scroll = (content_w - view_w).max(1.0);
        let thumb_x = track_x + (scroll_x / max_scroll) * (track_w - thumb_w);

        renderer.fill_quad(
            renderer::Quad {
                bounds: Rectangle { x: thumb_x, y: track_y, width: thumb_w, height: THICKNESS },
                border: iced::Border { radius: (THICKNESS / 2.0).into(), ..Default::default() },
                shadow: Default::default(),
            },
            Background::Color(THUMB),
        );
    }

    // Vertical indicator
    if content_h > view_h + 1.0 {
        let track_x = bounds.x + view_w - THICKNESS - MARGIN;
        let track_y = bounds.y + MARGIN;
        let track_h = view_h - 2.0 * MARGIN;

        renderer.fill_quad(
            renderer::Quad {
                bounds: Rectangle { x: track_x, y: track_y, width: THICKNESS, height: track_h },
                border: Default::default(),
                shadow: Default::default(),
            },
            Background::Color(TRACK),
        );

        let thumb_h = (track_h * view_h / content_h).max(20.0).min(track_h);
        let scroll_y = (-offset.y).max(0.0);
        let max_scroll = (content_h - view_h).max(1.0);
        let thumb_y = track_y + (scroll_y / max_scroll) * (track_h - thumb_h);

        renderer.fill_quad(
            renderer::Quad {
                bounds: Rectangle { x: track_x, y: thumb_y, width: THICKNESS, height: thumb_h },
                border: iced::Border { radius: (THICKNESS / 2.0).into(), ..Default::default() },
                shadow: Default::default(),
            },
            Background::Color(THUMB),
        );
    }
}
