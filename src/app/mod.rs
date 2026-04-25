use anyhow::Result;
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
use crate::ui::{ContextAction, Rect};

mod input;
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

    /// Cached pane separator geometry from the last render frame (TD-PERF-24).
    /// Avoids recomputing separator layout on every CursorMoved event.
    separator_snapshot: Vec<crate::ui::PaneSeparator>,

    /// Cached CWD of the active shell (TD-PERF-02).
    /// Refreshed via proc_pidinfo only when PTY data arrives or terminal focus changes,
    /// instead of every frame.
    cached_cwd: Option<std::path::PathBuf>,

    sidebar_visible: bool,
    sidebar_nav_cursor: usize,
    sidebar_rename_input: Option<String>,
    /// True once the user presses an arrow key in the sidebar. Only then does
    /// Enter confirm a workspace switch — prevents stealing Enter from the terminal.
    sidebar_kbd_active: bool,

    /// Current battery status. None until the first poll or on desktops with no battery.
    battery_status: Option<crate::platform::battery::BatteryStatus>,
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
            ui: UiManager::new(&config),
            input: InputHandler::new(&config),
            menu: AppMenu::build(),
            wakeup_proxy,
            pending_pty_redraw: false,
            last_pty_activity: std::time::Instant::now(),
            last_pty_batch_size: 0,
            window_occluded: false,
            window_focused: true,
            cursor_blink_dirty: false,
            cached_cwd: None,
            sidebar_visible: false,
            sidebar_nav_cursor: 0,
            sidebar_rename_input: None,
            sidebar_kbd_active: false,
            battery_status: None,
            battery_last_poll: std::time::Instant::now(),
            battery_saver_active: false,
            config_reload_at: None,
            mcp_watcher,
            mcp_reload_at: None,
            terminal_shell_ctxs: std::collections::HashMap::new(),
            separator_snapshot: Vec::new(),
            lua,
            toast: None,
        }
    }

    /// Drain mux.pending_lua_events, fire each via Lua, drain any queued toast.
    fn dispatch_lua_events(&mut self) {
        let events: Vec<&'static str> = std::mem::take(&mut self.mux.pending_lua_events);
        if events.is_empty() {
            return;
        }
        if let Some(lua) = &self.lua {
            for event in events {
                crate::config::lua::fire_lua_event(lua, event);
            }
            if let Some((msg, ms)) = crate::config::lua::drain_lua_toast(lua) {
                let deadline = std::time::Instant::now() + std::time::Duration::from_millis(ms);
                self.toast = Some((msg, deadline));
                if let Some(w) = &self.window {
                    w.request_redraw();
                }
            }
        }
    }

    /// Fire a single Lua event (terminal_exit, ai_response) and drain any queued toast.
    fn fire_lua_event(&mut self, event: &str) {
        if let Some(lua) = &self.lua {
            crate::config::lua::fire_lua_event(lua, event);
            if let Some((msg, ms)) = crate::config::lua::drain_lua_toast(lua) {
                let deadline = std::time::Instant::now() + std::time::Duration::from_millis(ms);
                self.toast = Some((msg, deadline));
                if let Some(w) = &self.window {
                    w.request_redraw();
                }
            }
        }
    }

    /// Refresh the cached CWD for the active pane (TD-PERF-02).
    /// Call on PTY data arrival or terminal focus change — NOT every frame.
    fn refresh_status_cache(&mut self) {
        self.cached_cwd = self.mux.active_cwd();
    }

    /// Read shell context for a specific terminal and store it by terminal_id.
    /// Skips the disk read when the context file has not changed since last call (TD-PERF-09).
    fn update_terminal_shell_ctx(&mut self, terminal_id: usize) {
        let pid = self
            .mux
            .terminals
            .get(terminal_id)
            .and_then(|t| t.as_ref())
            .map(|t| t.child_pid);
        if let Some(pid) = pid {
            let path = crate::llm::shell_context::ShellContext::context_file_path_for_pid(pid);
            let Ok(mtime) = std::fs::metadata(&path).and_then(|m| m.modified()) else {
                return;
            };
            if let Some((_, cached_mtime)) = self.terminal_shell_ctxs.get(&terminal_id) {
                if *cached_mtime == mtime {
                    return;
                }
            }
            if let Some(ctx) = crate::llm::shell_context::ShellContext::load_for_pid(pid) {
                self.terminal_shell_ctxs.insert(terminal_id, (ctx, mtime));
            }
        }
    }

    /// Shell context for the currently active pane, if any.
    fn active_shell_ctx(&self) -> Option<&crate::llm::shell_context::ShellContext> {
        let tid = self.mux.focused_terminal_id();
        self.terminal_shell_ctxs.get(&tid).map(|(ctx, _)| ctx)
    }

    fn tab_bar_visible(&self) -> bool {
        // The unified titlebar bar is always present in Custom mode for traffic lights clearance.
        if self.config.window.title_bar_style == TitleBarStyle::Custom {
            return true;
        }
        self.mux.tabs.tab_count() > 1
    }

    fn tab_bar_height_px(&self) -> f32 {
        let sf = self
            .render_ctx
            .as_ref()
            .map(|rc| rc.scale_factor)
            .unwrap_or(1.0);
        if self.config.window.title_bar_style == TitleBarStyle::Custom {
            TITLEBAR_HEIGHT * sf
        } else if self.mux.tabs.tab_count() > 1 {
            self.cell_dims().1 as f32
        } else {
            0.0
        }
    }

    fn status_bar_height_px(&self) -> f32 {
        if self.config.status_bar.enabled {
            self.cell_dims().1 as f32
        } else {
            0.0
        }
    }

    /// Update the GPU uniform padding to account for the tab bar (or lack thereof).
    /// Call whenever tab count crosses the 1↔2 boundary, or on initial setup.
    fn apply_tab_bar_padding(&mut self) {
        if let Some(rc) = &mut self.render_ctx {
            let title_h = if self.config.window.title_bar_style == TitleBarStyle::Custom {
                TITLEBAR_HEIGHT * rc.scale_factor
            } else if self.mux.tabs.tab_count() > 1 {
                rc.shaper.cell_height
            } else {
                0.0
            };
            let pad = &self.config.window.padding;
            let sidebar_px = if self.sidebar_visible {
                SIDEBAR_COLS as f32 * rc.shaper.cell_width + SIDEBAR_MARGIN
            } else {
                0.0
            };
            rc.renderer
                .set_padding(pad.left as f32 + sidebar_px, pad.top as f32 + title_h);
        }
    }

    fn default_grid_size(&self) -> (u16, u16) {
        if let Some(rc) = &self.render_ctx {
            let (w, h) = rc.renderer.size();
            let (cell_w, cell_h) = self.cell_dims();
            let pad = &self.config.window.padding;
            let panel_px = if self.ui.is_panel_visible() {
                self.chat_panel_width_px()
            } else {
                0.0
            };
            let sidebar_px = self.sidebar_width_px();
            let tab_h = self.tab_bar_height_px();
            let sb_h = self.status_bar_height_px();
            let cols = ((w as f32 - pad.left as f32 - pad.right as f32 - panel_px - sidebar_px)
                / cell_w as f32)
                .max(1.0) as u16;
            let rows = ((h as f32 - pad.top as f32 - pad.bottom as f32 - tab_h - sb_h)
                / cell_h as f32)
                .max(1.0) as u16;
            (cols, rows)
        } else {
            (120, 40)
        }
    }

    fn chat_panel_width_px(&self) -> f32 {
        let (cell_w, _) = self.cell_dims();
        self.ui.panel().width_cols as f32 * cell_w as f32
    }

    fn sidebar_width_px(&self) -> f32 {
        if self.sidebar_visible {
            let (cell_w, _) = self.cell_dims();
            SIDEBAR_COLS as f32 * cell_w as f32 + SIDEBAR_MARGIN
        } else {
            0.0
        }
    }

    /// Forward a `run_command` confirmation to the active PTY and clear the pending field.
    fn flush_pending_pty_run(&mut self) {
        if let Some(cmd) = self.ui.pending_pty_run.take() {
            if let Some(terminal) = self.mux.active_terminal() {
                let mut data = cmd.into_bytes();
                data.push(b'\n');
                terminal.write_input(&data);
            }
        }
    }

    /// Write clipboard text from an async paste request to the active terminal (TD-PERF-15).
    fn flush_pending_paste(&mut self) {
        if let Some(text) = self.ui.poll_pending_paste() {
            if let Some(terminal) = self.mux.active_terminal() {
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
    }

    fn cell_dims(&self) -> (u16, u16) {
        self.render_ctx
            .as_ref()
            .map(|rc| (rc.shaper.cell_width as u16, rc.shaper.cell_height as u16))
            .unwrap_or((8, 16))
    }

    fn open_initial_tab(&mut self) -> Result<()> {
        let viewport = self.viewport_rect();
        let (cols, rows) = self.default_grid_size();
        let (cell_w, cell_h) = self.cell_dims();
        self.mux.open_initial_tab(
            &self.config,
            viewport,
            cols,
            rows,
            cell_w,
            cell_h,
            self.wakeup_proxy.clone(),
        )
    }

    fn viewport_rect(&self) -> Rect {
        let pad = &self.config.window.padding;
        let tab_h = self.tab_bar_height_px();
        let sb_h = self.status_bar_height_px();
        if let Some(rc) = &self.render_ctx {
            let (w, h) = rc.renderer.size();
            let panel_px = if self.ui.is_panel_visible() {
                self.chat_panel_width_px()
            } else {
                0.0
            };
            let sidebar_px = self.sidebar_width_px();
            Rect {
                x: pad.left as f32 + sidebar_px,
                y: pad.top as f32 + tab_h,
                w: (w as f32 - pad.left as f32 - pad.right as f32 - panel_px - sidebar_px).max(0.0),
                h: (h as f32 - pad.top as f32 - pad.bottom as f32 - tab_h - sb_h).max(0.0),
            }
        } else {
            let sidebar_px = self.sidebar_width_px();
            Rect {
                x: pad.left as f32 + sidebar_px,
                y: pad.top as f32 + tab_h,
                w: 800.0,
                h: 600.0,
            }
        }
    }

    fn resize_terminals_for_panel(&mut self) {
        let viewport = self.viewport_rect();
        let (cell_w, cell_h) = self.cell_dims();
        self.mux.resize_all(
            viewport,
            self.config.scrollback_lines as usize,
            cell_w,
            cell_h,
        );
        // Panel layout depends on term_cols/screen_rows — rebuild instances after resize.
        self.ui.panel_mut().dirty = true;
    }

    /// Close any terminals that exited. Returns true if the last tab closed (caller should exit).
    fn close_exited_terminals(&mut self, exited: Vec<usize>) -> bool {
        if exited.is_empty() {
            return false;
        }
        for tid in exited {
            self.terminal_shell_ctxs.remove(&tid);
            self.ui.remove_terminal_state(tid);
            if let Some(rc) = &mut self.render_ctx {
                rc.row_caches.remove(&tid);
            }
            if self.mux.close_terminal(tid) {
                return true;
            }
        }
        self.apply_tab_bar_padding();
        self.resize_terminals_for_panel();
        self.fire_lua_event("terminal_exit");
        false
    }

    /// Given a pixel x coordinate, return which tab index is under the cursor in the tab bar.
    fn hit_test_tab_bar(&self, x_px: f64) -> Option<usize> {
        let (cell_w, _) = self.cell_dims();
        let sf = self
            .render_ctx
            .as_ref()
            .map(|rc| rc.scale_factor as f64)
            .unwrap_or(1.0);
        let tabs_start_x = if self.config.window.title_bar_style == TitleBarStyle::Custom {
            158.0 * sf
        } else {
            self.config.window.padding.left as f64
        };
        if x_px < tabs_start_x {
            return None; // click in the buttons zone, not a tab
        }
        let click_col = ((x_px - tabs_start_x) / cell_w as f64).floor() as usize;
        let mut col = 0usize;
        for (i, tab) in self.mux.tabs.tabs().iter().enumerate() {
            col += 1; // gap before pill
            col += format!(" {} ", i + 1).chars().count(); // badge
            let raw = format!(" {} ", tab.title);
            col += raw.chars().take(14).count();
            if click_col < col {
                return Some(i);
            }
        }
        None
    }

    fn check_config_reload(&mut self) {
        const DEBOUNCE_MS: u64 = 300;

        // Global config dir watcher — split lua (app config) vs json (MCP config).
        if let Some(watcher) = &self.config_watcher {
            if let Some(path) = watcher.poll() {
                if path.extension().is_some_and(|e| e == "json") {
                    self.mcp_reload_at = Some(
                        std::time::Instant::now() + std::time::Duration::from_millis(DEBOUNCE_MS),
                    );
                } else {
                    self.config_reload_at = Some(
                        std::time::Instant::now() + std::time::Duration::from_millis(DEBOUNCE_MS),
                    );
                }
            }
        }

        // Project-local .petruterm/ watcher.
        if let Some(watcher) = &self.mcp_watcher {
            if watcher.poll().is_some() {
                self.mcp_reload_at =
                    Some(std::time::Instant::now() + std::time::Duration::from_millis(DEBOUNCE_MS));
            }
        }

        // Fire lua config reload.
        if self
            .config_reload_at
            .is_some_and(|t| std::time::Instant::now() >= t)
        {
            self.config_reload_at = None;
            if let Ok((new_cfg, new_lua)) = config::reload() {
                self.config = new_cfg;
                self.lua = Some(new_lua);
                if let Some(rc) = &mut self.render_ctx {
                    rc.renderer
                        .update_bg_color(self.config.colors.background_wgpu());
                }
                self.ui.palette.rebuild_keybinds(&self.config);
                self.ui.palette.rebuild_snippets(&self.config.snippets);
                self.ui.rewire_llm_provider(&self.config);
                log::info!("Config hot-reloaded.");
            }
        }

        // Fire MCP reload.
        if self
            .mcp_reload_at
            .is_some_and(|t| std::time::Instant::now() >= t)
        {
            self.mcp_reload_at = None;
            let cwd = self
                .cached_cwd
                .clone()
                .or_else(|| std::env::current_dir().ok())
                .unwrap_or_default();
            self.ui.reload_mcp(&cwd);
        }
    }

    fn mouse_in_panel(&self) -> bool {
        if !self.ui.is_panel_visible() {
            return false;
        }
        let (cw, _) = self.cell_dims();
        let term_right_px = self.config.window.padding.left as f64
            + self.sidebar_width_px() as f64
            + self.mux.active_terminal_size().0 as f64 * cw as f64;
        self.input.mouse_pos.0 >= term_right_px
    }

    fn pixel_to_cell(&self, x: f64, y: f64) -> (usize, usize) {
        // Subtract tab bar height so y is relative to the content viewport top.
        let tab_h = self.tab_bar_height_px() as f64;
        let (raw_col, raw_row) = self.input.pixel_to_cell(
            x - self.sidebar_width_px() as f64,
            y - tab_h,
            &self.config,
            &self.render_ctx,
        );
        // Subtract the focused pane's offset to convert viewport coords to terminal-local coords.
        let (cell_w, cell_h) = self.cell_dims();
        let viewport = self.viewport_rect();
        let (col_off, row_off) =
            self.mux
                .focused_pane_offset(viewport, cell_w as f32, cell_h as f32);
        let (term_cols, term_rows) = self.mux.active_terminal_size();
        (
            raw_col
                .saturating_sub(col_off)
                .min(term_cols.saturating_sub(1)),
            raw_row
                .saturating_sub(row_off)
                .min(term_rows.saturating_sub(1)),
        )
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
        self.sidebar_nav_cursor = self.sidebar_nav_cursor.min(count.saturating_sub(1));
    }

    fn sidebar_selected_workspace_id(&self) -> Option<usize> {
        self.mux
            .workspaces()
            .get(self.sidebar_nav_cursor)
            .map(|w| w.id)
    }

    fn handle_sidebar_key(&mut self, event: &winit::event::KeyEvent) -> bool {
        if event.state != ElementState::Pressed || !self.sidebar_visible {
            return false;
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
                    self.sidebar_nav_cursor = self.mux.workspace_count().saturating_sub(1);
                    self.sidebar_rename_input = None;
                    return true;
                }
                Key::Character(s) if s == "." => {
                    // leader+.: rename selected workspace (mirrors leader+, for RenameTab).
                    self.input.leader_active = false;
                    self.input.leader_deadline = None;
                    self.clamp_sidebar_cursor();
                    if let Some(ws) = self.mux.workspaces().get(self.sidebar_nav_cursor) {
                        self.sidebar_rename_input = Some(ws.name.clone());
                    }
                    return true;
                }
                _ => return false,
            }
        }

        self.clamp_sidebar_cursor();

        if self.sidebar_rename_input.is_some() {
            match &event.logical_key {
                Key::Named(NamedKey::Escape) => {
                    self.sidebar_rename_input = None;
                }
                Key::Named(NamedKey::Enter) => {
                    let name = self
                        .sidebar_rename_input
                        .as_ref()
                        .map(|s| s.trim().to_string())
                        .unwrap_or_default();
                    if let Some(id) = self.sidebar_selected_workspace_id() {
                        if !name.is_empty() {
                            self.mux.cmd_rename_workspace_id(id, name);
                        }
                    }
                    self.sidebar_rename_input = None;
                }
                Key::Named(NamedKey::Backspace) => {
                    if let Some(input) = self.sidebar_rename_input.as_mut() {
                        input.pop();
                    }
                }
                Key::Named(NamedKey::Space) => {
                    if let Some(input) = self.sidebar_rename_input.as_mut() {
                        input.push(' ');
                    }
                }
                Key::Character(s) => {
                    if let Some(input) = self.sidebar_rename_input.as_mut() {
                        input.push_str(s);
                    }
                }
                _ => {}
            }
            return true;
        }

        match &event.logical_key {
            Key::Named(NamedKey::ArrowDown) => {
                let max = self.mux.workspace_count().saturating_sub(1);
                self.sidebar_nav_cursor = (self.sidebar_nav_cursor + 1).min(max);
                self.sidebar_kbd_active = true;
                true
            }
            Key::Named(NamedKey::ArrowUp) => {
                self.sidebar_nav_cursor = self.sidebar_nav_cursor.saturating_sub(1);
                self.sidebar_kbd_active = true;
                true
            }
            // Enter only confirms when the user has explicitly navigated via arrows,
            // preventing the sidebar from stealing Enter while the terminal is in use.
            Key::Named(NamedKey::Enter) if self.sidebar_kbd_active => {
                if let Some(id) = self.sidebar_selected_workspace_id() {
                    self.mux.cmd_switch_workspace(id);
                    self.refresh_status_cache();
                }
                self.sidebar_visible = false;
                self.sidebar_rename_input = None;
                self.sidebar_kbd_active = false;
                self.apply_tab_bar_padding();
                self.resize_terminals_for_panel();
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
}

impl ApplicationHandler<()> for App {
    fn user_event(&mut self, event_loop: &ActiveEventLoop, _event: ()) {
        let (data_ids, exited) = self.mux.poll_pty_events();
        if self.close_exited_terminals(exited) {
            event_loop.exit();
            return;
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
                if let Some(w) = &self.window {
                    w.request_redraw();
                }
            }
        }

        // AI events are low-frequency; render immediately.
        let ai_needs_redraw = self.ui.poll_ai_events() || self.ui.poll_ai_block_events();
        self.flush_pending_pty_run();
        self.flush_pending_paste();
        let scan_ready = self.ui.poll_file_scan();
        let branch_ready = self.ui.poll_branch_scan();
        if ai_needs_redraw || scan_ready || branch_ready {
            if let Some(w) = &self.window {
                w.request_redraw();
            }
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
        if let Some(w) = self.config.window.initial_width {
            if let Some(h) = self.config.window.initial_height {
                attrs = attrs.with_inner_size(winit::dpi::LogicalSize::new(w, h));
            }
        } else {
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
                event_loop.exit();
            }
            WindowEvent::Occluded(occluded) => {
                self.window_occluded = occluded;
            }
            WindowEvent::Focused(focused) => {
                self.window_focused = focused;
            }
            WindowEvent::RedrawRequested => {
                // Skip rendering when the window is occluded or minimized.
                if self.window_occluded {
                    return;
                }

                #[cfg(feature = "profiling")]
                let _span = tracing::info_span!("redraw_frame").entered();

                let frame_start = std::time::Instant::now();

                if let Some(rc) = &mut self.render_ctx {
                    rc.frame_counter = rc.frame_counter.wrapping_add(1);
                }

                self.check_config_reload();
                let (data_ids, exited) = self.mux.poll_pty_events();
                if self.close_exited_terminals(exited) {
                    event_loop.exit();
                    return;
                }
                for id in &data_ids {
                    self.update_terminal_shell_ctx(*id);
                }
                let had_ai = self.ui.poll_ai_events();
                let had_ai_block = self.ui.poll_ai_block_events();
                if had_ai {
                    self.fire_lua_event("ai_response");
                }
                self.flush_pending_pty_run();
                self.flush_pending_paste();
                self.ui.poll_file_scan();
                self.ui.poll_branch_scan();

                // ── Fast blink path ─────────────────────────────────────────────────────
                // When only cursor blink changed (no PTY data, no AI events, no panel
                // cursor), skip the full cell rebuild and update just the cursor vertex.
                // The GPU instance buffer retains cell content from the previous full frame.
                let blink_only = self.cursor_blink_dirty
                    && data_ids.is_empty()
                    && !had_ai
                    && !had_ai_block
                    && !self.pending_pty_redraw;
                if blink_only {
                    self.cursor_blink_dirty = false;
                    if let Some(rc) = &mut self.render_ctx {
                        let blink_on = self.input.cursor_blink_on;
                        // Upload cursor (or a transparent placeholder) at content_end so the
                        // status bar / tab bar / scroll bar at content_end+1.. remain in the
                        // GPU buffer and are drawn. cell_count = last_instance_count draws
                        // everything; last_overlay_start preserves correct overlay split.
                        if let Some(v) = rc.cursor_vertex_template {
                            let upload_v = if blink_on {
                                v
                            } else {
                                // Transparent cursor — shader discards when bg.a < 0.01.
                                crate::renderer::cell::CellVertex {
                                    bg: [0.0, 0.0, 0.0, 0.0],
                                    ..v
                                }
                            };
                            rc.renderer
                                .upload_instances(std::slice::from_ref(&upload_v), rc.content_end);
                        }
                        rc.renderer.set_cell_count(rc.last_instance_count);
                        rc.renderer.set_overlay_start(rc.last_overlay_start);
                        let _ = rc.renderer.render();
                    }
                    return;
                }

                // Sync per-pane chat panel to the focused terminal.
                let terminal_id = self.mux.focused_terminal_id();
                self.ui.set_active_terminal(terminal_id);

                // Compute viewport and per-pane layout.
                let viewport = self.viewport_rect();
                let (cell_w, cell_h) = self.cell_dims();
                self.separator_snapshot =
                    self.mux
                        .active_pane_separators(viewport, cell_w as f32, cell_h as f32);

                // Viewport-wide dimensions for overlay positioning.
                let total_cols = (viewport.w / cell_w as f32).floor() as usize;
                let total_rows = (viewport.h / cell_h as f32).floor() as usize;
                // Capture status bar layout values before the mutable borrow of render_ctx.
                let tab_bar_vis = self.tab_bar_visible();
                let sb_pad_y = self.config.window.padding.top as f32 + self.tab_bar_height_px();
                // Capture sidebar width before render_ctx is mutably borrowed.
                let sidebar_px_snapshot = self.sidebar_width_px();
                // Snapshot the active pane's shell context before the render_ctx borrow.
                let sb_exit_code = self.active_shell_ctx().and_then(|c| {
                    if c.last_exit_code != 0 {
                        Some(c.last_exit_code)
                    } else {
                        None
                    }
                });
                let sb_exit_code_raw = self
                    .active_shell_ctx()
                    .map(|c| c.last_exit_code)
                    .unwrap_or(0);
                // Focused pane dimensions (scroll bar, AI block anchor).
                let (term_cols, term_rows) = self.mux.active_terminal_size();

                if let Some(rc) = &mut self.render_ctx {
                    // Advance epoch once per frame so LRU eviction can age unused entries.
                    rc.renderer.atlas.next_epoch();
                    if let Some(lcd) = rc.renderer.get_lcd_atlas() {
                        lcd.borrow_mut().next_epoch();
                    }

                    // Proactive eviction: when the main atlas is 90% full, drop entries not
                    // touched in the last 60 frames (~1 second at 60fps).
                    if rc.renderer.atlas.is_near_full() {
                        let evicted = rc.renderer.atlas.evict_cold(60);
                        if evicted > 0 {
                            // evict_cold() removes logical entries but the physical texture
                            // is unchanged — cached UV coordinates in row caches remain valid.
                            // Only flush row caches after an actual clear() (TD-PERF-07).
                            log::debug!("Atlas eviction: removed {} stale glyphs", evicted);
                            // If the physical cursor is still >75% full after logical eviction,
                            // the next uploads would fail quickly. Clear the texture now and
                            // invalidate all row caches (UVs now point to wiped data).
                            if rc.renderer.atlas.cursor_fill_ratio() > 0.75 {
                                rc.renderer.atlas.clear(&rc.renderer.device());
                                if let Some(lcd) = rc.renderer.get_lcd_atlas() {
                                    lcd.borrow_mut().clear(&rc.renderer.device());
                                    rc.shaper.clear_lcd_rasterizer_cache();
                                }
                                rc.renderer.rebuild_atlas_bind_groups();
                                rc.atlas_generation += 1;
                                rc.clear_all_row_caches();
                                log::debug!(
                                    "Atlas: cursor still high after eviction — preemptive clear"
                                );
                            }
                        }
                    }

                    // Proactive eviction for the LCD atlas (TD-MEM-02).
                    if let Some(lcd) = rc.renderer.get_lcd_atlas() {
                        if lcd.borrow().is_near_full() {
                            let evicted = lcd.borrow_mut().evict_cold(60);
                            if evicted > 0 {
                                log::debug!("LCD atlas: evicted {} cold glyphs", evicted);
                            }
                        }
                    }

                    let scaled_font = rc.scaled_font_config(&self.config);

                    // ── Search: run query if dirty, scroll to current match ──────────────
                    let active_tid = self.mux.focused_terminal_id();
                    if self.ui.search_bar.visible && self.ui.search_bar.dirty {
                        let query = self.ui.search_bar.query.clone();
                        if query.is_empty() {
                            self.ui.search_bar.matches.clear();
                            self.ui.search_bar.current = 0;
                        } else {
                            // Incremental path: when the new query extends the previous one,
                            // filter existing matches instead of scanning the full grid (TD-PERF-11).
                            let prev_query = self.ui.search_bar.last_query.clone();
                            let can_filter = !self.ui.search_bar.matches.is_empty()
                                && query.starts_with(prev_query.as_str())
                                && !prev_query.is_empty();
                            let matches = if can_filter {
                                self.mux.filter_matches(&self.ui.search_bar.matches, &query)
                            } else {
                                self.mux.search_active_terminal(&query)
                            };
                            self.ui.search_bar.set_matches(matches);
                        }
                        self.ui.search_bar.last_query = query;
                        self.ui.search_bar.dirty = false;
                    }
                    if self.ui.search_bar.visible && self.ui.search_bar.scroll_needed {
                        if let Some(m) = self.ui.search_bar.current_match().cloned() {
                            if let Some(terminal) = self.mux.active_terminal() {
                                let (disp_off, _) = terminal.scrollback_info();
                                let screen_rows = terminal.rows as i32;
                                // Target: center the match in the viewport.
                                let target_offset = (screen_rows / 2 - m.grid_line).max(0) as usize;
                                let delta = disp_off as i32 - target_offset as i32;
                                if delta != 0 {
                                    terminal.scroll_display(-delta);
                                }
                            }
                        }
                        self.ui.search_bar.scroll_needed = false;
                    }

                    // ── Per-pane layout (reuse Vec allocation from rc — TD-PERF-40) ─────
                    let mut pane_infos = std::mem::take(&mut rc.pane_infos);
                    self.mux.fill_active_pane_infos(
                        viewport,
                        cell_w as f32,
                        cell_h as f32,
                        &mut pane_infos,
                    );

                    // ── Build cell instances for every pane ──────────────────────────────
                    let search_arg = if self.ui.search_bar.visible {
                        Some(&self.ui.search_bar)
                    } else {
                        None
                    };
                    let render_result = build_all_pane_instances(
                        rc,
                        &pane_infos,
                        &self.mux,
                        &self.config,
                        &scaled_font,
                        self.input.cursor_blink_on,
                        search_arg,
                        active_tid,
                    );

                    if let Err(crate::renderer::atlas::AtlasError::Full) = render_result {
                        // Atlas full — clear everything and retry.
                        rc.renderer.atlas.clear(&rc.renderer.device());
                        if let Some(atlas) = rc.renderer.get_lcd_atlas() {
                            atlas.borrow_mut().clear(&rc.renderer.device());
                            // LCD atlas clear invalidates the rasterizer's local cache (TD-MEM-02).
                            rc.shaper.clear_lcd_rasterizer_cache();
                        }
                        // Bind groups held stale wgpu TextureViews after clear() (TD-MEM-03).
                        rc.renderer.rebuild_atlas_bind_groups();
                        rc.clear_all_row_caches();
                        rc.atlas_generation += 1;
                        let _ = build_all_pane_instances(
                            rc,
                            &pane_infos,
                            &self.mux,
                            &self.config,
                            &scaled_font,
                            self.input.cursor_blink_on,
                            search_arg,
                            active_tid,
                        );
                    }

                    // Pane separator lines (one RoundedRectInstance per separator).
                    let sep_pad_x = self.config.window.padding.left as f32 + sidebar_px_snapshot;
                    rc.build_pane_separators(&self.separator_snapshot, sep_pad_x, sb_pad_y);
                    // Focus border — only when there are multiple panes.
                    if pane_infos.len() > 1 {
                        if let Some(focused) = pane_infos.iter().find(|p| p.focused) {
                            rc.build_focus_border(focused, &self.config.colors);
                        }
                    }

                    rc.pane_infos = pane_infos;

                    // ── Tab bar / unified titlebar (always shown in Custom mode) ────────
                    let renaming = self.ui.is_renaming_tab();
                    if tab_bar_vis || renaming {
                        let tab_total_cols = total_cols
                            + if self.ui.is_panel_visible() {
                                self.ui.panel().width_cols as usize
                            } else {
                                0
                            };
                        let rename_input = self.ui.tab_rename_input.as_deref();
                        let active_idx = self.mux.tabs.active_index();

                        // Fast comparison: check copiable inputs before hashing (TD-PERF-16).
                        let inputs_match =
                            if let Some((cached_idx, cached_cols)) = rc.tab_bar_inputs {
                                cached_idx == active_idx
                                    && cached_cols == tab_total_cols
                                    && rc.tab_bar_rename_input.as_deref() == rename_input
                            } else {
                                false
                            };

                        let titles_match = {
                            let current_titles: Vec<String> = self
                                .mux
                                .tabs
                                .tabs()
                                .iter()
                                .map(|t| t.title.clone())
                                .collect();
                            rc.tab_bar_titles == current_titles
                        };

                        let tab_key_changed = !inputs_match || !titles_match;

                        if !tab_key_changed && !rc.tab_bar_instances_cache.is_empty() {
                            // Tabs unchanged — append cached instances (TD-PERF-09/16).
                            rc.instances.extend_from_slice(&rc.tab_bar_instances_cache);
                            rc.rect_instances.extend_from_slice(&rc.tab_bar_rects_cache);
                        } else {
                            let inst_start = rc.instances.len();
                            let rect_start = rc.rect_instances.len();
                            let win_w = rc.renderer.size().0 as f32;
                            let gpu_pad_y = TITLEBAR_HEIGHT * rc.scale_factor
                                + self.config.window.padding.top as f32;
                            rc.build_tab_bar_instances(
                                self.mux.tabs.tabs(),
                                active_idx,
                                &scaled_font,
                                tab_total_cols,
                                win_w,
                                self.config.window.padding.left as f32 + sidebar_px_snapshot,
                                gpu_pad_y,
                                self.config.colors.background,
                                self.sidebar_visible,
                                self.ui.is_panel_visible(),
                                rename_input,
                                &self.config.colors,
                            );
                            rc.tab_bar_instances_cache.clear();
                            rc.tab_bar_instances_cache
                                .extend_from_slice(&rc.instances[inst_start..]);
                            rc.tab_bar_rects_cache.clear();
                            rc.tab_bar_rects_cache
                                .extend_from_slice(&rc.rect_instances[rect_start..]);

                            // Cache the inputs for next frame comparison (TD-PERF-16).
                            rc.tab_bar_inputs = Some((active_idx, tab_total_cols));
                            rc.tab_bar_titles = self
                                .mux
                                .tabs
                                .tabs()
                                .iter()
                                .map(|t| t.title.clone())
                                .collect();
                            rc.tab_bar_rename_input = rename_input.map(|s| s.to_string());
                        }
                    }

                    // ── Scroll bar (overlays right edge of terminal) ─────────────────────
                    if self.config.enable_scroll_bar {
                        if let Some(terminal) = self.mux.active_terminal() {
                            let (disp_off, hist) = terminal.scrollback_info();
                            let sb_state = (disp_off, hist, term_rows, term_cols);
                            if rc.scroll_bar_state.as_ref() == Some(&sb_state) {
                                // Geometry unchanged — append cached instances (TD-PERF-08).
                                rc.instances.extend_from_slice(&rc.scroll_bar_cache);
                            } else {
                                let start = rc.instances.len();
                                rc.build_scroll_bar_instances(
                                    disp_off,
                                    hist,
                                    term_rows,
                                    term_cols,
                                    &self.config.colors,
                                );
                                rc.scroll_bar_cache.clear();
                                rc.scroll_bar_cache
                                    .extend_from_slice(&rc.instances[start..]);
                                rc.scroll_bar_state = Some(sb_state);
                            }
                        }
                    }

                    // ── Chat panel (side panel) ───────────────────────────────────────────
                    let panel_visible = self.ui.is_panel_visible();
                    if panel_visible {
                        let panel_dirty = self.ui.panel_mut().dirty;
                        let panel_focused = self.ui.panel_focused;
                        let file_picker_focused = self.ui.file_picker_focused;
                        let blink = self.input.cursor_blink_on;
                        let force_rebuild = matches!(
                            self.ui.panel().state,
                            crate::llm::chat_panel::PanelState::Loading
                                | crate::llm::chat_panel::PanelState::Streaming
                        );
                        // If window was resized, term_cols changed — invalidate panel cache.
                        let panel_cols_changed = rc.panel_cache_term_cols != total_cols;
                        if panel_dirty || force_rebuild || panel_cols_changed {
                            let panel_start = rc.instances.len();
                            let rect_start = rc.rect_instances.len();
                            self.ui.panel_mut().dirty = false;
                            // Pre-wrap message lines once per dirty rebuild (TD-PERF-05).
                            let msg_wrap_w = self.ui.panel().width_cols.saturating_sub(8) as usize;
                            self.ui.panel_mut().ensure_wrap_cache(msg_wrap_w);
                            // Pre-build separator cache (TD-PERF-13).
                            let panel_cols = self.ui.panel().width_cols as usize;
                            self.ui.panel_mut().separator(panel_cols);
                            rc.build_chat_panel_instances(
                                self.ui.panel(),
                                self.ui.active_panel_id(),
                                panel_focused,
                                file_picker_focused,
                                &self.config,
                                &scaled_font,
                                total_cols,
                                total_rows,
                                blink,
                                self.config.window.padding.left as f32 + sidebar_px_snapshot,
                                sb_pad_y,
                            );
                            rc.panel_instances_cache.clear();
                            rc.panel_instances_cache
                                .extend_from_slice(&rc.instances[panel_start..]);
                            rc.panel_rect_cache.clear();
                            rc.panel_rect_cache
                                .extend_from_slice(&rc.rect_instances[rect_start..]);
                            rc.panel_cache_term_cols = total_cols;
                        } else {
                            rc.instances.extend_from_slice(&rc.panel_instances_cache);
                            rc.rect_instances.extend_from_slice(&rc.panel_rect_cache);
                        }
                        // Input rows (2 lines + hints) are always rebuilt fresh — cursor
                        // blink only touches these 3 rows, not the full message history (TD-PERF-10).
                        rc.build_chat_panel_input_rows(
                            self.ui.panel(),
                            panel_focused,
                            file_picker_focused,
                            &self.config,
                            &scaled_font,
                            total_cols,
                            total_rows,
                            blink,
                            self.config.window.padding.left as f32 + sidebar_px_snapshot,
                            sb_pad_y,
                        );
                    }

                    // ── Inline AI block (overlays bottom rows) ──────────────────────────
                    let block_visible = self.ui.is_block_visible();
                    if block_visible && self.ui.ai_block.dirty {
                        self.ui.ai_block.dirty = false;
                        rc.build_ai_block_instances(
                            &self.ui.ai_block,
                            &scaled_font,
                            total_cols,
                            total_rows,
                        );
                    }

                    // ── Status bar ───────────────────────────────────────────────────────
                    if self.config.status_bar.enabled {
                        // Use cached CWD and exit code (TD-PERF-01, TD-PERF-02).
                        // Git branch is polled on an independent timer in about_to_wait (TD-PERF-19).
                        let leader_resize_mode = (self.input.leader_active
                            && self.input.modifiers.state().alt_key())
                            || self.input.resize_mode
                            || self.input.dragging_separator.is_some();
                        let sb_total_cols = total_cols
                            + if panel_visible {
                                self.ui.panel().width_cols as usize
                            } else {
                                0
                            };
                        let sb_win_w = rc.renderer.size().0 as f32;
                        // Key: all inputs that affect segment text + layout (TD-PERF-10).
                        // Include current minute so the time widget invalidates the cache
                        // at each minute boundary (the WaitUntil in about_to_wait wakes us).
                        let sb_key = {
                            let flags = [
                                self.input.leader_active as u8,
                                leader_resize_mode as u8,
                                sb_exit_code_raw as u8,
                                self.battery_status
                                    .as_ref()
                                    .map(|s| s.percent)
                                    .unwrap_or(255),
                                self.battery_status
                                    .as_ref()
                                    .map(|s| s.on_battery as u8)
                                    .unwrap_or(0),
                            ];
                            let col_bytes = sb_total_cols.to_le_bytes();
                            let row_bytes = total_rows.to_le_bytes();
                            let win_w_bits = sb_win_w.to_bits().to_le_bytes();
                            let cwd_bytes = self
                                .cached_cwd
                                .as_ref()
                                .and_then(|p| p.to_str())
                                .unwrap_or("")
                                .as_bytes();
                            let branch_bytes =
                                self.ui.git_branch_cache.as_deref().unwrap_or("").as_bytes();
                            let leader_bytes = self.config.leader.key.as_bytes();
                            let mins_now = std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .map(|d| (d.as_secs() / 60) as u32)
                                .unwrap_or(0);
                            let mins_bytes = mins_now.to_le_bytes();
                            static_hash(&[
                                &flags,
                                &col_bytes,
                                &row_bytes,
                                &win_w_bits,
                                cwd_bytes,
                                branch_bytes,
                                leader_bytes,
                                &mins_bytes,
                            ])
                        };
                        if rc.status_bar_key == sb_key && !rc.status_bar_instances_cache.is_empty()
                        {
                            // Inputs unchanged — append cached instances (TD-PERF-10).
                            rc.instances
                                .extend_from_slice(&rc.status_bar_instances_cache);
                            rc.rect_instances
                                .extend_from_slice(&rc.status_bar_rect_cache);
                        } else {
                            let inst_start = rc.instances.len();
                            let rect_start = rc.rect_instances.len();
                            let battery = self
                                .battery_status
                                .as_ref()
                                .map(|s| (s.percent, s.on_battery));
                            let bar = crate::ui::status_bar::StatusBar::build(
                                self.input.leader_active,
                                leader_resize_mode,
                                &self.config.leader.key,
                                self.cached_cwd.as_deref(),
                                self.ui.git_branch_cache.as_deref(),
                                sb_exit_code,
                                self.config.status_bar.style.clone(),
                                battery,
                            );
                            rc.build_status_bar_instances(
                                &bar,
                                &scaled_font,
                                sb_total_cols,
                                total_rows,
                                sb_pad_y,
                                sb_win_w,
                                &self.config.colors,
                            );
                            rc.status_bar_instances_cache.clear();
                            rc.status_bar_instances_cache
                                .extend_from_slice(&rc.instances[inst_start..]);
                            rc.status_bar_rect_cache.clear();
                            rc.status_bar_rect_cache
                                .extend_from_slice(&rc.rect_instances[rect_start..]);
                            rc.status_bar_key = sb_key;
                        }
                    }

                    if self.sidebar_visible {
                        let counts = self.mux.workspace_tab_pane_counts();
                        rc.build_workspace_sidebar_instances(
                            self.mux.workspaces(),
                            self.mux.active_workspace_id,
                            self.sidebar_nav_cursor,
                            self.sidebar_rename_input.as_deref(),
                            SIDEBAR_COLS,
                            &counts,
                            self.config.window.padding.left as f32,
                            sb_pad_y,
                            self.config.window.padding.bottom as f32,
                            &scaled_font,
                            &self.config.colors,
                        );
                        let sidebar_sep_x = self.config.window.padding.left as f32
                            + SIDEBAR_COLS as f32 * rc.shaper.cell_width;
                        let sidebar_sep_y = sb_pad_y;
                        let sidebar_sep_h = (rc.renderer.size().1 as f32
                            - sidebar_sep_y
                            - self.config.window.padding.bottom as f32)
                            .max(0.0);
                        rc.rect_instances.push(
                            crate::renderer::rounded_rect::RoundedRectInstance {
                                rect: [sidebar_sep_x, sidebar_sep_y, 1.0, sidebar_sep_h],
                                color: self.config.colors.ui_muted,
                                radius: 0.0,
                                border_width: 0.0,
                                _pad: [0.0; 2],
                            },
                        );
                    }

                    // ── Overlays (search bar, palette, context menu) ─────────────────────
                    // Record the split point so the GPU renders these in a separate pass.
                    let overlay_start = rc.instances.len();
                    if self.ui.search_bar.visible {
                        rc.build_search_bar_instances(
                            &self.ui.search_bar,
                            &scaled_font,
                            total_cols,
                            total_rows,
                            &self.config.colors,
                        );
                    }
                    if self.ui.palette.visible {
                        let palette_cols = total_cols
                            + if panel_visible {
                                self.ui.panel().width_cols as usize
                            } else {
                                0
                            };
                        rc.build_palette_instances(
                            &self.ui.palette,
                            &scaled_font,
                            palette_cols,
                            total_rows,
                            self.config.window.padding.left as f32 + sidebar_px_snapshot,
                            sb_pad_y,
                            &self.config.colors,
                        );
                    }
                    if self.ui.context_menu.visible {
                        rc.build_context_menu_instances(
                            &self.ui.context_menu,
                            &scaled_font,
                            total_cols,
                            total_rows,
                            &self.config.colors,
                        );
                    }

                    // ── Toast notification ──────────────────────────────────────────────
                    let toast_active = self
                        .toast
                        .as_ref()
                        .is_some_and(|(_, deadline)| std::time::Instant::now() < *deadline);
                    if toast_active {
                        if let Some((msg, _)) = &self.toast {
                            let pad_x =
                                self.config.window.padding.left as f32 + sidebar_px_snapshot;
                            rc.build_toast_instances(
                                msg,
                                &scaled_font,
                                total_cols,
                                pad_x,
                                sb_pad_y,
                                &self.config.colors,
                            );
                        }
                    } else {
                        self.toast = None;
                    }

                    // ── Debug HUD (F12) — rendered last so it appears above all overlays ─
                    if rc.hud_visible {
                        rc.build_debug_hud_instances(&scaled_font);
                    }

                    // ── GPU upload ──────────────────────────────────────────────────────
                    rc.last_instance_count = rc.instances.len();
                    rc.last_overlay_start = overlay_start;
                    {
                        use crate::renderer::cell::CellVertex;
                        use crate::renderer::rounded_rect::RoundedRectInstance;
                        let instance_bytes = rc.instances.len() * std::mem::size_of::<CellVertex>();
                        let lcd_bytes = rc.lcd_instances.len() * std::mem::size_of::<CellVertex>();
                        let rect_bytes =
                            rc.rect_instances.len() * std::mem::size_of::<RoundedRectInstance>();
                        rc.last_gpu_upload_bytes = instance_bytes + lcd_bytes + rect_bytes;
                    }
                    rc.renderer.upload_rect_instances(&rc.rect_instances);
                    rc.renderer.set_overlay_start(overlay_start);
                    rc.renderer.upload_instances(&rc.instances, 0);
                    rc.renderer.set_cell_count(rc.instances.len());
                    rc.renderer.upload_lcd_instances(&rc.lcd_instances);
                    let _ = rc.renderer.render();

                    // ── Input-to-pixel latency probe (RUST_LOG=petruterm=debug) ─────────
                    // Only logged when PTY data arrived this frame (echo of a keypress).
                    if !data_ids.is_empty() {
                        if let Some(t) = self.input.last_key_instant.take() {
                            let latency_ms = t.elapsed().as_secs_f32() * 1000.0;
                            log::debug!("input-to-pixel: {:.1}ms", latency_ms);
                            rc.latency_samples.push_back(latency_ms);
                            if rc.latency_samples.len() > 120 {
                                rc.latency_samples.pop_front();
                            }
                        }
                    }

                    // ── Frame time tracking for HUD ─────────────────────────────────────
                    let frame_ms = frame_start.elapsed().as_secs_f32() * 1000.0;
                    rc.frame_times.push_back(frame_ms);
                    if rc.frame_times.len() > 120 {
                        rc.frame_times.pop_front();
                    }

                    // Suppress unused warning for scroll-bar focused dimensions.
                    let _ = (term_cols, term_rows);
                }
            }
            WindowEvent::Resized(size) => {
                if let Some(rc) = &mut self.render_ctx {
                    rc.renderer.resize(size.width, size.height);
                }
                self.resize_terminals_for_panel();
                self.ui.ai_block.dirty = true;
                if let Some(w) = &self.window {
                    w.request_redraw();
                }
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
                if self.handle_sidebar_key(&event) {
                    if let Some(w) = &self.window {
                        w.request_redraw();
                    }
                    return;
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
                    self.sidebar_visible = !self.sidebar_visible;
                    self.sidebar_rename_input = None;
                    self.sidebar_kbd_active = false;
                    self.clamp_sidebar_cursor();
                    self.apply_tab_bar_padding();
                    self.resize_terminals_for_panel();
                }
                if let Some(w) = &self.window {
                    w.request_redraw();
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.input.mouse_pos = (position.x, position.y);
                let (col, row) = self.pixel_to_cell(position.x, position.y);
                // Update context menu hover — redraw if hovered item changed.
                if self.ui.context_menu.update_hover(col, row) {
                    if let Some(w) = &self.window {
                        w.request_redraw();
                    }
                }
                // Separator drag — update ratio live.
                if let Some(drag) = &self.input.dragging_separator {
                    let node_id = drag.node_id;
                    self.mux
                        .cmd_drag_separator(node_id, position.x as f32, position.y as f32);
                    self.resize_terminals_for_panel();
                    if let Some(w) = &self.window {
                        w.request_redraw();
                    }
                } else if self.input.mouse_left_pressed && !self.mouse_in_panel() {
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
                        if let Some(w) = &self.window {
                            w.request_redraw();
                        }
                    }
                }
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let in_panel = self.mouse_in_panel();
                let (col, row) = self.pixel_to_cell(self.input.mouse_pos.0, self.input.mouse_pos.1);
                match (button, state) {
                    (MouseButton::Left, ElementState::Pressed) => {
                        if self.sidebar_visible {
                            let sf = self
                                .render_ctx
                                .as_ref()
                                .map(|rc| rc.scale_factor as f64)
                                .unwrap_or(1.0);
                            // Sidebar occupies the visual column area only (not the margin).
                            let sidebar_visual_right = self.config.window.padding.left as f64
                                + SIDEBAR_COLS as f64 * self.cell_dims().0 as f64;
                            let sidebar_left = self.config.window.padding.left as f64;
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
                                    + self.tab_bar_height_px() as f64;
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
                                    self.sidebar_nav_cursor =
                                        self.mux.workspace_count().saturating_sub(1);
                                    self.apply_tab_bar_padding();
                                    self.resize_terminals_for_panel();
                                    if let Some(w) = &self.window {
                                        w.request_redraw();
                                    }
                                    return;
                                }
                                let list_top = sidebar_top + 2.0 * cell_h as f64;
                                let rel_y = self.input.mouse_pos.1 - list_top;
                                if rel_y >= 0.0 {
                                    let idx = (rel_y / (2.0 * cell_h as f64)).floor() as usize;
                                    if idx < self.mux.workspace_count() {
                                        self.sidebar_nav_cursor = idx;
                                        // usize::MAX sentinel distinguishes sidebar clicks
                                        // from terminal cell clicks in register_click.
                                        let clicks = self.input.register_click((idx, usize::MAX));
                                        if clicks >= 2 {
                                            // Double-click: enter rename mode.
                                            if let Some(ws) = self.mux.workspaces().get(idx) {
                                                self.sidebar_rename_input = Some(ws.name.clone());
                                            }
                                        } else {
                                            // Single click: switch workspace and close.
                                            self.sidebar_rename_input = None;
                                            if let Some(id) = self.sidebar_selected_workspace_id() {
                                                self.mux.cmd_switch_workspace(id);
                                                self.refresh_status_cache();
                                            }
                                            self.sidebar_visible = false;
                                            self.apply_tab_bar_padding();
                                            self.resize_terminals_for_panel();
                                        }
                                    }
                                }
                                if let Some(w) = &self.window {
                                    w.request_redraw();
                                }
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
                                        let selected = self
                                            .mux
                                            .active_terminal()
                                            .and_then(|t| t.selection_text());
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
                                    ContextAction::Separator | ContextAction::Label => {}
                                }
                                if let Some(w) = &self.window {
                                    w.request_redraw();
                                }
                                return;
                            } else {
                                // Click outside menu closes it.
                                self.ui.context_menu.close();
                                if let Some(w) = &self.window {
                                    w.request_redraw();
                                }
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
                                self.sidebar_visible = !self.sidebar_visible;
                                self.sidebar_rename_input = None;
                                self.sidebar_kbd_active = false;
                                self.clamp_sidebar_cursor();
                                self.apply_tab_bar_padding();
                                self.resize_terminals_for_panel();
                                if let Some(w) = &self.window {
                                    w.request_redraw();
                                }
                                return;
                            }
                            // AI panel toggle button: logical [106..128]
                            if x >= 106.0 * sf && x < 128.0 * sf {
                                let terminal_id = self.mux.focused_terminal_id();
                                if self.ui.is_panel_visible() {
                                    self.ui.panel_mut().close();
                                } else {
                                    let cwd = self
                                        .mux
                                        .active_cwd()
                                        .or_else(|| std::env::current_dir().ok())
                                        .unwrap_or_default();
                                    self.ui.open_panel_with_context(terminal_id, cwd);
                                }
                                self.resize_terminals_for_panel();
                                if let Some(w) = &self.window {
                                    w.request_redraw();
                                }
                                return;
                            }
                            // Tab click
                            if let Some(idx) = self.hit_test_tab_bar(x) {
                                self.mux.tabs.switch_to_index(idx);
                                self.resize_terminals_for_panel();
                                self.refresh_status_cache();
                                if let Some(w) = &self.window {
                                    w.request_redraw();
                                }
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
                            && self.input.mouse_pos.1
                                < self.config.window.padding.top as f64 + tab_h
                        {
                            if let Some(idx) = self.hit_test_tab_bar(self.input.mouse_pos.0) {
                                self.mux.tabs.switch_to_index(idx);
                                self.resize_terminals_for_panel();
                                self.refresh_status_cache();
                            }
                            if let Some(w) = &self.window {
                                w.request_redraw();
                            }
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
                            if self.input.mouse_pos.1 >= sb_top
                                && self.input.mouse_pos.1 < sb_bottom
                            {
                                let col = ((self.input.mouse_pos.0
                                    - self.config.window.padding.left as f64)
                                    / cell_w as f64)
                                    .floor()
                                    .max(0.0) as usize;
                                let total_cols = self.mux.active_terminal_size().0;
                                let cwd = self.mux.active_cwd();
                                let git_branch = self.ui.git_branch_cache.clone();
                                let bar = crate::ui::status_bar::StatusBar::build(
                                    false,
                                    false,
                                    &self.config.leader.key,
                                    cwd.as_deref(),
                                    git_branch.as_deref(),
                                    None,
                                    self.config.status_bar.style.clone(),
                                    None,
                                );
                                match bar.click_kind(col, total_cols) {
                                    Some(crate::ui::status_bar::SegmentKind::GitBranch) => {
                                        if let Some(cwd_path) = self
                                            .mux
                                            .active_cwd()
                                            .or_else(|| std::env::current_dir().ok())
                                        {
                                            self.ui.open_branch_picker(&cwd_path);
                                            if let Some(w) = &self.window {
                                                w.request_redraw();
                                            }
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
                                                if let Some(w) = &self.window {
                                                    w.request_redraw();
                                                }
                                            }
                                        }
                                    }
                                    _ => {}
                                }
                                return;
                            }
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
                            if let Some(w) = &self.window {
                                w.request_redraw();
                            }
                            return;
                        }

                        if in_panel {
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
                                        if let Some(w) = &self.window {
                                            w.request_redraw();
                                        }
                                    }
                                }
                            }
                            self.input.send_mouse_report(0, col, row, false, &self.mux);
                        }
                    }
                    // In mouse-reporting mode, pass right-click to the terminal app.
                    // Otherwise, open the context menu.
                    (MouseButton::Right, ElementState::Pressed) if !in_panel => {
                        let (any_mouse, _, _) = self
                            .mux
                            .active_terminal()
                            .map(|t| t.mouse_mode_flags())
                            .unwrap_or((false, false, false));
                        if any_mouse {
                            self.input.send_mouse_report(2, col, row, true, &self.mux);
                        } else {
                            let (term_cols, term_rows) = self.mux.active_terminal_size();
                            self.ui.context_menu.open(col, row, term_cols, term_rows);
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
                if let Some(w) = &self.window {
                    w.request_redraw();
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let scale = self
                    .render_ctx
                    .as_ref()
                    .map(|rc| rc.scale_factor as f64)
                    .unwrap_or(1.0);
                let delta_lines = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y as f64,
                    // pos.y is in logical points; divide by logical cell height to get lines.
                    MouseScrollDelta::PixelDelta(pos) => {
                        -pos.y / (self.cell_dims().1 as f64 / scale)
                    }
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
                    if let Some(w) = &self.window {
                        w.request_redraw();
                    }
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
                    if let Some(w) = &self.window {
                        w.request_redraw();
                    }
                }
            }
            WindowEvent::DroppedFile(path) => {
                let path_str = path.to_string_lossy().into_owned();
                if self.ui.is_panel_visible() {
                    self.ui.panel_mut().append_path(&path_str);
                } else if let Some(terminal) = self.mux.active_terminal() {
                    terminal.write_input(path_str.as_bytes());
                }
                if let Some(w) = &self.window {
                    w.request_redraw();
                }
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
                if let Some(w) = &self.window {
                    w.request_redraw();
                }
            }
        }

        // Drain any PTY events that arrived since user_event last ran.
        // This catches batches that slipped in after user_event drained the channel,
        // and keeps last_pty_activity accurate for coalescing.
        let (data_ids, exited) = self.mux.poll_pty_events();
        if self.close_exited_terminals(exited) {
            event_loop.exit();
            return;
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
                if let Some(w) = &self.window {
                    w.request_redraw();
                }
            }
        }

        let had_ai = self.ui.poll_ai_events() || self.ui.poll_ai_block_events();
        let scan_ready = self.ui.poll_file_scan();
        if had_ai || scan_ready {
            if let Some(w) = &self.window {
                w.request_redraw();
            }
        }
        self.flush_pending_pty_run();
        self.flush_pending_paste();

        // ── Battery status poll (every 30 s, or immediately on first frame) ─
        if self.battery_status.is_none()
            || self.battery_last_poll.elapsed() >= std::time::Duration::from_secs(30)
        {
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
                        // Switch present mode immediately: Fifo (vsync) on battery to
                        // reduce GPU wakeup frequency; best available mode on AC power.
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
                    if let Some(w) = &self.window {
                        w.request_redraw();
                    }
                }
            }
        }

        // ── Independent git branch poll (TD-PERF-19) ────────────────────────
        // Runs at most once per second, regardless of PTY/render activity.
        // Removed from the render hot path (was called every RedrawRequested).
        // TTL is 5 s normally, 30 s in battery-saver mode.
        if self.config.status_bar.enabled
            && self.window_focused
            && self.ui.git_branch_last_poll.elapsed() >= std::time::Duration::from_secs(1)
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
                // Invalidate status bar key so it rebuilds on the next frame.
                if let Some(rc) = &mut self.render_ctx {
                    rc.status_bar_key = 0;
                }
                if let Some(w) = &self.window {
                    w.request_redraw();
                }
            }
        }

        // ── Idle detection ───────────────────────────────────────────────────
        // The frame is "idle" when there is no PTY data, no AI events, no active
        // drag, no overlay, and no search bar open. When idle, we skip cursor blink
        // entirely (many terminals do this) and use ControlFlow::Wait so the OS
        // keeps the event loop dormant until a real event arrives.
        //
        // The AI panel alone does NOT prevent idle when it is in the background
        // (terminal focused, nothing streaming). The panel cursor goes solid and the
        // thread parks — same energy profile as a plain terminal. Blink resumes as
        // soon as the user focuses the panel or LLM output starts arriving
        // (had_ai = true from poll_ai_events).
        let any_overlay = (self.ui.is_panel_visible() && self.ui.panel_focused)
            || self.ui.palette.visible
            || self.ui.context_menu.visible
            || self.ui.search_bar.visible
            || self.ui.is_block_visible();
        let any_drag = self.input.dragging_separator.is_some() || self.input.mouse_left_pressed;
        let idle =
            !had_pty_data && !had_ai && !self.pending_pty_redraw && !any_overlay && !any_drag;

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
            if let Some(w) = &self.window {
                w.request_redraw();
            }
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
                if let Some(w) = &self.window {
                    w.request_redraw();
                }
            }
            // else: WaitUntil below will wake us at pty_deadline to retry.
        }

        // Keep redrawing while a toast is active; clear it once expired.
        if let Some((_, deadline)) = &self.toast {
            if std::time::Instant::now() < *deadline {
                if let Some(w) = &self.window {
                    w.request_redraw();
                }
            } else {
                self.toast = None;
                if let Some(w) = &self.window {
                    w.request_redraw(); // one final frame to clear the toast
                }
            }
        }

        // Schedule a wakeup at the top of the next minute so the status bar time
        // widget stays accurate even when the thread would otherwise be parked.
        let next_minute_wake = {
            let now_inst = std::time::Instant::now();
            let secs_now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            let secs_remaining = 60 - (secs_now % 60);
            now_inst + std::time::Duration::from_secs(secs_remaining)
        };

        if idle || !self.window_focused {
            // Fully idle or window not focused: park the thread.
            // When unfocused, blink is suspended — no need to schedule a wakeup for it.
            // PTY data still arrives via user_event (winit wakes us), so background processes run.
            // Wake at the next minute boundary to refresh the status bar clock.
            let wake = if self.pending_pty_redraw {
                pty_deadline.min(next_minute_wake)
            } else {
                next_minute_wake
            };
            event_loop.set_control_flow(winit::event_loop::ControlFlow::WaitUntil(wake));
        } else if self.battery_saver_active {
            // On battery: no blink timer — cursor stays solid, thread parks until
            // PTY data, user input, or the minute boundary for the status bar clock.
            let wake = if self.pending_pty_redraw {
                pty_deadline.min(next_minute_wake)
            } else {
                next_minute_wake
            };
            event_loop.set_control_flow(winit::event_loop::ControlFlow::WaitUntil(wake));
        } else {
            let blink_deadline =
                self.input.cursor_last_blink + std::time::Duration::from_millis(530);
            let wake = if self.pending_pty_redraw {
                blink_deadline.min(pty_deadline).min(next_minute_wake)
            } else {
                blink_deadline.min(next_minute_wake)
            };
            event_loop.set_control_flow(winit::event_loop::ControlFlow::WaitUntil(wake));
        }
    }
}

