use std::collections::{HashMap, HashSet};
use std::ops::Range;
use std::sync::{Arc, Mutex};

// ── History index (I-3) ──────────────────────────────────────────────────────

/// Shell command history, most-recent-first.
pub struct HistoryIndex {
    entries: Vec<String>,
}

impl HistoryIndex {
    /// Load from ~/.zsh_history or ~/.bash_history (whichever is found first).
    pub fn load() -> Self {
        let mut entries = Self::load_zsh()
            .or_else(Self::load_bash)
            .unwrap_or_default();
        entries.reverse(); // chronological → most-recent-first
        Self { entries }
    }

    fn load_zsh() -> Option<Vec<String>> {
        let path = dirs::home_dir()?.join(".zsh_history");
        let raw = std::fs::read(path).ok()?;
        let text = String::from_utf8_lossy(&raw);
        let mut result = Vec::new();
        for line in text.lines() {
            // Extended format: ": <timestamp>:0;<cmd>" — strip the prefix.
            let cmd = if let Some(rest) = line.strip_prefix(": ") {
                rest.find(';').map(|i| &rest[i + 1..]).unwrap_or(line)
            } else {
                line
            };
            let cmd = cmd.trim();
            if !cmd.is_empty() {
                result.push(cmd.to_string());
            }
        }
        Some(result)
    }

    fn load_bash() -> Option<Vec<String>> {
        let path = dirs::home_dir()?.join(".bash_history");
        let content = std::fs::read_to_string(path).ok()?;
        Some(
            content
                .lines()
                .map(str::trim)
                .filter(|l| !l.is_empty())
                .map(String::from)
                .collect(),
        )
    }

    /// Return the completion suffix for `prefix` (i.e. `entry[prefix.len()..]` for the
    /// most-recent entry that starts with `prefix` and is longer than it).
    pub fn find_suffix<'a>(&'a self, prefix: &str) -> Option<&'a str> {
        if prefix.is_empty() {
            return None;
        }
        self.entries
            .iter()
            .find(|e| e.len() > prefix.len() && e.starts_with(prefix))
            .map(|e| &e[prefix.len()..])
    }
}

// ── End history index ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenKind {
    Command,
    Flag,
    String,
    Pipe,
    Redirect,
    Arg,
}

#[derive(Debug, Clone)]
pub struct Token {
    pub kind: TokenKind,
    pub range: Range<usize>,
}

/// Tokenize a shell command line into typed spans (byte ranges into `input`).
/// No regex — hand-rolled state machine, good enough for common cases.
pub fn tokenize_command(input: &str) -> Vec<Token> {
    let b = input.as_bytes();
    let n = b.len();
    let mut tokens = Vec::new();
    let mut i = 0;
    let mut first_word = true; // next unquoted word is Command

    while i < n {
        // Skip whitespace
        if b[i] == b' ' || b[i] == b'\t' {
            i += 1;
            continue;
        }

        // Pipe / && / || / ;
        if b[i] == b'|' || b[i] == b'&' || b[i] == b';' {
            let start = i;
            i += 1;
            if i < n && (b[i] == b'|' || b[i] == b'&') {
                i += 1;
            }
            tokens.push(Token { kind: TokenKind::Pipe, range: start..i });
            first_word = true;
            continue;
        }

        // Redirect: > >> <
        if b[i] == b'>' || b[i] == b'<' {
            let start = i;
            i += 1;
            if i < n && b[i] == b'>' {
                i += 1;
            }
            tokens.push(Token { kind: TokenKind::Redirect, range: start..i });
            first_word = false;
            continue;
        }

        // Quoted string
        if b[i] == b'"' || b[i] == b'\'' {
            let quote = b[i];
            let start = i;
            i += 1;
            while i < n && b[i] != quote {
                if b[i] == b'\\' {
                    i += 1;
                }
                i += 1;
            }
            if i < n {
                i += 1; // closing quote
            }
            tokens.push(Token { kind: TokenKind::String, range: start..i });
            first_word = false;
            continue;
        }

        // Flag: starts with '-' and not the first word
        if b[i] == b'-' && !first_word {
            let start = i;
            i += 1;
            if i < n && b[i] == b'-' {
                i += 1;
            }
            while i < n && !is_word_break(b[i]) {
                i += 1;
            }
            tokens.push(Token { kind: TokenKind::Flag, range: start..i });
            continue;
        }

        // Word: Command (first) or Arg (subsequent)
        let start = i;
        while i < n && !is_word_break(b[i]) && b[i] != b'"' && b[i] != b'\'' {
            i += 1;
        }
        let kind = if first_word {
            first_word = false;
            TokenKind::Command
        } else {
            TokenKind::Arg
        };
        tokens.push(Token { kind, range: start..i });
    }

    tokens
}

