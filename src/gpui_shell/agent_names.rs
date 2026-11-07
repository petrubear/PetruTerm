// Custom names for running agents, set from the Agents list's right-click menu. A name belongs
// to one running agent in one pane: it is dropped when the process goes away or another one
// takes the pane, and is never written to disk. `terminal_for` / `name_of` are the lookups the
// later inter-agent messaging feature will use to address agents. See
// `specs/001-agents-sidebar-tab/research.md` R13.

use std::collections::HashMap;

use crate::term::agents::{validate_name, AgentDetection, NameError};

#[derive(Default)]
pub struct AgentNames {
    /// terminal id -> (agent process pid, name)
    names: HashMap<usize, (u32, String)>,
}

impl AgentNames {
    /// Set or replace the name of the agent `pid` running in `terminal_id`. `locate` describes
    /// where another terminal's agent lives, for the duplicate-name message.
    pub fn set(
        &mut self,
        terminal_id: usize,
        pid: u32,
        input: &str,
        locate: impl Fn(usize) -> String,
    ) -> Result<(), NameError> {
        let name = validate_name(input)?;
        if let Some((other, _)) = self
            .names
            .iter()
            .find(|(t, (_, n))| **t != terminal_id && n.eq_ignore_ascii_case(&name))
        {
            return Err(NameError::Duplicate(locate(*other)));
        }
        self.names.insert(terminal_id, (pid, name));
        Ok(())
    }

    pub fn reset(&mut self, terminal_id: usize) {
        self.names.remove(&terminal_id);
    }

    pub fn name_of(&self, terminal_id: usize) -> Option<&str> {
        self.names.get(&terminal_id).map(|(_, n)| n.as_str())
    }

    /// The terminal whose agent has this name (case-insensitive).
    #[allow(dead_code)] // consumed by the later inter-agent messaging feature
    pub fn terminal_for(&self, name: &str) -> Option<usize> {
        self.names
            .iter()
            .find(|(_, (_, n))| n.eq_ignore_ascii_case(name))
            .map(|(t, _)| *t)
    }

    /// Drop names whose agent process is gone or was replaced by a different one.
    pub fn retain_live(&mut self, detections: &[AgentDetection]) {
        self.names.retain(|t, (pid, _)| {
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

    fn here(t: usize) -> String {
        format!("pane {t}")
    }

    #[test]
    fn set_get_reset() {
        let mut n = AgentNames::default();
        n.set(1, 10, " reviewer ", here).unwrap();
        assert_eq!(n.name_of(1), Some("reviewer"));
        n.set(1, 10, "builder", here).unwrap();
        assert_eq!(n.name_of(1), Some("builder"));
        n.reset(1);
        assert_eq!(n.name_of(1), None);
    }

    #[test]
    fn duplicates_are_refused_case_insensitively_but_own_name_can_be_resaved() {
        let mut n = AgentNames::default();
        n.set(1, 10, "reviewer", here).unwrap();
        assert_eq!(
            n.set(2, 20, "Reviewer", here),
            Err(NameError::Duplicate("pane 1".into()))
        );
        assert_eq!(n.name_of(2), None);
        n.set(1, 10, "REVIEWER", here).unwrap();
        assert_eq!(n.name_of(1), Some("REVIEWER"));
    }

    #[test]
    fn invalid_names_are_refused() {
        let mut n = AgentNames::default();
        assert_eq!(n.set(1, 10, "", here), Err(NameError::Empty));
        assert_eq!(n.set(1, 10, "a b", here), Err(NameError::BadChar(' ')));
        assert_eq!(n.name_of(1), None);
    }

    #[test]
    fn lookups_work_both_ways() {
        let mut n = AgentNames::default();
        n.set(3, 30, "backend-1", here).unwrap();
        assert_eq!(n.terminal_for("BACKEND-1"), Some(3));
        assert_eq!(n.terminal_for("nope"), None);
        assert_eq!(n.name_of(3), Some("backend-1"));
    }

    #[test]
    fn name_is_dropped_when_the_process_goes_or_is_replaced() {
        let mut n = AgentNames::default();
        n.set(1, 10, "a", here).unwrap();
        n.set(2, 20, "b", here).unwrap();
        n.set(3, 30, "c", here).unwrap();
        n.retain_live(&[det(1, 10), det(2, 99)]);
        assert_eq!(n.name_of(1), Some("a"));
        assert_eq!(n.name_of(2), None);
        assert_eq!(n.name_of(3), None);
    }
}
