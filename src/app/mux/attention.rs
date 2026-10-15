//! Mux helpers for the agent attention layer: enumerate panes across tabs and workspaces,
//! and bring a given terminal into view. Attention state itself lives in
//! `term::attention::AttentionTracker`, keyed by terminal id.

use super::Mux;

impl Mux {
    /// Terminal ids of every pane in tab `tab_idx` of the active workspace.
    pub fn tab_terminal_ids(&self, tab_idx: usize) -> Vec<usize> {
        self.panes
            .get(tab_idx)
            .map(|pm| pm.root.leaf_ids())
            .unwrap_or_default()
    }

    /// Terminal ids of every pane in every workspace, in a stable order: the active
    /// workspace first (tab order, then pane order), then archived workspaces.
    pub fn all_terminal_ids(&self) -> Vec<usize> {
        let mut ids: Vec<usize> = self
            .panes
            .iter()
            .flat_map(|pm| pm.root.leaf_ids())
            .collect();
        for data in &self.inactive_workspaces {
            ids.extend(data.panes.iter().flat_map(|pm| pm.root.leaf_ids()));
        }
        ids
    }

    /// Move messages received on the control socket into `attention_events`, resolving each
    /// pane uid (`PETRUTERM_PANE_ID`) to a terminal id. Unknown panes are dropped.
    pub fn collect_ipc_events(&mut self) {
        for msg in crate::ipc::server::drain() {
            let Some((uid, event)) = msg.into_event() else {
                continue;
            };
            let id = self
                .terminals
                .iter()
                .position(|t| t.as_ref().is_some_and(|t| t.pane_uid == uid));
            if let Some(id) = id {
                self.attention_events.push((id, event));
            }
        }
    }

    /// Title of the tab (in the active workspace) that holds `terminal_id`.
    pub fn tab_title_of(&self, terminal_id: usize) -> Option<&str> {
        let idx = self
            .panes
            .iter()
            .position(|pm| pm.root.leaf_ids().contains(&terminal_id))?;
        self.tabs.tabs().get(idx).map(|t| t.title.as_str())
    }

    /// Bring `terminal_id` into view: switch workspace and tab, focus the pane, and unzoom if
    /// a different pane is zoomed. Returns false if no pane owns that terminal.
    pub fn focus_terminal(&mut self, terminal_id: usize) -> bool {
        if let Some(tab_idx) = self
            .panes
            .iter()
            .position(|pm| pm.root.leaf_ids().contains(&terminal_id))
        {
            self.tabs.switch_to_index(tab_idx);
            self.panes[tab_idx].focused_terminal = terminal_id;
            if self.zoomed_pane.is_some_and(|z| z != terminal_id) {
                self.zoomed_pane = None;
            }
            return true;
        }
        let owner = self
            .inactive_workspaces
            .iter()
            .find(|d| {
                d.panes
                    .iter()
                    .any(|pm| pm.root.leaf_ids().contains(&terminal_id))
            })
            .map(|d| d.id);
        match owner {
            Some(ws_id) => {
                self.cmd_switch_workspace(ws_id);
                // Now active; the first branch will find it. No loop: a miss returns false.
                self.panes
                    .iter()
                    .any(|pm| pm.root.leaf_ids().contains(&terminal_id))
                    && self.focus_terminal(terminal_id)
            }
            None => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::{PaneManager, Rect};

    fn viewport() -> Rect {
        Rect {
            x: 0.0,
            y: 0.0,
            w: 800.0,
            h: 600.0,
        }
    }

    /// Two tabs: tab 0 has terminals [0, 1] (split), tab 1 has terminal [2].
    fn mux_with_tabs() -> Mux {
        let mut mux = Mux::new();
        mux.tabs.new_tab("a");
        let mut pm = PaneManager::new(viewport(), 0);
        pm.split(crate::ui::panes::SplitDir::Horizontal, 1);
        mux.panes.push(pm);
        mux.tabs.new_tab("b");
        mux.panes.push(PaneManager::new(viewport(), 2));
        mux
    }

    #[test]
    fn lists_terminal_ids_per_tab_and_overall() {
        let mux = mux_with_tabs();
        assert_eq!(mux.tab_terminal_ids(0), vec![0, 1]);
        assert_eq!(mux.tab_terminal_ids(1), vec![2]);
        assert!(mux.tab_terminal_ids(9).is_empty());
        assert_eq!(mux.all_terminal_ids(), vec![0, 1, 2]);
    }

    #[test]
    fn focus_terminal_switches_tab_and_pane() {
        let mut mux = mux_with_tabs();
        assert_eq!(mux.active_tab_index(), 1);
        assert!(mux.focus_terminal(1));
        assert_eq!(mux.active_tab_index(), 0);
        assert_eq!(mux.focused_terminal_id(), 1);
        assert!(!mux.focus_terminal(99));
    }

    #[test]
    fn focus_terminal_clears_zoom_on_other_pane() {
        let mut mux = mux_with_tabs();
        mux.zoomed_pane = Some(2);
        assert!(mux.focus_terminal(0));
        assert_eq!(mux.zoomed_pane, None);
    }

    #[test]
    fn focus_terminal_switches_workspace() {
        let mut mux = mux_with_tabs();
        mux.cmd_new_workspace("two".to_string());
        // The first workspace (with terminals 0..=2) is now archived.
        assert!(mux.all_terminal_ids().contains(&2));
        let before = mux.active_workspace_id;
        assert!(mux.focus_terminal(2));
        assert_ne!(mux.active_workspace_id, before);
        assert_eq!(mux.focused_terminal_id(), 2);
    }
}
