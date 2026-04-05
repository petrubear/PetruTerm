/// Number of terminal rows reserved for the inline AI overlay.
pub const AI_BLOCK_ROWS: usize = 4;

/// State machine for the inline AI overlay.
#[derive(Debug, Clone, PartialEq)]
pub enum AiState {
    Hidden,
    Typing,
    Loading,
    Streaming,
    Done,
    Error(String),
}

pub struct AiBlock {
    pub state: AiState,
    /// User's typed natural-language query.
    pub query: String,
    /// Accumulated LLM response tokens.
    pub response: String,
    /// Marks content as changed so the renderer can skip unnecessary reshaping.
    pub dirty: bool,
}

impl AiBlock {
    pub fn new() -> Self {
        Self {
            state: AiState::Hidden,
            query: String::new(),
            response: String::new(),
            dirty: true,
        }
    }

    pub fn is_visible(&self) -> bool { !matches!(self.state, AiState::Hidden) }
    pub fn is_typing(&self)  -> bool { matches!(self.state, AiState::Typing) }
    pub fn is_done(&self)    -> bool { matches!(self.state, AiState::Done) }

    pub fn open(&mut self) {
        self.state = AiState::Typing;
        self.query.clear();
        self.response.clear();
        self.dirty = true;
    }

    pub fn close(&mut self) {
        self.state = AiState::Hidden;
        self.dirty = true;
    }

    pub fn type_char(&mut self, c: char) {
        if self.is_typing() { self.query.push(c); self.dirty = true; }
    }

    pub fn backspace(&mut self) {
        if self.is_typing() { self.query.pop(); self.dirty = true; }
    }

    pub fn set_loading(&mut self) {
        self.state = AiState::Loading;
        self.response.clear();
        self.dirty = true;
    }

    pub fn append_token(&mut self, tok: &str) {
        self.state = AiState::Streaming;
        self.response.push_str(tok);
        self.dirty = true;
    }

    pub fn mark_done(&mut self) {
        self.state = AiState::Done;
        self.dirty = true;
    }

    pub fn mark_error(&mut self, msg: String) {
        self.state = AiState::Error(msg);
        self.dirty = true;
    }

    /// Returns the shell command ready to write to the PTY.
    /// Strips markdown code fences the model may have emitted.
    pub fn command_to_run(&self) -> Option<String> {
        if !matches!(self.state, AiState::Done | AiState::Streaming) { return None; }
        let s = self.response.trim();
        if s.is_empty() { return None; }
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
