// gpui chrome migration (M3d Task 2): a scrollable, read-only content
// popup -- the activation target for every sidebar row this milestone
// adds (MCP server, skill, steering file) and, later, workspace details.
//
// Ported fresh from `src/ui/info_overlay.rs`'s tiny data model (title +
// parsed-markdown lines + scroll position), not copied verbatim: that
// version hand-rolls scroll as a `usize` line offset because the wgpu
// renderer has no real scroll container to delegate to. gpui does --
// `gpui::ScrollHandle` plus `.track_scroll()`/`.scroll_to_item()` -- so
// this version drives a real one instead of reimplementing scrolling.
//
// Deliberately modal, and deliberately the ONE guard in this codebase keyed
// on visibility instead of focus (see `input.rs`'s own doc comment on its
// guard for the full reasoning): its backdrop calls `cx.stop_propagation()`
// on every click (Step 4), so nothing behind it is reachable while it's
// open -- unlike the chat panel or AI block, which are deliberately
// non-modal and let the terminal stay interactive underneath them.

use gpui::{
    div, prelude::*, px, rgba, App, FontWeight, MouseButton, MouseDownEvent, ScrollHandle, Window,
};

use crate::config::schema::ColorScheme;
use crate::llm::markdown::{parse_markdown, AnnotatedLine, ParseState};

use super::chat_panel::markdown::render_line;
use super::font_state;
use super::pane_view::to_rgba;

/// Character width `parse_markdown` wraps content to -- matches the wgpu
/// build's own `CONTENT_WIDTH` (`src/app/mod.rs`'s `open_sidebar_info_
/// overlay`): wide enough that tool-schema JSON and skill prose read
/// naturally, still narrower than most terminal windows.
const CONTENT_WIDTH: usize = 72;

pub struct InfoOverlay {
    visible: bool,
    title: String,
    lines: Vec<AnnotatedLine>,
    scroll_handle: ScrollHandle,
    /// Which line `scroll_to_item` targets on the next j/k/arrow press --
    /// gpui's `ScrollHandle` tracks pixel offset, not "the Nth line", so
    /// this is the source of truth for "where the keyboard cursor is",
    /// translated into a `scroll_to_item` call each time it moves.
    cursor_line: usize,
}

impl InfoOverlay {
    pub fn new() -> Self {
        Self {
            visible: false,
            title: String::new(),
            lines: Vec::new(),
            scroll_handle: ScrollHandle::new(),
            cursor_line: 0,
        }
    }

    pub fn is_visible(&self) -> bool {
        self.visible
    }

    /// Open the overlay showing `content` (parsed as markdown, reusing the
    /// same `AnnotatedLine` model the chat panel's message list already
    /// uses) under `title`.
    #[allow(dead_code)] // first caller is Task 4's sidebar row activation
    pub fn open(&mut self, title: String, content: &str) {
        let mut state = ParseState::default();
        self.lines = parse_markdown(content, CONTENT_WIDTH, &mut state);
        self.title = title;
        self.cursor_line = 0;
        self.scroll_handle = ScrollHandle::new();
        self.visible = true;
    }

    pub fn close(&mut self) {
        self.visible = false;
    }

    pub fn scroll_down(&mut self) {
        let max = self.lines.len().saturating_sub(1);
        self.cursor_line = (self.cursor_line + 1).min(max);
        self.scroll_handle.scroll_to_item(self.cursor_line);
    }

    pub fn scroll_up(&mut self) {
        self.cursor_line = self.cursor_line.saturating_sub(1);
        self.scroll_handle.scroll_to_item(self.cursor_line);
    }
}

impl Default for InfoOverlay {
    fn default() -> Self {
        Self::new()
    }
}

/// Build the overlay's `div()` tree: a dimmed, window-covering backdrop
/// (blocking all mouse interaction with anything behind it -- see this
/// module's own doc comment) centered on a fixed-size content box.
pub fn render_info_overlay(overlay: &InfoOverlay, colors: &ColorScheme) -> impl IntoElement {
    let lines: Vec<_> = overlay
        .lines
        .iter()
        .map(|line| render_line(line, colors))
        .collect();

    div()
        .id("info-overlay-backdrop")
        .absolute()
        .inset_0()
        .flex()
        .items_center()
        .justify_center()
        .bg(rgba(0x0000_0099))
        .on_mouse_down(
            MouseButton::Left,
            |_: &MouseDownEvent, _window: &mut Window, cx: &mut App| {
                // Swallow clicks on the backdrop itself so they can't leak
                // through to whatever is positioned underneath -- this call is
                // what makes `is_visible()` a provably correct keyboard-guard
                // signal in `input.rs` (see that guard's own doc comment).
                cx.stop_propagation();
            },
        )
        .child(
            div()
                .id("info-overlay-content")
                .flex()
                .flex_col()
                .w(px(640.0))
                .h(px(480.0))
                .bg(to_rgba(colors.ui_surface))
                .border_1()
                .border_color(to_rgba(colors.ui_border))
                .on_mouse_down(
                    MouseButton::Left,
                    |_: &MouseDownEvent, _window: &mut Window, cx: &mut App| {
                        // Clicks inside the content box (e.g. selecting text)
                        // must not also register as a backdrop click.
                        cx.stop_propagation();
                    },
                )
                .child(
                    div()
                        .px_3()
                        .py_2()
                        .border_b_1()
                        .border_color(to_rgba(colors.ui_border))
                        .font_family(font_state::font_family())
                        .font_weight(FontWeight::BOLD)
                        .text_color(to_rgba(colors.foreground))
                        .child(overlay.title.clone()),
                )
                .child(
                    div()
                        .id("info-overlay-scroll")
                        .flex()
                        .flex_col()
                        .flex_1()
                        .min_h_0()
                        .overflow_y_scroll()
                        .track_scroll(&overlay.scroll_handle)
                        .px_3()
                        .py_2()
                        .gap_1()
                        .font_family(font_state::font_family())
                        .text_size(px(font_state::font_size()))
                        .children(lines),
                ),
        )
}
