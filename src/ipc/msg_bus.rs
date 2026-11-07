//! Request/reply plumbing between the control socket's connection threads and the UI thread.
//! A connection thread submits a request and waits (at most 2 s) for the UI thread to decide
//! and deliver; the UI thread drains requests each tick. Builds that cannot deliver never call
//! `enable`, so requests are answered `Unsupported` immediately.

// Delivery is implemented by the gpui build only; the wgpu binary compiles but never drains.
#![allow(dead_code)]

use crate::term::messaging::Outcome;
use crossbeam_channel::{Receiver, Sender, TrySendError};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{sync_channel, SyncSender};
use std::sync::OnceLock;
use std::time::Duration;

const QUEUE_CAP: usize = 64;
const REPLY_TIMEOUT: Duration = Duration::from_secs(2);

pub struct MsgRequest {
    /// Pid of the connecting process, set by the server from the connection.
    pub peer_pid: u32,
    /// `PETRUTERM_PANE_ID` the caller claims; must agree with the peer pid.
    pub pane_hint: u64,
    pub to: String,
    pub text: String,
    pub no_reply: bool,
    pub reply: SyncSender<Outcome>,
}

pub struct Bus {
    enabled: AtomicBool,
    tx: Sender<MsgRequest>,
    rx: Receiver<MsgRequest>,
}

impl Bus {
    pub fn new() -> Self {
        let (tx, rx) = crossbeam_channel::bounded(QUEUE_CAP);
        Self {
            enabled: AtomicBool::new(false),
            tx,
            rx,
        }
    }

    pub fn enable(&self) {
        self.enabled.store(true, Ordering::SeqCst);
    }

    /// Called on a connection thread: queue the request, wake the UI, wait for its answer.
    pub fn submit(
        &self,
        peer_pid: u32,
        pane_hint: u64,
        to: String,
        text: String,
        no_reply: bool,
        wakeup: &(dyn Fn() + Send + Sync),
    ) -> Outcome {
        if !self.enabled.load(Ordering::SeqCst) {
            return Outcome::Unsupported;
        }
        let (reply, rx) = sync_channel(1);
        let req = MsgRequest {
            peer_pid,
            pane_hint,
            to,
            text,
            no_reply,
            reply,
        };
        match self.tx.try_send(req) {
            Ok(()) => wakeup(),
            Err(TrySendError::Full(_) | TrySendError::Disconnected(_)) => return Outcome::Timeout,
        }
        rx.recv_timeout(REPLY_TIMEOUT).unwrap_or(Outcome::Timeout)
    }

    /// Called on the UI thread: requests received since the last call.
    pub fn drain(&self) -> Vec<MsgRequest> {
        self.rx.try_iter().collect()
    }
}

impl Default for Bus {
    fn default() -> Self {
        Self::new()
    }
}

static BUS: OnceLock<Bus> = OnceLock::new();

/// The process-wide bus used by the control socket.
pub fn global() -> &'static Bus {
    BUS.get_or_init(Bus::new)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn answer_once(bus: &'static Bus, outcome: Outcome) -> std::thread::JoinHandle<()> {
        std::thread::spawn(move || {
            for _ in 0..200 {
                if let Some(req) = bus.drain().into_iter().next() {
                    let _ = req.reply.send(outcome);
                    return;
                }
                std::thread::sleep(Duration::from_millis(5));
            }
        })
    }

    fn leak() -> &'static Bus {
        Box::leak(Box::new(Bus::new()))
    }

    #[test]
    fn disabled_bus_answers_unsupported_immediately() {
        let bus = Bus::new();
        let out = bus.submit(1, 2, "k".into(), "hi".into(), false, &|| {});
        assert_eq!(out, Outcome::Unsupported);
    }

    #[test]
    fn enabled_bus_returns_the_ui_threads_answer() {
        let bus = leak();
        bus.enable();
        let h = answer_once(bus, Outcome::Delivered("kterm".into()));
        let out = bus.submit(10, 3, "kterm".into(), "hi".into(), true, &|| {});
        h.join().unwrap();
        assert_eq!(out, Outcome::Delivered("kterm".into()));
    }

    #[test]
    fn no_answer_times_out() {
        let bus = Bus::new();
        bus.enable();
        let t = std::time::Instant::now();
        let out = bus.submit(1, 1, "k".into(), "hi".into(), false, &|| {});
        assert_eq!(out, Outcome::Timeout);
        assert!(t.elapsed() >= REPLY_TIMEOUT);
    }

    #[test]
    fn requests_carry_their_fields_and_wakeup_fires() {
        let bus = leak();
        bus.enable();
        let woke = std::sync::Arc::new(AtomicBool::new(false));
        let w = woke.clone();
        let h = std::thread::spawn(move || {
            bus.submit(77, 5, "kterm".into(), "text".into(), true, &move || {
                w.store(true, Ordering::SeqCst)
            })
        });
        let mut got = None;
        for _ in 0..200 {
            if let Some(r) = bus.drain().into_iter().next() {
                got = Some(r);
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        let r = got.expect("request queued");
        assert_eq!(
            (r.peer_pid, r.pane_hint, r.to.as_str(), r.no_reply),
            (77, 5, "kterm", true)
        );
        assert!(woke.load(Ordering::SeqCst));
        let _ = r.reply.send(Outcome::Empty);
        assert_eq!(h.join().unwrap(), Outcome::Empty);
    }
}
