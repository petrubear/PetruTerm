#[derive(Debug, Default, Clone)]
pub struct SidebarState {
    pub visible: bool,
    pub nav_cursor: usize,
    pub panel_resize_drag: bool,
    pub panel_resize_hover: bool,
    pub rename_input: Option<String>,
    pub keyboard_active: bool,
    pub active_section: u8,
    pub mcp_scroll: usize,
    pub skills_scroll: usize,
    pub steering_scroll: usize,
}
