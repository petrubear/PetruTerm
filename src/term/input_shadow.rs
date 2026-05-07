use crate::term::tokenizer::{tokenize_command, CommandResolver, HistoryIndex, TokenKind};
use crate::term::Osc133Marker;
use winit::event::{KeyEvent, Modifiers};
use winit::keyboard::{Key, NamedKey};

/// Mirrors the user's current input line for decoration (syntax colour, ghost text, etc.).
/// Updated in parallel with PTY writes — does NOT replace them.
///
/// Lifecycle: active = true between OSC 133-A (PromptStart) and OSC 133-B (CommandStart).
/// Reset on Ctrl+C, Ctrl+U, Esc, and deactivated on CommandStart / CommandEnd.
pub struct InputShadow {
    /// Text typed since the last PromptStart, kept in sync with shell line-editor state.
    pub buf: String,
    /// Byte offset of the cursor within `buf`.
    pub cursor: usize,
    /// True while between OSC 133-A and OSC 133-B.
    pub active: bool,
    /// Non-blocking PATH resolver for the first token (command name).
    pub cmd_resolver: CommandResolver,
    /// I-3: history completion suffix to display as ghost text after the cursor.
    /// `None` when cursor is not at end of buf or no match found.
    pub ghost: Option<String>,
    history: HistoryIndex,
    last_cmd_name: String,
}

impl InputShadow {
    pub fn new() -> Self {
        Self {
            buf: String::new(),
            cursor: 0,
            active: false,
            cmd_resolver: CommandResolver::new(),
            ghost: None,
            history: HistoryIndex::load(),
            last_cmd_name: String::new(),
        }
    }

    /// Update state from an OSC 133 marker.
    pub fn on_osc133(&mut self, marker: &Osc133Marker) {
        match marker {
            Osc133Marker::PromptStart => {
                self.buf.clear();
                self.cursor = 0;
                self.active = true;
                self.ghost = None;
            }
            Osc133Marker::CommandStart(_) | Osc133Marker::CommandEnd(_) => {
                self.active = false;
                self.ghost = None;
            }
            Osc133Marker::OutputStart => {}
        }
    }

    /// Recompute ghost text from history. Called after every buf mutation.
    /// Ghost is only shown when cursor is at end of buf.
    pub fn update_ghost(&mut self) {
        if !self.active || self.buf.is_empty() || self.cursor != self.buf.len() {
            self.ghost = None;
            return;
        }
        self.ghost = self.history.find_suffix(&self.buf).map(str::to_string);
    }

    /// Accept the current ghost text (Tab / ArrowRight at end of buf).
    /// Returns the accepted suffix so the caller can write it to the PTY.
    /// Returns `None` if cursor is not at end or there is no ghost text.
    pub fn accept_ghost(&mut self) -> Option<String> {
        if self.cursor != self.buf.len() {
            return None;
        }
        let suffix = self.ghost.take()?;
        self.buf.push_str(&suffix);
        self.cursor = self.buf.len();
        self.update_ghost();
        Some(suffix)
    }

    /// Mirror a key event into the shadow buffer.
    /// Only has effect when `active == true`. Does not write to PTY.
    #[allow(clippy::collapsible_match)]
    pub fn on_key(&mut self, event: &KeyEvent, modifiers: &Modifiers) {
        if !self.active {
            return;
        }
        let ctrl = modifiers.state().control_key();

        match &event.logical_key {
            Key::Character(s) => {
                if ctrl {
                    match s.as_str() {
                        "c" | "C" => {
                            self.buf.clear();
                            self.cursor = 0;
                            self.active = false;
                        }
                        "u" | "U" => {
                            self.buf.drain(..self.cursor);
                            self.cursor = 0;
                        }
                        "k" | "K" => {
                            self.buf.truncate(self.cursor);
                        }
                        "w" | "W" => {
                            self.kill_word_before_cursor();
                        }
                        "a" | "A" => {
                            self.cursor = 0;
                        }
                        "e" | "E" => {
                            self.cursor = self.buf.len();
                        }
                        _ => {}
                    }
                } else {
                    self.buf.insert_str(self.cursor, s);
                    self.cursor += s.len();
                }
            }
            Key::Named(NamedKey::Backspace) => {
                if self.cursor > 0 {
                    let prev = self.buf[..self.cursor]
                        .char_indices()
                        .next_back()
                        .map(|(i, _)| i)
                        .unwrap_or(0);
                    self.buf.drain(prev..self.cursor);
                    self.cursor = prev;
                }
            }
            Key::Named(NamedKey::Delete) => {
                if self.cursor < self.buf.len() {
                    let ch_len = self.buf[self.cursor..]
                        .chars()
                        .next()
                        .map(|c| c.len_utf8())
                        .unwrap_or(0);
                    self.buf.drain(self.cursor..self.cursor + ch_len);
                }
            }
            Key::Named(NamedKey::ArrowLeft) => {
                if self.cursor > 0 {
                    let prev = self.buf[..self.cursor]
                        .char_indices()
                        .next_back()
                        .map(|(i, _)| i)
                        .unwrap_or(0);
                    self.cursor = prev;
                }
            }
            Key::Named(NamedKey::ArrowRight) => {
                if self.cursor < self.buf.len() {
                    let ch_len = self.buf[self.cursor..]
                        .chars()
                        .next()
                        .map(|c| c.len_utf8())
                        .unwrap_or(0);
                    self.cursor += ch_len;
                }
            }
            Key::Named(NamedKey::Home) => {
                self.cursor = 0;
            }
            Key::Named(NamedKey::End) => {
                self.cursor = self.buf.len();
            }
            Key::Named(NamedKey::Escape) => {
                self.buf.clear();
                self.cursor = 0;
            }
            // History navigation: the shell replaces the entire line — shadow is invalid.
            Key::Named(NamedKey::ArrowUp) | Key::Named(NamedKey::ArrowDown) => {
                self.active = false;
                self.buf.clear();
                self.cursor = 0;
                self.ghost = None;
            }
            _ => {}
        }
        self.maybe_schedule_cmd_resolve();
        self.update_ghost();
    }

