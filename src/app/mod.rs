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
use crate::ui::Rect;

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
        }
    }

    fn default_grid_size(&self) -> (u16, u16) {
        if let Some(rc) = &self.render_ctx {
            let (w, h) = rc.renderer.size();
            let (cell_w, cell_h) = self.cell_dims();
            let pad = &self.config.window.padding;
            let panel_px = if self.ui.is_panel_visible() { self.chat_panel_width_px() } else { 0.0 };
            let cols = ((w as f32 - pad.left as f32 - pad.right as f32 - panel_px) / cell_w as f32).max(1.0) as u16;
            let rows = ((h as f32 - pad.top as f32 - pad.bottom as f32) / cell_h as f32).max(1.0) as u16;
            (cols, rows)
        } else { (120, 40) }
    }

    fn chat_panel_width_px(&self) -> f32 {
        let (cell_w, _) = self.cell_dims();
        self.ui.panel().width_cols as f32 * cell_w as f32
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
        if let Some(rc) = &self.render_ctx {
            let (w, h) = rc.renderer.size();
            let panel_px = if self.ui.is_panel_visible() { self.chat_panel_width_px() } else { 0.0 };
            Rect {
                x: pad.left as f32,
                y: pad.top as f32,
                w: (w as f32 - pad.left as f32 - pad.right as f32 - panel_px).max(0.0),
                h: (h as f32 - pad.top as f32 - pad.bottom as f32).max(0.0),
            }
        } else { Rect { x: pad.left as f32, y: pad.top as f32, w: 800.0, h: 600.0 } }
    }

    fn resize_terminals_for_panel(&mut self) {
        let viewport = self.viewport_rect();
        let (cols, rows) = self.default_grid_size();
        let (cell_w, cell_h) = self.cell_dims();
        self.mux.resize_all(viewport, cols, rows, self.config.scrollback_lines as usize, cell_w, cell_h);
        // Panel layout depends on term_cols/screen_rows — rebuild instances after resize.
        self.ui.panel_mut().dirty = true;
    }

    fn check_config_reload(&mut self) {
        if let Some(watcher) = &self.config_watcher {
            if watcher.poll().is_some() {
                if let Ok(new_cfg) = config::reload() {
                    self.config = new_cfg;
                    if let Some(rc) = &mut self.render_ctx { rc.renderer.update_bg_color(self.config.colors.background_wgpu()); }
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
        let (has_data, shell_exited) = self.mux.poll_pty_events();
        if shell_exited { event_loop.exit(); return; }
        let needs_redraw = has_data
            || self.ui.poll_ai_events()
            || self.ui.poll_ai_block_events();
        if needs_redraw { if let Some(w) = &self.window { w.request_redraw(); } }
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
        if self.open_initial_tab().is_err() { event_loop.exit(); }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _window_id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => { event_loop.exit(); }
            WindowEvent::RedrawRequested => {
                self.check_config_reload();
                let (_, shell_exited) = self.mux.poll_pty_events();
                if shell_exited { event_loop.exit(); return; }
                self.ui.poll_ai_events();
                self.ui.poll_ai_block_events();

                // Sync per-pane chat panel to the focused terminal.
                let terminal_id = self.mux.focused_terminal_id();
                self.ui.set_active_terminal(terminal_id);

                let cell_data = self.mux.collect_grid_cells();
                let (term_cols, term_rows) = self.mux.active_terminal_size();
                let cursor = self.mux.active_terminal().map(|t| t.cursor_info());

                if let Some(rc) = &mut self.render_ctx {
                    let scaled_font = rc.scaled_font_config(&self.config);
                    let result = rc.build_instances(&cell_data, &self.config, &scaled_font, cursor.as_ref(), self.input.cursor_blink_on, terminal_id);

                    if let Err(crate::renderer::atlas::AtlasError::Full) = result {
                        rc.renderer.atlas.clear(&rc.renderer.device());
                        if let Some(atlas) = rc.renderer.get_lcd_atlas() { atlas.borrow_mut().clear(&rc.renderer.device()); }
                        rc.row_cache.clear();
                        rc.atlas_generation += 1;
                        let _ = rc.build_instances(&cell_data, &self.config, &scaled_font, cursor.as_ref(), self.input.cursor_blink_on, terminal_id);
                    }

                    // ── Chat panel (side panel) ───────────────────────────────────────────
                    let panel_visible = self.ui.is_panel_visible();
                    if panel_visible {
                        let panel_dirty = self.ui.panel_mut().dirty;
                        if panel_dirty {
                            let panel_start = rc.instances.len();
                            let panel_focused = self.ui.panel_focused;
                            let blink = self.input.cursor_blink_on;
                            // Temporarily borrow the panel immutably for rendering
                            // (panel_mut().dirty was already read above, so the mut borrow is released)
                            {
                                // We need &ChatPanel for build_chat_panel_instances.
                                // Use a raw pointer dance to satisfy the borrow checker: panel_mut()
                                // for the dirty clear + panel() for the render call are separate borrows.
                                self.ui.panel_mut().dirty = false;
                            }
                            rc.build_chat_panel_instances(
                                self.ui.panel(),
                                panel_focused,
                                &self.config,
                                &scaled_font,
                                term_cols,
                                term_rows,
                                blink,
                            );
                            rc.panel_instances_cache = rc.instances[panel_start..].to_vec();
                        } else {
                            rc.instances.extend_from_slice(&rc.panel_instances_cache);
                        }
                    }

                    // ── Inline AI block (overlays bottom rows) ──────────────────────────
                    let block_visible = self.ui.is_block_visible();
                    if block_visible {
                        if self.ui.ai_block.dirty {
                            self.ui.ai_block.dirty = false;
                            rc.build_ai_block_instances(&self.ui.ai_block, &scaled_font, term_cols, term_rows);
                        }
                    }

                    // ── Command palette ──────────────────────────────────────────────────
                    if self.ui.palette.visible {
                        rc.row_cache.dirty_rows.fill(true);
                        let total_cols = term_cols
                            + if panel_visible { self.ui.panel().width_cols as usize } else { 0 };
                        rc.build_palette_instances(&self.ui.palette, &scaled_font, total_cols, term_rows);
                    }

                    // ── GPU upload ───────────────────────────────────────────────────────
                    // When any overlay is visible its instances interleave with terminal rows,
                    // breaking the fixed-stride dirty-row upload. Do a full upload instead.
                    if self.ui.palette.visible || panel_visible || block_visible {
                        rc.renderer.upload_instances(&rc.instances, 0);
                        rc.row_cache.dirty_rows.fill(false);
                    } else {
                        for (row_idx, is_dirty) in rc.row_cache.dirty_rows.iter_mut().enumerate() {
                            if *is_dirty {
                                let start = row_idx * term_cols;
                                let end = (start + term_cols).min(rc.instances.len());
                                if start < rc.instances.len() {
                                    rc.renderer.upload_instances(&rc.instances[start..end], start);
                                }
                                *is_dirty = false;
                            }
                        }
                        // Always upload the cursor instance to prevent flickering.
                        if !rc.instances.is_empty() {
                            let cursor_idx = rc.instances.len() - 1;
                            rc.renderer.upload_instances(&rc.instances[cursor_idx..], cursor_idx);
                        }
                    }
                    rc.renderer.set_cell_count(rc.instances.len());
                    rc.renderer.upload_lcd_instances(&rc.lcd_instances);
                    let _ = rc.renderer.render();
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
                    if let Some(w) = &self.window { w.request_redraw(); }
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.input.mouse_pos = (position.x, position.y);
                if self.input.mouse_left_pressed && !self.mouse_in_panel() {
                    let (col, row) = self.input.pixel_to_cell(position.x, position.y, &self.config, &self.render_ctx, &self.mux);
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
                        if self.input.mouse_pos.1 < self.config.window.padding.top as f64 {
                            if let Some(w) = &self.window { let _ = w.drag_window(); }
                            return;
                        }
                        if in_panel {
                            self.ui.panel_focused = true;
                        } else {
                            if self.ui.is_panel_visible() { self.ui.panel_focused = false; }
                            self.input.mouse_left_pressed = true;
                            if !self.mux.active_terminal().map(|t| t.mouse_mode_flags().0).unwrap_or(false) {
                                if let Some(terminal) = self.mux.active_terminal() {
                                    terminal.start_selection(col, row, SelectionType::Simple);
                                }
                            }
                            self.input.send_mouse_report(0, col, row, true, &self.mux);
                        }
                    }
                    (MouseButton::Left, ElementState::Released) => {
                        self.input.mouse_left_pressed = false;
                        if !in_panel { self.input.send_mouse_report(0, col, row, false, &self.mux); }
                    }
                    (MouseButton::Right, ElementState::Pressed)  => if !in_panel { self.input.send_mouse_report(2, col, row, true,  &self.mux) },
                    (MouseButton::Right, ElementState::Released)  => if !in_panel { self.input.send_mouse_report(2, col, row, false, &self.mux) },
                    _ => {}
                }
                if let Some(w) = &self.window { w.request_redraw(); }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let delta_lines = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y as f64,
                    MouseScrollDelta::PixelDelta(pos) => -pos.y / self.cell_dims().1 as f64,
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
        let (_, shell_exited) = self.mux.poll_pty_events();
        if shell_exited { event_loop.exit(); return; }
        if self.ui.poll_ai_events()       { if let Some(w) = &self.window { w.request_redraw(); } }
        if self.ui.poll_ai_block_events() { if let Some(w) = &self.window { w.request_redraw(); } }
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
        event_loop.set_control_flow(winit::event_loop::ControlFlow::WaitUntil(
            self.input.cursor_last_blink + std::time::Duration::from_millis(530),
        ));
    }
}

impl Drop for App {
    fn drop(&mut self) {
        log::info!("App dropping; shutting down PTYs.");
        self.mux.shutdown();
    }
}
