/// An action that a context menu item can trigger.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContextAction {
    Copy,
    Paste,
    Clear,
}

/// A single item in the context menu.
#[derive(Debug, Clone)]
pub struct ContextMenuItem {
    pub label: String,
    pub keybind: Option<String>,
    pub action: ContextAction,
}

/// Floating right-click context menu.
pub struct ContextMenu {
    pub visible: bool,
    /// Terminal-grid column where the menu top-left starts.
    pub col: usize,
    /// Terminal-grid row where the menu top-left starts.
    pub row: usize,
    /// Items to show (always Copy / Paste / Clear for now).
    pub items: Vec<ContextMenuItem>,
    /// Index of the item currently hovered by the mouse, if any.
    pub hovered: Option<usize>,
}

/// Width in terminal columns for the context menu popup.
pub const CONTEXT_MENU_WIDTH: usize = 22;

impl ContextMenu {
    pub fn new() -> Self {
        Self {
            visible: false,
            col: 0,
            row: 0,
            items: vec![
                ContextMenuItem { label: "Copy".into(),  keybind: Some("Cmd+C".into()), action: ContextAction::Copy  },
                ContextMenuItem { label: "Paste".into(), keybind: Some("Cmd+V".into()), action: ContextAction::Paste },
                ContextMenuItem { label: "Clear".into(), keybind: None,                 action: ContextAction::Clear },
            ],
            hovered: None,
        }
    }

    /// Open the menu at the given terminal cell position, clamping to terminal bounds.
    pub fn open(&mut self, col: usize, row: usize, term_cols: usize, term_rows: usize) {
        let height = self.items.len();
        // Clamp so the menu doesn't extend past the right or bottom edge.
        let clamped_col = if col + CONTEXT_MENU_WIDTH > term_cols {
            term_cols.saturating_sub(CONTEXT_MENU_WIDTH)
        } else {
            col
        };
        let clamped_row = if row + height > term_rows {
            term_rows.saturating_sub(height)
        } else {
            row
        };
        self.col = clamped_col;
        self.row = clamped_row;
        self.hovered = None;
        self.visible = true;
    }

    pub fn close(&mut self) {
        self.visible = false;
        self.hovered = None;
    }

    /// Given a terminal cell (col, row), return the action for that item if it's inside the menu.
    pub fn hit_test(&self, col: usize, row: usize) -> Option<ContextAction> {
        if !self.visible { return None; }
        if col < self.col || col >= self.col + CONTEXT_MENU_WIDTH { return None; }
        if row < self.row || row >= self.row + self.items.len() { return None; }
        let idx = row - self.row;
        self.items.get(idx).map(|item| item.action.clone())
    }

    /// Update the hovered item index based on a terminal cell position.
    /// Returns true if the hover state changed.
    pub fn update_hover(&mut self, col: usize, row: usize) -> bool {
        if !self.visible {
            return false;
        }
        let new_hover = if col >= self.col
            && col < self.col + CONTEXT_MENU_WIDTH
            && row >= self.row
            && row < self.row + self.items.len()
        {
            Some(row - self.row)
        } else {
            None
        };
        if new_hover != self.hovered {
            self.hovered = new_hover;
            true
        } else {
            false
        }
    }
}
