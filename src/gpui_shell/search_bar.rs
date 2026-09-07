// gpui chrome migration (M4b Task 2): the search bar's render tree and its
// own keyboard guard. State (`SearchBar`) is reused directly from
// `crate::ui::search_bar` -- see `mod.rs`'s own doc comment on the
// `search_bar` field for why. Deliberately non-modal (no backdrop, no
// `cx.stop_propagation()`): the terminal stays interactive underneath
// while this is open, unlike M4a's command palette.

use gpui::{div, prelude::*, px, App, Context, Entity, Focusable, KeyDownEvent, Window};

use crate::config::schema::ColorScheme;
use crate::ui::search_bar::SearchBar;

use super::font_state;
use super::pane_view::to_rgba;
use super::text_input::TextInput;
use super::GpuiShellRoot;

/// Build the search bar's `div()` tree: a small row anchored to the top of
/// the pane area, holding the query field and a match-count label.
pub fn render_search_bar(
    search: &SearchBar,
    query_input: &Entity<TextInput>,
    colors: &ColorScheme,
) -> impl IntoElement {
    div()
        .id("search-bar")
        .absolute()
        .top_0()
        .right_0()
        .flex()
        .flex_row()
        .items_center()
        .gap_2()
        .px_2()
        .py_1()
        .bg(to_rgba(colors.ui_surface))
        .border_1()
        .border_color(to_rgba(colors.ui_border))
        .font_family(font_state::font_family())
        .text_size(px(font_state::font_size()))
        .child(div().w(px(200.0)).child(query_input.clone()))
        .child(
            div()
                .text_size(px(11.0))
                .text_color(to_rgba(colors.ui_muted))
                .child(search.count_label()),
        )
}

impl GpuiShellRoot {
    /// True while the search query field genuinely holds keyboard focus --
    /// same shape as `palette_query_focused` (M4a's `palette.rs`).
    pub(super) fn search_query_focused(&self, window: &Window, cx: &App) -> bool {
        self.search_query.focus_handle(cx).is_focused(window)
    }

    /// The search bar's own key guard, called from `input.rs`'s
    /// `on_key_down`. Returns `true` if the key was consumed. Up/Down move
    /// to the prev/next match directly (`SearchBar::prev_match`/
    /// `next_match`, TextInput has no binding for either -- verified in
    /// M4a). Enter/Escape are TextInput's own bound `Submit`/`Cancel`
    /// actions, handled via `search_query`'s `cx.subscribe` callback in
    /// `mod.rs` instead, same mechanism M4a's command palette established.
    pub(super) fn maybe_handle_search_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.search_query_focused(window, cx) {
            return false;
        }
        match event.keystroke.key.as_str() {
            "down" => self.search_bar.next_match(),
            "up" => self.search_bar.prev_match(),
            _ => {}
        }
        cx.notify();
        true
    }

    /// Run the query if dirty, scroll to the current match if needed --
    /// ported verbatim from the wgpu build's own driver (`src/app/
    /// frame.rs`, lines 647-685), just moved from "once per poll tick" to
    /// "once per `render()` call" (this codebase has no separate per-tick
    /// hook the way `frame.rs` does, and `render()` already runs every
    /// frame the poll loop wakes for). Called from `render.rs`'s own top
    /// -- moved here (out of `render.rs` itself) to keep that file under
    /// the 400-line convention.
    pub(super) fn drive_search(&mut self) {
        if !self.search_bar.visible {
            return;
        }
        let active_ws = self.workspaces.active();
        let active_tid = active_ws.tab_panes[active_ws.tabs.active_index()].focused_terminal;
        let Some(terminal) = self.terminals.get(&active_tid).cloned() else {
            return;
        };
        if self.search_bar.dirty {
            let query = self.search_bar.query.clone();
            if query.is_empty() {
                self.search_bar.set_matches(Vec::new(), false);
            } else {
                let prev_query = self.search_bar.last_query.clone();
                let can_filter = !self.search_bar.matches.is_empty()
                    && !self.search_bar.matches_truncated
                    && query.starts_with(prev_query.as_str())
                    && !prev_query.is_empty();
                let (matches, truncated) = if can_filter {
                    crate::term::search::filter_matches(&terminal, &self.search_bar.matches, &query)
                } else {
                    crate::term::search::search_terminal(&terminal, &query)
                };
                self.search_bar.set_matches(matches, truncated);
            }
            self.search_bar.last_query = query;
            self.search_bar.dirty = false;
        }
        if self.search_bar.scroll_needed {
            if let Some(m) = self.search_bar.current_match().cloned() {
                let (disp_off, _) = terminal.scrollback_info();
                let screen_rows = terminal.rows.get() as i32;
                let target_offset = (screen_rows / 2 - m.grid_line).max(0) as usize;
                let delta = disp_off as i32 - target_offset as i32;
                if delta != 0 {
                    terminal.scroll_display(-delta);
                }
            }
            self.search_bar.scroll_needed = false;
        }
    }
}
