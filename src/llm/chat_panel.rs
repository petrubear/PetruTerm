use std::path::{Path, PathBuf};
use crate::llm::{ChatMessage, ChatRole};

/// Default panel width in terminal cell columns.
pub const PANEL_COLS: u16 = 55;
/// Max number of file attachment rows shown in the panel header section.
pub const MAX_FILE_ROWS: usize = 4;

/// Events sent from the tokio streaming task to the main thread.
pub enum AiEvent {
    Token(String),
    Done,
    Error(String),
    /// Agent called a tool. `done=false` = started, `done=true` = finished.
    ToolStatus { tool: String, path: String, done: bool },
}

#[derive(Debug, Clone, PartialEq)]
pub enum PanelState {
    Hidden,
    /// Panel open, waiting for user input.
    Idle,
    /// Waiting for the first token (request in-flight).
    Loading,
    /// Tokens are arriving.
    Streaming,
    Error(String),
}

pub struct ChatPanel {
    pub state: PanelState,
    /// Full conversation history (User + Assistant turns).
    pub messages: Vec<ChatMessage>,
    /// Current user input being typed.
    pub input: String,
    /// Accumulated tokens for the in-flight response.
    pub streaming_buf: String,
    /// Lines scrolled back from the bottom (0 = latest visible).
    pub scroll_offset: usize,
    /// Panel width in terminal cell columns.
    pub width_cols: u16,
    /// Marks panel content as changed — renderer uses this to skip re-shaping
    /// unchanged frames (avoids HarfBuzz calls on every redraw).
    pub dirty: bool,

    // ── File context ──────────────────────────────────────────────────────────
    /// Files attached as context; injected into LLM system message at query time.
    pub attached_files: Vec<PathBuf>,
    /// Cached char counts for each attached file (index-parallel to attached_files).
    attached_file_chars: Vec<usize>,

    // ── File picker ───────────────────────────────────────────────────────────
    /// Whether the file picker overlay is open.
    pub file_picker_open: bool,
    /// Fuzzy search query typed in the picker.
    pub file_picker_query: String,
    /// All scanned files available to pick from (relative paths under CWD).
    pub file_picker_items: Vec<PathBuf>,
    /// Index of the highlighted item in the filtered list.
    pub file_picker_cursor: usize,
}

impl ChatPanel {
    pub fn new() -> Self {
        Self {
            state: PanelState::Hidden,
            messages: Vec::new(),
            input: String::new(),
            streaming_buf: String::new(),
            scroll_offset: 0,
            width_cols: PANEL_COLS,
            dirty: true,
            attached_files: Vec::new(),
            attached_file_chars: Vec::new(),
            file_picker_open: false,
            file_picker_query: String::new(),
            file_picker_items: Vec::new(),
            file_picker_cursor: 0,
        }
    }

    pub fn is_visible(&self) -> bool {
        !matches!(self.state, PanelState::Hidden)
    }

    pub fn is_idle(&self) -> bool {
        matches!(self.state, PanelState::Idle)
    }

    pub fn is_streaming(&self) -> bool {
        matches!(self.state, PanelState::Streaming | PanelState::Loading)
    }

    pub fn open(&mut self) {
        if matches!(self.state, PanelState::Hidden) {
            self.state = PanelState::Idle;
            self.input.clear();
            self.dirty = true;
        }
    }

    pub fn close(&mut self) {
        self.state = PanelState::Hidden;
        self.dirty = true;
    }

    pub fn type_char(&mut self, c: char) {
        if self.is_idle() {
            self.input.push(c);
            self.dirty = true;
        }
    }

    pub fn backspace(&mut self) {
        if self.is_idle() {
            self.input.pop();
            self.dirty = true;
        }
    }

    /// Append a file path to the current input (from drag-and-drop).
    pub fn append_path(&mut self, path: &str) {
        if self.is_idle() {
            if !self.input.is_empty() && !self.input.ends_with(' ') {
                self.input.push(' ');
            }
            self.input.push_str(path);
            self.dirty = true;
        }
    }

    /// Push the current input as a user message and return its content.
    /// Returns `None` if the input is empty.
    pub fn submit_input(&mut self) -> Option<String> {
        let content = self.input.trim().to_string();
        if content.is_empty() {
            return None;
        }
        self.messages.push(ChatMessage::user(&content));
        self.input.clear();
        self.state = PanelState::Loading;
        self.streaming_buf.clear();
        self.dirty = true;
        Some(content)
    }

    pub fn append_token(&mut self, tok: &str) {
        self.state = PanelState::Streaming;
        self.streaming_buf.push_str(tok);
        self.dirty = true;
    }

