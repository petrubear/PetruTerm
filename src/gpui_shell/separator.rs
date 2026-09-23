// The pane separator's custom `Element`. Its builder (`pane_view::separator`)
// and `SEPARATOR_PX` live in `pane_view.rs`.

use gpui::{
    fill, point, prelude::*, px, relative, size, App, Bounds, DispatchPhase, ElementId,
    GlobalElementId, InspectorElementId, LayoutId, MouseButton, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, Pixels, Rgba, Style, Window,
};
use std::cell::Cell;

use super::pane_view::SeparatorDragCallback;
use super::panes::SplitDir;

/// Thickness of the painted divider line itself, centered inside the
/// wider grab strip `pane_view.rs`'s `SEPARATOR_PX` defines.
const SEPARATOR_LINE_PX: f32 = 1.0;

thread_local! {
    /// `Some(node_id)` while a separator drag is in progress. Lives here
    /// rather than on `GpuiShellRoot` for the same reason `mouse.rs`'s
    /// `CLICK_STATE` does: the elements that read and write it are rebuilt
    /// from scratch every frame.
    static DRAGGING_SEPARATOR: Cell<Option<u32>> = const { Cell::new(None) };
}

/// Whether a pane separator is currently being dragged. `mouse.rs` checks
/// this so a drag that passes over a pane doesn't also extend that pane's
/// text selection.
pub(super) fn is_dragging_separator() -> bool {
    DRAGGING_SEPARATOR.with(|d| d.get().is_some())
}

/// Paints the divider line and owns the separator's drag gesture.
///
/// A custom `Element` rather than plain `div()` builder callbacks for one
/// concrete reason: `Div`'s own `on_mouse_move`/`on_mouse_up` listeners are
/// hover-gated (`Interactivity::on_mouse_move` checks `hitbox.is_hovered`),
/// and the pointer leaves a 6px strip within the first frame of any real
/// drag. `Window::on_mouse_event` -- the same primitive `mouse.rs` uses for
/// text-selection and scrollbar drags -- isn't hover-gated, but may only be
/// called during the paint phase, which is exactly what an `Element` gives.
pub(super) struct SeparatorElement {
    pub node_id: u32,
    pub dir: SplitDir,
    pub color: Rgba,
    pub on_drag: SeparatorDragCallback,
}

impl IntoElement for SeparatorElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for SeparatorElement {
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
        let line = px(SEPARATOR_LINE_PX);
        let line_bounds = match self.dir {
            SplitDir::Horizontal => Bounds {
                origin: point(
                    bounds.origin.x + (bounds.size.width - line) / 2.0,
                    bounds.origin.y,
                ),
                size: size(line, bounds.size.height),
            },
            SplitDir::Vertical => Bounds {
                origin: point(
                    bounds.origin.x,
                    bounds.origin.y + (bounds.size.height - line) / 2.0,
                ),
                size: size(bounds.size.width, line),
            },
        };
        window.paint_quad(fill(line_bounds, self.color));

        let node_id = self.node_id;
        window.on_mouse_event(move |event: &MouseDownEvent, phase, window, _cx| {
            if phase != DispatchPhase::Bubble || event.button != MouseButton::Left {
                return;
            }
            if !bounds.contains(&event.position) {
                return;
            }
            DRAGGING_SEPARATOR.with(|d| d.set(Some(node_id)));
            window.refresh();
        });

        let on_drag = self.on_drag.clone();
        window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
            if phase != DispatchPhase::Bubble {
                return;
            }
            if DRAGGING_SEPARATOR.with(|d| d.get()) != Some(node_id) {
                return;
            }
            if event.pressed_button != Some(MouseButton::Left) {
                // A release that never reached the mouse-up handler (e.g. it
                // happened outside the window) -- end the drag rather than
                // keeping it stuck to the pointer forever.
                DRAGGING_SEPARATOR.with(|d| d.set(None));
                return;
            }
            on_drag(node_id, event.position, window, cx);
        });

        window.on_mouse_event(move |event: &MouseUpEvent, phase, _window, _cx| {
            if phase != DispatchPhase::Bubble || event.button != MouseButton::Left {
                return;
            }
            if DRAGGING_SEPARATOR.with(|d| d.get()) == Some(node_id) {
                DRAGGING_SEPARATOR.with(|d| d.set(None));
            }
        });
    }
}
