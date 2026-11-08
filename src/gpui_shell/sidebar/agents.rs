// The sidebar's "Agents" section body: one row per recognized agent running in a pane,
// most urgent first. See `specs/001-agents-sidebar-tab/contracts/sidebar-agents-ui.md`.

use std::rc::Rc;

use gpui::{
    div, prelude::*, px, AnyElement, App, MouseButton, MouseDownEvent, Pixels, Point, Window,
};

use crate::config::schema::ColorScheme;
use crate::term::agents::{format_age, short_session, AgentRow};
use crate::term::attention::{AgentState, ATTENTION_COLOR};

use super::super::pane_view::to_rgba;
use super::agent_card::{card_background, card_style, selected_outline, EDGE_WIDTH};

/// Called with the clicked row's terminal id.
pub type AgentOpenCallback = Rc<dyn Fn(&usize, &mut Window, &mut App)>;
/// Called with the right-clicked row's terminal id and the pointer position.
pub type AgentMenuCallback = Rc<dyn Fn(&(usize, Point<Pixels>), &mut Window, &mut App)>;
/// Called when a click lands outside the open rename editor.
pub type AgentEditCancelCallback = Rc<dyn Fn(&mut Window, &mut App)>;

/// The inline rename editor shown in place of one row's name.
pub struct AgentEditor {
    pub terminal_id: usize,
    pub element: AnyElement,
    /// Why the last Enter was refused, shown under the row.
    pub error: Option<String>,
}

fn state_text(state: Option<AgentState>) -> &'static str {
    match state {
        None => "running",
        Some(AgentState::Idle) => "idle",
        Some(AgentState::Working) => "working",
        Some(AgentState::NeedsInput) => "needs input",
        Some(AgentState::Done) => "done",
    }
}

