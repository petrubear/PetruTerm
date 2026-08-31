use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

#[derive(Clone, Debug)]
pub(crate) struct WakeupGate {
    pending: Arc<AtomicBool>,
}

impl WakeupGate {
    pub(crate) fn new() -> Self {
        Self {
            pending: Arc::new(AtomicBool::new(false)),
        }
    }

    pub(crate) fn signal(&self) -> bool {
        !self.pending.swap(true, Ordering::AcqRel)
    }

    pub(crate) fn begin_drain(&self) {
        self.pending.store(false, Ordering::Release);
    }

    /// Consumer-side check-and-clear: returns whether a signal was pending,
    /// clearing it atomically. For a poll loop that only wants to act when
    /// something actually happened since the last check.
    // Only consumed by `gpui_shell` (M1a's poll loop), which the wgpu
    // `petruterm` binary crate doesn't compile in (see `src/main.rs`'s
    // module list vs `src/lib.rs`'s) — dead there, live in `gpui-petruterm`.
    #[allow(dead_code)]
    pub(crate) fn take_pending(&self) -> bool {
        self.pending.swap(false, Ordering::AcqRel)
    }
}

#[cfg(test)]
mod tests {
    use super::WakeupGate;

    #[test]
    fn gate_sends_once_until_drain() {
        let gate = WakeupGate::new();
        assert!(gate.signal());
        assert!(!gate.signal());
        gate.begin_drain();
        assert!(gate.signal());
        assert!(!gate.signal());
    }

    #[test]
    fn signal_during_drain_is_not_lost() {
        let gate = WakeupGate::new();
        gate.begin_drain();
        assert!(gate.signal());
    }

    #[test]
    fn take_pending_clears_and_reports() {
        let gate = WakeupGate::new();
        assert!(!gate.take_pending());
        gate.signal();
        assert!(gate.take_pending());
        assert!(!gate.take_pending());
    }
}
