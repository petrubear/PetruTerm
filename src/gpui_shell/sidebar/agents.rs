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
            let agent_color = |el: gpui::Div| match row.color {
                Some([r, g, b]) => el.text_color(gpui::rgb(
                    (u32::from(r) << 16) | (u32::from(g) << 8) | u32::from(b),
                )),
                None => el,
            };
            // Custom name first (in the agent's color), the agent type muted next to it.
            let title = match &row.custom_name {
                Some(name) => div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_1()
                    .flex_shrink_0()
                    .child(agent_color(div()).child(name.clone()))
                    .child(small().child(row.agent.clone())),
                None => agent_color(div().flex_shrink_0()).child(row.agent.clone()),
            };
            let (title_el, error) = match editing {
                Some(e) => {
                    let cancel = on_cancel_edit.clone();
                    let el = div()
                        .flex_1()
                        .on_mouse_down_out(move |_: &MouseDownEvent, window, cx| cancel(window, cx))
                        .child(e.element);
                    (el.into_any_element(), e.error)
                }
                None => (title.into_any_element(), None),
            };
            let el =
                div()
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
                    .on_mouse_down(
                        MouseButton::Right,
                        move |ev: &MouseDownEvent, window, cx| {
                            menu(&(terminal_id, ev.position), window, cx)
                        },
                    )
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
                            .child(title_el)
                            .children(row.session.as_deref().map(|id| {
                                small().flex_shrink_0().child(short_session(id).to_string())
                            }))
                            .child(small().child(location(row))),
                    )
                    .children(error.map(|msg| {
                        div()
                            .text_size(px(10.0))
                            .text_color(to_rgba(ATTENTION_COLOR))
                            .child(msg)
                    }))
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
