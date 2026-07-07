use std::sync::Arc;
use winit::application::ApplicationHandler;
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoopProxy};
use winit::keyboard::{Key, NamedKey};
use winit::window::{Window, WindowAttributes, WindowId};

use alacritty_terminal::selection::SelectionType;

use crate::config::schema::TitleBarStyle;
use crate::config::watcher::ConfigWatcher;
use crate::config::{self, Config};
use crate::ui::{ContextAction, SidebarState};

mod app_state;
mod frame;
mod hover_link;
mod input;
mod layout;
mod menu;
mod mux;
mod renderer;
mod ui;

pub use input::InputHandler;
pub use menu::AppMenu;
pub use mux::Mux;
pub use renderer::RenderContext;
pub use ui::UiManager;

pub const TITLEBAR_HEIGHT: f32 = 30.0;
pub const SIDEBAR_COLS: usize = 28;
/// Gap in physical pixels between the sidebar's right edge and the terminal content.
pub const SIDEBAR_MARGIN: f32 = 6.0;

/// Top-level application state. Delegates to specialized managers.
pub struct App {
    config: Config,
    config_watcher: Option<ConfigWatcher>,

    window: Option<Arc<Window>>,
    render_ctx: Option<RenderContext>,
    mux: Mux,
    ui: UiManager,
    input: InputHandler,

    menu: AppMenu,

    wakeup_proxy: EventLoopProxy<()>,

    /// PTY render coalescing: when PTY data arrives, don't render immediately.
    /// Instead, wait for a short quiet window so that multi-batch TUI updates
    /// (erase + redraw) are coalesced into a single frame, preventing flickering.
    pending_pty_redraw: bool,
    last_pty_activity: std::time::Instant,
    /// After we write input to a PTY (paste, key echo, atuin selection, agent
    /// command), the shell echoes back asynchronously. That echo wakes the loop
    /// via `EventLoopProxy::send_event`, but on macOS that wakeup is occasionally
    /// lost/delayed — leaving the parked loop showing stale content until an
    /// unrelated event (scroll/keypress) or the next timer fires. While this
    /// grace window is active, `about_to_wait` caps its wait to a few ms so it
    /// re-polls the PTY on a reliable timer and renders the echo regardless.
    pty_echo_grace_until: Option<std::time::Instant>,
    /// Number of PTY events in the last batch, used for adaptive coalescing.
    /// Small batches (≤2) are keyboard echo — skip coalescing for lower latency.
    last_pty_batch_size: usize,
    /// True when the window is occluded/minimized — skip render to save CPU/GPU.
    window_occluded: bool,
    /// True when the window has OS focus. When false, cursor blink and git polling
    /// are suspended and the event loop parks until a real event arrives (TD-MEM-19).
    window_focused: bool,
    /// Set when only cursor blink changed. Triggers the fast render path that
    /// skips full cell rebuild and uploads only the cursor vertex (cursor overlay).
    cursor_blink_dirty: bool,
    /// Latched by event handlers and flushed once per loop iteration in about_to_wait.
    needs_redraw: bool,

    /// Cached pane separator geometry from the last render frame (TD-PERF-24).
    /// Avoids recomputing separator layout on every CursorMoved event.
    separator_snapshot: Vec<crate::ui::PaneSeparator>,

    /// Cached CWD of the active shell (TD-PERF-02).
    /// Refreshed via proc_pidinfo only when PTY data arrives or terminal focus changes,
    /// instead of every frame.
    cached_cwd: Option<std::path::PathBuf>,

    sidebar: SidebarState,

    /// Current battery status. None until the first poll or on desktops with no battery.
    battery_status: Option<crate::platform::battery::BatteryStatus>,
    /// Timestamp of the last GPU frame submission. Used to enforce max_fps cap.
    last_frame_at: std::time::Instant,
    /// True after the first battery poll attempt, regardless of whether a battery exists.
    /// Prevents re-entering the poll block on every event loop iteration on desktops.
    battery_polled: bool,
    /// Last time battery status was queried via IOKit.
    battery_last_poll: std::time::Instant,
    /// True when battery-saver restrictions are currently active.
    battery_saver_active: bool,

    /// Debounce timer for config hot-reload — actual reload deferred 300 ms after last event (TD-PERF-17).
    config_reload_at: Option<std::time::Instant>,
    /// Watches CWD's `.petruterm/` directory for project-local MCP config changes.
    mcp_watcher: Option<ConfigWatcher>,
    /// Debounce timer for MCP hot-reload, separate from lua config reload.
    mcp_reload_at: Option<std::time::Instant>,
    /// Per-terminal shell context (exit code + last command), keyed by terminal_id.
    /// Stored with the mtime of the context file to skip redundant disk reads when
    /// the file has not changed since the last PTY event (TD-PERF-09).
    terminal_shell_ctxs: std::collections::HashMap<
        usize,
        (
            crate::llm::shell_context::ShellContext,
            std::time::SystemTime,
        ),
    >,
    /// Live Lua VM — kept alive so petruterm.on() callbacks registered in config.lua
    /// can be called at runtime.
    lua: Option<mlua::Lua>,
    /// Active toast notification: (message, expiry). Rendered as an overlay until expiry.
    toast: Option<(String, std::time::Instant)>,

    /// Sidebar item detail overlay (G-2-overlay).
    info_overlay: crate::ui::InfoOverlay,

    /// Cached sorted list of (server_name, [tool_names]) for sidebar render (AUDIT-PERF-03).
    /// Rebuilt lazily on first sidebar frame and after each MCP reload.
    mcp_tools_cache: Vec<(String, Vec<String>)>,
    mcp_tools_dirty: bool,

    /// Terminal cell currently under the cursor that contains a clickable link (H-1).
    hover_link: Option<hover_link::HoverLink>,
    /// Block (terminal_id, block_id) whose gutter the cursor is hovering (B-4).
    hover_block: Option<(usize, usize)>,
}

impl App {
    pub fn new(config: Config, lua: Option<mlua::Lua>, wakeup_proxy: EventLoopProxy<()>) -> Self {
        let config_watcher = config::config_dir()
            .exists()
            .then(|| ConfigWatcher::new(&config::config_dir()).ok())
            .flatten();

        let mcp_watcher = std::env::current_dir()
            .ok()
            .map(|cwd| cwd.join(".petruterm"))
            .filter(|p| p.is_dir())
            .and_then(|p| ConfigWatcher::new(&p).ok());

        Self {
            config: config.clone(),
            config_watcher,
            window: None,
            render_ctx: None,
            mux: Mux::new(),
            ui: UiManager::new(&config, wakeup_proxy.clone()),
            input: InputHandler::new(&config),
            menu: AppMenu::build(),
            wakeup_proxy,
            pending_pty_redraw: false,
            last_pty_activity: std::time::Instant::now(),
            pty_echo_grace_until: None,
            last_pty_batch_size: 0,
            window_occluded: false,
            window_focused: true,
            cursor_blink_dirty: false,
            needs_redraw: false,
            cached_cwd: None,
            sidebar: SidebarState::default(),
            last_frame_at: std::time::Instant::now(),
            battery_status: None,
            battery_polled: false,
            battery_last_poll: std::time::Instant::now(),
            battery_saver_active: false,
            config_reload_at: None,
            mcp_watcher,
            mcp_reload_at: None,
            terminal_shell_ctxs: std::collections::HashMap::new(),
            separator_snapshot: Vec::new(),
            lua,
            toast: None,
            info_overlay: crate::ui::InfoOverlay::new(),
            mcp_tools_cache: Vec::new(),
            mcp_tools_dirty: true,
            hover_link: None,
            hover_block: None,
        }
    }

    /// If the pixel position `(px, py)` is within ±8 physical pixels of a pane
    /// separator, returns the drag state identifying that separator.
    fn separator_at_pixel(&self, px: f32, py: f32) -> Option<input::SeparatorDragState> {
        let viewport = self.viewport_rect();
        let (cell_w, cell_h) = self.cell_dims();
        let (cw, ch) = (cell_w as f32, cell_h as f32);
        for sep in &self.separator_snapshot {
            if sep.vertical {
                let sep_x = viewport.x + sep.col as f32 * cw;
                let row_top = viewport.y + sep.row as f32 * ch;
                let row_bot = row_top + sep.length as f32 * ch;
                if (px - sep_x).abs() <= 8.0 && py >= row_top && py <= row_bot {
                    return Some(input::SeparatorDragState {
                        node_id: sep.node_id,
                    });
                }
            } else {
                let sep_y = viewport.y + sep.row as f32 * ch;
                let col_lft = viewport.x + sep.col as f32 * cw;
                let col_rgt = col_lft + sep.length as f32 * cw;
                if (py - sep_y).abs() <= 8.0 && px >= col_lft && px <= col_rgt {
                    return Some(input::SeparatorDragState {
                        node_id: sep.node_id,
                    });
                }
            }
        }
        None
    }

    fn clamp_sidebar_cursor(&mut self) {
        let count = self.mux.workspace_count();
        self.sidebar.nav_cursor = self.sidebar.nav_cursor.min(count.saturating_sub(1));
    }

    fn sidebar_selected_workspace_id(&self) -> Option<usize> {
        self.mux
            .workspaces()
            .get(self.sidebar.nav_cursor)
            .map(|w| w.id)
    }

    fn close_sidebar(&mut self) {
        self.sidebar.visible = false;
        self.sidebar.rename_input = None;
        self.sidebar.keyboard_active = false;
        self.info_overlay.close();
        self.apply_tab_bar_padding();
        self.resize_terminals_for_panel();
    }

