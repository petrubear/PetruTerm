// Paints one `term::Terminal`'s grid as a custom gpui `Element`. gpui's
// native text system doesn't produce ligatures at this gpui version, so the
// grid is shaped + rasterized with cosmic-text (`rasterize/`) into an RGBA
// bitmap and painted via `Window::paint_image`.

use std::rc::Rc;

use alacritty_terminal::vte::ansi::CursorShape;

use crate::ui::search_bar::SearchMatch;
use gpui::{
    fill, point, px, relative, size, App, Bounds, Corners, Element, ElementId, GlobalElementId,
    InspectorElementId, IntoElement, LayoutId, Pixels, Style, Window,
};

use crate::term::Terminal;

use super::{context_menu, mouse, mouse::OnFocusCallback, rasterize};

pub struct TerminalGridElement {
    pub terminal: Rc<Terminal>,
    pub cell_width: Pixels,
    pub cell_height: Pixels,
    pub colors: crate::config::schema::ColorScheme,
    /// Window is translucent (`opacity < 1` or blur): skip the opaque base
    /// fill so the root div's translucent background shows through instead
    /// of stacking a second layer of alpha on top of it.
    pub translucent: bool,
    pub is_active: bool,
    pub cursor_blink_on: bool,
    pub on_focus: OnFocusCallback,
    /// Active matches for the currently-focused pane's search, plus which
    /// index is "current" -- `None` for every pane except the focused one
    /// (search always targets the focused terminal only, matching the
    /// wgpu build's own `Mux::focused_terminal_id()` scoping). Threaded
    /// through to `rasterize::rasterize_grid`'s own `search` parameter.
    pub search: Option<(Vec<SearchMatch>, usize)>,
    /// Opens the context menu at a right-click's position over this pane.
    /// See `context_menu.rs`'s own doc comment on `RightClickCallback` for
    /// why this carries no terminal id.
    pub on_right_click: context_menu::RightClickCallback,
}

