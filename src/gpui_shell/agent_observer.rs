// Remembers the output-based state of each detected agent pane so the poll tick can tell when
// the Agents list needs a repaint (terminal output does not otherwise invalidate it). The
// rows themselves recompute the state from the live atomics; this only detects changes. See
// `specs/004-agent-integrations/contracts/output-observer.md`.

use std::collections::HashMap;

use crate::term::agents::observed_state;
use crate::term::attention::AgentState;

#[derive(Default)]
pub struct AgentObserver {
    last: HashMap<usize, Option<AgentState>>,
}

impl AgentObserver {
    /// `terminals` yields (terminal id, last agent output stamp) for the detected agent panes.
    /// Returns true when any pane's observed state changed since the previous poll.
    pub fn poll(&mut self, terminals: impl Iterator<Item = (usize, u64)>, now_ms: u64) -> bool {
        let mut changed = false;
        let mut next = HashMap::new();
        for (id, last_output) in terminals {
            let state = observed_state(now_ms, last_output);
            changed |= self.last.get(&id) != Some(&state);
            next.insert(id, state);
        }
        self.last = next;
        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_a_change_only_when_a_state_flips() {
        let mut o = AgentObserver::default();
        // first sight of a pane counts as a change
        assert!(o.poll([(1, 9_000)].into_iter(), 10_000));
        // still working a moment later: no change
        assert!(!o.poll([(1, 9_000)].into_iter(), 10_500));
        // quiet for 3 s: flips to idle
        assert!(o.poll([(1, 9_000)].into_iter(), 12_100));
        assert!(!o.poll([(1, 9_000)].into_iter(), 20_000));
        // new output: back to working
        assert!(o.poll([(1, 19_900)].into_iter(), 20_000));
    }

    #[test]
    fn a_gone_pane_is_forgotten_and_a_return_counts_as_new() {
        let mut o = AgentObserver::default();
        o.poll([(1, 100), (2, 100)].into_iter(), 100_000);
        assert!(
            !o.poll([(2, 100)].into_iter(), 100_000),
            "losing a pane changes nothing visible"
        );
        assert!(o.poll([(1, 100), (2, 100)].into_iter(), 100_000));
    }

    #[test]
    fn panes_that_never_produced_output_are_stable() {
        let mut o = AgentObserver::default();
        assert!(o.poll([(3, 0)].into_iter(), 1_000));
        assert!(!o.poll([(3, 0)].into_iter(), 9_000));
    }
}
