use super::{snapshot, Mux, Workspace, WorkspaceData};

impl Mux {
    pub fn workspaces(&self) -> &[Workspace] {
        &self.workspaces
    }

    pub fn workspace_count(&self) -> usize {
        self.workspaces.len()
    }

    /// Return `(tab_count, pane_count)` for each workspace in display order.
    pub fn workspace_tab_pane_counts(&self) -> Vec<(usize, usize)> {
        self.workspaces
            .iter()
            .map(|w| {
                if w.id == self.active_workspace_id {
                    let tabs = self.tabs.tab_count();
                    let panes = self
                        .panes
                        .iter()
                        .map(|pm| pm.root.leaf_count())
                        .sum::<usize>();
                    (tabs, panes)
                } else if let Some(data) = self.inactive_workspaces.iter().find(|d| d.id == w.id) {
                    let tabs = data.tabs.tab_count();
                    let panes = data
                        .panes
                        .iter()
                        .map(|pm| pm.root.leaf_count())
                        .sum::<usize>();
                    (tabs, panes)
                } else {
                    (0, 0)
                }
            })
            .collect()
    }

    /// Index of the active workspace in `self.workspaces`.
    pub fn active_workspace_index(&self) -> usize {
        self.workspaces
            .iter()
            .position(|w| w.id == self.active_workspace_id)
            .unwrap_or(0)
    }

    /// Archive the active workspace and activate a new empty one.
    /// Caller must open an initial tab after calling this.
    pub fn cmd_new_workspace(&mut self, name: String) {
        let id = self.next_workspace_id;
        self.next_workspace_id += 1;
        self.workspaces.push(Workspace { id, name });
        self.inactive_workspaces.push(WorkspaceData {
            id: self.active_workspace_id,
            tabs: std::mem::take(&mut self.tabs),
            panes: std::mem::take(&mut self.panes),
        });
        self.active_workspace_id = id;
    }

    /// Close the active workspace. No-op if only one workspace remains.
    /// Caller must drain `closed_ids` to clean up external state.
    pub fn cmd_close_workspace(&mut self) {
        if self.workspaces.len() <= 1 {
            return;
        }
        let all_tids: Vec<usize> = self
            .panes
            .iter()
            .flat_map(|pm| pm.root.leaf_ids())
            .collect();
        for tid in all_tids {
            if let Some(slot) = self.terminals.get_mut(tid) {
                *slot = None;
            }
            self.closed_ids.push(tid);
        }
        let pos = self.active_workspace_index();
        self.workspaces.remove(pos);
        let target_pos = pos.min(self.workspaces.len() - 1);
        let target_id = self.workspaces[target_pos].id;
        if let Some(idx) = self
            .inactive_workspaces
            .iter()
            .position(|d| d.id == target_id)
        {
            let data = self.inactive_workspaces.remove(idx);
            self.tabs = data.tabs;
            self.panes = data.panes;
        }
        self.active_workspace_id = target_id;
    }

    pub fn cmd_rename_workspace(&mut self, name: String) {
        if let Some(w) = self
            .workspaces
            .iter_mut()
            .find(|w| w.id == self.active_workspace_id)
        {
            w.name = name;
        }
    }

    pub fn cmd_rename_workspace_id(&mut self, id: usize, name: String) {
        if let Some(w) = self.workspaces.iter_mut().find(|w| w.id == id) {
            w.name = name;
        }
    }

    pub fn cmd_close_workspace_id(&mut self, id: usize) {
        if self.workspaces.len() <= 1 {
            return;
        }
        if id == self.active_workspace_id {
            self.cmd_close_workspace();
            return;
        }
        if let Some(pos) = self.workspaces.iter().position(|w| w.id == id) {
            self.workspaces.remove(pos);
        }
        if let Some(snapshot_idx) = self.inactive_workspaces.iter().position(|d| d.id == id) {
            let data = self.inactive_workspaces.remove(snapshot_idx);
            let all_tids: Vec<usize> = data
                .panes
                .iter()
                .flat_map(|pm| pm.root.leaf_ids())
                .collect();
            for tid in all_tids {
                if let Some(slot) = self.terminals.get_mut(tid) {
                    *slot = None;
                }
                self.closed_ids.push(tid);
            }
        }
    }

