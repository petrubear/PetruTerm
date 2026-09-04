// gpui chrome migration (M1b): mouse-driven interaction for the terminal
// grid -- click-drag selection, click-to-focus for split panes, and (later
// tasks in this plan) mouse-report passthrough and scrollbar drag. Each
// `TerminalGridElement` owns its own mouse handling, registered fresh every
// `paint()` call and scoped to that element's own `bounds` -- mirrors how
// the element already owns cursor/text painting math scoped to its own
// bounds, and avoids tracking child-element bounds in the parent `div` just
// for hit-testing (gpui's flex layout doesn't expose child bounds until
// paint completes).

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::time::Instant;

use alacritty_terminal::selection::SelectionType;
use gpui::{
    App, Bounds, DispatchPhase, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels,
    Point, Window,
};

use crate::term::Terminal;

pub type OnFocusCallback = Rc<dyn Fn(&mut Window, &mut App)>;

/// Per-terminal click-tracking state, keyed the same way `rasterize`'s
/// `LAST_IMAGE` is (the `Rc<Terminal>`'s heap address) -- `TerminalGridElement`
/// is rebuilt every frame, so this can't live on the element itself.
struct ClickState {
    last_click_time: Instant,
    last_click_cell: (usize, usize),
    click_count: u32,
}

thread_local! {
    static CLICK_STATE: RefCell<HashMap<usize, ClickState>> = RefCell::new(HashMap::new());
}

/// Update click count for multi-click detection at `cell`, keyed by
/// `terminal_key` (an `Rc<Terminal>` heap address, matching `rasterize`'s
/// `LAST_IMAGE` key). Returns 1 / 2 / 3 based on timing and position --
/// ported from `src/app/input/mod.rs`'s `register_click` as-is.
pub fn register_click(terminal_key: usize, cell: (usize, usize)) -> u32 {
    const DOUBLE_CLICK_MS: u128 = 500;
    CLICK_STATE.with_borrow_mut(|states| {
        let state = states.entry(terminal_key).or_insert(ClickState {
            last_click_time: Instant::now() - std::time::Duration::from_secs(1),
            last_click_cell: (usize::MAX, usize::MAX),
            click_count: 0,
        });
        let same_cell = state.last_click_cell == cell;
        let within_time = state.last_click_time.elapsed().as_millis() < DOUBLE_CLICK_MS;
        state.click_count = if same_cell && within_time {
            (state.click_count + 1).min(3)
        } else {
            1
        };
        state.last_click_time = Instant::now();
        state.last_click_cell = cell;
        state.click_count
    })
}

/// Map a click count to the alacritty selection type it starts -- ported
/// from `src/app/mod.rs`'s mapping as-is.
pub fn selection_type_for_clicks(clicks: u32) -> SelectionType {
    match clicks {
        2 => SelectionType::Semantic,
        3 => SelectionType::Lines,
        _ => SelectionType::Simple,
    }
}

/// Convert a window-relative mouse position to a (col, row) grid cell,
/// relative to `bounds`'s origin -- simpler than the wgpu app's
/// `pixel_to_cell` (no pane padding to account for; `bounds` is already
/// this element's own painted area).
pub fn pixel_to_cell(
    position: Point<Pixels>,
    bounds: Bounds<Pixels>,
    cell_width: Pixels,
    cell_height: Pixels,
) -> (usize, usize) {
    let x = f32::from(position.x - bounds.origin.x);
    let y = f32::from(position.y - bounds.origin.y);
    let col = (x / f32::from(cell_width)).floor().max(0.0) as usize;
    let row = (y / f32::from(cell_height)).floor().max(0.0) as usize;
    (col, row)
}

