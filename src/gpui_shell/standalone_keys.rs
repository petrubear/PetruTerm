// Standalone (non-leader) key handlers: Ctrl+Space, Cmd+F, Cmd+K, the
// confirm-card key guards, and undo-last-write.

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
    /// Mode-keyed on `panel.state`, not focus.
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

    /// The ACP write/run confirm card's own key guard, called from
    /// `input.rs`'s `on_key_down`. Returns `true` if the key was
    /// consumed. Mode-keyed on `panel.state`, same reasoning as `maybe_
    /// handle_confirm_action_key`.
    pub(super) fn maybe_handle_awaiting_confirm_key(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        use crate::llm::chat_panel::{ConfirmDisplay, PanelState};
        if !matches!(self.chat.panel.state, PanelState::AwaitingConfirm) {
            return false;
        }
        let key = event.keystroke.key.as_str();
        if key == "y" || key == "enter" {
            if let Some(tx) = self.chat.pending_confirm_tx.take() {
                if let Some(ConfirmDisplay::Run { cmd }) = self.chat.panel.confirm_display.as_ref()
                {
                    self.pending_pty_run = Some(cmd.clone());
                }
                let _ = tx.send(true);
                self.chat.panel.resolve_confirm();
            }
        } else if key == "n" || key == "escape" {
            if let Some(tx) = self.chat.pending_confirm_tx.take() {
                let _ = tx.send(false);
                self.chat.panel.resolve_confirm();
            }
        }
        cx.notify();
        true
    }

    /// `Leader a z` -- restore the most recently agent-written file's
    /// prior content. Ported from `UiManager::cmd_undo_last_write`
    /// (`src/app/ui/mod.rs:630+`).
    pub(super) fn undo_last_write(&mut self) {
        if let Some((path, content)) = self.chat.undo_stack.pop_back() {
            match std::fs::write(&path, &content) {
                Ok(()) => {
                    let msg = format!("Restored: {}", path.display());
                    self.chat
                        .panel
                        .messages
                        .push(crate::llm::ChatMessage::assistant(msg));
                }
                Err(e) => {
                    log::error!("undo write {}: {e}", path.display());
                    let msg = format!("Undo failed: {e}");
                    self.chat
                        .panel
                        .messages
                        .push(crate::llm::ChatMessage::assistant(msg));
                }
            }
        }
    }
}
