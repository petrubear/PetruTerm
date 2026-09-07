// gpui chrome migration (M4b Task 1 review): the two standalone
// modifier-combo toggles (`Ctrl+Space` for the inline AI block, `Cmd+F`
// for the search bar) -- neither is a leader chord, both were previously
// inline in `input.rs`'s own `on_key_down`. Moved here for the 400-line
// convention: Task 1's own Cmd+F guard pushed `input.rs` to 423 lines
// (`ai_block.rs`, the AI block's natural home, is already at 399 and has
// no room either). Same class of fix M4a's `palette.rs`/M3d's
// `sidebar_nav.rs` already established, just grouped by "standalone
// combo" rather than by feature, since neither surface alone is large
// enough to justify its own extraction file.

use gpui::{Context, Focusable, Window};

use super::GpuiShellRoot;

impl GpuiShellRoot {
    /// `Ctrl+Space` — toggle the inline AI block. `toggle` only ever moves
    /// focus TO the composer (opening); closing deliberately returns none,
    /// mirroring `LeaderAction::ToggleAiPanel`'s own division of labor
    /// (`leader_dispatch.rs`). This is the other half: send focus back to
    /// the terminal right here rather than waiting on `render()`'s guard,
    /// which can't tell "the block just closed" from "the composer still
    /// holds a stale focus handle".
    pub(super) fn toggle_ai_block(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.ai_block.toggle(window, cx);
        if !self.ai_block.is_visible() {
            window.focus(&self.focus_handle);
        }
        cx.notify();
    }

    /// `Cmd+F` — toggle the in-terminal search bar. Opening clears and
    /// focuses the query field; closing returns focus to the terminal --
    /// same division of labor `toggle_ai_block` above already uses.
    pub(super) fn toggle_search_bar(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.search_bar.visible {
            self.search_bar.close();
            window.focus(&self.focus_handle);
        } else {
            self.search_query
                .update(cx, |input, cx| input.set_content("", cx));
            self.search_bar.open();
            self.search_query.focus_handle(cx).focus(window);
        }
        cx.notify();
    }
}