/// Free text under the state line: the live activity while the agent works, else its last
/// message. `None` when there is nothing to say (the line is then not drawn).
fn body_text(row: &AgentRow) -> Option<String> {
    if row.state == Some(AgentState::Working) {
        if let Some(a) = &row.activity {
            return Some(a.clone());
        }
    }
    row.message.clone()
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
    on_menu: AgentMenuCallback,
    editor: Option<AgentEditor>,
    on_cancel_edit: AgentEditCancelCallback,
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
    let mut editor = editor;
    let items: Vec<_> = rows
        .iter()
        .enumerate()
        .map(|(idx, row)| {
            let open = on_open.clone();
            let menu = on_menu.clone();
            let terminal_id = row.terminal_id;
            let editing = editor
                .as_ref()
                .is_some_and(|e| e.terminal_id == terminal_id)
                .then(|| editor.take())
                .flatten();
            let style = card_style(row.state, colors);
            let selected = idx == cursor;
            // Raised card: neutral surface, state tint over it, outline (accent when selected).
            let base = if selected {
                colors.ui_surface_active
            } else {
                colors.ui_surface_hover
            };
            let card_bg = card_background(&style, base);
            let border = if selected {
                selected_outline(row.color, colors.ui_accent)
            } else {
                colors.ui_border
            };
            let text = if selected {
                colors.foreground
            } else {
                colors.ui_muted
            };
            let hot = row.unread || row.state == Some(AgentState::NeedsInput);
            let state_color = match (hot, style.edge) {
                (true, _) => ATTENTION_COLOR,
                (false, Some(c)) => c,
                (false, None) => colors.ui_muted,
            };
            let age = row.updated_at.map(|t| format_age(t.elapsed()));
            let small = || {
                div()
                    .min_w_0()
                    .text_size(px(10.0))
                    .truncate()
                    .text_color(to_rgba(colors.ui_muted))
            };
            let agent_color = |el: gpui::Div| match row.color {
                Some([r, g, b]) => el.text_color(gpui::rgb(
                    (u32::from(r) << 16) | (u32::from(g) << 8) | u32::from(b),
                )),
                None => el,
            };
            // Custom name (in the agent's color) or the agent type when there is none.
            let name = row.custom_name.as_deref().unwrap_or(&row.agent).to_string();
            let (title_el, error) = match editing {
                Some(e) => {
                    let cancel = on_cancel_edit.clone();
                    let el = div()
                        .flex_1()
                        .min_w_0()
                        .on_mouse_down_out(move |_: &MouseDownEvent, window, cx| cancel(window, cx))
                        .child(e.element);
                    (el.into_any_element(), e.error)
                }
                None => (
                    agent_color(div().min_w_0().truncate())
                        .child(name)
                        .into_any_element(),
                    None,
                ),
            };
            let head = div()
                .flex()
                .flex_row()
                .items_center()
                .gap_1()
                .min_w_0()
                .child(
                    div()
                        .w(px(6.0))
                        .h(px(6.0))
                        .rounded_full()
                        .flex_shrink_0()
                        .bg(to_rgba(state_color)),
                )
                .child(title_el)
                .children(row.accepting.then(|| {
                    let c = if row.msg_recent {
                        ATTENTION_COLOR
                    } else {
                        colors.ui_muted
                    };
                    div()
                        .flex_shrink_0()
                        .text_size(px(9.5))
                        .text_color(to_rgba(c))
                        .child("msg")
                }))
                .child(div().flex_1())
                .children(age.map(|a| small().flex_shrink_0().child(a)));
            // "Claude Code  working  984f93a8": agent type, colored state word, session id.
            let status = div()
                .flex()
                .flex_row()
                .items_center()
                .gap_2()
                .min_w_0()
                .child(small().flex_shrink_0().child(row.agent.clone()))
                .child(
                    div()
                        .flex_shrink_0()
                        .text_size(px(10.0))
                        .text_color(to_rgba(state_color))
                        .child(state_text(row.state)),
                )
                .children(
                    row.session
                        .as_deref()
                        .map(|id| small().child(short_session(id).to_string())),
                );
            let edge = div()
                .absolute()
                .top_0()
                .bottom_0()
                .left_0()
                .w(px(EDGE_WIDTH))
                .bg(to_rgba(style.edge.unwrap_or([0.0; 4])));
            let content = div()
                .flex()
                .flex_col()
                .gap_0p5()
                .min_w_0()
                .pl(px(EDGE_WIDTH + 8.0))
                .pr_2()
                .py_2()
                .child(head)
                .children(error.map(|msg| {
                    div()
                        .text_size(px(10.0))
                        .text_color(to_rgba(ATTENTION_COLOR))
                        .child(msg)
                }))
                .child(status)
                .children(body_text(row).map(|t| small().child(t)))
                .child(small().child(location(row)));
            div()
                .relative()
                .mx_2()
                .min_w_0()
                .overflow_hidden()
                .rounded_lg()
                .border_1()
                .border_color(to_rgba(border))
                .bg(to_rgba(card_bg))
                .text_color(to_rgba(text))
                .cursor_pointer()
                .on_mouse_down(MouseButton::Left, move |_: &MouseDownEvent, window, cx| {
                    open(&terminal_id, window, cx)
                })
                .on_mouse_down(
                    MouseButton::Right,
                    move |ev: &MouseDownEvent, window, cx| {
                        menu(&(terminal_id, ev.position), window, cx)
                    },
                )
                .child(edge)
                .child(content)
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
        .gap_2()
        .children(items)
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(state: Option<AgentState>, message: Option<&str>, activity: Option<&str>) -> AgentRow {
        AgentRow {
            terminal_id: 1,
            agent: "Claude Code".into(),
            color: None,
            agent_pid: 1,
            accepting: false,
            msg_recent: false,
            custom_name: None,
            session: None,
            workspace_name: String::new(),
            tab_title: String::new(),
            pane_number: None,
            state,
            unread: false,
            message: message.map(str::to_string),
            activity: activity.map(str::to_string),
            updated_at: None,
        }
    }

    #[test]
    fn working_shows_the_activity_else_the_message() {
        let w = Some(AgentState::Working);
        assert_eq!(
            body_text(&row(w, None, Some("Bash: ls"))).as_deref(),
            Some("Bash: ls")
        );
        assert_eq!(body_text(&row(w, None, None)), None);
        assert_eq!(
            body_text(&row(w, Some("old"), None)).as_deref(),
            Some("old")
        );
    }

    #[test]
    fn other_states_show_their_message_only() {
        let n = Some(AgentState::NeedsInput);
        assert_eq!(
            body_text(&row(n, Some("Allow?"), None)).as_deref(),
            Some("Allow?")
        );
        assert_eq!(body_text(&row(n, None, None)), None);
        assert_eq!(body_text(&row(Some(AgentState::Done), None, None)), None);
        assert_eq!(body_text(&row(None, None, None)), None);
        // a stale activity on a non-working row is never shown
        assert_eq!(
            body_text(&row(Some(AgentState::Done), None, Some("Bash: ls"))),
            None
        );
    }

    #[test]
    fn state_words() {
        assert_eq!(state_text(None), "running");
        assert_eq!(state_text(Some(AgentState::NeedsInput)), "needs input");
        assert_eq!(state_text(Some(AgentState::Working)), "working");
    }
}
