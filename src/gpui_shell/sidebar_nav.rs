// gpui chrome migration (M3d Task 3 post-review split): the sidebar's own
// keyboard-navigation dispatch -- moving the per-section cursor and
// activating the cursor's row. Split out of `input.rs` for the 400-line
// convention: `input.rs` grew to 456 lines once M3d Task 3's sidebar guard
// and these two methods landed on top of it (the same failure class M3c's
// `actions.rs` hit -- see that file's own history). Task 4 adds three more
// methods here (`sidebar_open_mcp_at`/`sidebar_open_skill_at`/
// `sidebar_open_steering_at`) and fills in these two methods' Mcp/Skills/
// Steering arms, keeping the growth contained to this file instead of
// re-inflating `input.rs`.

use gpui::{Context, KeyDownEvent, Window};

use super::{sidebar, GpuiShellRoot};

impl GpuiShellRoot {
    /// The workspace sidebar's own key guard, called from `input.rs`'s
    /// `on_key_down` once `sidebar_focus_handle.is_focused(window)` is
    /// confirmed there -- kept as a separate call (not inlined) so that
    /// guard's own doc comment in `input.rs` can stay short.
    ///
    /// `ToggleWorkspaceSidebar`'s dispatch arm (`leader_dispatch.rs`) is the
    /// only place that moves focus TO this handle via the keyboard; clicking
    /// a row or section tab (Task 4 / M3d Task 3's own section-tab clicks)
    /// must do the same explicitly, same fix `TextInput::on_mouse_down`
    /// needed in M3a.
    pub(super) fn handle_sidebar_focused_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event.keystroke.key.as_str() {
            "tab" => {
                if event.keystroke.modifiers.shift {
                    self.sidebar.prev_section();
                } else {
                    self.sidebar.next_section();
                }
            }
            "down" | "j" => self.sidebar_move_cursor(1, cx),
            "up" | "k" => self.sidebar_move_cursor(-1, cx),
            "enter" => self.sidebar_activate_selection(cx),
            "escape" => {
                self.sidebar.toggle();
                window.focus(&self.focus_handle);
            }
            _ => {}
        }
        cx.notify();
    }

    /// Move the highlighted row within whichever section is active by
    /// `delta` (`+1`/`-1`). Only the Workspaces arm does anything yet --
    /// arrow-nav there switches immediately, same as a click (see this
    /// plan's Global Constraints on why that section's cursor IS its active
    /// index, unlike the wgpu reference's decoupled highlight-then-Enter).
    /// The other three arms are Task 4's to fill in, once their sections
    /// have real lists to move a cursor over.
    pub(super) fn sidebar_move_cursor(&mut self, delta: i32, cx: &mut Context<Self>) {
        match self.sidebar.active_section() {
            sidebar::SidebarSection::Workspaces => {
                let len = self.workspaces.len();
                if len == 0 {
                    return;
                }
                let current = self.workspaces.active_index() as i32;
                let next = (current + delta).rem_euclid(len as i32) as usize;
                self.switch_workspace_to_index(next);
            }
            sidebar::SidebarSection::Mcp => {
                let count = {
                    let mut set: std::collections::BTreeSet<String> = Default::default();
                    for (server, _) in self.mcp_manager.all_tools() {
                        set.insert(server);
                    }
                    set.len()
                };
                if count > 0 {
                    let current = self.sidebar.mcp_cursor() as i32;
                    let next = (current + delta).rem_euclid(count as i32) as usize;
                    self.sidebar.set_mcp_cursor(next);
                }
            }
            sidebar::SidebarSection::Skills => {
                let count = self.skill_manager.skills().len();
                if count > 0 {
                    let current = self.sidebar.skills_cursor() as i32;
                    let next = (current + delta).rem_euclid(count as i32) as usize;
                    self.sidebar.set_skills_cursor(next);
                }
            }
            sidebar::SidebarSection::Steering => {
                let count = self.steering_manager.files().len();
                if count > 0 {
                    let current = self.sidebar.steering_cursor() as i32;
                    let next = (current + delta).rem_euclid(count as i32) as usize;
                    self.sidebar.set_steering_cursor(next);
                }
            }
        }
        cx.notify();
    }

    pub(super) fn sidebar_activate_selection(&mut self, cx: &mut Context<Self>) {
        match self.sidebar.active_section() {
            sidebar::SidebarSection::Workspaces => {
                // Arrow-nav already switched; nothing left for Enter to do.
            }
            sidebar::SidebarSection::Mcp => self.sidebar_open_mcp_at(self.sidebar.mcp_cursor()),
            sidebar::SidebarSection::Skills => {
                self.sidebar_open_skill_at(self.sidebar.skills_cursor())
            }
            sidebar::SidebarSection::Steering => {
                self.sidebar_open_steering_at(self.sidebar.steering_cursor())
            }
        }
        cx.notify();
    }

    pub(super) fn sidebar_open_mcp_at(&mut self, idx: usize) {
        let mut servers: Vec<String> = {
            let mut set: std::collections::BTreeSet<String> = Default::default();
            for (server, _) in self.mcp_manager.all_tools() {
                set.insert(server);
            }
            set.into_iter().collect()
        };
        servers.sort();
        if let Some(name) = servers.get(idx) {
            let content = super::mcp_overlay::mcp_overlay_content(&self.mcp_manager, name);
            self.info_overlay.open(name.clone(), &content);
        }
    }

    pub(super) fn sidebar_open_skill_at(&mut self, idx: usize) {
        if let Some(skill) = self.skill_manager.skills().get(idx) {
            let title = skill.name.clone();
            let content = self
                .skill_manager
                .read_body(skill)
                .unwrap_or_else(|e| format!("Error reading skill: {e}"));
            self.info_overlay.open(title, &content);
        }
    }

    pub(super) fn sidebar_open_steering_at(&mut self, idx: usize) {
        if let Some((name, content)) = self.steering_manager.files().get(idx) {
            let display = name.strip_suffix(".md").unwrap_or(name).to_string();
            self.info_overlay.open(display, content);
        }
    }
}
