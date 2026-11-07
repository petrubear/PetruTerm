// Session ids announced by agent hooks (e.g. Claude Code's `session_id`), per terminal. An id is
// tied to the agent process once a scan sees it, and dropped when that process goes away or
// another one takes the pane. See `specs/001-agents-sidebar-tab/research.md` R12.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use crate::term::agents::AgentDetection;

use super::GpuiShellRoot;

/// How long a reported id may wait for a scan to see its process before it is dropped.
const UNBOUND_GRACE: Duration = Duration::from_secs(5);

struct Reported {
    id: String,
    pid: Option<u32>,
    noted_at: Instant,
}

#[derive(Default)]
pub struct AgentSessions {
    reported: HashMap<usize, Reported>,
}

impl AgentSessions {
    /// Record the latest id for `terminal_id`. Returns true when it changed.
    pub fn note(&mut self, terminal_id: usize, id: String, now: Instant) -> bool {
        let changed = self.reported.get(&terminal_id).map(|r| &r.id) != Some(&id);
        self.reported.insert(
            terminal_id,
            Reported {
                id,
                pid: None,
                noted_at: now,
            },
        );
        changed
    }

    pub fn get(&self, terminal_id: usize) -> Option<&str> {
        self.reported.get(&terminal_id).map(|r| r.id.as_str())
    }

    /// Reconcile with a fresh scan: bind ids to the detected process, drop ids whose process
    /// is gone or was replaced, and drop unbound ids no process claimed within the grace time.
    pub fn retain_live(&mut self, detections: &[AgentDetection], now: Instant) {
        self.reported.retain(|terminal_id, r| {
            let det = detections.iter().find(|d| d.terminal_id == *terminal_id);
            match (r.pid, det) {
                (Some(pid), Some(d)) => pid == d.pid,
                (None, Some(d)) => {
                    r.pid = Some(d.pid);
                    true
                }
                (Some(_), None) => false,
                (None, None) => now.duration_since(r.noted_at) < UNBOUND_GRACE,
            }
        });
    }
}

impl GpuiShellRoot {
    /// Hook announced `id` for the pane with `pane_uid`. True when the shown id may change.
    pub(super) fn note_agent_session(&mut self, pane_uid: u64, id: String) -> bool {
        let Some((&terminal_id, _)) = self.terminals.iter().find(|(_, t)| t.pane_uid == pane_uid)
        else {
            return false;
        };
        self.agent_sessions.note(terminal_id, id, Instant::now())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn det(terminal_id: usize, pid: u32) -> AgentDetection {
        AgentDetection {
            terminal_id,
            agent: "Claude Code".into(),
            color: None,
            pid,
            session: None,
        }
    }

    #[test]
    fn note_replaces_and_reports_change() {
        let mut s = AgentSessions::default();
        let t = Instant::now();
        assert!(s.note(1, "aaa".into(), t));
        assert!(!s.note(1, "aaa".into(), t));
        assert!(s.note(1, "bbb".into(), t));
        assert_eq!(s.get(1), Some("bbb"));
        assert_eq!(s.get(2), None);
    }

    #[test]
    fn binds_to_the_process_and_drops_when_it_changes_or_goes() {
        let mut s = AgentSessions::default();
        let t = Instant::now();
        s.note(1, "aaa".into(), t);
        s.retain_live(&[det(1, 100)], t);
        assert_eq!(s.get(1), Some("aaa"));
        s.retain_live(&[det(1, 100)], t);
        assert_eq!(s.get(1), Some("aaa"));
        // a different agent process took the pane
        s.retain_live(&[det(1, 200)], t);
        assert_eq!(s.get(1), None);
    }

    #[test]
    fn process_gone_drops_bound_id() {
        let mut s = AgentSessions::default();
        let t = Instant::now();
        s.note(1, "aaa".into(), t);
        s.retain_live(&[det(1, 100)], t);
        s.retain_live(&[], t);
        assert_eq!(s.get(1), None);
    }

    #[test]
    fn unbound_id_survives_a_scan_that_predates_the_process_until_the_grace_ends() {
        let mut s = AgentSessions::default();
        let t = Instant::now();
        s.note(1, "aaa".into(), t);
        s.retain_live(&[], t + Duration::from_secs(1));
        assert_eq!(s.get(1), Some("aaa"));
        s.retain_live(&[], t + Duration::from_secs(6));
        assert_eq!(s.get(1), None);
    }
}
