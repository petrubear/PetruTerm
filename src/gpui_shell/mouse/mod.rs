// gpui chrome migration (M1b): mouse-driven interaction for the terminal
// grid -- click-drag selection, click-to-focus for split panes, and (later
// tasks in this plan) mouse-report passthrough and scrollbar drag. Each
// `TerminalGridElement` owns its own mouse handling, registered fresh every
// `paint()` call and scoped to that element's own `bounds` -- mirrors how
// the element already owns cursor/text painting math scoped to its own
// bounds, and avoids tracking child-element bounds in the parent `div` just
// for hit-testing (gpui's flex layout doesn't expose child bounds until
// paint completes).
//
// Split into this directory (TD-GPUI-03, 2026-09-17): the single `mouse.rs`
// this replaces had grown to 875 lines. `click_state.rs` holds the per-
// terminal click/drag/scroll state machine, `geometry.rs` the pure pixel/
// cell/scrollbar geometry helpers; this file keeps the public API surface
// (`register_mouse_handlers`, `format_mouse_report`, `OnFocusCallback`,
// `SCROLLBAR_PX`) plus re-exports so every external `mouse::...` call site
// (`actions.rs`, `context_menu.rs`, `terminal_element.rs`, `pane_view.rs`)
// keeps working unchanged. Pure code motion throughout: no logic changed.

use std::rc::Rc;

use gpui::{
    px, App, Bounds, DispatchPhase, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent,
    Pixels, Window,
};

use crate::term::Terminal;

mod click_state;
mod geometry;

pub(super) use click_state::forget_terminal;
use click_state::{
    accumulate_scroll_lines, is_dragging_scrollbar, mark_dragged, mark_dragged_if_moved,
    register_click, set_dragging_scrollbar, start_gesture, take_dragged,
};
use geometry::{in_scrollbar_strip, selection_type_for_clicks, y_to_display_offset};
pub use geometry::{pixel_to_cell, scrollbar_thumb_geometry};

pub type OnFocusCallback = Rc<dyn Fn(&mut Window, &mut App)>;

/// Width of the scrollbar's hit-test strip and painted thumb, on the right
/// edge of the terminal's `bounds`. `terminal_element.rs`'s paint code uses
/// this same constant (not a duplicate) so the hit-test strip and the
/// painted thumb can never drift apart.
pub const SCROLLBAR_PX: Pixels = px(6.0);

