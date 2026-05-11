use super::ChatPanel;
use fuzzy_matcher::FuzzyMatcher;
use std::path::{Path, PathBuf};

// ── ChatPanel: file context ───────────────────────────────────────────────────

impl ChatPanel {
    /// Auto-attach `AGENTS.md` from `cwd` if it exists (idempotent).
    pub fn init_default_files(&mut self, cwd: &Path) {
        let agents = cwd.join("AGENTS.md");
        if agents.exists() {
            self.attach_file(agents);
        }
    }

    /// Attach a file, reading its char count once for token estimation.
    /// No-op if already attached.
    pub fn attach_file(&mut self, path: PathBuf) {
        if self.attached_files.contains(&path) {
            return;
        }
        let chars = std::fs::read_to_string(&path).map(|s| s.len()).unwrap_or(0);
        self.attached_files.push(path);
        self.attached_file_chars.push(chars);
        self.dirty = true;
    }

    /// Remove an attached file by index.
    pub fn detach_file(&mut self, idx: usize) {
        if idx < self.attached_files.len() {
            self.attached_files.remove(idx);
            self.attached_file_chars.remove(idx);
            self.dirty = true;
        }
    }

    /// Estimated token count (chars / 4) across messages + attached files.
    pub fn estimated_tokens(&self) -> usize {
        let msg_chars: usize = self.messages.iter().map(|m| m.content.len()).sum::<usize>()
            + self.input.len()
            + self.streaming_buf.len();
        let file_chars: usize = self.attached_file_chars.iter().sum();
        (msg_chars + file_chars) / 4
    }

    // ── File picker ───────────────────────────────────────────────────────────

    pub fn close_file_picker(&mut self) {
        self.file_picker_items.clear();
        self.file_picker_items.shrink_to_fit();
        self.file_picker_open = false;
        self.dirty = true;
    }

    pub fn picker_type_char(&mut self, c: char) {
        self.file_picker_query.push(c);
        self.file_picker_cursor = 0;
        self.dirty = true;
    }

    pub fn picker_backspace(&mut self) {
        self.file_picker_query.pop();
        self.file_picker_cursor = 0;
        self.dirty = true;
    }

    pub fn picker_move_up(&mut self) {
        self.file_picker_cursor = self.file_picker_cursor.saturating_sub(1);
        self.dirty = true;
    }

    pub fn picker_move_down(&mut self, filtered_len: usize) {
        if self.file_picker_cursor + 1 < filtered_len {
            self.file_picker_cursor += 1;
        }
        self.dirty = true;
    }

    /// Toggle attach/detach for the currently highlighted picker item, given the
    /// pre-computed filtered list (avoids re-running the fuzzy match here).
    pub fn picker_confirm(&mut self, cwd: &Path, filtered_items: &[PathBuf]) {
        if let Some(rel) = filtered_items.get(self.file_picker_cursor) {
            let abs = cwd.join(rel);
            if let Some(idx) = self.attached_files.iter().position(|p| p == &abs) {
                self.detach_file(idx);
            } else {
                self.attach_file(abs);
            }
        }
    }

    /// Returns filtered file picker items matching the current query (fuzzy).
    /// Returns references — callers must not outlive `self` (TD-PERF-33).
    pub fn filtered_picker_items(&self) -> Vec<&PathBuf> {
        if self.file_picker_query.is_empty() {
            return self.file_picker_items.iter().collect();
        }
        let query = &self.file_picker_query;
        let mut scored: Vec<(i64, &PathBuf)> = self
            .file_picker_items
            .iter()
            .filter_map(|p| {
                self.matcher
                    .fuzzy_match(&p.to_string_lossy(), query)
                    .map(|s| (s, p))
            })
            .collect();
        scored.sort_by_key(|b| std::cmp::Reverse(b.0));
        scored.into_iter().map(|(_, p)| p).collect()
    }
}

// ── File scanning ─────────────────────────────────────────────────────────────

