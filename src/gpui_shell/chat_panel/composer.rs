// gpui chrome migration: the chat panel's composer row -- attached-file
// chips, the file-picker popup, and the text-input row itself. Split out of
// `render.rs` (M5a Task 3) to reclaim headroom under the 400-line convention
// after that task's header change (`◈`/`✦` backend distinction) pushed
// `render.rs` over it -- purely a file-boundary move, no behavior change.

use gpui::{div, prelude::*, px, IntoElement};

use crate::config::schema::ColorScheme;

use super::super::font_state;
use super::super::pane_view::to_rgba;
use super::ChatPanelView;

pub(super) fn render_composer(view: &ChatPanelView, colors: &ColorScheme) -> impl IntoElement {
    let mut root = div()
        .flex()
        .flex_col()
        .flex_shrink_0()
        .gap_1()
        .border_t_1()
        .border_color(to_rgba(colors.ui_border))
        .px_3()
        .py_2();

    if !view.panel.attached_files.is_empty() {
        let mut chips = div().flex().flex_row().flex_wrap().gap_1();
        for path in &view.panel.attached_files {
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| path.to_string_lossy().into_owned());
            chips = chips.child(
                div()
                    .px_2()
                    .py_0()
                    .rounded_md()
                    .bg(to_rgba(colors.ui_surface_hover))
                    .text_color(to_rgba(colors.ui_muted))
                    .text_size(px(11.0))
                    .child(format!("\u{1F4CE} {name}")),
            );
        }
        root = root.child(chips);
    }

    if view.panel.file_picker_open {
        let filtered = view.panel.filtered_picker_items();
        let mut picker_list = div()
            .id("chat-panel-file-picker")
            .flex()
            .flex_col()
            .max_h(px(160.0))
            .overflow_y_scroll()
            .border_1()
            .border_color(to_rgba(colors.ui_border))
            .rounded_md();
        for (idx, path) in filtered.iter().enumerate() {
            let is_cursor = idx == view.panel.file_picker_cursor;
            let is_attached = view.panel.attached_files.contains(path);
            let label = path.to_string_lossy().into_owned();
            let mut row = div()
                .px_2()
                .py_1()
                .text_size(px(font_state::font_size()))
                .text_color(to_rgba(colors.foreground));
            if is_cursor {
                row = row.bg(to_rgba(colors.ui_surface_active));
            }
            let prefix = if is_attached { "\u{2713} " } else { "  " };
            picker_list = picker_list.child(row.child(format!("{prefix}{label}")));
        }
        root = root.child(picker_list);
    }

    root.child(
        div()
            .flex()
            .h(px(28.0))
            .items_center()
            .font_family(font_state::font_family())
            .text_size(px(font_state::font_size()))
            .text_color(to_rgba(colors.foreground))
            .child(view.composer.clone()),
    )
    .child(
        div()
            .font_family(font_state::font_family())
            .text_size(px(font_state::font_size()))
            .text_color(to_rgba(colors.ui_muted))
            .child("Enter to send  ·  Tab to attach files  ·  /clear /skills /mcp /model /agent  ·  /q to close"),
    )
}
