// gpui chrome migration (M5a Task 1): the shared confirm-card chrome and
// the inline-action confirm card built on top of it. Split out of
// `render.rs` for the 400-line convention -- Task 6 (ACP write/run
// confirms, later in this milestone) reuses `render_confirm_card` from
// here too, hence a dedicated file rather than folding this into
// `render.rs`.

use gpui::{div, prelude::*, px, FontWeight};

use crate::config::schema::ColorScheme;
use crate::llm::agent_action::AgentAction;

use super::super::pane_view::to_rgba;

/// Shared bordered-card chrome for both confirm-prompt surfaces (inline
/// actions here, ACP write/run confirms later) -- a title row, an
/// arbitrary body element, and a keyboard-hint row. Neither surface has
/// any clickable rows of its own; both are driven entirely by their own
/// mode-keyed key guard (`GpuiShellRoot::maybe_handle_confirm_action_key`/
/// `maybe_handle_awaiting_confirm_key`), so this needs no callback
/// parameters.
pub(super) fn render_confirm_card(
    title: &str,
    body: impl IntoElement,
    hint: &str,
    colors: &ColorScheme,
) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .gap_1()
        .mx_3()
        .my_2()
        .px_3()
        .py_2()
        .rounded_md()
        .border_1()
        .border_color(to_rgba(colors.ui_accent))
        .bg(to_rgba(colors.ui_surface_hover))
        .child(
            div()
                .text_color(to_rgba(colors.ui_accent))
                .font_weight(FontWeight::BOLD)
                .child(title.to_string()),
        )
        .child(body)
        .child(
            div()
                .text_size(px(11.0))
                .text_color(to_rgba(colors.ui_muted))
                .child(hint.to_string()),
        )
}

pub(super) fn render_agent_action_card(
    action: &AgentAction,
    colors: &ColorScheme,
) -> impl IntoElement {
    let body = match action {
        AgentAction::RunCommand { cmd, explanation } => {
            if explanation.is_empty() {
                format!("Run: `{cmd}`")
            } else {
                format!("Run: `{cmd}`\n{explanation}")
            }
        }
        AgentAction::OpenFile { path } => format!("Open: `{path}`"),
        AgentAction::ExplainOutput { last_n_lines } => {
            format!("Explain last {last_n_lines} terminal lines")
        }
    };
    render_confirm_card(
        "Confirm action",
        div().text_color(to_rgba(colors.foreground)).child(body),
        "[y]es  [a]lways  [n]o",
        colors,
    )
}