fn is_word_break(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'|' | b'&' | b'>' | b'<' | b';')
}

/// Per-column (relative to cmd_start_col) fg color override for the active input row.
/// `None` at a column means "keep original fg".
pub type SyntaxFg = Vec<Option<[f32; 4]>>;

/// Dracula-palette colors used for syntax spans.
/// These are RGBA [0,1] values.
const CMD_VALID:   [f32; 4] = [0.314, 0.980, 0.482, 1.0]; // #50fa7b green
const CMD_ERROR:   [f32; 4] = [1.000, 0.333, 0.333, 1.0]; // #ff5555 red
const FLAG_COLOR:  [f32; 4] = [0.545, 0.914, 0.992, 1.0]; // #8be9fd cyan
const STR_COLOR:   [f32; 4] = [0.945, 0.980, 0.549, 1.0]; // #f1fa8c yellow
const PIPE_COLOR:  [f32; 4] = [1.000, 0.722, 0.424, 1.0]; // #ffb86c orange

/// Compute per-column fg overrides for `buf` given resolved command validity.
/// `cmd_valid`: `None` if the command name hasn't been resolved yet (no override).
pub fn build_syntax_fg(buf: &str, cmd_valid: Option<bool>) -> SyntaxFg {
    let char_count = buf.chars().count();
    let mut fg: SyntaxFg = vec![None; char_count];

    let tokens = tokenize_command(buf);
    for token in tokens {
        let color: Option<[f32; 4]> = match token.kind {
            TokenKind::Command => match cmd_valid {
                Some(true) => Some(CMD_VALID),
                Some(false) => Some(CMD_ERROR),
                None => None,
            },
            TokenKind::Flag => Some(FLAG_COLOR),
            TokenKind::String => Some(STR_COLOR),
            TokenKind::Pipe | TokenKind::Redirect => Some(PIPE_COLOR),
            TokenKind::Arg => None,
        };
        let Some(color) = color else { continue };
        // Map byte range to char column range
        let col_start = buf[..token.range.start].chars().count();
        let col_end = col_start + buf[token.range.clone()].chars().count();
        for col in col_start..col_end.min(char_count) {
            fg[col] = Some(color);
        }
    }
    fg
}

/// Non-blocking command resolver: checks if a command name is in $PATH.
/// Results are cached. Background threads fill the cache on misses.
#[derive(Clone)]
pub struct CommandResolver {
    // None = pending, Some(true) = found, Some(false) = not found
    cache: Arc<Mutex<HashMap<String, Option<bool>>>>,
    pending: Arc<Mutex<HashSet<String>>>,
}

impl CommandResolver {
    pub fn new() -> Self {
        Self {
            cache: Arc::new(Mutex::new(HashMap::new())),
            pending: Arc::new(Mutex::new(HashSet::new())),
        }
    }

    /// Returns cached result: `Some(true)` = valid, `Some(false)` = not found,
    /// `None` = not yet resolved.
    pub fn resolve(&self, cmd: &str) -> Option<bool> {
        self.cache.lock().ok()?.get(cmd).copied().flatten()
    }

