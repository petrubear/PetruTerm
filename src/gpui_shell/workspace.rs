// The workspace data model: `Workspace` (one named group of tabs+panes, owning
// its `TabManager` + `Vec<PaneForest>` + zoom state) and `WorkspaceManager`
// (the ordered list, plus the active one). Switching just moves the active
// index.
//
// `active` is tracked by INDEX, not by id, like `TabManager`: display order
// IS index order. Every mutation below shifts `active` using the same logic
// as `TabManager::close_tab`.

use super::panes::PaneForest;
use super::tabs::TabManager;

/// One named group of tabs+panes+zoom-state.
pub struct Workspace {
    pub id: usize,
    pub name: String,
    pub tabs: TabManager,
    pub tab_panes: Vec<PaneForest>,
    /// Render-time zoom filter: workspace-scoped, since a zoomed terminal id
    /// only makes sense against ITS workspace's `tab_panes`.
    pub zoomed_pane: Option<usize>,
}

impl Workspace {
    fn new(id: usize, name: impl Into<String>) -> Self {
        Self {
            id,
            name: name.into(),
            tabs: TabManager::new(),
            tab_panes: Vec::new(),
            zoomed_pane: None,
        }
    }
}

/// Manages the ordered list of workspaces. Never empty once `new_workspace`
/// has been called at least once -- `GpuiShellRoot::new` calls it
/// immediately, same invariant `TabManager` relies on callers upholding for
/// tabs (`GpuiShellRoot::new` calls `tabs.new_tab` right after
/// `TabManager::new`).
pub struct WorkspaceManager {
    workspaces: Vec<Workspace>,
    active: usize,
    next_id: usize,
}

impl WorkspaceManager {
    pub fn new() -> Self {
        Self {
            workspaces: Vec::new(),
            active: 0,
            next_id: 0,
        }
    }

    /// Create a new, empty workspace and make it active. Returns its id.
    /// Caller is responsible for giving it an initial tab+pane, same
    /// division of labor `TabManager::new_tab`/`GpuiShellRoot::new` already
    /// have for tabs.
    pub fn new_workspace(&mut self, name: impl Into<String>) -> usize {
        let id = self.next_id;
        self.next_id += 1;
        self.workspaces.push(Workspace::new(id, name));
        self.active = self.workspaces.len() - 1;
        id
    }

    /// Close the workspace with the given id. Refuses (returns `None`) if
    /// it's the only one left -- a workspace-of-zero is not a state
    /// anything here can render. On success, returns the removed
    /// `Workspace` so the caller can walk its `tab_panes` and reap every
    /// terminal it owned (mirrors `Mux::cmd_close_workspace_id`'s own
    /// `closed_ids` collection, just handed back instead of pushed to a
    /// shared queue -- `gpui_shell` has no equivalent queue).
    ///
    /// Active-index shift on removal uses the exact fixed logic
    /// `TabManager::close_tab` documents: removing an element before
    /// `active` must shift `active` down with it, not just clamp it.
    pub fn close_workspace(&mut self, id: usize) -> Option<Workspace> {
        if self.workspaces.len() <= 1 {
            return None;
        }
        let pos = self.workspaces.iter().position(|w| w.id == id)?;
        let removed = self.workspaces.remove(pos);
        if pos < self.active {
            self.active -= 1;
        } else {
            self.active = self.active.min(self.workspaces.len() - 1);
        }
        Some(removed)
    }

    /// Switch to the workspace at the given display index. Returns whether
    /// it existed.
    pub fn switch_to_index(&mut self, idx: usize) -> bool {
        if idx < self.workspaces.len() {
            self.active = idx;
            true
        } else {
            false
        }
    }

    /// Switch to the next workspace (wraps around). No-op with one workspace.
    pub fn next_workspace(&mut self) {
        if !self.workspaces.is_empty() {
            self.active = (self.active + 1) % self.workspaces.len();
        }
    }

    /// Switch to the previous workspace (wraps around). No-op with one workspace.
    pub fn prev_workspace(&mut self) {
        if !self.workspaces.is_empty() {
            self.active = (self.active + self.workspaces.len() - 1) % self.workspaces.len();
        }
    }

    /// Rename the workspace with the given id, wherever it sits and
    /// regardless of which workspace is active -- same reasoning as
    /// `TabManager::rename_tab`: the rename editor is pinned to an id at
    /// the moment it opens, and a workspace switch mid-edit must not
    /// redirect the commit.
    pub fn rename_workspace(&mut self, id: usize, name: impl Into<String>) -> bool {
        let Some(w) = self.workspaces.iter_mut().find(|w| w.id == id) else {
            return false;
        };
        w.name = name.into();
        true
    }

    pub fn active(&self) -> &Workspace {
        &self.workspaces[self.active]
    }

    pub fn active_mut(&mut self) -> &mut Workspace {
        &mut self.workspaces[self.active]
    }

    pub fn active_id(&self) -> usize {
        self.workspaces[self.active].id
    }

    pub fn active_index(&self) -> usize {
        self.active
    }

    pub fn workspaces(&self) -> &[Workspace] {
        &self.workspaces
    }

    pub fn workspaces_mut(&mut self) -> &mut [Workspace] {
        &mut self.workspaces
    }

    pub fn workspace_mut(&mut self, idx: usize) -> Option<&mut Workspace> {
        self.workspaces.get_mut(idx)
    }

