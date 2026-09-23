// The command palette's action list and dispatch table. Not handled in
// gpui_shell: theme picker, Enable/DisableAiFeatures, ClearAiContext,
// TrustLocalMcp, GitCheckout. `gpui_shell_actions` and
// `dispatch_palette_action` must stay in sync: every variant filtered in
// has a real arm below.

use gpui::{Context, Window};

use crate::config::Config;
use crate::ui::palette::actions::built_in_actions;
use crate::ui::palette::{Action, PaletteAction};

use super::leader::LeaderAction;
use super::GpuiShellRoot;

/// Build the palette's item list for `gpui_shell`: the wgpu build's own
/// `built_in_actions(config)`, filtered down to variants
/// `dispatch_palette_action` actually handles. Called fresh on every open
/// (matches `CommandPalette::open()`'s own "rebuild from `all_actions`"
/// behavior for the has-a-dispatch-target case).
pub(super) fn gpui_shell_actions(config: &Config) -> Vec<PaletteAction> {
    built_in_actions(config)
        .into_iter()
        .filter(|item| {
            matches!(
                item.action,
                Action::NewTab
                    | Action::CloseTab
                    | Action::NextTab
                    | Action::PrevTab
                    | Action::RenameTab
                    | Action::NewWorkspace
                    | Action::CloseWorkspace
                    | Action::RenameWorkspace
                    | Action::NextWorkspace
                    | Action::PrevWorkspace
                    | Action::SplitHorizontal
                    | Action::SplitVertical
                    | Action::ClosePane
                    | Action::ZoomPane
                    | Action::FocusPane(_)
                    | Action::ToggleAiPanel
                    | Action::FocusAiPanel
                    | Action::ToggleFullscreen
                    | Action::Quit
                    | Action::ToggleStatusBar
                    | Action::OpenConfigFile
                    | Action::OpenConfigFolder
                    | Action::ReloadConfig
                    | Action::SwitchToTab(_)
                    | Action::OpenBranchPicker
                    | Action::ExpandSnippet(_)
                    | Action::SaveWorkspace
                    | Action::OpenSavedWorkspaces
                    | Action::RestoreWorkspace(_)
                    | Action::ExplainLastOutput
                    | Action::FixLastError
            )
        })
        .collect()
}

/// Convert the wgpu-native `crate::ui::panes::FocusDir` (what `Action::
/// FocusPane` carries) to `gpui_shell`'s own, structurally identical but
/// separately defined, `panes::FocusDir` -- the two are parallel
/// types, not the same one, matching this codebase's established "mirror,
/// don't wrap" relationship for anything ported from `Mux`/`src/ui/`.
fn convert_focus_dir(dir: crate::ui::panes::FocusDir) -> super::panes::FocusDir {
    match dir {
        crate::ui::panes::FocusDir::Left => super::panes::FocusDir::Left,
        crate::ui::panes::FocusDir::Right => super::panes::FocusDir::Right,
        crate::ui::panes::FocusDir::Up => super::panes::FocusDir::Up,
        crate::ui::panes::FocusDir::Down => super::panes::FocusDir::Down,
    }
}

