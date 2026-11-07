// The sidebar's "Agents" section body: one row per recognized agent running in a pane,
// most urgent first. See `specs/001-agents-sidebar-tab/contracts/sidebar-agents-ui.md`.

use std::rc::Rc;

use gpui::{div, prelude::*, px, App, MouseButton, MouseDownEvent, Window};

use crate::config::schema::ColorScheme;
use crate::term::agents::{format_age, AgentRow};
use crate::term::attention::{AgentState, ATTENTION_COLOR};

use super::super::pane_view::to_rgba;

/// Called with the clicked row's terminal id.
pub type AgentOpenCallback = Rc<dyn Fn(&usize, &mut Window, &mut App)>;

fn state_text(state: Option<AgentState>) -> &'static str {
    match state {
        None => "running",
        Some(AgentState::Idle) => "idle",
        Some(AgentState::Working) => "working",
        Some(AgentState::NeedsInput) => "needs input",
        Some(AgentState::Done) => "done",
    }
}

fn location(row: &AgentRow) -> String {
    let mut s = format!("{} / {}", row.workspace_name, row.tab_title);
    if let Some(n) = row.pane_number {
        s.push_str(&format!(" #{n}"));
    }
    s
}

pub fn render_agents_section(
    rows: &[AgentRow],
    colors: &ColorScheme,
    cursor: usize,
    on_open: AgentOpenCallback,
) -> impl IntoElement {
    if rows.is_empty() {
        return div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .px_2()
            .pt_4()
            .pb_2()
            .text_size(px(11.5))
            .text_color(to_rgba(colors.ui_muted))
            .child("No agents running")
            .into_any_element();
    }
    let items: Vec<_> = rows
        .iter()
        .enumerate()
        .map(|(idx, row)| {
            let open = on_open.clone();
            let terminal_id = row.terminal_id;
            let hot = row.unread || row.state == Some(AgentState::NeedsInput);
            let dot_color = if hot {
                to_rgba(ATTENTION_COLOR)
            } else {
                to_rgba(colors.ui_muted)
            };
            let detail = match &row.message {
                Some(m) => format!("{}: {m}", state_text(row.state)),
                None => state_text(row.state).to_string(),
            };
            let age = row.updated_at.map(|t| format_age(t.elapsed()));
            let small = || {
                div()
                    .text_size(px(10.0))
                    .truncate()
                    .text_color(to_rgba(colors.ui_muted))
            };
            let el = div()
                .flex()
                .flex_col()
                .mx_2()
                .px_2()
                .py_1()
                .rounded_md()
                .cursor_pointer()
                .on_mouse_down(MouseButton::Left, move |_: &MouseDownEvent, window, cx| {
                    open(&terminal_id, window, cx)
                })
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_1()
                        .child(
                            div()
                                .w(px(6.0))
                                .h(px(6.0))
                                .rounded_full()
                                .flex_shrink_0()
                                .bg(dot_color),
                        )
                        .child(div().flex_shrink_0().child(row.agent.clone()))
                        .child(small().child(location(row))),
                )
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .justify_between()
                        .gap_2()
                        .child(small().flex_1().child(detail))
                        .children(age.map(|a| small().flex_shrink_0().child(a))),
                );
            if idx == cursor {
                el.bg(to_rgba(colors.ui_surface_active))
                    .text_color(to_rgba(colors.foreground))
            } else {
                el.text_color(to_rgba(colors.ui_muted))
            }
        })
        .collect();
    div()
        .id("agents-section-scroll")
        .flex()
        .flex_col()
        .flex_1()
        .min_h_0()
        .overflow_y_scroll()
        .pt_4()
        .pb_2()
        .gap_1()
        .children(items)
        .into_any_element()
}
