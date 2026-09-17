// gpui chrome migration (M2 Task 6a): `render_status_bar`, split out of
// `status_bar.rs` for the 400-line convention.

use gpui::{div, prelude::*, px, Div, MouseButton, MouseDownEvent, Rgba};

use crate::config::schema::StatusBarColors;
use crate::gpui_shell::pane_view::to_rgba;

use super::{SegmentKind, StatusBar, StatusBarSegment};

// ── Rendering ────────────────────────────────────────────────────────────────

/// Render one status-bar row: one fully-rounded pill `div()` per segment,
/// gapped from its neighbors -- left-aligned segments, a flexible spacer,
/// then right-aligned segments, all sitting inside the row's own
/// rounded/bordered floating-card frame (visual-polish pass 2, 2026-09-17,
/// matching the approved mockup's separated-pill status treatment).
///
/// This replaces the previous powerline-chevron treatment (segments joined
/// into one continuous strip via `PillEdge::Start/Middle/End` grouping and
/// `StatusBar::pl_left_arrow`/`pl_right_arrow` connector glyphs), which a
/// live dogfood screenshot showed rendering as "objetos de formas
/// extrañas": those Nerd Font glyphs don't rasterize cleanly at UI text
/// size outside the terminal grid's own ligature-aware glyph path. Because
/// of that, `bar.style` (Powerline vs. Plain, a real user config option
/// that still matters to the wgpu binary's own true powerline rendering) is
/// intentionally not consulted here anymore -- gpui_shell always renders
/// the separated-pill style regardless of that setting.
pub fn render_status_bar(bar: &StatusBar, colors: &StatusBarColors) -> Div {
    let bar_bg_color = to_rgba(StatusBar::bar_bg(colors));
    let [br, bg, bb, _] = colors.fg_dim;
    let border_color = Rgba {
        r: br,
        g: bg,
        b: bb,
        a: 0.2,
    };

    let segment_div = |seg: &StatusBarSegment| -> Div {
        let clickable = matches!(seg.kind, SegmentKind::GitBranch | SegmentKind::ExitCode);
        let mut cell = div()
            .px_3()
            .py_1()
            .rounded_md()
            .bg(to_rgba(seg.bg))
            .text_color(to_rgba(seg.fg))
            .child(seg.text.clone());
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

    let left_row = div()
        .flex()
        .flex_row()
        .items_center()
        .gap_2()
        .children(bar.left.iter().map(segment_div));

    let right_row = div()
        .flex()
        .flex_row()
        .items_center()
        .gap_2()
        .children(bar.right.iter().map(segment_div));

    div()
        .flex()
        .flex_row()
        .items_center()
        .w_full()
        .flex_shrink_0()
        .px_3()
        .py_1()
        .rounded_lg()
        .border_1()
        .border_color(border_color)
        .bg(bar_bg_color)
        // Without this, this row's text falls back to gpui's own default UI
        // font -- a different (and differently metriced) typeface from the
        // terminal grid's own cosmic-text-rasterized glyphs sitting right
        // above it, which is exactly what a dogfood report flagged ("the
        // statusbar seems to be a completely different font"). `.font_family`
        // on a div cascades to its text children via gpui's TextStyle stack,
        // the same mechanism `.text_color` above already relies on.
        .font_family(crate::gpui_shell::font_state::font_family())
        .text_size(px(crate::gpui_shell::font_state::font_size()))
        .child(left_row)
        .child(div().flex_1())
        .child(right_row)
}