    fn open_sidebar_info_overlay(&mut self) {
        const CONTENT_WIDTH: usize = 72;
        match self.sidebar.active_section {
            1 => {
                // MCP: collect servers in sorted order (mirrors sidebar render)
                let servers: Vec<String> = {
                    let mut map: std::collections::BTreeMap<String, ()> = Default::default();
                    for (server, _) in self.ui.mcp_manager.all_tools() {
                        map.insert(server, ());
                    }
                    map.into_keys().collect()
                };
                if let Some(name) = servers.get(self.sidebar.mcp_scroll) {
                    let content = self.ui.mcp_overlay_content(name);
                    self.info_overlay
                        .open(name.clone(), &content, CONTENT_WIDTH);
                }
            }
            2 => {
                // Skills
                if let Some(skill) = self
                    .ui
                    .skill_manager
                    .skills()
                    .get(self.sidebar.skills_scroll)
                {
                    let title = skill.name.clone();
                    let content = self
                        .ui
                        .skill_manager
                        .read_body(skill)
                        .unwrap_or_else(|e| format!("Error reading skill: {e}"));
                    self.info_overlay.open(title, &content, CONTENT_WIDTH);
                }
            }
            3 => {
                // Steering
                if let Some((name, content)) = self
                    .ui
                    .steering_manager
                    .files()
                    .get(self.sidebar.steering_scroll)
                {
                    let display = name.strip_suffix(".md").unwrap_or(name).to_string();
                    self.info_overlay.open(display, content, CONTENT_WIDTH);
                }
            }
            _ => {}
        }
    }

    fn handle_sidebar_key(&mut self, event: &winit::event::KeyEvent) -> bool {
        if event.state != ElementState::Pressed || !self.sidebar.visible {
            if !self.sidebar.visible {
                self.info_overlay.close();
            }
            return false;
        }

        // Info overlay intercepts all keys when visible.
        if self.info_overlay.visible {
            match &event.logical_key {
                Key::Named(NamedKey::Escape) => {
                    self.info_overlay.close();
                    self.sidebar.keyboard_active = false;
                }
                Key::Named(NamedKey::ArrowDown) => {
                    self.info_overlay.scroll_down();
                }
                Key::Named(NamedKey::ArrowUp) => {
                    self.info_overlay.scroll_up();
                }
                Key::Character(s) if s.as_str() == "j" => {
                    self.info_overlay.scroll_down();
                }
                Key::Character(s) if s.as_str() == "k" => {
                    self.info_overlay.scroll_up();
                }
                _ => {}
            }
            return true;
        }

        // When leader is active, only intercept sidebar-specific final keys.
        // Prefix keys (a, e, W) and all others pass through to normal leader dispatch.
        if self.input.leader_active {
            match &event.logical_key {
                Key::Character(s) if s == "w" => {
                    // leader+w: new workspace (sidebar context).
                    self.input.leader_active = false;
                    self.input.leader_deadline = None;
                    self.clamp_sidebar_cursor();
                    let name = format!("ws{}", self.mux.workspace_count() + 1);
                    let (cols, rows) = self.default_grid_size();
                    let (cell_w, cell_h) = self.cell_dims();
                    let viewport = self.viewport_rect();
                    let cwd = self
                        .mux
                        .active_cwd()
                        .or_else(|| std::env::current_dir().ok());
                    self.mux.cmd_new_workspace(name);
                    self.mux.cmd_new_tab(
                        &self.config,
                        viewport,
                        cols,
                        rows,
                        cell_w,
                        cell_h,
                        self.wakeup_proxy.clone(),
                        cwd,
                    );
                    self.sidebar.nav_cursor = self.mux.workspace_count().saturating_sub(1);
                    self.sidebar.rename_input = None;
                    return true;
                }
                Key::Character(s) if s == "." => {
                    // leader+.: rename selected workspace (mirrors leader+, for RenameTab).
                    self.input.leader_active = false;
                    self.input.leader_deadline = None;
                    self.clamp_sidebar_cursor();
                    if let Some(ws) = self.mux.workspaces().get(self.sidebar.nav_cursor) {
                        self.sidebar.rename_input = Some(ws.name.clone());
                    }
                    return true;
                }
                _ => return false,
            }
        }

        self.clamp_sidebar_cursor();

        if self.sidebar.rename_input.is_some() {
            match &event.logical_key {
                Key::Named(NamedKey::Escape) => {
                    self.sidebar.rename_input = None;
                }
                Key::Named(NamedKey::Enter) => {
                    let name = self
                        .sidebar
                        .rename_input
                        .as_ref()
                        .map(|s| s.trim().to_string())
                        .unwrap_or_default();
                    if let Some(id) = self.sidebar_selected_workspace_id() {
                        if !name.is_empty() {
                            self.mux.cmd_rename_workspace_id(id, name);
                        }
                    }
                    self.sidebar.rename_input = None;
                }
                Key::Named(NamedKey::Backspace) => {
                    if let Some(input) = self.sidebar.rename_input.as_mut() {
                        input.pop();
                    }
                }
                Key::Named(NamedKey::Space) => {
                    if let Some(input) = self.sidebar.rename_input.as_mut() {
                        input.push(' ');
                    }
                }
                Key::Character(s) => {
                    if let Some(input) = self.sidebar.rename_input.as_mut() {
                        input.push_str(s);
                    }
                }
                _ => {}
            }
            return true;
        }

        match &event.logical_key {
            // Escape: close sidebar.
            Key::Named(NamedKey::Escape) => {
                self.close_sidebar();
                true
            }
            // Tab / Shift+Tab: cycle active section.
            Key::Named(NamedKey::Tab) => {
                let shift = self.input.modifiers.state().shift_key();
                self.sidebar.active_section = if shift {
                    (self.sidebar.active_section + 3) % 4
                } else {
                    (self.sidebar.active_section + 1) % 4
                };
                self.sidebar.keyboard_active = true;
                true
            }
            // ArrowDown / j: move down within the active section.
            Key::Named(NamedKey::ArrowDown) => {
                match self.sidebar.active_section {
                    0 => {
                        let max = self.mux.workspace_count().saturating_sub(1);
                        self.sidebar.nav_cursor = (self.sidebar.nav_cursor + 1).min(max);
                    }
                    1 => self.sidebar.mcp_scroll = self.sidebar.mcp_scroll.saturating_add(1),
                    2 => self.sidebar.skills_scroll = self.sidebar.skills_scroll.saturating_add(1),
                    3 => {
                        self.sidebar.steering_scroll =
                            self.sidebar.steering_scroll.saturating_add(1)
                    }
                    _ => {}
                }
                self.sidebar.keyboard_active = true;
                true
            }
            // ArrowUp / k: move up within the active section.
            Key::Named(NamedKey::ArrowUp) => {
                match self.sidebar.active_section {
                    0 => self.sidebar.nav_cursor = self.sidebar.nav_cursor.saturating_sub(1),
                    1 => self.sidebar.mcp_scroll = self.sidebar.mcp_scroll.saturating_sub(1),
                    2 => self.sidebar.skills_scroll = self.sidebar.skills_scroll.saturating_sub(1),
                    3 => {
                        self.sidebar.steering_scroll =
                            self.sidebar.steering_scroll.saturating_sub(1)
                    }
                    _ => {}
                }
                self.sidebar.keyboard_active = true;
                true
            }
            Key::Character(s) if s.as_str() == "j" => {
                match self.sidebar.active_section {
                    0 => {
                        let max = self.mux.workspace_count().saturating_sub(1);
                        self.sidebar.nav_cursor = (self.sidebar.nav_cursor + 1).min(max);
                    }
                    1 => self.sidebar.mcp_scroll = self.sidebar.mcp_scroll.saturating_add(1),
                    2 => self.sidebar.skills_scroll = self.sidebar.skills_scroll.saturating_add(1),
                    3 => {
                        self.sidebar.steering_scroll =
                            self.sidebar.steering_scroll.saturating_add(1)
                    }
                    _ => {}
                }
                self.sidebar.keyboard_active = true;
                true
            }
            Key::Character(s) if s.as_str() == "k" => {
                match self.sidebar.active_section {
                    0 => self.sidebar.nav_cursor = self.sidebar.nav_cursor.saturating_sub(1),
                    1 => self.sidebar.mcp_scroll = self.sidebar.mcp_scroll.saturating_sub(1),
                    2 => self.sidebar.skills_scroll = self.sidebar.skills_scroll.saturating_sub(1),
                    3 => {
                        self.sidebar.steering_scroll =
                            self.sidebar.steering_scroll.saturating_sub(1)
                    }
                    _ => {}
                }
                self.sidebar.keyboard_active = true;
                true
            }
            // Enter: confirm workspace switch (section 0) or placeholder for future overlay.
            Key::Named(NamedKey::Enter) if self.sidebar.keyboard_active => {
                if self.sidebar.active_section == 0 {
                    if let Some(id) = self.sidebar_selected_workspace_id() {
                        self.mux.cmd_switch_workspace(id);
                        self.refresh_status_cache();
                    }
                    self.close_sidebar();
                } else {
                    self.open_sidebar_info_overlay();
                }
                true
            }
            Key::Character(s) if s == "&" => {
                if let Some(id) = self.sidebar_selected_workspace_id() {
                    let prev_active = self.mux.active_workspace_id;
                    self.mux.cmd_close_workspace_id(id);
                    self.clamp_sidebar_cursor();
                    if self.mux.active_workspace_id != prev_active {
                        self.refresh_status_cache();
                    }
                    self.apply_tab_bar_padding();
                    self.resize_terminals_for_panel();
                }
                true
            }
            _ => false,
        }
    }

