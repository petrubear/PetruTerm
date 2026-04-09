use anyhow::Result;
use std::sync::Arc;
use winit::application::ApplicationHandler;
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoopProxy};
use winit::window::{Window, WindowAttributes, WindowId};

use alacritty_terminal::selection::SelectionType;

use crate::config::{self, Config};
use crate::config::schema::TitleBarStyle;
use crate::config::watcher::ConfigWatcher;
use crate::ui::{ContextAction, Rect};

mod renderer;
mod mux;
mod ui;
mod input;

pub use renderer::RenderContext;
pub use mux::Mux;
pub use ui::UiManager;
pub use input::InputHandler;

/// Top-level application state. Delegates to specialized managers.
pub struct App {
    config: Config,
    config_watcher: Option<ConfigWatcher>,

    window: Option<Arc<Window>>,
    render_ctx: Option<RenderContext>,
    mux: Mux,
    ui: UiManager,
    input: InputHandler,

    wakeup_proxy: EventLoopProxy<()>,

    /// PTY render coalescing: when PTY data arrives, don't render immediately.
    /// Instead, wait for a short quiet window so that multi-batch TUI updates
    /// (erase + redraw) are coalesced into a single frame, preventing flickering.
    pending_pty_redraw: bool,
    last_pty_activity: std::time::Instant,
}

impl App {
    pub fn new(config: Config, wakeup_proxy: EventLoopProxy<()>) -> Self {
        let config_watcher = config::config_dir()
            .exists()
            .then(|| ConfigWatcher::new(&config::config_dir()).ok())
            .flatten();

        Self {
            config: config.clone(),
            config_watcher,
            window: None,
            render_ctx: None,
            mux: Mux::new(),
            ui: UiManager::new(&config),
            input: InputHandler::new(&config),
            wakeup_proxy,
            pending_pty_redraw: false,
            last_pty_activity: std::time::Instant::now(),
        }
    }

    fn tab_bar_visible(&self) -> bool {
        self.mux.tabs.tab_count() > 1
    }

    fn tab_bar_height_px(&self) -> f32 {
        if self.tab_bar_visible() { self.cell_dims().1 as f32 } else { 0.0 }
    }

    fn status_bar_height_px(&self) -> f32 {
        if self.config.status_bar.enabled { self.cell_dims().1 as f32 } else { 0.0 }
    }

    /// Update the GPU uniform padding to account for the tab bar (or lack thereof).
    /// Call whenever tab count crosses the 1↔2 boundary, or on initial setup.
    fn apply_tab_bar_padding(&mut self) {
        if let Some(rc) = &mut self.render_ctx {
            let tab_h = if self.mux.tabs.tab_count() > 1 { rc.shaper.cell_height } else { 0.0 };
            let pad = &self.config.window.padding;
            rc.renderer.set_padding(pad.left as f32, pad.top as f32 + tab_h);
        }
    }

    fn default_grid_size(&self) -> (u16, u16) {
        if let Some(rc) = &self.render_ctx {
            let (w, h) = rc.renderer.size();
            let (cell_w, cell_h) = self.cell_dims();
            let pad = &self.config.window.padding;
            let panel_px = if self.ui.is_panel_visible() { self.chat_panel_width_px() } else { 0.0 };
            let tab_h = self.tab_bar_height_px();
            let sb_h = self.status_bar_height_px();
            let cols = ((w as f32 - pad.left as f32 - pad.right as f32 - panel_px) / cell_w as f32).max(1.0) as u16;
            let rows = ((h as f32 - pad.top as f32 - pad.bottom as f32 - tab_h - sb_h) / cell_h as f32).max(1.0) as u16;
            (cols, rows)
        } else { (120, 40) }
    }

