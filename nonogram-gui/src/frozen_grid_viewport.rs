use std::sync::atomic::{AtomicBool, Ordering};

use iced::{
    advanced::{
        layout, renderer,
        widget::{self, Tree},
        Clipboard, Layout, Shell, Widget,
    },
    event, mouse, Background, Color, Element, Event, Length, Point, Rectangle, Size, Vector,
};

use crate::app::{Message as AppMessage, grid_view::GridRegions};

type Key = (usize, usize);

// ── Region enum ───────────────────────────────────────────────────────────────

/// Which logical region of the frozen grid the cursor is currently over.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FrozenRegion {
    Cells,          // main puzzle cells
    RowAxis(usize), // row_clues or row_sums — carries the row index at cursor y
    ColAxis(usize), // col_clues or col_sums — carries the col index at cursor x
    Other,          // corner, sum corners, bottom_right, controls, nav
}

// ── Debug dump ────────────────────────────────────────────────────────────────

static DEBUG_REQUESTED: AtomicBool = AtomicBool::new(false);

pub(crate) fn request_debug_dump() {
    DEBUG_REQUESTED.store(true, Ordering::Relaxed);
}

// ── State ─────────────────────────────────────────────────────────────────────

#[derive(Debug)]
struct State {
    drag_start: Option<(Point, Vector)>,
    puzzle_key: Key,
}

impl Default for State {
    fn default() -> Self {
        Self { drag_start: None, puzzle_key: (usize::MAX, usize::MAX) }
    }
}

// ── Child index constants ──────────────────────────────────────────────────────

const IDX_CORNER:       usize = 0;
const IDX_COL_CLUES:    usize = 1;
const IDX_TOP_RIGHT:    usize = 2;
const IDX_ROW_CLUES:    usize = 3;
const IDX_CELLS:        usize = 4;
const IDX_ROW_SUMS:     usize = 5;
const IDX_COL_SUMS:     usize = 6;
const IDX_BOTTOM_LEFT:  usize = 7;
const IDX_BOTTOM_RIGHT: usize = 8;
const IDX_CONTROLS:     usize = 9;
const IDX_NAV:          usize = 10;
const N_CHILDREN:       usize = 11;

// Gap between col_sums bottom and controls top, and between controls and nav.
const BTN_GAP_TOP: f32 = 4.0;
const BTN_GAP_MID: f32 = 2.0;

// ── FrozenGridViewport widget ─────────────────────────────────────────────────

pub struct FrozenGridViewport<'a, Message, Theme = iced::Theme, Renderer = iced::Renderer> {
    corner:       Element<'a, Message, Theme, Renderer>,
    col_clues:    Element<'a, Message, Theme, Renderer>,
    top_right:    Element<'a, Message, Theme, Renderer>,
    row_clues:    Element<'a, Message, Theme, Renderer>,
    cells:        Element<'a, Message, Theme, Renderer>,
    row_sums:     Element<'a, Message, Theme, Renderer>,
    col_sums:     Element<'a, Message, Theme, Renderer>,
    bottom_left:  Element<'a, Message, Theme, Renderer>,
    bottom_right: Element<'a, Message, Theme, Renderer>,
    controls:     Element<'a, Message, Theme, Renderer>,
    nav:          Element<'a, Message, Theme, Renderer>,

    corner_w: f32,
    corner_h: f32,
    sum_w:    f32,
    sum_h:    f32,

    puzzle_w:   usize,
    puzzle_h:   usize,
    key:        Key,
    pan_offset: Vector,
    on_pan:     Option<Box<dyn Fn(Vector) -> Message + 'a>>,
    on_region:  Option<Box<dyn Fn(FrozenRegion) -> Message + 'a>>,
    bg:         Color,
}

