// gpui chrome migration (M3b Task 1): the chat panel's `div()` tree --
// drawer frame, header, scrollable message list, composer row.
//
// `src/app/renderer/chat.rs` is the reference for *what* the wgpu build
// draws (see its `build_panel_header`/`build_panel_messages`), not *how* --
// every line of its `RoundedRectInstance` pixel math is replaced by real
// flex elements here, the same "port the logic, rewrite the painting" split
// `status_bar::render_status_bar` and `tabs::render_tab_bar` already went
// through.
//
// Provider/model name (`build_panel_header`'s left label) is deliberately
// NOT shown here: that data lives in `Config`, which this function's fixed
// interface (`render_chat_panel(view, colors)`, consumed as-is by Tasks 2
// and 3) does not receive. The header shows the panel's own state instead
// (idle/loading/streaming/error) -- real content `ChatPanel` already tracks
// without a `Config` reference. A model-name segment can be layered on once
// Task 2 wires submission through `Config`'s LLM view, if wanted.
//
// The header's close affordance is a text hint, not a clickable icon: wiring
// a real click handler needs a `cx.listener` built where `cx` is in scope
// (`GpuiShellRoot::render`, the same place `tabs::render_tab_bar`'s
// `on_select_tab` is built), which this function's fixed 2-argument
// interface has no room for. `Leader a a` (Task 1) and `/q` (Task 2) are the
// real close paths; a decorative "×" that silently did nothing on click
// would be worse than no icon at all.

use gpui::{div, prelude::*, px, Div, FontWeight};

use crate::config::schema::ColorScheme;
use crate::llm::chat_panel::{ChatPanel, PanelState};
use crate::llm::markdown::{parse_markdown, ParseState};
use crate::llm::{ChatMessage, ChatRole};

use super::super::font_state;
use super::super::pane_view::to_rgba;
use super::markdown::render_line;
use super::ChatPanelView;

/// Fixed drawer width (§3.3). Not yet user-resizable -- a future task's
/// concern if the dogfood asks for it.
pub const PANEL_WIDTH_PX: f32 = 480.0;

/// Wide enough that `parse_markdown`'s own char-count wrapping never fires --
/// gpui wraps instead (see `markdown.rs`'s doc comment).
const MARKDOWN_WRAP_WIDTH: usize = 100_000;

/// Build the chat panel's `div()` tree. Callers own visibility (only called
/// `when view.is_visible()`) and width/animation (`render.rs`'s root layout
/// places this as a flex sibling of the pane tree and, on open, animates its
/// width in -- see that module's own doc comment on gpui 0.2.2's animation
/// API).
pub fn render_chat_panel(view: &ChatPanelView, colors: &ColorScheme) -> Div {
    div()
        .flex()
        .flex_col()
        .flex_shrink_0()
        .h_full()
        .w(px(PANEL_WIDTH_PX))
        .bg(to_rgba(colors.ui_surface))
        .border_l_1()
        .border_color(to_rgba(colors.ui_border))
        .child(render_header(&view.panel, colors))
        .child(render_message_list(&view.panel, colors))
        .child(render_composer(view, colors))
}

fn render_header(panel: &ChatPanel, colors: &ColorScheme) -> impl IntoElement {
    let status = header_status(panel);
    div()
        .flex()
        .flex_row()
        .items_center()
        .justify_between()
        .flex_shrink_0()
        .px_3()
        .py_2()
        .border_b_1()
        .border_color(to_rgba(colors.ui_border))
        .font_family(font_state::font_family())
        .text_size(px(font_state::font_size()))
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap_2()
                .child(div().text_color(to_rgba(colors.ui_accent)).child("AI"))
                .when(!status.is_empty(), |el| {
                    el.child(div().text_color(to_rgba(colors.ui_muted)).child(status))
                }),
        )
        .child(
            div()
                .text_color(to_rgba(colors.ui_muted))
                .child("Leader a a to close"),
        )
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

fn render_message_list(panel: &ChatPanel, colors: &ColorScheme) -> impl IntoElement {
    let mut list = div()
        .id("chat-panel-messages")
        .flex()
        .flex_col()
        .flex_1()
        .min_h_0()
        .overflow_y_scroll()
        .gap_3()
        .px_3()
        .py_2()
        .font_family(font_state::font_family())
        .text_size(px(font_state::font_size()));

    if panel.messages.is_empty() && panel.streaming_buf.is_empty() {
        list = list.child(
            div()
                .text_color(to_rgba(colors.ui_muted))
                .child("Ask a question to get started."),
        );
    }

    for msg in &panel.messages {
        list = list.child(render_message(msg, colors));
    }

    if !panel.streaming_buf.is_empty() {
        list = list.child(render_message_body(&panel.streaming_buf, colors));
    }

    list
}

fn render_message(msg: &ChatMessage, colors: &ColorScheme) -> impl IntoElement {
    let label = match msg.role {
        ChatRole::User => "You",
        ChatRole::Assistant => "AI",
        ChatRole::System => "System",
        ChatRole::Tool(_) => "Tool",
    };
    div()
        .flex()
        .flex_col()
        .gap_1()
        .child(
            div()
                .text_color(to_rgba(colors.ui_muted))
                .font_weight(FontWeight::BOLD)
                .child(label),
        )
        .child(render_message_body(&msg.content, colors))
}

/// Parse `content` fresh on every render rather than reusing `ChatPanel`'s
/// own `wrapped_cache` (`ensure_wrap_cache`/`wrapped_message`): that cache is
/// keyed to a `width` in terminal columns and requires `&mut ChatPanel` to
/// populate, which this module's read-only `render_chat_panel(&ChatPanelView,
/// ..)` signature doesn't have. Per Step 5, `parse_markdown` is called
/// directly with a large width purely for its styling annotations -- the
/// cache exists for the wgpu renderer's own re-shaping-avoidance concern,
/// which doesn't apply to gpui's element diffing.
fn render_message_body(content: &str, colors: &ColorScheme) -> Div {
    let lines = parse_markdown(content, MARKDOWN_WRAP_WIDTH, &mut ParseState::default());
    let mut block = div().flex().flex_col();
    for line in &lines {
        block = block.child(render_line(line, colors));
    }
    block
}

fn render_composer(view: &ChatPanelView, colors: &ColorScheme) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .flex_shrink_0()
        .gap_1()
        .border_t_1()
        .border_color(to_rgba(colors.ui_border))
        .px_3()
        .py_2()
        .child(
            div()
                .flex()
                .h(px(28.0))
                .items_center()
                .font_family(font_state::font_family())
                .text_size(px(font_state::font_size()))
                .text_color(to_rgba(colors.foreground))
                .child(view.composer.clone()),
        )
        .child(
            div()
                .font_family(font_state::font_family())
                .text_size(px(font_state::font_size()))
                .text_color(to_rgba(colors.ui_muted))
                .child("Enter to send  ·  /clear /skills /mcp /model /agent  ·  /q to close"),
        )
}
