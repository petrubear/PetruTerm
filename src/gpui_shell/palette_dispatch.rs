// gpui chrome migration (M4a Task 3): the command palette's real action
// list and dispatch table -- replaces Task 2's 3-item `interim_actions`.
// Every `Action` variant NOT listed in either function here is one this
// milestone's own spec (docs/superpowers/specs/2026-09-07-gpui-m4-
// remaining-surfaces-design.md, §7) explicitly defers: no gpui_shell
// equivalent exists yet (snippets, saved workspaces, git-branch picker,
// theme picker, M3b's already-deferred AI actions, command-block/hover-
// link-dependent actions). `gpui_shell_actions` and `dispatch_
// palette_action` below are two views of the same "which actions does
// gpui_shell support" boundary and must be kept in sync by construction --
// every variant filtered IN here has a real arm below, and vice versa.

use gpui::{Context, Window};

use crate::config::Config;
use crate::ui::palette::actions::built_in_actions;
use crate::ui::palette::{Action, PaletteAction};

use super::leader::LeaderAction;
use super::GpuiShellRoot;

/// Build the palette's item list for `gpui_shell`: the wgpu build's own
/// `built_in_actions(config)`, filtered down to variants this milestone's
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
            )
        })
        .collect()
}

/// Convert the wgpu-native `crate::ui::panes::FocusDir` (what `Action::
/// FocusPane` carries) to `gpui_shell`'s own, structurally identical but
/// separately defined, `panes::FocusDir` (M2) -- the two are parallel
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
    /// Run one confirmed palette action -- the real dispatch table, per the
    /// M4 spec's §3 mapping. Replaces Task 2's 3-arm interim version.
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
            // `ToggleAiPanel` arm). Not a perfect "focus without toggle"
            // semantic if the panel is already open, but inventing a new
            // entry point is out of this task's scope.
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
            // Every other variant is filtered out of `gpui_shell_actions`
            // (see its own doc comment) -- unreachable in practice, kept as
            // an explicit no-op rather than a `panic!`/`unreachable!()`
            // since a stale `CommandPalette::results` entry surviving a
            // hot-reload race is a cosmetic miss, not a crash-worthy one.
            _ => {}
        }
    }
}
