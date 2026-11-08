//! Control channel between agent hooks / the `petruterm notify` CLI and the running app.
//!
//! Each running instance listens on a Unix socket (see [`server`]); every pane's shell gets
//! `PETRUTERM_PANE_ID`, `PETRUTERM_SOCKET` and `PETRUTERM_BIN` in its environment, so
//! anything running inside the pane (an agent hook, a script) can report state. Messages are
//! newline-delimited JSON ([`Message`]). The payload is data: it is sanitized and mapped to an
//! [`AttentionEvent`], never executed. See
//! `docs/superpowers/specs/2026-10-15-agent-attention-design.md`.

pub mod agent_setup;
pub mod claude_hooks;
pub mod cli;
pub mod hook_files;
pub mod hooks_cli;
pub mod integrations;
pub mod msg_bus;
pub mod msg_cli;
pub mod peer;
pub mod server;
pub mod shell;

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::term::attention::{AgentState, AttentionEvent};
use crate::term::osc_notify::sanitize;

/// Longest accepted message line, in bytes. Longer lines drop the connection.
pub const MAX_LINE_BYTES: usize = 16 * 1024;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Message {
    /// Pane uid from `PETRUTERM_PANE_ID` (see [`next_pane_uid`]).
    #[serde(default)]
    pub pane: u64,
    /// `working`, `needs-input`, `done` or `idle`. Absent: a plain notification.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
    /// What the agent is doing right now (`Bash: cargo test`); only meaningful with a
    /// `working` state. Cleaned again on receipt (see `into_event`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub activity: Option<String>,
    /// The agent's session id (e.g. Claude Code's `session_id`), announced by its hooks.
    /// Use [`Message::session`], which validates it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session: Option<String>,
    /// An agent-to-agent message request (see `msg_bus`). Answered on the same connection.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub msg: Option<MsgPayload>,
    /// Remote-access command (`start`, `stop`, `offer`); handled by `remote::control`, not
    /// an attention event.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub secret: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires: Option<u64>,
}

