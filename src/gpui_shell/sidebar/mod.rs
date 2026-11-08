// The workspace sidebar drawer's visibility, active-section, and
// per-section-cursor state.

pub mod agent_card;
pub mod agents;
pub mod render;
pub mod sections;

/// Which of the sidebar's five sections is active. Cycled by Tab/Shift+Tab
/// while the sidebar holds keyboard focus (`sidebar_nav.rs`), or by clicking a
/// section-tab label (`render.rs`'s `on_select_section`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SidebarSection {
    #[default]
    Agents,
    Workspaces,
    Mcp,
    Skills,
    Steering,
}

impl SidebarSection {
    pub fn next(self) -> Self {
        match self {
            Self::Agents => Self::Workspaces,
            Self::Workspaces => Self::Mcp,
            Self::Mcp => Self::Skills,
            Self::Skills => Self::Steering,
            Self::Steering => Self::Agents,
        }
    }

    pub fn prev(self) -> Self {
        match self {
            Self::Agents => Self::Steering,
            Self::Workspaces => Self::Agents,
            Self::Mcp => Self::Workspaces,
            Self::Skills => Self::Mcp,
            Self::Steering => Self::Skills,
        }
    }
}

#[derive(Default)]
pub struct WorkspaceSidebar {
    visible: bool,
    active_section: SidebarSection,
    /// Highlighted row in the MCP list.
    mcp_cursor: usize,
    /// Highlighted row within the Skills section's list. See `mcp_cursor`.
    skills_cursor: usize,
    /// Highlighted row within the Steering section's list. See `mcp_cursor`.
    steering_cursor: usize,
    /// Highlighted row within the Agents section's list. See `mcp_cursor`.
    agents_cursor: usize,
}

impl WorkspaceSidebar {
    pub fn is_visible(&self) -> bool {
        self.visible
    }

    pub fn toggle(&mut self) {
        self.visible = !self.visible;
    }

    /// Force the drawer open -- used by `begin_workspace_rename` so
    /// the rename editor (rendered inline in the sidebar row) is never
    /// focused while invisible.
    pub fn show(&mut self) {
        self.visible = true;
    }

    pub fn active_section(&self) -> SidebarSection {
        self.active_section
    }

    pub fn next_section(&mut self) {
        self.active_section = self.active_section.next();
    }

    pub fn prev_section(&mut self) {
        self.active_section = self.active_section.prev();
    }

    pub fn set_section(&mut self, section: SidebarSection) {
        self.active_section = section;
    }

    pub fn mcp_cursor(&self) -> usize {
        self.mcp_cursor
    }

    pub fn set_mcp_cursor(&mut self, idx: usize) {
        self.mcp_cursor = idx;
    }

    pub fn skills_cursor(&self) -> usize {
        self.skills_cursor
    }

    pub fn set_skills_cursor(&mut self, idx: usize) {
        self.skills_cursor = idx;
    }

    pub fn steering_cursor(&self) -> usize {
        self.steering_cursor
    }

    pub fn set_steering_cursor(&mut self, idx: usize) {
        self.steering_cursor = idx;
    }

    pub fn agents_cursor(&self) -> usize {
        self.agents_cursor
    }

    pub fn set_agents_cursor(&mut self, idx: usize) {
        self.agents_cursor = idx;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn section_next_cycles_through_all_five_and_wraps() {
        let mut s = SidebarSection::Workspaces;
        s = s.next();
        assert_eq!(s, SidebarSection::Mcp);
        s = s.next();
        assert_eq!(s, SidebarSection::Skills);
        s = s.next();
        assert_eq!(s, SidebarSection::Steering);
        s = s.next();
        assert_eq!(s, SidebarSection::Agents);
        s = s.next();
        assert_eq!(s, SidebarSection::Workspaces);
    }

    #[test]
    fn section_prev_cycles_backward_and_wraps() {
        let mut s = SidebarSection::Workspaces;
        s = s.prev();
        assert_eq!(s, SidebarSection::Agents);
        s = s.prev();
        assert_eq!(s, SidebarSection::Steering);
        s = s.prev();
        assert_eq!(s, SidebarSection::Skills);
        s = s.prev();
        assert_eq!(s, SidebarSection::Mcp);
        s = s.prev();
        assert_eq!(s, SidebarSection::Workspaces);
    }
}
