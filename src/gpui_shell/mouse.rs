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
    px, App, Bounds, DispatchPhase, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent,
    Pixels, Point, Window,
};

use crate::term::Terminal;

pub type OnFocusCallback = Rc<dyn Fn(&mut Window, &mut App)>;

/// Width of the scrollbar's hit-test strip and painted thumb, on the right
/// edge of the terminal's `bounds`. `terminal_element.rs`'s paint code uses
/// this same constant (not a duplicate) so the hit-test strip and the
/// painted thumb can never drift apart.
pub const SCROLLBAR_PX: Pixels = px(6.0);

/// Per-terminal click-tracking and scrollbar-drag state, keyed the same way
/// `rasterize`'s `LAST_IMAGE` is (the `Rc<Terminal>`'s heap address) --
/// `TerminalGridElement` is rebuilt every frame, so this can't live on the
/// element itself.
struct ClickState {
    last_click_time: Instant,
    last_click_cell: (usize, usize),
    click_count: u32,
    /// Set by mouse-down when the click lands in the scrollbar strip,
    /// cleared by mouse-up. While set, mouse-move drags the scrollbar
    /// thumb instead of extending a text selection.
    dragging_scrollbar: bool,
    /// Fractional scroll lines left over from the last wheel event, carried
    /// to the next one -- see `accumulate_scroll_lines`'s doc comment.
    scroll_accum: f32,
    /// The cell the current gesture's mouse-down landed on, set fresh by
    /// `start_gesture` on every non-scrollbar mouse-down.
    gesture_start_cell: (usize, usize),
    /// Whether the current gesture has moved to a different cell than
    /// `gesture_start_cell` yet -- i.e. whether this is an actual drag, not
    /// a plain click. `alacritty_terminal::Selection` starts a real
    /// (inverted, one-cell) selection on `start_selection` alone, so a
    /// click with zero or sub-cell movement still leaves a selection
    /// behind unless something clears it -- see `take_dragged`'s doc
    /// comment for the consequences of not doing that.
    dragged: bool,
    /// Whether THIS pane's own mouse-down started the gesture currently in
    /// progress -- `true` from `start_gesture` until `take_dragged` ends it.
    /// `mark_dragged_if_moved` only acts while this is set. Without it, a
    /// drag that begins in one split pane and overshoots into a neighbour's
    /// `bounds` would run the neighbour's own `mark_dragged_if_moved`
    /// (mouse-move handlers gate on `bounds.contains`, not on "did my own
    /// mouse-down start this"), spuriously marking a pane `dragged` for a
    /// gesture it never started and corrupting whatever selection it
    /// already had.
    gesture_active: bool,
}

impl ClickState {
    fn new() -> Self {
        ClickState {
            last_click_time: Instant::now() - std::time::Duration::from_secs(1),
            last_click_cell: (usize::MAX, usize::MAX),
            click_count: 0,
            dragging_scrollbar: false,
            scroll_accum: 0.0,
            gesture_start_cell: (usize::MAX, usize::MAX),
            dragged: false,
            gesture_active: false,
        }
    }
}

thread_local! {
    static CLICK_STATE: RefCell<HashMap<usize, ClickState>> = RefCell::new(HashMap::new());
}

/// Forget one terminal's click/drag state, for a pane that is going away.
///
/// Unlike `rasterize`'s `LAST_IMAGE` this holds no GPU resource, so a
/// stranded entry is only a small leak. It matters because the key is an
/// `Rc<Terminal>` heap address: an entry outliving its terminal can be
/// inherited by a later pane allocated at the same address, handing it a
/// stale click count, scroll remainder, or `dragging_scrollbar` flag.
/// Same call-before-you-drop requirement as `rasterize::evict_terminal`.
pub(super) fn forget_terminal(terminal_key: usize) {
    CLICK_STATE.with_borrow_mut(|states| states.remove(&terminal_key));
}

