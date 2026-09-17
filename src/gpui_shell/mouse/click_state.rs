// gpui chrome migration (TD-GPUI-03 split): per-terminal click-tracking and
// scrollbar-drag state. Split out of the single `mouse.rs` (M1b) for the
// 400-line convention -- pure code motion, no logic changed.

use std::cell::RefCell;
use std::collections::HashMap;
use std::time::Instant;

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
pub(crate) fn forget_terminal(terminal_key: usize) {
    CLICK_STATE.with_borrow_mut(|states| states.remove(&terminal_key));
}

/// Update click count for multi-click detection at `cell`, keyed by
/// `terminal_key` (an `Rc<Terminal>` heap address, matching `rasterize`'s
/// `LAST_IMAGE` key). Returns 1 / 2 / 3 based on timing and position --
/// ported from `src/app/input/mod.rs`'s `register_click` as-is.
pub(super) fn register_click(terminal_key: usize, cell: (usize, usize)) -> u32 {
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
pub(super) fn set_dragging_scrollbar(terminal_key: usize, dragging: bool) {
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
pub(super) fn start_gesture(terminal_key: usize, cell: (usize, usize)) {
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
pub(super) fn mark_dragged_if_moved(terminal_key: usize, cell: (usize, usize)) {
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
pub(super) fn mark_dragged(terminal_key: usize) {
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
pub(super) fn take_dragged(terminal_key: usize) -> bool {
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
pub(super) fn accumulate_scroll_lines(terminal_key: usize, raw_lines: f32) -> i32 {
    CLICK_STATE.with_borrow_mut(|states| {
        let state = states.entry(terminal_key).or_insert_with(ClickState::new);
        state.scroll_accum += raw_lines;
        let lines = state.scroll_accum.trunc();
        state.scroll_accum -= lines;
        lines as i32
    })
}

/// Whether `terminal_key` is currently mid-drag on its scrollbar thumb.
pub(super) fn is_dragging_scrollbar(terminal_key: usize) -> bool {
    CLICK_STATE.with_borrow(|states| {
        states
            .get(&terminal_key)
            .is_some_and(|s| s.dragging_scrollbar)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