impl GpuiShellRoot {
    /// Run one confirmed palette action.
    pub(super) fn dispatch_palette_action(
        &mut self,
        action: Action,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match action {
            Action::NewTab => self.dispatch_leader_action(LeaderAction::NewTab, window, cx),
            Action::CloseTab => self.dispatch_leader_action(LeaderAction::CloseTab, window, cx),
            Action::NextTab => self.dispatch_leader_action(LeaderAction::NextTab, window, cx),
            Action::PrevTab => self.dispatch_leader_action(LeaderAction::PrevTab, window, cx),
            Action::RenameTab => self.dispatch_leader_action(LeaderAction::RenameTab, window, cx),
            Action::NewWorkspace => {
                self.dispatch_leader_action(LeaderAction::NewWorkspace, window, cx)
            }
            Action::CloseWorkspace => {
                self.dispatch_leader_action(LeaderAction::CloseWorkspace, window, cx)
            }
            Action::RenameWorkspace => {
                self.dispatch_leader_action(LeaderAction::RenameWorkspace, window, cx)
            }
            Action::NextWorkspace => {
                self.dispatch_leader_action(LeaderAction::NextWorkspace, window, cx)
            }
            Action::PrevWorkspace => {
                self.dispatch_leader_action(LeaderAction::PrevWorkspace, window, cx)
            }
            Action::SplitHorizontal => {
                self.dispatch_leader_action(LeaderAction::SplitHorizontal, window, cx)
            }
            Action::SplitVertical => {
                self.dispatch_leader_action(LeaderAction::SplitVertical, window, cx)
            }
            Action::ClosePane => self.dispatch_leader_action(LeaderAction::ClosePane, window, cx),
            Action::ZoomPane => self.dispatch_leader_action(LeaderAction::ZoomPane, window, cx),
            Action::FocusPane(dir) => self.dispatch_leader_action(
                LeaderAction::FocusPane(convert_focus_dir(dir)),
                window,
                cx,
            ),
            Action::ToggleAiPanel => {
                self.dispatch_leader_action(LeaderAction::ToggleAiPanel, window, cx)
            }
            // `ChatPanelView` has no separate "focus without toggling
            // closed" entry point (confirmed: no `focus_composer` method
            // exists) -- `FocusAiPanel` falls back to the same behavior as
            // `ToggleAiPanel` itself, which already opens-and-focuses when
            // closed via `self.chat.toggle` (`leader_dispatch.rs`'s own
            // `ToggleAiPanel` arm), so an already-open panel closes.
            Action::FocusAiPanel => {
                self.dispatch_leader_action(LeaderAction::ToggleAiPanel, window, cx)
            }
            Action::ToggleFullscreen => window.toggle_fullscreen(),
            Action::Quit => cx.quit(),
            Action::ToggleStatusBar => {
                self.config.status_bar.enabled = !self.config.status_bar.enabled;
            }
            Action::OpenConfigFile => {
                let _ = std::process::Command::new("open")
                    .arg(crate::config::config_path())
                    .spawn();
            }
            Action::OpenConfigFolder => {
                let _ = std::process::Command::new("open")
                    .arg(crate::config::config_dir())
                    .spawn();
            }
            Action::ReloadConfig => {
                if let Ok((new_config, _lua)) = crate::config::reload() {
                    *super::config_watch::PENDING_CONFIG_RELOAD.lock().unwrap() = Some(new_config);
                    super::config_watch::CONFIG_CHANGED
                        .store(true, std::sync::atomic::Ordering::Release);
                }
            }
            Action::SwitchToTab(n) if self.workspaces.active_mut().tabs.switch_to_index(n) => {
                cx.notify();
            }
            Action::ExpandSnippet(body) => {
                let active_ws = self.workspaces.active();
                let active_tid =
                    active_ws.tab_panes[active_ws.tabs.active_index()].focused_terminal;
                if let Some(terminal) = self.terminals.get(&active_tid) {
                    terminal.write_input(body.as_bytes());
                }
            }
            Action::OpenBranchPicker => {
                if let Some(cwd) = self.cached_cwd.clone() {
                    self.open_branch_picker(&cwd);
                }
            }
            Action::SaveWorkspace => {
                if let Err(e) = self.save_active_workspace() {
                    log::error!("save_active_workspace: {e}");
                }
            }
            Action::OpenSavedWorkspaces => {
                let items: Vec<crate::ui::palette::PaletteAction> =
                    crate::app::mux::snapshot::list_saved_workspaces()
                        .into_iter()
                        .map(|info| crate::ui::palette::PaletteAction {
                            name: format!(
                                "{} ({} tabs) — {}",
                                info.name, info.tab_count, info.saved_at
                            ),
                            action: crate::ui::palette::Action::RestoreWorkspace(
                                info.path.to_string_lossy().into_owned(),
                            ),
                            keybind: None,
                        })
                        .collect();
                if items.is_empty() {
                    self.palette.open();
                } else {
                    self.palette.open_with_items(items);
                }
            }
            Action::RestoreWorkspace(path) => {
                match crate::app::mux::snapshot::load_workspace(&std::path::PathBuf::from(&path)) {
                    Ok(snap) => self.restore_workspace(snap, cx),
                    Err(e) => log::error!("load_workspace: {e}"),
                }
            }
            Action::ExplainLastOutput => self.explain_last_output(window, cx),
            Action::FixLastError => self.fix_last_error(window, cx),
            // Variants filtered out of `gpui_shell_actions` land here as a
            // no-op. Reachable: the branch picker (`branch_picker.rs`) emits
            // `GitCheckout`, which is not handled yet.
            _ => {}
        }
    }
}
