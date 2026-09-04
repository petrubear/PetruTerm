// gpui chrome migration (M0 foundation spike + M1a foundation fixes): paints
// one `term::Terminal`'s grid as a custom gpui `Element`.
//
// gpui's native text-shaping/painting facilities (`window.text_system()`,
// used here originally) do not produce ligatures at this gpui version, per
// real dogfood testing (not a config mistake — a confirmed limitation).
// Fallback: shape + rasterize the grid with cosmic-text (which already
// renders ligatures correctly in this project's existing wgpu renderer, see
// src/font/shaper.rs) into an RGBA bitmap, and paint that bitmap into gpui
// via `Window::paint_image`.

use std::rc::Rc;

use gpui::{
    fill, point, px, size, App, Bounds, Corners, Element, ElementId, GlobalElementId,
    InspectorElementId, IntoElement, LayoutId, Pixels, Style, Window,
};

use crate::term::Terminal;

use super::{mouse, mouse::OnFocusCallback, rasterize};

pub struct TerminalGridElement {
    pub terminal: Rc<Terminal>,
    pub cell_width: Pixels,
    pub cell_height: Pixels,
    pub colors: crate::config::schema::ColorScheme,
    pub is_active: bool,
    pub cursor_blink_on: bool,
    pub on_focus: OnFocusCallback,
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
        let cols = self.terminal.cols.get() as f32;
        let rows = self.terminal.rows.get() as f32;
        let mut style = Style::default();
        style.size.width = (self.cell_width * cols).into();
        style.size.height = (self.cell_height * rows).into();
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
        // Background. Real theme background (not the M0/M1a placeholder
        // color) -- `rasterize::rasterize_grid` skips filling any cell whose
        // resolved background equals `colors.background` on the assumption
        // that this base fill already covers it, so the two must agree.
        let [r, g, b, a] = self.colors.background;
        window.paint_quad(fill(bounds, gpui::Rgba { r, g, b, a }));

        mouse::register_mouse_handlers(
            self.terminal.clone(),
            bounds,
            self.cell_width,
            self.cell_height,
            self.on_focus.clone(),
            window,
        );

        if let Some(render_image) = rasterize::rasterize_grid(
            &self.terminal,
            self.cell_width,
            self.cell_height,
            window.scale_factor(),
            &self.colors,
            window,
        ) {
            let _ = window.paint_image(bounds, Corners::default(), render_image, 0, false);
        }

        // Cursor. Shape from Terminal::cursor_info() (DECSCUSR / default),
        // geometry ported from src/app/renderer/terminal.rs's
        // build_cursor_overlay. HollowBlock swaps in for Block when this
        // pane isn't the split-focus target -- matches the wgpu renderer's
        // convention for showing which pane has keyboard focus.
        let cursor = self.terminal.cursor_info();
        if cursor.visible && self.cursor_blink_on {
            use alacritty_terminal::vte::ansi::CursorShape;
            let shape = if !self.is_active && cursor.shape == CursorShape::Block {
                CursorShape::HollowBlock
            } else {
                cursor.shape
            };
            let cell_w = self.cell_width;
            let cell_h = self.cell_height;
            let cursor_origin = point(
                bounds.origin.x + cell_w * (cursor.col as f32),
                bounds.origin.y + cell_h * (cursor.row as f32),
            );
            let (offset, geom_size) = match shape {
                CursorShape::Block | CursorShape::HollowBlock => {
                    (point(px(0.0), px(0.0)), size(cell_w, cell_h))
                }
                CursorShape::Underline => (
                    point(px(0.0), (cell_h - px(2.0)).max(px(0.0))),
                    size(cell_w, px(2.0)),
                ),
                CursorShape::Beam => (point(px(0.0), px(0.0)), size(px(2.0), cell_h)),
                CursorShape::Hidden => return,
            };
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
}
