// gpui chrome migration (M2 Task 6a): `render_status_bar`, split out of
// `status_bar.rs` for the 400-line convention.

use gpui::{div, prelude::*, Div, MouseButton, MouseDownEvent};

use crate::config::schema::{StatusBarColors, StatusBarStyle};
use crate::gpui_shell::pane_view::to_rgba;

use super::{SegmentKind, StatusBar, StatusBarSegment};

// ── Rendering ────────────────────────────────────────────────────────────────

/// Which corners of a segment's own pill get rounded -- only the outer ends
/// of a left/right GROUP round; segments in the middle of a group, and the
/// powerline chevrons joining them, stay square so the group still reads as
/// one continuous pill with arrow caps, not a row of disconnected chips.
#[derive(Clone, Copy, PartialEq, Eq)]
enum PillEdge {
    /// Only member of its group: round all four corners.
    Solo,
    /// First of 2+: round the left corners only.
    Start,
    /// Neither first nor last of 2+: no rounding.
    Middle,
    /// Last of 2+: round the right corners only.
    End,
}

impl PillEdge {
    fn for_index(i: usize, len: usize) -> Self {
        match (i, len) {
            (_, 1) => PillEdge::Solo,
            (0, _) => PillEdge::Start,
            (i, len) if i + 1 == len => PillEdge::End,
            _ => PillEdge::Middle,
        }
    }

    fn apply(self, el: Div) -> Div {
        match self {
            PillEdge::Solo => el.rounded_md(),
            PillEdge::Start => el.rounded_l_md(),
            PillEdge::Middle => el,
            PillEdge::End => el.rounded_r_md(),
        }
    }
}

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

    // Padding + group-edge rounding turn each left/right group into a pill
    // (visual-polish pass, 2026-09-17) -- before this, segments were bare
    // colored rects flush against each other, no breathing room at all.
    let segment_div = |seg: &StatusBarSegment, edge: PillEdge| -> Div {
        let clickable = matches!(seg.kind, SegmentKind::GitBranch | SegmentKind::ExitCode);
        let mut cell = edge.apply(
            div()
                .px_3()
                .py_1()
                .bg(to_rgba(seg.bg))
                .text_color(to_rgba(seg.fg))
                .child(seg.text.clone()),
        );
        if seg.kind == SegmentKind::GitBranch {
            cell = cell.italic();
        }
        if clickable {
            cell.cursor_pointer()
                .on_mouse_down(MouseButton::Left, |_: &MouseDownEvent, _window, _cx| {})
        } else {
            cell
        }
    };

    let left_len = bar.left.len();
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
        left_row = left_row.child(segment_div(seg, PillEdge::for_index(i, left_len)));
    }

    let right_len = bar.right.len();
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
        right_row = right_row.child(segment_div(seg, PillEdge::for_index(i, right_len)));
        if i + 1 < right_len {
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
        .px_2()
        .py_1()
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
