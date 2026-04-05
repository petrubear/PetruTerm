use crate::llm::{ChatMessage, ChatRole};

/// Default panel width in terminal cell columns.
pub const PANEL_COLS: u16 = 55;

/// Events sent from the tokio streaming task to the main thread.
pub enum AiEvent {
    Token(String),
    Done,
    Error(String),
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
