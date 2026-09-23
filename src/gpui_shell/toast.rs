// A transient top-right notification. Non-modal, no key guard, no
// `FocusHandle`. Only trigger: config hot-reload; `petruterm.notify()` is
// not supported in gpui_shell.

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
    /// `poll.rs`'s own tick, called from `poll.rs`'s config-hot-reload
    /// branch (the only trigger).
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
