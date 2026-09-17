// gpui chrome migration (post-M5 dogfood): a draggable vertical edge for
// resizing a fixed-width panel (the workspace sidebar, first user) --
// requested live after the sidebar's 220px fixed width was reported
// unusable at anything short of a maximized window.
//
// Mirrors `pane_view.rs`'s own `SeparatorElement`/`DRAGGING_SEPARATOR`
// almost exactly, and for the same reason that file's own doc comment
// gives: `Div`'s `on_mouse_move`/`on_mouse_up` listeners are hover-gated,
// and the pointer leaves a few-pixel-wide strip within the first frame of
// any real drag, so the drag has to be driven from `Window::on_mouse_event`
// inside a custom `Element`'s `paint`, not from `div()` builder callbacks.
// A separate thread-local/element rather than reusing `SeparatorElement`
// directly: that one's drag state and callback are keyed to a pane-tree
// `node_id` and a `SplitDir` (it resizes a split ratio, not a fixed-width
// panel against the mouse's raw position) -- conflating the two would mean
// either inventing a fake node_id for this or widening that element's
// contract for a second, unrelated caller.

use gpui::{
    fill, point, prelude::*, px, relative, size, App, Bounds, DispatchPhase, ElementId,
    GlobalElementId, InspectorElementId, LayoutId, MouseButton, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, Pixels, Point, Rgba, Style, Window,
};
use std::cell::Cell;
use std::rc::Rc;

/// Width of the grab strip (matches `pane_view.rs`'s own `SEPARATOR_PX`).
pub(super) const RESIZE_HANDLE_PX: f32 = 6.0;

/// Called on every mouse-move while a drag is active, with the pointer's
/// current window-relative position -- the caller (not this element) owns
/// turning that into a clamped width and updating its own state.
pub(super) type ResizeDragCallback = Rc<dyn Fn(Point<Pixels>, &mut Window, &mut App)>;

thread_local! {
    /// `true` while this handle's drag is in progress. Only one resizable
    /// panel exists today (the sidebar), so a single flag is enough; a
    /// second resizable panel would need to key this the way
    /// `DRAGGING_SEPARATOR` keys on `node_id`.
    static DRAGGING: Cell<bool> = const { Cell::new(false) };
}

/// Whether this handle is currently being dragged -- `mouse.rs` can check
/// this the same way it already checks `pane_view::is_dragging_separator()`,
/// so an overshoot from this drag into the terminal beside it doesn't also
/// start or extend a text selection there.
pub(super) fn is_dragging_resize_handle() -> bool {
    DRAGGING.with(|d| d.get())
}

/// Paints the grab strip's divider line and owns its drag gesture.
pub(super) struct ResizeHandleElement {
    pub color: Rgba,
    pub on_drag: ResizeDragCallback,
}

impl IntoElement for ResizeHandleElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for ResizeHandleElement {
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
        // The grab-line only paints while it's actually being grabbed or
        // hovered -- a permanent 1px line down the sidebar's edge, live at
        // every window size, read as a stray rule cutting the UI in half
        // rather than an affordance ("necesito que desaparezca"). The
        // handle's hit area (this whole 6px strip, via `on_mouse_event`
        // below) still exists everywhere: only the visible feedback is
        // conditional. Cheap poll-loop repaints already run at ~30Hz, so
        // this needs no extra `window.refresh()` to catch a hover starting
        // or ending -- the next tick just repaints with a fresh
        // `mouse_position()` read.
        let dragging = DRAGGING.with(|d| d.get());
        let hovered = bounds.contains(&window.mouse_position());
        if dragging || hovered {
            let line = px(1.0);
            let line_bounds = Bounds {
                origin: point(
                    bounds.origin.x + (bounds.size.width - line) / 2.0,
                    bounds.origin.y,
                ),
                size: size(line, bounds.size.height),
            };
            window.paint_quad(fill(line_bounds, self.color));
        }

        window.on_mouse_event(move |event: &MouseDownEvent, phase, window, _cx| {
            if phase != DispatchPhase::Bubble || event.button != MouseButton::Left {
                return;
            }
            if !bounds.contains(&event.position) {
                return;
            }
            DRAGGING.with(|d| d.set(true));
            window.refresh();
        });

        let on_drag = self.on_drag.clone();
        window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
            if phase != DispatchPhase::Bubble {
                return;
            }
            if !DRAGGING.with(|d| d.get()) {
                return;
            }
            if event.pressed_button != Some(MouseButton::Left) {
                // A release that never reached the mouse-up handler (e.g. it
                // happened outside the window) -- end the drag rather than
                // keeping it stuck to the pointer forever.
                DRAGGING.with(|d| d.set(false));
                return;
            }
            on_drag(event.position, window, cx);
        });

        window.on_mouse_event(move |event: &MouseUpEvent, phase, _window, _cx| {
            if phase != DispatchPhase::Bubble || event.button != MouseButton::Left {
                return;
            }
            if DRAGGING.with(|d| d.get()) {
                DRAGGING.with(|d| d.set(false));
            }
        });
    }
}
