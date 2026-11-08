//! Output-based agent state for panes whose agent reports nothing through hooks, and the
//! rule for combining it with a hook-reported state. Pure logic, no UI. See
//! `specs/004-agent-integrations/contracts/output-observer.md`.

// Used by the gpui binary only; the wgpu binary compiles this module but never calls it.
#![allow(dead_code)]

use std::time::{Duration, Instant};

use crate::term::attention::AgentState;

/// Output newer than this means the agent is working.
pub const QUIET_MS: u64 = 3000;

/// A hook-reported `Working` older than this, with a quiet terminal, is treated as idle (the
/// agent crashed, was killed, or was interrupted without a hook saying so).
pub const STALE_HOOK: Duration = Duration::from_secs(120);

/// State inferred from terminal output alone. `last_agent_output_ms` is 0 when the pane never
/// produced agent output (see `pty_activity`); `now_ms` is on the same clock.
pub fn observed_state(now_ms: u64, last_agent_output_ms: u64) -> Option<AgentState> {
    if last_agent_output_ms == 0 {
        return None;
    }
    if now_ms.saturating_sub(last_agent_output_ms) < QUIET_MS {
        Some(AgentState::Working)
    } else {
        Some(AgentState::Idle)
    }
}

/// Combine the hook-reported state (with when it was reported) and the observed one: a hook
/// state wins, except a stale `Working` while the terminal is quiet.
pub fn merge_state(
    hook: Option<(AgentState, Instant)>,
    observed: Option<AgentState>,
    now: Instant,
) -> Option<AgentState> {
    match hook {
        Some((AgentState::Working, at))
            if observed == Some(AgentState::Idle)
                && now.saturating_duration_since(at) > STALE_HOOK =>
        {
            Some(AgentState::Idle)
        }
        Some((state, _)) => Some(state),
        None => observed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn observed_state_thresholds() {
        assert_eq!(observed_state(10_000, 0), None);
        assert_eq!(observed_state(10_000, 9_999), Some(AgentState::Working));
        assert_eq!(
            observed_state(10_000, 10_000 - (QUIET_MS - 1)),
            Some(AgentState::Working)
        );
        assert_eq!(
            observed_state(10_000, 10_000 - QUIET_MS),
            Some(AgentState::Idle)
        );
        assert_eq!(observed_state(10_000, 1), Some(AgentState::Idle));
        // a stamp from the future (clock skew between threads) is still "just now"
        assert_eq!(observed_state(10_000, 10_005), Some(AgentState::Working));
    }

    #[test]
    fn a_hook_state_wins_over_the_observer() {
        let now = Instant::now();
        for hook in [
            AgentState::Idle,
            AgentState::Working,
            AgentState::NeedsInput,
            AgentState::Done,
        ] {
            for obs in [None, Some(AgentState::Working), Some(AgentState::Idle)] {
                assert_eq!(merge_state(Some((hook, now)), obs, now), Some(hook));
            }
        }
    }

    #[test]
    fn no_hook_state_falls_back_to_the_observer() {
        let now = Instant::now();
        assert_eq!(merge_state(None, None, now), None);
        assert_eq!(
            merge_state(None, Some(AgentState::Working), now),
            Some(AgentState::Working)
        );
    }

    #[test]
    fn a_stale_hook_working_with_a_quiet_terminal_becomes_idle() {
        let now = Instant::now();
        let old = now - STALE_HOOK - Duration::from_secs(1);
        let fresh = now - Duration::from_secs(5);
        let working = Some((AgentState::Working, old));
        assert_eq!(
            merge_state(working, Some(AgentState::Idle), now),
            Some(AgentState::Idle)
        );
        // still streaming, never observed, or only recently reported: stays working
        assert_eq!(
            merge_state(working, Some(AgentState::Working), now),
            Some(AgentState::Working)
        );
        assert_eq!(merge_state(working, None, now), Some(AgentState::Working));
        assert_eq!(
            merge_state(
                Some((AgentState::Working, fresh)),
                Some(AgentState::Idle),
                now
            ),
            Some(AgentState::Working)
        );
        // only Working goes stale
        assert_eq!(
            merge_state(
                Some((AgentState::NeedsInput, old)),
                Some(AgentState::Idle),
                now
            ),
            Some(AgentState::NeedsInput)
        );
    }
}
