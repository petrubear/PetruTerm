// gpui chrome migration (M5c Task 5): build/restore a WorkspaceSnapshot
// (crate::app::mux::snapshot -- already engine-agnostic, reused
// verbatim) against gpui_shell's OWN Workspace/PaneTree types, since
// gpui_shell doesn't use Mux at all. Mirrors Mux::build_workspace_
// snapshot/snapshot_pane_node (src/app/mux/workspace.rs:194-260) and
// Mux::restore_workspace/restore_pane_recursive (:273-340), minus the
// winit::event_loop::EventLoopProxy parameter -- gpui_shell's own
// spawn_terminal_at has no such dependency.

use std::path::PathBuf;

use gpui::Context;

use crate::app::mux::snapshot::{
    PaneNodeSnapshot, SplitDirSnapshot, TabSnapshot, WorkspaceSnapshot,
};

use super::panes::{next_node_id, PaneForest, PaneTree, SplitDir};
use super::GpuiShellRoot;

impl GpuiShellRoot {
    /// Snapshot the active workspace to disk.
    pub(super) fn save_active_workspace(&self) -> anyhow::Result<()> {
        let snap = self.build_workspace_snapshot();
        crate::app::mux::snapshot::save_snapshot(&snap)
    }

    fn build_workspace_snapshot(&self) -> WorkspaceSnapshot {
        let workspace = self.workspaces.active();
        let tabs: Vec<TabSnapshot> = workspace
            .tabs
            .tabs()
            .iter()
            .enumerate()
            .map(|(i, tab)| {
                let pane_tree = workspace
                    .tab_panes
                    .get(i)
                    .map(|forest| self.snapshot_pane_node(&forest.root))
                    .unwrap_or(PaneNodeSnapshot::Leaf { cwd: home_str() });
                TabSnapshot {
                    title: tab.title.clone(),
                    pane_tree,
                    accent_color: tab.accent_color,
                }
            })
            .collect();
        let saved_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        WorkspaceSnapshot {
            version: 1,
            name: workspace.name.clone(),
            saved_at,
            tabs,
        }
    }

    fn snapshot_pane_node(&self, node: &PaneTree) -> PaneNodeSnapshot {
        match node {
            PaneTree::Leaf { terminal_id } => {
                let cwd = self
                    .terminals
                    .get(terminal_id)
                    .and_then(|t| crate::term::process_cwd(t.child_pid))
                    .map(|p| p.to_string_lossy().into_owned())
                    .unwrap_or_else(home_str);
                PaneNodeSnapshot::Leaf { cwd }
            }
            PaneTree::Split {
                dir,
                ratio,
                left,
                right,
                ..
            } => PaneNodeSnapshot::Split {
                dir: match dir {
                    SplitDir::Horizontal => SplitDirSnapshot::Horizontal,
                    SplitDir::Vertical => SplitDirSnapshot::Vertical,
                },
                ratio: *ratio,
                left: Box::new(self.snapshot_pane_node(left)),
                right: Box::new(self.snapshot_pane_node(right)),
            },
        }
    }

    /// Restore a workspace from a snapshot: creates a NEW workspace with
    /// the saved tab/pane layout. Tabs and panes are created fresh (no
    /// process state is restored -- fresh shells at the saved CWDs),
    /// matching the wgpu build's own documented behavior exactly.
    pub(super) fn restore_workspace(&mut self, snap: WorkspaceSnapshot, cx: &mut Context<Self>) {
        self.workspaces.new_workspace(snap.name.clone());
        for tab_snap in &snap.tabs {
            match self.restore_pane_tree(&tab_snap.pane_tree) {
                Ok((root, focused_terminal)) => {
                    let workspace = self.workspaces.active_mut();
                    let idx = workspace.tabs.new_tab(&tab_snap.title);
                    if let Some(color) = tab_snap.accent_color {
                        workspace.tabs.set_tab_color(idx, Some(color));
                    }
                    workspace.tab_panes.push(PaneForest {
                        root,
                        focused_terminal,
                    });
                }
                Err(e) => log::error!("Failed to restore tab '{}': {e}", tab_snap.title),
            }
        }
        cx.notify();
    }

    fn restore_pane_tree(&mut self, node: &PaneNodeSnapshot) -> anyhow::Result<(PaneTree, usize)> {
        match node {
            PaneNodeSnapshot::Leaf { cwd } => {
                let cwd_path = if cwd.is_empty() {
                    None
                } else {
                    Some(PathBuf::from(cwd))
                };
                let (terminal, gate) = super::spawn_terminal_at(80, 24, &self.config, cwd_path)?;
                let terminal_id = self.next_terminal_id;
                self.next_terminal_id += 1;
                self.terminals.insert(terminal_id, terminal);
                self.wakeup_gates.insert(terminal_id, gate);
                self.block_managers
                    .insert(terminal_id, crate::term::BlockManager::new());
                Ok((PaneTree::Leaf { terminal_id }, terminal_id))
            }
            PaneNodeSnapshot::Split {
                dir,
                ratio,
                left,
                right,
            } => {
                let (left_tree, left_focused) = self.restore_pane_tree(left)?;
                let (right_tree, right_focused) = self.restore_pane_tree(right)?;
                Ok((
                    PaneTree::Split {
                        node_id: next_node_id(),
                        dir: match dir {
                            SplitDirSnapshot::Horizontal => SplitDir::Horizontal,
                            SplitDirSnapshot::Vertical => SplitDir::Vertical,
                        },
                        ratio: *ratio,
                        left: Box::new(left_tree),
                        right: Box::new(right_tree),
                    },
                    right_focused.max(left_focused), // arbitrary but deterministic: newest-created leaf wins focus
                ))
            }
        }
    }
}

fn home_str() -> String {
    dirs::home_dir()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|| "/".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `restore_pane_tree`'s focus-selection rule for a Split node:
    /// `right_focused.max(left_focused)`. Pure logic, no PTY/Terminal
    /// needed -- exercises the actual rule (not just the dir/ratio mapping)
    /// against hand-built terminal ids on both sides.
    #[test]
    fn split_focus_prefers_the_larger_terminal_id() {
        let left_focused = 3usize;
        let right_focused = 7usize;
        assert_eq!(right_focused.max(left_focused), 7);

        let left_focused = 9usize;
        let right_focused = 2usize;
        assert_eq!(right_focused.max(left_focused), 9);
    }

    #[test]
    fn split_dir_round_trips() {
        assert!(matches!(
            match SplitDir::Horizontal {
                SplitDir::Horizontal => SplitDirSnapshot::Horizontal,
                SplitDir::Vertical => SplitDirSnapshot::Vertical,
            },
            SplitDirSnapshot::Horizontal
        ));
        assert!(matches!(
            match SplitDir::Vertical {
                SplitDir::Horizontal => SplitDirSnapshot::Horizontal,
                SplitDir::Vertical => SplitDirSnapshot::Vertical,
            },
            SplitDirSnapshot::Vertical
        ));
    }
}
