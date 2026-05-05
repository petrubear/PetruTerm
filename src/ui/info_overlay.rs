use crate::llm::markdown::{parse_markdown, AnnotatedLine, ParseState};

pub struct InfoOverlay {
    pub visible: bool,
    pub title: String,
    pub lines: Vec<AnnotatedLine>,
    pub scroll: usize,
}

impl InfoOverlay {
    pub fn new() -> Self {
        Self {
            visible: false,
            title: String::new(),
            lines: Vec::new(),
            scroll: 0,
        }
    }

    pub fn open(&mut self, title: String, content: &str, content_width: usize) {
        let lines = parse_markdown(content, content_width.max(20), &mut ParseState::default());
        self.title = title;
        self.lines = lines;
        self.scroll = 0;
        self.visible = true;
    }

    pub fn close(&mut self) {
        self.visible = false;
    }

    pub fn scroll_down(&mut self) {
        let max = self.lines.len().saturating_sub(1);
        self.scroll = (self.scroll + 1).min(max);
    }

    pub fn scroll_up(&mut self) {
        self.scroll = self.scroll.saturating_sub(1);
    }
}
