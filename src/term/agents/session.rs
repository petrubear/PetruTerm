//! Session id found on an agent's command line (`--resume <id>` and friends).

use super::names::sanitize_session;

const FLAGS: [&str; 4] = ["--resume", "-r", "--resume-id", "--session-id"];

/// The session id passed on the command line, if any. Only the id token is returned; the
/// rest of `args` is never kept.
pub fn session_from_args(args: &str) -> Option<String> {
    let mut tokens = args.split_whitespace().skip(1);
    while let Some(tok) = tokens.next() {
        for flag in FLAGS {
            if let Some(value) = tok.strip_prefix(flag).and_then(|r| r.strip_prefix('=')) {
                return sanitize_session(value);
            }
        }
        if FLAGS.contains(&tok) {
            return tokens.next().and_then(sanitize_session);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    const ID: &str = "3f6c0a52-91d4-4b1e-8c3a-0d2f7e5b9a11";

    #[test]
    fn finds_ids_after_known_flags() {
        assert_eq!(
            session_from_args(&format!("claude --resume {ID}")).as_deref(),
            Some(ID)
        );
        assert_eq!(
            session_from_args(&format!("claude -r {ID} --x")).as_deref(),
            Some(ID)
        );
        assert_eq!(
            session_from_args(&format!("kiro-cli chat --resume-id {ID}")).as_deref(),
            Some(ID)
        );
        assert_eq!(
            session_from_args(&format!("claude --session-id={ID}")).as_deref(),
            Some(ID)
        );
    }

    #[test]
    fn ignores_missing_or_unsafe_values() {
        assert_eq!(session_from_args("claude --resume"), None);
        assert_eq!(session_from_args("claude --resume ../../etc/passwd"), None);
        assert_eq!(session_from_args("claude"), None);
        assert_eq!(session_from_args("claude --continue"), None);
    }

    #[test]
    fn argv0_is_not_treated_as_a_flag() {
        assert_eq!(session_from_args("-r"), None);
    }
}
