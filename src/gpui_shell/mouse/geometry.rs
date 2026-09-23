// Pure pixel/cell/scrollbar geometry helpers `register_mouse_handlers`
// builds on.

use alacritty_terminal::selection::SelectionType;
use gpui::{Bounds, Pixels, Point};

use super::SCROLLBAR_PX;

/// Map a click count to the alacritty selection type it starts -- ported
/// from `src/app/mod.rs`'s mapping as-is.
pub(super) fn selection_type_for_clicks(clicks: u32) -> SelectionType {
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
pub(super) fn in_scrollbar_strip(position: Point<Pixels>, bounds: Bounds<Pixels>) -> bool {
    position.x >= bounds.origin.x + bounds.size.width - SCROLLBAR_PX
}

/// Convert a Y pixel position to the scrollback `display_offset` it
/// represents, by inverting `scrollbar_thumb_geometry`'s `thumb_start`
/// formula around the thumb's vertical center -- so a click or drag
/// anywhere in the scrollbar strip centers the thumb under the pointer,
/// clamped to the track's ends. `thumb_rows`/`slack` don't depend on
/// `display_offset` in the forward formula, so they're computed once here
/// with an arbitrary offset (0) purely to get the track geometry.
pub(super) fn y_to_display_offset(
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

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{point, px};

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
