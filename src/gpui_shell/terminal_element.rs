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
    fill, point, size, App, Bounds, Corners, Element, ElementId, GlobalElementId,
    InspectorElementId, IntoElement, LayoutId, Pixels, Style, Window,
};

use crate::term::Terminal;

use super::rasterize;

pub struct TerminalGridElement {
    pub terminal: Rc<Terminal>,
    pub cell_width: Pixels,
    pub cell_height: Pixels,
    pub colors: crate::config::schema::ColorScheme,
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
        let cols = self.terminal.cols as f32;
        let rows = self.terminal.rows as f32;
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

        // Cursor.
        let cursor = self.terminal.cursor_info();
        if cursor.visible {
            let cursor_origin = point(
                bounds.origin.x + self.cell_width * (cursor.col as f32),
                bounds.origin.y + self.cell_height * (cursor.row as f32),
            );
            window.paint_quad(fill(
                Bounds {
                    origin: cursor_origin,
                    size: size(self.cell_width, self.cell_height),
                },
                gpui::rgba(0xf8f8f280),
            ));
        }
    }
}
