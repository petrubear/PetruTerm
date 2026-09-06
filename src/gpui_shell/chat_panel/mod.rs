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

mod markdown;
mod render;

pub use render::{render_chat_panel, PANEL_WIDTH_PX};

use gpui::{App, AppContext, Context, Entity, Focusable, Window};

use crate::config::schema::ColorScheme;
use crate::llm::chat_panel::ChatPanel;

use super::text_input::TextInput;
use super::GpuiShellRoot;

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
}

impl ChatPanelView {
    pub fn new(cx: &mut Context<GpuiShellRoot>, colors: &ColorScheme) -> Self {
        let composer = cx.new(|cx| TextInput::new(cx, colors, "", "Ask anything…"));
        Self {
            panel: ChatPanel::new(),
            composer,
            visible: false,
        }
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
            self.visible = false;
            self.panel.close();
        } else {
            self.visible = true;
            self.panel.open();
            self.composer.focus_handle(cx).focus(window);
        }
        cx.notify();
    }
}
