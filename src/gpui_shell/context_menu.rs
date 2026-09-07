// gpui chrome migration (M4c Task 1): the right-click context menu's own
// state and action dispatch. `crate::ui::context_menu::{ContextAction,
// ContextMenuItem}` are pure data (no wgpu/coordinate coupling) and are
// reused directly -- `ContextMenu` itself is NOT reused: its `open()`/
// `hit_test()`/`col`/`row` are built around the wgpu build's manual
// terminal-cell hit-testing (a custom rect renderer over grid cells),
// which has no equivalent in gpui_shell. This file's own `ContextMenu`
// is a genuinely new, minimal type: visible + a real pixel position +
// the item list, rendered as real clickable `div()`s (Task 2/3) the same
// way every other list this migration has built already works.

use gpui::{Context, Pixels, Point};

use crate::ui::context_menu::{ContextAction, ContextMenuItem};

use super::GpuiShellRoot;

/// The right-click menu's own state. `position` is a real window-relative
/// pixel point (the click that opened it), not a terminal-cell coordinate.
#[derive(Default)]
pub struct ContextMenu {
    #[allow(dead_code)]
    pub visible: bool,
    #[allow(dead_code)]
    pub position: Point<Pixels>,
    #[allow(dead_code)]
    pub items: Vec<ContextMenuItem>,
}

impl ContextMenu {
    #[allow(dead_code)]
    pub fn close(&mut self) {
        self.visible = false;
    }
}

impl GpuiShellRoot {
    /// Run one confirmed context-menu action. `ContextAction`'s other
    /// variants (`SendToChat`, `CopyLastCommand`, `OpenLink`, `CopyLink`,
    /// `CopyBlockOutput`, `ReRunCommand`, `Separator`, `Label`) are never
    /// constructed by this milestone's own item lists (Task 2/3) -- no
    /// arm needed for them here, `_ => {}` covers anything unreachable in
    /// practice the same way M4a's palette dispatch does.
    #[allow(dead_code)]
    pub(super) fn dispatch_context_action(
        &mut self,
        action: ContextAction,
        cx: &mut Context<Self>,
    ) {
        self.context_menu.close();
        match action {
            ContextAction::Copy => {
                let active_ws = self.workspaces.active();
                let active_tid =
                    active_ws.tab_panes[active_ws.tabs.active_index()].focused_terminal;
                if let Some(terminal) = self.terminals.get(&active_tid) {
                    if let Some(text) = terminal.selection_text() {
                        cx.write_to_clipboard(gpui::ClipboardItem::new_string(text));
                    }
                }
            }
            ContextAction::Paste => {
                if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
                    self.paste_text_to_active_terminal(&text);
                }
                cx.notify();
            }
            ContextAction::Clear => self.clear_active_terminal(),
            ContextAction::SetTabColor(idx, color) => {
                self.workspaces.active_mut().tabs.set_tab_color(idx, color);
                cx.notify();
            }
            _ => {}
        }
    }

    pub(super) fn clear_active_terminal(&mut self) {
        let active_ws = self.workspaces.active();
        let active_tid = active_ws.tab_panes[active_ws.tabs.active_index()].focused_terminal;
        if let Some(terminal) = self.terminals.get(&active_tid) {
            terminal.clear_screen_and_scrollback();
        }
    }

    /// Shared with `input.rs`'s own `Cmd+V` handler (updated below in this
    /// same step) so the two can't drift apart. Ported verbatim from that
    /// handler's own pre-existing bracketed-paste branching.
    pub(super) fn paste_text_to_active_terminal(&mut self, text: &str) {
        let active_ws = self.workspaces.active();
        let active_tid = active_ws.tab_panes[active_ws.tabs.active_index()].focused_terminal;
        let Some(terminal) = self.terminals.get(&active_tid) else {
            return;
        };
        if terminal.bracketed_paste_mode() {
            let mut data = b"\x1b[200~".to_vec();
            data.extend_from_slice(text.as_bytes());
            data.extend_from_slice(b"\x1b[201~");
            terminal.write_input(&data);
        } else {
            terminal.write_input(text.as_bytes());
        }
    }
}