    fn handle_keyboard(&mut self, event_loop: &ActiveEventLoop, event: winit::event::KeyEvent) {
        if self.handle_sidebar_key(&event) {
            self.request_redraw();
            return;
        }
        // B-4: leader+y / leader+r — block operations requiring App-level hover state.
        if event.state == ElementState::Pressed && self.input.leader_active {
            if let winit::keyboard::Key::Character(s) = &event.logical_key {
                match s.as_str() {
                    "y" => {
                        self.input.leader_active = false;
                        self.input.leader_deadline = None;
                        self.copy_hover_block_output();
                        self.request_redraw();
                        return;
                    }
                    "r" => {
                        self.input.leader_active = false;
                        self.input.leader_deadline = None;
                        self.rerun_hover_block_command();
                        self.request_redraw();
                        return;
                    }
                    _ => {}
                }
            }
        }
        let panel_was_visible = self.ui.is_panel_visible();
        let tab_count_before = self.mux.tabs.tab_count();
        let tab_idx_before = self.mux.active_tab_index();
        let pane_count_before = self.mux.active_pane_count();
        self.ui.set_active_terminal(self.mux.focused_terminal_id());
        self.input.handle_key_input(
            &event,
            event_loop,
            &mut self.config,
            &mut self.mux,
            &mut self.ui,
            &mut self.render_ctx,
            self.window.as_deref(),
            self.wakeup_proxy.clone(),
        );
        // Clean up per-terminal state for any panes/tabs closed by input (TD-MEM-08).
        for tid in self.mux.closed_ids.drain(..) {
            self.terminal_shell_ctxs.remove(&tid);
            self.ui.remove_terminal_state(tid);
            if let Some(rc) = &mut self.render_ctx {
                rc.row_caches.remove(&tid);
            }
        }
        self.dispatch_lua_events();
        if self.ui.is_panel_visible() != panel_was_visible {
            self.resize_terminals_for_panel();
        }
        if self.mux.tabs.tab_count() != tab_count_before {
            self.apply_tab_bar_padding();
            self.resize_terminals_for_panel();
        } else if self.mux.active_tab_index() != tab_idx_before {
            // Tab switched — resize the newly active tab's panes.
            self.resize_terminals_for_panel();
            // Different tab = different shell process = potentially different CWD.
            self.refresh_status_cache();
        } else if self.mux.active_pane_count() != pane_count_before {
            // Pane split or close — resize all panes in current tab.
            self.resize_terminals_for_panel();
        } else if self.input.pane_ratio_adjusted {
            // <leader>+Option+Arrow pane resize — resize with new ratio.
            self.input.pane_ratio_adjusted = false;
            self.resize_terminals_for_panel();
        }
        if self.input.toggle_sidebar_requested {
            self.input.toggle_sidebar_requested = false;
            self.sidebar.visible = !self.sidebar.visible;
            self.sidebar.rename_input = None;
            self.sidebar.keyboard_active = false;
            self.clamp_sidebar_cursor();
            self.apply_tab_bar_padding();
            self.resize_terminals_for_panel();
        }
        // A key may have written to the PTY (typing, or an atuin/ZLE widget that
        // rewrites the line asynchronously). Guard against a lost echo wakeup.
        self.note_pty_input();
        self.request_redraw();
    }

    fn handle_mouse_motion(&mut self, position: winit::dpi::PhysicalPosition<f64>) {
        self.input.mouse_pos = (position.x, position.y);
        let (col, row) = self.pixel_to_cell(position.x, position.y);
        // Update context menu hover — redraw if hovered item changed.
        if self.ui.context_menu.update_hover(col, row) {
            self.request_redraw();
        }
        // W-5: zero state pill hover tracking.
        if let Some((_, panel_row)) = self.panel_hit_cell(position.x, position.y) {
            let panel = self.ui.panel();
            if panel.messages.is_empty()
                && matches!(panel.state, crate::llm::chat_panel::PanelState::Idle)
            {
                let new_hover = self.zero_state_hover_for_row(panel_row);
                if panel.zero_state_hover != new_hover {
                    self.ui.panel_mut().zero_state_hover = new_hover;
                    self.ui.panel_mut().dirty = true;
                    self.request_redraw();
                }
            } else if self.ui.panel().zero_state_hover.is_some() {
                self.ui.panel_mut().zero_state_hover = None;
                self.ui.panel_mut().dirty = true;
            }
        } else if self.ui.panel().zero_state_hover.is_some() {
            self.ui.panel_mut().zero_state_hover = None;
            self.ui.panel_mut().dirty = true;
        }
        // W-7: suggestion pill hover tracking.
        if let Some((_, panel_row)) = self.panel_hit_cell(position.x, position.y) {
            let panel = self.ui.panel();
            if panel.show_suggestions
                && !panel.messages.is_empty()
                && matches!(panel.state, crate::llm::chat_panel::PanelState::Idle)
            {
                let new_hover = self.suggestion_hover_for_row(panel_row);
                if panel.suggestion_hover != new_hover {
                    self.ui.panel_mut().suggestion_hover = new_hover;
                    self.ui.panel_mut().dirty = true;
                    self.request_redraw();
                }
            } else if self.ui.panel().suggestion_hover.is_some() {
                self.ui.panel_mut().suggestion_hover = None;
                self.ui.panel_mut().dirty = true;
            }
        } else if self.ui.panel().suggestion_hover.is_some() {
            self.ui.panel_mut().suggestion_hover = None;
            self.ui.panel_mut().dirty = true;
        }
        // W-8: panel resize drag — update width live.
        if self.sidebar.panel_resize_drag {
            if let Some(rc) = &self.render_ctx {
                let (win_w, _) = rc.renderer.size();
                let right_edge = win_w as f32 - self.config.window.padding.right as f32;
                let cell_w = self.cell_dims().0 as f32;
                let new_cols = ((right_edge - position.x as f32) / cell_w).clamp(30.0, 90.0) as u16;
                if self.ui.panel().width_cols != new_cols {
                    self.ui.panel_mut().width_cols = new_cols;
                    self.ui.panel_mut().dirty = true;
                    self.resize_terminals_for_panel();
                    self.request_redraw();
                }
            }
        }
        // W-8: hover over panel left-edge resize handle.
        {
            let near = self.near_panel_left_edge(position.x);
            if near != self.sidebar.panel_resize_hover {
                self.sidebar.panel_resize_hover = near;
                self.request_redraw();
            }
        }
        // H-1: hover link detection — only in terminal area (not panel/sidebar/menu).
        {
            let in_terminal =
                !self.mouse_in_panel() && !self.sidebar.visible && !self.ui.context_menu.visible;
            let new_link = if in_terminal {
                let row_text = self.mux.viewport_row_text(row);
                hover_link::scan_link_at(&row_text, col).map(|(cs, ce, kind, text)| {
                    hover_link::HoverLink {
                        row,
                        col_start: cs,
                        col_end: ce,
                        kind,
                        text,
                    }
                })
            } else {
                None
            };
            if new_link != self.hover_link {
                self.hover_link = new_link;
                self.request_redraw();
            }
        }
        // B-4: block gutter hover detection.
        {
            let in_terminal =
                !self.mouse_in_panel() && !self.sidebar.visible && !self.ui.context_menu.visible;
            let new_hover_block = if in_terminal {
                self.block_at_cursor(position.x as f32, position.y as f32)
            } else {
                None
            };
            if new_hover_block != self.hover_block {
                self.hover_block = new_hover_block;
                self.request_redraw();
            }
        }
        // Separator drag — update ratio live.
        if let Some(drag) = &self.input.dragging_separator {
            let node_id = drag.node_id;
            self.mux
                .cmd_drag_separator(node_id, position.x as f32, position.y as f32);
            self.resize_terminals_for_panel();
            self.request_redraw();
        } else if self.input.mouse_left_pressed
            && !self.mouse_in_panel()
            && !self.sidebar.panel_resize_drag
        {
            let dx = position.x - self.input.mouse_press_pos.0;
            let dy = position.y - self.input.mouse_press_pos.1;
            // Only treat as a drag once the cursor moves at least 4 physical pixels.
            // This prevents trackpad micro-jitter from creating lingering selections.
            if dx * dx + dy * dy >= 16.0 {
                if let Some(terminal) = self.mux.active_terminal() {
                    terminal.update_selection(col, row);
                    let (any_mouse, _, motion) = terminal.mouse_mode_flags();
                    if any_mouse && motion {
                        self.input.send_mouse_report(32, col, row, true, &self.mux);
                    }
                }
                self.input.mouse_dragged = true;
                self.request_redraw();
            }
        }
    }

