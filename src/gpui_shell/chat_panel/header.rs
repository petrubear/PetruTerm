// gpui chrome migration (alignment pass, 2026-09-17): the chat panel's
// header row. Split out of `render.rs` to keep that file under the
// 400-line convention -- pure code motion, no behavior change.
//
// The header's close affordance is a text hint, not a clickable icon: wiring
// a real click handler needs a `cx.listener` built where `cx` is in scope
// (`GpuiShellRoot::render`, the same place `tabs::render_tab_bar`'s
// `on_select_tab` is built), which this function's fixed 2-argument
// interface has no room for. `Leader a a` (Task 1) and `/q` (Task 2) are the
// real close paths; a decorative "×" that silently did nothing on click
// would be worse than no icon at all.

use gpui::{div, prelude::*, px};

use crate::config::schema::LlmConfig;
use crate::llm::chat_panel::{ChatPanel, PanelState};

use super::super::font_state;
use super::super::pane_view::to_rgba;

pub(super) fn render_header(
    panel: &ChatPanel,
    llm: &LlmConfig,
    acp_session: Option<&crate::llm::acp::AcpSession>,
    colors: &crate::config::schema::ColorScheme,
) -> impl IntoElement {
    let status = header_status(panel);
    let (icon_label, detail) = if let Some(session) = acp_session {
        (
            format!("\u{25c8} {}", session.display_name),
            format!("agent:{}", session.agent_name),
        )
    } else {
        let short_model = short_model_name(&llm.model);
        (
            format!("\u{2726} {short_model}"),
            format!("{}:{}", llm.provider, llm.model),
        )
    };
    let content_row = div()
        .flex()
        .flex_row()
        .items_center()
        .justify_between()
        .flex_shrink_0()
        // Must equal `render_message_list`'s own `px_3` so the icon lines up
        // with the message bubbles below it.
        .px_3()
        .py_2()
        .min_h(font_state::header_row_min_height())
        .font_family(font_state::font_family())
        .text_size(px(font_state::font_size()))
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap_2()
                .child(
                    // Same pill treatment as the terminal tab bar's active
                    // cell (`tabs::render_tab_bar`) -- requested live to
                    // read as the same visual language rather than plain
                    // text sitting next to a proper pill.
                    div()
                        .px_2()
                        .py_1()
                        .rounded_md()
                        .bg(to_rgba(colors.ui_surface_hover))
                        .text_color(to_rgba(colors.ui_accent))
                        .child(div().italic().child(icon_label)),
                )
                .child(
                    div()
                        .text_color(to_rgba(colors.ui_muted))
                        .child(format!("\u{2502} {detail}")),
                )
                .when(!status.is_empty(), |el| {
                    el.child(div().text_color(to_rgba(colors.ui_muted)).child(status))
                }),
        )
        .child(
            div()
                .text_color(to_rgba(colors.ui_muted))
                .child("Leader a a to close"),
        );

    // Separate from `content_row`'s own padding -- see
    // `tabs::render_tab_bar`'s doc comment on why the corner-clash fix
    // (`mx_2`) lives on its own element instead of `content_row`'s border.
    let divider = div().mx_2().h(px(1.0)).bg(to_rgba(colors.ui_border));

    div()
        .flex()
        .flex_col()
        .flex_shrink_0()
        .child(content_row)
        .child(divider)
}

/// Strip a `provider/model:tag` name down to its bare model name -- mirrors
/// the wgpu build's own `short_chat_header_model_name`
/// (`src/app/renderer/mod.rs`), minus its char-count truncation: that exists
/// to fit a fixed terminal-cell header width, which doesn't apply to gpui's
/// proportional text layout, so the full (short) name is shown here instead
/// of an 8-character-truncated one.
fn short_model_name(model: &str) -> &str {
    model
        .rsplit('/')
        .next()
        .unwrap_or(model)
        .rsplit(':')
        .next()
        .unwrap_or(model)
}

fn header_status(panel: &ChatPanel) -> String {
    match &panel.state {
        PanelState::Hidden | PanelState::Idle => String::new(),
        PanelState::Loading => "loading…".to_string(),
        PanelState::Streaming => "streaming…".to_string(),
        PanelState::Error(msg) => format!("error: {msg}"),
        PanelState::AwaitingConfirm => "awaiting confirmation".to_string(),
        PanelState::ConfirmAction(_) => "confirm action".to_string(),
    }
}
