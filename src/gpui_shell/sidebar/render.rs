// gpui chrome migration (M3c Task 4): the workspace sidebar's `div()` tree
// -- one clickable row per workspace, a header "+" new-workspace
// affordance, and a per-row "x" close affordance. Same "flat clickable
// cell, closure built at the call site" shape as `tabs::render_tab_bar`
// (`tabs.rs`), laid out as a column instead of a row.

use std::rc::Rc;

use gpui::{div, prelude::*, px, App, Div, MouseButton, MouseDownEvent, Window};

use crate::config::schema::ColorScheme;

use super::super::font_state;
use super::super::pane_view::to_rgba;
use super::super::workspace::WorkspaceManager;

pub const SIDEBAR_WIDTH_PX: f32 = 220.0;

pub type WorkspaceSelectCallback = Rc<dyn Fn(&usize, &mut Window, &mut App)>;
pub type WorkspaceNewCallback = Rc<dyn Fn(&(), &mut Window, &mut App)>;
pub type WorkspaceCloseCallback = Rc<dyn Fn(&usize, &mut Window, &mut App)>;

/// `rename`: `(workspace_id, editor)` -- same "pinned to an id, taken at
/// most once, placed on the matching row" shape as `render_tab_bar`'s own
/// `rename` parameter (`tabs.rs`).
pub fn render_workspace_sidebar(
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
        .flex_shrink_0()
        .h_full()
        .w(px(SIDEBAR_WIDTH_PX))
        .bg(to_rgba(colors.ui_surface))
        .border_r_1()
        .border_color(to_rgba(colors.ui_border))
        .font_family(font_state::font_family())
        .text_size(px(font_state::font_size()))
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
