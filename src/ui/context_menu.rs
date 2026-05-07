/// An action that a context menu item can trigger.
#[derive(Debug, Clone, PartialEq)]
pub enum ContextAction {
    Copy,
    Paste,
    Clear,
    SendToChat,
    /// Copy the last shell command to the clipboard.
    CopyLastCommand,
    /// Open a hovered URL or file path with the system default handler.
    OpenLink(String),
    /// Copy a hovered URL or file path to the clipboard.
    CopyLink(String),
    /// Copy the output text of a command block (terminal_id, block_id).
    CopyBlockOutput(usize, usize),
    /// Re-run the command of a block by writing it to the PTY.
    ReRunCommand(String),
    /// Set the accent color for a tab (tab_index, color). None resets to theme default.
    SetTabColor(usize, Option<[f32; 4]>),
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
    /// When Some, a colored "●" swatch is rendered before the label.
    pub swatch_color: Option<[f32; 4]>,
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
pub const CONTEXT_MENU_WIDTH: usize = 30;

fn item(label: &str, action: ContextAction) -> ContextMenuItem {
    ContextMenuItem {
        label: label.to_string(),
        keybind: None,
        action,
        swatch_color: None,
    }
}

fn item_kb(label: &str, kb: &str, action: ContextAction) -> ContextMenuItem {
    ContextMenuItem {
        label: label.to_string(),
        keybind: Some(kb.to_string()),
        action,
        swatch_color: None,
    }
}

fn separator() -> ContextMenuItem {
    ContextMenuItem {
        label: String::new(),
        keybind: None,
        action: ContextAction::Separator,
        swatch_color: None,
    }
}

fn label_item(text: &str) -> ContextMenuItem {
    ContextMenuItem {
        label: text.to_string(),
        keybind: None,
        action: ContextAction::Label,
        swatch_color: None,
    }
}

impl ContextMenu {
    pub fn new() -> Self {
        Self {
            visible: false,
            col: 0,
            row: 0,
            items: vec![
                item_kb("Copy", "Cmd+C", ContextAction::Copy),
                item_kb("Paste", "Cmd+V", ContextAction::Paste),
                item("Clear", ContextAction::Clear),
                separator(),
                item("Ask AI", ContextAction::SendToChat),
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

    /// Open a link-specific context menu at (col, row).
    pub fn open_with_link(
        &mut self,
        link_text: String,
        col: usize,
        row: usize,
        term_cols: usize,
        term_rows: usize,
    ) {
        self.items = vec![
            item("Open Link", ContextAction::OpenLink(link_text.clone())),
            item("Copy Link", ContextAction::CopyLink(link_text)),
            separator(),
            item_kb("Copy", "Cmd+C", ContextAction::Copy),
            item_kb("Paste", "Cmd+V", ContextAction::Paste),
        ];
        self.open(col, row, term_cols, term_rows);
    }

    /// Open an exit-code info popup just above the status bar.
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
            label_item(&format!("✘ Exit code: {exit_code}")),
            separator(),
            label_item(&cmd_display),
            separator(),
            item("Copy command", ContextAction::CopyLastCommand),
        ];

        let height = self.items.len();
        let clamped_col = if col + CONTEXT_MENU_WIDTH > term_cols {
            term_cols.saturating_sub(CONTEXT_MENU_WIDTH)
        } else {
            col
        };
        let row = term_rows.saturating_sub(height);
        self.col = clamped_col;
        self.row = row;
        self.hovered = None;
        self.visible = true;
    }

    /// Open a block-specific context menu at (col, row).
    #[allow(clippy::too_many_arguments)]
    pub fn open_with_block(
        &mut self,
        terminal_id: usize,
        block_id: usize,
        command_text: String,
        col: usize,
        row: usize,
        term_cols: usize,
        term_rows: usize,
    ) {
        let max_label = CONTEXT_MENU_WIDTH.saturating_sub(4);
        let cmd_display = if command_text.is_empty() {
            "(no command)".to_string()
        } else {
            let chars: Vec<char> = command_text.chars().collect();
            if chars.len() > max_label {
                format!(
                    "{}…",
                    chars[..max_label.saturating_sub(1)]
                        .iter()
                        .collect::<String>()
                )
            } else {
                command_text.clone()
            }
        };
        self.items = vec![
            label_item(&cmd_display),
            separator(),
            item_kb(
                "Copy Output",
                "Leader y",
                ContextAction::CopyBlockOutput(terminal_id, block_id),
            ),
            item_kb(
                "Re-run Command",
                "Leader r",
                ContextAction::ReRunCommand(command_text),
            ),
            separator(),
            item("Clear", ContextAction::Clear),
            item_kb("Copy", "Cmd+C", ContextAction::Copy),
            item_kb("Paste", "Cmd+V", ContextAction::Paste),
            item("Ask AI", ContextAction::SendToChat),
        ];
        self.open(col, row, term_cols, term_rows);
    }

    /// Open a tab color picker at (col, row).
    /// `tab_idx`: the tab index to color.
    /// `brights`: the theme's bright ANSI colors array [8 entries].
    pub fn open_tab_color_picker(
        &mut self,
        tab_idx: usize,
        brights: &[[f32; 4]; 8],
        col: usize,
        row: usize,
        term_cols: usize,
        term_rows: usize,
    ) {
        let color_names = ["Red", "Green", "Yellow", "Blue", "Magenta", "Cyan", "White"];
        let mut items: Vec<ContextMenuItem> = vec![label_item("Tab Color"), separator()];
        for (i, name) in color_names.iter().enumerate() {
            let color = brights[i + 1];
            items.push(ContextMenuItem {
                label: name.to_string(),
                keybind: None,
                action: ContextAction::SetTabColor(tab_idx, Some(color)),
                swatch_color: Some(color),
            });
        }
        items.push(separator());
        items.push(item("Reset", ContextAction::SetTabColor(tab_idx, None)));
        self.items = items;
        self.open(col, row, term_cols, term_rows);
    }

    /// Given a terminal cell (col, row), return the action for that item if it's inside the menu.
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