    fn chat_panel_width_px(&self) -> f32 {
        let (cell_w, _) = self.cell_dims();
        self.ui.panel().width_cols as f32 * cell_w as f32
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

    fn cell_dims(&self) -> (u16, u16) {
        self.render_ctx.as_ref()
            .map(|rc| (rc.shaper.cell_width as u16, rc.shaper.cell_height as u16))
            .unwrap_or((8, 16))
    }

    fn open_initial_tab(&mut self) -> Result<()> {
        let viewport = self.viewport_rect();
        let (cols, rows) = self.default_grid_size();
        let (cell_w, cell_h) = self.cell_dims();
        self.mux.open_initial_tab(&self.config, viewport, cols, rows, cell_w, cell_h, self.wakeup_proxy.clone())
    }

    fn viewport_rect(&self) -> Rect {
        let pad = &self.config.window.padding;
        let tab_h = self.tab_bar_height_px();
        let sb_h = self.status_bar_height_px();
        if let Some(rc) = &self.render_ctx {
            let (w, h) = rc.renderer.size();
            let panel_px = if self.ui.is_panel_visible() { self.chat_panel_width_px() } else { 0.0 };
            Rect {
                x: pad.left as f32,
                y: pad.top as f32 + tab_h,
                w: (w as f32 - pad.left as f32 - pad.right as f32 - panel_px).max(0.0),
                h: (h as f32 - pad.top as f32 - pad.bottom as f32 - tab_h - sb_h).max(0.0),
            }
        } else { Rect { x: pad.left as f32, y: pad.top as f32 + tab_h, w: 800.0, h: 600.0 } }
    }

    fn resize_terminals_for_panel(&mut self) {
        let viewport = self.viewport_rect();
        let (cell_w, cell_h) = self.cell_dims();
        self.mux.resize_all(viewport, self.config.scrollback_lines as usize, cell_w, cell_h);
        // Panel layout depends on term_cols/screen_rows — rebuild instances after resize.
        self.ui.panel_mut().dirty = true;
    }

    /// Close any terminals that exited. Returns true if the last tab closed (caller should exit).
    fn close_exited_terminals(&mut self, exited: Vec<usize>) -> bool {
        if exited.is_empty() { return false; }
        for tid in exited {
            if self.mux.close_terminal(tid) { return true; }
        }
        self.apply_tab_bar_padding();
        self.resize_terminals_for_panel();
        false
    }

    /// Given a pixel x coordinate, return which tab index is under the cursor in the tab bar.
    fn hit_test_tab_bar(&self, x_px: f64) -> Option<usize> {
        let (cell_w, _) = self.cell_dims();
        let pad_left = self.config.window.padding.left as f64;
        let click_col = ((x_px - pad_left) / cell_w as f64).floor() as usize;
        let mut col = 0usize;
        for (i, tab) in self.mux.tabs.tabs().iter().enumerate() {
            col += 1; // gap
            col += format!(" {} ", i + 1).chars().count(); // badge
            let raw = format!(" {} ", tab.title);
            col += raw.chars().take(14).count(); // title (capped at 14)
            if click_col < col { return Some(i); }
        }
        None
    }

    fn check_config_reload(&mut self) {
        if let Some(watcher) = &self.config_watcher {
            if watcher.poll().is_some() {
                if let Ok(new_cfg) = config::reload() {
                    self.config = new_cfg;
                    if let Some(rc) = &mut self.render_ctx { rc.renderer.update_bg_color(self.config.colors.background_wgpu()); }
                    self.ui.palette.rebuild_keybinds(&self.config);
                    // TD-020: also rewire LLM provider so provider/width_cols stay in sync.
                    self.ui.rewire_llm_provider(&self.config);
                    log::info!("Config hot-reloaded.");
                }
            }
        }
    }

    fn mouse_in_panel(&self) -> bool {
        if !self.ui.is_panel_visible() { return false; }
        let (cw, _) = self.cell_dims();
        let term_right_px = self.config.window.padding.left as f64
            + self.mux.active_terminal_size().0 as f64 * cw as f64;
        self.input.mouse_pos.0 >= term_right_px
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
        let (has_data, exited) = self.mux.poll_pty_events();
        if self.close_exited_terminals(exited) { event_loop.exit(); return; }

        // PTY data: mark pending but do NOT request_redraw immediately.
        // about_to_wait will fire the render after a short coalescing window (4ms),
        // ensuring multi-batch TUI updates (erase + redraw) are shown as one frame.
        if has_data {
            self.pending_pty_redraw = true;
            self.last_pty_activity = std::time::Instant::now();
        }

        // AI events are low-frequency; render immediately.
        let ai_needs_redraw = self.ui.poll_ai_events() || self.ui.poll_ai_block_events();
        self.flush_pending_pty_run();
        if ai_needs_redraw {
            if let Some(w) = &self.window { w.request_redraw(); }
        }
    }

    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() { return; }
        let mut attrs = WindowAttributes::default().with_title("PetruTerm");
        if self.config.window.title_bar_style == TitleBarStyle::None { attrs = attrs.with_decorations(false); }
        if let Some(w) = self.config.window.initial_width {
            if let Some(h) = self.config.window.initial_height { attrs = attrs.with_inner_size(winit::dpi::LogicalSize::new(w, h)); }
        } else { attrs = attrs.with_inner_size(winit::dpi::LogicalSize::new(1280u32, 800u32)); }

        let window = match event_loop.create_window(attrs) {
            Ok(w) => Arc::new(w),
            Err(e) => { log::error!("Failed to create window: {e}"); event_loop.exit(); return; }
        };
        #[cfg(target_os = "macos")]
        if self.config.window.title_bar_style == TitleBarStyle::Custom {
            unsafe { self.apply_macos_custom_titlebar(&window); }
        }

        if self.config.window.start_maximized { window.set_maximized(true); }

        let render_ctx = match pollster::block_on(RenderContext::new(window.clone(), &self.config)) {
            Ok(rc) => rc,
            Err(e) => { log::error!("Failed to initialize RenderContext: {e}"); event_loop.exit(); return; }
        };

        self.window = Some(window);
        self.render_ctx = Some(render_ctx);
        self.apply_tab_bar_padding(); // no-op here (0 tabs yet), but sets up for first tab
        if self.open_initial_tab().is_err() { event_loop.exit(); }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _window_id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => { event_loop.exit(); }
            WindowEvent::RedrawRequested => {
                self.check_config_reload();
                let (_, exited) = self.mux.poll_pty_events();
                if self.close_exited_terminals(exited) { event_loop.exit(); return; }
                self.ui.poll_ai_events();
                self.ui.poll_ai_block_events();
                self.flush_pending_pty_run();

                // Sync per-pane chat panel to the focused terminal.
                let terminal_id = self.mux.focused_terminal_id();
                self.ui.set_active_terminal(terminal_id);

                // Compute viewport and per-pane layout.
                let viewport = self.viewport_rect();
                let (cell_w, cell_h) = self.cell_dims();
                let pane_infos = self.mux.active_pane_infos(viewport, cell_w as f32, cell_h as f32);
                let pane_seps  = self.mux.active_pane_separators(viewport, cell_w as f32, cell_h as f32);

                // Viewport-wide dimensions for overlay positioning.
                let total_cols = (viewport.w / cell_w as f32).floor() as usize;
                let total_rows = (viewport.h / cell_h as f32).floor() as usize;
                // Focused pane dimensions (scroll bar, AI block anchor).
                let (term_cols, term_rows) = self.mux.active_terminal_size();

                if let Some(rc) = &mut self.render_ctx {
                    // Advance epoch once per frame so LRU eviction can age unused entries.
                    rc.renderer.atlas.next_epoch();

                    // Proactive eviction: when the atlas is 90% full, drop entries not
                    // touched in the last 60 frames (~1 second at 60fps).
                    if rc.renderer.atlas.is_near_full() {
                        let evicted = rc.renderer.atlas.evict_cold(60);
                        if evicted > 0 {
                            rc.clear_all_row_caches();
                            log::debug!("Atlas eviction: removed {} stale glyphs", evicted);
                        }
                    }

                    let scaled_font = rc.scaled_font_config(&self.config);

                    // ── Build cell instances for every pane ──────────────────────────────
                    let render_result = build_all_pane_instances(
                        rc, &pane_infos, &self.mux, &self.config, &scaled_font, self.input.cursor_blink_on,
                    );

                    if let Err(crate::renderer::atlas::AtlasError::Full) = render_result {
                        // Atlas full — clear everything and retry.
                        rc.renderer.atlas.clear(&rc.renderer.device());
                        if let Some(atlas) = rc.renderer.get_lcd_atlas() { atlas.borrow_mut().clear(&rc.renderer.device()); }
                        rc.clear_all_row_caches();
                        rc.atlas_generation += 1;
                        let _ = build_all_pane_instances(
                            rc, &pane_infos, &self.mux, &self.config, &scaled_font, self.input.cursor_blink_on,
                        );
                    }

                    // Pane separator lines.
                    rc.build_pane_separators(&pane_seps);

                    // ── Tab bar (2+ tabs, or while a rename prompt is active) ───────────
                    let renaming = self.ui.is_renaming_tab();
                    if self.mux.tabs.tab_count() > 1 || renaming {
                        let tab_total_cols = total_cols
                            + if self.ui.is_panel_visible() { self.ui.panel().width_cols as usize } else { 0 };
                        let rename_input = self.ui.tab_rename_input.as_deref();
                        rc.build_tab_bar_instances(
                            self.mux.tabs.tabs(),
                            self.mux.tabs.active_index(),
                            &scaled_font,
                            tab_total_cols,
                            self.config.window.padding.left as f32,
                            self.config.window.padding.top as f32,
                            self.config.colors.background,
                            rename_input,
                        );
                    }

                    // ── Scroll bar (overlays right edge of terminal) ─────────────────────
                    if self.config.enable_scroll_bar {
                        if let Some(terminal) = self.mux.active_terminal() {
                            let (disp_off, hist) = terminal.scrollback_info();
                            rc.build_scroll_bar_instances(disp_off, hist, term_rows, term_cols);
                        }
                    }

                    // ── Chat panel (side panel) ───────────────────────────────────────────
                    let panel_visible = self.ui.is_panel_visible();
                    if panel_visible {
                        let panel_dirty = self.ui.panel_mut().dirty;
                        if panel_dirty {
                            let panel_start = rc.instances.len();
                            let panel_focused = self.ui.panel_focused;
                            let blink = self.input.cursor_blink_on;
                            self.ui.panel_mut().dirty = false;
                            rc.build_chat_panel_instances(
                                self.ui.panel(),
                                panel_focused,
                                self.ui.file_picker_focused,
                                &self.config,
                                &scaled_font,
                                total_cols,
                                total_rows,
                                blink,
                            );
                            rc.panel_instances_cache = rc.instances[panel_start..].to_vec();
                        } else {
                            rc.instances.extend_from_slice(&rc.panel_instances_cache);
                        }
                    }

                    // ── Inline AI block (overlays bottom rows) ──────────────────────────
                    let block_visible = self.ui.is_block_visible();
                    if block_visible && self.ui.ai_block.dirty {
                        self.ui.ai_block.dirty = false;
                        rc.build_ai_block_instances(&self.ui.ai_block, &scaled_font, total_cols, total_rows);
                    }

                    // ── Command palette ──────────────────────────────────────────────────
                    if self.ui.palette.visible {
                        rc.mark_all_rows_dirty();
                        let palette_cols = total_cols
                            + if panel_visible { self.ui.panel().width_cols as usize } else { 0 };
                        rc.build_palette_instances(&self.ui.palette, &scaled_font, palette_cols, total_rows);
                    }

                    // ── Context menu (right-click) ────────────────────────────────────────
                    if self.ui.context_menu.visible {
                        rc.build_context_menu_instances(&self.ui.context_menu, &scaled_font, total_cols, total_rows);
                    }

                    // ── Status bar ───────────────────────────────────────────────────────
                    if self.config.status_bar.enabled {
                        let cwd = self.mux.active_cwd();
                        self.ui.poll_git_branch(cwd.as_deref());
                        let bar = crate::ui::status_bar::StatusBar::build(
                            self.input.leader_active,
                            cwd.as_deref(),
                            self.ui.git_branch_cache.as_deref(),
                            crate::llm::shell_context::ShellContext::load()
                                .and_then(|ctx| if ctx.last_exit_code != 0 { Some(ctx.last_exit_code) } else { None }),
                        );
                        rc.build_status_bar_instances(&bar, &scaled_font, total_cols + if panel_visible { self.ui.panel().width_cols as usize } else { 0 }, total_rows);
                    }

                    // ── GPU upload ──────────────────────────────────────────────────────
                    rc.renderer.upload_rect_instances(&rc.rect_instances);
                    rc.renderer.upload_instances(&rc.instances, 0);
                    rc.reset_row_dirty_flags();
                    rc.renderer.set_cell_count(rc.instances.len());
                    rc.renderer.upload_lcd_instances(&rc.lcd_instances);
                    let _ = rc.renderer.render();

                    // Suppress unused warning for scroll-bar focused dimensions.
                    let _ = (term_cols, term_rows);
                }
            }
            WindowEvent::Resized(size) => {
                if let Some(rc) = &mut self.render_ctx { rc.renderer.resize(size.width, size.height); }
                self.resize_terminals_for_panel();
                self.ui.ai_block.dirty = true;
                if let Some(w) = &self.window { w.request_redraw(); }
            }
            WindowEvent::ModifiersChanged(mods) => self.input.modifiers = mods,
            WindowEvent::KeyboardInput { event, is_synthetic, .. } => {
                if !is_synthetic {
                    let panel_was_visible = self.ui.is_panel_visible();
                    let tab_count_before = self.mux.tabs.tab_count();
                    let tab_idx_before = self.mux.active_tab_index();
                    let pane_count_before = self.mux.active_pane_count();
                    self.ui.set_active_terminal(self.mux.focused_terminal_id());
                    self.input.handle_key_input(
                        &event, event_loop, &self.config,
                        &mut self.mux, &mut self.ui,
                        &mut self.render_ctx, self.window.as_deref(),
                        self.wakeup_proxy.clone(),
                    );
                    if self.ui.is_panel_visible() != panel_was_visible {
                        self.resize_terminals_for_panel();
                    }
                    if self.mux.tabs.tab_count() != tab_count_before {
                        self.apply_tab_bar_padding();
                        self.resize_terminals_for_panel();
                    } else if self.mux.active_tab_index() != tab_idx_before {
                        // Tab switched — resize the newly active tab's panes.
                        self.resize_terminals_for_panel();
                    } else if self.mux.active_pane_count() != pane_count_before {
                        // Pane split or close — resize all panes in current tab.
                        self.resize_terminals_for_panel();
                    }
                    if let Some(w) = &self.window { w.request_redraw(); }
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.input.mouse_pos = (position.x, position.y);
                let (col, row) = self.input.pixel_to_cell(position.x, position.y, &self.config, &self.render_ctx, &self.mux);
                // Update context menu hover — redraw if hovered item changed.
                if self.ui.context_menu.update_hover(col, row) {
                    if let Some(w) = &self.window { w.request_redraw(); }
                }
                if self.input.mouse_left_pressed && !self.mouse_in_panel() {
                    if let Some(terminal) = self.mux.active_terminal() {
                        terminal.update_selection(col, row);
                        let (any_mouse, _, motion) = terminal.mouse_mode_flags();
                        if any_mouse && motion { self.input.send_mouse_report(32, col, row, true, &self.mux); }
                    }
                    if let Some(w) = &self.window { w.request_redraw(); }
                }
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let in_panel = self.mouse_in_panel();
                let (col, row) = self.input.pixel_to_cell(self.input.mouse_pos.0, self.input.mouse_pos.1, &self.config, &self.render_ctx, &self.mux);
                match (button, state) {
                    (MouseButton::Left, ElementState::Pressed) => {
                        // Context menu: consume click if it lands inside the menu.
                        if self.ui.context_menu.visible {
                            if let Some(action) = self.ui.context_menu.hit_test(col, row) {
                                self.ui.context_menu.close();
                                match action {
                                    ContextAction::Copy => {
                                        if let Some(terminal) = self.mux.active_terminal() {
                                            if let Some(text) = terminal.selection_text() {
                                                if let Ok(mut cb) = arboard::Clipboard::new() { let _ = cb.set_text(text); }
                                            }
                                        }
                                    }
                                    ContextAction::Paste => {
                                        if let Some(terminal) = self.mux.active_terminal() {
                                            if let Ok(text) = arboard::Clipboard::new().and_then(|mut cb| cb.get_text()) {
                                                if terminal.bracketed_paste_mode() {
                                                    let mut data = b"\x1b[200~".to_vec();
                                                    data.extend_from_slice(text.as_bytes());
                                                    data.extend_from_slice(b"\x1b[201~");
                                                    terminal.write_input(&data);
                                                } else { terminal.write_input(text.as_bytes()); }
                                            }
                                        }
                                    }
                                    ContextAction::Clear => {
                                        if let Some(terminal) = self.mux.active_terminal() {
                                            terminal.write_input(b"clear\n");
                                        }
                                    }
                                    ContextAction::SendToChat => {
                                        let selected = self.mux.active_terminal()
                                            .and_then(|t| t.selection_text());
                                        if let Some(text) = selected {
                                            let terminal_id = self.mux.focused_terminal_id();
                                            let cwd = self.mux.active_cwd()
                                                .or_else(|| std::env::current_dir().ok())
                                                .unwrap_or_default();
                                            self.ui.open_panel_with_context(terminal_id, cwd);
                                            self.ui.panel_mut().set_input(text);
                                            self.resize_terminals_for_panel();
                                        }
                                    }
                                    ContextAction::Separator => {}
                                }
                                if let Some(w) = &self.window { w.request_redraw(); }
                                return;
                            } else {
                                // Click outside menu closes it.
                                self.ui.context_menu.close();
                                if let Some(w) = &self.window { w.request_redraw(); }
                            }
                        }

                        if self.input.mouse_pos.1 < self.config.window.padding.top as f64 {
                            if let Some(w) = &self.window { let _ = w.drag_window(); }
                            return;
                        }
                        // Tab bar click — switch tab without passing event to terminal.
                        let tab_h = self.tab_bar_height_px() as f64;
                        if tab_h > 0.0 && self.input.mouse_pos.1 < self.config.window.padding.top as f64 + tab_h {
                            if let Some(idx) = self.hit_test_tab_bar(self.input.mouse_pos.0) {
                                self.mux.tabs.switch_to_index(idx);
                                self.resize_terminals_for_panel();
                            }
                            if let Some(w) = &self.window { w.request_redraw(); }
                            return;
                        }
                        if in_panel {
                            self.ui.panel_focused = true;
                        } else {
                            if self.ui.is_panel_visible() { self.ui.panel_focused = false; self.ui.file_picker_focused = false; }
                            // Multi-pane: focus the pane under the cursor.
                            {
                                let (px, py) = (self.input.mouse_pos.0 as f32, self.input.mouse_pos.1 as f32);
                                let tab_idx = self.mux.active_tab_index();
                                if let Some(pane_mgr) = self.mux.panes.get_mut(tab_idx) {
                                    pane_mgr.focus_at(px, py);
                                    let new_tid = self.mux.focused_terminal_id();
                                    self.ui.set_active_terminal(new_tid);
                                }
                            }
                            self.input.mouse_left_pressed = true;
                            if !self.mux.active_terminal().map(|t| t.mouse_mode_flags().0).unwrap_or(false) {
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
                        if !in_panel { self.input.send_mouse_report(0, col, row, false, &self.mux); }
                    }
                    (MouseButton::Right, ElementState::Pressed) => {
                        // In mouse-reporting mode, pass right-click to the terminal app.
                        // Otherwise, open the context menu.
                        if !in_panel {
                            let (any_mouse, _, _) = self.mux.active_terminal()
                                .map(|t| t.mouse_mode_flags()).unwrap_or((false, false, false));
                            if any_mouse {
                                self.input.send_mouse_report(2, col, row, true, &self.mux);
                            } else {
                                let (term_cols, term_rows) = self.mux.active_terminal_size();
                                self.ui.context_menu.open(col, row, term_cols, term_rows);
                            }
                        }
                    }
                    (MouseButton::Right, ElementState::Released) => {
                        let (any_mouse, _, _) = self.mux.active_terminal()
                            .map(|t| t.mouse_mode_flags()).unwrap_or((false, false, false));
                        if !in_panel && any_mouse {
                            self.input.send_mouse_report(2, col, row, false, &self.mux);
                        }
                    }
                    _ => {}
                }
                if let Some(w) = &self.window { w.request_redraw(); }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let scale = self.render_ctx.as_ref().map(|rc| rc.scale_factor as f64).unwrap_or(1.0);
                let delta_lines = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y as f64,
                    // pos.y is in logical points; divide by logical cell height to get lines.
                    MouseScrollDelta::PixelDelta(pos) => -pos.y / (self.cell_dims().1 as f64 / scale),
                };
                self.input.scroll_pixel_accum += delta_lines;
                let lines = self.input.scroll_pixel_accum.trunc() as i32;
                self.input.scroll_pixel_accum -= lines as f64;
                if lines == 0 { return; }
                if self.mouse_in_panel() {
                    if lines > 0 { self.ui.panel_mut().scroll_down(lines as usize); }
                    else         { self.ui.panel_mut().scroll_up((-lines) as usize); }
                    if let Some(w) = &self.window { w.request_redraw(); }
                    return;
                }
                let (col, row) = self.input.pixel_to_cell(self.input.mouse_pos.0, self.input.mouse_pos.1, &self.config, &self.render_ctx, &self.mux);
                let (any_mouse, _, _) = self.mux.active_terminal().map(|t| t.mouse_mode_flags()).unwrap_or((false, false, false));
                if any_mouse {
                    let btn = if lines > 0 { 65u8 } else { 64u8 };
                    for _ in 0..lines.abs() { self.input.send_mouse_report(btn, col, row, true, &self.mux); }
                } else if let Some(terminal) = self.mux.active_terminal() {
                    terminal.scroll_display(-lines);
                    if self.input.mouse_left_pressed { terminal.update_selection(col, row); }
                    if let Some(w) = &self.window { w.request_redraw(); }
                }
            }
            WindowEvent::DroppedFile(path) => {
                let path_str = path.to_string_lossy().into_owned();
                if self.ui.is_panel_visible() { self.ui.panel_mut().append_path(&path_str); }
                else if let Some(terminal) = self.mux.active_terminal() { terminal.write_input(path_str.as_bytes()); }
                if let Some(w) = &self.window { w.request_redraw(); }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        // Drain any PTY events that arrived since user_event last ran.
        // This catches batches that slipped in after user_event drained the channel,
        // and keeps last_pty_activity accurate for coalescing.
        let (more_data, exited) = self.mux.poll_pty_events();
        if self.close_exited_terminals(exited) { event_loop.exit(); return; }
        if more_data {
            self.pending_pty_redraw = true;
            self.last_pty_activity = std::time::Instant::now();
        }

        if self.ui.poll_ai_events()       { if let Some(w) = &self.window { w.request_redraw(); } }
        if self.ui.poll_ai_block_events() { if let Some(w) = &self.window { w.request_redraw(); } }
        self.flush_pending_pty_run();
        if self.input.update_cursor_blink() {
            // Panel input cursor blinks — mark dirty so cached instances are rebuilt.
            if self.ui.is_panel_visible() && self.ui.panel_focused {
                self.ui.panel_mut().dirty = true;
            }
            // AI block query cursor blinks when typing.
            if self.ui.ai_block.is_typing() {
                self.ui.ai_block.dirty = true;
            }
            if let Some(w) = &self.window { w.request_redraw(); }
        }

        // PTY render coalescing: fire the deferred redraw once the PTY has been
        // quiet for 4ms. This window is long enough to catch Gemini/TUI "erase +
        // redraw" sequences (usually < 2ms apart) but short enough to be imperceptible.
        const PTY_COALESCE_MS: u64 = 4;
        let pty_deadline = self.last_pty_activity + std::time::Duration::from_millis(PTY_COALESCE_MS);
        if self.pending_pty_redraw {
            let now = std::time::Instant::now();
            if now >= pty_deadline {
                self.pending_pty_redraw = false;
                if let Some(w) = &self.window { w.request_redraw(); }
            }
            // else: WaitUntil below will wake us at pty_deadline to retry.
        }

        let blink_deadline = self.input.cursor_last_blink + std::time::Duration::from_millis(530);
        let wake = if self.pending_pty_redraw {
            blink_deadline.min(pty_deadline)
        } else {
            blink_deadline
        };
        event_loop.set_control_flow(winit::event_loop::ControlFlow::WaitUntil(wake));
    }
}

/// Build and upload cell instances for every pane in the active tab.
/// Calls `rc.begin_frame()` first to clear previous frame's instances.
fn build_all_pane_instances(
    rc: &mut RenderContext,
    pane_infos: &[crate::ui::PaneInfo],
    mux: &Mux,
    config: &crate::config::Config,
    font: &crate::config::schema::FontConfig,
    cursor_blink_on: bool,
) -> Result<(), crate::renderer::atlas::AtlasError> {
    rc.begin_frame();
    for info in pane_infos {
        let cell_data = mux.collect_grid_cells_for(info.terminal_id);
        let cursor = if info.focused {
            mux.terminals
                .get(info.terminal_id)
                .and_then(|s| s.as_ref())
                .map(|t| t.cursor_info())
        } else {
            None
        };
        rc.build_instances(
            &cell_data,
            config,
            font,
            cursor.as_ref(),
            cursor_blink_on,
            info.terminal_id,
            info.col_offset,
            info.row_offset,
        )?;
    }
    Ok(())
}

impl Drop for App {
    fn drop(&mut self) {
        log::info!("App dropping; shutting down PTYs.");
        self.mux.shutdown();
    }
}