/// Recursively collect source files under `dir` up to `max_depth`, returning
/// paths relative to `dir`. Skips hidden entries and common non-source dirs.
pub fn scan_files(dir: &Path, max_depth: usize) -> Vec<PathBuf> {
    let mut result = Vec::new();
    scan_dir(dir, dir, max_depth, &mut result);
    result
}

fn scan_dir(base: &Path, dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    if depth == 0 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if name_str.starts_with('.') {
            continue;
        }
        if matches!(
            name_str.as_ref(),
            "target" | "node_modules" | "dist" | "build" | ".git"
        ) {
            continue;
        }
        if path.is_dir() {
            scan_dir(base, &path, depth - 1, out);
        } else {
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
            if matches!(
                ext,
                "rs" | "toml"
                    | "lua"
                    | "md"
                    | "json"
                    | "yaml"
                    | "yml"
                    | "txt"
                    | "sh"
                    | "zsh"
                    | "py"
                    | "js"
                    | "ts"
                    | "go"
                    | "c"
                    | "cpp"
                    | "h"
                    | "hpp"
                    | "lock"
            ) || ext.is_empty()
            {
                if let Ok(rel) = path.strip_prefix(base) {
                    out.push(rel.to_path_buf());
                }
            }
        }
    }
}

// ── Text utilities ────────────────────────────────────────────────────────────

/// Wrap an input string into display lines of at most `width` chars.
/// Explicit `\n` characters (from Shift+Enter) create hard line breaks;
/// each segment is then soft-wrapped by character count if it overflows.
pub fn wrap_input(text: &str, width: usize) -> Vec<String> {
    if width == 0 {
        return vec![text.to_string()];
    }
    let mut result = Vec::new();
    for segment in text.split('\n') {
        let chars: Vec<char> = segment.chars().collect();
        if chars.is_empty() {
            result.push(String::new());
        } else {
            for chunk in chars.chunks(width) {
                result.push(chunk.iter().collect());
            }
        }
    }
    if result.is_empty() {
        result.push(String::new());
    }
    result
}

pub fn word_wrap(text: &str, width: usize) -> Vec<String> {
    if width == 0 {
        return vec![text.to_string()];
    }
    let mut lines = Vec::new();
    for paragraph in text.lines() {
        if paragraph.is_empty() {
            lines.push(String::new());
            continue;
        }
        let mut current = String::new();
        for word in paragraph.split_whitespace() {
            if current.is_empty() {
                if word.len() > width {
                    for chunk in char_chunks(word, width) {
                        lines.push(chunk);
                    }
                } else {
                    current.push_str(word);
                }
            } else if current.len() + 1 + word.len() <= width {
                current.push(' ');
                current.push_str(word);
            } else {
                lines.push(std::mem::take(&mut current));
                if word.len() > width {
                    for chunk in char_chunks(word, width) {
                        lines.push(chunk);
                    }
                } else {
                    current.push_str(word);
                }
            }
        }
        if !current.is_empty() {
            lines.push(current);
        }
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

fn char_chunks(s: &str, width: usize) -> Vec<String> {
    let mut result = Vec::new();
    let mut chunk = String::with_capacity(width);
    let mut count = 0usize;
    for ch in s.chars() {
        chunk.push(ch);
        count += 1;
        if count == width {
            result.push(std::mem::take(&mut chunk));
            chunk = String::with_capacity(width);
            count = 0;
        }
    }
    if !chunk.is_empty() {
        result.push(chunk);
    }
    result
}

#[allow(dead_code)]
/// Build a separator line with `title` centered: `── title ──────`.
pub fn titled_separator(title: &str, width: usize) -> String {
    let inner = format!(" {} ", title);
    let inner_len = inner.chars().count();
    if inner_len >= width {
        return "─".repeat(width);
    }
    let left = (width - inner_len) / 2;
    let right = width - inner_len - left;
    format!("{}{}{}", "─".repeat(left), inner, "─".repeat(right))
}
