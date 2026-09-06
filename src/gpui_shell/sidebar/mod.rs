// gpui chrome migration (M3c Task 4): the workspace sidebar drawer's
// visibility state. Mirrors `chat_panel`'s own `visible: bool` +
// `toggle`/`is_visible` shape (`chat_panel/mod.rs`) -- this drawer has no
// streaming state or composer of its own (M3c scope is "Workspaces section
// only", per the M3 design's milestone table; MCP/Skills/Steering +
// `InfoOverlay` are M3d), so there is nothing else to hold here yet.

pub mod render;

#[derive(Default)]
pub struct WorkspaceSidebar {
    visible: bool,
}

impl WorkspaceSidebar {
    pub fn is_visible(&self) -> bool {
        self.visible
    }

    pub fn toggle(&mut self) {
        self.visible = !self.visible;
    }

    /// Force the drawer open -- `begin_workspace_rename` calls this so the
    /// rename editor (rendered inline in the sidebar row, same as a tab
    /// rename renders inline in the tab bar) is never focused while
    /// invisible.
    pub fn show(&mut self) {
        self.visible = true;
    }
}
