// gpui chrome migration (M2 Task 5b): `impl Render for GpuiShellRoot`.
// Split out of `mod.rs` for the 400-line convention.

use std::rc::Rc;

use gpui::{div, prelude::*, Context, Render, Window};

use super::pane_view::to_rgba;
use super::{pane_view, status_bar, tabs, GpuiShellRoot};

impl Render for GpuiShellRoot {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Skipped while a child owns focus. This runs every frame, and the
        // poll loop repaints at ~30Hz, so focusing unconditionally would tear
        // focus away from the rename editor ~30 times a second and make it
        // look like typing does nothing.
        if self.tab_rename.is_none() {
            window.focus(&self.focus_handle);
        }

        let active_index = self.tabs.active_index();
        let (cell_width, cell_height) = super::font_state::measured_cell_size();

        // A zoomed pane that no longer belongs to the active tab (tab switch,
        // pane closed) has to be dropped before it's used, mirroring the wgpu
        // app's own "zoomed pane no longer in active tab -- clear zoom" guard
        // in src/app/frame.rs.
        if let Some(id) = self.zoomed_pane {
            if !self.tab_panes[active_index].root.leaf_ids().contains(&id) {
                self.zoomed_pane = None;
            }
        }

        // Drop last frame's geometry before this frame's prepaint pass
        // repopulates it: a pane that just closed, or one belonging to a tab
        // that's no longer active, must not keep answering focus_dir's
        // nearest-neighbour search with a rect it no longer occupies.
        {
            let mut rects = self.rect_cache.borrow_mut();
            rects.leaves.clear();
            rects.separators.clear();
        }

        // Both callbacks below outlive `render()` (they're owned by the
        // element tree and run during event dispatch), so they hold a WEAK
        // handle -- exactly what `Context::listener` does internally, and for
        // the same reason: a strong `Entity<Self>` parked in a per-frame
        // closure would keep this view alive past window close.
        let view = cx.entity().downgrade();
        let focus_view = view.clone();
        let on_focus: pane_view::PaneFocusCallback = Rc::new(move |terminal_id, _window, cx| {
            focus_view
                .update(cx, |root, cx| {
                    let active = root.tabs.active_index();
                    if root.tab_panes[active].focused_terminal != terminal_id {
                        root.tab_panes[active].focused_terminal = terminal_id;
                        cx.notify();
                    }
                })
                .ok();
        });
        let drag_view = view;
        let on_drag: pane_view::SeparatorDragCallback =
            Rc::new(move |node_id, position, _window, cx| {
                drag_view
                    .update(cx, |root, cx| {
                        // Clone the Rc first: `drag_separator` needs `&mut
                        // self.tab_panes[..]` and `&self.rect_cache`'s
                        // contents at once, which a single `root.` borrow of
                        // both fields can't express.
                        let rects = root.rect_cache.clone();
                        let rects = rects.borrow();
                        let active = root.tabs.active_index();
                        root.tab_panes[active].drag_separator(
                            node_id,
                            f32::from(position.x),
                            f32::from(position.y),
                            &rects,
                        );
                        cx.notify();
                    })
                    .ok();
            });

        let pane_ctx = pane_view::PaneRenderCx {
            terminals: &self.terminals,
            focused: self.tab_panes[active_index].focused_terminal,
            colors: &self.config.colors,
            cell_width,
            cell_height,
            cursor_blink_on: self.cursor_blink_on,
            scrollback: self.config.scrollback_lines as usize,
            rects: self.rect_cache.clone(),
            on_focus,
            on_drag,
        };
        let panes = match self.zoomed_pane {
            Some(terminal_id) => pane_view::render_leaf(terminal_id, &pane_ctx),
            None => pane_view::render_pane_tree(&self.tab_panes[active_index].root, &pane_ctx),
        };

        let on_select_tab: tabs::TabSelectCallback =
            Rc::new(cx.listener(|this, idx: &usize, _window, cx| {
                if this.tabs.switch_to_index(*idx) {
                    cx.notify();
                }
            }));
        let rename_editor = self
            .tab_rename
            .as_ref()
            .map(|input| input.clone().into_any_element());
        let tab_bar = tabs::render_tab_bar(
            &self.tabs,
            &self.config.colors,
            on_select_tab,
            rename_editor,
        );

        // Status bar row -- built from the poll-loop-refreshed cwd/git-branch/
        // exit-code state above plus this frame's leader/zoom state, same
        // inputs `StatusBar::build` takes in the wgpu app's own render path
        // (`src/app/frame.rs`). `leader_resize_mode` here is `resize_mode`
        // (set by a completed `Leader Option+Arrow`) OR a live separator
        // drag, since gpui_shell has no `ModifiersChanged`-equivalent hook
        // to ask "is Option currently held" outside of a keystroke (see
        // `on_key_down`'s own doc comment on that gap).
        let status_bar_row = self.config.status_bar.enabled.then(|| {
            let leader_resize_mode = self.resize_mode || pane_view::is_dragging_separator();
            let sb_colors = self.config.colors.status_bar_colors();
            let bar = status_bar::StatusBar::build(
                self.leader_active,
                leader_resize_mode,
                &self.config.leader.key,
                self.cached_cwd.as_deref(),
                self.git_branch.cache.as_deref(),
                self.exit_code.cache,
                self.zoomed_pane.is_some(),
                self.config.status_bar.style.clone(),
                None, // battery -- not tracked in gpui_shell yet, out of this task's scope
                &sb_colors,
            );
            status_bar::render_status_bar(&bar, &sb_colors)
        });

        div()
            .track_focus(&self.focus_handle)
            .on_key_down(cx.listener(Self::on_key_down))
            .flex()
            .flex_col()
            .size_full()
            .bg(to_rgba(self.config.colors.background))
            .child(tab_bar)
            // `min_h_0`: a flex item's automatic minimum size is its content
            // size, so without this the pane row refuses to shrink below the
            // terminal grid it contains and pushes the tab bar off-screen on
            // a small window.
            .child(div().flex().flex_1().min_h_0().child(panes))
            .when_some(status_bar_row, |el, bar| el.child(bar))
    }
}
