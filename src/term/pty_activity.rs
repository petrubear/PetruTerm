//! When a pane's PTY last received output that was not the echo of the user's own typing.
//! Stamped by the PTY reader thread and `Pty::write` with two relaxed atomic stores (no locks,
//! no allocation); read by the Agents list to infer working/idle for agents without hooks. See
//! `specs/004-agent-integrations/contracts/output-observer.md`.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;
use std::time::Instant;

/// Output arriving within this long after a write to the PTY is treated as echo.
pub const ECHO_WINDOW_MS: u64 = 300;

/// Milliseconds since the first call in this process (never 0, so 0 can mean "never").
pub fn now_ms() -> u64 {
    static EPOCH: OnceLock<Instant> = OnceLock::new();
    EPOCH.get_or_init(Instant::now).elapsed().as_millis() as u64 + 1
}

#[derive(Debug, Default)]
pub struct PtyActivity {
    last_input_ms: AtomicU64,
    last_agent_output_ms: AtomicU64,
}

impl PtyActivity {
    /// Bytes were written to the PTY (keyboard input, pasted text, a delivered message).
    pub fn on_input(&self) {
        self.on_input_at(now_ms());
    }

    /// Bytes arrived from the PTY.
    pub fn on_output(&self) {
        self.on_output_at(now_ms());
    }

    pub fn on_input_at(&self, now: u64) {
        self.last_input_ms.store(now, Ordering::Relaxed);
    }

    /// Counts as agent output unless it falls inside the echo window of the last input.
    pub fn on_output_at(&self, now: u64) {
        let input = self.last_input_ms.load(Ordering::Relaxed);
        if input == 0 || now.saturating_sub(input) > ECHO_WINDOW_MS {
            self.last_agent_output_ms.store(now, Ordering::Relaxed);
        }
    }

    /// 0 when the pane never produced agent output.
    #[allow(dead_code)] // read by the gpui Agents list only
    pub fn last_agent_output_ms(&self) -> u64 {
        self.last_agent_output_ms.load(Ordering::Relaxed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn never_active_is_zero() {
        assert_eq!(PtyActivity::default().last_agent_output_ms(), 0);
        assert!(now_ms() > 0);
    }

    #[test]
    fn output_without_prior_input_counts() {
        let a = PtyActivity::default();
        a.on_output_at(1000);
        assert_eq!(a.last_agent_output_ms(), 1000);
    }

    #[test]
    fn echo_within_the_window_does_not_count_but_later_output_does() {
        let a = PtyActivity::default();
        a.on_input_at(1000);
        a.on_output_at(1000 + ECHO_WINDOW_MS);
        assert_eq!(
            a.last_agent_output_ms(),
            0,
            "the boundary still counts as echo"
        );
        a.on_output_at(1000 + ECHO_WINDOW_MS + 1);
        assert_eq!(a.last_agent_output_ms(), 1000 + ECHO_WINDOW_MS + 1);
    }

    #[test]
    fn typing_during_a_stream_keeps_the_last_real_output() {
        let a = PtyActivity::default();
        a.on_output_at(5000);
        a.on_input_at(5100);
        a.on_output_at(5150); // echo of the keystroke
        assert_eq!(a.last_agent_output_ms(), 5000);
        a.on_output_at(5600); // the stream continues
        assert_eq!(a.last_agent_output_ms(), 5600);
    }
}
