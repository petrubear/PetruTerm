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
// Provider/model name (`build_panel_header`'s left label) IS shown here as
// of Task 2: `render_chat_panel` takes an extra `&LlmConfig` (the caller
// already has `&self.config.llm` on hand in `gpui_shell::render`) purely for
// that label -- `ChatPanelView` itself doesn't need `LlmConfig` for anything
// else, so this stays a render-time parameter rather than a field.
//
// The header's close affordance is a text hint, not a clickable icon: wiring
// a real click handler needs a `cx.listener` built where `cx` is in scope
// (`GpuiShellRoot::render`, the same place `tabs::render_tab_bar`'s
// `on_select_tab` is built), which this function's fixed 2-argument
// interface has no room for. `Leader a a` (Task 1) and `/q` (Task 2) are the
// real close paths; a decorative "×" that silently did nothing on click
// would be worse than no icon at all.

use gpui::{
    div, prelude::*, px, App, Div, FontWeight, MouseButton, MouseDownEvent, ScrollHandle, Window,
};
use std::rc::Rc;

use crate::config::schema::{ColorScheme, LlmConfig};
use crate::llm::chat_panel::{ChatPanel, PanelState};
use crate::llm::markdown::{parse_markdown, AnnotatedLine, ParseState};
use crate::llm::{ChatMessage, ChatRole};

use super::super::font_state;
use super::super::pane_view::to_rgba;
use super::composer::render_composer;
use super::confirm::{render_agent_action_card, render_awaiting_confirm_card};
use super::markdown::render_line;
use super::{ChatPanelView, MARKDOWN_WRAP_WIDTH};

/// Fixed drawer width (§3.3). Not yet user-resizable -- a future task's
/// concern if the dogfood asks for it.
pub const PANEL_WIDTH_PX: f32 = 480.0;

/// Called on a suggestion-pill click ("Fix last error" / "Explain
/// command"/"Explain more", both the zero-state's and the post-response
/// row's) -- built where `cx` is in scope (`render_callbacks.rs`,
/// `GpuiShellRoot::render`'s own indirect caller), same "callback passed
/// down as a render parameter" shape `render_tab_bar`'s `on_select`/
/// `render_context_menu`'s `on_action` already use.
pub type ChatPillCallback = Rc<dyn Fn(&mut Window, &mut App)>;

/// Build the chat panel's `div()` tree. Callers own visibility (only called
/// `when view.is_visible()`), width/animation (`render.rs`'s root layout
/// places this as a flex sibling of the pane tree and, on open, animates its
/// width in -- see that module's own doc comment on gpui 0.2.2's animation
/// API), and the markdown cache (`view.sync_markdown_cache()` must run,
/// against the same `&mut ChatPanelView` `gpui_shell::render` already holds,
/// before this read-only call -- see `mod.rs`'s doc comment on that split).
pub fn render_chat_panel(
    view: &ChatPanelView,
    llm: &LlmConfig,
    colors: &ColorScheme,
    on_fix_last_error: ChatPillCallback,
    on_explain_last_output: ChatPillCallback,
) -> Div {
    div()
        .flex()
        .flex_col()
        .flex_shrink_0()
        .h_full()
        .w(px(PANEL_WIDTH_PX))
        .rounded_lg()
        .overflow_hidden()
        .bg(to_rgba(colors.ui_surface))
        .border_1()
        .border_color(to_rgba(colors.ui_border))
        .child(render_header(
            &view.panel,
            llm,
            view.acp_session.as_ref(),
            colors,
        ))
        .child(render_message_list(
            &view.panel,
            &view.scroll_handle,
            colors,
            on_fix_last_error,
            on_explain_last_output,
        ))
        .child(render_composer(view, colors))
}

