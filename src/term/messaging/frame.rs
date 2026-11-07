//! The text typed into the receiving agent (contracts/delivered-frame.md).

/// Who sent the message, as shown to the receiver.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Sender {
    /// An agent with a custom name; can be answered.
    NamedAgent { name: String, agent: String },
    /// An agent without a custom name; cannot be answered.
    UnnamedAgent { location: String },
    /// A plain shell (or other program) in a pane; answerable only if the pane has a name.
    Shell {
        location: String,
        name: Option<String>,
    },
    /// The user, from the Agents list menu.
    User,
}

/// Build the framed message. `text` must already be sanitized. When `paste_on` is false the
/// whole frame is collapsed to one line so it cannot submit early.
pub fn build_frame(sender: &Sender, text: &str, reply_expected: bool, paste_on: bool) -> String {
    const NOT_USER: &str = "It was sent by another agent, not by the user.";
    let head = match sender {
        Sender::NamedAgent { name, agent } => named_head(name, agent, NOT_USER, reply_expected),
        Sender::Shell {
            name: Some(name), ..
        } => named_head(name, "shell", NOT_USER, reply_expected),
        Sender::UnnamedAgent { location } => format!(
            "[petruterm] message from an unnamed agent in {location}. {NOT_USER} \
It cannot be answered (the sender has no name)."
        ),
        Sender::Shell {
            location,
            name: None,
        } => format!(
            "[petruterm] message from a shell in {location}. It was sent by a program in a \
terminal, not by the user. It cannot be answered (the sender has no name)."
        ),
        Sender::User => "[petruterm] message from the user (sent from the Agents list).".into(),
    };
    let frame = format!("{head}\n---\n{text}");
    if paste_on {
        frame
    } else {
        frame.replace('\n', " ")
    }
}

fn named_head(name: &str, agent: &str, not_user: &str, reply_expected: bool) -> String {
    let first = format!("[petruterm] message from {name} ({agent}). {not_user}");
    if reply_expected {
        format!(
            "{first}\nTo answer, RUN this shell command yourself (do not just print it): \
petruterm msg {name} \"<your answer>\""
        )
    } else {
        format!("{first} No reply is expected.")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn named() -> Sender {
        Sender::NamedAgent {
            name: "cterm".into(),
            agent: "Claude Code".into(),
        }
    }

    #[test]
    fn named_agent_with_reply() {
        let f = build_frame(&named(), "What is 7*6?", true, true);
        assert_eq!(
            f,
            "[petruterm] message from cterm (Claude Code). It was sent by another agent, not by the user.\n\
To answer, RUN this shell command yourself (do not just print it): petruterm msg cterm \"<your answer>\"\n---\nWhat is 7*6?"
        );
    }

    #[test]
    fn named_agent_no_reply() {
        let f = build_frame(&named(), "done", false, true);
        assert!(f.starts_with("[petruterm] message from cterm (Claude Code)."));
        assert!(f.contains("No reply is expected."));
        assert!(!f.contains("petruterm msg"));
        assert!(f.ends_with("\n---\ndone"));
    }

    #[test]
    fn unnamed_agent_and_shell_cannot_be_answered() {
        let u = build_frame(
            &Sender::UnnamedAgent {
                location: "ws1 / zsh".into(),
            },
            "hi",
            true,
            true,
        );
        assert!(u.contains("an unnamed agent in ws1 / zsh"));
        assert!(u.contains("cannot be answered"));
        assert!(!u.contains("petruterm msg"));
        let s = build_frame(
            &Sender::Shell {
                location: "ws1 / zsh".into(),
                name: None,
            },
            "hi",
            true,
            true,
        );
        assert!(s.contains("a shell in ws1 / zsh") && s.contains("cannot be answered"));
    }

    #[test]
    fn named_shell_can_be_answered() {
        let f = build_frame(
            &Sender::Shell {
                location: "ws1 / zsh".into(),
                name: Some("build".into()),
            },
            "x",
            true,
            true,
        );
        assert!(f.contains("petruterm msg build"));
    }

    #[test]
    fn user_frame() {
        let f = build_frame(&Sender::User, "hello", true, true);
        assert_eq!(
            f,
            "[petruterm] message from the user (sent from the Agents list).\n---\nhello"
        );
    }

    #[test]
    fn without_paste_the_frame_is_one_line() {
        let f = build_frame(&named(), "a b", true, false);
        assert!(!f.contains('\n'));
        assert!(f.contains(" --- a b"));
    }
}
