//! Agent-to-agent messaging: shared pure logic (outcomes, text sanitizing, framing, limits,
//! the delivery decision). No UI or socket code here; see `ipc::msg_*` and
//! `gpui_shell::agent_msg`. Specs: `specs/002-agent-messaging/`.
//!
//! Message text is never written to disk or logs (FR-018).

// Used by the gpui binary and the CLI; the wgpu binary compiles this module but not all of it.
#![allow(dead_code)]

pub mod decide;
pub mod frame;
pub mod limits;

/// Longest accepted message, in characters, counted after sanitizing.
pub const MAX_TEXT_CHARS: usize = 4000;

/// What happened to a message; returned to the sender.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// Delivered to the agent with this name.
    Delivered(String),
    UnknownName(String),
    NotAccepting(String),
    RateLimited {
        retry_after_s: u64,
    },
    ConversationLimit,
    TooLong,
    AgentGone(String),
    /// The caller is not in a PetruTerm pane, or its identity could not be verified.
    SenderUnknown,
    SelfMessage,
    Empty,
    /// This build cannot deliver messages.
    Unsupported,
    /// The app did not answer in time.
    Timeout,
}

impl Outcome {
    /// The `result` string on the wire (contracts/socket-msg-protocol.md).
    pub fn result_name(&self) -> &'static str {
        match self {
            Outcome::Delivered(_) => "delivered",
            Outcome::UnknownName(_) => "unknown_name",
            Outcome::NotAccepting(_) => "not_accepting",
            Outcome::RateLimited { .. } => "rate_limited",
            Outcome::ConversationLimit => "conversation_limit",
            Outcome::TooLong => "too_long",
            Outcome::AgentGone(_) => "agent_gone",
            Outcome::SenderUnknown => "sender_unknown",
            Outcome::SelfMessage => "self_message",
            Outcome::Empty => "empty",
            Outcome::Unsupported => "unsupported",
            Outcome::Timeout => "timeout",
        }
    }

    /// The agent name carried by the outcome, if any.
    pub fn detail(&self) -> Option<&str> {
        match self {
            Outcome::Delivered(n)
            | Outcome::UnknownName(n)
            | Outcome::NotAccepting(n)
            | Outcome::AgentGone(n) => Some(n),
            _ => None,
        }
    }

    pub fn retry_after_s(&self) -> Option<u64> {
        match self {
            Outcome::RateLimited { retry_after_s } => Some(*retry_after_s),
            _ => None,
        }
    }

    /// Rebuild an outcome from its wire form; unknown `result` strings become `Timeout`.
    pub fn from_wire(result: &str, detail: Option<&str>, retry_after_s: Option<u64>) -> Outcome {
        let name = || detail.unwrap_or("agent").to_string();
        match result {
            "delivered" => Outcome::Delivered(name()),
            "unknown_name" => Outcome::UnknownName(name()),
            "not_accepting" => Outcome::NotAccepting(name()),
            "rate_limited" => Outcome::RateLimited {
                retry_after_s: retry_after_s.unwrap_or(1),
            },
            "conversation_limit" => Outcome::ConversationLimit,
            "too_long" => Outcome::TooLong,
            "agent_gone" => Outcome::AgentGone(name()),
            "sender_unknown" => Outcome::SenderUnknown,
            "self_message" => Outcome::SelfMessage,
            "empty" => Outcome::Empty,
            "unsupported" => Outcome::Unsupported,
            _ => Outcome::Timeout,
        }
    }

    /// The single line the CLI prints (contracts/cli-msg.md).
    pub fn cli_line(&self) -> String {
        match self {
            Outcome::Delivered(n) => format!("delivered to {n}"),
            Outcome::UnknownName(n) => format!("error: unknown agent {n}"),
            Outcome::NotAccepting(n) => format!(
                "error: {n} is not accepting messages (turn on \"Accept messages\" in the Agents list)"
            ),
            Outcome::RateLimited { retry_after_s } => {
                format!("error: rate limited, try again in {retry_after_s}s")
            }
            Outcome::ConversationLimit => "error: conversation limit reached (10 messages); wait 2 minutes or ask the user".into(),
            Outcome::TooLong => format!("error: message too long (max {MAX_TEXT_CHARS} characters)"),
            Outcome::AgentGone(n) => format!("error: {n} is no longer running"),
            Outcome::SenderUnknown => {
                "error: cannot send from here (run this inside a PetruTerm pane)".into()
            }
            Outcome::SelfMessage => "error: cannot message yourself".into(),
            Outcome::Empty => "error: empty message".into(),
            Outcome::Unsupported => "error: messaging is not available in this build".into(),
            Outcome::Timeout => "error: PetruTerm did not answer".into(),
        }
    }

    /// Process exit code of the CLI (0 delivered; 2 is reserved for usage errors).
    pub fn exit_code(&self) -> i32 {
        match self {
            Outcome::Delivered(_) => 0,
            Outcome::UnknownName(_) => 3,
            Outcome::NotAccepting(_) => 4,
            Outcome::RateLimited { .. } => 5,
            Outcome::ConversationLimit => 6,
            Outcome::TooLong => 7,
            Outcome::AgentGone(_) => 8,
            Outcome::SenderUnknown => 9,
            Outcome::SelfMessage => 10,
            Outcome::Empty => 11,
            Outcome::Unsupported => 12,
            Outcome::Timeout => 13,
        }
    }
}