    fn handle_mouse_button(&mut self, state: ElementState, button: MouseButton) {
        let in_panel = self.mouse_in_panel();
        let (col, row) = self.pixel_to_cell(self.input.mouse_pos.0, self.input.mouse_pos.1);
        match (button, state) {
            (MouseButton::Left, ElementState::Pressed) => {
                if self.sidebar.visible {
                    let sf = self
                        .render_ctx
                        .as_ref()
                        .map(|rc| rc.scale_factor as f64)
                        .unwrap_or(1.0);
                    // Sidebar occupies the visual column area only (not the margin).
                    // R-8: the sidebar floats by the content inset, so shift the hit
                    // box right by the same amount.
                    let inset = self.content_inset() as f64;
                    let sidebar_visual_right = self.config.window.padding.left as f64
                        + inset
                        + SIDEBAR_COLS as f64 * self.cell_dims().0 as f64;
                    let sidebar_left = self.config.window.padding.left as f64 + inset;
                    // Only intercept clicks that are (a) inside the visual sidebar area
                    // and (b) below the titlebar so that the titlebar toggle button
                    // remains reachable.
                    let titlebar_bottom = TITLEBAR_HEIGHT as f64 * sf;
                    if self.input.mouse_pos.0 >= sidebar_left
                        && self.input.mouse_pos.0 < sidebar_visual_right
                        && self.input.mouse_pos.1 >= titlebar_bottom
                    {
                        let (_, cell_h) = self.cell_dims();
                        let sidebar_top = self.config.window.padding.top as f64
                            + self.tab_bar_height_px() as f64
                            + inset;
                        let header_bottom = sidebar_top + cell_h as f64;
                        // Click in the header row → create new workspace.
                        if self.input.mouse_pos.1 < header_bottom {
                            let name = format!("ws{}", self.mux.workspace_count() + 1);
                            let (cols, rows) = self.default_grid_size();
                            let (cell_w, cell_h2) = self.cell_dims();
                            let viewport = self.viewport_rect();
                            let cwd = self
                                .mux
                                .active_cwd()
                                .or_else(|| std::env::current_dir().ok());
                            self.mux.cmd_new_workspace(name);
                            self.mux.cmd_new_tab(
                                &self.config,
                                viewport,
                                cols,
                                rows,
                                cell_w,
                                cell_h2,
                                self.wakeup_proxy.clone(),
                                cwd,
                            );
                            self.sidebar.nav_cursor = self.mux.workspace_count().saturating_sub(1);
                            self.apply_tab_bar_padding();
                            self.resize_terminals_for_panel();
                            self.request_redraw();
                            return;
                        }
                        let list_top = sidebar_top + 2.0 * cell_h as f64;
                        let rel_y = self.input.mouse_pos.1 - list_top;
                        if rel_y >= 0.0 {
                            let idx = (rel_y / (2.0 * cell_h as f64)).floor() as usize;
                            if idx < self.mux.workspace_count() {
                                self.sidebar.nav_cursor = idx;
                                // usize::MAX sentinel distinguishes sidebar clicks
                                // from terminal cell clicks in register_click.
                                let clicks = self.input.register_click((idx, usize::MAX));
                                if clicks >= 2 {
                                    // Double-click: enter rename mode.
                                    if let Some(ws) = self.mux.workspaces().get(idx) {
                                        self.sidebar.rename_input = Some(ws.name.clone());
                                    }
                                } else {
                                    // Single click: switch workspace and close.
                                    self.sidebar.rename_input = None;
                                    if let Some(id) = self.sidebar_selected_workspace_id() {
                                        self.mux.cmd_switch_workspace(id);
                                        self.refresh_status_cache();
                                    }
                                    self.close_sidebar();
                                }
                            }
                        }
                        self.request_redraw();
                        return;
                    }
                }
                // Context menu: consume click if it lands inside the menu.
                if self.ui.context_menu.visible {
                    if let Some(action) = self.ui.context_menu.hit_test(col, row) {
                        self.ui.context_menu.close();
                        match action {
                            ContextAction::Copy => {
                                if let Some(terminal) = self.mux.active_terminal() {
                                    if let Some(text) = terminal.selection_text() {
                                        std::thread::spawn(move || {
                                            let _ = arboard::Clipboard::new()
                                                .and_then(|mut cb| cb.set_text(text));
                                        });
                                    }
                                }
                            }
                            ContextAction::Paste => {
                                self.ui.request_paste_async(self.wakeup_proxy.clone());
                            }
                            ContextAction::Clear => {
                                if let Some(terminal) = self.mux.active_terminal() {
                                    terminal.write_input(b"clear\n");
                                }
                            }
                            ContextAction::SendToChat => {
                                let selected =
                                    self.mux.active_terminal().and_then(|t| t.selection_text());
                                if let Some(text) = selected {
                                    let terminal_id = self.mux.focused_terminal_id();
                                    let cwd = self
                                        .mux
                                        .active_cwd()
                                        .or_else(|| std::env::current_dir().ok())
                                        .unwrap_or_default();
                                    self.ui.open_panel_with_context(terminal_id, cwd);
                                    self.ui.panel_mut().set_input(text);
                                    self.resize_terminals_for_panel();
                                }
                            }
                            ContextAction::CopyLastCommand => {
                                if let Some(cmd) =
                                    self.active_shell_ctx().map(|c| c.last_command.clone())
                                {
                                    if !cmd.is_empty() {
                                        std::thread::spawn(move || {
                                            let _ = arboard::Clipboard::new()
                                                .and_then(|mut cb| cb.set_text(cmd));
                                        });
                                    }
                                }
                            }
                            ContextAction::OpenLink(url) => {
                                let open_arg = if url.starts_with('/')
                                    || url.starts_with("./")
                                    || url.starts_with("../")
                                {
                                    hover_link::path_for_open(&url).to_string()
                                } else {
                                    url
                                };
                                std::thread::spawn(move || {
                                    let _ =
                                        std::process::Command::new("open").arg(&open_arg).spawn();
                                });
                            }
                            ContextAction::CopyLink(url) => {
                                std::thread::spawn(move || {
                                    let _ = arboard::Clipboard::new()
                                        .and_then(|mut cb| cb.set_text(url));
                                });
                            }
                            // B-4: block context menu actions.
                            ContextAction::CopyBlockOutput(tid, bid) => {
                                if let Some(text) = self.mux.block_output_text(tid, bid) {
                                    if !text.is_empty() {
                                        std::thread::spawn(move || {
                                            let _ = arboard::Clipboard::new()
                                                .and_then(|mut cb| cb.set_text(text));
                                        });
                                    }
                                }
                            }
                            ContextAction::ReRunCommand(cmd) => {
                                if !cmd.is_empty() {
                                    if let Some(terminal) = self.mux.active_terminal() {
                                        let input = format!("{}\n", cmd);
                                        terminal.write_input(input.as_bytes());
                                    }
                                }
                            }
                            ContextAction::SetTabColor(idx, color) => {
                                self.mux.tabs.set_tab_color(idx, color);
                                if let Some(rc) = &mut self.render_ctx {
                                    rc.tab_bar_instances_cache.clear();
                                }
                            }
                            ContextAction::Separator | ContextAction::Label => {}
                        }
                        self.request_redraw();
                        return;
                    } else {
                        // Click outside menu closes it.
                        self.ui.context_menu.close();
                        self.request_redraw();
                    }
                }

                if self.config.window.title_bar_style == TitleBarStyle::Custom
                    && self.input.mouse_pos.1
                        < TITLEBAR_HEIGHT as f64
                            * self
                                .render_ctx
                                .as_ref()
                                .map(|rc| rc.scale_factor as f64)
                                .unwrap_or(1.0)
                {
                    let sf = self
                        .render_ctx
                        .as_ref()
                        .map(|rc| rc.scale_factor as f64)
                        .unwrap_or(1.0);
                    let x = self.input.mouse_pos.0;
                    // Sidebar toggle button: logical [80..102] scaled to physical
                    if x >= 80.0 * sf && x < 102.0 * sf {
                        self.sidebar.visible = !self.sidebar.visible;
                        self.sidebar.rename_input = None;
                        self.sidebar.keyboard_active = false;
                        self.clamp_sidebar_cursor();
                        self.apply_tab_bar_padding();
                        self.resize_terminals_for_panel();
                        self.request_redraw();
                        return;
                    }
                    // AI panel toggle button: logical [106..128]
                    if x >= 106.0 * sf && x < 128.0 * sf {
                        let terminal_id = self.mux.focused_terminal_id();
                        if self.ui.is_panel_visible() {
                            self.ui.panel_mut().close();
                            self.ui.panel_focused = false;
                            self.ui.file_picker_focused = false;
                        } else {
                            let cwd = self
                                .mux
                                .active_cwd()
                                .or_else(|| std::env::current_dir().ok())
                                .unwrap_or_default();
                            self.ui.open_panel_with_context(terminal_id, cwd);
                        }
                        self.resize_terminals_for_panel();
                        self.request_redraw();
                        return;
                    }
                    // Tab click
                    if let Some(idx) = self.hit_test_tab_bar(x) {
                        self.mux.tabs.switch_to_index(idx);
                        self.resize_terminals_for_panel();
                        self.refresh_status_cache();
                        self.request_redraw();
                        return;
                    }
                    // Default: drag window
                    if let Some(w) = &self.window {
                        let _ = w.drag_window();
                    }
                    return;
                }
                // Fallback for Native/None titlebar styles: old logic
                if self.input.mouse_pos.1 < self.config.window.padding.top as f64 {
                    if let Some(w) = &self.window {
                        let _ = w.drag_window();
                    }
                    return;
                }
                // Tab bar click — switch tab without passing event to terminal.
                let tab_h = self.tab_bar_height_px() as f64;
                if tab_h > 0.0
                    && self.input.mouse_pos.1 < self.config.window.padding.top as f64 + tab_h
                {
                    if let Some(idx) = self.hit_test_tab_bar(self.input.mouse_pos.0) {
                        self.mux.tabs.switch_to_index(idx);
                        self.resize_terminals_for_panel();
                        self.refresh_status_cache();
                    }
                    self.request_redraw();
                    return;
                }
                // Status bar click — hit-test segments.
                if self.config.status_bar.enabled {
                    // Use the same row-based math as the renderer so the hit zone
                    // aligns exactly with the drawn bar regardless of viewport floor() rounding.
                    let (cell_w, cell_h_u) = self.cell_dims();
                    let cell_h = cell_h_u as f64;
                    let win_h = self
                        .render_ctx
                        .as_ref()
                        .map(|rc| rc.renderer.size().1 as f64)
                        .unwrap_or(0.0);
                    let pad_top = self.config.window.padding.top as f64;
                    let pad_bottom = self.config.window.padding.bottom as f64;
                    let tab_h = self.tab_bar_height_px() as f64;
                    let sb_h = self.status_bar_height_px() as f64;
                    let viewport_h = (win_h - pad_top - pad_bottom - tab_h - sb_h).max(0.0);
                    let total_sb_rows = (viewport_h / cell_h).floor() as usize;
                    let sb_top = pad_top + tab_h + total_sb_rows as f64 * cell_h;
                    let sb_bottom = sb_top + cell_h;
                    if self.input.mouse_pos.1 >= sb_top && self.input.mouse_pos.1 < sb_bottom {
                        let col = ((self.input.mouse_pos.0
                            - self.config.window.padding.left as f64)
                            / cell_w as f64)
                            .floor()
                            .max(0.0) as usize;
                        let total_cols = self.mux.active_terminal_size().0;
                        let cwd = self.mux.active_cwd();
                        let git_branch = self.ui.git_branch_cache.clone();
                        let sb_colors = self.config.colors.status_bar_colors();
                        let bar = crate::ui::status_bar::StatusBar::build(
                            false,
                            false,
                            &self.config.leader.key,
                            cwd.as_deref(),
                            git_branch.as_deref(),
                            None,
                            false,
                            self.config.status_bar.style.clone(),
                            None,
                            &sb_colors,
                        );
                        match bar.click_kind(col, total_cols) {
                            Some(crate::ui::status_bar::SegmentKind::GitBranch) => {
                                if let Some(cwd_path) = self
                                    .mux
                                    .active_cwd()
                                    .or_else(|| std::env::current_dir().ok())
                                {
                                    self.ui.open_branch_picker(&cwd_path);
                                    self.request_redraw();
                                }
                            }
                            Some(crate::ui::status_bar::SegmentKind::ExitCode) => {
                                if let Some(ctx) = self.active_shell_ctx() {
                                    if ctx.last_exit_code != 0 {
                                        let (exit_code, last_cmd) =
                                            (ctx.last_exit_code, ctx.last_command.clone());
                                        let (term_cols, term_rows) =
                                            self.mux.active_terminal_size();
                                        self.ui.context_menu.open_exit_info(
                                            exit_code, &last_cmd, col, term_rows, term_cols,
                                        );
                                        self.request_redraw();
                                    }
                                }
                            }
                            _ => {}
                        }
                        return;
                    }
                }

                // W-8: panel left-edge resize drag.
                if self.near_panel_left_edge(self.input.mouse_pos.0) {
                    self.sidebar.panel_resize_drag = true;
                    self.request_redraw();
                    return;
                }
                // Separator drag: if click is within ±3px of a separator, start drag.
                let sep_hit = if !in_panel {
                    self.separator_at_pixel(
                        self.input.mouse_pos.0 as f32,
                        self.input.mouse_pos.1 as f32,
                    )
                } else {
                    None
                };
                if let Some(drag_state) = sep_hit {
                    self.input.dragging_separator = Some(drag_state);
                    self.request_redraw();
                    return;
                }

                if in_panel {
                    if let Some((panel_col, panel_row)) =
                        self.panel_hit_cell(self.input.mouse_pos.0, self.input.mouse_pos.1)
                    {
                        if panel_row == 0 {
                            let action = crate::llm::chat_panel::header_action_for_col(
                                self.ui.panel().width_cols as usize,
                                panel_col,
                                !self.ui.panel().messages.is_empty(),
                            );
                            if let Some(action) = action {
                                match action {
                                    crate::llm::chat_panel::HeaderAction::Restart => {
                                        self.ui.restart_chat_panel();
                                        self.ui.panel_focused = true;
                                    }
                                    crate::llm::chat_panel::HeaderAction::Copy => {
                                        self.ui.copy_chat_panel_transcript();
                                        self.ui.panel_focused = true;
                                    }
                                    crate::llm::chat_panel::HeaderAction::Close => {
                                        self.ui.close_panel();
                                    }
                                }
                                self.request_redraw();
                                return;
                            }
                        }
                    }
                    // W-5: zero state pill click — pre-fill and submit.
                    {
                        let panel = self.ui.panel();
                        if panel.messages.is_empty()
                            && matches!(panel.state, crate::llm::chat_panel::PanelState::Idle)
                        {
                            let panel_row = self
                                .panel_hit_cell(self.input.mouse_pos.0, self.input.mouse_pos.1)
                                .map(|(_, row)| row)
                                .unwrap_or(0);
                            let pill_hit = self.zero_state_hover_for_row(panel_row);
                            if let Some(idx) = pill_hit {
                                let text = if idx == 0 {
                                    "fix last error"
                                } else {
                                    "explain command"
                                };
                                self.ui.panel_mut().input = text.to_string();
                                self.ui.panel_mut().input_cursor = text.chars().count();
                                let cwd = self
                                    .mux
                                    .active_cwd()
                                    .or_else(|| std::env::current_dir().ok())
                                    .unwrap_or_default();
                                self.ui.submit_ai_query(self.wakeup_proxy.clone(), cwd);
                                self.ui.panel_focused = true;
                                self.request_redraw();
                                return;
                            }
                        }
                    }
                    // W-7: suggestion pill click — pre-fill and submit.
                    {
                        let panel = self.ui.panel();
                        if panel.show_suggestions
                            && !panel.messages.is_empty()
                            && matches!(panel.state, crate::llm::chat_panel::PanelState::Idle)
                        {
                            let panel_row = self
                                .panel_hit_cell(self.input.mouse_pos.0, self.input.mouse_pos.1)
                                .map(|(_, row)| row)
                                .unwrap_or(0);
                            let pill_hit = self.suggestion_hover_for_row(panel_row);
                            if let Some(idx) = pill_hit {
                                let text = if idx == 0 {
                                    "fix last error"
                                } else {
                                    "explain more"
                                };
                                self.ui.panel_mut().input = text.to_string();
                                self.ui.panel_mut().input_cursor = text.chars().count();
                                let cwd = self
                                    .mux
                                    .active_cwd()
                                    .or_else(|| std::env::current_dir().ok())
                                    .unwrap_or_default();
                                self.ui.submit_ai_query(self.wakeup_proxy.clone(), cwd);
                                self.ui.panel_focused = true;
                                self.request_redraw();
                                return;
                            }
                        }
                    }
                    self.ui.panel_focused = true;
                } else {
                    if self.ui.is_panel_visible() {
                        self.ui.panel_focused = false;
                        self.ui.file_picker_focused = false;
                    }
                    // Multi-pane: focus the pane under the cursor.
                    {
                        let (px, py) =
                            (self.input.mouse_pos.0 as f32, self.input.mouse_pos.1 as f32);
                        let tab_idx = self.mux.active_tab_index();
                        if let Some(pane_mgr) = self.mux.panes.get_mut(tab_idx) {
                            pane_mgr.focus_at(px, py);
                            let new_tid = self.mux.focused_terminal_id();
                            self.ui.set_active_terminal(new_tid);
                            // Different pane = different shell process = potentially different CWD.
                            self.refresh_status_cache();
                        }
                    }
                    self.input.mouse_left_pressed = true;
                    self.input.mouse_dragged = false;
                    self.input.mouse_press_pos = self.input.mouse_pos;
                    // H-1: open hovered link on click (takes priority over selection).
                    if let Some(link) = self.hover_link.clone() {
                        if !in_panel
                            && row == link.row
                            && col >= link.col_start
                            && col < link.col_end
                        {
                            let text = link.text.clone();
                            let open_arg = match link.kind {
                                hover_link::HoverLinkKind::Path => {
                                    hover_link::path_for_open(&text).to_string()
                                }
                                hover_link::HoverLinkKind::Url => text,
                            };
                            std::thread::spawn(move || {
                                let _ = std::process::Command::new("open").arg(&open_arg).spawn();
                            });
                            if let Some(terminal) = self.mux.active_terminal() {
                                terminal.clear_selection();
                            }
                            self.request_redraw();
                            return;
                        }
                    }
                    if !self
                        .mux
                        .active_terminal()
                        .map(|t| t.mouse_mode_flags().0)
                        .unwrap_or(false)
                    {
                        let clicks = self.input.register_click((col, row));
                        let sel_type = match clicks {
                            2 => SelectionType::Semantic,
                            3 => SelectionType::Lines,
                            _ => SelectionType::Simple,
                        };
                        if let Some(terminal) = self.mux.active_terminal() {
                            terminal.start_selection(col, row, sel_type);
                        }
                    }
                    self.input.send_mouse_report(0, col, row, true, &self.mux);
                }
            }
            (MouseButton::Left, ElementState::Released) => {
                self.input.mouse_left_pressed = false;
                // W-8: panel resize drag end.
                if self.sidebar.panel_resize_drag {
                    self.sidebar.panel_resize_drag = false;
                    self.resize_terminals_for_panel();
                    self.request_redraw();
                }
                if self.input.dragging_separator.take().is_some() {
                    // Separator drag ended — resize terminals to new pane dimensions.
                    self.resize_terminals_for_panel();
                } else if !in_panel {
                    // Plain click (no drag): clear the 1-cell selection alacritty
                    // starts on press, otherwise that cell lingers with inverted
                    // colours (white bg where the cursor was).
                    if !self.input.mouse_dragged {
                        if let Some(terminal) = self.mux.active_terminal() {
                            if !terminal.mouse_mode_flags().0 {
                                terminal.clear_selection();
                                self.request_redraw();
                            }
                        }
                    }
                    self.input.send_mouse_report(0, col, row, false, &self.mux);
                }
            }
            // Right-click on tab bar → open tab color picker.
            (MouseButton::Right, ElementState::Pressed) if !in_panel => {
                let tab_h = self.tab_bar_height_px() as f64;
                let y_px = self.input.mouse_pos.1;
                let pad_top = self.config.window.padding.top as f64;
                if tab_h > 0.0 && y_px >= pad_top && y_px < pad_top + tab_h {
                    if let Some(tab_idx) = self.hit_test_tab_bar(self.input.mouse_pos.0) {
                        let (term_cols, term_rows) = self.mux.active_terminal_size();
                        let brights = &self.config.colors.brights;
                        self.ui.context_menu.open_tab_color_picker(
                            tab_idx, brights, col, row, term_cols, term_rows,
                        );
                        self.request_redraw();
                        return;
                    }
                }
                let (any_mouse, _, _) = self
                    .mux
                    .active_terminal()
                    .map(|t| t.mouse_mode_flags())
                    .unwrap_or((false, false, false));
                if any_mouse {
                    self.input.send_mouse_report(2, col, row, true, &self.mux);
                } else {
                    let (term_cols, term_rows) = self.mux.active_terminal_size();
                    // H-1: show link-specific context menu when right-clicking a link.
                    let link_under_cursor = self
                        .hover_link
                        .as_ref()
                        .filter(|l| row == l.row && col >= l.col_start && col < l.col_end);
                    // Show link menu, block menu (only on exit-code pill), or default menu.
                    let mx = self.input.mouse_pos.0 as f32;
                    let my = self.input.mouse_pos.1 as f32;
                    if let Some(link) = link_under_cursor {
                        let text = link.text.clone();
                        self.ui
                            .context_menu
                            .open_with_link(text, col, row, term_cols, term_rows);
                    } else if let Some((tid, bid)) = self.block_indicator_at_pixel(mx, my) {
                        let cmd = self
                            .mux
                            .terminals
                            .get(tid)
                            .and_then(|t| t.as_ref())
                            .and_then(|t| t.block_manager.find_block_by_id(bid))
                            .map(|b| b.command_text.clone())
                            .unwrap_or_default();
                        self.ui
                            .context_menu
                            .open_with_block(tid, bid, cmd, col, row, term_cols, term_rows);
                    } else {
                        self.ui
                            .context_menu
                            .open_default(col, row, term_cols, term_rows);
                    }
                }
            }
            (MouseButton::Right, ElementState::Released) => {
                let (any_mouse, _, _) = self
                    .mux
                    .active_terminal()
                    .map(|t| t.mouse_mode_flags())
                    .unwrap_or((false, false, false));
                if !in_panel && any_mouse {
                    self.input.send_mouse_report(2, col, row, false, &self.mux);
                }
            }
            _ => {}
        }
        self.request_redraw();
    }

