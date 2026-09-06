// gpui chrome migration (M3b Task 3): the inline `Ctrl+Space` AI block.
//
// `crate::llm::ai_block::AiBlock` (118 lines, `Hidden -> Typing -> Loading ->
// Streaming -> Done/Error`) is engine-agnostic and used here directly,
// unmodified -- same "use it, don't copy it" rule Task 1/2 followed for
// `ChatPanel`. It is a genuinely separate surface from the chat panel: its
// own state machine, its own one-line composer, its own streaming channel
// (never `ChatPanelView`'s -- the two drawers can be open independently and
// must not share a `Sender`/`Receiver` pair or a stream landing in one would
// be silently consumed by the other's poll-drain).
//
// Two things this module builds in from the start rather than discovering
// via a dogfood report, per the M3b Task 3 brief:
//
// 1. Focus guard keyed on real focus, never on visibility. `composer_focused`
//    below is `is_focused(window)`, mirroring `ChatPanelView::composer_
//    focused` -- the same M3a Critical (a visibility-keyed guard freezes the
//    app once focus moves) applies here identically. `input.rs`'s
//    top-of-function guard and `render.rs`'s per-frame refocus guard both
//    key on this, never on `is_visible()` alone.
//
// 2. A provider stream error is never overwritten by an unconditional `Done`
//    sent after the loop -- see `submit`'s `errored` flag below, the same
//    fix commit 3fdf2fb applied to `ChatPanelView::submit` (`stream.rs`).
//    NOTE: the *original* wgpu build's `submit_ai_block_query`
//    (`src/app/ui/mod.rs:1280`) still has this exact bug -- found while
//    porting, not fixed there (out of scope, matches established practice
//    for wgpu-side gaps found mid-port).
//
// Error recovery: `AiBlock` has no `dismiss_error`-equivalent (unlike
// `ChatPanel`). It doesn't need one -- `close()` already resets `state` all
// the way to `Hidden`, strictly more recovery than a dismiss that just clears
// the error. `TextInputEvent::Cancel` (Escape) calls `close` unconditionally,
// mirroring the wgpu build's own Escape handler (`src/app/input/mod.rs`:
// `Key::Named(NamedKey::Escape) => ui.ai_block.close()`, reached in every
// state including `Error`).
//
// No-`Window` dismiss path: YES, unlike the chat panel's `/q` (which exists
// because of a slash-command layer this surface has no equivalent of).
// Enter after the response is `Done` runs the resolved command and closes
// the block (`run_ai_block_command` below), reached through the composer's
// `cx.subscribe` callback (`on_ai_block_composer_event`), which gpui hands
// no `Window`. So `close` follows `ChatPanelView::close`'s division of
// labor: it only clears state, and `render.rs`'s per-frame guard
// (`!is_visible() || ...`) reclaims root focus next frame regardless of what
// the composer's stale `FocusHandle` still claims.

use std::sync::Arc;

use gpui::{div, prelude::*, px, App, AppContext, Context, Div, Entity, Focusable, Rgba, Window};

use crate::config::schema::{ColorScheme, LlmConfig};
use crate::config::Config;
use crate::llm::ai_block::{AiBlock, AiState};
use crate::llm::chat_panel::AiEvent;
use crate::llm::shell_context::ShellContext;
use crate::llm::{ChatMessage, LlmProvider};

use super::pane_view::to_rgba;
use super::text_input::{TextInput, TextInputEvent};
use super::{font_state, GpuiShellRoot};

/// Bounded channel capacity -- same shape as `ChatPanelView`'s own
/// `AI_CHANNEL_CAP`, but a genuinely separate channel (see this module's own
/// doc comment on why the two surfaces must never share one).
const AI_CHANNEL_CAP: usize = 256;

/// Cap on `AiEvent`s drained per poll tick -- mirrors `ChatPanelView`'s own
/// `AI_POLL_CAP` and the wgpu build's `AI_POLL_CAP` (`src/app/ui/mod.rs`).
const AI_POLL_CAP: usize = 64;