    pub fn cmd_switch_workspace(&mut self, id: usize) {
        if id == self.active_workspace_id {
            return;
        }
        self.inactive_workspaces.push(WorkspaceData {
            id: self.active_workspace_id,
            tabs: std::mem::take(&mut self.tabs),
            panes: std::mem::take(&mut self.panes),
        });
        if let Some(idx) = self.inactive_workspaces.iter().position(|d| d.id == id) {
            let data = self.inactive_workspaces.remove(idx);
            self.tabs = data.tabs;
            self.panes = data.panes;
            self.active_workspace_id = id;
        } else {
            // Target not found — undo the archive (shouldn't happen in practice).
            let data = self.inactive_workspaces.pop().unwrap();
            self.tabs = data.tabs;
            self.panes = data.panes;
        }
    }

    pub fn cmd_next_workspace(&mut self) {
        let pos = self.active_workspace_index();
        let id = self.workspaces[(pos + 1) % self.workspaces.len()].id;
        self.cmd_switch_workspace(id);
    }

    pub fn cmd_prev_workspace(&mut self) {
        let pos = self.active_workspace_index();
        let len = self.workspaces.len();
        let id = self.workspaces[if pos == 0 { len - 1 } else { pos - 1 }].id;
        self.cmd_switch_workspace(id);
    }
}

// ── Workspace persistence ─────────────────────────────────────────────────────

impl Mux {
    /// Snapshot the active workspace to disk.
    pub fn save_workspace(&self) -> anyhow::Result<()> {
        let snap = self.build_workspace_snapshot(self.active_workspace_id, &self.tabs, &self.panes);
        snapshot::save_snapshot(&snap)
    }

    /// Snapshot all workspaces (active + inactive) to disk.
    pub fn save_all_workspaces(&self) {
        if let Err(e) = self.save_workspace() {
            log::error!("Failed to save workspace: {e}");
        }
        for data in &self.inactive_workspaces {
            let snap = self.build_workspace_snapshot(data.id, &data.tabs, &data.panes);
            if let Err(e) = snapshot::save_snapshot(&snap) {
                log::error!("Failed to save workspace '{}': {e}", snap.name);
            }
        }
    }

