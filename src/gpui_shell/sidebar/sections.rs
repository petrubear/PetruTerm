// Per-section list bodies for the sidebar drawer: Workspaces, MCP, Skills,
// Steering.

use std::rc::Rc;

use gpui::{div, prelude::*, px, App, Div, MouseButton, MouseDownEvent, Window};

use crate::config::schema::ColorScheme;
use crate::llm::mcp::manager::McpManager;
use crate::llm::skills::SkillManager;
use crate::llm::steering::SteeringManager;

use super::super::pane_view::to_rgba;
use super::super::workspace::WorkspaceManager;
use super::super::workspace_info::RowDetail;
use crate::term::attention::ATTENTION_COLOR;

pub type WorkspaceSelectCallback = Rc<dyn Fn(&usize, &mut Window, &mut App)>;
pub type WorkspaceNewCallback = Rc<dyn Fn(&(), &mut Window, &mut App)>;
pub type WorkspaceCloseCallback = Rc<dyn Fn(&usize, &mut Window, &mut App)>;
pub type McpOpenCallback = Rc<dyn Fn(&usize, &mut Window, &mut App)>;
pub type SkillOpenCallback = Rc<dyn Fn(&usize, &mut Window, &mut App)>;
pub type SteeringOpenCallback = Rc<dyn Fn(&usize, &mut Window, &mut App)>;

/// `rename`: `(workspace_id, editor)` -- same "pinned to an id, taken at
/// most once, placed on the matching row" shape as `tabs::render_tab_bar`'s
/// own `rename` parameter.
pub fn render_workspaces_section(
    workspaces: &WorkspaceManager,
    colors: &ColorScheme,
    on_select: WorkspaceSelectCallback,
    on_new: WorkspaceNewCallback,
    on_close: WorkspaceCloseCallback,
    rename: Option<(usize, gpui::AnyElement)>,
    details: &std::collections::HashMap<usize, RowDetail>,
) -> impl IntoElement {
    let active_index = workspaces.active_index();
    let mut rename = rename;
    let rows: Vec<_> = workspaces
        .workspaces()
        .iter()
        .enumerate()
        .map(|(idx, ws)| {
            let is_active = idx == active_index;
            let is_renaming = rename.as_ref().is_some_and(|(id, _)| *id == ws.id);
            let select = on_select.clone();
            let close = on_close.clone();
            let ws_id = ws.id;
            let label = format!("{}  ({} tabs)", ws.name, ws.tabs.tab_count());
            let detail = details.get(&ws.id);
            let name_line =
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_1()
                            .when(detail.is_some_and(|d| d.unread), |el| {
                                el.child(
                                    div()
                                        .w(px(6.0))
                                        .h(px(6.0))
                                        .rounded_full()
                                        .bg(to_rgba(ATTENTION_COLOR)),
                                )
                            })
                            .when(is_renaming, |el| {
                                el.child(rename.take().expect("checked is_some").1)
                            })
                            .when(!is_renaming, |el| el.child(label)),
                    )
                    .child(div().cursor_pointer().px_1().child("x").on_mouse_down(
                        MouseButton::Left,
                        move |_: &MouseDownEvent, window, cx| close(&ws_id, window, cx),
                    ));
            let row = div()
                .flex()
                .flex_col()
                .mx_2()
                .px_2()
                .py_1()
                .rounded_md()
                .cursor_pointer()
                .on_mouse_down(MouseButton::Left, move |_: &MouseDownEvent, window, cx| {
                    select(&idx, window, cx)
                })
                .child(name_line)
                .children(detail.and_then(|d| detail_lines(d, colors)));
            if is_active {
                row.bg(to_rgba(colors.ui_surface_active))
                    .text_color(to_rgba(colors.foreground))
            } else {
                row.text_color(to_rgba(colors.ui_muted))
            }
        })
        .collect();

    div()
        .id("workspace-list-scroll")
        .flex()
        .flex_col()
        .flex_1()
        .min_h_0()
        .overflow_y_scroll()
        .pt_4()
        .pb_2()
        .gap_1()
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .justify_between()
                // `px_2`, matching `render_browser_row`'s/this section's own
                // row `mx_2` below, so the label lines up with the row
                // pills' own edge.
                .px_2()
                .pb_2()
                .text_size(px(10.5))
                .child(
                    div()
                        .text_color(to_rgba(colors.ui_muted))
                        .child("WORKSPACES"),
                )
                .child(
                    div()
                        .cursor_pointer()
                        .px_1()
                        .child("+")
                        .on_mouse_down(MouseButton::Left, move |_: &MouseDownEvent, window, cx| {
                            on_new(&(), window, cx)
                        }),
                ),
        )
        .children(rows)
}

/// Secondary lines under a workspace name: branch / PR / ports, then the agent state and
/// its last message. `None` when there is nothing to show.
fn detail_lines(detail: &RowDetail, colors: &ColorScheme) -> Option<Div> {
    let info = &detail.info;
    let mut facts: Vec<String> = Vec::new();
    if let Some(branch) = &info.branch {
        facts.push(format!("\u{e0a0} {branch}"));
    }
    if let Some(pr) = &info.pr {
        facts.push(pr.label());
    }
    if !info.ports.is_empty() {
        let ports: Vec<String> = info.ports.iter().take(4).map(|p| format!(":{p}")).collect();
        facts.push(ports.join(" "));
    }
    let attention = detail.state_label().map(|label| match &detail.message {
        Some(m) => format!("{label}: {m}"),
        None => label.to_string(),
    });
    if facts.is_empty() && attention.is_none() {
        return None;
    }
    let small = || div().text_size(px(10.0)).truncate();
    Some(
        div()
            .flex()
            .flex_col()
            .when(!facts.is_empty(), |el| {
                el.child(
                    small()
                        .text_color(to_rgba(colors.ui_muted))
                        .child(facts.join("  ")),
                )
            })
            .when_some(attention, |el, line| {
                el.child(
                    small()
                        .text_color(if detail.unread {
                            to_rgba(ATTENTION_COLOR)
                        } else {
                            to_rgba(colors.ui_muted)
                        })
                        .child(line),
                )
            }),
    )
}

