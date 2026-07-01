use std::path::PathBuf;
use tokio::sync::oneshot;

/// Messages sent from the ACP tokio task to the main (winit) thread for
/// terminal operations the agent has requested.
pub enum AcpTerminalRequest {
    /// Agent called `terminal/create` — create a pane running the command.
    Create {
        command: String,
        args: Vec<String>,
        cwd: Option<PathBuf>,
        /// Responds with the new pane_id.
        tx: oneshot::Sender<usize>,
    },
    /// Agent called `terminal/output` — return current scrollback as text plus
    /// the exit code if the process has already finished.
    GetOutput {
        pane_id: usize,
        tx: oneshot::Sender<(String, Option<i32>)>,
    },
    /// Agent called `terminal/wait_for_exit` — block until the pane exits.
    WaitForExit {
        pane_id: usize,
        tx: oneshot::Sender<i32>,
    },
    /// Agent called `terminal/kill` or `terminal/release` — close the pane.
    Kill { pane_id: usize },
}