    fn build_workspace_snapshot(
        &self,
        workspace_id: usize,
        tabs: &crate::ui::TabManager,
        panes: &[crate::ui::PaneManager],
    ) -> snapshot::WorkspaceSnapshot {
        use snapshot::{PaneNodeSnapshot, TabSnapshot, WorkspaceSnapshot};
        let name = self
            .workspaces
            .iter()
            .find(|w| w.id == workspace_id)
            .map(|w| w.name.clone())
            .unwrap_or_else(|| "workspace".to_string());
        let tab_snapshots: Vec<TabSnapshot> = tabs
            .tabs()
            .iter()
            .enumerate()
            .map(|(i, tab)| {
                let pane_tree = panes
                    .get(i)
                    .map(|pm| self.snapshot_pane_node(&pm.root))
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
            name,
            saved_at,
            tabs: tab_snapshots,
        }
    }

    fn snapshot_pane_node(&self, node: &crate::ui::panes::PaneNode) -> snapshot::PaneNodeSnapshot {
        use crate::ui::panes::PaneNode;
        use crate::ui::SplitDir;
        use snapshot::{PaneNodeSnapshot, SplitDirSnapshot};
        match node {
            PaneNode::Leaf { terminal_id, .. } => {
                let cwd = self
                    .terminals
                    .get(*terminal_id)
                    .and_then(|t| t.as_ref())
                    .and_then(|t| crate::term::process_cwd(t.child_pid))
                    .map(|p| p.to_string_lossy().into_owned())
                    .unwrap_or_else(home_str);
                PaneNodeSnapshot::Leaf { cwd }
            }
            PaneNode::Split {
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

    /// Restore a workspace from a snapshot: creates a new workspace with the saved layout.
    /// Tabs and panes are created fresh (no process state is restored).
    #[allow(clippy::too_many_arguments)]
    pub fn restore_workspace(
        &mut self,
        snap: snapshot::WorkspaceSnapshot,
        config: &crate::config::Config,
        viewport: crate::ui::Rect,
        cols: u16,
        rows: u16,
        cell_w: u16,
        cell_h: u16,
        proxy: winit::event_loop::EventLoopProxy<()>,
    ) {
        self.cmd_new_workspace(snap.name.clone());

        for tab_snap in &snap.tabs {
            let result = restore_pane_recursive(
                self,
                &tab_snap.pane_tree,
                viewport,
                config,
                cols,
                rows,
                cell_w,
                cell_h,
                proxy.clone(),
            );
            match result {
                Ok((root, focused)) => {
                    self.tabs.new_tab(&tab_snap.title);
                    self.tabs.rename_active(&tab_snap.title);
                    let idx = self.tabs.tab_count().saturating_sub(1);
                    if let Some(color) = tab_snap.accent_color {
                        self.tabs.set_tab_color(idx, Some(color));
                    }
                    let mut pm = crate::ui::PaneManager::new(viewport, focused);
                    pm.root = root;
                    pm.root.layout(viewport);
                    self.panes.push(pm);
                    self.pending_lua_events.push("tab_created");
                }
                Err(e) => log::error!("Failed to restore tab '{}': {e}", tab_snap.title),
            }
        }
    }
}

fn home_str() -> String {
    dirs::home_dir()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|| "/".to_string())
}

fn restore_pane_recursive(
    mux: &mut Mux,
    node: &snapshot::PaneNodeSnapshot,
    viewport: crate::ui::Rect,
    config: &crate::config::Config,
    cols: u16,
    rows: u16,
    cell_w: u16,
    cell_h: u16,
    proxy: winit::event_loop::EventLoopProxy<()>,
) -> anyhow::Result<(crate::ui::panes::PaneNode, usize)> {
    use crate::ui::panes::{next_node_id, PaneNode};
    use crate::ui::SplitDir;
    use snapshot::{PaneNodeSnapshot, SplitDirSnapshot};
    match node {
        PaneNodeSnapshot::Leaf { cwd } => {
            let path = std::path::PathBuf::from(cwd);
            let cwd_opt = if path.exists() {
                Some(path)
            } else {
                dirs::home_dir()
            };
            let tid = mux.open_terminal(config, cols, rows, cell_w, cell_h, proxy, cwd_opt)?;
            Ok((
                PaneNode::Leaf {
                    terminal_id: tid,
                    rect: viewport,
                },
                tid,
            ))
        }
        PaneNodeSnapshot::Split {
            dir,
            ratio,
            left,
            right,
        } => {
            let (ln, lf) = restore_pane_recursive(
                mux,
                left,
                viewport,
                config,
                cols,
                rows,
                cell_w,
                cell_h,
                proxy.clone(),
            )?;
            let (rn, _) = restore_pane_recursive(
                mux, right, viewport, config, cols, rows, cell_w, cell_h, proxy,
            )?;
            let split_dir = match dir {
                SplitDirSnapshot::Horizontal => SplitDir::Horizontal,
                SplitDirSnapshot::Vertical => SplitDir::Vertical,
            };
            Ok((
                PaneNode::Split {
                    node_id: next_node_id(),
                    dir: split_dir,
                    ratio: *ratio,
                    left: Box::new(ln),
                    right: Box::new(rn),
                    rect: viewport,
                },
                lf,
            ))
        }
    }
}
