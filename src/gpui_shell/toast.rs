// gpui chrome migration (M4d Task 1): a transient top-right notification,
// mirroring the wgpu build's own toast (`src/app/mod.rs`'s `toast` field,
// `src/app/renderer/overlay.rs`'s `build_toast_instances`) as a real gpui
// `div()` rather than a hand-shaped GPU rect. Deliberately non-modal: no
// backdrop, no `cx.stop_propagation()`, no `FocusHandle` -- the ONE surface
// in this codebase that needs no key guard at all, since nothing about it
// is interactive and `input.rs`'s `on_key_down` never needs to ask it
// anything (see this milestone's own Global Constraints for why this
// differs from every text-input-holding surface M4a/b built).
//
// The Lua-triggered path the wgpu build uses (`petruterm.notify()`) is
// deferred -- see this plan's own Global Constraints for the full,
// verified reasoning (`gpui_shell` has no live Lua VM at all yet). This
// file's own `show_toast` is the primitive a future Lua bridge would call;
// Task 2 wires it to the one concrete trigger this milestone actually
// ships: config hot-reload.

use std::time::{Duration, Instant};

use gpui::{div, prelude::*, px, Context};

use crate::config::schema::ColorScheme;

use super::pane_view::to_rgba;
use super::{font_state, GpuiShellRoot};

impl GpuiShellRoot {
    /// Queue a toast. Single-slot (matching the wgpu build's own
    /// `Option<(String, Instant)>` shape) -- a second call while one is
    /// already showing simply replaces the message and restarts the
    /// clock, rather than queueing both; this is exactly the wgpu build's
    /// own `dispatch_notification`'s behavior (`self.toast = Some((msg,
    /// deadline))`, unconditional overwrite). Drained on expiry by
    /// `poll.rs`'s own tick (Task 2).
    #[allow(dead_code)]
    pub(super) fn show_toast(
        &mut self,
        message: impl Into<String>,
        duration: Duration,
        cx: &mut Context<Self>,
    ) {
        self.toast = Some((message.into(), Instant::now() + duration));
        cx.notify();
    }
}

/// Build the toast's `div()` tree: a small, floating, top-right label,
/// styled to match the wgpu build's own `build_toast_instances` (rounded
/// rect, `ui_overlay` background, `ui_accent` border, `foreground` text).
/// Non-modal by design (this module's own doc comment) -- a click landing
/// on the toast's own screen area still reaches whatever is underneath it.
pub fn render_toast(message: &str, colors: &ColorScheme) -> impl IntoElement {
    div()
        .id("toast")
        .absolute()
        .top_2()
        .right_2()
        .px_3()
        .py_2()
        .rounded_md()
        .border_1()
        .border_color(to_rgba(colors.ui_accent))
        .bg(to_rgba(colors.ui_overlay))
        .font_family(font_state::font_family())
        .text_size(px(font_state::font_size()))
        .text_color(to_rgba(colors.foreground))
        .child(message.to_string())
}
