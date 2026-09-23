// The AI chat panel's gpui presentation. `crate::llm::chat_panel::ChatPanel`
// holds the panel's logic (history, input, markdown wrap caching); this
// module adds the gpui state (`composer`, `visible`, provider, event
// channel) and the `div()` tree (`render.rs`). There is ONE global panel,
// not one per pane. Composer submit goes through the `cx.subscribe` hook
// in `stream.rs` (`on_composer_event`); `poll.rs` drains events.

mod backend;
mod composer;
mod confirm;
mod file_picker;
mod header;
pub(super) mod markdown;
mod render;
mod slash_command;
mod stream;

pub use render::{render_chat_panel, ChatPillCallback, PANEL_WIDTH_PX};

use std::sync::Arc;

use gpui::{App, AppContext, Context, Entity, Focusable, Window};

use crate::config::schema::LlmConfig;
use crate::config::Config;
use crate::llm::chat_panel::{AiEvent, ChatPanel};
use crate::llm::LlmProvider;

use super::text_input::TextInput;
use super::GpuiShellRoot;

/// Wide enough that `parse_markdown`'s own char-count wrapping never fires --
/// gpui wraps instead. Shared between `render.rs` (the live streaming
/// buffer, reparsed fresh every frame -- it changes every token, so there is
/// nothing to cache) and `sync_markdown_cache` below (settled messages,
/// cached via `ChatPanel::ensure_wrap_cache`/`wrapped_message` -- the same
/// mechanism the wgpu build uses for exactly this cost, TD-PERF-05). Defined
/// here, not in `render.rs`, because both need it and this is their nearest
/// common ancestor module.
const MARKDOWN_WRAP_WIDTH: usize = 100_000;

/// Bounded channel capacity for the AI event stream -- same shape as the
/// wgpu build's own `crossbeam_channel::bounded(256)` (`src/app/ui/mod.rs`),
/// minus the `(panel_id, event)` tuple: there is one global panel, so a
/// bare `AiEvent` is all `stream.rs`'s channel carries.
const AI_CHANNEL_CAP: usize = 256;

/// Cap on `undo_stack`'s size -- oldest entry evicted past this, matching
/// `UiManager::cmd_undo_last_write`'s own `MAX_UNDO = 10`.
const UNDO_STACK_CAP: usize = 10;

