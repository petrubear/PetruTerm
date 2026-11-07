// Right-click menu of the Agents list: Rename, Copy session id, Reset name. Built on the
// shared `ContextMenu`; the actions are dispatched from `context_menu.rs`.

use gpui::{ClipboardItem, Context, Pixels, Point};

use super::GpuiShellRoot;
use crate::ui::context_menu::{ContextAction, ContextMenuItem};

fn item(label: &str, action: ContextAction) -> ContextMenuItem {
    ContextMenuItem {
        label: label.to_string(),
        keybind: None,
        action,
        swatch_color: None,
    }
}

impl GpuiShellRoot {
    /// Open the menu for the agent in `terminal_id` at `position`.
    pub(super) fn open_agent_menu(&mut self, terminal_id: usize, position: Point<Pixels>) {
        let Some(row) = self
            .agent_rows()
            .into_iter()
            .find(|r| r.terminal_id == terminal_id)
        else {
            return;
        };
        let mut items = vec![item("Rename", ContextAction::AgentRename(terminal_id))];
        if row.session.is_some() {
            items.push(item(
                "Copy session id",
                ContextAction::AgentCopySessionId(terminal_id),
            ));
        }
        if row.custom_name.is_some() {
            items.push(item(
                "Reset name",
                ContextAction::AgentResetName(terminal_id),
            ));
        }
        self.context_menu.position = position;
        self.context_menu.items = items;
        self.context_menu.visible = true;
    }

    /// Run one of the three agent actions. Rename only records the request: opening the editor
    /// needs a `Window`, which the menu callback does not have, so `render()` picks it up.
    pub(super) fn dispatch_agent_action(&mut self, action: ContextAction, cx: &mut Context<Self>) {
        match action {
            ContextAction::AgentRename(t) => self.pending_agent_rename = Some(t),
            ContextAction::AgentCopySessionId(t) => {
                if let Some(id) = self
                    .agent_rows()
                    .into_iter()
                    .find(|r| r.terminal_id == t)
                    .and_then(|r| r.session)
                {
                    cx.write_to_clipboard(ClipboardItem::new_string(id));
                }
            }
            ContextAction::AgentResetName(t) => self.agent_names.reset(t),
            _ => {}
        }
        cx.notify();
    }
}