impl<'a> FrozenGridViewport<'a, AppMessage> {
    pub(crate) fn from_regions(r: GridRegions<'a>, key: Key, pan_offset: Vector, bg: Color) -> Self {
        let GridRegions {
            corner, col_clues, top_right, row_clues, cells,
            row_sums, col_sums, bottom_left, bottom_right,
            controls, nav,
            corner_w, corner_h, sum_w, sum_h,
            puzzle_w, puzzle_h,
        } = r;
        Self {
            corner, col_clues, top_right, row_clues, cells,
            row_sums, col_sums, bottom_left, bottom_right,
            controls, nav,
            corner_w, corner_h, sum_w, sum_h,
            puzzle_w, puzzle_h,
            key, pan_offset, on_pan: None, on_region: None, bg,
        }
    }

    pub fn on_pan(mut self, f: impl Fn(Vector) -> AppMessage + 'a) -> Self {
        self.on_pan = Some(Box::new(f));
        self
    }

    pub fn on_region(mut self, f: impl Fn(FrozenRegion) -> AppMessage + 'a) -> Self {
        self.on_region = Some(Box::new(f));
        self
    }
}

// ── Layout computation ────────────────────────────────────────────────────────
//
// Sign convention: px > 0 means content shifted right (seeing left side of a
// smaller-than-viewport puzzle). px < 0 means seeing the right side.
//
// corner tracks (px.max(0), py.max(0)) so it never floats left of the widget
// origin when panning into negative territory.
//
// col_sums_y is clamped to leave exactly btn_gap + controls_h + btn_gap_mid +
// nav_h pixels below it before the viewport bottom.  This guarantees the
// button rows always fit in view.
//
// Controls and nav span the full puzzle width (corner + cells + sums) and are
// anchored at corner_x, mirroring where the row_clues start.

fn compute_offsets(
    vp: Size,
    px: f32, py: f32,
    cw: f32, ch: f32,
    sw: f32, sh: f32,
    cells_natural_w: f32,
    cells_natural_h: f32,
    controls_h: f32,
    nav_h: f32,
) -> [(f32, f32); N_CHILDREN] {
    let corner_x = px.max(0.0);
    let corner_y = py.max(0.0);

    let row_sums_x = (cw + px + cells_natural_w)
        .min(vp.width - sw)
        .max(corner_x + cw);

    let btn_total_h = controls_h + BTN_GAP_MID + nav_h + BTN_GAP_TOP;
    let col_sums_y = (ch + py + cells_natural_h)
        .min(vp.height - sh - btn_total_h)
        .max(corner_y + ch);

    let controls_y = col_sums_y + sh + BTN_GAP_TOP;
    let nav_y      = controls_y + controls_h + BTN_GAP_MID;

    [
        (corner_x,    corner_y),    // IDX_CORNER
        (cw + px,     corner_y),    // IDX_COL_CLUES
        (row_sums_x,  corner_y),    // IDX_TOP_RIGHT
        (corner_x,    ch + py),     // IDX_ROW_CLUES
        (cw + px,     ch + py),     // IDX_CELLS
        (row_sums_x,  ch + py),     // IDX_ROW_SUMS
        (cw + px,     col_sums_y),  // IDX_COL_SUMS
        (corner_x,    col_sums_y),  // IDX_BOTTOM_LEFT
        (row_sums_x,  col_sums_y),  // IDX_BOTTOM_RIGHT
        (corner_x,    controls_y),  // IDX_CONTROLS — full width, starts at corner
        (corner_x,    nav_y),       // IDX_NAV       — ditto
    ]
}