/// Register this element's mouse handlers for the current frame (cleared
/// automatically by gpui after paint -- must be called fresh every
/// `paint()`, per `Window::on_mouse_event`'s own contract). Handles
/// click-drag selection, click-to-focus, scrollbar-thumb drag, and the
/// scroll wheel; mouse-report passthrough (Task 5) and scrollbar-drag
/// (this task) are checked before selection so neither also starts a
/// selection or forwards to the remote program.
///
/// None of this module's `write_input` call sites (mouse-report passthrough,
/// below) are gated on `GpuiShellRoot::tab_rename`, and that's deliberate,
/// not an oversight the tab-rename work forgot -- but not because gpui
/// focus has already moved by the time these handlers run. It hasn't:
/// `Interactivity::paint` registers a div's own listeners before recursing
/// into children (`elements/div.rs:1855` calls `paint_mouse_listeners`,
/// then `div.rs:1865` paints children), and the bubble phase walks
/// registered listeners in reverse order (`window.rs:3705`'s `.rev()`), so
/// the root div's auto-focus listener -- outermost, registered first --
/// fires LAST, strictly after every handler in this module. Nothing here
/// may assume gpui focus has already changed.
///
/// These handlers simply never consult gpui focus at all: a mouse event is
/// routed to whichever handler's hitbox contains it (`bounds.contains`,
/// checked in every handler below), independent of window focus state, and
/// `on_focus` updates `focused_terminal` -- the app-level state this
/// codebase actually keys "which pane is active" on -- directly, not by
/// reading it back from gpui. So a terminal click is unambiguous terminal
/// input by construction, whatever `tab_rename` or gpui's own focus happen
/// to say; the keyboard guard in `input.rs` polices the PTY's other input
/// path (typed keys), and has no bearing on this one.
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
        let (offset, history_size) = down_terminal.scrollback_info();
        if history_size > 0 && in_scrollbar_strip(event.position, bounds) {
            let rows = down_terminal.rows.get() as usize;
            let target =
                y_to_display_offset(event.position.y, bounds, cell_height, rows, history_size);
            // `Terminal::scroll_display`'s delta convention is positive =
            // toward history, negative = toward the live bottom (verified
            // against alacritty_terminal's own `Term::scroll_display` --
            // `Terminal::scroll_display`'s own doc comment has this
            // backwards, matching a pre-existing wrong comment this task
            // doesn't touch). `target - offset`, not `offset - target`: the
            // wgpu app's own two scroll-to-position call sites both negate
            // the same `offset - target` difference for exactly this reason
            // (src/app/mod.rs's scroll handler, src/app/frame.rs's search
            // match centering).
            let delta = target as i32 - offset as i32;
            if delta != 0 {
                down_terminal.scroll_display(delta);
            }
            set_dragging_scrollbar(terminal_key, true);
            window.refresh();
            return; // scrollbar click: no local selection, no mouse report
        }
        // Not a scrollbar click: make sure a stuck flag (e.g. a missed
        // mouse-up from a drag that ended outside the window) can't wrongly
        // route this fresh gesture to scroll instead of select.
        set_dragging_scrollbar(terminal_key, false);
        let cols = down_terminal.cols.get() as usize;
        let rows = down_terminal.rows.get() as usize;
        let (col, row) = pixel_to_cell(event.position, bounds, cell_width, cell_height, cols, rows);
        start_gesture(terminal_key, (col, row));
        // Click-to-focus is a chrome concern, like the scrollbar above --
        // it must run regardless of mouse-report mode, so a click into an
        // unfocused pane running vim/tmux still moves keyboard focus there
        // even though the click itself is forwarded as a mouse report
        // rather than starting a local selection.
        on_focus(window, cx);
        let (any_mouse, sgr, _) = down_terminal.mouse_mode_flags();
        if any_mouse {
            if let Some(bytes) = format_mouse_report(0, col, row, true, sgr) {
                down_terminal.write_input(&bytes);
            }
            return; // mouse-report mode: don't also start a local selection
        }
        let clicks = register_click(terminal_key, (col, row));
        down_terminal.start_selection(col, row, selection_type_for_clicks(clicks));
        if clicks > 1 {
            // See `mark_dragged`'s doc comment: a double/triple click's
            // word/line selection is already complete from this mouse-down
            // alone.
            mark_dragged(terminal_key);
        }
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
        if super::pane_view::is_dragging_separator()
            || super::resize_handle::is_dragging_resize_handle()
        {
            // A separator/panel-resize drag sweeps the pointer across
            // whichever panes it passes over, with the left button held.
            // Without this, each of those panes would run the selection
            // path below -- `bounds`-gated, so it can't start a NEW
            // selection (no mouse-down landed in the pane), but
            // `update_selection` would still silently extend an OLD one the
            // pane was already showing.
            return;
        }
        if is_dragging_scrollbar(terminal_key) {
            let (offset, history_size) = move_terminal.scrollback_info();
            if history_size > 0 {
                let rows = move_terminal.rows.get() as usize;
                let target =
                    y_to_display_offset(event.position.y, bounds, cell_height, rows, history_size);
                // See the mouse-down handler's comment on this same
                // computation: the delta is target-relative-to-offset, not
                // the reverse.
                let delta = target as i32 - offset as i32;
                if delta != 0 {
                    move_terminal.scroll_display(delta);
                }
                window.refresh();
            }
            return; // dragging the scrollbar thumb, not extending a selection
        }
        if !bounds.contains(&event.position) {
            return;
        }
        let cols = move_terminal.cols.get() as usize;
        let rows = move_terminal.rows.get() as usize;
        let (col, row) = pixel_to_cell(event.position, bounds, cell_width, cell_height, cols, rows);
        let (any_mouse, sgr, motion) = move_terminal.mouse_mode_flags();
        if any_mouse {
            if motion {
                if let Some(bytes) = format_mouse_report(32, col, row, true, sgr) {
                    move_terminal.write_input(&bytes);
                }
            }
            return; // mouse-report mode: don't also extend a local selection
        }
        mark_dragged_if_moved(terminal_key, (col, row));
        move_terminal.update_selection(col, row);
        // See the mouse-down handler's comment: without this, the selection
        // highlight only catches up to the live drag whenever some other
        // event happens to trigger a repaint, which reads as laggy/unsnappy
        // selection even though the underlying selection state is current.
        window.refresh();
    });

    let scroll_terminal = terminal.clone();
    window.on_mouse_event(move |event: &gpui::ScrollWheelEvent, phase, window, _cx| {
        if phase != DispatchPhase::Bubble {
            return;
        }
        if !bounds.contains(&event.position) {
            return;
        }
        let pixel_delta = event.delta.pixel_delta(cell_height);
        let raw_lines = f32::from(pixel_delta.y) / f32::from(cell_height);
        let line_delta = accumulate_scroll_lines(terminal_key, raw_lines);
        if line_delta == 0 {
            return;
        }
        let cols = scroll_terminal.cols.get() as usize;
        let rows = scroll_terminal.rows.get() as usize;
        let (col, row) = pixel_to_cell(event.position, bounds, cell_width, cell_height, cols, rows);
        let (any_mouse, sgr, _) = scroll_terminal.mouse_mode_flags();
        if any_mouse {
            // xterm wheel-report convention: button 64 = wheel up (toward
            // history), 65 = wheel down (toward the live bottom) -- same
            // sign as `line_delta` itself (positive = toward history, per
            // the branch below), so no extra negation here. Capped at 3
            // reports per gesture, matching the wgpu app's own
            // handle_scroll: each report triggers a full redraw in the
            // remote program, and sending more than that per wheel tick is
            // visible lag, not extra precision.
            let button = if line_delta > 0 { 64u8 } else { 65u8 };
            for _ in 0..line_delta.abs().min(3) {
                if let Some(bytes) = format_mouse_report(button, col, row, true, sgr) {
                    scroll_terminal.write_input(&bytes);
                }
            }
            window.refresh();
            return;
        }
        scroll_terminal.scroll_display(line_delta);
        // Without this, an idle-prompt wheel-scroll doesn't repaint until
        // the poll loop's own next incidental notify (up to 530ms later) --
        // the same class of lag Task 4's mouse-down/move comments describe.
        window.refresh();
    });

    let up_terminal = terminal;
    window.on_mouse_event(move |event: &MouseUpEvent, phase, window, cx| {
        if phase != DispatchPhase::Bubble || event.button != MouseButton::Left {
            return;
        }
        if is_dragging_scrollbar(terminal_key) {
            set_dragging_scrollbar(terminal_key, false);
            return; // scrollbar release: no mouse report, no clipboard copy
        }
        let cols = up_terminal.cols.get() as usize;
        let rows = up_terminal.rows.get() as usize;
        let (col, row) = pixel_to_cell(event.position, bounds, cell_width, cell_height, cols, rows);
        let (any_mouse, sgr, _) = up_terminal.mouse_mode_flags();
        if any_mouse {
            if let Some(bytes) = format_mouse_report(0, col, row, false, sgr) {
                up_terminal.write_input(&bytes);
            }
            return; // mouse-report mode: don't also copy a local selection
        }
        // Read-and-clear (not just read): see `take_dragged`'s doc comment
        // for why this must be one-shot with multiple split panes. A real
        // drag copies; a plain click (or a released-outside-any-pane event
        // reaching a pane it didn't start in) clears the one-cell selection
        // `start_selection` leaves behind instead of copying it -- matches
        // the wgpu app's own mouse-up handler (src/app/mod.rs), which does
        // the same for exactly the same reason.
        if take_dragged(terminal_key) {
            if let Some(text) = up_terminal.selection_text() {
                cx.write_to_clipboard(gpui::ClipboardItem::new_string(text));
            }
        } else {
            up_terminal.clear_selection();
            window.refresh();
        }
    });
}

