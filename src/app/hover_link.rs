#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HoverLinkKind {
    Url,
    Path,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HoverLink {
    pub row: usize,
    pub col_start: usize,
    pub col_end: usize,
    pub kind: HoverLinkKind,
    pub text: String,
}

/// Scan `row_text` for a URL or file path containing terminal column `cursor_col`.
/// Returns `(col_start, col_end, kind, text)` if a link is found.
pub fn scan_link_at(
    row_text: &str,
    cursor_col: usize,
) -> Option<(usize, usize, HoverLinkKind, String)> {
    let chars: Vec<char> = row_text.chars().collect();
    if cursor_col >= chars.len() {
        return None;
    }

    // Find token start (go left until a boundary or string start).
    let mut start = cursor_col;
    while start > 0 && !is_boundary(chars[start - 1]) {
        start -= 1;
    }

    // Find token end (go right until a boundary or string end).
    let mut end = cursor_col + 1;
    while end < chars.len() && !is_boundary(chars[end]) {
        end += 1;
    }

    if start >= end {
        return None;
    }

    let token: String = chars[start..end].iter().collect();

    // Strip trailing sentence punctuation that is not part of the link.
    let trimmed = token.trim_end_matches(['.', ',', ';']);
    if trimmed.len() < 2 {
        return None;
    }
    let col_end = start + trimmed.chars().count();

    // URL: http:// or https://
    if trimmed.starts_with("https://") || trimmed.starts_with("http://") {
        if trimmed.len() > 8 {
            return Some((start, col_end, HoverLinkKind::Url, trimmed.to_string()));
        }
        return None;
    }

    // Absolute path: starts with / (not //)
    if trimmed.starts_with('/') && !trimmed.starts_with("//") {
        return Some((start, col_end, HoverLinkKind::Path, trimmed.to_string()));
    }

    // Relative path: ./ or ../
    if trimmed.starts_with("./") || trimmed.starts_with("../") {
        return Some((start, col_end, HoverLinkKind::Path, trimmed.to_string()));
    }

    // Stack trace: relative path with file extension and line number (e.g. src/app.rs:123:5)
    // Conditions: has '/', has '.', has ':' followed by a digit.
    if trimmed.contains('/') && trimmed.contains('.') {
        if let Some(colon) = trimmed.find(':') {
            if trimmed[colon + 1..].starts_with(|c: char| c.is_ascii_digit()) {
                return Some((start, col_end, HoverLinkKind::Path, trimmed.to_string()));
            }
        }
    }

    None
}

/// Strip trailing `:NNN` line/col suffixes from a path before passing to `open`.
/// `src/app.rs:123:45` → `src/app.rs`
pub fn path_for_open(text: &str) -> &str {
    let mut s = text;
    while let Some(pos) = s.rfind(':') {
        let suffix = &s[pos + 1..];
        if !suffix.is_empty() && suffix.chars().all(|c| c.is_ascii_digit()) {
            s = &s[..pos];
        } else {
            break;
        }
    }
    s
}

fn is_boundary(c: char) -> bool {
    matches!(
        c,
        ' ' | '\t' | '"' | '\'' | '`' | '(' | ')' | '[' | ']' | '{' | '}' | '<' | '>'
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_url() {
        let row = "see https://example.com/path for details";
        let (s, e, k, t) = scan_link_at(row, 10).unwrap();
        assert_eq!(k, HoverLinkKind::Url);
        assert_eq!(t, "https://example.com/path");
        assert_eq!(s, 4);
        assert_eq!(e, 28);
    }

    #[test]
    fn detects_absolute_path() {
        let row = "error in /Users/foo/bar.rs line 5";
        let (_, _, k, t) = scan_link_at(row, 12).unwrap();
        assert_eq!(k, HoverLinkKind::Path);
        assert_eq!(t, "/Users/foo/bar.rs");
    }

    #[test]
    fn detects_stack_trace() {
        let row = "   --> src/app/mod.rs:1234:56";
        let (_, _, k, t) = scan_link_at(row, 10).unwrap();
        assert_eq!(k, HoverLinkKind::Path);
        assert!(t.contains("src/app/mod.rs"));
    }

    #[test]
    fn no_match_on_plain_word() {
        let row = "hello world";
        assert!(scan_link_at(row, 3).is_none());
    }

    #[test]
    fn path_for_open_strips_line_col() {
        assert_eq!(path_for_open("src/app.rs:123:45"), "src/app.rs");
        assert_eq!(path_for_open("/Users/foo/bar.rs:10"), "/Users/foo/bar.rs");
        assert_eq!(path_for_open("https://example.com"), "https://example.com");
    }
}