/// Gpui-side state for the chat panel: the engine-agnostic `ChatPanel`
/// itself, the `TextInput` entity that is its composer, and whether the
/// drawer is currently open.
///
/// `visible` deliberately duplicates `panel.is_visible()` (which reads
/// `state != PanelState::Hidden`) rather than being derived from it --
/// `render.rs`'s layout guard (`is_visible()`) and focus guard
/// (`composer_focused()`) both need a cheap, always-current answer without
/// reaching into `ChatPanel`'s state machine, and keeping this as a real
/// field (rather than a method delegating into `panel`) keeps `ChatPanelView`
/// the single place that knows how "open" is represented on the gpui side.
pub struct ChatPanelView {
    pub panel: ChatPanel,
    pub composer: Entity<TextInput>,
    visible: bool,
    /// The active direct-provider backend, rebuilt by
    /// `rewire_provider` at construction, on `/model`, and on config
    /// hot-reload (`poll.rs`). `None` with `llm_init_error` set describes
    /// why: disabled in config, or `build_provider` failed (e.g. a missing
    /// API key) -- mirrors the wgpu build's own `llm_provider`/
    /// `llm_init_error` pair (`src/app/ui/mod.rs`), just owned here instead
    /// of on the shell root.
    llm_provider: Option<Arc<dyn LlmProvider>>,
    llm_init_error: Option<String>,
    /// Streaming event channel -- see `AI_CHANNEL_CAP` for why this is a
    /// bare `AiEvent` rather than the wgpu build's `(panel_id, event)`.
    ai_tx: crossbeam_channel::Sender<AiEvent>,
    ai_rx: crossbeam_channel::Receiver<AiEvent>,
    /// The in-flight streaming task, if any -- aborted when a new query is
    /// submitted (`stream.rs::submit`), mirroring the wgpu build's own
    /// `streaming_handle.abort()` (TD-MEM-12).
    in_flight: Option<tokio::task::JoinHandle<()>>,
    /// The file picker's async directory-scan channel -- see
    /// `file_picker.rs`'s `open_file_picker_async`/`poll_file_scan`.
    file_scan_rx: Option<crossbeam_channel::Receiver<Vec<std::path::PathBuf>>>,
    /// Oneshot channel to answer the ACP agent's own `session/
    /// requestPermission`/`fs/write_text_file` request once the user
    /// presses y/n. `None` while no confirm card is showing.
    pub(super) pending_confirm_tx: Option<tokio::sync::oneshot::Sender<bool>>,
    /// Saved (path, original content) pairs for `Leader a z` -- newest
    /// last, capped at `UNDO_STACK_CAP`.
    pub(super) undo_stack: std::collections::VecDeque<(std::path::PathBuf, String)>,
    /// The connected ACP agent session, if `config.llm.backend == Agent`
    /// and the connect succeeded. `None` in Provider mode, or while a
    /// connect is still pending/failed.
    pub(super) acp_session: Option<crate::llm::acp::AcpSession>,
    /// In-flight ACP connect attempt -- see `backend.rs`'s own doc
    /// comment.
    acp_pending_connect:
        Option<tokio::sync::oneshot::Receiver<Result<crate::llm::acp::AcpSession, String>>>,
    /// Sender half of the ACP terminal-request bridge -- cloned into each
    /// ACP prompt's `try_send_prompt` call as `terminal_tx`. The
    /// receiver half is drained by `GpuiShellRoot::handle_acp_terminal_
    /// requests` (`acp_bridge.rs`), called from `poll.rs`'s own tick.
    /// `tokio::sync::mpsc`, not `crossbeam_channel` -- `AcpSession::try_
    /// send_prompt` requires this exact channel type.
    pub(super) acp_terminal_tx:
        tokio::sync::mpsc::Sender<crate::llm::acp::terminal::AcpTerminalRequest>,
    /// `pub(super)`, not private, despite the doc comment above describing
    /// this as "the receiver half" of an otherwise-private-looking pair --
    /// `GpuiShellRoot::handle_acp_terminal_requests` (`acp_bridge.rs`) reads
    /// it directly via `self.chat.acp_terminal_rx.try_recv()`, and that
    /// function lives outside the `chat_panel` module, so the field needs
    /// the same outward visibility `acp_terminal_tx` already has.
    pub(super) acp_terminal_rx:
        tokio::sync::mpsc::Receiver<crate::llm::acp::terminal::AcpTerminalRequest>,
    /// Tracks the message list's scroll position across renders (`ScrollHandle`
    /// wraps `Rc<RefCell<..>>`, so a shared `&` reference is enough to read
    /// it or call `scroll_to_bottom()` -- no `&mut self` needed at render
    /// time). `render.rs`'s `render_message_list` reads it each frame to
    /// decide whether new content (a streamed token, a finished message)
    /// should pull the view back down, the same "stick to bottom unless the
    /// user scrolled away to read history" behavior every chat UI has.
    pub(super) scroll_handle: gpui::ScrollHandle,
}

impl ChatPanelView {
    pub fn new(
        cx: &mut Context<GpuiShellRoot>,
        config: &Config,
        tokio_rt: &tokio::runtime::Runtime,
    ) -> Self {
        let composer = cx.new(|cx| TextInput::new(cx, &config.colors, "", "Ask anything…"));
        // Wired here, once, rather than per-render: the same `cx.subscribe`
        // shape `begin_tab_rename` uses for the tab-rename editor
        // (`rename.rs`), just set up at construction instead of on demand,
        // since the composer (unlike a rename editor) lives for the whole
        // session. `cx`'s type parameter is `GpuiShellRoot` here (this
        // constructor is called from `GpuiShellRoot::new`, passing the same
        // `cx` through), so `on_composer_event`'s `&mut GpuiShellRoot`
        // receiver matches what `subscribe` expects.
        cx.subscribe(&composer, GpuiShellRoot::on_composer_event)
            .detach();
        let (ai_tx, ai_rx) = crossbeam_channel::bounded(AI_CHANNEL_CAP);
        let (acp_terminal_tx, acp_terminal_rx) = tokio::sync::mpsc::channel(32);
        let mut view = Self {
            panel: ChatPanel::new(),
            composer,
            visible: false,
            llm_provider: None,
            llm_init_error: None,
            ai_tx,
            ai_rx,
            in_flight: None,
            file_scan_rx: None,
            pending_confirm_tx: None,
            undo_stack: std::collections::VecDeque::new(),
            acp_session: None,
            acp_pending_connect: None,
            acp_terminal_tx,
            acp_terminal_rx,
            scroll_handle: gpui::ScrollHandle::new(),
        };
        view.rewire_backend(config, tokio_rt);
        view
    }

    pub fn is_visible(&self) -> bool {
        self.visible
    }

    /// Whether the composer `TextInput` currently holds gpui's global
    /// window focus. Used by `render.rs`'s per-frame focus guard -- keyed on
    /// this, never on `is_visible()`: clicking the terminal moves gpui
    /// focus to the root while a visibility flag stays set, so a guard
    /// keyed on the flag would never hand focus back.
    pub fn composer_focused(&self, window: &Window, cx: &App) -> bool {
        self.composer.focus_handle(cx).is_focused(window)
    }

