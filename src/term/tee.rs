//! Fan-out of a pane's raw PTY output to remote subscribers.
//!
//! Ordering: `send` runs on the reader thread with the term lock held, right after the bytes
//! were applied to the grid. `subscribe` takes the same lock to snapshot the grid and
//! register. A subscriber therefore sees a snapshot that already includes every byte sent
//! before it registered, and every byte after, with no gap and no duplicate.
//! (Bytes held back by a synchronized-update block are the one exception: they are
//! already teed but not yet in the grid, so a client joining mid-block misses them until the
//! next redraw.)

use crossbeam_channel::{Receiver, Sender, TrySendError};
use parking_lot::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RemoteChunk {
    /// ANSI that rebuilds the current screen. Always the first chunk of a subscription.
    Snapshot(Vec<u8>),
    /// Raw PTY bytes, in order, following the snapshot.
    Output(Vec<u8>),
}

#[derive(Default)]
pub struct Tee {
    subs: Mutex<Vec<Sender<RemoteChunk>>>,
    count: AtomicUsize,
}

impl Tee {
    /// Forward `bytes` to every subscriber. A subscriber whose queue is full or whose
    /// receiver is gone is removed; its receiver then sees the channel disconnect and the
    /// client can re-attach for a fresh snapshot. Never blocks.
    pub fn send(&self, bytes: &[u8]) {
        if self.count.load(Ordering::Relaxed) == 0 {
            return;
        }
        let mut subs = self.subs.lock();
        subs.retain(
            |tx| match tx.try_send(RemoteChunk::Output(bytes.to_vec())) {
                Ok(()) => true,
                Err(TrySendError::Full(_)) | Err(TrySendError::Disconnected(_)) => false,
            },
        );
        self.count.store(subs.len(), Ordering::Relaxed);
    }

    /// Register a subscriber. `snapshot` must be produced while the caller holds the term
    /// lock (see module docs); it is queued as the first chunk. `capacity` is the queue
    /// depth in chunks and must be at least 2.
    pub fn subscribe(&self, snapshot: Vec<u8>, capacity: usize) -> Receiver<RemoteChunk> {
        let (tx, rx) = crossbeam_channel::bounded(capacity.max(2));
        let _ = tx.try_send(RemoteChunk::Snapshot(snapshot));
        let mut subs = self.subs.lock();
        subs.push(tx);
        self.count.store(subs.len(), Ordering::Relaxed);
        rx
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_first_then_output_in_order() {
        let tee = Tee::default();
        let rx = tee.subscribe(b"snap".to_vec(), 8);
        tee.send(b"a");
        tee.send(b"b");
        assert_eq!(rx.try_recv(), Ok(RemoteChunk::Snapshot(b"snap".to_vec())));
        assert_eq!(rx.try_recv(), Ok(RemoteChunk::Output(b"a".to_vec())));
        assert_eq!(rx.try_recv(), Ok(RemoteChunk::Output(b"b".to_vec())));
    }

    #[test]
    fn send_without_subscribers_is_a_no_op() {
        let tee = Tee::default();
        tee.send(b"x");
        let rx = tee.subscribe(Vec::new(), 4);
        assert_eq!(rx.try_recv(), Ok(RemoteChunk::Snapshot(Vec::new())));
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn slow_subscriber_is_dropped_without_affecting_others() {
        let tee = Tee::default();
        let slow = tee.subscribe(Vec::new(), 2);
        let fast = tee.subscribe(Vec::new(), 64);
        tee.send(b"1"); // slow: snapshot + 1 = full
        tee.send(b"2"); // slow overflows and is removed
        tee.send(b"3");
        assert_eq!(slow.try_iter().count(), 2);
        assert!(slow.recv().is_err(), "dropped subscriber disconnects");
        assert_eq!(fast.try_iter().count(), 4);
    }

    #[test]
    fn dropped_receiver_is_removed() {
        let tee = Tee::default();
        drop(tee.subscribe(Vec::new(), 4));
        tee.send(b"x");
        assert_eq!(tee.count.load(Ordering::Relaxed), 0);
    }
}
