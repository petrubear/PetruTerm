#![allow(dead_code)]

/// Number of terminal rows reserved for the AI block overlay.
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
}

/// Events sent from the tokio streaming task to the main thread.
pub enum AiEvent {
    Token(String),
    Done,
    Error(String),
}

impl AiBlock {
    pub fn new() -> Self {
        Self { state: AiState::Hidden, query: String::new(), response: String::new() }
    }

    pub fn is_visible(&self) -> bool { !matches!(self.state, AiState::Hidden) }
    pub fn is_typing(&self) -> bool  { matches!(self.state, AiState::Typing) }

    pub fn open(&mut self) {
        self.state = AiState::Typing;
        self.query.clear();
        self.response.clear();
    }

    pub fn close(&mut self) { self.state = AiState::Hidden; }

    pub fn type_char(&mut self, c: char) {
        if self.is_typing() { self.query.push(c); }
    }

    pub fn backspace(&mut self) {
        if self.is_typing() { self.query.pop(); }
    }

    pub fn set_loading(&mut self) {
        self.state = AiState::Loading;
        self.response.clear();
    }

    pub fn append_token(&mut self, tok: &str) {
        self.state = AiState::Streaming;
        self.response.push_str(tok);
    }

    pub fn mark_done(&mut self) { self.state = AiState::Done; }

    pub fn mark_error(&mut self, msg: String) { self.state = AiState::Error(msg); }

    /// Returns the shell command ready to write to the PTY.
    /// Strips any markdown code fences the model might have emitted.
    pub fn command_to_run(&self) -> Option<String> {
        if !matches!(self.state, AiState::Done | AiState::Streaming) { return None; }
        let s = self.response.trim();
        if s.is_empty() { return None; }
        let cmd = if s.starts_with("```") {
            // ```sh\ncommand\n```  → take line after the opening fence
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