/// Build and upload cell instances for every pane in the active tab.
/// Calls `rc.begin_frame()` first to clear previous frame's instances.
#[allow(clippy::too_many_arguments)]
fn build_all_pane_instances(
    rc: &mut RenderContext,
    pane_infos: &[crate::ui::PaneInfo],
    mux: &Mux,
    config: &crate::config::Config,
    font: &crate::config::schema::FontConfig,
    cursor_blink_on: bool,
    search_bar: Option<&crate::ui::SearchBar>,
    active_terminal_id: usize,
) -> Result<(), crate::renderer::atlas::AtlasError> {
    rc.begin_frame();
    // Take the scratch buffer out of rc so we can mutably borrow rc (build_instances)
    // while the buffer is filled by mux. Returned at the end of the function.
    let mut cell_data_scratch = std::mem::take(&mut rc.cell_data_scratch);
    let mut last_scratch_tid = rc.scratch_terminal_id.take();
    for info in pane_infos {
        // If the terminal changed (tab switch or split panes with different terminals),
        // clear the scratch buffer so collect_grid_cells_for treats all rows as damaged.
        // Without this, undamaged-row skipping retains stale data from the previous
        // terminal, causing TUI app content to bleed into unrelated tabs.
        let terminal_changed = last_scratch_tid != Some(info.terminal_id);
        if terminal_changed {
            cell_data_scratch.clear();
        }
        // Pass search highlight info only for the active pane with a non-empty query.
        let search_arg = search_bar.and_then(|sb| {
            if !sb.query.is_empty()
                && !sb.matches.is_empty()
                && info.terminal_id == active_terminal_id
            {
                Some((sb.matches.as_slice(), sb.current))
            } else {
                None
            }
        });
        mux.collect_grid_cells_for(
            info.terminal_id,
            &mut cell_data_scratch,
            search_arg,
            terminal_changed,
        );
        let cell_data = &cell_data_scratch[..];
        rc.build_instances(
            cell_data,
            config,
            font,
            info.terminal_id,
            info.col_offset,
            info.row_offset,
        )?;
        last_scratch_tid = Some(info.terminal_id);
    }
    rc.cell_data_scratch = cell_data_scratch;
    rc.scratch_terminal_id = last_scratch_tid;

    // Record content boundary before cursor — used by the fast blink path.
    rc.content_end = rc.instances.len();

    // Emit cursor for the focused pane (always after content_end).
    if let Some(info) = pane_infos.iter().find(|i| i.focused) {
        if let Some(cursor) = mux
            .terminals
            .get(info.terminal_id)
            .and_then(|s| s.as_ref())
            .map(|t| t.cursor_info())
        {
            rc.build_cursor_instance(
                &cursor,
                cursor_blink_on,
                info.col_offset,
                info.row_offset,
                config,
            );
        } else {
            rc.cursor_vertex_template = None;
        }
    } else {
        rc.cursor_vertex_template = None;
    }

    Ok(())
}

impl Drop for App {
    fn drop(&mut self) {
        log::info!("App dropping; shutting down PTYs.");
        self.mux.shutdown();
    }
}

/// Hash a sequence of byte slices into a single u64.
/// Used to detect whether static-geometry inputs (tab bar, status bar, scroll bar)
/// changed since the last frame, so we can skip rebuilding their GPU instances.
fn static_hash(parts: &[&[u8]]) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = rustc_hash::FxHasher::default();
    for p in parts {
        p.hash(&mut h);
    }
    h.finish()
}