/// Format a mouse-report escape sequence for `button` at (col, row),
/// `pressed` or released, in SGR or legacy X10 mode -- ported from
/// `src/app/input/mod.rs`'s `send_mouse_report` as-is. Legacy X10 mode
/// only reports presses (returns `None` on release, matching the original).
pub fn format_mouse_report(
    button: u8,
    col: usize,
    row: usize,
    pressed: bool,
    sgr: bool,
) -> Option<Vec<u8>> {
    if sgr {
        let c = if pressed { 'M' } else { 'm' };
        Some(format!("\x1b[<{button};{};{}{c}", col + 1, row + 1).into_bytes())
    } else if pressed {
        let b = button.saturating_add(32);
        let x = ((col + 1) as u8).saturating_add(32);
        let y = ((row + 1) as u8).saturating_add(32);
        Some(vec![0x1b, b'[', b'M', b, x, y])
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sgr_press_format() {
        let bytes = format_mouse_report(0, 4, 9, true, true).unwrap();
        assert_eq!(bytes, b"\x1b[<0;5;10M");
    }

    #[test]
    fn sgr_release_format() {
        let bytes = format_mouse_report(0, 4, 9, false, true).unwrap();
        assert_eq!(bytes, b"\x1b[<0;5;10m");
    }

    #[test]
    fn legacy_x10_press_format() {
        let bytes = format_mouse_report(0, 4, 9, true, false).unwrap();
        assert_eq!(bytes, &[0x1b, b'[', b'M', 32, 5 + 32, 10 + 32]);
    }

    #[test]
    fn legacy_x10_release_sends_nothing() {
        assert_eq!(format_mouse_report(0, 4, 9, false, false), None);
    }
}
