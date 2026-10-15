//! Hand-off between the network side (tokio, `Send`) and the GUI thread that owns the
//! terminals (`Rc<Terminal>` in gpui, so never touched from another thread).
//!
//! The network side enqueues a [`Request`]; each binary calls [`service`] from its event
//! loop (same pattern as `ipc::server::drain`) with a [`PaneView`] over its panes. The reply
//! travels back through a oneshot.

use crate::term::tee::RemoteChunk;
use crate::term::Terminal;
use crossbeam_channel::{Receiver, Sender};
use remote_proto::msg::PaneInfo;
use std::sync::OnceLock;
use tokio::sync::oneshot;

pub struct Attached {
    pub cols: u16,
    pub rows: u16,
    pub rx: Receiver<RemoteChunk>,
}

pub enum Request {
    List(oneshot::Sender<Vec<PaneInfo>>),
    Attach {
        pane: u64,
        reply: oneshot::Sender<Result<Attached, String>>,
    },
    Input {
        pane: u64,
        bytes: Vec<u8>,
    },
}

/// What each binary exposes about its panes.
pub trait PaneView {
    fn panes(&self) -> Vec<PaneInfo>;
    fn terminal(&self, uid: u64) -> Option<&Terminal>;
}

static QUEUE: OnceLock<(Sender<Request>, Receiver<Request>)> = OnceLock::new();

fn queue() -> &'static (Sender<Request>, Receiver<Request>) {
    QUEUE.get_or_init(crossbeam_channel::unbounded)
}

/// Queue depth limit: requests beyond it are dropped (their replies fail), so a stalled GUI
/// thread cannot make the host accumulate unbounded work.
const MAX_PENDING: usize = 1024;

pub fn submit(req: Request) -> bool {
    let (tx, _) = queue();
    if tx.len() >= MAX_PENDING {
        return false;
    }
    tx.send(req).is_ok()
}

/// Answer all pending requests. Call from the GUI thread.
pub fn service(view: &dyn PaneView) {
    for req in queue().1.try_iter() {
        handle(req, view);
    }
}

/// Subscription queue depth, in chunks. A client that falls this far behind is detached.
const SUBSCRIBE_DEPTH: usize = 512;

pub(crate) fn handle(req: Request, view: &dyn PaneView) {
    match req {
        Request::List(reply) => {
            let _ = reply.send(view.panes());
        }
        Request::Attach { pane, reply } => {
            let result = match view.terminal(pane) {
                Some(t) => Ok(Attached {
                    cols: t.cols.get(),
                    rows: t.rows.get(),
                    rx: t.subscribe_remote(SUBSCRIBE_DEPTH),
                }),
                None => Err("no such pane".to_string()),
            };
            let _ = reply.send(result);
        }
        Request::Input { pane, bytes } => {
            if let Some(t) = view.terminal(pane) {
                t.write_input(&bytes);
            }
        }
    }
}
