// Agent-to-agent messaging router (gpui build). Each tick it answers the requests that
// `petruterm msg` sent over the control socket: it verifies who sent it, finds the target by
// custom name, applies acceptance, rate and conversation limits, then types the framed message
// into the target's pane. The decision itself is `term::messaging::decide`. Message text is
// never written to disk or logs. See `specs/002-agent-messaging/`.

use std::collections::HashMap;
use std::time::Instant;

use crate::ipc::msg_bus::{self, MsgRequest};
use crate::term::agents::validate_name;
use crate::term::messaging::decide::{decide, DecideInput};
use crate::term::messaging::frame::{build_frame, Sender};
use crate::term::messaging::limits::{Conversations, RateLimiter};
use crate::term::messaging::{sanitize_text, Outcome};

use super::GpuiShellRoot;

#[derive(Default)]
pub struct AgentMsgState {
    conversations: Conversations,
    rates: HashMap<(usize, usize), RateLimiter>,
    /// When a message was last delivered to a terminal (for the row badge flash).
    last_delivered: HashMap<usize, Instant>,
}

impl AgentMsgState {
    pub fn delivered_recently(&self, terminal_id: usize, now: Instant) -> bool {
        self.last_delivered
            .get(&terminal_id)
            .is_some_and(|t| now.duration_since(*t).as_secs() < 10)
    }

    fn prune(&mut self, now: Instant) {
        self.conversations.prune(now);
        self.rates.retain(|_, r| !r.is_idle(now));
        self.last_delivered
            .retain(|_, t| now.duration_since(*t).as_secs() < 10);
    }
}

/// Is the process alive? Non-blocking (`kill(pid, 0)`).
fn pid_alive(pid: u32) -> bool {
    // SAFETY: signal 0 only checks existence/permission.
    let rc = unsafe { libc::kill(pid as libc::pid_t, 0) };
    rc == 0 || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
}

/// A name echoed back to the caller must be safe to print: address-safe and short.
fn echo_name(raw: &str) -> String {
    raw.chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        .take(32)
        .collect()
}

impl GpuiShellRoot {
    /// `workspace / tab` of the pane holding `terminal_id`.
    fn location_of(&self, terminal_id: usize) -> Option<String> {
        self.workspaces.workspaces().iter().find_map(|ws| {
            ws.tab_panes.iter().enumerate().find_map(|(ti, forest)| {
                forest.root.leaf_ids().contains(&terminal_id).then(|| {
                    let tab = ws.tabs.tabs().get(ti).map_or("", |t| t.title.as_str());
                    format!("{} / {}", ws.name, tab)
                })
            })
        })
    }

    /// The terminal the caller of `req` runs in: the kernel-reported peer pid must descend
    /// from that pane's shell, and agree with the pane id the caller claims.
    fn sender_terminal(&self, req: &MsgRequest) -> Option<usize> {
        if req.peer_pid == 0 {
            return None;
        }
        let chain = crate::ipc::peer::ancestors(req.peer_pid);
        self.terminals
            .iter()
            .find(|(_, t)| chain.contains(&t.child_pid) && t.pane_uid == req.pane_hint)
            .map(|(id, _)| *id)
    }

    /// How the receiver sees the sender in `terminal_id`.
    fn sender_for(&self, terminal_id: usize) -> Sender {
        let location = self.location_of(terminal_id).unwrap_or_default();
        let name = self.agent_names.name_of(terminal_id).map(str::to_owned);
        let agent = self
            .agent_scan
            .detections()
            .iter()
            .find(|d| d.terminal_id == terminal_id)
            .map(|d| d.agent.clone());
        match (agent, name) {
            (Some(agent), Some(name)) => Sender::NamedAgent { name, agent },
            (Some(_), None) => Sender::UnnamedAgent { location },
            (None, name) => Sender::Shell { location, name },
        }
    }

