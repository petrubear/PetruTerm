// gpui chrome migration (M2 Task 6a): `render_status_bar`, split out of
// `status_bar.rs` for the 400-line convention.

use gpui::{div, prelude::*, Div, MouseButton, MouseDownEvent};

use crate::config::schema::{StatusBarColors, StatusBarStyle};
use crate::gpui_shell::pane_view::to_rgba;

use super::{SegmentKind, StatusBar, StatusBarSegment};

// ── Rendering ────────────────────────────────────────────────────────────────

/// Render one status-bar row: one `div()` per segment, left-aligned segments
/// then a flexible spacer then right-aligned segments. Replaces the wgpu
/// renderer's pixel-column math (`click_kind`/`left_sep_width`/
/// `right_sep_width`, dropped from this port) with gpui's own flex layout --
/// the same simplification the tab bar already got (`tabs::render_tab_bar`).
///
/// Git-branch and exit-code segments get a click target (`cursor_pointer` +
/// `on_mouse_down`) reserved for the branch picker / exit-info context menu
/// (both M4, command-palette era) -- deliberately a no-op for now, per this
/// task's brief.
pub fn render_status_bar(bar: &StatusBar, colors: &StatusBarColors) -> Div {
    let powerline = bar.style == StatusBarStyle::Powerline;
    let bar_bg_color = to_rgba(StatusBar::bar_bg(colors));
    let sep_fg = to_rgba(colors.fg_dim);

    let segment_div = |seg: &StatusBarSegment| -> Div {
        let clickable = matches!(seg.kind, SegmentKind::GitBranch | SegmentKind::ExitCode);
        let cell = div()
            .bg(to_rgba(seg.bg))
            .text_color(to_rgba(seg.fg))
            .child(seg.text.clone());
        if clickable {
            cell.cursor_pointer()
                .on_mouse_down(MouseButton::Left, |_: &MouseDownEvent, _window, _cx| {})
        } else {
            cell
        }
    };

    let mut left_row = div().flex().flex_row().items_center();
    for (i, seg) in bar.left.iter().enumerate() {
        if i > 0 {
            let prev_bg = bar.left[i - 1].bg;
            left_row = left_row.child(if powerline {
                div()
                    .text_color(to_rgba(prev_bg))
                    .bg(to_rgba(seg.bg))
                    .child(StatusBar::pl_left_arrow())
            } else {
                div().text_color(sep_fg).bg(to_rgba(seg.bg)).child(" › ")
            });
        }
        left_row = left_row.child(segment_div(seg));
    }

    let mut right_row = div().flex().flex_row().items_center();
    if powerline && !bar.right.is_empty() {
        right_row = right_row.child(
            div()
                .text_color(to_rgba(bar.right[0].bg))
                .bg(bar_bg_color)
                .child(StatusBar::pl_right_arrow()),
        );
    }
    for (i, seg) in bar.right.iter().enumerate() {
        right_row = right_row.child(segment_div(seg));
        if i + 1 < bar.right.len() {
            let next_bg = bar.right[i + 1].bg;
            right_row = right_row.child(if powerline {
                div()
                    .text_color(to_rgba(next_bg))
                    .bg(to_rgba(seg.bg))
                    .child(StatusBar::pl_right_arrow())
            } else {
                div().text_color(sep_fg).bg(bar_bg_color).child(" │ ")
            });
        }
    }

    div()
        .flex()
        .flex_row()
        .items_center()
        .w_full()
        .flex_shrink_0()
        .bg(bar_bg_color)
        // Without this, this row's text falls back to gpui's own default UI
        // font -- a different (and differently metriced) typeface from the
        // terminal grid's own cosmic-text-rasterized glyphs sitting right
        // above it, which is exactly what a dogfood report flagged ("the
        // statusbar seems to be a completely different font"). `.font_family`
        // on a div cascades to its text children via gpui's TextStyle stack,
        // the same mechanism `.text_color` above already relies on.
        .font_family(crate::gpui_shell::font_state::font_family())
        .text_size(gpui::px(crate::gpui_shell::font_state::font_size()))
        .child(left_row)
        .child(div().flex_1())
        .child(right_row)
}
