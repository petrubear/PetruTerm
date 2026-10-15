//! Control channel between agent hooks / the `petruterm notify` CLI and the running app.
//!
//! Each running instance listens on a Unix socket (see [`server`]); every pane's shell gets
//! `PETRUTERM_PANE_ID`, `PETRUTERM_SOCKET` and `PETRUTERM_BIN` in its environment, so
//! anything running inside the pane (an agent hook, a script) can report state. Messages are
//! newline-delimited JSON ([`Message`]). The payload is data: it is sanitized and mapped to an
//! [`AttentionEvent`], never executed. See
//! `docs/superpowers/specs/2026-10-15-agent-attention-design.md`.

pub mod claude_hooks;
pub mod cli;
pub mod server;
pub mod shell;

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::term::attention::{AgentState, AttentionEvent};
use crate::term::osc_notify::sanitize;

/// Longest accepted message line, in bytes. Longer lines drop the connection.
pub const MAX_LINE_BYTES: usize = 16 * 1024;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Message {
    /// Pane uid from `PETRUTERM_PANE_ID` (see [`next_pane_uid`]).
    pub pane: u64,
    /// `working`, `needs-input`, `done` or `idle`. Absent: a plain notification.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
}

pub fn parse_state(s: &str) -> Option<AgentState> {
    match s {
        "working" => Some(AgentState::Working),
        "needs-input" => Some(AgentState::NeedsInput),
        "done" => Some(AgentState::Done),
        "idle" => Some(AgentState::Idle),
        _ => None,
    }
}

pub fn parse_line(line: &str) -> Option<Message> {
    serde_json::from_str(line.trim()).ok()
}

impl Message {
    /// Sanitize and map to the pane uid plus the attention event. `None` when the message
    /// carries nothing actionable (unknown state, or no state and no text).
    pub fn into_event(self) -> Option<(u64, AttentionEvent)> {
        let clean = |s: Option<String>| s.map(|t| sanitize(&t)).filter(|t| !t.is_empty());
        let title = clean(self.title);
        let body = clean(self.body);
        let event = match self.state {
            Some(s) => AttentionEvent::State {
                state: parse_state(&s)?,
                title,
                message: body,
            },
            None => AttentionEvent::Notify { title, body: body? },
        };
        Some((self.pane, event))
    }
}

static NEXT_PANE_UID: AtomicU64 = AtomicU64::new(1);

/// Process-unique id for a new pane. Exported to the pane's shell as `PETRUTERM_PANE_ID`;
/// the app resolves it back to a terminal. Not the terminal id itself, because the wgpu build
/// allocates that after the shell has already been spawned.
pub fn next_pane_uid() -> u64 {
    NEXT_PANE_UID.fetch_add(1, Ordering::Relaxed)
}

/// Directory with agent shims (`claude`, ...) to put first on a pane's PATH: the bundle's
/// `Contents/Resources/bin`, or `PETRUTERM_BIN_DIR` for unbundled builds.
pub fn shim_dir() -> Option<PathBuf> {
    let bundled = std::env::current_exe()
        .ok()
        .and_then(|exe| Some(exe.parent()?.parent()?.join("Resources/bin")));
    bundled
        .filter(|d| d.is_dir())
        .or_else(|| std::env::var_os("PETRUTERM_BIN_DIR").map(PathBuf::from))
        .filter(|d| d.is_dir())
}

/// Environment to add to a new pane's shell.
pub fn pane_env(pane_uid: u64, shell: &str) -> Vec<(String, String)> {
    let mut env = vec![("PETRUTERM_PANE_ID".to_string(), pane_uid.to_string())];
    if let Some(sock) = server::socket_path() {
        env.push((
            "PETRUTERM_SOCKET".to_string(),
            sock.to_string_lossy().into_owned(),
        ));
    }
    if let Ok(exe) = std::env::current_exe() {
        env.push((
            "PETRUTERM_BIN".to_string(),
            exe.to_string_lossy().into_owned(),
        ));
    }
    if let Some(dir) = shim_dir() {
        let mut path = std::ffi::OsString::from(&dir);
        if let Some(old) = std::env::var_os("PATH") {
            path.push(":");
            path.push(old);
        }
        env.push(("PATH".to_string(), path.to_string_lossy().into_owned()));
        // Login-shell startup files reorder PATH; put the shim back first at the first prompt.
        if let Some(base) = server::state_dir() {
            env.extend(shell::integration_env(shell, &dir, &base));
        }
    }
    env
}

#[cfg(test)]
mod tests {
    use super::*;

    fn msg(state: Option<&str>, title: Option<&str>, body: Option<&str>) -> Message {
        Message {
            pane: 7,
            state: state.map(str::to_string),
            title: title.map(str::to_string),
            body: body.map(str::to_string),
        }
    }

    #[test]
    fn state_message_maps_to_state_event() {
        let (pane, ev) = msg(Some("needs-input"), Some("Claude Code"), Some("Allow?"))
            .into_event()
            .unwrap();
        assert_eq!(pane, 7);
        assert_eq!(
            ev,
            AttentionEvent::State {
                state: AgentState::NeedsInput,
                title: Some("Claude Code".into()),
                message: Some("Allow?".into()),
            }
        );
    }

    #[test]
    fn stateless_message_is_a_notification_and_needs_a_body() {
        let (_, ev) = msg(None, None, Some("hi")).into_event().unwrap();
        assert_eq!(
            ev,
            AttentionEvent::Notify {
                title: None,
                body: "hi".into()
            }
        );
        assert!(msg(None, Some("t"), None).into_event().is_none());
        assert!(msg(None, None, Some("  \x01 ")).into_event().is_none());
    }

    #[test]
    fn unknown_state_is_ignored() {
        assert!(msg(Some("exploding"), None, None).into_event().is_none());
    }

    #[test]
    fn text_is_sanitized_and_capped() {
        let long = "x".repeat(5000);
        let (_, ev) = msg(Some("done"), None, Some(&format!("a\x1b[31mb{long}")))
            .into_event()
            .unwrap();
        let AttentionEvent::State { message, .. } = ev else {
            panic!("expected state")
        };
        let m = message.unwrap();
        assert!(!m.contains('\x1b'));
        assert!(m.chars().count() <= crate::term::osc_notify::MAX_TEXT_CHARS);
    }

    #[test]
    fn parse_line_rejects_garbage_and_accepts_unknown_fields() {
        assert!(parse_line("not json").is_none());
        assert!(parse_line("{}").is_none()); // pane is required
        assert!(parse_line(r#"{"pane":"x"}"#).is_none());
        let m = parse_line(r#"{"pane":3,"state":"done","extra":1}"#).unwrap();
        assert_eq!(m.pane, 3);
        assert_eq!(m.state.as_deref(), Some("done"));
    }

    #[test]
    fn pane_uids_are_unique() {
        assert_ne!(next_pane_uid(), next_pane_uid());
    }

    #[test]
    fn pane_env_exports_id_and_binary() {
        let env = pane_env(42, "/bin/zsh");
        assert!(env.contains(&("PETRUTERM_PANE_ID".to_string(), "42".to_string())));
        assert!(env.iter().any(|(k, _)| k == "PETRUTERM_BIN"));
    }
}