/// Update click count for multi-click detection at `cell`, keyed by
/// `terminal_key` (an `Rc<Terminal>` heap address, matching `rasterize`'s
/// `LAST_IMAGE` key). Returns 1 / 2 / 3 based on timing and position --
/// ported from `src/app/input/mod.rs`'s `register_click` as-is.
pub fn register_click(terminal_key: usize, cell: (usize, usize)) -> u32 {
    const DOUBLE_CLICK_MS: u128 = 500;
    CLICK_STATE.with_borrow_mut(|states| {
        let state = states.entry(terminal_key).or_insert_with(ClickState::new);
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

/// Set/clear the scrollbar-thumb drag flag for `terminal_key`.
fn set_dragging_scrollbar(terminal_key: usize, dragging: bool) {
    CLICK_STATE.with_borrow_mut(|states| {
        states
            .entry(terminal_key)
            .or_insert_with(ClickState::new)
            .dragging_scrollbar = dragging;
    });
}

/// Start tracking a fresh non-scrollbar mouse-down gesture at `cell`:
/// resets `dragged` to false and records `cell` as the point later moves
/// are compared against. Must be called on every such mouse-down, even one
/// that turns out to just be a plain click, so a leftover `dragged: true`
/// from an EARLIER gesture in this pane can never survive into this one.
fn start_gesture(terminal_key: usize, cell: (usize, usize)) {
    CLICK_STATE.with_borrow_mut(|states| {
        let state = states.entry(terminal_key).or_insert_with(ClickState::new);
        state.dragged = false;
        state.gesture_start_cell = cell;
        state.gesture_active = true;
    });
}

/// Mark the in-progress gesture as a real drag once `cell` differs from
/// where it started -- ported from the wgpu app's `mouse_dragged` flag
/// (`src/app/mod.rs`), which exists for exactly this reason: alacritty's
/// `Selection::new` on mouse-down already creates a real (inverted,
/// one-cell) selection, so without distinguishing "moved" from "didn't",
/// every plain click leaves a lingering highlighted cell and mouse-up's
/// `selection_text()` returns `Some` for it -- clobbering the system
/// clipboard with a single stray character on an ordinary click.
fn mark_dragged_if_moved(terminal_key: usize, cell: (usize, usize)) {
    CLICK_STATE.with_borrow_mut(|states| {
        let state = states.entry(terminal_key).or_insert_with(ClickState::new);
        if state.gesture_active && cell != state.gesture_start_cell {
            state.dragged = true;
        }
    });
}

/// Force the current gesture's `dragged` flag on, independent of any
/// subsequent pointer movement -- for the case where the mouse-DOWN itself
/// already constitutes a complete, intentional selection. `SelectionType::
/// Semantic`/`Lines` (double/triple click) expand to the whole word/line
/// from a single point with zero movement (`alacritty_terminal::selection::
/// Selection::range_semantic`/`range_lines` search left/right from `start
/// == end`), so treating a double/triple click as an undragged "plain
/// click" -- the same rule that correctly clears a single click's stray
/// one-cell selection -- would incorrectly clear the word/line it just
/// selected on release instead of copying it.
fn mark_dragged(terminal_key: usize) {
    CLICK_STATE.with_borrow_mut(|states| {
        states
            .entry(terminal_key)
            .or_insert_with(ClickState::new)
            .dragged = true;
    });
}

/// Read and reset `dragged` in one step -- called once, by mouse-up. A
/// one-shot read-and-clear (not just a read) matters with multiple split
/// panes: `MouseUpEvent` has no bounds check (a drag can legitimately end
/// outside the pane it started in), so EVERY pane's mouse-up handler fires
/// on every release, not just the pane the gesture happened in. If
/// `dragged` weren't consumed here, a pane whose own last real drag left it
/// `true` would keep re-copying its (unrelated, unchanged) selection to the
/// clipboard on every future release anywhere in the window, silently
/// racing whichever other pane's release the user actually meant. Also
/// ends `gesture_active`, so a later drag that merely passes back through
/// this pane's `bounds` (started and still owned by some OTHER pane) can't
/// mark this one dragged again -- see `gesture_active`'s doc comment.
fn take_dragged(terminal_key: usize) -> bool {
    CLICK_STATE.with_borrow_mut(|states| {
        let state = states.entry(terminal_key).or_insert_with(ClickState::new);
        state.gesture_active = false;
        std::mem::take(&mut state.dragged)
    })
}

/// Add `raw_lines` (a single wheel event's un-rounded line delta) to
/// `terminal_key`'s running fractional remainder, then split off and return
/// the whole-line part, keeping the leftover fraction for next time --
/// ported from the wgpu app's `scroll_pixel_accum` (`src/app/mod.rs`'s
/// `handle_scroll`). A trackpad reports many small events per gesture; each
/// one's delta is frequently under one line's worth of pixels (an 18px cell
/// swallows anything under ~9px per `.round()`), so rounding each event
/// independently -- this function's previous behaviour -- silently dropped
/// most of a gentle scroll. Accumulating first means no motion is lost, just
/// delayed by at most one line until enough of it has arrived.
fn accumulate_scroll_lines(terminal_key: usize, raw_lines: f32) -> i32 {
    CLICK_STATE.with_borrow_mut(|states| {
        let state = states.entry(terminal_key).or_insert_with(ClickState::new);
        state.scroll_accum += raw_lines;
        let lines = state.scroll_accum.trunc();
        state.scroll_accum -= lines;
        lines as i32
    })
}

/// Whether `terminal_key` is currently mid-drag on its scrollbar thumb.
fn is_dragging_scrollbar(terminal_key: usize) -> bool {
    CLICK_STATE.with_borrow(|states| {
        states
            .get(&terminal_key)
            .is_some_and(|s| s.dragging_scrollbar)
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
/// this element's own painted area), but keeps its grid clamp
/// (`src/app/layout.rs`): every caller here already gates on
/// `bounds.contains(&event.position)` first, but `Bounds::contains` is
/// inclusive on the far edge, so a click on the exact right/bottom boundary
/// pixel would otherwise resolve to `col == cols` / `row == rows` -- one
/// past the last real cell -- and reach `Selection::update`/
/// `format_mouse_report` out of grid range.
pub fn pixel_to_cell(
    position: Point<Pixels>,
    bounds: Bounds<Pixels>,
    cell_width: Pixels,
    cell_height: Pixels,
    cols: usize,
    rows: usize,
) -> (usize, usize) {
    let x = f32::from(position.x - bounds.origin.x);
    let y = f32::from(position.y - bounds.origin.y);
    let col = (x / f32::from(cell_width)).floor().max(0.0) as usize;
    let row = (y / f32::from(cell_height)).floor().max(0.0) as usize;
    (
        col.min(cols.saturating_sub(1)),
        row.min(rows.saturating_sub(1)),
    )
}

/// Whether `position` falls in the scrollbar's hit-test strip: the 6px
/// column on the right edge of `bounds`, matching the width of the thumb
/// painted in `terminal_element.rs`'s `paint()`.
fn in_scrollbar_strip(position: Point<Pixels>, bounds: Bounds<Pixels>) -> bool {
    position.x >= bounds.origin.x + bounds.size.width - SCROLLBAR_PX
}

/// Convert a Y pixel position to the scrollback `display_offset` it
/// represents, by inverting `scrollbar_thumb_geometry`'s `thumb_start`
/// formula around the thumb's vertical center -- so a click or drag
/// anywhere in the scrollbar strip centers the thumb under the pointer,
/// clamped to the track's ends. `thumb_rows`/`slack` don't depend on
/// `display_offset` in the forward formula, so they're computed once here
/// with an arbitrary offset (0) purely to get the track geometry.
fn y_to_display_offset(
    y: Pixels,
    bounds: Bounds<Pixels>,
    cell_height: Pixels,
    screen_rows: usize,
    history_size: usize,
) -> usize {
    if screen_rows == 0 || history_size == 0 {
        return 0;
    }
    let (_, thumb_rows) = scrollbar_thumb_geometry(screen_rows, history_size, 0);
    let slack = screen_rows.saturating_sub(thumb_rows);
    if slack == 0 {
        return 0;
    }
    let row = f32::from(y - bounds.origin.y) / f32::from(cell_height);
    let thumb_start = (row - thumb_rows as f32 / 2.0).clamp(0.0, slack as f32);
    let scroll_frac = 1.0 - thumb_start / slack as f32;
    (scroll_frac * history_size as f32).round() as usize
}

/// Scrollbar thumb geometry in row units: `(thumb_start, thumb_rows)`.
/// `display_offset` = 0 means at the bottom of scrollback, `history_size`
/// means at the top -- matches `Terminal::scrollback_info`'s own convention.
/// Ported from `src/app/renderer/overlay.rs`'s `build_scroll_bar_instances`
/// geometry as-is.
pub fn scrollbar_thumb_geometry(
    screen_rows: usize,
    history_size: usize,
    display_offset: usize,
) -> (usize, usize) {
    let total_lines = (screen_rows + history_size).max(1);
    let thumb_rows = (((screen_rows as f32 / total_lines as f32) * screen_rows as f32).round()
        as usize)
        .clamp(1, screen_rows);
    let slack = screen_rows.saturating_sub(thumb_rows);
    let scroll_frac = if history_size == 0 {
        0.0
    } else {
        display_offset as f32 / history_size as f32
    };
    let thumb_start = ((1.0 - scroll_frac) * slack as f32).round() as usize;
    (thumb_start, thumb_rows)
}

/// Register this element's mouse handlers for the current frame (cleared
/// automatically by gpui after paint -- must be called fresh every
/// `paint()`, per `Window::on_mouse_event`'s own contract). Handles
/// click-drag selection, click-to-focus, scrollbar-thumb drag, and the
/// scroll wheel; mouse-report passthrough (Task 5) and scrollbar-drag
/// (this task) are checked before selection so neither also starts a
/// selection or forwards to the remote program.
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
        if super::pane_view::is_dragging_separator() {
            // A separator drag sweeps the pointer across whichever panes it
            // passes over, with the left button held. Without this, each of
            // those panes would run the selection path below -- `bounds`-
            // gated, so it can't start a NEW selection (no mouse-down landed
            // in the pane), but `update_selection` would still silently
            // extend an OLD one the pane was already showing.
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
    use gpui::{point, px};

    #[test]
    fn plain_click_is_not_marked_dragged() {
        let key = 3001;
        start_gesture(key, (5, 5));
        // No move happened -- take_dragged must report false so the
        // caller clears the selection instead of copying it.
        assert!(!take_dragged(key));
    }

    #[test]
    fn moving_to_a_different_cell_marks_the_gesture_dragged() {
        let key = 3002;
        start_gesture(key, (5, 5));
        mark_dragged_if_moved(key, (6, 5));
        assert!(take_dragged(key));
    }

    #[test]
    fn moving_within_the_same_cell_does_not_count_as_dragged() {
        let key = 3003;
        start_gesture(key, (5, 5));
        // Trackpad jitter that resolves to the same cell repeatedly.
        mark_dragged_if_moved(key, (5, 5));
        mark_dragged_if_moved(key, (5, 5));
        assert!(!take_dragged(key));
    }

    #[test]
    fn double_and_triple_click_are_dragged_with_zero_movement() {
        // A double/triple click's word/line selection is already complete
        // from the click alone (see `mark_dragged`'s doc comment) -- it
        // must copy on release even though the pointer never moved.
        let key = 3008;
        start_gesture(key, (5, 5));
        mark_dragged(key);
        assert!(take_dragged(key));
    }

    #[test]
    fn a_pane_that_never_started_the_gesture_is_not_marked_dragged() {
        // A neighbouring pane's mouse-move handlers gate on their own
        // `bounds.contains`, not on "did my own mouse-down start this" --
        // a drag that overshoots from pane A into pane B's bounds must not
        // mark B dragged, or B's own (unrelated) selection would get
        // silently mutated and re-copied on the next release anywhere.
        let key = 3006; // B's key: never had start_gesture called on it here
        mark_dragged_if_moved(key, (6, 5));
        assert!(!take_dragged(key));
    }

    #[test]
    fn a_pane_whose_gesture_already_ended_is_not_marked_dragged_by_a_later_pass_through() {
        let key = 3007;
        start_gesture(key, (5, 5));
        mark_dragged_if_moved(key, (6, 5));
        assert!(take_dragged(key)); // this pane's own gesture, consumed normally
                                    // Some OTHER pane's drag later passes back through this pane's
                                    // bounds -- must not resurrect `dragged` for a gesture this pane
                                    // isn't part of anymore.
        mark_dragged_if_moved(key, (7, 5));
        assert!(!take_dragged(key));
    }

    #[test]
    fn take_dragged_is_one_shot() {
        let key = 3004;
        start_gesture(key, (5, 5));
        mark_dragged_if_moved(key, (6, 5));
        assert!(take_dragged(key)); // consumed here
                                    // A later, unrelated release (e.g. another pane's gesture ending)
                                    // must not see this pane's already-consumed drag as still active --
                                    // otherwise it would silently re-copy stale content on every future
                                    // release anywhere in the window.
        assert!(!take_dragged(key));
    }

    #[test]
    fn start_gesture_resets_a_leftover_dragged_flag() {
        let key = 3005;
        start_gesture(key, (5, 5));
        mark_dragged_if_moved(key, (6, 5));
        // A fresh gesture begins before the previous one's drag flag was
        // ever consumed (e.g. mouse-up was missed) -- it must not inherit
        // the stale `true`.
        start_gesture(key, (1, 1));
        assert!(!take_dragged(key));
    }

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
        let cell = pixel_to_cell(
            point(px(109.0), px(66.0)),
            bounds,
            px(9.0),
            px(18.0),
            80,
            24,
        );
        assert_eq!(cell, (1, 0)); // (109-100)/9 = 1.0, (66-50)/18 = 0.888 -> row 0
    }

    #[test]
    fn pixel_to_cell_clamps_to_the_last_row_and_column() {
        let bounds = Bounds {
            origin: point(px(0.0), px(0.0)),
            size: gpui::size(px(720.0), px(432.0)), // 80 cols x 24 rows, 9x18 cells
        };
        // Bounds::contains is inclusive on the far edge, so a click on the
        // exact bottom-right pixel must still resolve inside the grid, not
        // one cell past it.
        let cell = pixel_to_cell(
            point(px(719.0), px(431.0)),
            bounds,
            px(9.0),
            px(18.0),
            80,
            24,
        );
        assert_eq!(cell, (79, 23));
    }

    #[test]
    fn small_scroll_deltas_accumulate_instead_of_rounding_to_zero() {
        // Four 0.3-line trackpad events: 0.3, 0.6, 0.9 all round-to-zero
        // individually, but their running total (1.2 on the fourth) crosses
        // a whole line.
        let key = 1001;
        assert_eq!(accumulate_scroll_lines(key, 0.3), 0);
        assert_eq!(accumulate_scroll_lines(key, 0.3), 0);
        assert_eq!(accumulate_scroll_lines(key, 0.3), 0);
        assert_eq!(accumulate_scroll_lines(key, 0.3), 1);
    }

    #[test]
    fn scroll_accumulator_keeps_the_remainder_after_a_whole_line() {
        let key = 1002;
        // 1.6 lines in one event: 1 line now, 0.6 carried forward.
        assert_eq!(accumulate_scroll_lines(key, 1.6), 1);
        // Another 0.6 arrives: 1.2 total, 1 line out, 0.2 left over.
        assert_eq!(accumulate_scroll_lines(key, 0.6), 1);
    }

    #[test]
    fn scroll_accumulator_is_independent_per_terminal() {
        assert_eq!(accumulate_scroll_lines(2001, 0.9), 0);
        // A different terminal's own 0.9 doesn't inherit terminal 2001's
        // pending remainder.
        assert_eq!(accumulate_scroll_lines(2002, 0.9), 0);
    }

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

    #[test]
    fn no_scrollback_thumb_fills_track() {
        let (start, rows) = scrollbar_thumb_geometry(24, 0, 0);
        assert_eq!((start, rows), (0, 24));
    }

    #[test]
    fn at_bottom_thumb_sits_at_bottom() {
        let (start, rows) = scrollbar_thumb_geometry(24, 100, 0);
        assert!(rows < 24); // thumb shrinks once there's scrollback
        assert_eq!(start + rows, 24); // flush with the bottom of the track
    }

    #[test]
    fn at_top_thumb_sits_at_top() {
        let (start, _rows) = scrollbar_thumb_geometry(24, 100, 100);
        assert_eq!(start, 0);
    }

    // 24 rows, 100 lines of history, 18px cells, strip origin at y=0 --
    // matches `scrollbar_thumb_geometry`'s own test fixtures. Pins
    // `y_to_display_offset`'s output directly, and documents the sign
    // convention a scrollbar-drag delta must be computed against
    // (`target - offset`, not `offset - target` -- seeded by a real bug
    // caught in task review, where the subtraction was backwards and
    // scrolled away from the clicked position instead of toward it).
    fn strip_bounds() -> Bounds<Pixels> {
        Bounds {
            origin: point(px(0.0), px(0.0)),
            size: gpui::size(px(900.0), px(432.0)), // 24 * 18
        }
    }

    #[test]
    fn click_top_of_strip_targets_full_history() {
        let offset = y_to_display_offset(px(1.0), strip_bounds(), px(18.0), 24, 100);
        assert_eq!(offset, 100);
    }

    #[test]
    fn click_bottom_of_strip_targets_live_bottom() {
        let offset = y_to_display_offset(px(430.0), strip_bounds(), px(18.0), 24, 100);
        assert_eq!(offset, 0);
    }

    #[test]
    fn drag_delta_sign_points_toward_target() {
        // At offset=50 (mid-scroll), clicking the top of the strip must
        // produce a POSITIVE delta (toward more history) -- the exact case
        // the inverted-subtraction bug got backwards (it produced -50,
        // which scrolled to the live bottom instead of further back).
        let target = y_to_display_offset(px(1.0), strip_bounds(), px(18.0), 24, 100);
        let delta = target as i32 - 50_i32;
        assert!(
            delta > 0,
            "expected a positive (toward-history) delta, got {delta}"
        );
    }
}
