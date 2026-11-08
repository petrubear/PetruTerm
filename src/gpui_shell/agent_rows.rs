// Sidebar "Agents" section data: which panes to scan and the rows built from the latest
// scan plus the attention tracker. See `agent_scan.rs` and `specs/001-agents-sidebar-tab/`.

use super::GpuiShellRoot;
use crate::term::agents::{self, AgentRow, PaneProbe};
use crate::term::attention::AgentState;

impl GpuiShellRoot {
    /// One probe per live terminal: its id and the pid of its shell.
    pub(super) fn agent_probes(&self) -> Vec<PaneProbe> {
        self.terminals
            .iter()
            .map(|(&terminal_id, t)| PaneProbe {
                terminal_id,
                shell_pid: t.child_pid,
            })
            .collect()
    }

    /// Rows for the Agents section, most urgent first. Detections whose pane no longer
    /// exists are dropped; panes the scan did not detect never appear.
    pub(super) fn agent_rows(&self) -> Vec<AgentRow> {
        let mut rows = Vec::new();
        for det in self.agent_scan.detections() {
            let found = self.workspaces.workspaces().iter().find_map(|ws| {
                ws.tab_panes.iter().enumerate().find_map(|(ti, forest)| {
                    let leaves = forest.root.leaf_ids();
                    let pos = leaves.iter().position(|&id| id == det.terminal_id)?;
                    let title = ws.tabs.tabs().get(ti).map(|t| t.title.clone());
                    Some((
                        ws.name.clone(),
                        title.unwrap_or_default(),
                        pos + 1,
                        leaves.len(),
                    ))
                })
            });
            let Some((workspace_name, tab_title, pane, pane_count)) = found else {
                continue;
            };
            let tracked = self.attention.get(det.terminal_id);
            rows.push(AgentRow {
                terminal_id: det.terminal_id,
                agent: det.agent.clone(),
                color: det.color,
                agent_pid: det.pid,
                accepting: self.agent_accept.is_accepting(det.terminal_id, det.pid),
                msg_recent: self
                    .msg
                    .delivered_recently(det.terminal_id, std::time::Instant::now()),
                custom_name: self.agent_names.name_of(det.terminal_id).map(str::to_owned),
                session: self
                    .agent_sessions
                    .get(det.terminal_id)
                    .map(str::to_owned)
                    .or_else(|| det.session.clone()),
                workspace_name,
                tab_title,
                pane_number: (pane_count > 1).then_some(pane),
                state: tracked.map(|p| p.state),
                unread: tracked.is_some_and(|p| p.unread) && self.config.agents.ring,
                message: tracked.and_then(|p| p.last_message.clone()),
                activity: tracked
                    .filter(|p| p.state == AgentState::Working)
                    .and_then(|p| p.activity.as_deref())
                    .and_then(|a| agents::apply_detail(self.config.agents.activity, a)),
                updated_at: tracked.and_then(|p| p.updated_at),
            });
        }
        agents::sort_rows(&mut rows);
        rows
    }
}
