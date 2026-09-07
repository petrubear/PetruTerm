// gpui chrome migration (M3b Task 1): the AI chat panel's gpui presentation.
//
// `crate::llm::chat_panel::ChatPanel` (919 lines, 12 passing tests, zero
// winit/wgpu references) already holds every bit of the panel's *logic* --
// conversation history, input editing, prompt history, markdown wrap
// caching. It is used here directly, unmodified: this module only adds the
// gpui-side presentation state (`composer`, `visible`) and the `div()` tree
// (`render.rs`) that paints it, replacing every line of
// `src/app/renderer/chat.rs`'s pixel math -- the same "port the logic,
// rewrite the painting" split `status_bar`/`tabs` already went through (see
// `status_bar/mod.rs`'s header comment).
//
// Per the M3 design's §1, there is ONE global panel, not one per pane -- the
// wgpu build's `panel_id`/`set_active_terminal` plumbing is dead code
// (`set_active_terminal` is an empty function; `active_panel_id()` returns a
// hardcoded `0`) and is deliberately not reproduced here.
//
// M3b Task 2 (`stream.rs`) adds streaming and slash commands on top of
// Task 1's shell: the LLM provider, the AI event channel, and the composer's
// `TextInputEvent` subscription all live on this struct too, so `submit`/
// `drain_events` (consumed by `input.rs`'s composer-submit path and
// `poll.rs`'s drain tick) have everything they need without reaching back
// into `GpuiShellRoot` for anything but `config`/`tokio_rt` at the call site.

pub(super) mod markdown;
mod render;
mod stream;

pub use render::{render_chat_panel, PANEL_WIDTH_PX};

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
/// minus the `(panel_id, event)` tuple: per the M3 design's §3.5 there is one
/// global panel, so a bare `AiEvent` is all `stream.rs`'s channel carries.
const AI_CHANNEL_CAP: usize = 256;

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
    /// The active direct-provider backend (Task 2), rebuilt by
    /// `rewire_provider` at construction, on `/model`, and on config
    /// hot-reload (`poll.rs`). `None` with `llm_init_error` set describes
    /// why: disabled in config, or `build_provider` failed (e.g. a missing
    /// API key) -- mirrors the wgpu build's own `llm_provider`/
    /// `llm_init_error` pair (`src/app/ui/mod.rs`), just owned here instead
    /// of on the shell root, since `submit`'s plan-specified signature
    /// (`&mut self, tokio_rt, cx` -- see `stream.rs`) has no room to receive
    /// it as a parameter.
    llm_provider: Option<Arc<dyn LlmProvider>>,
    llm_init_error: Option<String>,
    /// Streaming event channel -- see `stream.rs`'s doc comment for why this
    /// is a bare `AiEvent` rather than the wgpu build's `(panel_id, event)`.
    ai_tx: crossbeam_channel::Sender<AiEvent>,
    ai_rx: crossbeam_channel::Receiver<AiEvent>,
    /// The in-flight streaming task, if any -- aborted when a new query is
    /// submitted (`stream.rs::submit`), mirroring the wgpu build's own
    /// `streaming_handle.abort()` (TD-MEM-12).
    in_flight: Option<tokio::task::JoinHandle<()>>,
}

impl ChatPanelView {
    pub fn new(cx: &mut Context<GpuiShellRoot>, config: &Config) -> Self {
        let composer = cx.new(|cx| TextInput::new(cx, &config.colors, "", "Ask anything…"));
        // Wired here, once, rather than per-render: the same `cx.subscribe`
        // shape `begin_tab_rename` uses for the tab-rename editor
        // (`actions.rs`), just set up at construction instead of on demand,
        // since the composer (unlike a rename editor) lives for the whole
        // session. `cx`'s type parameter is `GpuiShellRoot` here (this
        // constructor is called from `GpuiShellRoot::new`, passing the same
        // `cx` through), so `on_composer_event`'s `&mut GpuiShellRoot`
        // receiver matches what `subscribe` expects.
        cx.subscribe(&composer, GpuiShellRoot::on_composer_event)
            .detach();
        let (ai_tx, ai_rx) = crossbeam_channel::bounded(AI_CHANNEL_CAP);
        let mut view = Self {
            panel: ChatPanel::new(),
            composer,
            visible: false,
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
        self.visible
    }

    /// Whether the composer `TextInput` currently holds gpui's global
    /// window focus. Used by `render.rs`'s per-frame focus guard -- keyed on
    /// this, never on `is_visible()`, per M3a's own Critical fix (see that
    /// module's `tab_rename` guard and its doc comment for why a
    /// state-keyed guard froze the whole app: clicking the terminal moves
    /// gpui focus to the root while a visibility/open flag stays set, and a
    /// guard keyed on the flag would then never hand focus back).
    pub fn composer_focused(&self, window: &Window, cx: &App) -> bool {
        self.composer.focus_handle(cx).is_focused(window)
    }

    /// Open the drawer and focus the composer, or close it. Closing does
    /// NOT move focus anywhere -- the caller (`actions.rs`'s leader
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
    /// from `stream.rs`'s `/q` handler, which runs inside the composer's
    /// `cx.subscribe` callback and is handed no `Window`. That leaves the
    /// composer's `FocusHandle` reporting stale "focused" state (gpui's own
    /// `FocusId::is_focused` is just `window.focus == Some(id)` -- nothing
    /// clears it just because the focused element's div left the render
    /// tree, confirmed against gpui 0.2.2's `window.rs`), which is exactly
    /// why `render.rs`'s per-frame guard checks `!is_visible()` first: it
    /// reclaims root focus on the very next frame regardless of what
    /// `composer_focused` still (incorrectly) claims. `Leader a a`'s close
    /// path (`actions.rs`) calls this too, then immediately fixes focus
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
    /// that call also does: none of those exist on this shell (see
    /// `stream.rs`'s doc comment on what Task 2 deliberately doesn't wire).
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