impl IntoElement for TerminalGridElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for TerminalGridElement {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        // Fill whatever the parent gives us, rather than asking for
        // `cols x rows` cells' worth of pixels. The pane tree makes the
        // *layout* the authority on a pane's size and resizes the terminal to
        // match (`pane_view::fit_terminal`, driven from the wrapping div's
        // `on_children_prepainted`). Requesting a relative
        // size also means the bounds recorded in `RectCache` for this leaf
        // are the pane's full rect, not a smaller grid rect floating inside
        // it, so hit-testing and `focus_dir`'s geometry agree with what the
        // user sees.
        let mut style = Style::default();
        style.size.width = relative(1.0).into();
        style.size.height = relative(1.0).into();
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        _window: &mut Window,
        _cx: &mut App,
    ) -> Self::PrepaintState {
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        _prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        _cx: &mut App,
    ) {
        // Background. `rasterize::rasterize_grid` skips filling any cell whose
        // resolved background equals `colors.background` on the assumption
        // that this base fill already covers it, so the two must agree.
        if !self.translucent {
            let [r, g, b, a] = self.colors.background;
            window.paint_quad(fill(bounds, gpui::Rgba { r, g, b, a }));
        }

        mouse::register_mouse_handlers(
            self.terminal.clone(),
            bounds,
            self.cell_width,
            self.cell_height,
            self.on_focus.clone(),
            window,
        );

        context_menu::register_right_click(
            bounds,
            self.cell_width,
            self.cell_height,
            self.terminal.cols.get() as usize,
            self.terminal.rows.get() as usize,
            self.on_right_click.clone(),
            window,
        );

        let search_ref = self
            .search
            .as_ref()
            .map(|(matches, current)| (matches.as_slice(), *current));
        if let Some(render_image) = rasterize::rasterize_grid(
            &self.terminal,
            self.cell_width,
            self.cell_height,
            window.scale_factor(),
            &self.colors,
            window,
            search_ref,
        ) {
            // `bounds` is the pane's FULL layout rect (the flex tree
            // sizes this element via `relative(1.0)`, not a fixed
            // `cell_width * cols`), but `rasterize_grid`'s bitmap is still
            // sized to exactly `cell_width * cols` x `cell_height * rows`
            // (`pane_view::fit_terminal` floors the pane's rect to a whole
            // cell count before resizing the PTY, so the pane's actual
            // pixel size is only ever >= the grid's own size, by less than
            // one cell in each axis). Painting the bitmap into the FULL
            // `bounds` would have gpui stretch it to fill that leftover
            // fractional-cell strip (blurred/smeared glyph edges).
            // Clamp the destination rect to the bitmap's own native size
            // instead; the leftover strip (at most one cell wide/tall)
            // stays the plain background already painted above.
            let cols = f32::from(self.terminal.cols.get());
            let rows = f32::from(self.terminal.rows.get());
            let image_bounds = Bounds {
                origin: bounds.origin,
                size: size(
                    (self.cell_width * cols).min(bounds.size.width),
                    (self.cell_height * rows).min(bounds.size.height),
                ),
            };
            let _ = window.paint_image(image_bounds, Corners::default(), render_image, 0, false);
        }

        // Cursor. Shape from Terminal::cursor_info() (DECSCUSR / default),
        // geometry ported from src/app/renderer/terminal.rs's
        // build_cursor_overlay. Only the focused pane draws a cursor,
        // matching the wgpu renderer.
        let cursor = self.terminal.cursor_info();
        if self.is_active && cursor.visible && self.cursor_blink_on {
            let shape = cursor.shape;
            let cell_w = self.cell_width;
            let cell_h = self.cell_height;
            let cursor_origin = point(
                bounds.origin.x + cell_w * (cursor.col as f32),
                bounds.origin.y + cell_h * (cursor.row as f32),
            );
            // `cursor.visible` (checked above) is false whenever
            // `Terminal::cursor_info` reports `Hidden`, so this arm is
            // unreachable today -- `None` here rather than an early
            // `return` from `paint()` anyway, since a bare `return` from
            // the middle of a growing `paint()` is a trap for whoever adds
            // code after the cursor block next (the scrollbar below
            // already almost was that code).
            let geom = match shape {
                CursorShape::Block | CursorShape::HollowBlock => {
                    Some((point(px(0.0), px(0.0)), size(cell_w, cell_h)))
                }
                CursorShape::Underline => Some((
                    point(px(0.0), (cell_h - px(2.0)).max(px(0.0))),
                    size(cell_w, px(2.0)),
                )),
                CursorShape::Beam => Some((point(px(0.0), px(0.0)), size(px(2.0), cell_h))),
                CursorShape::Hidden => None,
            };
            if let Some((offset, geom_size)) = geom {
                let quad_bounds = Bounds {
                    origin: point(cursor_origin.x + offset.x, cursor_origin.y + offset.y),
                    size: geom_size,
                };
                if shape == CursorShape::HollowBlock {
                    // Outline only -- four thin edge rects, not a filled quad,
                    // so the cell's own content stays visible underneath.
                    let t = px(1.0);
                    let color = gpui::rgba(0xf8f8f2ff);
                    window.paint_quad(fill(
                        Bounds {
                            origin: quad_bounds.origin,
                            size: size(geom_size.width, t),
                        },
                        color,
                    ));
                    window.paint_quad(fill(
                        Bounds {
                            origin: point(
                                quad_bounds.origin.x,
                                quad_bounds.origin.y + geom_size.height - t,
                            ),
                            size: size(geom_size.width, t),
                        },
                        color,
                    ));
                    window.paint_quad(fill(
                        Bounds {
                            origin: quad_bounds.origin,
                            size: size(t, geom_size.height),
                        },
                        color,
                    ));
                    window.paint_quad(fill(
                        Bounds {
                            origin: point(
                                quad_bounds.origin.x + geom_size.width - t,
                                quad_bounds.origin.y,
                            ),
                            size: size(t, geom_size.height),
                        },
                        color,
                    ));
                } else {
                    window.paint_quad(fill(quad_bounds, gpui::rgba(0xf8f8f280)));
                }
            }
        }

        // Scrollbar. 6px thumb on the right edge, matching the wgpu app's
        // build_scroll_bar_instances geometry.
        let (display_offset, history_size) = self.terminal.scrollback_info();
        let rows = self.terminal.rows.get() as usize;
        let (thumb_start, thumb_rows) =
            mouse::scrollbar_thumb_geometry(rows, history_size, display_offset);
        if history_size > 0 {
            window.paint_quad(fill(
                Bounds {
                    origin: point(
                        bounds.origin.x + bounds.size.width - mouse::SCROLLBAR_PX,
                        bounds.origin.y + self.cell_height * thumb_start as f32,
                    ),
                    size: size(mouse::SCROLLBAR_PX, self.cell_height * thumb_rows as f32),
                },
                gpui::rgba(0xf8f8f260),
            ));
        }
    }
}