// Derive clip rectangles from actual child screen-space layout positions.
// This is more robust than using self.corner_w/corner_h directly because it
// stays correct even when the layout was computed for a different puzzle on
// the same frame (size mismatch would cause black strips otherwise).
fn clips_from_layouts(
    child_layouts: &[Layout<'_>],
    sw: f32,
) -> [Rectangle; N_CHILDREN] {
    let c_b   = child_layouts[IDX_CORNER].bounds();
    let rs_b  = child_layouts[IDX_ROW_SUMS].bounds();
    let cs_b  = child_layouts[IDX_COL_SUMS].bounds();
    let ctl_b = child_layouts[IDX_CONTROLS].bounds();
    let nav_b = child_layouts[IDX_NAV].bounds();

    let corner_sx  = c_b.x;
    let corner_sy  = c_b.y;
    let cw         = c_b.width;
    let ch         = c_b.height;
    let row_sums_sx = rs_b.x;
    let col_sums_sy = cs_b.y;

    let mid_w   = (row_sums_sx - corner_sx - cw).max(0.0);
    let mid_h   = (col_sums_sy - corner_sy - ch).max(0.0);
    let btn_w   = (row_sums_sx + sw - corner_sx).max(0.0);   // spans corner → right of sums
    let sum_h   = cs_b.height;

    [
        Rectangle { x: corner_sx,      y: corner_sy,      width: cw,    height: ch      }, // corner
        Rectangle { x: corner_sx + cw, y: corner_sy,      width: mid_w, height: ch      }, // col_clues
        Rectangle { x: row_sums_sx,    y: corner_sy,      width: sw,    height: ch      }, // top_right
        Rectangle { x: corner_sx,      y: corner_sy + ch, width: cw,    height: mid_h   }, // row_clues
        Rectangle { x: corner_sx + cw, y: corner_sy + ch, width: mid_w, height: mid_h   }, // cells
        Rectangle { x: row_sums_sx,    y: corner_sy + ch, width: sw,    height: mid_h   }, // row_sums
        Rectangle { x: corner_sx + cw, y: col_sums_sy,    width: mid_w, height: sum_h   }, // col_sums
        Rectangle { x: corner_sx,      y: col_sums_sy,    width: cw,    height: sum_h   }, // bottom_left
        Rectangle { x: row_sums_sx,    y: col_sums_sy,    width: sw,    height: sum_h   }, // bottom_right
        Rectangle { x: corner_sx,      y: ctl_b.y,        width: btn_w, height: ctl_b.height.max(0.0) }, // controls
        Rectangle { x: corner_sx,      y: nav_b.y,        width: btn_w, height: nav_b.height.max(0.0) }, // nav
    ]
}

// ── Clamping ──────────────────────────────────────────────────────────────────

fn clamp_axis(o: f32, vp: f32, co: f32) -> f32 {
    if co <= vp { o.clamp(0.0, vp - co) } else { o.clamp(vp - co, 0.0) }
}

// ── Widget impl ───────────────────────────────────────────────────────────────

impl<Message, Theme, Renderer> Widget<Message, Theme, Renderer>
    for FrozenGridViewport<'_, Message, Theme, Renderer>
where
    Renderer: iced::advanced::Renderer,
    Message: Clone,
{
    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Fill)
    }

    fn children(&self) -> Vec<Tree> {
        [
            &self.corner, &self.col_clues, &self.top_right,
            &self.row_clues, &self.cells, &self.row_sums,
            &self.col_sums, &self.bottom_left, &self.bottom_right,
            &self.controls, &self.nav,
        ]
        .iter()
        .map(|c| Tree::new(*c))
        .collect()
    }

    fn diff(&self, tree: &mut Tree) {
        {
            let state = tree.state.downcast_mut::<State>();
            if state.puzzle_key != self.key {
                state.drag_start = None;
                state.puzzle_key = self.key;
            }
        }
        let children: [&Element<'_, Message, Theme, Renderer>; N_CHILDREN] = [
            &self.corner, &self.col_clues, &self.top_right,
            &self.row_clues, &self.cells, &self.row_sums,
            &self.col_sums, &self.bottom_left, &self.bottom_right,
            &self.controls, &self.nav,
        ];
        tree.diff_children(&children);
    }

    fn state(&self) -> widget::tree::State {
        widget::tree::State::new(State {
            drag_start: None,
            puzzle_key: self.key,
        })
    }

    fn layout(
        &self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let vp = limits.max();
        let inf = layout::Limits::new(Size::ZERO, Size::INFINITY);

        // ── Pass 1: lay out the 9 grid regions unconstrained ─────────────────
        let grid_refs: [&Element<'_, Message, Theme, Renderer>; 9] = [
            &self.corner, &self.col_clues, &self.top_right,
            &self.row_clues, &self.cells, &self.row_sums,
            &self.col_sums, &self.bottom_left, &self.bottom_right,
        ];
        let mut child_nodes: Vec<layout::Node> = grid_refs
            .iter()
            .zip(tree.children[..9].iter_mut())
            .map(|(child, ct)| child.as_widget().layout(ct, renderer, &inf))
            .collect();

        let cells_natural_w = child_nodes[IDX_CELLS].bounds().width;
        let cells_natural_h = child_nodes[IDX_CELLS].bounds().height;

        // Full button width = corner + cells + sums, independent of px.
        let corner_x_local  = self.pan_offset.x.max(0.0);
        let row_sums_x_local = (self.corner_w + self.pan_offset.x + cells_natural_w)
            .min(vp.width  - self.sum_w)
            .max(corner_x_local + self.corner_w);
        let full_btn_w = (row_sums_x_local + self.sum_w - corner_x_local).max(1.0);

        // ── Pass 2: lay out controls/nav at full puzzle width ─────────────────
        let btn_limits = layout::Limits::new(
            Size::ZERO,
            Size::new(full_btn_w, f32::INFINITY),
        );
        let controls_node = self.controls.as_widget()
            .layout(&mut tree.children[IDX_CONTROLS], renderer, &btn_limits);
        let controls_h = controls_node.bounds().height;
        let nav_node = self.nav.as_widget()
            .layout(&mut tree.children[IDX_NAV], renderer, &btn_limits);
        let nav_h = nav_node.bounds().height;

        child_nodes.push(controls_node);
        child_nodes.push(nav_node);

        // ── Apply offsets ─────────────────────────────────────────────────────
        let offsets = compute_offsets(
            vp,
            self.pan_offset.x, self.pan_offset.y,
            self.corner_w, self.corner_h,
            self.sum_w, self.sum_h,
            cells_natural_w, cells_natural_h,
            controls_h, nav_h,
        );
        for (node, (ox, oy)) in child_nodes.iter_mut().zip(offsets.iter()) {
            *node = node.clone().translate(Vector::new(*ox, *oy));
        }

        layout::Node::with_children(vp, child_nodes)
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
        let child_layouts: Vec<Layout<'_>> = layout.children().collect();
        let clips = clips_from_layouts(&child_layouts, self.sum_w);

        let children_ref: [&Element<'_, Message, Theme, Renderer>; N_CHILDREN] = [
            &self.corner, &self.col_clues, &self.top_right,
            &self.row_clues, &self.cells, &self.row_sums,
            &self.col_sums, &self.bottom_left, &self.bottom_right,
            &self.controls, &self.nav,
        ];

        // All drawing happens inside a single outer compositing layer so that
        // cells and frozen headers share one compositing context. Without this,
        // iced's wgpu backend defers inner with_layer draws to end-of-frame,
        // causing the cells layer to composite on top of frozen headers even
        // though frozen headers are drawn last in the loop — making the
        // crosshair (which lives in the cells layer) visible outside the
        // puzzle grid area, under the clue strips.
        renderer.with_layer(bounds, |renderer| {
            // Snap all rendering to integer pixel boundaries.  bounds.y (and .x)
            // are fractional because font-measured heights in the toolbar cascade
            // a sub-pixel offset down through iced's f32 layout tree.  Applying
            // the negative fractional part as a translation makes every border,
            // cell background, and separator land on an exact pixel row/column.
            let snap = Vector::new(-bounds.x.fract(), -bounds.y.fract());
            renderer.with_translation(snap, |renderer| {
                renderer.fill_quad(
                    renderer::Quad { bounds, border: Default::default(), shadow: Default::default() },
                    Background::Color(self.bg),
                );

                // Back-to-front order: cells behind frozen headers.
                let draw_order = [
                    IDX_CELLS,
                    IDX_ROW_SUMS, IDX_COL_SUMS,
                    IDX_CONTROLS, IDX_NAV,
                    IDX_ROW_CLUES, IDX_COL_CLUES,
                    IDX_BOTTOM_RIGHT, IDX_BOTTOM_LEFT, IDX_TOP_RIGHT,
                    IDX_CORNER,
                ];

                for &idx in &draw_order {
                    let clip    = clips[idx];
                    let child_b = child_layouts[idx].bounds();
                    if clip.width < 0.5 || clip.height < 0.5 { continue; }

                    let overflows = child_b.x < clip.x - 0.5
                        || child_b.y < clip.y - 0.5
                        || child_b.x + child_b.width  > clip.x + clip.width  + 0.5
                        || child_b.y + child_b.height > clip.y + clip.height + 0.5;

                    if overflows {
                        renderer.with_layer(clip, |r| {
                            r.fill_quad(
                                renderer::Quad { bounds: clip, border: Default::default(), shadow: Default::default() },
                                Background::Color(self.bg),
                            );
                            children_ref[idx].as_widget().draw(
                                &tree.children[idx], r, theme, style,
                                child_layouts[idx], cursor, viewport,
                            );
                        });
                    } else {
                        children_ref[idx].as_widget().draw(
                            &tree.children[idx], renderer, theme, style,
                            child_layouts[idx], cursor, viewport,
                        );
                    }
                }

                // Overlay crisp 2 px separator quads to cover sub-pixel bleed from
                // minimap cell rendering.  Dimensions are constrained to the actual
                // grid content (corner → top_right), NOT the full widget size.
                let sep = Color::from_rgb(0.28, 0.32, 0.44);
                let corner_b    = child_layouts[IDX_CORNER].bounds();
                let top_right_b = child_layouts[IDX_TOP_RIGHT].bounds();
                // Horizontal: from corner left edge to top_right right edge.
                renderer.fill_quad(
                    renderer::Quad {
                        bounds: Rectangle {
                            x:      corner_b.x,
                            y:      corner_b.y + corner_b.height - 2.0,
                            width:  top_right_b.x + top_right_b.width - corner_b.x,
                            height: 2.0,
                        },
                        border: Default::default(),
                        shadow: Default::default(),
                    },
                    Background::Color(sep),
                );
                // Vertical: right edge of corner/row-clue strip, header height only.
                renderer.fill_quad(
                    renderer::Quad {
                        bounds: Rectangle {
                            x:      corner_b.x + corner_b.width - 2.0,
                            y:      corner_b.y,
                            width:  2.0,
                            height: corner_b.height,
                        },
                        border: Default::default(),
                        shadow: Default::default(),
                    },
                    Background::Color(sep),
                );

                draw_scrollbars(
                    renderer,
                    clips[IDX_CELLS],
                    child_layouts[IDX_CELLS].bounds(),
                    self.pan_offset,
                );
            });
        });

        // Debug snapshot — printed once when requested, then cleared.
        if DEBUG_REQUESTED.swap(false, Ordering::Relaxed) {
            let c_b = child_layouts[IDX_CORNER].bounds();
            let rs_b = child_layouts[IDX_ROW_SUMS].bounds();
            let cs_b = child_layouts[IDX_COL_SUMS].bounds();
            eprintln!("=== GRID DEBUG SNAPSHOT ===");
            eprintln!("  widget bounds    : {:?}", bounds);
            eprintln!("  pan_offset       : ({:.1}, {:.1})", self.pan_offset.x, self.pan_offset.y);
            eprintln!("  corner_w/h       : {:.0} / {:.0}", self.corner_w, self.corner_h);
            eprintln!("  sum_w/h          : {:.0} / {:.0}", self.sum_w, self.sum_h);
            eprintln!("  cells natural    : {:.0} × {:.0}",
                child_layouts[IDX_CELLS].bounds().width,
                child_layouts[IDX_CELLS].bounds().height);
            eprintln!("  anchors (screen) : corner=({:.1},{:.1}) cw={:.0} ch={:.0} row_sums_sx={:.1} col_sums_sy={:.1}",
                c_b.x, c_b.y, c_b.width, c_b.height, rs_b.x, cs_b.y);
            eprintln!("  mid_w            : {:.1}", (rs_b.x - c_b.x - c_b.width).max(0.0));
            let labels = ["corner","col_clues","top_right","row_clues","cells",
                          "row_sums","col_sums","bot_left","bot_right","controls","nav"];
            for (i, (label, cl)) in labels.iter().zip(child_layouts.iter()).enumerate() {
                let b = cl.bounds();
                let c = clips[i];
                let ov = b.width > c.width + 0.5 || b.height > c.height + 0.5;
                eprintln!("  [{i:>2}] {label:<12}: layout=({:.1},{:.1} {:.1}×{:.1})  clip=({:.1},{:.1} {:.1}×{:.1}){}",
                    b.x, b.y, b.width, b.height,
                    c.x, c.y, c.width, c.height,
                    if ov { " CLIPPED" } else { "" });
            }
            eprintln!("===========================");
        }
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
        let child_layouts: Vec<Layout<'_>> = layout.children().collect();
        let clips = clips_from_layouts(&child_layouts, self.sum_w);

        // Front-to-back dispatch — interactive overlays first.
        let dispatch_order = [
            IDX_CORNER, IDX_TOP_RIGHT, IDX_BOTTOM_LEFT, IDX_BOTTOM_RIGHT,
            IDX_COL_CLUES, IDX_ROW_CLUES, IDX_ROW_SUMS, IDX_COL_SUMS,
            IDX_CONTROLS, IDX_NAV,
            IDX_CELLS,
        ];

        let mut captured = event::Status::Ignored;
        for &idx in &dispatch_order {
            if captured == event::Status::Captured { break; }
            let clip = clips[idx];
            let masked_cursor = match cursor.position() {
                Some(pos)
                    if clip.width > 0.0 && clip.height > 0.0 && clip.contains(pos)
                    => cursor,
                _ => mouse::Cursor::Unavailable,
            };
            let child_widget = match idx {
                IDX_CORNER       => self.corner.as_widget_mut(),
                IDX_COL_CLUES    => self.col_clues.as_widget_mut(),
                IDX_TOP_RIGHT    => self.top_right.as_widget_mut(),
                IDX_ROW_CLUES    => self.row_clues.as_widget_mut(),
                IDX_CELLS        => self.cells.as_widget_mut(),
                IDX_ROW_SUMS     => self.row_sums.as_widget_mut(),
                IDX_COL_SUMS     => self.col_sums.as_widget_mut(),
                IDX_BOTTOM_LEFT  => self.bottom_left.as_widget_mut(),
                IDX_BOTTOM_RIGHT => self.bottom_right.as_widget_mut(),
                IDX_CONTROLS     => self.controls.as_widget_mut(),
                IDX_NAV          => self.nav.as_widget_mut(),
                _                => unreachable!(),
            };
            let status = child_widget.on_event(
                &mut tree.children[idx],
                event.clone(),
                child_layouts[idx],
                masked_cursor,
                renderer, clipboard, shell, viewport,
            );
            if status == event::Status::Captured {
                captured = event::Status::Captured;
            }
        }

        // Pan gesture — only when no child captured.
        if captured == event::Status::Ignored {
            let state = tree.state.downcast_mut::<State>();
            let cells_natural = child_layouts[IDX_CELLS].bounds().size();
            let vp = bounds.size();

            // Compute how much vertical space the button rows consume so the
            // pan clamp stops short of scrolling cells behind the buttons.
            let controls_h  = child_layouts[IDX_CONTROLS].bounds().height;
            let nav_h       = child_layouts[IDX_NAV].bounds().height;
            let btn_total_h = controls_h + BTN_GAP_MID + nav_h + BTN_GAP_TOP;

            let vp_cells = Size::new(
                (vp.width  - self.corner_w - self.sum_w).max(0.0),
                (vp.height - self.corner_h - self.sum_h - btn_total_h).max(0.0),
            );

            match event {
                Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left))
                    if cursor.is_over(bounds) =>
                {
                    if let Some(pos) = cursor.position() {
                        state.drag_start = Some((pos, self.pan_offset));
                        return event::Status::Captured;
                    }
                }

                Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                    state.drag_start = None;
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
                        let new_offset = Vector::new(
                            clamp_axis(raw.x, vp_cells.width,  cells_natural.width),
                            clamp_axis(raw.y, vp_cells.height, cells_natural.height),
                        );
                        if let Some(on_pan) = &self.on_pan {
                            shell.publish(on_pan(new_offset));
                        }
                        return event::Status::Captured;
                    }
                }

                _ => {}
            }
        }

        // Emit which region the cursor is in on every CursorMoved, regardless
        // of whether a child captured the event.  Used to drive per-axis
        // crosshair visibility and position without scattering mouse_area wrappers.
        if let (Event::Mouse(mouse::Event::CursorMoved { position }), Some(on_region_fn)) =
            (&event, &self.on_region)
        {
            if bounds.contains(*position) {
                let pos = *position;
                // Derive row/col from cursor position relative to the cells child layout.
                // cells_b.height = puzzle_h * C + 2 (the +2 is the bottom border solid).
                let cells_b = child_layouts[IDX_CELLS].bounds();
                let cell_h = if self.puzzle_h > 0 {
                    (cells_b.height - 2.0) / self.puzzle_h as f32
                } else { 26.0 };
                let cell_w = if self.puzzle_w > 0 {
                    (cells_b.width - 2.0) / self.puzzle_w as f32
                } else { 26.0 };
                let row = ((pos.y - cells_b.y) / cell_h)
                    .floor()
                    .clamp(0.0, self.puzzle_h.saturating_sub(1) as f32) as usize;
                let col = ((pos.x - cells_b.x) / cell_w)
                    .floor()
                    .clamp(0.0, self.puzzle_w.saturating_sub(1) as f32) as usize;
                let region = if clips[IDX_CELLS].contains(pos) {
                    FrozenRegion::Cells
                } else if clips[IDX_ROW_CLUES].contains(pos)
                    || clips[IDX_ROW_SUMS].contains(pos)
                {
                    FrozenRegion::RowAxis(row)
                } else if clips[IDX_COL_CLUES].contains(pos)
                    || clips[IDX_COL_SUMS].contains(pos)
                {
                    FrozenRegion::ColAxis(col)
                } else {
                    FrozenRegion::Other
                };
                shell.publish(on_region_fn(region));
            }
        }

        captured
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
        let child_layouts: Vec<Layout<'_>> = layout.children().collect();
        let clips = clips_from_layouts(&child_layouts, self.sum_w);

        let children_ref: [&Element<'_, Message, Theme, Renderer>; N_CHILDREN] = [
            &self.corner, &self.col_clues, &self.top_right,
            &self.row_clues, &self.cells, &self.row_sums,
            &self.col_sums, &self.bottom_left, &self.bottom_right,
            &self.controls, &self.nav,
        ];

        for (idx, child) in children_ref.iter().enumerate() {
            let clip = clips[idx];
            let masked_cursor = match cursor.position() {
                Some(pos)
                    if clip.width > 0.0 && clip.height > 0.0 && clip.contains(pos)
                    => cursor,
                _ => mouse::Cursor::Unavailable,
            };
            let interaction = child.as_widget().mouse_interaction(
                &tree.children[idx], child_layouts[idx],
                masked_cursor, viewport, renderer,
            );
            if interaction != mouse::Interaction::None {
                return interaction;
            }
        }
        mouse::Interaction::default()
    }
}