/// Make message text safe to type into another agent: removes every control character (C0
/// including ESC and DEL, C1, CR), invisible line/paragraph separators and bidirectional
/// overrides, turns tabs into spaces, and keeps `\n` only when `keep_newlines` (otherwise it
/// becomes a space so a multi-line message cannot submit early). No escape sequence, including
/// one that would end a bracketed paste, survives.
pub fn sanitize_text(input: &str, keep_newlines: bool) -> String {
    input
        .chars()
        .filter_map(|c| match c {
            '\n' if keep_newlines => Some('\n'),
            '\n' | '\t' => Some(' '),
            '\u{2028}' | '\u{2029}' => Some(' '),
            '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}' | '\u{200B}'..='\u{200F}' => None,
            c if c.is_control() => None,
            c => Some(c),
        })
        .collect::<String>()
        .trim()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exit_codes_and_names_match_the_contract() {
        let n = || "kterm".to_string();
        let table: [(Outcome, i32, &str); 12] = [
            (Outcome::Delivered(n()), 0, "delivered"),
            (Outcome::UnknownName(n()), 3, "unknown_name"),
            (Outcome::NotAccepting(n()), 4, "not_accepting"),
            (Outcome::RateLimited { retry_after_s: 7 }, 5, "rate_limited"),
            (Outcome::ConversationLimit, 6, "conversation_limit"),
            (Outcome::TooLong, 7, "too_long"),
            (Outcome::AgentGone(n()), 8, "agent_gone"),
            (Outcome::SenderUnknown, 9, "sender_unknown"),
            (Outcome::SelfMessage, 10, "self_message"),
            (Outcome::Empty, 11, "empty"),
            (Outcome::Unsupported, 12, "unsupported"),
            (Outcome::Timeout, 13, "timeout"),
        ];
        for (o, code, name) in table {
            assert_eq!(o.exit_code(), code, "{o:?}");
            assert_eq!(o.result_name(), name);
            let back = Outcome::from_wire(name, o.detail(), o.retry_after_s());
            assert_eq!(back, o, "wire round trip");
            assert!(!o.cli_line().is_empty());
        }
    }

    #[test]
    fn cli_lines_follow_the_contract() {
        assert_eq!(
            Outcome::Delivered("kterm".into()).cli_line(),
            "delivered to kterm"
        );
        assert!(Outcome::NotAccepting("kterm".into())
            .cli_line()
            .contains("Accept messages"));
        assert_eq!(
            Outcome::RateLimited { retry_after_s: 12 }.cli_line(),
            "error: rate limited, try again in 12s"
        );
        assert!(Outcome::SenderUnknown.cli_line().contains("PetruTerm pane"));
    }

    #[test]
    fn unknown_wire_result_is_a_timeout() {
        assert_eq!(Outcome::from_wire("nonsense", None, None), Outcome::Timeout);
    }

    #[test]
    fn sanitize_removes_escapes_including_paste_end() {
        let s = sanitize_text("a\x1b[201~b\x1b]0;x\x07c\x7fd\r\ne\u{85}f", true);
        assert!(!s.contains('\x1b') && !s.contains('\x07') && !s.contains('\x7f'));
        assert!(!s.contains('\r') && !s.contains('\u{85}'));
        assert!(s.contains('\n'));
    }

    #[test]
    fn sanitize_newlines_tabs_and_unicode() {
        assert_eq!(sanitize_text("one\ntwo", true), "one\ntwo");
        assert_eq!(sanitize_text("one\ntwo", false), "one two");
        assert_eq!(sanitize_text("a\tb", true), "a b");
        assert_eq!(
            sanitize_text("  caf\u{e9} \u{4f60}\u{597d} ", true),
            "caf\u{e9} \u{4f60}\u{597d}"
        );
        assert_eq!(sanitize_text("a\u{202E}b\u{200B}c", true), "abc");
        assert_eq!(sanitize_text("\x1b\x07", true), "");
    }
}
