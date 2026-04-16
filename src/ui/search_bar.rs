/// A single text match in the terminal grid.
#[derive(Debug, Clone)]
pub struct SearchMatch {
    /// Grid line (0+ = current screen, negative = scrollback history).
    pub grid_line: i32,
    pub col: usize,
    pub len: usize,
}

/// In-terminal text search overlay (Cmd+F).
#[derive(Default)]
pub struct SearchBar {
    pub query: String,
    pub matches: Vec<SearchMatch>,
    /// Index of the currently highlighted match.
    pub current: usize,
    pub visible: bool,
    /// Query changed — re-run search before next render.
    pub dirty: bool,
    /// Current match changed — scroll terminal to it before next render.
    pub scroll_needed: bool,
    /// Query that produced the current `matches` list.
    /// Used to detect when the new query extends the old one so we can filter
    /// existing matches instead of re-scanning the whole grid (TD-PERF-11).
    pub last_query: String,
}


impl SearchBar {
    pub fn open(&mut self) {
        self.visible = true;
        self.dirty = true;
    }

    pub fn close(&mut self) {
        self.visible = false;
        self.query.clear();
        self.last_query.clear();
        self.matches.clear();
        self.current = 0;
        self.dirty = true;
        self.scroll_needed = false;
    }

    pub fn type_char(&mut self, c: char) {
        self.query.push(c);
        self.dirty = true;
    }

    pub fn backspace(&mut self) {
        if self.query.pop().is_some() {
            self.dirty = true;
        }
    }

    /// Replace match list after a search run. Resets to first match.
    pub fn set_matches(&mut self, matches: Vec<SearchMatch>) {
        self.current = 0;
        self.matches = matches;
        self.scroll_needed = !self.matches.is_empty();
    }

    pub fn next_match(&mut self) {
        if !self.matches.is_empty() {
            self.current = (self.current + 1) % self.matches.len();
            self.scroll_needed = true;
        }
    }

    pub fn prev_match(&mut self) {
        if !self.matches.is_empty() {
            self.current = self.current.checked_sub(1).unwrap_or(self.matches.len() - 1);
            self.scroll_needed = true;
        }
    }

    pub fn current_match(&self) -> Option<&SearchMatch> {
        self.matches.get(self.current)
    }

    /// Match counter label, e.g. "3 / 12" or "no results".
    pub fn count_label(&self) -> String {
        if self.query.is_empty() {
            String::new()
        } else if self.matches.is_empty() {
            "no results".to_string()
        } else {
            format!("{} / {}", self.current + 1, self.matches.len())
        }
    }
}