/// Gpui-side state for the inline AI block: the engine-agnostic `AiBlock`
/// itself, its one-line `TextInput` composer, the direct-provider backend,
/// and its own streaming channel.
pub struct AiBlockView {
    pub block: AiBlock,
    pub composer: Entity<TextInput>,
    llm_provider: Option<Arc<dyn LlmProvider>>,
    llm_init_error: Option<String>,
    ai_tx: crossbeam_channel::Sender<AiEvent>,
    ai_rx: crossbeam_channel::Receiver<AiEvent>,
    /// Aborted on a new submit AND on close (TD-MEM-12 parity, plus: without
    /// aborting on close, a request finishing after the block was dismissed
    /// would still drain into `append_token`/`mark_done`, silently flipping
    /// `state` back out of `Hidden` and "reopening" an already-closed block).
    in_flight: Option<tokio::task::JoinHandle<()>>,
}

impl AiBlockView {
    pub fn new(cx: &mut Context<GpuiShellRoot>, config: &Config) -> Self {
        let composer =
            cx.new(|cx| TextInput::new(cx, &config.colors, "", "Ask the AI to run a command…"));
        cx.subscribe(&composer, GpuiShellRoot::on_ai_block_composer_event)
            .detach();
        let (ai_tx, ai_rx) = crossbeam_channel::bounded(AI_CHANNEL_CAP);
        let mut view = Self {
            block: AiBlock::new(),
            composer,
            llm_provider: None,
            llm_init_error: None,
            ai_tx,
            ai_rx,
            in_flight: None,
        };
        view.rewire_provider(&config.llm);
        view
    }

    pub fn is_visible(&self) -> bool {
        self.block.is_visible()
    }

    /// Whether the composer `TextInput` currently holds gpui's global window
    /// focus. See this module's doc comment (point 1) for why every guard
    /// keys on this, never on `is_visible()`.
    pub fn composer_focused(&self, window: &Window, cx: &App) -> bool {
        self.composer.focus_handle(cx).is_focused(window)
    }

    /// Open the block and focus the composer, or close it. Closing does NOT
    /// move focus anywhere -- the caller (`input.rs`'s `Ctrl+Space` handler)
    /// does that, same division of labor `ChatPanelView::toggle` uses.
    pub fn toggle(&mut self, window: &mut Window, cx: &mut Context<GpuiShellRoot>) {
        if self.block.is_visible() {
            self.close(cx);
        } else {
            self.block.open();
            self.composer
                .update(cx, |input, cx| input.set_content("", cx));
            self.composer.focus_handle(cx).focus(window);
            cx.notify();
        }
    }

    /// Close without touching window focus -- reachable with no `Window` in
    /// hand (see this module's doc comment on the Enter-after-`Done` path).
    pub fn close(&mut self, cx: &mut Context<GpuiShellRoot>) {
        self.block.close();
        if let Some(handle) = self.in_flight.take() {
            handle.abort();
        }
        cx.notify();
    }

    /// Rebuild the active provider from `llm_config`, mirroring
    /// `ChatPanelView::rewire_provider` -- this surface has no `/model`
    /// command of its own to also call it from.
    pub fn rewire_provider(&mut self, llm_config: &LlmConfig) {
        (self.llm_provider, self.llm_init_error) = if llm_config.enabled {
            match crate::llm::build_provider(llm_config) {
                Ok(p) => (Some(p), None),
                Err(e) => (None, Some(format!("{e:#}"))),
            }
        } else {
            (None, None)
        };
    }