    fn handle_scroll(&mut self, delta: MouseScrollDelta) {
        let scale = self
            .render_ctx
            .as_ref()
            .map(|rc| rc.scale_factor as f64)
            .unwrap_or(1.0);
        let delta_lines = match delta {
            MouseScrollDelta::LineDelta(_, y) => y as f64,
            // pos.y is in logical points; divide by logical cell height to get lines.
            MouseScrollDelta::PixelDelta(pos) => -pos.y / (self.cell_dims().1 as f64 / scale),
        };
        self.input.scroll_pixel_accum += delta_lines;
        let lines = self.input.scroll_pixel_accum.trunc() as i32;
        self.input.scroll_pixel_accum -= lines as f64;
        if lines == 0 {
            return;
        }
        if self.mouse_in_panel() {
            if lines > 0 {
                self.ui.panel_mut().scroll_down(lines as usize);
            } else {
                self.ui.panel_mut().scroll_up((-lines) as usize);
            }
            self.request_redraw();
            return;
        }
        let (col, row) = self.pixel_to_cell(self.input.mouse_pos.0, self.input.mouse_pos.1);
        let (any_mouse, _, _) = self
            .mux
            .active_terminal()
            .map(|t| t.mouse_mode_flags())
            .unwrap_or((false, false, false));
        if any_mouse {
            let btn = if lines > 0 { 65u8 } else { 64u8 };
            // Cap at 3 events per gesture: each report triggers a full TUI redraw + GPU
            // frame. Sending too many at once causes visible lag on slower hardware (M2).
            let capped = lines.abs().min(3);
            for _ in 0..capped {
                self.input.send_mouse_report(btn, col, row, true, &self.mux);
            }
        } else if let Some(terminal) = self.mux.active_terminal() {
            terminal.scroll_display(-lines);
            if self.input.mouse_left_pressed {
                terminal.update_selection(col, row);
            }
            self.request_redraw();
        }
    }
    #[cfg(target_os = "macos")]
    unsafe fn apply_macos_custom_titlebar(&self, window: &Window) {
        use objc2::msg_send;
        use objc2::runtime::{AnyObject, Bool};
        use winit::raw_window_handle::HasWindowHandle;
        if let Ok(h) = window.window_handle() {
            if let winit::raw_window_handle::RawWindowHandle::AppKit(h) = h.as_raw() {
                let ns_view: &AnyObject = &*(h.ns_view.as_ptr() as *const AnyObject);
                let ns_win_ptr: *mut AnyObject = msg_send![ns_view, window];
                if !ns_win_ptr.is_null() {
                    let ns_win: &AnyObject = &*ns_win_ptr;
                    let current_mask: usize = msg_send![ns_win, styleMask];
                    let () = msg_send![ns_win, setStyleMask: current_mask | (1_usize << 15)];
                    let () = msg_send![ns_win, setTitlebarAppearsTransparent: Bool::YES];
                    let () = msg_send![ns_win, setTitleVisibility: 1_i64];
                    let () = msg_send![ns_win, setMovableByWindowBackground: Bool::NO];
                }
            }
        }
    }

