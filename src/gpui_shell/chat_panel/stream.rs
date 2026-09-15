// gpui chrome migration (M3b Task 2, extended by M5a Task 5): streaming both
// the direct-provider LLM response and the ACP agent's `AiEvent` stream into
// the chat panel, plus composer submit and slash-command dispatch.
//
// M5a Task 5 wired `submit`'s ACP branch (`AcpSession::try_send_prompt`) and
// `drain_events`'s `AiEvent::ToolStatus` handler. Still deliberately NOT
// here (see the M3b plan's Scope): the remaining confirm-prompt surfaces
// that exist only to gate a tool call (`AiEvent::ConfirmWrite`/`ConfirmRun`/
// `UndoState`, and `ChatPanel::resolve_action_yes`/`resolve_action_no` for
// inline actions) -- completed in Task 6. `SkillManager`/`McpManager`/
// `SteeringManager`/`ShellContext` are likewise not wired: `/skills` and
// `/mcp` report their real (always empty) state below rather than
// pretending to a manager that doesn't exist, and the system message sent
// with every direct-provider query is just `crate::config::load_system_
// prompt()` -- no steering-file block, no skill-match injection, no
// shell-context paragraph, no attached-file content, all of which need one
// of those managers to produce.
//
// The wgpu build's `submit_ai_query` (`src/app/ui/mod.rs:794`) takes a
// `cwd: PathBuf` purely to sandbox tool execution (`execute_tool`'s
// `canon.starts_with(cwd)` check). `submit` below has no tools to sandbox,
// so it takes no `cwd` -- a deliberate narrowing of the plan's sketched
// `submit(&mut self, cwd, tokio_rt, cx)` signature, not an oversight.

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
    /// the configured direct provider otherwise -- see this module's doc
    /// comment for what's still deliberately excluded from both paths.
    pub fn submit(&mut self, tokio_rt: &tokio::runtime::Runtime, cx: &mut Context<GpuiShellRoot>) {
        let Some(user_content) = self.panel.submit_input() else {
            return;
        };

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
            let send_result = self.acp_session.as_mut().unwrap().try_send_prompt(
                user_content,
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

        let system_prompt = format!(
            "{}\n\n{}",
            crate::config::load_system_prompt(),
            crate::llm::agent_action::system_prompt_instructions()
        );
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
                // Confirm/undo surfaces for tool-calling -- still out of
                // scope here; completed in Task 6.
                AiEvent::ConfirmWrite { .. }
                | AiEvent::ConfirmRun { .. }
                | AiEvent::UndoState { .. } => {}
            }
        }
        changed
    }
}

impl GpuiShellRoot {
    /// Wired once, from `ChatPanelView::new`, onto the composer's
    /// `TextInputEvent` stream -- the same `cx.subscribe` shape
    /// `begin_tab_rename` uses for the tab-rename editor (`actions.rs`).
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
            self.chat.panel.set_input(text);
            self.chat.submit(&self.tokio_rt, cx);
        }
    }

    /// Slash-command dispatch. Ported from `src/app/ui/providers.rs`'s
    /// `handle_slash_command` (straight string dispatch plus
    /// `messages.push`) minus: the ACP reconnect its `/agent` branch does
    /// (no ACP session here to reconnect); `SkillManager`/`McpManager`
    /// (neither is wired -- `/skills`/`/mcp` report their real, always-empty
    /// state rather than pretending to a manager this shell doesn't have);
    /// and the `wakeup_proxy` parameter (the winit wake has no gpui
    /// equivalent and none is needed here either).
    fn handle_slash_command(&mut self, input: &str, cx: &mut Context<Self>) {
        let trimmed = input.trim_start_matches('/');
        let (cmd, args) = trimmed
            .split_once(' ')
            .map_or((trimmed, ""), |(c, a)| (c, a.trim()));

        match cmd {
            "q" | "quit" => {
                self.chat.close(cx);
                return;
            }
            "clear" | "reset" => {
                self.chat.panel.clear_messages();
            }
            "skills" => self.push_chat_message(
                "No skills loaded. Skill injection is not wired in this build \
                 (see the M3b plan's Scope -- deferred alongside the ACP/tool-calling \
                 surfaces)."
                    .to_string(),
            ),
            "mcp" => self.push_chat_message(
                "No MCP servers connected. MCP is not wired in this build \
                 (see the M3b plan's Scope -- deferred alongside the ACP/tool-calling \
                 surfaces)."
                    .to_string(),
            ),
            "model" => {
                use crate::config::schema::LlmBackend;
                let msg = match self.config.llm.backend {
                    LlmBackend::Agent => "Agent mode: use /agent to switch agents.".to_string(),
                    LlmBackend::Provider if args.is_empty() => {
                        format!(
                            "Active: {}:{}",
                            self.config.llm.provider, self.config.llm.model
                        )
                    }
                    LlmBackend::Provider => {
                        self.config.llm.model = args.to_string();
                        self.chat.rewire_backend(&self.config, &self.tokio_rt);
                        format!("Model set to '{args}'.")
                    }
                };
                self.push_chat_message(msg);
            }
            "agent" => {
                use crate::config::schema::{AcpAgentConfig, LlmBackend};
                let msg = match self.config.llm.backend {
                    LlmBackend::Provider => {
                        "Provider mode active. Use /model to change models.".to_string()
                    }
                    LlmBackend::Agent if args.is_empty() => {
                        match crate::config::llm_view::agent_display_name(
                            self.config.llm.agent.as_ref(),
                        ) {
                            Some(name) => format!("Active agent: {name}"),
                            None => {
                                "No agent configured. Set llm.agent.command in config.".to_string()
                            }
                        }
                    }
                    LlmBackend::Agent => {
                        if let Some(agent_cfg) = self.config.llm.agent.as_mut() {
                            agent_cfg.command = args.to_string();
                        } else {
                            self.config.llm.agent = Some(AcpAgentConfig {
                                command: args.to_string(),
                                args: vec![],
                                env: vec![],
                                display_name: None,
                            });
                        }
                        self.chat.acp_session = None;
                        self.chat.rewire_backend(&self.config, &self.tokio_rt);
                        format!("Agent set to '{args}'. Reconnecting...")
                    }
                };
                self.push_chat_message(msg);
            }
            _ => self.push_chat_message(format!(
                "Unknown command: /{cmd}. Try /clear, /skills, /mcp, /model, /agent or /quit."
            )),
        }
        cx.notify();
    }

    fn push_chat_message(&mut self, text: String) {
        self.chat.panel.messages.push(ChatMessage::assistant(text));
        self.chat.panel.dirty = true;
    }
}
