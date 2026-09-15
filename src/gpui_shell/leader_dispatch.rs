// gpui chrome migration (M3c post-Task-4 split): `dispatch_leader_action`,
// the single big match over every resolved leader-key action. Split out of
// `actions.rs` for the 400-line convention -- `actions.rs` grew past 400
// lines once M3c's three workspace tasks landed on top of it, and this
// function alone (all sixteen `LeaderAction` variants) was the single
// largest contributor. Pure code motion: no logic changed.

use gpui::{Context, Focusable, Window};

use super::leader::LeaderAction;
use super::panes::{PaneForest, SplitDir};
use super::{spawn_terminal, GpuiShellRoot};

impl GpuiShellRoot {
    /// Execute one resolved leader-key action (`on_key_down`'s leader
    /// dispatch branch). See `leader::LeaderAction`'s doc comment for why
    /// the set stops at these variants.
    pub(super) fn dispatch_leader_action(
        &mut self,
        action: LeaderAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match action {
            LeaderAction::NewTab => {
                let (terminal, gate) = match spawn_terminal(80, 24, &self.config) {
                    Ok(pair) => pair,
                    Err(e) => {
                        log::error!("gpui-shell: failed to spawn terminal for new tab: {e:#}");
                        return;
                    }
                };
                let terminal_id = self.next_terminal_id;
                self.next_terminal_id += 1;
                self.terminals.insert(terminal_id, terminal);
                self.wakeup_gates.insert(terminal_id, gate);
                self.block_managers
                    .insert(terminal_id, crate::term::BlockManager::new());
                let ws = self.workspaces.active_mut();
                ws.tabs.new_tab("zsh");
                ws.tab_panes.push(PaneForest::new(terminal_id));
                // Same reasoning as `split_focused`: a zoomed pane from the
                // tab being left would otherwise linger, filling the window
                // even after the new tab (which has nothing zoomed) becomes
                // active.
                ws.zoomed_pane = None;
            }
            LeaderAction::CloseTab => {
                // Mirrors `Mux::cmd_close_tab` (src/app/mux/mod.rs:792-807),
                // via the shared `close_tab_at` helper (also used by
                // `on_terminal_exited` for the "shell exited as a tab's
                // last pane" case) so the two close paths can't drift
                // apart. `signal_shells: true` since this tab's shells may
                // still be alive (the user is closing it explicitly, not
                // reacting to an exit already observed).
                let ws_idx = self.workspaces.active_index();
                let tab_idx = self.workspaces.active().tabs.active_index();
                self.close_tab_at(ws_idx, tab_idx, true, cx);
            }
            LeaderAction::NextTab => self.workspaces.active_mut().tabs.next_tab(),
            LeaderAction::PrevTab => self.workspaces.active_mut().tabs.prev_tab(),
            LeaderAction::RenameTab => self.begin_tab_rename(window, cx),
            LeaderAction::SplitHorizontal => self.split_focused(SplitDir::Horizontal),
            LeaderAction::SplitVertical => self.split_focused(SplitDir::Vertical),
            LeaderAction::ClosePane => self.close_focused_pane(cx),
            LeaderAction::ZoomPane => self.toggle_zoom(),
            LeaderAction::FocusPane(dir) => {
                let active = self.workspaces.active().tabs.active_index();
                // Clone the Rc first, same reason as `on_drag` in render():
                // `focus_dir` needs `&mut self.workspaces.active_mut().
                // tab_panes[..]` and `&self.rect_cache`'s contents at once,
                // which a single `self.` borrow of both fields can't
                // express.
                let rects = self.rect_cache.clone();
                let rects = rects.borrow();
                self.workspaces.active_mut().tab_panes[active].focus_dir(dir, &rects);
            }
            LeaderAction::ToggleAiPanel => {
                self.chat.toggle(window, cx);
                if self.chat.is_visible() {
                    // `init_default_files` is idempotent (checks `attached_
                    // files.contains` before adding), matching the wgpu
                    // build's own `open_panel_with_context`, which calls it
                    // unconditionally on every open -- safe to call here
                    // every time the panel opens, not just the first time.
                    if let Some(cwd) = self.cached_cwd.clone() {
                        self.chat.panel.init_default_files(&cwd);
                    }
                } else {
                    // `toggle` only ever moves focus TO the composer
                    // (opening); closing deliberately returns none,
                    // mirroring `end_tab_rename`'s division of labor. This
                    // is the other half: send focus back to the terminal
                    // right here rather than waiting on render()'s guard,
                    // which can't tell "the panel just closed" from "the
                    // composer still holds a stale focus handle" -- gpui
                    // doesn't clear a `FocusHandle`'s focused status just
                    // because its element left the tree.
                    window.focus(&self.focus_handle);
                }
            }
            LeaderAction::ExplainLastOutput => self.explain_last_output(window, cx),
            LeaderAction::FixLastError => self.fix_last_error(window, cx),
            LeaderAction::UndoLastWrite => self.undo_last_write(),
            LeaderAction::NewWorkspace => {
                let name = format!("ws{}", self.workspaces.len() + 1);
                let (terminal, gate) = match spawn_terminal(80, 24, &self.config) {
                    Ok(pair) => pair,
                    Err(e) => {
                        log::error!(
                            "gpui-shell: failed to spawn terminal for new workspace: {e:#}"
                        );
                        return;
                    }
                };
                let terminal_id = self.next_terminal_id;
                self.next_terminal_id += 1;
                self.terminals.insert(terminal_id, terminal);
                self.wakeup_gates.insert(terminal_id, gate);
                self.block_managers
                    .insert(terminal_id, crate::term::BlockManager::new());
                self.workspaces.new_workspace(name);
                self.workspaces.active_mut().tabs.new_tab("zsh");
                self.workspaces
                    .active_mut()
                    .tab_panes
                    .push(PaneForest::new(terminal_id));
                self.tab_rename = None;
            }
            LeaderAction::CloseWorkspace => {
                let ws_idx = self.workspaces.active_index();
                self.close_workspace_at(ws_idx, true, cx);
            }
            LeaderAction::NextWorkspace => self.next_workspace(),
            LeaderAction::PrevWorkspace => self.prev_workspace(),
            LeaderAction::RenameWorkspace => self.begin_workspace_rename(window, cx),
            LeaderAction::ToggleWorkspaceSidebar => {
                self.sidebar.toggle();
                // Same division of labor `ToggleAiPanel` already has: opening
                // moves focus TO the sidebar so Tab/arrow navigation (Task 3)
                // works immediately; closing gives it back to the terminal
                // rather than leaving a stale focus target (`is_focused`
                // would otherwise report `true` for a handle nothing can see
                // or type into anymore).
                if self.sidebar.is_visible() {
                    window.focus(&self.sidebar_focus_handle);
                } else {
                    window.focus(&self.focus_handle);
                }
            }
            LeaderAction::OpenCommandPalette => {
                self.palette_query
                    .update(cx, |input, cx| input.set_content("", cx));
                self.palette
                    .open_with_items(super::palette_dispatch::gpui_shell_actions(&self.config));
                self.palette_query.focus_handle(cx).focus(window);
            }
        }
        cx.notify();
    }
}