    /// Submit `block.query` (set by the caller just before this runs) to the
    /// configured provider. Ported from `submit_ai_block_query`
    /// (`src/app/ui/mod.rs:1280`) minus the `wakeup_proxy` sends (the poll
    /// tick is the wake here, no winit event loop to nudge).
    pub fn submit(&mut self, tokio_rt: &tokio::runtime::Runtime, cx: &mut Context<GpuiShellRoot>) {
        let query = self.block.query.trim().to_string();
        if query.is_empty() {
            return;
        }
        let Some(provider) = self.llm_provider.clone() else {
            let msg = self
                .llm_init_error
                .clone()
                .unwrap_or_else(|| "LLM is disabled in config.".into());
            self.block.mark_error(msg);
            cx.notify();
            return;
        };
        self.block.set_loading();

        let mut system = "You are a shell command generator. The user describes what they \
                           want to do in natural language. Reply with ONLY the shell command \
                           to run — no explanation, no markdown, no code fences."
            .to_string();
        if let Some(ctx) = ShellContext::load() {
            system.push_str(&format!(
                "\n\nShell context:\n{}",
                ctx.format_for_system_message()
            ));
        }
        let messages = vec![ChatMessage::system(system), ChatMessage::user(&query)];

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
                    // `state = Done`, so an unconditional `Done` here would
                    // run immediately after `mark_error`'s `Error` in the
                    // same poll-drain tick and silently overwrite it back --
                    // the error vanishes with no message shown. Same fix as
                    // commit 3fdf2fb (`ChatPanelView::submit`); see this
                    // module's doc comment for why the original wgpu build
                    // still has this bug and why it's not fixed there.
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
        cx.notify();
    }

    /// Drain up to `AI_POLL_CAP` pending events into `block`'s existing
    /// handlers. Returns whether anything changed, `poll.rs`'s cue to
    /// `cx.notify()`.
    pub fn drain_events(&mut self) -> bool {
        let mut changed = false;
        for _ in 0..AI_POLL_CAP {
            let Ok(event) = self.ai_rx.try_recv() else {
                break;
            };
            changed = true;
            match event {
                AiEvent::Token(tok) => self.block.append_token(&tok),
                AiEvent::Done => self.block.mark_done(),
                AiEvent::Error(msg) => self.block.mark_error(msg),
                // `LlmProvider::stream` never produces these -- matched only
                // for exhaustiveness against `AiEvent`, same reasoning as
                // `ChatPanelView::drain_events`.
                AiEvent::Usage { .. }
                | AiEvent::ToolStatus { .. }
                | AiEvent::ConfirmWrite { .. }
                | AiEvent::ConfirmRun { .. }
                | AiEvent::UndoState { .. } => {}
            }
        }
        changed
    }
}

impl GpuiShellRoot {
    /// Wired once, from `AiBlockView::new`, onto the composer's
    /// `TextInputEvent` stream.
    pub(super) fn on_ai_block_composer_event(
        &mut self,
        _composer: Entity<TextInput>,
        event: &TextInputEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            TextInputEvent::Submit => self.handle_ai_block_submit(cx),
            TextInputEvent::Cancel => self.ai_block.close(cx),
        }
    }

    /// `Enter` in the block's composer: submit the typed query while
    /// `Typing`, run the resolved command while `Done`, otherwise a no-op --
    /// mirrors the wgpu build's own two-armed `if/else if` in `on_key_down`
    /// (`src/app/input/mod.rs:469-473`) exactly, including the silent no-op
    /// for every other state (Loading/Streaming/Error).
    fn handle_ai_block_submit(&mut self, cx: &mut Context<Self>) {
        match self.ai_block.block.state {
            AiState::Typing => {
                let text = self.ai_block.composer.read(cx).content().trim().to_string();
                if text.is_empty() {
                    return;
                }
                self.ai_block
                    .composer
                    .update(cx, |input, cx| input.set_content("", cx));
                self.ai_block.block.query = text;
                self.ai_block.submit(&self.tokio_rt, cx);
            }
            AiState::Done => self.run_ai_block_command(cx),
            _ => {}
        }
    }

    /// Write the block's resolved command to the focused pane's PTY and
    /// close. Ported from the wgpu build's `run_ai_block_command`
    /// (`src/app/ui/mod.rs:1338`) minus its `Mux` lookup -- this shell keys
    /// terminals by id directly (`self.terminals`/`self.tab_panes`).
    fn run_ai_block_command(&mut self, cx: &mut Context<Self>) {
        if let Some(cmd) = self.ai_block.block.command_to_run() {
            let mut data = cmd.into_bytes();
            data.push(b'\r');
            let active = self.tabs.active_index();
            let active_tid = self.tab_panes[active].focused_terminal;
            if let Some(terminal) = self.terminals.get(&active_tid) {
                terminal.write_input(&data);
            }
        }
        self.ai_block.close(cx);
    }
}

