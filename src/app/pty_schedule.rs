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
}