/// The body of a message request: who to send to and what.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MsgPayload {
    pub to: String,
    pub text: String,
    #[serde(default)]
    pub no_reply: bool,
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
    #[cfg_attr(not(test), allow(dead_code))] // read by the gpui build only
    /// The session id if it is well formed (`[A-Za-z0-9_-]`, 1-64 chars); otherwise `None`.
    pub fn session(&self) -> Option<String> {
        self.session
            .as_deref()
            .and_then(crate::term::agents::sanitize_session)
    }

    /// Sanitize and map to the pane uid plus the attention event. `None` when the message
    /// carries nothing actionable (unknown state, or no state and no text).
    pub fn into_event(self) -> Option<(u64, AttentionEvent)> {
        let clean = |s: Option<String>| s.map(|t| sanitize(&t)).filter(|t| !t.is_empty());
        let title = clean(self.title);
        let body = clean(self.body);
        let activity = self
            .activity
            .as_deref()
            .and_then(crate::term::agents::clean_activity);
        let event = match self.state {
            Some(s) => AttentionEvent::State {
                state: parse_state(&s)?,
                title,
                message: body,
                activity,
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
/// `Contents/Resources/bin`, `PETRUTERM_BIN_DIR`, or the source tree's `scripts/shims` when
/// the executable runs from that tree's `target/` (so `cargo run`, debug or release, works
/// without bundling; a copied or installed binary never points into a source tree).
pub fn shim_dir() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok();
    let bundled = exe
        .as_ref()
        .and_then(|exe| Some(exe.parent()?.parent()?.join("Resources/bin")));
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let dev = exe
        .as_ref()
        .is_some_and(|exe| exe.starts_with(manifest.join("target")))
        .then(|| manifest.join("scripts/shims"));
    bundled
        .into_iter()
        .chain(std::env::var_os("PETRUTERM_BIN_DIR").map(PathBuf::from))
        .chain(dev)
        .find(|d| d.is_dir())
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
            ..Message::default()
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
                activity: None,
            }
        );
    }

    #[test]
    fn activity_is_cleaned_forwarded_and_ignored_without_state() {
        let m = Message {
            activity: Some(format!("Bash:\n\tcargo\x1b test {}", "x".repeat(5000))),
            ..msg(Some("working"), None, None)
        };
        let (_, ev) = m.into_event().unwrap();
        let AttentionEvent::State {
            activity, message, ..
        } = ev
        else {
            panic!("expected state")
        };
        let a = activity.unwrap();
        assert!(a.starts_with("Bash: cargo test"));
        assert!(a.chars().count() <= crate::term::agents::MAX_ACTIVITY_CHARS);
        assert!(!a.chars().any(char::is_control));
        assert_eq!(message, None);

        // needs-input keeps its own message next to the activity field
        let m = Message {
            activity: Some("Bash: ls".into()),
            ..msg(Some("needs-input"), None, Some("Allow?"))
        };
        let (_, ev) = m.into_event().unwrap();
        let AttentionEvent::State {
            message, activity, ..
        } = ev
        else {
            panic!("expected state")
        };
        assert_eq!(
            (message.as_deref(), activity.as_deref()),
            (Some("Allow?"), Some("Bash: ls"))
        );

        // blank activity counts as absent; a state-less message ignores it
        let blank = Message {
            activity: Some(" \n ".into()),
            ..msg(Some("working"), None, None)
        };
        let (_, ev) = blank.into_event().unwrap();
        assert!(matches!(ev, AttentionEvent::State { activity: None, .. }));
        let plain = Message {
            activity: Some("Bash: ls".into()),
            ..msg(None, None, Some("hi"))
        };
        assert!(matches!(
            plain.into_event().unwrap().1,
            AttentionEvent::Notify { .. }
        ));
    }

    #[test]
    fn activity_field_is_optional_on_the_wire() {
        let old = parse_line(r#"{"pane":3,"state":"working"}"#).unwrap();
        assert_eq!(old.activity, None);
        let json = serde_json::to_string(&old).unwrap();
        assert!(!json.contains("activity"));
        let new = parse_line(r#"{"pane":3,"state":"working","activity":"Read: a"}"#).unwrap();
        assert_eq!(new.activity.as_deref(), Some("Read: a"));
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
        // `pane` is optional (control commands carry none), but an empty message does nothing.
        assert!(parse_line("{}").and_then(Message::into_event).is_none());
        assert!(parse_line(r#"{"pane":"x"}"#).is_none());
        let m = parse_line(r#"{"pane":3,"state":"done","extra":1}"#).unwrap();
        assert_eq!(m.pane, 3);
        assert_eq!(m.state.as_deref(), Some("done"));
    }

    #[test]
    fn session_is_validated_and_does_not_make_an_event() {
        let id = "3f6c0a52-91d4-4b1e-8c3a-0d2f7e5b9a11";
        let m = parse_line(&format!(r#"{{"pane":2,"session":"{id}"}}"#)).unwrap();
        assert_eq!(m.session().as_deref(), Some(id));
        assert!(m.into_event().is_none());
        let bad = parse_line(r#"{"pane":2,"session":"../../x"}"#).unwrap();
        assert_eq!(bad.session(), None);
        let long = format!(r#"{{"pane":2,"session":"{}"}}"#, "a".repeat(200));
        assert_eq!(parse_line(&long).unwrap().session(), None);
        assert_eq!(parse_line(r#"{"pane":2}"#).unwrap().session(), None);
    }

    #[test]
    fn msg_requests_parse_and_hook_messages_are_unchanged() {
        let m =
            parse_line(r#"{"pane":4,"msg":{"to":"kterm","text":"hi","no_reply":true}}"#).unwrap();
        let p = m.msg.clone().unwrap();
        assert_eq!(
            (p.to.as_str(), p.text.as_str(), p.no_reply),
            ("kterm", "hi", true)
        );
        assert!(
            m.into_event().is_none(),
            "a msg request is not an attention event"
        );
        let h = parse_line(r#"{"pane":4,"state":"done"}"#).unwrap();
        assert!(h.msg.is_none());
        let json = serde_json::to_string(&h).unwrap();
        assert!(!json.contains("msg"), "{json}");
        let d = parse_line(r#"{"pane":4,"msg":{"to":"k","text":"x"}}"#).unwrap();
        assert!(!d.msg.unwrap().no_reply);
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
