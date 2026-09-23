// Builds the workspace sidebar drawer element. Called only from `render()`'s
// `middle_row` construction, inside `.when(self.sidebar.is_visible(), ...)`.

use std::rc::Rc;
use std::time::Duration;

use gpui::{div, ease_out_quint, prelude::*, px, Animation, AnimationExt as _, Context};

use super::leader::LeaderAction;
use super::pane_view::to_rgba;
use super::resize_handle::{self, ResizeHandleElement};
use super::sidebar;
use super::sidebar::render::{MAX_SIDEBAR_WIDTH_PX, MIN_SIDEBAR_WIDTH_PX};
use super::sidebar::SidebarSection;
use super::GpuiShellRoot;

/// Duration of the drawer's opening grow animation.
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

        let width_px = self.sidebar_width_px;
        let sidebar_ctx = sidebar::render::SidebarRenderCx {
            workspaces: &self.workspaces,
            colors: &self.config.colors,
            width_px,
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
        let bar = bar.with_animation(
            "workspace-sidebar-drawer",
            Animation::new(SIDEBAR_OPEN_ANIM).with_easing(ease_out_quint()),
            move |bar, delta| bar.w(px(width_px * delta)),
        );

        // Drag handle -- requested live after the fixed 220px width was
        // reported unusable at anything short of a maximized window.
        // `resize_handle.rs`'s own doc comment covers why this needs a
        // custom `Element` rather than plain `div()` mouse listeners.
        // Mirrors `render_callbacks.rs`'s `on_drag` (pane-separator
        // dragging) exactly: a weak handle + manual `.update()`, not
        // `cx.listener`, because the callback's own event type
        // (`Point<Pixels>`, by value) doesn't match what `cx.listener`
        // expects (a reference).
        //
        // Floats (`.absolute()`, `right(-CARD_GAP_PX)`) over the
        // `CARD_GAP_PX` gap `middle_row` already leaves between this card
        // and its neighbor, rather than being a normal flex sibling that
        // adds its own width on top of that gap -- the earlier flex-sibling
        // version doubled the visual space between the sidebar and the
        // terminal card (handle width + the row's own gap + both cards' own
        // borders), visibly wider than every other card-to-card gap in the
        // window. This keeps every gap in the layout the same width, with nothing
        // added on top.
        let drag_view = cx.entity().downgrade();
        let on_drag: resize_handle::ResizeDragCallback = Rc::new(move |position, _window, cx| {
            drag_view
                .update(cx, |root, cx| {
                    let new_width =
                        f32::from(position.x).clamp(MIN_SIDEBAR_WIDTH_PX, MAX_SIDEBAR_WIDTH_PX);
                    if root.sidebar_width_px != new_width {
                        root.sidebar_width_px = new_width;
                        cx.notify();
                    }
                })
                .ok();
        });
        let handle_color = to_rgba(self.config.colors.ui_border);
        let handle = div()
            .absolute()
            .top_0()
            .right(px(-super::render::CARD_GAP_PX))
            .h_full()
            .w(px(super::render::CARD_GAP_PX))
            .cursor_col_resize()
            .child(ResizeHandleElement {
                color: handle_color,
                on_drag,
            });

        div().relative().h_full().child(bar).child(handle)
    }
}