    /// V-1: toggle the render view's CAMetalLayer opacity. wgpu leaves the layer
    /// opaque by default, which discards the clear-color alpha; setting it
    /// non-opaque lets the translucent background and blur composite through.
    #[cfg(target_os = "macos")]
    unsafe fn set_metal_layer_opaque(window: &Window, opaque: bool) {
        use objc2::msg_send;
        use objc2::runtime::{AnyObject, Bool};
        use winit::raw_window_handle::HasWindowHandle;

        let Ok(h) = window.window_handle() else {
            return;
        };
        let winit::raw_window_handle::RawWindowHandle::AppKit(h) = h.as_raw() else {
            return;
        };
        let ns_view: &AnyObject = &*(h.ns_view.as_ptr() as *const AnyObject);
        let layer_ptr: *mut AnyObject = msg_send![ns_view, layer];
        if layer_ptr.is_null() {
            return;
        }
        let flag = if opaque { Bool::YES } else { Bool::NO };
        // wgpu (raw-window-metal) hosts the CAMetalLayer as a sublayer, so the
        // real drawable is not [ns_view layer]. Set opacity on the host layer and
        // every sublayer so the actual Metal layer is covered.
        let layer: &AnyObject = &*layer_ptr;
        let () = msg_send![layer, setOpaque: flag];
        let sublayers_ptr: *mut AnyObject = msg_send![layer, sublayers];
        if !sublayers_ptr.is_null() {
            let count: usize = msg_send![sublayers_ptr, count];
            for i in 0..count {
                let sub_ptr: *mut AnyObject = msg_send![sublayers_ptr, objectAtIndex: i];
                if !sub_ptr.is_null() {
                    let sub: &AnyObject = &*sub_ptr;
                    let () = msg_send![sub, setOpaque: flag];
                }
            }
        }
    }

    /// V-3: round the corners of a borderless window (`title_bar_style = "none"`).
    /// Native and custom titlebars already inherit the system window shape; a
    /// decorations-off window is a square NSWindow, so we clip the content view's
    /// layer to a rounded rect and clear the window background so the corners are
    /// transparent. `masksToBounds` + `cornerRadius` also clips the wgpu
    /// CAMetalLayer sublayer, so the GPU content respects the radius.
    #[cfg(target_os = "macos")]
    unsafe fn apply_macos_rounded_corners(window: &Window, radius: f64) {
        use objc2::runtime::{AnyObject, Bool};
        use objc2::{class, msg_send};
        use winit::raw_window_handle::HasWindowHandle;

        let Ok(h) = window.window_handle() else {
            return;
        };
        let winit::raw_window_handle::RawWindowHandle::AppKit(h) = h.as_raw() else {
            return;
        };
        let ns_view: &AnyObject = &*(h.ns_view.as_ptr() as *const AnyObject);
        let ns_win_ptr: *mut AnyObject = msg_send![ns_view, window];
        if ns_win_ptr.is_null() {
            return;
        }
        let ns_win: &AnyObject = &*ns_win_ptr;
        let () = msg_send![ns_win, setOpaque: Bool::NO];
        let clear: *mut AnyObject = msg_send![class!(NSColor), clearColor];
        if !clear.is_null() {
            let () = msg_send![ns_win, setBackgroundColor: clear];
        }
        let content_ptr: *mut AnyObject = msg_send![ns_win, contentView];
        if content_ptr.is_null() {
            return;
        }
        let content: &AnyObject = &*content_ptr;
        let () = msg_send![content, setWantsLayer: Bool::YES];
        let layer_ptr: *mut AnyObject = msg_send![content, layer];
        if layer_ptr.is_null() {
            return;
        }
        let layer: &AnyObject = &*layer_ptr;
        let () = msg_send![layer, setCornerRadius: radius];
        let () = msg_send![layer, setMasksToBounds: Bool::YES];
    }

    /// V-2: insert an NSVisualEffectView behind the (transparent) render view so
    /// the window content sits over a blurred vibrancy layer. Uses the WezTerm
    /// approach: the blur view becomes the window's contentView and the original
    /// render view is re-parented on top of it. Only called when blur is enabled.
    #[cfg(target_os = "macos")]
    unsafe fn apply_macos_blur(&self, window: &Window) {
        use objc2::runtime::{AnyObject, Bool};
        use objc2::{class, msg_send};
        use objc2_foundation::{NSRect, NSString};
        use winit::raw_window_handle::HasWindowHandle;

        let Ok(h) = window.window_handle() else {
            return;
        };
        let winit::raw_window_handle::RawWindowHandle::AppKit(h) = h.as_raw() else {
            return;
        };
        let ns_view: &AnyObject = &*(h.ns_view.as_ptr() as *const AnyObject);
        let ns_win_ptr: *mut AnyObject = msg_send![ns_view, window];
        if ns_win_ptr.is_null() {
            return;
        }
        let ns_win: &AnyObject = &*ns_win_ptr;

        // winit's WinitView (the content view) owns ivars we must not disturb, so
        // do NOT replace it. Insert the blur view as a sibling *behind* it, as a
        // child of the window's frame view. Transparent clear pixels then reveal
        // the vibrancy instead of the desktop.
        let content_ptr: *mut AnyObject = msg_send![ns_win, contentView];
        if content_ptr.is_null() {
            return;
        }
        let content: &AnyObject = &*content_ptr;
        let frame_ptr: *mut AnyObject = msg_send![content, superview];
        if frame_ptr.is_null() {
            return; // no frame view yet — skip rather than risk the hierarchy
        }
        let frame_view: &AnyObject = &*frame_ptr;
        let bounds: NSRect = msg_send![frame_view, bounds];

        let blur_ptr: *mut AnyObject = msg_send![class!(NSVisualEffectView), alloc];
        let blur_ptr: *mut AnyObject = msg_send![blur_ptr, initWithFrame: bounds];
        if blur_ptr.is_null() {
            return;
        }
        let blur: &AnyObject = &*blur_ptr;
        // material 13 = UnderWindowBackground; blendingMode 0 = BehindWindow; state 1 = Active.
        let () = msg_send![blur, setMaterial: 13_i64];
        let () = msg_send![blur, setBlendingMode: 0_i64];
        let () = msg_send![blur, setState: 1_i64];
        let () = msg_send![blur, setAutoresizingMask: 18_usize]; // width|height sizable

        // Light/dark vibrancy appearance.
        let name = match self.config.window.blur {
            crate::config::schema::WindowBlur::Light => "NSAppearanceNameAqua",
            _ => "NSAppearanceNameDarkAqua",
        };
        let ns_name = NSString::from_str(name);
        let appearance: *mut AnyObject =
            msg_send![class!(NSAppearance), appearanceNamed: &*ns_name];
        if !appearance.is_null() {
            let () = msg_send![blur, setAppearance: appearance];
        }

        // NSWindowBelow = -1: place the blur behind the winit content view.
        let () = msg_send![frame_view, addSubview: blur, positioned: -1_i64, relativeTo: content];
        let () = msg_send![ns_win, setOpaque: Bool::NO];
    }