    pub fn len(&self) -> usize {
        self.workspaces.len()
    }
}

impl Default for WorkspaceManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_workspace_creates_and_activates_it() {
        let mut mgr = WorkspaceManager::new();
        let a = mgr.new_workspace("a");
        assert_eq!(mgr.active_id(), a);
        assert_eq!(mgr.active_index(), 0);
        assert_eq!(mgr.len(), 1);

        let b = mgr.new_workspace("b");
        assert_eq!(mgr.active_id(), b);
        assert_eq!(mgr.active_index(), 1);
        assert_eq!(mgr.len(), 2);
    }

    #[test]
    fn closing_a_background_workspace_before_active_keeps_the_same_workspace_active() {
        let mut mgr = WorkspaceManager::new();
        let a = mgr.new_workspace("a");
        let _b = mgr.new_workspace("b");
        let _c = mgr.new_workspace("c");
        let _d = mgr.new_workspace("d");
        // 4 workspaces: a b c d, active on d. Move active to b (index 1)
        // before closing a (index 0) -- the exact shape that silently
        // mis-clamped for TabManager before its own fix: `pos (0) < active
        // (1)`, nowhere near the last index, so a bare `.min(len-1)` clamp
        // is a no-op and would leave `active` pointing at whatever now
        // occupies index 1 (c, after removal) instead of following b to 0.
        mgr.switch_to_index(1);
        assert_eq!(mgr.active().name, "b");

        assert!(mgr.close_workspace(a).is_some());

        assert_eq!(mgr.active_index(), 0);
        assert_eq!(mgr.active().name, "b");
        assert_eq!(
            mgr.workspaces()
                .iter()
                .map(|w| w.name.as_str())
                .collect::<Vec<_>>(),
            vec!["b", "c", "d"]
        );
    }

    #[test]
    fn closing_a_background_workspace_after_active_leaves_active_index_unchanged() {
        let mut mgr = WorkspaceManager::new();
        let _a = mgr.new_workspace("a");
        let _b = mgr.new_workspace("b");
        let c = mgr.new_workspace("c");
        mgr.switch_to_index(0);
        assert_eq!(mgr.active().name, "a");

        assert!(mgr.close_workspace(c).is_some());

        assert_eq!(mgr.active_index(), 0);
        assert_eq!(mgr.active().name, "a");
    }

    #[test]
    fn closing_the_active_workspace_clamps_to_the_new_last_index() {
        let mut mgr = WorkspaceManager::new();
        let _a = mgr.new_workspace("a");
        let b = mgr.new_workspace("b");
        assert_eq!(mgr.active_index(), 1);

        let removed = mgr
            .close_workspace(b)
            .expect("two workspaces, closing one is fine");
        assert_eq!(removed.name, "b");
        assert_eq!(mgr.active_index(), 0);
        assert_eq!(mgr.active().name, "a");
    }

    #[test]
    fn closing_the_only_remaining_workspace_is_refused() {
        let mut mgr = WorkspaceManager::new();
        let a = mgr.new_workspace("a");
        assert!(mgr.close_workspace(a).is_none());
        assert_eq!(mgr.len(), 1);
    }

    #[test]
    fn closing_an_unknown_workspace_id_is_refused() {
        let mut mgr = WorkspaceManager::new();
        mgr.new_workspace("a");
        mgr.new_workspace("b");
        assert!(mgr.close_workspace(999).is_none());
        assert_eq!(mgr.len(), 2);
    }

    #[test]
    fn rename_workspace_by_id_renames_a_non_active_workspace_and_leaves_active_alone() {
        let mut mgr = WorkspaceManager::new();
        let a = mgr.new_workspace("a");
        let _b = mgr.new_workspace("b");
        assert_eq!(mgr.active().name, "b");

        assert!(mgr.rename_workspace(a, "notes"));

        assert_eq!(mgr.workspaces()[0].name, "notes");
        assert_eq!(mgr.active().name, "b");
    }

    #[test]
    fn rename_workspace_with_unknown_id_returns_false() {
        let mut mgr = WorkspaceManager::new();
        let _a = mgr.new_workspace("a");
        assert!(!mgr.rename_workspace(999, "notes"));
    }

    #[test]
    fn next_and_prev_workspace_wrap_around() {
        let mut mgr = WorkspaceManager::new();
        mgr.new_workspace("a");
        mgr.new_workspace("b");
        mgr.new_workspace("c");
        mgr.switch_to_index(2); // "c"

        mgr.next_workspace();
        assert_eq!(mgr.active_index(), 0); // wrapped to "a"

        mgr.prev_workspace();
        assert_eq!(mgr.active_index(), 2); // wrapped back to "c"
    }

    #[test]
    fn switch_to_index_out_of_bounds_is_refused_and_leaves_active_unchanged() {
        let mut mgr = WorkspaceManager::new();
        mgr.new_workspace("a");
        assert!(!mgr.switch_to_index(5));
        assert_eq!(mgr.active_index(), 0);
    }

    #[test]
    fn workspace_mut_returns_none_out_of_bounds() {
        let mut mgr = WorkspaceManager::new();
        mgr.new_workspace("a");
        assert!(mgr.workspace_mut(5).is_none());
        assert!(mgr.workspace_mut(0).is_some());
    }
}