    /// Kick off a background lookup if `cmd` is not cached or pending.
    pub fn schedule(&self, cmd: &str) {
        // Skip empty, absolute paths, and relative paths (./foo)
        if cmd.is_empty() || cmd.starts_with('/') || cmd.starts_with('.') {
            return;
        }
        {
            let Ok(cache) = self.cache.lock() else { return };
            if cache.contains_key(cmd) {
                return; // already cached
            }
        }
        {
            let Ok(mut pending) = self.pending.lock() else { return };
            if !pending.insert(cmd.to_string()) {
                return; // already in flight
            }
        }
        let cmd_owned = cmd.to_string();
        let cache = Arc::clone(&self.cache);
        let pending = Arc::clone(&self.pending);
        std::thread::spawn(move || {
            let found = is_in_path(&cmd_owned);
            if let Ok(mut c) = cache.lock() {
                c.insert(cmd_owned.clone(), Some(found));
            }
            if let Ok(mut p) = pending.lock() {
                p.remove(&cmd_owned);
            }
        });
    }
}

fn is_in_path(cmd: &str) -> bool {
    let Ok(path_var) = std::env::var("PATH") else {
        return false;
    };
    for dir in path_var.split(':') {
        if dir.is_empty() {
            continue;
        }
        if std::path::Path::new(dir).join(cmd).exists() {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(input: &str) -> Vec<TokenKind> {
        tokenize_command(input).into_iter().map(|t| t.kind).collect()
    }

    #[test]
    fn simple_command() {
        assert_eq!(kinds("ls"), vec![TokenKind::Command]);
    }

    #[test]
    fn command_with_flag() {
        assert_eq!(kinds("ls -la"), vec![TokenKind::Command, TokenKind::Flag]);
    }

    #[test]
    fn command_arg_flag() {
        assert_eq!(
            kinds("git commit -m"),
            vec![TokenKind::Command, TokenKind::Arg, TokenKind::Flag]
        );
    }

    #[test]
    fn pipe_resets_command() {
        assert_eq!(
            kinds("cat file | grep foo"),
            vec![
                TokenKind::Command,
                TokenKind::Arg,
                TokenKind::Pipe,
                TokenKind::Command,
                TokenKind::Arg,
            ]
        );
    }

    #[test]
    fn quoted_string() {
        assert_eq!(
            kinds(r#"echo "hello world""#),
            vec![TokenKind::Command, TokenKind::String]
        );
    }

    #[test]
    fn redirect() {
        assert_eq!(
            kinds("ls > out.txt"),
            vec![TokenKind::Command, TokenKind::Redirect, TokenKind::Arg]
        );
    }

    #[test]
    fn double_dash_flag() {
        assert_eq!(
            kinds("cargo build --release"),
            vec![TokenKind::Command, TokenKind::Arg, TokenKind::Flag]
        );
    }

    #[test]
    fn build_syntax_fg_length_matches_char_count() {
        let buf = "git commit";
        let fg = build_syntax_fg(buf, Some(true));
        assert_eq!(fg.len(), buf.chars().count());
    }

    #[test]
    fn command_valid_colors_first_word() {
        let fg = build_syntax_fg("ls -la", Some(true));
        assert_eq!(fg[0], Some(CMD_VALID)); // 'l'
        assert_eq!(fg[1], Some(CMD_VALID)); // 's'
        assert_eq!(fg[2], None);            // ' '
        assert_eq!(fg[3], Some(FLAG_COLOR)); // '-'
    }

    #[test]
    fn command_error_colors_first_word() {
        let fg = build_syntax_fg("notacmd --flag", Some(false));
        assert_eq!(fg[0], Some(CMD_ERROR));
    }

    #[test]
    fn is_in_path_finds_ls() {
        // ls is always in PATH on Unix
        assert!(is_in_path("ls"));
    }

    #[test]
    fn is_in_path_rejects_fake() {
        assert!(!is_in_path("__no_such_cmd_xyz__"));
    }
}
