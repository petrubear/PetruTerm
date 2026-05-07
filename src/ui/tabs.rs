#![allow(dead_code)]

/// A single terminal tab.
#[derive(Debug)]
pub struct Tab {
    pub id: usize,
    pub title: String,
    /// Index into the pane tree (one pane tree per tab).
    pub pane_tree_id: usize,
    /// Optional accent color override. None → use theme ui_accent.
    pub accent_color: Option<[f32; 4]>,
}

/// Manages the ordered list of tabs.
pub struct TabManager {
    tabs: Vec<Tab>,
    active: usize,
    next_id: usize,
}

impl TabManager {
    pub fn new() -> Self {
        Self {
            tabs: Vec::new(),
            active: 0,
            next_id: 0,
        }
    }

    /// Create a new tab, returning its ID.
    pub fn new_tab(&mut self, title: impl Into<String>) -> usize {
        let id = self.next_id;
        self.next_id += 1;
        let pane_tree_id = id; // 1:1 mapping for now
        self.tabs.push(Tab {
            id,
            title: title.into(),
            pane_tree_id,
            accent_color: None,
        });
        self.active = self.tabs.len() - 1;
        id
    }

    /// Close the tab with the given ID. Returns true if a tab was removed.
    pub fn close_tab(&mut self, id: usize) -> bool {
        if let Some(pos) = self.tabs.iter().position(|t| t.id == id) {
            self.tabs.remove(pos);
            // Adjust active index.
            if !self.tabs.is_empty() {
                self.active = self.active.min(self.tabs.len() - 1);
            }
            return true;
        }
        false
    }

    /// Switch to the tab at the given index (0-based). Returns true if successful.
    pub fn switch_to_index(&mut self, idx: usize) -> bool {
        if idx < self.tabs.len() {
            self.active = idx;
            true
        } else {
            false
        }
    }

    /// Switch to the next tab (wraps around).
    pub fn next_tab(&mut self) {
        if !self.tabs.is_empty() {
            self.active = (self.active + 1) % self.tabs.len();
        }
    }

    /// Switch to the previous tab (wraps around).
    pub fn prev_tab(&mut self) {
        if !self.tabs.is_empty() {
            self.active = (self.active + self.tabs.len() - 1) % self.tabs.len();
        }
    }

    /// Set the accent color for the tab at `idx`. Pass None to reset to theme default.
    pub fn set_tab_color(&mut self, idx: usize, color: Option<[f32; 4]>) {
        if let Some(tab) = self.tabs.get_mut(idx) {
            tab.accent_color = color;
        }
    }

    /// Returns the active tab's accent color, falling back to `default`.
    pub fn active_accent(&self, default: [f32; 4]) -> [f32; 4] {
        self.tabs
            .get(self.active)
            .and_then(|t| t.accent_color)
            .unwrap_or(default)
    }

    /// Rename the active tab.
    pub fn rename_active(&mut self, title: impl Into<String>) {
        if let Some(tab) = self.tabs.get_mut(self.active) {
            tab.title = title.into();
        }
    }

    pub fn active_tab(&self) -> Option<&Tab> {
        self.tabs.get(self.active)
    }

    pub fn active_index(&self) -> usize {
        self.active
    }

    pub fn tabs(&self) -> &[Tab] {
        &self.tabs
    }

    pub fn tab_count(&self) -> usize {
        self.tabs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tabs.is_empty()
    }
}

impl Default for TabManager {
    fn default() -> Self {
        Self::new()
    }
}