    // ── Low-frequency background tasks (battery + git) ───────────────────────
    // Each sub-poll has its own TTL guard; separation keeps about_to_wait focused
    // on scheduling / wakeup logic.
    fn poll_low_freq_tasks(&mut self) {
        // Battery status: immediately on first frame, then every 30 s.
        // Skipped when unfocused — IOKit call is unnecessary in background.
        // battery_polled guards against re-entry on desktops where query() always returns None.
        if self.window_focused
            && (!self.battery_polled
                || self.battery_last_poll.elapsed() >= std::time::Duration::from_secs(30))
        {
            self.battery_polled = true;
            self.battery_last_poll = std::time::Instant::now();
            if let Some(status) = crate::platform::battery::query() {
                use crate::config::schema::BatterySaverMode;
                let active = match self.config.battery_saver {
                    BatterySaverMode::Always => true,
                    BatterySaverMode::Never => false,
                    BatterySaverMode::Auto => status.on_battery,
                };
                let changed = self.battery_saver_active != active
                    || self
                        .battery_status
                        .as_ref()
                        .map(|s| s.percent != status.percent || s.on_battery != status.on_battery)
                        .unwrap_or(true);
                self.battery_status = Some(status);
                self.battery_saver_active = active;
                if changed {
                    if let Some(rc) = &mut self.render_ctx {
                        rc.status_bar_key = 0;
                        // Switch present mode: Fifo (vsync) on battery; best available on AC.
                        let modes = rc.renderer.surface_caps_present_modes();
                        let mode = if active {
                            wgpu::PresentMode::Fifo
                        } else if modes.contains(&wgpu::PresentMode::Mailbox) {
                            wgpu::PresentMode::Mailbox
                        } else if modes.contains(&wgpu::PresentMode::FifoRelaxed) {
                            wgpu::PresentMode::FifoRelaxed
                        } else {
                            wgpu::PresentMode::Fifo
                        };
                        rc.renderer.set_present_mode(mode);
                    }
                    self.request_redraw();
                }
            }
        }

        // Git branch poll (TD-PERF-19): at most once per second (60 s in battery-saver).
        let git_poll_interval = if self.battery_saver_active {
            std::time::Duration::from_secs(60)
        } else {
            std::time::Duration::from_secs(1)
        };
        if self.config.status_bar.enabled
            && self.window_focused
            && self.ui.git_branch_last_poll.elapsed() >= git_poll_interval
        {
            self.ui.git_branch_last_poll = std::time::Instant::now();
            let git_ttl = if self.battery_saver_active {
                std::time::Duration::from_secs(60)
            } else {
                std::time::Duration::from_secs(15)
            };
            let git_dirty = self.config.status_bar.git_dirty_check && !self.battery_saver_active;
            let git_updated =
                self.ui
                    .poll_git_branch(self.cached_cwd.as_deref(), git_dirty, git_ttl);
            if git_updated {
                if let Some(rc) = &mut self.render_ctx {
                    rc.status_bar_key = 0;
                }
                self.request_redraw();
            }
        }
    }
}

impl ApplicationHandler<()> for App {
    fn user_event(&mut self, event_loop: &ActiveEventLoop, _event: ()) {
        let (data_ids, exited) = self.mux.poll_pty_events();
        self.mux.apply_osc133_events();
        let had_exit = !exited.is_empty();
        if self.close_exited_terminals(exited) {
            event_loop.exit();
            return;
        }
        if had_exit {
            self.request_redraw();
        }

        // PTY data: mark pending but do NOT request_redraw immediately.
        // about_to_wait will fire the render after a short coalescing window (4ms),
        // ensuring multi-batch TUI updates (erase + redraw) are shown as one frame.
        // Exception: small batches (≤2 events) are likely keyboard echo — render immediately.
        if !data_ids.is_empty() {
            self.last_pty_batch_size = data_ids.len();
            self.pending_pty_redraw = true;
            self.last_pty_activity = std::time::Instant::now();
            for id in &data_ids {
                self.update_terminal_shell_ctx(*id);
            }
            self.refresh_status_cache();
            // Adaptive coalescing: keyboard echo has small batches — skip the wait.
            if data_ids.len() <= 2 {
                self.pending_pty_redraw = false;
                self.request_redraw();
            }
        }

        self.flush_pending_pty_run();
        self.flush_pending_agent_action();
        self.flush_pending_paste();
        let scan_ready = self.ui.poll_file_scan();
        let branch_ready = self.ui.poll_branch_scan();
        if scan_ready || branch_ready {
            self.request_redraw();
        }
    }

    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let mut attrs = WindowAttributes::default().with_title("PetruTerm");
        if self.config.window.title_bar_style == TitleBarStyle::None {
            attrs = attrs.with_decorations(false);
        }
        // V-1: a translucent window (opacity < 1 or blur) needs a transparent
        // surface so the clear-color alpha and the vibrancy behind it show.
        let want_transparent = self.config.window.blur != crate::config::schema::WindowBlur::None
            || self.config.window.opacity < 1.0;
        if want_transparent {
            attrs = attrs.with_transparent(true);
        }
        if let Some(w) = self.config.window.initial_width {
            if let Some(h) = self.config.window.initial_height {
                attrs = attrs.with_inner_size(winit::dpi::LogicalSize::new(w, h));
            }
        } else if !self.config.window.start_maximized {
            // Only set an explicit inner_size when not maximizing. When start_maximized is true,
            // letting winit pick the default avoids a spurious Resized(1280x800) event that
            // arrives after the maximize Resized and would shrink the PTY back to the wrong size.
            attrs = attrs.with_inner_size(winit::dpi::LogicalSize::new(1280u32, 800u32));
        }

        let window = match event_loop.create_window(attrs) {
            Ok(w) => Arc::new(w),
            Err(e) => {
                log::error!("Failed to create window: {e}");
                event_loop.exit();
                return;
            }
        };
        #[cfg(target_os = "macos")]
        if self.config.window.title_bar_style == TitleBarStyle::Custom {
            unsafe {
                self.apply_macos_custom_titlebar(&window);
            }
        }
        #[cfg(target_os = "macos")]
        if self.config.window.blur != crate::config::schema::WindowBlur::None {
            unsafe {
                self.apply_macos_blur(&window);
            }
        }

        if self.config.window.start_maximized {
            window.set_maximized(true);
        }

        let render_ctx = match pollster::block_on(RenderContext::new(window.clone(), &self.config))
        {
            Ok(rc) => rc,
            Err(e) => {
                log::error!("Failed to initialize RenderContext: {e}");
                event_loop.exit();
                return;
            }
        };

        // V-1: wgpu's CAMetalLayer defaults to opaque, which ignores the clear
        // alpha. Make it non-opaque so translucency/blur show through.
        #[cfg(target_os = "macos")]
        if want_transparent {
            unsafe {
                Self::set_metal_layer_opaque(&window, false);
            }
        }

        // V-3: a borderless window is square; round it to match the floating look.
        // Custom/Native titlebars already inherit the system window shape.
        #[cfg(target_os = "macos")]
        if self.config.window.title_bar_style == TitleBarStyle::None {
            // cornerRadius is in points (Core Animation applies the backing scale).
            let radius = crate::renderer::ui_style::R_PANEL as f64;
            unsafe {
                Self::apply_macos_rounded_corners(&window, radius);
            }
        }

        // Register the native menu bar with the macOS application and window manager.
        #[cfg(target_os = "macos")]
        {
            self.menu.menu_bar.init_for_nsapp();
            self.menu.window_submenu.set_as_windows_menu_for_nsapp();
        }

