//! `PaneView` over the wgpu `Mux`, so the remote host can list and attach to panes.

use super::Mux;
use crate::remote::bridge::PaneView;
use crate::term::Terminal;
use remote_proto::msg::PaneInfo;

impl PaneView for Mux {
    fn panes(&self) -> Vec<PaneInfo> {
        let workspace = self
            .workspaces
            .iter()
            .find(|w| w.id == self.active_workspace_id)
            .map(|w| w.name.clone())
            .unwrap_or_default();
        self.all_terminal_ids()
            .into_iter()
            .filter_map(|id| {
                let t = self.terminals.get(id)?.as_ref()?;
                Some(PaneInfo {
                    uid: t.pane_uid,
                    title: self
                        .tab_title_of(id)
                        .map(str::to_owned)
                        .unwrap_or_else(|| format!("pane {id}")),
                    workspace: workspace.clone(),
                    cols: t.cols.get(),
                    rows: t.rows.get(),
                    state: None,
                    unread: false,
                })
            })
            .collect()
    }

    fn terminal(&self, uid: u64) -> Option<&Terminal> {
        self.terminals.iter().flatten().find(|t| t.pane_uid == uid)
    }
}
