// gpui chrome migration (M3d Task 3): per-section list bodies for the
// sidebar drawer. Workspaces (this task, moved out of `render.rs` to keep
// that file focused and under the 400-line convention as Task 4 adds three more)
// is the only one implemented here yet; Task 4 adds Mcp/Skills/Steering to this
// same file.

use std::rc::Rc;

use gpui::{div, prelude::*, App, Div, MouseButton, MouseDownEvent, Window};

use crate::config::schema::ColorScheme;

use super::super::pane_view::to_rgba;
use super::super::workspace::WorkspaceManager;

pub type WorkspaceSelectCallback = Rc<dyn Fn(&usize, &mut Window, &mut App)>;
pub type WorkspaceNewCallback = Rc<dyn Fn(&(), &mut Window, &mut App)>;
pub type WorkspaceCloseCallback = Rc<dyn Fn(&usize, &mut Window, &mut App)>;

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
) -> Div {
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
            let row =
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .px_2()
                    .py_1()
                    .cursor_pointer()
                    .on_mouse_down(MouseButton::Left, move |_: &MouseDownEvent, window, cx| {
                        select(&idx, window, cx)
                    })
                    .when(is_renaming, |el| {
                        el.child(rename.take().expect("checked is_some").1)
                    })
                    .when(!is_renaming, |el| el.child(label))
                    .child(div().cursor_pointer().px_1().child("x").on_mouse_down(
                        MouseButton::Left,
                        move |_: &MouseDownEvent, window, cx| close(&ws_id, window, cx),
                    ));
            if is_active {
                row.bg(to_rgba(colors.ui_surface_active))
                    .text_color(to_rgba(colors.foreground))
            } else {
                row.text_color(to_rgba(colors.ui_muted))
            }
        })
        .collect();

    div()
        .flex()
        .flex_col()
        .flex_1()
        .min_h_0()
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .justify_between()
                .px_2()
                .py_2()
                .border_b_1()
                .border_color(to_rgba(colors.ui_border))
                .child("Workspaces")
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

/// A section with no content of its own yet -- Task 4 replaces every call
/// site of this with a real list renderer.
pub fn render_placeholder_section(name: &str, colors: &ColorScheme) -> Div {
    div()
        .flex()
        .flex_col()
        .flex_1()
        .min_h_0()
        .p_2()
        .text_color(to_rgba(colors.ui_muted))
        .child(format!("{name}: not yet implemented."))
}