        self.window = Some(window);
        self.render_ctx = Some(render_ctx);
        self.apply_tab_bar_padding(); // no-op here (0 tabs yet), but sets up for first tab
        if self.open_initial_tab().is_err() {
            event_loop.exit();
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::CloseRequested => {
                if self.config.workspaces.auto_save_on_exit {
                    self.mux.save_all_workspaces();
                }
                event_loop.exit();
            }
            WindowEvent::Occluded(occluded) => {
                self.window_occluded = occluded;
            }
            WindowEvent::Focused(focused) => {
                self.window_focused = focused;
                if focused {
                    // Clear any stale modifier state (e.g. Shift held when Cmd+Tab-ing
                    // away and released outside the window). Stale shift + active KKP
                    // (DISAMBIGUATE_ESC_CODES) would cause Enter to send \x1b[13;2u
                    // (Shift+Enter) instead of \r, making zsh insert a newline rather
                    // than execute the command.
                    self.input.modifiers = winit::event::Modifiers::default();
                }
            }
            WindowEvent::RedrawRequested => {
                self.handle_redraw(event_loop);
            }
            WindowEvent::Resized(size) => {
                if let Some(rc) = &mut self.render_ctx {
                    rc.renderer.resize(size.width, size.height);
                }
                self.resize_terminals_for_panel();
                self.ui.ai_block.dirty = true;
                self.request_redraw();
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                if let Some(rc) = &mut self.render_ctx {
                    if let Err(err) = rc.refresh_text_metrics(&self.config, scale_factor as f32) {
                        log::warn!(
                            "Failed to rebuild text metrics after scale-factor change: {err}"
                        );
                    }
                    if let Some(window) = &self.window {
                        let size = window.inner_size();
                        rc.renderer.resize(size.width, size.height);
                    }
                }
                self.apply_tab_bar_padding();
                self.resize_terminals_for_panel();
                self.ui.panel_mut().dirty = true;
                self.ui.ai_block.dirty = true;
                self.request_redraw();
            }
            WindowEvent::ModifiersChanged(mods) => {
                self.input.modifiers = mods;
                if !mods.state().alt_key() {
                    self.input.resize_mode = false;
                }
            }
            WindowEvent::KeyboardInput {
                event,
                is_synthetic: false,
                ..
            } => {
                self.handle_keyboard(event_loop, event);
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.handle_mouse_motion(position);
            }
            WindowEvent::MouseInput { state, button, .. } => {
                self.handle_mouse_button(state, button);
            }
            WindowEvent::MouseWheel { delta, .. } => {
                self.handle_scroll(delta);
            }
            WindowEvent::DroppedFile(path) => {
                let path_str = path.to_string_lossy().into_owned();
                if self.ui.is_panel_visible() {
                    self.ui.panel_mut().append_path(&path_str);
                } else if let Some(terminal) = self.mux.active_terminal() {
                    terminal.write_input(path_str.as_bytes());
                    self.note_pty_input();
                }
                self.request_redraw();
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        // Drain native menu events. muda uses an internal static channel when no
        // custom set_event_handler is registered; about_to_wait runs after every
        // OS event (including the FocusGained that firing a menu item triggers).
        while let Ok(menu_event) = muda::MenuEvent::receiver().try_recv() {
            if let Some(action) = self.menu.action_for(&menu_event) {
                if action == crate::ui::palette::Action::Quit {
                    event_loop.exit();
                    return;
                }
                let tab_count_before = self.mux.tabs.tab_count();
                let pane_count_before = self.mux.active_pane_count();
                let panel_was_visible = self.ui.is_panel_visible();
                if let (Some(rc), Some(w)) = (self.render_ctx.as_mut(), self.window.as_deref()) {
                    self.ui.handle_palette_action(
                        action,
                        &mut self.mux,
                        rc,
                        &mut self.config,
                        Some(w),
                        self.wakeup_proxy.clone(),
                    );
                }
                // Mirror the post-action resize logic from KeyboardInput handler.
                if self.ui.is_panel_visible() != panel_was_visible {
                    self.resize_terminals_for_panel();
                }
                if self.mux.tabs.tab_count() != tab_count_before {
                    self.apply_tab_bar_padding();
                    self.resize_terminals_for_panel();
                } else if self.mux.active_pane_count() != pane_count_before {
                    self.resize_terminals_for_panel();
                }
                self.request_redraw();
            }
        }

        // Drain any PTY events that arrived since user_event last ran.
        // This catches batches that slipped in after user_event drained the channel,
        // and keeps last_pty_activity accurate for coalescing.
        let (data_ids, exited) = self.mux.poll_pty_events();
        self.mux.apply_osc133_events();
        let had_exit = !exited.is_empty();
        if self.close_exited_terminals(exited) {
            event_loop.exit();
            return;
        }
        if had_exit {
            self.request_redraw();
        }
        let had_pty_data = !data_ids.is_empty();
        if had_pty_data {
            self.last_pty_batch_size = data_ids.len();
            self.pending_pty_redraw = true;
            self.last_pty_activity = std::time::Instant::now();
            // Update per-terminal shell context only for terminals that fired (TD-PERF-01).
            // Refresh CWD for the active pane (TD-PERF-02).
            for id in &data_ids {
                self.update_terminal_shell_ctx(*id);
            }
            self.refresh_status_cache();
            // Adaptive coalescing: small batches (≤2) are keyboard echo — skip the wait.
            if data_ids.len() <= 2 {
                self.pending_pty_redraw = false;
                self.request_redraw();
            }
        }

        self.handle_acp_terminal_requests();
        self.ui.poll_acp_connect();
        let panel_ai = self.ui.poll_ai_events();
        let block_ai = self.ui.poll_ai_block_events();
        if panel_ai.completed {
            self.fire_lua_event("ai_response");
        }
        let had_ai = panel_ai.changed || block_ai.changed;
        let scan_ready = self.ui.poll_file_scan();
        if had_ai || scan_ready || panel_ai.more || block_ai.more {
            self.request_redraw();
        }
        self.flush_pending_pty_run();
        self.flush_pending_agent_action();
        self.flush_pending_paste();

        self.poll_low_freq_tasks();

        // ── Idle detection ───────────────────────────────────────────────────
        // The frame is "idle" when there is no PTY data, no AI events, no active
        // drag, no overlay, and no search bar open. When idle, we skip cursor blink
        // entirely (many terminals do this) and use ControlFlow::Wait so the OS
        // keeps the event loop dormant until a real event arrives.
        //
        // Background AI activity should not keep the app "active" by itself.
        // Only visible, interactive AI surfaces prevent idle; hidden/background
        // streaming still requests redraws on token arrival, but it does not keep
        // the blink timer alive between events.
        let panel_overlay_active = self.ui.is_panel_visible() && self.ui.panel_focused;
        let block_overlay_active = self.ui.is_block_visible();
        let visible_ai_activity = (panel_ai.changed && panel_overlay_active)
            || (block_ai.changed && block_overlay_active);
        let any_overlay = panel_overlay_active
            || self.ui.palette.visible
            || self.ui.context_menu.visible
            || self.ui.search_bar.visible
            || block_overlay_active;
        let any_drag = self.input.dragging_separator.is_some() || self.input.mouse_left_pressed;
        let idle = !had_pty_data
            && !visible_ai_activity
            && !self.pending_pty_redraw
            && !any_overlay
            && !any_drag;

        // Blink only when focused, not idle, and not in battery-saver mode.
        // On battery the cursor stays solid — eliminates the 530 ms periodic GPU wakeup.
        if !idle
            && self.window_focused
            && !self.battery_saver_active
            && self.input.update_cursor_blink()
        {
            // Input rows are rebuilt fresh every frame (TD-PERF-10), so blink alone does not
            // require a full content rebuild. Only mark dirty when the file picker is open,
            // because its search-query cursor lives in the content section.
            if self.ui.is_panel_visible()
                && self.ui.panel_focused
                && self.ui.panel().file_picker_open
            {
                self.ui.panel_mut().dirty = true;
            }
            // else: request_redraw() below is enough; input rows are always rebuilt.
            // AI block query cursor blinks when typing.
            if self.ui.ai_block.is_typing() {
                self.ui.ai_block.dirty = true;
            }
            // Fast blink path: when only the terminal cursor changed, flag it so
            // RedrawRequested can skip the full cell rebuild (cursor overlay).
            let needs_full = (self.ui.is_panel_visible() && self.ui.panel_focused)
                || self.ui.ai_block.is_typing();
            if !needs_full {
                self.cursor_blink_dirty = true;
            }
            self.request_redraw();
        }
        // When idle or unfocused: skip blink entirely — saves periodic reshape + GPU upload.

        // PTY render coalescing: fire the deferred redraw once the PTY has been
        // quiet for 4ms. This window is long enough to catch Gemini/TUI "erase +
        // redraw" sequences (usually < 2ms apart) but short enough to be imperceptible.
        const PTY_COALESCE_MS: u64 = 4;
        let pty_deadline =
            self.last_pty_activity + std::time::Duration::from_millis(PTY_COALESCE_MS);
        if self.pending_pty_redraw {
            let now = std::time::Instant::now();
            if now >= pty_deadline {
                self.pending_pty_redraw = false;
                self.request_redraw();
            }
            // else: WaitUntil below will wake us at pty_deadline to retry.
        }

        // Keep redrawing while a toast is active; clear it once expired.
        if let Some((_, deadline)) = &self.toast {
            if std::time::Instant::now() < *deadline {
                self.request_redraw();
            } else {
                self.toast = None;
                self.request_redraw(); // one final frame to clear the toast
            }
        }

        self.flush_redraw_request();

        // If flush_redraw_request deferred a frame due to max_fps, compute when
        // the next frame slot opens so we can wake at exactly that time.
        let frame_deadline: Option<std::time::Instant> = if self.needs_redraw {
            let fps = self.config.max_fps.max(1) as u64;
            let interval = std::time::Duration::from_nanos(1_000_000_000 / fps);
            Some(self.last_frame_at + interval)
        } else {
            None
        };

        // Schedule a wakeup at the top of the next minute so the status bar time
        // widget stays accurate. Omitted when the status bar is disabled — no clock to refresh.
        let next_minute_wake: Option<std::time::Instant> = if self.config.status_bar.enabled {
            let now_inst = std::time::Instant::now();
            let secs_now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            let secs_remaining = 60 - (secs_now % 60);
            Some(now_inst + std::time::Duration::from_secs(secs_remaining))
        } else {
            None
        };

        // Compute the wakeup deadline once. The base differs by mode:
        // - Active (focused, not battery-saver, not idle): blink timer drives minimum wakeup.
        // - Everything else (idle, unfocused, battery-saver): park for up to 1 hour; real
        //   events (PTY data, user input) arrive via winit wakeups without a spin.
        let active = self.window_focused && !self.battery_saver_active && !idle;
        let base_deadline = if active {
            self.input.cursor_last_blink + std::time::Duration::from_millis(530)
        } else {
            std::time::Instant::now() + std::time::Duration::from_secs(3600)
        };

        let mut wake = match next_minute_wake {
            Some(t) => base_deadline.min(t),
            None => base_deadline,
        };
        if self.pending_pty_redraw {
            wake = wake.min(pty_deadline);
        }
        if let Some(fd) = frame_deadline {
            wake = wake.min(fd);
        }
        // PTY-echo grace: after writing input to a PTY we expect an async echo
        // shortly. macOS occasionally loses/delays the reader thread's
        // EventLoopProxy wakeup, so poll on a reliable short timer instead of
        // trusting the wakeup alone. Cleared once the window expires.
        match self.pty_echo_grace_until {
            Some(t) if std::time::Instant::now() < t => {
                wake = wake.min(std::time::Instant::now() + std::time::Duration::from_millis(8));
            }
            Some(_) => self.pty_echo_grace_until = None,
            None => {}
        }

        event_loop.set_control_flow(winit::event_loop::ControlFlow::WaitUntil(wake));
    }
}

impl Drop for App {
    fn drop(&mut self) {
        log::info!("App dropping; shutting down PTYs.");
        self.mux.shutdown();
    }
}