    /// Deliver one message to `target` (None = no such agent). `from` is the sender terminal
    /// (None = the user); `shown` is the name to put in the outcome.
    #[allow(clippy::too_many_arguments)]
    fn route(
        &mut self,
        from: Option<usize>,
        sender_resolved: bool,
        target: Option<usize>,
        shown: String,
        text: &str,
        no_reply: bool,
        now: Instant,
    ) -> Outcome {
        let Some(target) = target else {
            return Outcome::UnknownName(shown);
        };
        let det = self
            .agent_scan
            .detections()
            .iter()
            .find(|d| d.terminal_id == target)
            .cloned();
        let target_name = self
            .agent_names
            .name_of(target)
            .map_or(shown.clone(), str::to_owned);
        let paste_on = self
            .terminals
            .get(&target)
            .is_some_and(|t| t.bracketed_paste_mode());
        let text = sanitize_text(text, paste_on);
        let from_user = from.is_none() && sender_resolved;
        let key = from.map(|f| (f, target));
        let rate = key.map_or(Ok(()), |k| self.msg.rates.entry(k).or_default().check(now));
        let input = DecideInput {
            from_user,
            sender_resolved,
            same_terminal: from == Some(target),
            target_found: true,
            target_alive: det.as_ref().is_some_and(|d| pid_alive(d.pid))
                && self.terminals.contains_key(&target),
            target_accepting: det
                .as_ref()
                .is_some_and(|d| self.agent_accept.is_accepting(target, d.pid)),
            target_name: &target_name,
            text: &text,
            rate,
            conversation_ok: from.is_none_or(|f| self.msg.conversations.allowed(f, target, now)),
        };
        if let Err(outcome) = decide(&input) {
            return outcome;
        }
        let sender = from.map_or(Sender::User, |f| self.sender_for(f));
        let frame = build_frame(&sender, &text, !no_reply, paste_on);
        let Some(terminal) = self.terminals.get(&target) else {
            return Outcome::AgentGone(target_name);
        };
        if paste_on {
            let mut data = b"\x1b[200~".to_vec();
            data.extend_from_slice(frame.as_bytes());
            data.extend_from_slice(b"\x1b[201~");
            terminal.write_input(&data);
        } else {
            terminal.write_input(frame.as_bytes());
        }
        terminal.write_input(b"\r");
        match from {
            Some(f) => {
                self.msg.rates.entry((f, target)).or_default().record(now);
                self.msg.conversations.record(f, target, now);
            }
            None => self.msg.conversations.reset_terminal(target),
        }
        self.msg.last_delivered.insert(target, now);
        // outcome and size only: no message text in logs
        log::info!(
            "msg delivered to terminal {target} ({} chars)",
            text.chars().count()
        );
        Outcome::Delivered(target_name)
    }

    fn handle_msg_request(&mut self, req: &MsgRequest, now: Instant) -> Outcome {
        let from = self.sender_terminal(req);
        let target = validate_name(&req.to)
            .ok()
            .and_then(|n| self.agent_names.terminal_for(&n));
        let shown = echo_name(&req.to);
        self.route(
            from,
            from.is_some(),
            target,
            shown,
            &req.text,
            req.no_reply,
            now,
        )
    }

    /// The user sends `text` to the agent in `terminal_id` from the Agents list.
    pub(super) fn send_as_user(&mut self, terminal_id: usize, text: &str) -> Outcome {
        let shown = self
            .agent_names
            .name_of(terminal_id)
            .map(str::to_owned)
            .or_else(|| {
                self.agent_scan
                    .detections()
                    .iter()
                    .find(|d| d.terminal_id == terminal_id)
                    .map(|d| d.agent.clone())
            })
            .unwrap_or_default();
        self.route(
            None,
            true,
            Some(terminal_id),
            shown,
            text,
            true,
            Instant::now(),
        )
    }
}

/// Answer the requests that arrived since the last tick. True when anything was delivered
/// (the row badge changes).
pub(super) fn tick(root: &mut GpuiShellRoot) -> bool {
    let now = Instant::now();
    let mut changed = false;
    for req in msg_bus::global().drain() {
        let outcome = root.handle_msg_request(&req, now);
        changed |= matches!(outcome, Outcome::Delivered(_));
        let _ = req.reply.send(outcome);
    }
    root.msg.prune(now);
    changed
}
