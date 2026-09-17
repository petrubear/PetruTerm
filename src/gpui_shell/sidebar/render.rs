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
use super::sections::{
    render_mcp_section, render_skills_section, render_steering_section, render_workspaces_section,
};
use super::SidebarSection;
use crate::llm::mcp::manager::McpManager;
use crate::llm::skills::SkillManager;
use crate::llm::steering::SteeringManager;

pub const SIDEBAR_WIDTH_PX: f32 = 220.0;

pub use super::sections::{
    McpOpenCallback, SkillOpenCallback, SteeringOpenCallback, WorkspaceCloseCallback,
    WorkspaceNewCallback, WorkspaceSelectCallback,
};

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
    pub mcp_manager: &'a McpManager,
    pub mcp_cursor: usize,
    pub on_open_mcp: McpOpenCallback,
    pub skill_manager: &'a SkillManager,
    pub skills_cursor: usize,
    pub on_open_skill: SkillOpenCallback,
    pub steering_manager: &'a SteeringManager,
    pub steering_cursor: usize,
    pub on_open_steering: SteeringOpenCallback,
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
            )
            .into_any_element(),
            SidebarSection::Mcp => {
                render_mcp_section(ctx.mcp_manager, ctx.colors, ctx.mcp_cursor, ctx.on_open_mcp)
                    .into_any_element()
            }
            SidebarSection::Skills => render_skills_section(
                ctx.skill_manager,
                ctx.colors,
                ctx.skills_cursor,
                ctx.on_open_skill,
            )
            .into_any_element(),
            SidebarSection::Steering => render_steering_section(
                ctx.steering_manager,
                ctx.colors,
                ctx.steering_cursor,
                ctx.on_open_steering,
            )
            .into_any_element(),
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
    // Rounded-pill treatment (visual-polish pass, 2026-09-17), replacing the
    // original 2px-underline cells: those existed because a flat background
    // fill with no padding read as barely distinguishable from its
    // neighbors. With real inset padding + a rounded pill (same fill/radius
    // as `tabs::render_tab_bar`'s own active cell) the fill alone reads
    // clearly, so the underline is no longer needed.
    let cells: Vec<_> = tabs
        .into_iter()
        .map(|(section, label)| {
            let is_active = section == active;
            let select = on_select.clone();
            let cell = div()
                .flex_1()
                .px_2()
                .py_1()
                .m_1()
                .rounded_md()
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