    /// Show a tool-call status line in the streaming buffer.
    /// `done=false` replaces the last status line; `done=true` marks it finished.
    pub fn set_tool_status(&mut self, tool: &str, path: &str, done: bool) {
        self.state = PanelState::Streaming;
        let icon = if done { "✓" } else { "⟳" };
        let line = format!("{icon} {tool}({path})\n");
        // Replace the last status line if it starts with ⟳ (in-progress).
        if !done {
            if self.streaming_buf.ends_with('\n') {
                let prev = self.streaming_buf.trim_end_matches('\n');
                if prev.contains('⟳') {
                    let last_nl = prev.rfind('\n').map(|i| i + 1).unwrap_or(0);
                    self.streaming_buf.truncate(last_nl);
                }
            }
        }
        self.streaming_buf.push_str(&line);
        self.dirty = true;
    }

    pub fn mark_done(&mut self) {
        let response = self.streaming_buf.trim().to_string();
        if !response.is_empty() {
            self.messages.push(ChatMessage::assistant(response));
        }
        self.streaming_buf.clear();
        self.state = PanelState::Idle;
        self.scroll_offset = 0; // snap to bottom
        self.dirty = true;
    }

    pub fn mark_error(&mut self, msg: String) {
        self.streaming_buf.clear();
        self.state = PanelState::Error(msg);
        self.dirty = true;
    }

    pub fn dismiss_error(&mut self) {
        if matches!(self.state, PanelState::Error(_)) {
            self.state = PanelState::Idle;
            self.dirty = true;
        }
    }

    /// Scroll history toward older messages (up).
    pub fn scroll_up(&mut self, lines: usize) {
        self.scroll_offset = self.scroll_offset.saturating_add(lines);
        self.dirty = true;
    }

    /// Scroll history toward newer messages (down).
    pub fn scroll_down(&mut self, lines: usize) {
        self.scroll_offset = self.scroll_offset.saturating_sub(lines);
        self.dirty = true;
    }

    /// Returns the last assistant message stripped of markdown fences,
    /// suitable for writing directly to the PTY.
    pub fn last_assistant_command(&self) -> Option<String> {
        let msg = self.messages.iter().rev()
            .find(|m| matches!(m.role, ChatRole::Assistant))?;
        let s = msg.content.trim();
        if s.is_empty() {
            return None;
        }
        let cmd = if s.starts_with("```") {
            s.splitn(3, '\n')
                .nth(1)
                .unwrap_or(s)
                .trim_end_matches('`')
                .trim()
        } else {
            s
        };
        if cmd.is_empty() { None } else { Some(cmd.to_string()) }
    }

    // ── File context ─────────────────────────────────────────────────────────

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
        if self.attached_files.contains(&path) { return; }
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

    /// Open the file picker, scanning `cwd` for pickable files.
    pub fn open_file_picker(&mut self, cwd: &Path) {
        self.file_picker_items = scan_files(cwd, 3);
        self.file_picker_items.sort();
        self.file_picker_query.clear();
        self.file_picker_cursor = 0;
        self.file_picker_open = true;
        self.dirty = true;
    }

    pub fn close_file_picker(&mut self) {
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
    pub fn filtered_picker_items(&self) -> Vec<PathBuf> {
        if self.file_picker_query.is_empty() {
            return self.file_picker_items.clone();
        }
        use fuzzy_matcher::FuzzyMatcher;
        use fuzzy_matcher::skim::SkimMatcherV2;
        let matcher = SkimMatcherV2::default();
        let query = &self.file_picker_query;
        let mut scored: Vec<(i64, PathBuf)> = self.file_picker_items.iter()
            .filter_map(|p| {
                matcher.fuzzy_match(&p.to_string_lossy(), query).map(|s| (s, p.clone()))
            })
            .collect();
        scored.sort_by(|a, b| b.0.cmp(&a.0));
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
    if depth == 0 { return; }
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if name_str.starts_with('.') { continue; }
        if matches!(name_str.as_ref(), "target" | "node_modules" | "dist" | "build" | ".git") {
            continue;
        }
        if path.is_dir() {
            scan_dir(base, &path, depth - 1, out);
        } else {
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
            if matches!(ext,
                "rs" | "toml" | "lua" | "md" | "json" | "yaml" | "yml" |
                "txt" | "sh" | "zsh" | "py" | "js" | "ts" | "go" |
                "c" | "cpp" | "h" | "hpp" | "lock"
            ) || ext.is_empty() {
                if let Ok(rel) = path.strip_prefix(base) {
                    out.push(rel.to_path_buf());
                }
            }
        }
    }
}

// ── Text utilities ────────────────────────────────────────────────────────────

/// Word-wrap `text` to at most `width` characters per line.
/// Wrap an input string by characters (not words) into lines of `width` chars.
/// Used for the input field so the user can always see what they're typing.
pub fn wrap_input(text: &str, width: usize) -> Vec<String> {
    if width == 0 {
        return vec![text.to_string()];
    }
    let chars: Vec<char> = text.chars().collect();
    if chars.is_empty() {
        return vec![String::new()];
    }
    chars.chunks(width).map(|c| c.iter().collect()).collect()
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
    s.chars()
        .collect::<Vec<_>>()
        .chunks(width)
        .map(|c| c.iter().collect())
        .collect()
}

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