    /// Open the drawer and focus the composer, or close it. Closing does
    /// NOT move focus anywhere -- the caller (`leader_dispatch.rs`'s leader
    /// dispatch) does that, the same division of labor `end_tab_rename`
    /// uses: this method only owns the panel's own state, not who owns
    /// keyboard focus afterward.
    pub fn toggle(&mut self, window: &mut Window, cx: &mut Context<GpuiShellRoot>) {
        if self.visible {
            self.close(cx);
        } else {
            self.visible = true;
            self.panel.open();
            self.composer.focus_handle(cx).focus(window);
            cx.notify();
        }
    }

    /// Close the panel without touching window focus at all -- unlike
    /// `toggle`'s close branch (equivalent otherwise), this is reachable
    /// from `slash_command.rs`'s `/q` handler, which runs inside the composer's
    /// `cx.subscribe` callback and is handed no `Window`. That leaves the
    /// composer's `FocusHandle` reporting stale "focused" state (gpui's own
    /// `FocusId::is_focused` is just `window.focus == Some(id)` -- nothing
    /// clears it just because the focused element's div left the render
    /// tree, confirmed against gpui 0.2.2's `window.rs`), which is exactly
    /// why `render.rs`'s per-frame guard checks `!is_visible()` first: it
    /// reclaims root focus on the very next frame regardless of what
    /// `composer_focused` still (incorrectly) claims. `Leader a a`'s close
    /// path (`leader_dispatch.rs`, via `toggle`) calls this too, then fixes focus
    /// itself since it DOES have a `Window` -- that inline fix and this
    /// guard are redundant with each other by design, not a gap in either.
    pub fn close(&mut self, cx: &mut Context<GpuiShellRoot>) {
        self.visible = false;
        self.panel.close();
        cx.notify();
    }

    /// Rebuild the active provider from `llm_config`. Called at
    /// construction, by `/model` (`stream.rs`), and on every config
    /// hot-reload (`poll.rs`) -- mirrors the wgpu build's own
    /// `rewire_llm_provider` (`src/app/ui/providers.rs`), minus the
    /// `panel_width_cols`/`system_prompt`/skill-and-steering-manager reload
    /// that call also does.
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

    /// Populate `panel`'s wrapped-line cache for every settled message.
    /// Called once per frame from `gpui_shell::render` (which has the `&mut
    /// self` this needs), BEFORE the read-only `render_chat_panel` reads it
    /// -- the same "populate, then render" split the wgpu build's own
    /// `mod.rs`/`chat.rs` pair uses (see `render.rs`'s
    /// `render_message_list` doc comment for the cache-miss cost this
    /// avoids). A no-op once already synced at the current message count:
    /// `ensure_wrap_cache` only wraps messages appended since the last call,
    /// so calling this every frame while streaming (dozens of times a
    /// second) costs nothing extra once the settled history is caught up --
    /// only a newly-completed message ever triggers real work here.
    pub fn sync_markdown_cache(&mut self) {
        self.panel.ensure_wrap_cache(MARKDOWN_WRAP_WIDTH);
    }
}

impl GpuiShellRoot {
    /// The file picker's own key guard, called from `input.rs`'s
    /// `on_key_down`. Returns `true` if the key was consumed. Keyed on
    /// `self.chat.panel.file_picker_open` (a mode flag), not
    /// `is_focused(window)` -- see this plan's own Global Constraints for
    /// why that's correct here, matching `InfoOverlay`'s own precedent.
    pub(super) fn maybe_handle_file_picker_key(
        &mut self,
        event: &gpui::KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.chat.panel.file_picker_open {
            return false;
        }
        let key = event.keystroke.key.as_str();
        if key == "escape" || key == "tab" {
            self.chat.panel.close_file_picker();
        } else if key == "enter" {
            let cwd = self.cached_cwd.clone().unwrap_or_default();
            let filtered: Vec<std::path::PathBuf> = self
                .chat
                .panel
                .filtered_picker_items()
                .into_iter()
                .cloned()
                .collect();
            self.chat.panel.picker_confirm(&cwd, &filtered);
        } else if key == "up" {
            self.chat.panel.picker_move_up();
        } else if key == "down" {
            let len = self.chat.panel.filtered_picker_items().len();
            self.chat.panel.picker_move_down(len);
        } else if key == "backspace" {
            self.chat.panel.picker_backspace();
        } else if !event.keystroke.modifiers.platform && !event.keystroke.modifiers.control {
            if key == "space" {
                self.chat.panel.picker_type_char(' ');
            } else if key.chars().count() == 1 {
                self.chat
                    .panel
                    .picker_type_char(key.chars().next().unwrap());
            }
        }
        cx.notify();
        true
    }
}