/// Build the block's `div()` tree: a bar anchored to the bottom of the
/// focused pane's area (an `.absolute()` overlay -- see `render.rs`'s call
/// site, which wraps the pane area in `.relative()` for this to anchor
/// against). Callers own visibility (only called `when view.is_visible()`).
pub fn render_ai_block(view: &AiBlockView, colors: &ColorScheme) -> Div {
    let (body, body_color) = ai_block_body(&view.block, colors);

    let mut root = div()
        .absolute()
        .bottom_0()
        .left_0()
        .right_0()
        .flex()
        .flex_col()
        .gap_1()
        .bg(to_rgba(colors.background))
        .border_t_1()
        .border_color(to_rgba(colors.ui_accent))
        .px_3()
        .py_2()
        .font_family(font_state::font_family())
        .text_size(px(font_state::font_size()))
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .text_color(to_rgba(colors.ui_accent))
                .child("⚡ AI"),
        )
        .child(
            div()
                .flex()
                .items_center()
                .h(px(24.0))
                .gap_2()
                .text_color(to_rgba(colors.foreground))
                .child(div().text_color(to_rgba(colors.ui_accent)).child("›"))
                .child(view.composer.clone()),
        );

    if let Some(body) = body {
        root = root.child(div().text_color(body_color).child(body));
    }

    root.child(
        div()
            .text_color(to_rgba(colors.ui_muted))
            .child(ai_block_hint(&view.block.state)),
    )
}

/// The response line's content and color, per state -- mirrors the wgpu
/// build's `build_ai_block_instances` response-row match
/// (`src/app/renderer/chat.rs:1495-1559`), minus its char-count truncation
/// and `word_wrap` calls (fixed terminal-cell concerns that don't apply to
/// gpui's proportional text, which wraps on its own) and its animated
/// spinner glyph (no `frame_counter` equivalent on this shell; a static
/// label is enough for this surface).
fn ai_block_body(block: &AiBlock, colors: &ColorScheme) -> (Option<String>, Rgba) {
    match &block.state {
        AiState::Hidden | AiState::Typing => (None, to_rgba(colors.foreground)),
        AiState::Loading => (Some("… thinking".to_string()), to_rgba(colors.ansi[3])),
        AiState::Streaming => (
            Some(format!("→ {}", block.response)),
            to_rgba(colors.ansi[3]),
        ),
        AiState::Done => match block.command_to_run() {
            Some(cmd) => (Some(format!("→ {cmd}")), to_rgba(colors.ui_success)),
            None => (Some(block.response.clone()), to_rgba(colors.ui_success)),
        },
        AiState::Error(msg) => (Some(format!("✗ {msg}")), to_rgba(colors.ansi[1])),
    }
}

fn ai_block_hint(state: &AiState) -> &'static str {
    match state {
        AiState::Hidden => "",
        AiState::Typing => "Enter: send   Esc: cancel",
        AiState::Loading | AiState::Streaming => "Esc: cancel",
        AiState::Done => "Enter: run \u{23ce}   Esc: dismiss",
        AiState::Error(_) => "Esc: dismiss",
    }
}
