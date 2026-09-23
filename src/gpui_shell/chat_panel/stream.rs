// Streams provider/ACP `AiEvent`s into the panel; composer submit builds a
// `PromptAddendum` from skills/steering.

use gpui::{Context, Entity};

use crate::llm::chat_panel::{AiEvent, PanelState};
use crate::llm::ChatMessage;

use super::super::text_input::{TextInput, TextInputEvent};
use super::super::GpuiShellRoot;
use super::ChatPanelView;

/// Cap on `AiEvent`s drained per poll tick -- mirrors the wgpu build's own
/// `AI_POLL_CAP` (`src/app/ui/mod.rs`), so a fast stream can't starve the
/// rest of the 33ms tick's work (PTY reads, cursor blink, status bar refresh
/// -- see `poll.rs`'s own doc comment on everything else sharing that tick).
const AI_POLL_CAP: usize = 64;

impl ChatPanelView {
    /// Submit the current panel input to the ACP agent (if connected) or
    /// the configured direct provider otherwise.
    pub fn submit(
        &mut self,
        addendum: crate::llm::prompt_context::PromptAddendum,
        tokio_rt: &tokio::runtime::Runtime,
        cx: &mut Context<GpuiShellRoot>,
    ) {
        let Some(user_content) = self.panel.submit_input() else {
            return;
        };
        if let Some(name) = addendum.matched_skill.clone() {
            self.panel.matched_skill = Some(name);
        }

        if self.acp_session.is_some() {
            // `try_send_prompt` requires a `tokio::sync::mpsc::Sender<AiEvent>`
            // (see `AcpSession::try_send_prompt`'s real signature), but the
            // direct-provider path above -- and `drain_events` below, which
            // both backends share -- already reads from `self.ai_rx`, a
            // `crossbeam_channel::Receiver`. Rather than give `drain_events`
            // a second receiver to poll, bridge a fresh per-prompt tokio
            // channel back into the existing one: same "spawn a small
            // forwarding task" shape `backend.rs`'s own `spawn_acp_connect`
            // already uses for an unrelated result.
            let (bridge_tx, mut bridge_rx) = tokio::sync::mpsc::channel::<AiEvent>(256);
            let ai_tx_out = self.ai_tx.clone();
            tokio_rt.spawn(async move {
                while let Some(event) = bridge_rx.recv().await {
                    if ai_tx_out.send(event).is_err() {
                        break;
                    }
                }
            });
            let terminal_tx = self.acp_terminal_tx.clone();
            let prompt_text = if addendum.text.is_empty() {
                user_content
            } else {
                format!("{}\n\n{user_content}", addendum.text.trim_start())
            };
            let send_result = self.acp_session.as_mut().unwrap().try_send_prompt(
                prompt_text,
                bridge_tx,
                terminal_tx,
            );
            if let Err(e) = send_result {
                self.acp_session = None;
                self.panel
                    .mark_error(format!("ACP agent disconnected: {e:#}"));
            }
            cx.notify();
            return;
        }

        let Some(provider) = self.llm_provider.clone() else {
            let msg = self
                .llm_init_error
                .clone()
                .unwrap_or_else(|| "LLM is disabled in config.".into());
            self.panel.mark_error(msg);
            cx.notify();
            return;
        };
        self.panel.context_window = provider.context_window();

        let mut system_prompt = crate::config::load_system_prompt();
        system_prompt.push_str(&addendum.text);
        system_prompt.push('\n');
        system_prompt.push('\n');
        system_prompt.push_str(crate::llm::agent_action::system_prompt_instructions());
        let mut messages = vec![ChatMessage::system(system_prompt)];
        messages.extend(self.panel.messages.iter().cloned());

        // TD-MEM-12 parity: cancel any previous in-flight stream before
        // starting a new one.
        if let Some(handle) = self.in_flight.take() {
            handle.abort();
        }
        let tx = self.ai_tx.clone();
        self.in_flight = Some(tokio_rt.spawn(async move {
            use futures_util::StreamExt;
            match provider.stream(messages).await {
                Err(e) => {
                    let _ = tx.send(AiEvent::Error(e.to_string()));
                }
                Ok(mut stream) => {
                    // `errored` matters: `mark_done` unconditionally sets
                    // `state = Idle`, so an unconditional `Done` here would
                    // run immediately after `mark_error`'s `Error` in the
                    // same poll-drain tick and silently overwrite it back to
                    // Idle -- the error vanishes with no message shown and
                    // no way to tell the request ever failed. Found by a
                    // Codex trial review; the earlier Escape-recovers-a-
                    // stuck-panel fix only covered a request that fails
                    // before any token arrives, not one that fails partway
                    // through.
                    let mut errored = false;
                    while let Some(result) = stream.next().await {
                        match result {
                            Ok(tok) => {
                                let _ = tx.send(AiEvent::Token(tok));
                            }
                            Err(e) => {
                                let _ = tx.send(AiEvent::Error(e.to_string()));
                                errored = true;
                                break;
                            }
                        }
                    }
                    if !errored {
                        let _ = tx.send(AiEvent::Done);
                    }
                }
            }
        }));
        // `submit_input()` above already flipped `panel.state` to `Loading`
        // synchronously -- notify now so the header shows it immediately
        // rather than waiting for the next 33ms poll tick.
        cx.notify();
    }

