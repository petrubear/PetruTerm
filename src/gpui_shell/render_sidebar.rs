// gpui chrome migration (M3d Task 4 post-review split): builds the
// already-open-animated workspace sidebar drawer element. Split out of
// `render.rs`'s own `render()` for the 400-line convention -- `render.rs`
// grew to 418 lines once Task 4's three new sidebar-section callbacks
// (MCP/Skills/Steering open handlers) landed on top of Tasks 1-3's own
// growth, the same failure class M3c's `actions.rs` and M3d's own
// `input.rs` (see that file's own post-Task-3 doc comment) both hit.
//
// Called only from `render()`'s `middle_row` construction, inside
// `.when(self.sidebar.is_visible(), ...)`.

use std::rc::Rc;
use std::time::Duration;

use gpui::{ease_out_quint, prelude::*, px, Animation, AnimationExt as _, Context};

use super::leader::LeaderAction;
use super::sidebar;
use super::sidebar::SidebarSection;
use super::GpuiShellRoot;

/// Same duration the drawer's own opening grow animation has used since
/// M3c Task 4 -- kept here (not re-exported from `render.rs`) since this is
/// now the only place that reads it.
const SIDEBAR_OPEN_ANIM: Duration = Duration::from_millis(180);

impl GpuiShellRoot {
    pub(super) fn render_sidebar_drawer(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let on_select_workspace: sidebar::render::WorkspaceSelectCallback =
            Rc::new(cx.listener(|this, idx: &usize, _window, cx| {
                if this.switch_workspace_to_index(*idx) {
                    cx.notify();
                }
            }));
        let on_new_workspace: sidebar::render::WorkspaceNewCallback =
            Rc::new(cx.listener(|this, _: &(), window, cx| {
                this.dispatch_leader_action(LeaderAction::NewWorkspace, window, cx);
            }));
        let on_close_workspace: sidebar::render::WorkspaceCloseCallback =
            Rc::new(cx.listener(|this, id: &usize, _window, cx| {
                if let Some(idx) = this
                    .workspaces
                    .workspaces()
                    .iter()
                    .position(|w| w.id == *id)
                {
                    this.close_workspace_at(idx, true, cx);
                    cx.notify();
                }
            }));
        let workspace_rename_element = self
            .workspace_rename
            .as_ref()
            .map(|(id, input)| (*id, input.clone().into_any_element()));

        let on_select_section: sidebar::render::SectionSelectCallback =
            Rc::new(cx.listener(|this, section: &SidebarSection, window, cx| {
                this.sidebar.set_section(*section);
                window.focus(&this.sidebar_focus_handle);
                cx.notify();
            }));

        let on_open_mcp: sidebar::sections::McpOpenCallback =
            Rc::new(cx.listener(|this, idx: &usize, _window, cx| {
                this.sidebar.set_mcp_cursor(*idx);
                this.sidebar_open_mcp_at(*idx);
                cx.notify();
            }));
        let on_open_skill: sidebar::sections::SkillOpenCallback =
            Rc::new(cx.listener(|this, idx: &usize, _window, cx| {
                this.sidebar.set_skills_cursor(*idx);
                this.sidebar_open_skill_at(*idx);
                cx.notify();
            }));
        let on_open_steering: sidebar::sections::SteeringOpenCallback =
            Rc::new(cx.listener(|this, idx: &usize, _window, cx| {
                this.sidebar.set_steering_cursor(*idx);
                this.sidebar_open_steering_at(*idx);
                cx.notify();
            }));

        let sidebar_ctx = sidebar::render::SidebarRenderCx {
            workspaces: &self.workspaces,
            colors: &self.config.colors,
            active_section: self.sidebar.active_section(),
            on_select_workspace,
            on_new_workspace,
            on_close_workspace,
            on_select_section,
            workspace_rename: workspace_rename_element,
            mcp_manager: &self.mcp_manager,
            mcp_cursor: self.sidebar.mcp_cursor(),
            on_open_mcp,
            skill_manager: &self.skill_manager,
            skills_cursor: self.sidebar.skills_cursor(),
            on_open_skill,
            steering_manager: &self.steering_manager,
            steering_cursor: self.sidebar.steering_cursor(),
            on_open_steering,
        };
        let bar = sidebar::render::render_workspace_sidebar(sidebar_ctx);
        bar.with_animation(
            "workspace-sidebar-drawer",
            Animation::new(SIDEBAR_OPEN_ANIM).with_easing(ease_out_quint()),
            |bar, delta| bar.w(px(sidebar::render::SIDEBAR_WIDTH_PX * delta)),
        )
    }
}
