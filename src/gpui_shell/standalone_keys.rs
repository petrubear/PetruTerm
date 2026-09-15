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

use gpui::{Context, Focusable, KeyDownEvent, Window};

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

    /// `Cmd+K` -- clear the focused terminal's screen and scrollback.
    /// Shares its actual clear logic with the context menu's own `Clear`
    /// item (`context_menu.rs`'s `dispatch_context_action`) via
    /// `clear_active_terminal`, so the two can't drift apart.
    pub(super) fn clear_focused_terminal(&mut self, cx: &mut Context<Self>) {
        self.clear_active_terminal();
        cx.notify();
    }

    /// The inline-action confirm card's own key guard, called from
    /// `input.rs`'s `on_key_down`. Returns `true` if the key was consumed.
    /// Mode-keyed on `panel.state`, not focus -- see this milestone's own
    /// Global Constraints. Lives here rather than in `mod.rs` (where the M5a
    /// plan sketched it) purely for the 400-line convention -- `mod.rs` was
    /// already at the limit before this task's own `pending_agent_action`
    /// field addition, same budget pressure this file's own doc comment
    /// above already explains for its other three methods.
    pub(super) fn maybe_handle_confirm_action_key(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        use crate::llm::chat_panel::PanelState;
        if !matches!(self.chat.panel.state, PanelState::ConfirmAction(_)) {
            return false;
        }
        let key = event.keystroke.key.as_str();
        if key == "y" || key == "enter" {
            if let Some(action) = self.chat.panel.resolve_action_yes() {
                self.pending_agent_action = Some(action);
            }
        } else if key == "a" {
            self.chat.panel.auto_confirm_actions = true;
            if let Some(action) = self.chat.panel.resolve_action_yes() {
                self.pending_agent_action = Some(action);
            }
        } else if key == "n" || key == "escape" {
            self.chat.panel.resolve_action_no();
        }
        cx.notify();
        true
    }
}
