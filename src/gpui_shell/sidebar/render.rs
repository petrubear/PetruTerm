// gpui chrome migration (M3c Task 4 / M3d Task 3): the sidebar drawer's
// outer frame -- fixed width, background, section-tab row -- plus dispatch
// to whichever section's body (`sections.rs`) is active. Section bodies
// themselves moved to `sections.rs` in M3d Task 3 to keep this file
// focused and under the 400-line convention as Task 4 adds three more.

use std::rc::Rc;

use gpui::{div, prelude::*, px, App, Div, MouseButton, MouseDownEvent, Window};

use crate::config::schema::ColorScheme;

use super::super::font_state;
use super::super::pane_view::to_rgba;
use super::super::workspace::WorkspaceManager;
use super::sections::{render_placeholder_section, render_workspaces_section};
use super::SidebarSection;

pub const SIDEBAR_WIDTH_PX: f32 = 220.0;

pub use super::sections::{WorkspaceCloseCallback, WorkspaceNewCallback, WorkspaceSelectCallback};

pub type SectionSelectCallback = Rc<dyn Fn(&SidebarSection, &mut Window, &mut App)>;

/// Every render-time input the sidebar needs, bundled the same way
/// `pane_view::PaneRenderCx` bundles the pane tree's -- Task 4 adds the
/// MCP/Skills/Steering fields to this same struct.
pub struct SidebarRenderCx<'a> {
    pub workspaces: &'a WorkspaceManager,
    pub colors: &'a ColorScheme,
    pub active_section: SidebarSection,
    pub on_select_workspace: WorkspaceSelectCallback,
    pub on_new_workspace: WorkspaceNewCallback,
    pub on_close_workspace: WorkspaceCloseCallback,
    pub on_select_section: SectionSelectCallback,
    pub workspace_rename: Option<(usize, gpui::AnyElement)>,
}

pub fn render_workspace_sidebar(ctx: SidebarRenderCx<'_>) -> Div {
    div()
        .flex()
        .flex_col()
        .flex_shrink_0()
        .h_full()
        .w(px(SIDEBAR_WIDTH_PX))
        .bg(to_rgba(ctx.colors.ui_surface))
        .border_r_1()
        .border_color(to_rgba(ctx.colors.ui_border))
        .font_family(font_state::font_family())
        .text_size(px(font_state::font_size()))
        .child(render_section_tabs(
            ctx.active_section,
            ctx.colors,
            ctx.on_select_section,
        ))
        .child(match ctx.active_section {
            SidebarSection::Workspaces => render_workspaces_section(
                ctx.workspaces,
                ctx.colors,
                ctx.on_select_workspace,
                ctx.on_new_workspace,
                ctx.on_close_workspace,
                ctx.workspace_rename,
            ),
            SidebarSection::Mcp => render_placeholder_section("MCP", ctx.colors),
            SidebarSection::Skills => render_placeholder_section("Skills", ctx.colors),
            SidebarSection::Steering => render_placeholder_section("Steering", ctx.colors),
        })
}

fn render_section_tabs(
    active: SidebarSection,
    colors: &ColorScheme,
    on_select: SectionSelectCallback,
) -> Div {
    let tabs = [
        (SidebarSection::Workspaces, "Workspaces"),
        (SidebarSection::Mcp, "MCP"),
        (SidebarSection::Skills, "Skills"),
        (SidebarSection::Steering, "Steering"),
    ];
    let cells: Vec<_> = tabs
        .into_iter()
        .map(|(section, label)| {
            let is_active = section == active;
            let select = on_select.clone();
            let cell = div()
                .flex_1()
                .px_1()
                .py_1()
                .text_size(px(11.0))
                .cursor_pointer()
                .child(label)
                .on_mouse_down(MouseButton::Left, move |_: &MouseDownEvent, window, cx| {
                    select(&section, window, cx)
                });
            if is_active {
                cell.bg(to_rgba(colors.ui_surface_active))
                    .text_color(to_rgba(colors.foreground))
            } else {
                cell.text_color(to_rgba(colors.ui_muted))
            }
        })
        .collect();

    div()
        .flex()
        .flex_row()
        .flex_shrink_0()
        .border_b_1()
        .border_color(to_rgba(colors.ui_border))
        .children(cells)
}
