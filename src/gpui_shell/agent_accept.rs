// Which agents accept messages from other agents. Off by default, per agent process, in memory
// only: an entry goes away when the process exits or another one takes the pane, and nothing is
// ever written to disk. See `specs/002-agent-messaging/` (US3).

use std::collections::HashMap;

use crate::term::agents::AgentDetection;

#[derive(Default)]
pub struct AgentAccept {
    /// terminal id -> pid of the agent process that opted in
    on: HashMap<usize, u32>,
}

impl AgentAccept {
    /// Is the agent `pid` in `terminal_id` accepting messages?
    pub fn is_accepting(&self, terminal_id: usize, pid: u32) -> bool {
        self.on.get(&terminal_id) == Some(&pid)
    }

    pub fn set(&mut self, terminal_id: usize, pid: u32, accepting: bool) {
        if accepting {
            self.on.insert(terminal_id, pid);
        } else {
            self.on.remove(&terminal_id);
        }
    }

    pub fn any(&self) -> bool {
        !self.on.is_empty()
    }

    /// Drop entries whose agent process is gone or was replaced.
    pub fn retain_live(&mut self, detections: &[AgentDetection]) {
        self.on.retain(|t, pid| {
            detections
                .iter()
                .any(|d| d.terminal_id == *t && d.pid == *pid)
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn det(terminal_id: usize, pid: u32) -> AgentDetection {
        AgentDetection {
            terminal_id,
            agent: "Kiro".into(),
            color: None,
            pid,
            session: None,
        }
    }

    #[test]
    fn off_by_default_and_toggles_per_process() {
        let mut a = AgentAccept::default();
        assert!(!a.is_accepting(1, 10));
        a.set(1, 10, true);
        assert!(a.is_accepting(1, 10));
        assert!(
            !a.is_accepting(1, 11),
            "another process in the pane is not accepting"
        );
        assert!(a.any());
        a.set(1, 10, false);
        assert!(!a.is_accepting(1, 10) && !a.any());
    }

    #[test]
    fn replaced_or_gone_process_clears_acceptance() {
        let mut a = AgentAccept::default();
        a.set(1, 10, true);
        a.set(2, 20, true);
        a.set(3, 30, true);
        a.retain_live(&[det(1, 10), det(2, 99)]);
        assert!(a.is_accepting(1, 10));
        assert!(!a.is_accepting(2, 20) && !a.is_accepting(2, 99));
        assert!(!a.is_accepting(3, 30));
    }
}