// ── Element conversion ────────────────────────────────────────────────────────

impl<'a, Message, Theme, Renderer> From<FrozenGridViewport<'a, Message, Theme, Renderer>>
    for Element<'a, Message, Theme, Renderer>
where
    Renderer: iced::advanced::Renderer + 'a,
    Message: Clone + 'a,
    Theme: 'a,
{
    fn from(w: FrozenGridViewport<'a, Message, Theme, Renderer>) -> Self {
        Element::new(w)
    }
}

// ── Scrollbar indicators ──────────────────────────────────────────────────────

fn draw_scrollbars<Renderer: iced::advanced::Renderer>(
    renderer: &mut Renderer,
    cells_clip: Rectangle,
    cells_bounds: Rectangle,
    offset: Vector,
) {
    const THICKNESS: f32 = 5.0;
    const MARGIN:    f32 = 2.0;
    const TRACK: Color = Color { r: 0.5, g: 0.5, b: 0.5, a: 0.12 };
    const THUMB: Color = Color { r: 0.45, g: 0.48, b: 0.55, a: 0.55 };

    let content_w = cells_bounds.width;
    let content_h = cells_bounds.height;
    let view_w    = cells_clip.width;
    let view_h    = cells_clip.height;

    if content_w > view_w + 1.0 {
        let track_x = cells_clip.x + MARGIN;
        let track_y = cells_clip.y + view_h - THICKNESS - MARGIN;
        let track_w = view_w - 2.0 * MARGIN;
        renderer.fill_quad(
            renderer::Quad { bounds: Rectangle { x: track_x, y: track_y, width: track_w, height: THICKNESS },
                border: Default::default(), shadow: Default::default() },
            Background::Color(TRACK),
        );
        let thumb_w    = (track_w * view_w / content_w).max(20.0).min(track_w);
        let scroll_x   = (-offset.x).max(0.0);
        let max_scroll = (content_w - view_w).max(1.0);
        let thumb_x    = track_x + (scroll_x / max_scroll) * (track_w - thumb_w);
        renderer.fill_quad(
            renderer::Quad { bounds: Rectangle { x: thumb_x, y: track_y, width: thumb_w, height: THICKNESS },
                border: iced::Border { radius: (THICKNESS / 2.0).into(), ..Default::default() },
                shadow: Default::default() },
            Background::Color(THUMB),
        );
    }

    if content_h > view_h + 1.0 {
        let track_x = cells_clip.x + view_w - THICKNESS - MARGIN;
        let track_y = cells_clip.y + MARGIN;
        let track_h = view_h - 2.0 * MARGIN;
        renderer.fill_quad(
            renderer::Quad { bounds: Rectangle { x: track_x, y: track_y, width: THICKNESS, height: track_h },
                border: Default::default(), shadow: Default::default() },
            Background::Color(TRACK),
        );
        let thumb_h    = (track_h * view_h / content_h).max(20.0).min(track_h);
        let scroll_y   = (-offset.y).max(0.0);
        let max_scroll = (content_h - view_h).max(1.0);
        let thumb_y    = track_y + (scroll_y / max_scroll) * (track_h - thumb_h);
        renderer.fill_quad(
            renderer::Quad { bounds: Rectangle { x: track_x, y: thumb_y, width: THICKNESS, height: thumb_h },
                border: iced::Border { radius: (THICKNESS / 2.0).into(), ..Default::default() },
                shadow: Default::default() },
            Background::Color(THUMB),
        );
    }
}