/// Register this element's mouse handlers for the current frame (cleared
/// automatically by gpui after paint -- must be called fresh every
/// `paint()`, per `Window::on_mouse_event`'s own contract). Handles
/// click-drag selection and click-to-focus; later tasks in this plan add
/// mouse-report passthrough and scrollbar-drag checks before this task's
/// selection logic runs.
pub fn register_mouse_handlers(
    terminal: Rc<Terminal>,
    bounds: Bounds<Pixels>,
    cell_width: Pixels,
    cell_height: Pixels,
    on_focus: OnFocusCallback,
    window: &mut Window,
) {
    let terminal_key = Rc::as_ptr(&terminal) as usize;

    let down_terminal = terminal.clone();
    window.on_mouse_event(move |event: &MouseDownEvent, phase, window, cx| {
        if phase != DispatchPhase::Bubble || event.button != MouseButton::Left {
            return;
        }
        if !bounds.contains(&event.position) {
            return;
        }
        on_focus(window, cx);
        let (col, row) = pixel_to_cell(event.position, bounds, cell_width, cell_height);
        let clicks = register_click(terminal_key, (col, row));
        down_terminal.start_selection(col, row, selection_type_for_clicks(clicks));
        // Selection state changed, but nothing else in this frame requested
        // a repaint (a click into the already-active pane skips on_focus's
        // own notify). Without this, gpui only redraws whenever the poll
        // loop's blink toggle happens to fire (up to 530ms later) -- ported
        // from the wgpu app's own drag handler, which calls
        // `self.request_redraw()` after every start_selection/
        // update_selection for exactly this reason (src/app/mod.rs).
        window.refresh();
    });

    let move_terminal = terminal.clone();
    window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, _cx| {
        if phase != DispatchPhase::Bubble {
            return;
        }
        if event.pressed_button != Some(MouseButton::Left) {
            return;
        }
        if !bounds.contains(&event.position) {
            return;
        }
        let (col, row) = pixel_to_cell(event.position, bounds, cell_width, cell_height);
        move_terminal.update_selection(col, row);
        // See the mouse-down handler's comment: without this, the selection
        // highlight only catches up to the live drag whenever some other
        // event happens to trigger a repaint, which reads as laggy/unsnappy
        // selection even though the underlying selection state is current.
        window.refresh();
    });

    let up_terminal = terminal;
    window.on_mouse_event(move |event: &MouseUpEvent, phase, _window, cx| {
        if phase != DispatchPhase::Bubble || event.button != MouseButton::Left {
            return;
        }
        if let Some(text) = up_terminal.selection_text() {
            cx.write_to_clipboard(gpui::ClipboardItem::new_string(text));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{point, px};

    #[test]
    fn first_click_is_count_one() {
        assert_eq!(register_click(1, (5, 5)), 1);
    }

    #[test]
    fn same_cell_quick_second_click_is_count_two() {
        let key = 2;
        register_click(key, (5, 5));
        assert_eq!(register_click(key, (5, 5)), 2);
    }

    #[test]
    fn different_cell_resets_to_count_one() {
        let key = 3;
        register_click(key, (5, 5));
        assert_eq!(register_click(key, (6, 5)), 1);
    }

    #[test]
    fn click_count_caps_at_three() {
        let key = 4;
        register_click(key, (5, 5));
        register_click(key, (5, 5));
        register_click(key, (5, 5));
        assert_eq!(register_click(key, (5, 5)), 3);
    }

    #[test]
    fn click_count_maps_to_selection_type() {
        assert_eq!(selection_type_for_clicks(1), SelectionType::Simple);
        assert_eq!(selection_type_for_clicks(2), SelectionType::Semantic);
        assert_eq!(selection_type_for_clicks(3), SelectionType::Lines);
    }

    #[test]
    fn pixel_to_cell_is_bounds_relative() {
        let bounds = Bounds {
            origin: point(px(100.0), px(50.0)),
            size: gpui::size(px(800.0), px(600.0)),
        };
        let cell = pixel_to_cell(point(px(109.0), px(66.0)), bounds, px(9.0), px(18.0));
        assert_eq!(cell, (1, 0)); // (109-100)/9 = 1.0, (66-50)/18 = 0.888 -> row 0
    }
}
