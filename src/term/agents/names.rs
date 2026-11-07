//! Validation helpers for custom agent names and session identifiers.

use std::fmt;

/// Longest accepted custom agent name.
pub const MAX_NAME_LEN: usize = 32;
/// Longest accepted session identifier.
pub const MAX_SESSION_LEN: usize = 64;
/// Characters of a session id shown in the list.
pub const SHORT_SESSION_LEN: usize = 8;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NameError {
    Empty,
    TooLong,
    BadChar(char),
    /// Another agent already has this name; carries that agent's location for the message.
    Duplicate(String),
}

impl fmt::Display for NameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NameError::Empty => write!(f, "Name cannot be empty"),
            NameError::TooLong => write!(f, "Max {MAX_NAME_LEN} characters"),
            NameError::BadChar(_) => write!(f, "Only letters, digits, - _ ."),
            NameError::Duplicate(w) => write!(f, "Name already used by {w}"),
        }
    }
}

/// Trim and validate a custom name: 1-32 chars from `[A-Za-z0-9._-]`.
pub fn validate_name(input: &str) -> Result<String, NameError> {
    let name = input.trim();
    if name.is_empty() {
        return Err(NameError::Empty);
    }
    if name.chars().count() > MAX_NAME_LEN {
        return Err(NameError::TooLong);
    }
    if let Some(bad) = name
        .chars()
        .find(|c| !(c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.')))
    {
        return Err(NameError::BadChar(bad));
    }
    Ok(name.to_string())
}

/// A session id is kept only if it is 1-64 chars of `[A-Za-z0-9_-]`.
pub fn sanitize_session(input: &str) -> Option<String> {
    let id = input.trim();
    let ok = !id.is_empty()
        && id.len() <= MAX_SESSION_LEN
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    ok.then(|| id.to_string())
}

/// First characters of a session id, for display.
pub fn short_session(id: &str) -> &str {
    match id.char_indices().nth(SHORT_SESSION_LEN) {
        Some((i, _)) => &id[..i],
        None => id,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_names_are_trimmed_and_kept() {
        assert_eq!(validate_name("  reviewer ").unwrap(), "reviewer");
        assert_eq!(validate_name("backend-1.a_b").unwrap(), "backend-1.a_b");
        assert!(validate_name(&"a".repeat(32)).is_ok());
    }

    #[test]
    fn invalid_names_are_refused() {
        assert_eq!(validate_name(""), Err(NameError::Empty));
        assert_eq!(validate_name("   "), Err(NameError::Empty));
        assert_eq!(validate_name(&"a".repeat(33)), Err(NameError::TooLong));
        assert_eq!(validate_name("my agent"), Err(NameError::BadChar(' ')));
        assert_eq!(validate_name("a/b"), Err(NameError::BadChar('/')));
        assert_eq!(
            validate_name("caf\u{e9}"),
            Err(NameError::BadChar('\u{e9}'))
        );
    }

    #[test]
    fn session_ids_are_sanitized() {
        let uuid = "3f6c0a52-91d4-4b1e-8c3a-0d2f7e5b9a11";
        assert_eq!(sanitize_session(uuid).as_deref(), Some(uuid));
        assert_eq!(sanitize_session("sess_ab12").as_deref(), Some("sess_ab12"));
        assert_eq!(sanitize_session(""), None);
        assert_eq!(sanitize_session("../../x"), None);
        assert_eq!(sanitize_session(&"a".repeat(65)), None);
    }

    #[test]
    fn short_session_takes_eight_chars() {
        assert_eq!(short_session("3f6c0a52-91d4"), "3f6c0a52");
        assert_eq!(short_session("abc"), "abc");
    }
}
