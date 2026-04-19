/// An action that a context menu item can trigger.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContextAction {
    Copy,
    Paste,
    Clear,
    SendToChat,
    /// Copy the last shell command to the clipboard.
    CopyLastCommand,
    /// Non-interactive separator row.
    Separator,
    /// Non-interactive informational label (displays text, no action).
    Label,
}

/// A single item in the context menu.
#[derive(Debug, Clone)]
pub struct ContextMenuItem {
    pub label: String,
    pub keybind: Option<String>,
    pub action: ContextAction,
}

impl ContextMenuItem {
    pub fn is_separator(&self) -> bool {
        self.action == ContextAction::Separator
    }

    /// True for non-interactive display rows (Separator or Label).
    pub fn is_non_interactive(&self) -> bool {
        matches!(self.action, ContextAction::Separator | ContextAction::Label)
    }
}

/// Floating right-click context menu.
pub struct ContextMenu {
    pub visible: bool,
    /// Terminal-grid column where the menu top-left starts.
    pub col: usize,
    /// Terminal-grid row where the menu top-left starts.
    pub row: usize,
    /// Items to show.
    pub items: Vec<ContextMenuItem>,
    /// Index of the item currently hovered by the mouse, if any.
    pub hovered: Option<usize>,
}

/// Width in terminal columns for the context menu popup.
pub const CONTEXT_MENU_WIDTH: usize = 24;

impl ContextMenu {
    pub fn new() -> Self {
        Self {
            visible: false,
            col: 0,
            row: 0,
            items: vec![
                ContextMenuItem {
                    label: "Copy".into(),
                    keybind: Some("Cmd+C".into()),
                    action: ContextAction::Copy,
                },
                ContextMenuItem {
                    label: "Paste".into(),
                    keybind: Some("Cmd+V".into()),
                    action: ContextAction::Paste,
                },
                ContextMenuItem {
                    label: "Clear".into(),
                    keybind: None,
                    action: ContextAction::Clear,
                },
                ContextMenuItem {
                    label: String::new(),
                    keybind: None,
                    action: ContextAction::Separator,
                },
                ContextMenuItem {
                    label: "Ask AI".into(),
                    keybind: None,
                    action: ContextAction::SendToChat,
                },
            ],
            hovered: None,
        }
    }

    /// Open the menu at the given terminal cell position, clamping to terminal bounds.
    pub fn open(&mut self, col: usize, row: usize, term_cols: usize, term_rows: usize) {
        let height = self.items.len();
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

    /// Open an exit-code info popup just above the status bar.
    ///
    /// `exit_code`: the non-zero code to display.
    /// `last_command`: the last shell command that ran (may be empty).
    /// `col`: click column from the status bar click.
    /// `term_rows`: terminal grid height (status bar is at row `term_rows`).
    /// `term_cols`: terminal grid width.
    pub fn open_exit_info(
        &mut self,
        exit_code: i32,
        last_command: &str,
        col: usize,
        term_rows: usize,
        term_cols: usize,
    ) {
        let max_label = CONTEXT_MENU_WIDTH.saturating_sub(4);
        let cmd_display = if last_command.is_empty() {
            "(no command recorded)".to_string()
        } else {
            let chars: Vec<char> = last_command.chars().collect();
            if chars.len() > max_label {
                format!(
                    "{}…",
                    chars[..max_label.saturating_sub(1)]
                        .iter()
                        .collect::<String>()
                )
            } else {
                last_command.to_string()
            }
        };

        self.items = vec![
            ContextMenuItem {
                label: format!("✘ Exit code: {exit_code}"),
                keybind: None,
                action: ContextAction::Label,
            },
            ContextMenuItem {
                label: String::new(),
                keybind: None,
                action: ContextAction::Separator,
            },
            ContextMenuItem {
                label: cmd_display,
                keybind: None,
                action: ContextAction::Label,
            },
            ContextMenuItem {
                label: String::new(),
                keybind: None,
                action: ContextAction::Separator,
            },
            ContextMenuItem {
                label: "Copy command".to_string(),
                keybind: None,
                action: ContextAction::CopyLastCommand,
            },
        ];

        let height = self.items.len();
        let clamped_col = if col + CONTEXT_MENU_WIDTH > term_cols {
            term_cols.saturating_sub(CONTEXT_MENU_WIDTH)
        } else {
            col
        };
        // Place above the status bar (status bar is at term_rows, menu goes upward).
        let row = term_rows.saturating_sub(height);
        self.col = clamped_col;
        self.row = row;
        self.hovered = None;
        self.visible = true;
    }

    /// Given a terminal cell (col, row), return the action for that item if it's inside the menu.
    /// Separator rows are skipped (return None).
    pub fn hit_test(&self, col: usize, row: usize) -> Option<ContextAction> {
        if !self.visible {
            return None;
        }
        if col < self.col || col >= self.col + CONTEXT_MENU_WIDTH {
            return None;
        }
        if row < self.row || row >= self.row + self.items.len() {
            return None;
        }
        let idx = row - self.row;
        self.items.get(idx).and_then(|item| {
            if item.is_non_interactive() {
                None
            } else {
                Some(item.action.clone())
            }
        })
    }

    /// Update the hovered item index based on a terminal cell position.
    /// Separator rows are never hovered. Returns true if hover state changed.
    pub fn update_hover(&mut self, col: usize, row: usize) -> bool {
        if !self.visible {
            return false;
        }
        let new_hover = if col >= self.col
            && col < self.col + CONTEXT_MENU_WIDTH
            && row >= self.row
            && row < self.row + self.items.len()
        {
            let idx = row - self.row;
            if self
                .items
                .get(idx)
                .map(|i| i.is_non_interactive())
                .unwrap_or(false)
            {
                None
            } else {
                Some(idx)
            }
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