    /// If the first token (command name) changed since last check, schedule a PATH lookup.
    fn maybe_schedule_cmd_resolve(&mut self) {
        let cmd_name = tokenize_command(&self.buf)
            .into_iter()
            .find(|t| t.kind == TokenKind::Command)
            .map(|t| self.buf[t.range.clone()].to_string())
            .unwrap_or_default();
        if cmd_name != self.last_cmd_name {
            self.last_cmd_name = cmd_name.clone();
            self.cmd_resolver.schedule(&cmd_name);
        }
    }

    fn kill_word_before_cursor(&mut self) {
        if self.cursor == 0 {
            return;
        }
        let before = &self.buf[..self.cursor];
        let chars: Vec<(usize, char)> = before.char_indices().collect();
        let mut i = chars.len();
        while i > 0 && chars[i - 1].1 == ' ' {
            i -= 1;
        }
        while i > 0 && chars[i - 1].1 != ' ' {
            i -= 1;
        }
        let byte_start = chars.get(i).map(|(b, _)| *b).unwrap_or(0);
        self.buf.drain(byte_start..self.cursor);
        self.cursor = byte_start;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::term::Osc133Marker;

    fn shadow() -> InputShadow {
        let mut s = InputShadow::new();
        s.on_osc133(&Osc133Marker::PromptStart);
        s
    }

    fn insert(s: &mut InputShadow, text: &str) {
        s.buf.insert_str(s.cursor, text);
        s.cursor += text.len();
    }

    #[test]
    fn prompt_start_activates() {
        let s = shadow();
        assert!(s.active);
        assert!(s.buf.is_empty());
        assert_eq!(s.cursor, 0);
    }

    #[test]
    fn command_start_deactivates() {
        let mut s = shadow();
        insert(&mut s, "ls");
        s.on_osc133(&Osc133Marker::CommandStart("ls".into()));
        assert!(!s.active);
    }

    #[test]
    fn command_end_deactivates() {
        let mut s = shadow();
        s.on_osc133(&Osc133Marker::CommandEnd(0));
        assert!(!s.active);
    }

    #[test]
    fn kill_word() {
        let mut s = shadow();
        insert(&mut s, "git commit");
        s.kill_word_before_cursor();
        assert_eq!(s.buf, "git ");
        assert_eq!(s.cursor, 4);
    }

    #[test]
    fn kill_word_trailing_spaces() {
        let mut s = shadow();
        insert(&mut s, "git   ");
        s.kill_word_before_cursor();
        assert_eq!(s.buf, "");
        assert_eq!(s.cursor, 0);
    }

    #[test]
    fn kill_word_at_start() {
        let mut s = shadow();
        s.kill_word_before_cursor();
        assert_eq!(s.buf, "");
        assert_eq!(s.cursor, 0);
    }

    #[test]
    fn ctrl_u_clears_before_cursor() {
        let mut s = shadow();
        insert(&mut s, "hello world");
        // move cursor back 5
        s.cursor -= 5;
        s.buf.drain(..s.cursor);
        s.cursor = 0;
        assert_eq!(s.buf, "world");
        assert_eq!(s.cursor, 0);
    }
}
