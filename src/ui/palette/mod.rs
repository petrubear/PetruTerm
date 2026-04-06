pub mod actions;

pub use actions::{Action, PaletteAction};

use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;

use crate::config::Config;
use self::actions::built_in_actions;

/// Command palette state machine.
pub struct CommandPalette {
    /// All registered actions (built-in + plugin-registered in Phase 3).
    all_actions: Vec<PaletteAction>,
    /// Current search query.
    pub query: String,
    /// Filtered + scored results (sorted by score desc).
    pub results: Vec<PaletteAction>,
    /// Currently highlighted result index.
    pub selected: usize,
    /// Whether the palette is visible.
    pub visible: bool,
    matcher: SkimMatcherV2,
}

impl CommandPalette {
    pub fn new(config: &Config) -> Self {
        let all_actions = built_in_actions(config);
        let results = all_actions.clone();
        Self {
            all_actions,
            query: String::new(),
            results,
            selected: 0,
            visible: false,
            matcher: SkimMatcherV2::default(),
        }
    }

    /// Rebuild the action list with fresh keybinds from `config` (call after hot-reload).
    pub fn rebuild_keybinds(&mut self, config: &Config) {
        self.all_actions = built_in_actions(config);
        if self.visible { self.filter(); } else { self.results = self.all_actions.clone(); }
    }

    /// Open the palette (reset state).
    pub fn open(&mut self) {
        self.query.clear();
        self.results = self.all_actions.clone();
        self.selected = 0;
        self.visible = true;
    }

    /// Close the palette.
    pub fn close(&mut self) {
        self.visible = false;
        self.query.clear();
    }

    /// Append a character to the search query and re-filter results.
    pub fn type_char(&mut self, c: char) {
        self.query.push(c);
        self.filter();
    }

    /// Delete the last character from the query and re-filter.
    pub fn backspace(&mut self) {
        self.query.pop();
        self.filter();
    }

    /// Move selection up.
    pub fn select_up(&mut self) {
        if !self.results.is_empty() {
            self.selected = (self.selected + self.results.len() - 1) % self.results.len();
        }
    }

    /// Move selection down.
    pub fn select_down(&mut self) {
        if !self.results.is_empty() {
            self.selected = (self.selected + 1) % self.results.len();
        }
    }

    /// Confirm the current selection, returning the action if any.
    pub fn confirm(&mut self) -> Option<Action> {
        let action = self.results.get(self.selected).map(|a| a.action.clone());
        self.close();
        action
    }

    /// Register an additional action (used by plugins in Phase 3).
    #[allow(dead_code)]
    pub fn register(&mut self, action: PaletteAction) {
        self.all_actions.push(action);
        if self.visible {
            self.filter();
        }
    }

    fn filter(&mut self) {
        if self.query.is_empty() {
            self.results = self.all_actions.clone();
        } else {
            let query = &self.query;
            let matcher = &self.matcher;
            let mut scored: Vec<(i64, &PaletteAction)> = self
                .all_actions
                .iter()
                .filter_map(|a| matcher.fuzzy_match(&a.name, query).map(|s| (s, a)))
                .collect();
            scored.sort_by(|a, b| b.0.cmp(&a.0));
            self.results = scored.into_iter().map(|(_, a)| a.clone()).collect();
        }
        self.selected = 0;
    }
}