/// One row per connected MCP server (sorted by name, matching the wgpu
/// build's own sidebar render order in `src/app/mod.rs`'s `open_sidebar_
/// info_overlay`), showing its connected tool count. `cursor` highlights
/// the keyboard-nav row (`sidebar_move_cursor`); clicking a row
/// opens it directly regardless of the cursor.
pub fn render_mcp_section(
    mcp: &McpManager,
    colors: &ColorScheme,
    cursor: usize,
    on_open: McpOpenCallback,
) -> impl IntoElement {
    let mut servers: Vec<String> = {
        let mut set: std::collections::BTreeSet<String> = Default::default();
        for (server, _) in mcp.all_tools() {
            set.insert(server);
        }
        set.into_iter().collect()
    };
    servers.sort();

    if servers.is_empty() {
        return render_empty_section("No MCP servers connected.", colors).into_any_element();
    }

    let rows: Vec<_> = servers
        .iter()
        .enumerate()
        .map(|(idx, name)| {
            let tool_count = mcp.tools_for_server(name).len();
            let label = format!("{name}  ({tool_count} tools)");
            render_browser_row(label, idx == cursor, idx, on_open.clone(), colors)
        })
        .collect();

    div()
        .id("mcp-section-scroll")
        .flex()
        .flex_col()
        .flex_1()
        .min_h_0()
        .overflow_y_scroll()
        .pt_4()
        .pb_2()
        .children(rows)
        .into_any_element()
}

/// One row per loaded skill, sorted the same way `SkillManager::skills()`
/// already returns them (load order: global first, then project-local
/// overlaying by name).
pub fn render_skills_section(
    skills: &SkillManager,
    colors: &ColorScheme,
    cursor: usize,
    on_open: SkillOpenCallback,
) -> impl IntoElement {
    let metas = skills.skills();
    if metas.is_empty() {
        return render_empty_section("No skills loaded.", colors).into_any_element();
    }
    let rows: Vec<_> = metas
        .iter()
        .enumerate()
        .map(|(idx, skill)| {
            render_browser_row(
                skill.name.clone(),
                idx == cursor,
                idx,
                on_open.clone(),
                colors,
            )
        })
        .collect();
    div()
        .id("skills-section-scroll")
        .flex()
        .flex_col()
        .flex_1()
        .min_h_0()
        .overflow_y_scroll()
        .pt_4()
        .pb_2()
        .children(rows)
        .into_any_element()
}

/// One row per loaded steering file, `.md` suffix stripped for display
/// (matches `src/app/mod.rs`'s `open_sidebar_info_overlay`'s own `strip_
/// suffix(".md")`).
pub fn render_steering_section(
    steering: &SteeringManager,
    colors: &ColorScheme,
    cursor: usize,
    on_open: SteeringOpenCallback,
) -> impl IntoElement {
    let files = steering.files();
    if files.is_empty() {
        return render_empty_section("No steering files loaded.", colors).into_any_element();
    }
    let rows: Vec<_> = files
        .iter()
        .enumerate()
        .map(|(idx, (name, _content))| {
            let label = name.strip_suffix(".md").unwrap_or(name).to_string();
            render_browser_row(label, idx == cursor, idx, on_open.clone(), colors)
        })
        .collect();
    div()
        .id("steering-section-scroll")
        .flex()
        .flex_col()
        .flex_1()
        .min_h_0()
        .overflow_y_scroll()
        .pt_4()
        .pb_2()
        .children(rows)
        .into_any_element()
}

/// One clickable row, shared shape across all three browser sections above.
#[allow(clippy::type_complexity)]
fn render_browser_row(
    label: String,
    is_cursor: bool,
    idx: usize,
    on_open: Rc<dyn Fn(&usize, &mut Window, &mut App)>,
    colors: &ColorScheme,
) -> Div {
    let row = div()
        .mx_2()
        .px_2()
        .py_1()
        .rounded_md()
        .cursor_pointer()
        .child(label)
        .on_mouse_down(MouseButton::Left, move |_: &MouseDownEvent, window, cx| {
            on_open(&idx, window, cx)
        });
    if is_cursor {
        row.bg(to_rgba(colors.ui_surface_active))
            .text_color(to_rgba(colors.foreground))
    } else {
        row.text_color(to_rgba(colors.ui_muted))
    }
}

fn render_empty_section(message: &str, colors: &ColorScheme) -> Div {
    div()
        .flex()
        .flex_col()
        .flex_1()
        .min_h_0()
        // `px_2`, matching every section's own row inset -- see
        // `render_workspaces_section`'s label row for the full reasoning.
        .px_2()
        .pt_4()
        .pb_2()
        .text_size(px(11.5))
        .text_color(to_rgba(colors.ui_muted))
        .child(message.to_string())
}
