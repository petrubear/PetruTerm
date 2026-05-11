use super::{Mux, Workspace, WorkspaceData};

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