fn render_header(
    panel: &ChatPanel,
    llm: &LlmConfig,
    acp_session: Option<&crate::llm::acp::AcpSession>,
    colors: &ColorScheme,
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
    div()
        .flex()
        .flex_row()
        .items_center()
        .justify_between()
        .flex_shrink_0()
        .px_3()
        .py_2()
        .min_h(font_state::header_row_min_height())
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
                .child(
                    div()
                        .text_color(to_rgba(colors.ui_accent))
                        .child(icon_label),
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
        )
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

/// Slop, in pixels, for deciding the message list is "still at the bottom"
/// -- a user's last scroll gesture (trackpad momentum, a mouse-wheel tick)
/// rarely lands on the precise maximum offset, so a strict `==` comparison
/// would treat "essentially at the bottom" as "scrolled away" and stop
/// auto-following. One line's worth of slack covers that safely.
const AUTO_SCROLL_EPSILON_PX: f32 = 32.0;

/// Whether the message list's last known scroll position (before this
/// frame's content is added) was at or near the bottom -- gpui's
/// `ScrollHandle::offset()` grows more negative as the user scrolls down,
/// bottoming out at `-max_offset()`, so "near the bottom" is "not much
/// less negative than that". Before any layout has run (`max_offset()` is
/// still zero), this is trivially true, matching the desired initial
/// pinned-to-bottom state.
fn scrolled_near_bottom(handle: &ScrollHandle) -> bool {
    let max = handle.max_offset().height;
    if max <= px(0.) {
        return true;
    }
    handle.offset().y + max >= px(-AUTO_SCROLL_EPSILON_PX)
}

fn render_message_list(
    panel: &ChatPanel,
    scroll_handle: &ScrollHandle,
    colors: &ColorScheme,
    on_fix_last_error: ChatPillCallback,
    on_explain_last_output: ChatPillCallback,
) -> impl IntoElement {
    // Read BEFORE this frame's (possibly new) content is appended below --
    // this is what answers "was the user already following the
    // conversation" rather than "does the list look scrolled-to-bottom
    // after we just added a message", which would always be false the
    // instant a message pushes old content further up.
    let was_near_bottom = scrolled_near_bottom(scroll_handle);

    let mut list = div()
        .id("chat-panel-messages")
        .track_scroll(scroll_handle)
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
                .flex()
                .flex_col()
                .items_center()
                .gap_2()
                .py_4()
                .child(
                    div()
                        .text_color(to_rgba(colors.ui_accent))
                        .child("\u{2726}"),
                )
                .child(
                    div()
                        .text_color(to_rgba(colors.ui_muted))
                        .child("Ask a question below"),
                )
                .child(render_pill(
                    "Fix last error",
                    on_fix_last_error.clone(),
                    colors,
                ))
                .child(render_pill(
                    "Explain command",
                    on_explain_last_output.clone(),
                    colors,
                )),
        );
    }

    // Settled messages read from `panel`'s own wrapped-line cache
    // (`ChatPanel::wrapped_message`, populated by `ensure_wrap_cache` --
    // `ChatPanelView::sync_markdown_cache` calls it once per frame, from
    // `gpui_shell::render`, BEFORE this read-only function runs). Task 1
    // called `parse_markdown` fresh here for every message on every frame;
    // once Task 2 makes the panel repaint at ~30Hz while streaming, that
    // became an O(whole conversation) reparse per frame for content that,
    // for every message except the very last one added, never changes again
    // -- this cache turns "reparse everything" into "reparse only messages
    // appended since the last frame that had a new one" (a no-op on every
    // frame in between). The streaming buffer below is NOT cached: it
    // mutates on every single token, so there is no repeated work to save,
    // only the unavoidable cost of parsing the one in-flight message.
    for (idx, msg) in panel.messages.iter().enumerate() {
        list = list.child(render_message(msg, panel.wrapped_message(idx), colors));
    }

    if !panel.streaming_buf.is_empty() {
        let lines = parse_markdown(
            &panel.streaming_buf,
            MARKDOWN_WRAP_WIDTH,
            &mut ParseState::default(),
        );
        list = list.child(render_message_body_lines(&lines, colors));
    }

    if panel.show_suggestions {
        list = list.child(
            div()
                .flex()
                .flex_row()
                .justify_center()
                .gap_2()
                .py_2()
                .child(render_pill("Fix last error", on_fix_last_error, colors))
                .child(render_pill("Explain more", on_explain_last_output, colors)),
        );
    }

    if let PanelState::ConfirmAction(action) = &panel.state {
        list = list.child(render_agent_action_card(action, colors));
    }
    if let (PanelState::AwaitingConfirm, Some(display)) = (&panel.state, &panel.confirm_display) {
        list = list.child(render_awaiting_confirm_card(display, colors));
    }

    // Pull the view back down for a streamed token or a newly-added message
    // -- but only if the user was already following the conversation.
    // `ScrollHandle::scroll_to_bottom` just sets a flag consumed at the next
    // prepaint, so it's safe to call unconditionally every render; gpui
    // itself is what makes this idempotent/cheap when nothing changed.
    if was_near_bottom {
        scroll_handle.scroll_to_bottom();
    }

    list
}