    /// Drain up to `AI_POLL_CAP` pending events, feeding each to `panel`'s
    /// existing handlers. Returns whether anything changed, which is
    /// `poll.rs`'s cue to `cx.notify()`.
    pub fn drain_events(&mut self) -> bool {
        let mut changed = false;
        for _ in 0..AI_POLL_CAP {
            let Ok(event) = self.ai_rx.try_recv() else {
                break;
            };
            changed = true;
            match event {
                AiEvent::Token(tok) => self.panel.append_token(&tok),
                AiEvent::Done => self.panel.mark_done(),
                AiEvent::Error(msg) => self.panel.mark_error(msg),
                // `LlmProvider::stream` never produces a `Usage` event (only
                // `agent_step`, the tool-calling path, returns usage stats)
                // -- matched so this stays exhaustive against `AiEvent`,
                // harmless no-op if one is ever sent down this channel.
                AiEvent::Usage { .. } => {}
                AiEvent::ToolStatus { tool, path, done } => {
                    self.panel.set_tool_status(&tool, &path, done);
                }
                AiEvent::ConfirmWrite { display, result_tx } => {
                    self.panel.mark_awaiting_confirm(display);
                    self.pending_confirm_tx = Some(result_tx);
                }
                AiEvent::ConfirmRun { cmd, result_tx } => {
                    self.panel
                        .mark_awaiting_confirm(crate::llm::chat_panel::ConfirmDisplay::Run { cmd });
                    self.pending_confirm_tx = Some(result_tx);
                }
                AiEvent::UndoState { path, content } => {
                    if self.undo_stack.len() >= super::UNDO_STACK_CAP {
                        self.undo_stack.pop_front();
                    }
                    self.undo_stack.push_back((path, content));
                }
            }
        }
        changed
    }
}

impl GpuiShellRoot {
    /// Wired once, from `ChatPanelView::new`, onto the composer's
    /// `TextInputEvent` stream -- the same `cx.subscribe` shape
    /// `begin_tab_rename` uses for the tab-rename editor (`rename.rs`).
    pub(super) fn on_composer_event(
        &mut self,
        _composer: Entity<TextInput>,
        event: &TextInputEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            TextInputEvent::Submit => self.handle_chat_composer_submit(cx),
            // Mirrors the wgpu build's own Escape handler exactly
            // (`src/app/input/mod.rs:591-595`): dismiss a stuck error so the
            // panel can accept input again, otherwise just give up focus
            // (`TextInput::cancel` already called `window.blur()` before
            // this ran). Without the dismiss half, an errored request left
            // `ChatPanel::state` in `PanelState::Error` forever --
            // `is_idle()` never true again -- and `handle_chat_composer_
            // submit`'s own top-of-function gate then silently swallowed
            // every future Enter, /q included, with no way back in except
            // clicking the terminal and reopening the panel from scratch.
            TextInputEvent::Cancel => {
                if matches!(self.chat.panel.state, PanelState::Error(_)) {
                    self.chat.panel.dismiss_error();
                    cx.notify();
                }
            }
        }
    }

    /// `Enter` in the composer: submit as a query, or dispatch a `/`
    /// command. Only acts while the panel is idle -- mirrors the wgpu
    /// build's own gate around both branches (`src/app/input/mod.rs`'s
    /// `ui.panel().is_idle()` check), so a stray Enter during an in-flight
    /// request or an unresolved error can't discard the user's draft (the
    /// composer is left untouched below when this returns early) or race
    /// the active stream.
    fn handle_chat_composer_submit(&mut self, cx: &mut Context<Self>) {
        if !self.chat.panel.is_idle() {
            return;
        }
        let text = self.chat.composer.read(cx).content().trim().to_string();
        if text.is_empty() {
            return;
        }
        self.chat
            .composer
            .update(cx, |input, cx| input.set_content("", cx));
        if text.starts_with('/') {
            self.handle_slash_command(&text, cx);
        } else {
            // `ChatPanelView` has no access to `skill_manager`/`steering_manager`
            // (they live on `GpuiShellRoot`, a different struct) -- built here,
            // where both are in scope, and passed down into `submit` rather
            // than reached for from inside it.
            let addendum = crate::llm::prompt_context::build_prompt_addendum(
                &self.skill_manager,
                &self.steering_manager,
                self.chat.panel.matched_skill.as_deref(),
                &text,
                &self.chat.panel.attached_files,
            );
            self.chat.panel.set_input(text);
            self.chat.submit(addendum, &self.tokio_rt, cx);
        }
    }
}
