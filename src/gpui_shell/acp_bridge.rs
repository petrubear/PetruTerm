// gpui chrome migration (M5a Task 4): drains `ChatPanelView::acp_
// terminal_rx` and answers each `AcpTerminalRequest` the agent sent
// (terminal/create, terminal/output, terminal/wait_for_exit, terminal/
// kill -- terminal/release is a client-side no-op, matching the wgpu
// build's own comment on why). Mirrors `App::handle_acp_terminal_
// requests` (`src/app/frame.rs:187-246`) exactly in control flow.
//
// `Kill`'s own mechanism is genuinely different from the wgpu build's:
// `Mux::kill_terminal` calls `term.pty.shutdown()` through `&mut
// Terminal`, unreachable here since every `gpui_shell` `Terminal` is
// `Rc<Terminal>` (confirmed: `actions.rs`'s own `reap_pane` doc comment
// already documents that `Drop for Pty` runs the full shutdown sequence
// when a terminal's last `Rc` is dropped, never a direct call). `Terminal
// ::child_pid: u32` needs no mutable access, so this sends SIGHUP
// directly -- the reader thread's own already-existing exit detection
// (EIO on the master fd) does the rest, exactly like a natural shell
// exit.

use std::path::PathBuf;

use gpui::Context;

use crate::llm::acp::terminal::AcpTerminalRequest;

use super::panes::SplitDir;
use super::{spawn_terminal_at, GpuiShellRoot};

impl GpuiShellRoot {
    /// Drain every pending `AcpTerminalRequest` and resolve any completed
    /// `WaitForExit` requests. Called from `poll.rs`'s existing 33ms tick.
    pub(super) fn handle_acp_terminal_requests(&mut self, cx: &mut Context<Self>) {
        loop {
            let Ok(req) = self.chat.acp_terminal_rx.try_recv() else {
                break;
            };
            match req {
                AcpTerminalRequest::Create {
                    command,
                    args,
                    cwd,
                    tx,
                } => {
                    let pane_id = self.open_terminal_for_acp(cwd, &command, &args, cx);
                    let _ = tx.send(pane_id);
                }
                AcpTerminalRequest::GetOutput { pane_id, tx } => {
                    let output = self.terminal_output_text(pane_id);
                    let exit_code = self.terminal_exit_code(pane_id);
                    let _ = tx.send((output, exit_code));
                }
                AcpTerminalRequest::WaitForExit { pane_id, tx } => {
                    if let Some(code) = self.terminal_exit_code(pane_id) {
                        let _ = tx.send(code);
                    } else {
                        self.pending_acp_wait_for_exit.push((pane_id, tx));
                    }
                }
                AcpTerminalRequest::Kill { pane_id } => {
                    if let Some(terminal) = self.terminals.get(&pane_id) {
                        unsafe {
                            libc::kill(terminal.child_pid as libc::pid_t, libc::SIGHUP);
                        }
                    }
                }
            }
        }

        let pending = std::mem::take(&mut self.pending_acp_wait_for_exit);
        for (pane_id, tx) in pending {
            match self.terminal_exit_code(pane_id) {
                Some(code) => {
                    let _ = tx.send(code);
                }
                None => self.pending_acp_wait_for_exit.push((pane_id, tx)),
            }
        }
    }

    /// Split the active pane for an ACP `terminal/create` request: spawn a
    /// terminal at `cwd` (`spawn_terminal_at` falls back to the process's
    /// own cwd when `cwd` is `None`, matching `CreateTerminalRequest::
    /// cwd`'s own optional shape exactly), split the active tab's tree
    /// around it, and immediately write the shell-quoted command + args
    /// to it. Returns the new terminal's id.
    fn open_terminal_for_acp(
        &mut self,
        cwd: Option<PathBuf>,
        command: &str,
        args: &[String],
        cx: &mut Context<Self>,
    ) -> usize {
        let (terminal, gate) = match spawn_terminal_at(80, 24, &self.config, cwd) {
            Ok(pair) => pair,
            Err(e) => {
                log::error!("gpui-shell: ACP terminal/create failed: {e:#}");
                return 0;
            }
        };
        let terminal_id = self.next_terminal_id;
        self.next_terminal_id += 1;
        self.terminals.insert(terminal_id, terminal);
        self.wakeup_gates.insert(terminal_id, gate);
        self.block_managers
            .insert(terminal_id, crate::term::BlockManager::new());
        let ws = self.workspaces.active_mut();
        let active = ws.tabs.active_index();
        ws.tab_panes[active].split(SplitDir::Horizontal, terminal_id);
        ws.zoomed_pane = None;
        if let Some(terminal) = self.terminals.get(&terminal_id) {
            let mut cmd_str = shell_quote(command);
            for arg in args {
                cmd_str.push(' ');
                cmd_str.push_str(&shell_quote(arg));
            }
            cmd_str.push('\r');
            terminal.write_input(cmd_str.as_bytes());
        }
        cx.notify();
        terminal_id
    }
}

/// POSIX single-quote a token so it reaches the shell as one literal
/// argument, regardless of embedded spaces or shell metacharacters (ACP
/// `terminal/create` passes `command`/`args` with argv semantics, not
/// shell semantics). Duplicated from `Mux`'s own private `shell_quote`
/// (`src/app/mux/mod.rs:156-167`) rather than cross-wired -- `src/app/` is
/// off-limits per this plan's own Global Constraints, and this is a tiny,
/// pure, three-line-rule-exempt helper (it's the whole reason this
/// function exists).
fn shell_quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('\'');
    for c in s.chars() {
        if c == '\'' {
            out.push_str("'\\''");
        } else {
            out.push(c);
        }
    }
    out.push('\'');
    out
}

#[cfg(test)]
mod tests {
    use super::shell_quote;

    #[test]
    fn shell_quote_wraps_plain_tokens_in_single_quotes() {
        assert_eq!(shell_quote("echo"), "'echo'");
    }

    #[test]
    fn shell_quote_escapes_embedded_single_quotes() {
        assert_eq!(shell_quote("it's"), "'it'\\''s'");
    }

    #[test]
    fn shell_quote_neutralizes_shell_metacharacters() {
        assert_eq!(shell_quote("a; rm -rf /"), "'a; rm -rf /'");
        assert_eq!(shell_quote("$(whoami)"), "'$(whoami)'");
    }
}