/// Bubble treatment (visual-polish pass, 2026-09-17): filled, right-aligned
/// bubble for the user's own messages (mirrors the wgpu build's W-1 tinted
/// rows); assistant/system/tool stay left-aligned. No per-child `align-self`
/// in gpui 0.2.2, so alignment comes from justifying each bubble's own
/// full-width row, not the bubble div itself.
fn render_message(
    msg: &ChatMessage,
    lines: &[AnnotatedLine],
    colors: &ColorScheme,
) -> impl IntoElement {
    let label = match msg.role {
        ChatRole::User => "You",
        ChatRole::Assistant => "AI",
        ChatRole::System => "System",
        ChatRole::Tool(_) => "Tool",
    };
    let is_user = matches!(msg.role, ChatRole::User);
    let bubble = div()
        .flex()
        .flex_col()
        .gap_1()
        .max_w(gpui::relative(0.88))
        .px_3()
        .py_2()
        .rounded_md()
        .when(is_user, |el| el.bg(to_rgba(colors.ui_surface_active)))
        .when(!is_user, |el| el.bg(to_rgba(colors.ui_surface_hover)))
        .child(
            div()
                .text_color(to_rgba(colors.ui_muted))
                .text_size(px(10.5))
                .font_weight(FontWeight::BOLD)
                .child(label),
        )
        .child(render_message_body_lines(lines, colors));
    div()
        .flex()
        .flex_row()
        .when(is_user, |el| el.justify_end())
        .when(!is_user, |el| el.justify_start())
        .child(bubble)
}

fn render_message_body_lines(lines: &[AnnotatedLine], colors: &ColorScheme) -> Div {
    let mut block = div().flex().flex_col();
    for line in lines {
        block = block.child(render_line(line, colors));
    }
    block
}

/// One clickable suggestion pill -- shared by the zero-state's two pills
/// and the post-response row's two pills (Step 5/6). Real gpui `.hover()`
/// (already used once, M4c's context-menu rows) replaces the wgpu build's
/// manual `zero_state_hover`/`suggestion_hover` tracking entirely -- both
/// fields stay permanently unread by `gpui_shell`.
fn render_pill(label: &str, on_click: ChatPillCallback, colors: &ColorScheme) -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .justify_center()
        .mx_2()
        .px_3()
        .py_1()
        .rounded_md()
        .border_1()
        .border_color(to_rgba(colors.ui_border))
        .bg(to_rgba(colors.ui_surface_hover))
        .text_color(to_rgba(colors.ui_muted))
        .cursor_pointer()
        .hover(|el| {
            el.bg(to_rgba(colors.ui_surface_active))
                .border_color(to_rgba(colors.ui_accent))
                .text_color(to_rgba(colors.foreground))
        })
        .on_mouse_down(MouseButton::Left, move |_: &MouseDownEvent, window, cx| {
            on_click(window, cx)
        })
        .child(label.to_string())
}
