use anyhow::Result;
use std::sync::Arc;
use winit::application::ApplicationHandler;
use winit::event::{ElementState, KeyEvent, Modifiers, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::ActiveEventLoop;
use winit::keyboard::{Key, NamedKey};
use winit::window::{Window, WindowAttributes, WindowId};

use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Column, Line};
use alacritty_terminal::selection::SelectionType;
use alacritty_terminal::term::TermMode;
use alacritty_terminal::vte::ansi::Color as AnsiColor;

use winit::event_loop::EventLoopProxy;

use crate::config::{self, Config};
use crate::config::schema::TitleBarStyle;
use crate::config::watcher::ConfigWatcher;
use crate::font::{build_font_system, TextShaper};
use crate::renderer::cell::{CellVertex, FLAG_CURSOR};
use crate::renderer::GpuRenderer;
use crate::term::color::resolve_color;
use crate::term::{CursorInfo, CursorShape, Terminal};
use crate::ui::{CommandPalette, PaneManager, Rect, SplitDir, TabManager};

/// Top-level application state.
///
/// Created before the event loop starts. The winit window and wgpu renderer
/// are initialized lazily in `resumed()` (required by winit 0.30's lifecycle).
pub struct App {
    config: Config,
    config_watcher: Option<ConfigWatcher>,

    // Initialized in `resumed()`
    window: Option<Arc<Window>>,
    renderer: Option<GpuRenderer>,
    shaper: Option<TextShaper>,

    // Terminal state
    tabs: TabManager,
    panes: Vec<PaneManager>,           // one PaneManager per tab
    terminals: Vec<Option<Terminal>>,  // indexed by terminal_id
    next_terminal_id: usize,

    // UI overlays
    palette: CommandPalette,

    // Input state
    modifiers: Modifiers,
    leader_active: bool,
    leader_timer: Option<std::time::Instant>,
    leader_timeout_ms: u64,

    // HiDPI scale factor (set in resumed() from window.scale_factor())
    scale_factor: f32,

    // Mouse state
    mouse_pos: (f64, f64),
    mouse_left_pressed: bool,
    scroll_pixel_accum: f64,

    // Cursor blink state
    cursor_blink_on: bool,
    cursor_last_blink: std::time::Instant,

    // winit event loop proxy — lets PTY threads wake the event loop immediately.
    wakeup_proxy: EventLoopProxy<()>,
}

impl App {
    pub fn new(config: Config, wakeup_proxy: EventLoopProxy<()>) -> Self {
        let config_watcher = config::config_dir()
            .exists()
            .then(|| ConfigWatcher::new(&config::config_dir()).ok())
            .flatten();

        let leader_timeout_ms = config.leader.timeout_ms;

        Self {
            config,
            config_watcher,
            window: None,
            renderer: None,
            shaper: None,
            tabs: TabManager::new(),
            panes: Vec::new(),
            terminals: Vec::new(),
            next_terminal_id: 0,
            palette: CommandPalette::new(),
            modifiers: Modifiers::default(),
            leader_active: false,
            leader_timer: None,
            leader_timeout_ms,
            scale_factor: 1.0,
            mouse_pos: (0.0, 0.0),
            mouse_left_pressed: false,
            scroll_pixel_accum: 0.0,
            cursor_blink_on: true,
            cursor_last_blink: std::time::Instant::now(),
            wakeup_proxy,
        }
    }

    /// Allocate a new terminal pane within the current tab.
    fn open_terminal(&mut self, viewport: Option<Rect>) -> Result<usize> {
        let (cols, rows) = self.default_grid_size();
        let (cell_w, cell_h) = self.cell_dims();
        let terminal = Terminal::new(&self.config, cols, rows, cell_w, cell_h, self.wakeup_proxy.clone())?;
        let id = self.next_terminal_id;
        self.next_terminal_id += 1;

        if self.terminals.len() <= id {
            self.terminals.resize_with(id + 1, || None);
        }
        self.terminals[id] = Some(terminal);
        Ok(id)
    }

    fn default_grid_size(&self) -> (u16, u16) {
        if let Some(renderer) = &self.renderer {
            let (w, h) = renderer.size();
            let (cell_w, cell_h) = self.cell_dims();
            let pad = &self.config.window.padding;
            let cols = ((w as f32 - pad.left as f32 - pad.right as f32) / cell_w as f32).max(1.0) as u16;
            let rows = ((h as f32 - pad.top as f32 - pad.bottom as f32) / cell_h as f32).max(1.0) as u16;
            (cols, rows)
        } else {
            (120, 40)
        }
    }

    /// Physical pixel dimensions of one terminal cell, sourced from the font shaper.
    fn cell_dims(&self) -> (u16, u16) {
        self.shaper.as_ref()
            .map(|s| (s.cell_width as u16, s.cell_height as u16))
            .unwrap_or((8, 16))
    }

    /// Open the first tab after the window has been created.
    fn open_initial_tab(&mut self) -> Result<()> {
        let viewport = self.viewport_rect();
        let tab_id = self.tabs.new_tab("zsh");
        let terminal_id = self.open_terminal(Some(viewport))?;
        let pane_mgr = PaneManager::new(viewport);
        self.panes.push(pane_mgr);
        log::info!("Opened initial tab {tab_id}, terminal {terminal_id}");
        Ok(())
    }

    /// Returns the font config with size scaled to physical pixels.
    fn scaled_font_config(&self) -> crate::config::schema::FontConfig {
        let mut cfg = self.config.font.clone();
        cfg.size *= self.scale_factor;
        cfg
    }

    fn viewport_rect(&self) -> Rect {
        let pad = &self.config.window.padding;
        if let Some(renderer) = &self.renderer {
            let (w, h) = renderer.size();
            Rect {
                x: pad.left as f32,
                y: pad.top as f32,
                w: (w as f32 - pad.left as f32 - pad.right as f32).max(0.0),
                h: (h as f32 - pad.top as f32 - pad.bottom as f32).max(0.0),
            }
        } else {
            Rect { x: pad.left as f32, y: pad.top as f32, w: 800.0, h: 600.0 }
        }
    }

    fn handle_palette_action(&mut self, action: crate::ui::palette::Action) {
        use crate::ui::palette::Action;
        match action {
            Action::ReloadConfig => {
                match config::reload() {
                    Ok(new_cfg) => {
                        self.config = new_cfg;
                        log::info!("Config reloaded.");
                    }
                    Err(e) => log::error!("Config reload failed: {e}"),
                }
            }
            Action::OpenConfigFile => {
                let path = config::config_path();
                let _ = std::process::Command::new("open").arg(path).spawn();
            }
            Action::NewTab => self.cmd_new_tab(),
            Action::CloseTab => self.cmd_close_tab(),
            Action::SplitHorizontal => self.cmd_split(SplitDir::Horizontal),
            Action::SplitVertical   => self.cmd_split(SplitDir::Vertical),
            Action::ClosePane       => self.cmd_close_pane(),
            Action::ToggleFullscreen => {
                if let Some(window) = &self.window {
                    let is_fs = window.fullscreen().is_some();
                    window.set_fullscreen(if is_fs {
                        None
                    } else {
                        Some(winit::window::Fullscreen::Borderless(None))
                    });
                }
            }
            Action::Quit => {
                // Will be handled by the event loop via close request.
                if let Some(window) = &self.window {
                    let _ = window.request_inner_size(winit::dpi::PhysicalSize::new(0u32, 0u32));
                }
            }
            // Phase 2 stubs — no-op for now.
            Action::ToggleAiMode
            | Action::EnableAiFeatures
            | Action::DisableAiFeatures
            | Action::ExplainLastOutput
            | Action::FixLastError => {
                log::info!("AI features available in Phase 2.");
            }
        }
    }

    fn cmd_new_tab(&mut self) {
        let tab_id = self.tabs.new_tab("zsh");
        let viewport = self.viewport_rect();
        match self.open_terminal(Some(viewport)) {
            Ok(_) => {
                self.panes.push(PaneManager::new(viewport));
                log::info!("Opened tab {tab_id}");
            }
            Err(e) => log::error!("Failed to open terminal: {e}"),
        }
    }

    fn cmd_close_tab(&mut self) {
        if let Some(tab) = self.tabs.active_tab() {
            let id = tab.id;
            self.tabs.close_tab(id);
        }
    }

    fn cmd_split(&mut self, dir: SplitDir) {
        let active = self.tabs.active_index();
        if let Some(pane_mgr) = self.panes.get_mut(active) {
            let new_id = pane_mgr.split(dir);
            let (split_cols, split_rows) = self.default_grid_size();
            let (cell_w, cell_h) = self.cell_dims();
            match Terminal::new(&self.config, split_cols, split_rows, cell_w, cell_h, self.wakeup_proxy.clone()) {
                Ok(terminal) => {
                    if self.terminals.len() <= new_id {
                        self.terminals.resize_with(new_id + 1, || None);
                    }
                    self.terminals[new_id] = Some(terminal);
                }
                Err(e) => log::error!("Failed to split pane: {e}"),
            }
        }
    }

    fn cmd_copy(&self) {
        let Some(terminal) = self.active_terminal() else { return };
        if let Some(text) = terminal.selection_text() {
            if let Ok(mut cb) = arboard::Clipboard::new() {
                let _ = cb.set_text(text);
            }
        }
    }

    fn cmd_paste(&self) {
        let Some(terminal) = self.active_terminal() else { return };
        let text = match arboard::Clipboard::new().and_then(|mut cb| cb.get_text()) {
            Ok(t) => t,
            Err(_) => return,
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

    fn cmd_close_pane(&mut self) {
        let active = self.tabs.active_index();
        if let Some(pane_mgr) = self.panes.get_mut(active) {
            if let Some(closed_id) = pane_mgr.close_focused() {
                if let Some(slot) = self.terminals.get_mut(closed_id) {
                    *slot = None;
                }
            }
        }
    }

    fn handle_key_input(&mut self, event: &KeyEvent, event_loop: &ActiveEventLoop) {
        if event.state != ElementState::Pressed {
            return;
        }

        // Reset blink phase so the cursor is always visible immediately after a keypress.
        self.cursor_blink_on = true;
        self.cursor_last_blink = std::time::Instant::now();

        // Check for leader key timeout.
        if self.leader_active {
            if let Some(t) = self.leader_timer {
                if t.elapsed().as_millis() > self.leader_timeout_ms as u128 {
                    self.leader_active = false;
                    self.leader_timer = None;
                }
            }
        }

        let cmd  = self.modifiers.state().super_key();
        let ctrl = self.modifiers.state().control_key();
        let shift = self.modifiers.state().shift_key();

        // Cmd+Shift+P — Command palette
        if cmd && shift {
            if let Key::Character(s) = &event.logical_key {
                if s.as_str().eq_ignore_ascii_case("p") {
                    self.palette.open();
                    return;
                }
            }
        }

        // Palette active — route all input to it.
        if self.palette.visible {
            match &event.logical_key {
                Key::Named(NamedKey::Escape) => self.palette.close(),
                Key::Named(NamedKey::Enter)  => {
                    if let Some(action) = self.palette.confirm() {
                        self.handle_palette_action(action);
                    }
                }
                Key::Named(NamedKey::ArrowUp)   => self.palette.select_up(),
                Key::Named(NamedKey::ArrowDown)  => self.palette.select_down(),
                Key::Named(NamedKey::Backspace)  => self.palette.backspace(),
                Key::Character(s) => {
                    for c in s.chars() {
                        self.palette.type_char(c);
                    }
                }
                _ => {}
            }
            return;
        }

        // Cmd+T — new tab
        if cmd && !shift && !ctrl {
            if let Key::Character(s) = &event.logical_key {
                match s.as_str() {
                    "t" => { self.cmd_new_tab(); return; }
                    "w" => { self.cmd_close_tab(); return; }
                    "q" => { event_loop.exit(); return; }
                    "c" => { self.cmd_copy(); return; }
                    "v" => { self.cmd_paste(); return; }
                    _ => {}
                }
            }
            // Cmd+1..9 — switch tab
            if let Key::Character(s) = &event.logical_key {
                if let Ok(n) = s.parse::<usize>() {
                    if n >= 1 && n <= 9 {
                        self.tabs.switch_to_index(n - 1);
                        return;
                    }
                }
            }
        }

        // Leader key detection (Ctrl+B by default).
        if ctrl {
            if let Key::Character(s) = &event.logical_key {
                if s.as_str() == self.config.leader.key.as_str() {
                    self.leader_active = true;
                    self.leader_timer = Some(std::time::Instant::now());
                    return;
                }
            }
        }

        // Leader key combos.
        if self.leader_active {
            self.leader_active = false;
            self.leader_timer = None;
            if let Key::Character(s) = &event.logical_key {
                match s.as_str() {
                    "%" => { self.cmd_split(SplitDir::Horizontal); return; }
                    "\"" => { self.cmd_split(SplitDir::Vertical); return; }
                    "x" => { self.cmd_close_pane(); return; }
                    _ => {}
                }
            }
            return; // Consume unknown leader combos.
        }

        // Forward all other input to the active terminal.
        self.send_key_to_active_terminal(&event);
    }

    fn send_key_to_active_terminal(&self, event: &KeyEvent) {
        // Check terminal mode flags that affect key encoding.
        let mode = self.active_terminal()
            .map(|t| *t.term.lock().mode())
            .unwrap_or(TermMode::empty());
        let app_cursor = mode.contains(TermMode::APP_CURSOR);

        // Convert the logical key to bytes and write to the active PTY.
        // Arrow keys send application sequences (\x1bO_) when APP_CURSOR is set,
        // otherwise normal ANSI sequences (\x1b[_). This is required for atuin,
        // nvim, tmux, and any readline/ZLE widget that activates DECCKM.
        let bytes: Option<Vec<u8>> = match &event.logical_key {
            Key::Character(s)              => Some(s.as_bytes().to_vec()),
            Key::Named(NamedKey::Enter)    => Some(b"\r".to_vec()),
            Key::Named(NamedKey::Backspace)=> Some(b"\x7f".to_vec()),
            Key::Named(NamedKey::Escape)   => Some(b"\x1b".to_vec()),
            Key::Named(NamedKey::Tab)      => Some(b"\t".to_vec()),
            Key::Named(NamedKey::Space)      => Some(b" ".to_vec()),
            Key::Named(NamedKey::ArrowUp)   => Some(if app_cursor { b"\x1bOA".to_vec() } else { b"\x1b[A".to_vec() }),
            Key::Named(NamedKey::ArrowDown) => Some(if app_cursor { b"\x1bOB".to_vec() } else { b"\x1b[B".to_vec() }),
            Key::Named(NamedKey::ArrowRight)=> Some(if app_cursor { b"\x1bOC".to_vec() } else { b"\x1b[C".to_vec() }),
            Key::Named(NamedKey::ArrowLeft) => Some(if app_cursor { b"\x1bOD".to_vec() } else { b"\x1b[D".to_vec() }),
            Key::Named(NamedKey::Home)     => Some(b"\x1b[H".to_vec()),
            Key::Named(NamedKey::End)      => Some(b"\x1b[F".to_vec()),
            Key::Named(NamedKey::Delete)   => Some(b"\x1b[3~".to_vec()),
            Key::Named(NamedKey::PageUp)   => Some(b"\x1b[5~".to_vec()),
            Key::Named(NamedKey::PageDown) => Some(b"\x1b[6~".to_vec()),
            _ => None,
        };

        if let Some(data) = bytes {
            let active_tab_idx = self.tabs.active_index();
            if let Some(pane_mgr) = self.panes.get(active_tab_idx) {
                let tid = pane_mgr.focused_terminal;
                if let Some(Some(terminal)) = self.terminals.get(tid) {
                    terminal.write_input(&data);
                }
            }
        }
    }

    /// Poll all terminal PTY event queues.
    /// Returns `(has_new_data, shell_exited)`.
    /// `shell_exited` is true when any PTY sends an Exit event — the caller
    /// should then call `event_loop.exit()`.
    fn poll_pty_events(&self) -> (bool, bool) {
        let mut has_data = false;
        let mut shell_exited = false;
        for terminal in self.terminals.iter().flatten() {
            loop {
                use crate::term::PtyEvent;
                use crossbeam_channel::TryRecvError;
                match terminal.pty.rx.try_recv() {
                    Ok(event) => match event {
                        PtyEvent::DataReady => { has_data = true; }
                        PtyEvent::TitleChanged(t) => {
                            log::debug!("PTY title: {t}");
                        }
                        PtyEvent::Exit => {
                            log::info!("PTY shell exited (Exit event).");
                            shell_exited = true;
                        }
                        PtyEvent::Bell => {}
                        PtyEvent::ClipboardStore(text) => {
                            if let Ok(mut cb) = arboard::Clipboard::new() {
                                let _ = cb.set_text(text);
                            }
                        }
                        PtyEvent::ClipboardLoad(fmt) => {
                            let text = arboard::Clipboard::new()
                                .ok()
                                .and_then(|mut cb| cb.get_text().ok())
                                .unwrap_or_default();
                            let response = fmt(&text);
                            terminal.write_input(response.as_bytes());
                        }
                        PtyEvent::PtyWrite(text) => {
                            terminal.write_input(text.as_bytes());
                        }
                    },
                    // Channel TX was dropped — the PTY event-loop thread exited
                    // without sending PtyEvent::Exit (common on macOS when the
                    // shell exits and alacritty_terminal drains the PTY before
                    // dispatching the event).  Treat as shell exit.
                    Err(TryRecvError::Disconnected) => {
                        log::info!("PTY shell exited (channel disconnected).");
                        shell_exited = true;
                        break;
                    }
                    Err(TryRecvError::Empty) => break,
                }
            }
        }
        (has_data, shell_exited)
    }

    /// Collect visible cell data from the active terminal grid.
    /// Returns one entry per visible row: (line_text, per-col (fg, bg) colors).
    /// Releases the term lock before returning so rendering can proceed.
    fn collect_grid_cells(&self) -> Vec<(String, Vec<(AnsiColor, AnsiColor)>)> {
        let active = self.tabs.active_index();
        let pane_mgr = match self.panes.get(active) {
            Some(p) => p,
            None => return vec![],
        };
        let terminal = match self.terminals.get(pane_mgr.focused_terminal).and_then(|t| t.as_ref()) {
            Some(t) => t,
            None => return vec![],
        };

        terminal.with_term(|term| {
            let rows = term.screen_lines();
            let cols = term.columns();
            // grid()[Line(row)] does NOT account for display_offset — it always
            // returns viewport-relative rows from the bottom of history. Subtract
            // display_offset so scrolled content is read from the correct position.
            let display_offset = term.grid().display_offset() as i32;
            let mut result = Vec::with_capacity(rows);

            for row in 0..rows {
                let mut text = String::with_capacity(cols);
                let mut colors: Vec<(AnsiColor, AnsiColor)> = Vec::with_capacity(cols);

                for col in 0..cols {
                    let cell = &term.grid()[Line(row as i32 - display_offset)][Column(col)];
                    let ch = if cell.c == '\0' { ' ' } else { cell.c };
                    text.push(ch);
                    colors.push((cell.fg, cell.bg));
                }
                result.push((text, colors));
            }
            result
        })
    }

    /// Convert a physical pixel position to a (col, row) terminal cell coordinate.
    fn pixel_to_cell(&self, x: f64, y: f64) -> (usize, usize) {
        let pad = &self.config.window.padding;
        let (cw, ch) = self.shaper.as_ref()
            .map(|s| (s.cell_width as f64, s.cell_height as f64))
            .unwrap_or((8.0, 16.0));
        let col = ((x - pad.left as f64) / cw).floor().max(0.0) as usize;
        let row = ((y - pad.top as f64) / ch).floor().max(0.0) as usize;
        let (term_cols, term_rows) = self.active_terminal_size();
        (col.min(term_cols.saturating_sub(1)), row.min(term_rows.saturating_sub(1)))
    }

    fn active_terminal_size(&self) -> (usize, usize) {
        let idx = self.tabs.active_index();
        if let Some(pane_mgr) = self.panes.get(idx) {
            if let Some(Some(t)) = self.terminals.get(pane_mgr.focused_terminal) {
                return (t.cols as usize, t.rows as usize);
            }
        }
        (80, 24)
    }

    fn active_terminal(&self) -> Option<&crate::term::Terminal> {
        let idx = self.tabs.active_index();
        let pane_mgr = self.panes.get(idx)?;
        self.terminals.get(pane_mgr.focused_terminal)?.as_ref()
    }

    /// Build and write an SGR or X10 mouse report to the active PTY.
    /// `button`: 0=left, 1=middle, 2=right, 64=wheel-up, 65=wheel-down.
    fn send_mouse_report(&self, button: u8, col: usize, row: usize, pressed: bool) {
        let Some(terminal) = self.active_terminal() else { return };
        let (any_mouse, sgr, _motion) = terminal.mouse_mode_flags();
        if !any_mouse { return; }

        let col1 = col + 1; // 1-indexed
        let row1 = row + 1;

        if sgr {
            let c = if pressed { 'M' } else { 'm' };
            let seq = format!("\x1b[<{button};{col1};{row1}{c}");
            terminal.write_input(seq.as_bytes());
        } else if pressed {
            // X10 encoding — only sent on press, coordinates clamped to 223.
            let b = button.saturating_add(32);
            let x = (col1 as u8).saturating_add(32).min(255);
            let y = (row1 as u8).saturating_add(32).min(255);
            terminal.write_input(&[0x1b, b'[', b'M', b, x, y]);
        }
    }

    fn check_config_reload(&mut self) {
        if let Some(watcher) = &self.config_watcher {
            if let Some(_changed) = watcher.poll() {
                match config::reload() {
                    Ok(new_cfg) => {
                        let new_bg = new_cfg.colors.background_wgpu();
                        self.config = new_cfg;
                        if let Some(renderer) = &mut self.renderer {
                            renderer.update_bg_color(new_bg);
                        }
                        log::info!("Config hot-reloaded.");
                    }
                    Err(e) => log::error!("Config reload failed: {e}"),
                }
            }
        }
    }
}

impl ApplicationHandler<()> for App {
    /// Called by the PTY background thread (via `EventLoopProxy::send_event`)
    /// whenever a PTY event is ready.  This wakes the NSApp run loop immediately
    /// so we don't wait up to 530 ms for the next `WaitUntil` blink timer.
    fn user_event(&mut self, event_loop: &ActiveEventLoop, _event: ()) {
        let (has_data, shell_exited) = self.poll_pty_events();
        if shell_exited {
            event_loop.exit();
            return;
        }
        if has_data {
            if let Some(window) = &self.window {
                window.request_redraw();
            }
        }
    }

    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return; // Already initialized.
        }

        let mut attrs = WindowAttributes::default().with_title("PetruTerm");

        // TitleBarStyle::None → remove all window chrome (no traffic lights).
        // TitleBarStyle::Custom → keep native frame; we'll patch it via objc2 below.
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

        // Apply macOS-specific title bar customization before renderer init.
        #[cfg(target_os = "macos")]
        if self.config.window.title_bar_style == TitleBarStyle::Custom {
            unsafe { apply_macos_custom_titlebar(&window); }
        }

        let renderer = match pollster::block_on(GpuRenderer::new(window.clone(), &self.config)) {
            Ok(r) => r,
            Err(e) => {
                log::error!("Failed to initialize GPU renderer: {e}");
                event_loop.exit();
                return;
            }
        };

        // Capture the HiDPI scale factor before moving the window.
        self.scale_factor = window.scale_factor() as f32;
        log::info!("Window scale factor: {}", self.scale_factor);

        self.window = Some(window);
        self.renderer = Some(renderer);

        // Initialize font shaper scaled to physical pixels (Retina-aware).
        // cosmic-text works in physical pixels; multiply by scale_factor so
        // a 15pt font renders at 30 physical pixels on a 2× Retina display.
        match build_font_system(&self.config.font) {
            Ok(fs) => {
                let scaled_font = self.scaled_font_config();
                let shaper = TextShaper::new(fs, &scaled_font);
                if let Some(r) = &mut self.renderer {
                    r.set_cell_size(shaper.cell_width, shaper.cell_height);
                }
                self.shaper = Some(shaper);
            }
            Err(e) => log::error!("Font system init failed: {e}"),
        }

        if let Err(e) = self.open_initial_tab() {
            log::error!("Failed to open initial terminal: {e}");
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
                log::info!("Window close requested.");
                event_loop.exit();
            }

            WindowEvent::RedrawRequested => {
                self.check_config_reload();
                let (_, shell_exited) = self.poll_pty_events();
                if shell_exited {
                    event_loop.exit();
                    return;
                }

                // Collect cells (releases term lock immediately).
                let cell_data = self.collect_grid_cells();
                let scaled_font = self.scaled_font_config();

                let cursor = self.active_terminal().map(|t| t.cursor_info());

                if let (Some(renderer), Some(shaper)) =
                    (&mut self.renderer, &mut self.shaper)
                {
                    let instances = build_instances(
                        &cell_data,
                        shaper,
                        renderer,
                        &self.config,
                        &scaled_font,
                        cursor.as_ref(),
                        self.cursor_blink_on,
                    );
                    renderer.upload_instances(&instances);
                    if let Err(e) = renderer.render() {
                        log::error!("Render error: {e}");
                    }
                }
            }

            WindowEvent::Resized(size) => {
                if let Some(renderer) = &mut self.renderer {
                    renderer.resize(size.width, size.height);
                }
                // Relayout all pane managers.
                let viewport = self.viewport_rect();
                for pane_mgr in &mut self.panes {
                    pane_mgr.resize(viewport);
                }
                // Propagate new grid dimensions to every terminal + PTY.
                let (cols, rows) = self.default_grid_size();
                let (cell_w, cell_h) = self.cell_dims();
                let scrollback = self.config.scrollback_lines as usize;
                for terminal in self.terminals.iter_mut().flatten() {
                    terminal.resize(cols, rows, scrollback, cell_w, cell_h);
                }
                log::debug!("Resized: {}×{} cells, {}×{}px each", cols, rows, cell_w, cell_h);
                if let Some(window) = &self.window {
                    window.request_redraw();
                }
            }

            WindowEvent::ModifiersChanged(mods) => {
                self.modifiers = mods;
            }

            WindowEvent::KeyboardInput { event, is_synthetic, .. } => {
                if !is_synthetic {
                    let event_clone = event.clone();
                    self.handle_key_input(&event_clone, event_loop);
                    if let Some(window) = &self.window {
                        window.request_redraw();
                    }
                }
            }

            WindowEvent::CursorMoved { position, .. } => {
                self.mouse_pos = (position.x, position.y);
                if self.mouse_left_pressed {
                    let (col, row) = self.pixel_to_cell(position.x, position.y);
                    if let Some(terminal) = self.active_terminal() {
                        terminal.update_selection(col, row);
                        let (any_mouse, _sgr, motion) = terminal.mouse_mode_flags();
                        if any_mouse && motion {
                            // Button 32 = left button held during motion (SGR drag)
                            self.send_mouse_report(32, col, row, true);
                        }
                    }
                    if let Some(window) = &self.window {
                        window.request_redraw();
                    }
                }
            }

            WindowEvent::MouseInput { state, button, .. } => {
                let (col, row) = self.pixel_to_cell(self.mouse_pos.0, self.mouse_pos.1);
                match (button, state) {
                    (MouseButton::Left, ElementState::Pressed) => {
                        self.mouse_left_pressed = true;
                        if let Some(terminal) = self.active_terminal() {
                            terminal.start_selection(col, row, SelectionType::Simple);
                        }
                        self.send_mouse_report(0, col, row, true);
                    }
                    (MouseButton::Left, ElementState::Released) => {
                        self.mouse_left_pressed = false;
                        self.send_mouse_report(0, col, row, false);
                    }
                    (MouseButton::Right, ElementState::Pressed) => {
                        self.send_mouse_report(2, col, row, true);
                    }
                    (MouseButton::Right, ElementState::Released) => {
                        self.send_mouse_report(2, col, row, false);
                    }
                    _ => {}
                }
                if let Some(window) = &self.window {
                    window.request_redraw();
                }
            }

            WindowEvent::MouseWheel { delta, .. } => {
                // Normalize both delta types to fractional lines and accumulate.
                // PixelDelta.y is in LOGICAL points (not physical px), so divide
                // by cell_height_logical = cell_height / scale_factor.
                let delta_lines = match delta {
                    MouseScrollDelta::LineDelta(_x, y) => y as f64,
                    MouseScrollDelta::PixelDelta(pos) => {
                        let ch_logical = self.shaper.as_ref()
                            .map(|s| s.cell_height as f64 / self.scale_factor as f64)
                            .unwrap_or(8.0);
                        pos.y / ch_logical
                    }
                };
                self.scroll_pixel_accum += delta_lines;
                let lines = self.scroll_pixel_accum.trunc() as i32;
                self.scroll_pixel_accum -= lines as f64;
                if lines == 0 { return; }

                let (col, row) = self.pixel_to_cell(self.mouse_pos.0, self.mouse_pos.1);
                let (any_mouse, _sgr, _motion) = self.active_terminal()
                    .map(|t| t.mouse_mode_flags())
                    .unwrap_or((false, false, false));

                if any_mouse {
                    // Forward as scroll wheel buttons (64=up, 65=down) for tmux/nvim.
                    let btn = if lines > 0 { 64u8 } else { 65u8 };
                    for _ in 0..lines.abs() {
                        self.send_mouse_report(btn, col, row, true);
                    }
                } else if let Some(terminal) = self.active_terminal() {
                    // Scroll the local scrollback buffer.
                    // Positive wheel delta = scroll up = show older history = Delta(-lines).
                    terminal.scroll_display(-lines);
                    if let Some(window) = &self.window {
                        window.request_redraw();
                    }
                }
            }

            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let (has_data, shell_exited) = self.poll_pty_events();
        if shell_exited {
            event_loop.exit();
            return;
        }
        if has_data {
            if let Some(window) = &self.window {
                window.request_redraw();
            }
        }

        // Toggle cursor blink every 530 ms.
        const BLINK_MS: u64 = 530;
        if self.cursor_last_blink.elapsed() >= std::time::Duration::from_millis(BLINK_MS) {
            self.cursor_blink_on = !self.cursor_blink_on;
            self.cursor_last_blink = std::time::Instant::now();
            if let Some(window) = &self.window {
                window.request_redraw();
            }
        }

        // Schedule next wakeup for the next blink toggle.
        let next_blink = self.cursor_last_blink
            + std::time::Duration::from_millis(BLINK_MS);
        event_loop.set_control_flow(winit::event_loop::ControlFlow::WaitUntil(next_blink));
    }
}

/// Apply macOS-specific title bar customization to an NSWindow.
///
/// Activates `NSWindowStyleMaskFullSizeContentView` so the content area
/// extends behind the title bar, makes the title bar transparent, hides
/// the title text (traffic lights stay in their native position), and
/// enables moving the window by dragging the background.
///
/// Uses `HasWindowHandle` → `AppKitWindowHandle.ns_view` → `[view window]`
/// because winit 0.30 removed the `ns_window()` platform extension.
#[cfg(target_os = "macos")]
unsafe fn apply_macos_custom_titlebar(window: &Window) {
    use objc2::msg_send;
    use objc2::runtime::{AnyObject, Bool};
    use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};

    let handle = match window.window_handle() {
        Ok(h) => h,
        Err(_) => return,
    };

    let ns_view_ptr = match handle.as_raw() {
        RawWindowHandle::AppKit(h) => h.ns_view.as_ptr(),
        _ => return,
    };

    let ns_view: &AnyObject = &*(ns_view_ptr as *const AnyObject);

    // Get the NSWindow that owns this view: [nsView window]
    let ns_win_ptr: *mut AnyObject = msg_send![ns_view, window];
    if ns_win_ptr.is_null() {
        return;
    }
    let ns_win: &AnyObject = &*ns_win_ptr;

    // Add NSWindowStyleMaskFullSizeContentView (1 << 15 = 32768).
    let current_mask: usize = msg_send![ns_win, styleMask];
    let () = msg_send![ns_win, setStyleMask: current_mask | (1_usize << 15)];

    // Transparent title bar so our GPU background shows through.
    let () = msg_send![ns_win, setTitlebarAppearsTransparent: Bool::YES];

    // NSWindowTitleHidden = 1 — hides the title text, traffic lights remain.
    let () = msg_send![ns_win, setTitleVisibility: 1_i64];

    // Allow dragging the window from the terminal content area.
    let () = msg_send![ns_win, setMovableByWindowBackground: Bool::YES];
}

/// Crop a glyph quad to fit within a cell rectangle `[0, cell_w] × [0, cell_h]`.
///
/// Returns `(glyph_offset, glyph_size, atlas_uv)` with both the screen rect and
/// the atlas UV range trimmed so only the visible portion of the bitmap is drawn.
/// Nerd Font PUA icons are often taller than `cell_height`; this prevents them
/// from bleeding into adjacent rows (TD-012).
fn clamp_glyph_to_cell(
    offset: [f32; 2],
    size: [f32; 2],
    uv: [f32; 4],
    cell_w: f32,
    cell_h: f32,
) -> ([f32; 2], [f32; 2], [f32; 4]) {
    let [ox, oy] = offset;
    let [gw, gh] = size;
    let [u0, v0, u1, v1] = uv;

    // Visible pixel extent within the cell.
    let x0 = ox.max(0.0);
    let y0 = oy.max(0.0);
    let x1 = (ox + gw).min(cell_w);
    let y1 = (oy + gh).min(cell_h);

    // Fully outside — emit a zero-size instance (will be invisible).
    if x1 <= x0 || y1 <= y0 || gw == 0.0 || gh == 0.0 {
        return ([0.0; 2], [0.0; 2], [0.0; 4]);
    }

    // Map pixel crop fractions back to atlas UV space.
    let fx0 = (x0 - ox) / gw;
    let fy0 = (y0 - oy) / gh;
    let fx1 = (x1 - ox) / gw;
    let fy1 = (y1 - oy) / gh;

    let cropped_uv = [
        u0 + fx0 * (u1 - u0),
        v0 + fy0 * (v1 - v0),
        u0 + fx1 * (u1 - u0),
        v0 + fy1 * (v1 - v0),
    ];

    ([x0, y0], [x1 - x0, y1 - y0], cropped_uv)
}

/// Build the GPU instance list from raw terminal cell data.
///
/// Shapes each row with cosmic-text, rasterizes glyphs into the atlas,
/// and emits one `CellVertex` per glyph. A cursor vertex is appended at
/// the end when the cursor is visible and blink is on.
fn build_instances(
    cell_data: &[(String, Vec<(AnsiColor, AnsiColor)>)],
    shaper: &mut TextShaper,
    renderer: &mut GpuRenderer,
    config: &Config,
    font: &crate::config::schema::FontConfig,
    cursor: Option<&CursorInfo>,
    cursor_blink_on: bool,
) -> Vec<CellVertex> {
    let mut instances = Vec::with_capacity(cell_data.len() * 80);

    for (row_idx, (text, raw_colors)) in cell_data.iter().enumerate() {
        // Resolve alacritty colors → linear RGBA
        let colors: Vec<([f32; 4], [f32; 4])> = raw_colors
            .iter()
            .map(|(fg, bg)| {
                (
                    resolve_color(*fg, &config.colors),
                    resolve_color(*bg, &config.colors),
                )
            })
            .collect();

        let shaped = shaper.shape_line(text, &colors, font);

        for glyph in &shaped.glyphs {
            let (atlas, queue) = renderer.atlas_and_queue();
            let entry = shaper.rasterize_to_atlas(glyph.cache_key, atlas, queue);

            let (atlas_uv, glyph_offset, glyph_size) = match entry {
                Some(e) => {
                    let off = [e.bearing_x as f32, shaped.ascent - e.bearing_y as f32];
                    let sz  = [e.width as f32, e.height as f32];
                    let (off, sz, uv) = clamp_glyph_to_cell(
                        off, sz, e.uv,
                        shaper.cell_width, shaper.cell_height,
                    );
                    (uv, off, sz)
                }
                None => ([0.0f32; 4], [0.0; 2], [0.0; 2]),
            };

            instances.push(CellVertex {
                grid_pos: [glyph.col as f32, row_idx as f32],
                atlas_uv,
                fg: glyph.fg,
                bg: glyph.bg,
                glyph_offset,
                glyph_size,
                flags: 0,
                _pad: 0,
            });
        }
    }

    // Cursor instance — appended to the bg pass so it draws on top of cell backgrounds.
    if let Some(info) = cursor {
        if info.visible && cursor_blink_on {
            let cw = shaper.cell_width;
            let ch = shaper.cell_height;

            let (glyph_offset, glyph_size) = match info.shape {
                CursorShape::Block | CursorShape::HollowBlock => {
                    ([0.0f32, 0.0], [cw, ch])
                }
                CursorShape::Underline => {
                    ([0.0, (ch - 2.0).max(0.0)], [cw, 2.0])
                }
                CursorShape::Beam => {
                    ([0.0, 0.0], [2.0, ch])
                }
                CursorShape::Hidden => ([0.0; 2], [0.0; 2]),
            };

            instances.push(CellVertex {
                grid_pos:     [info.col as f32, info.row as f32],
                atlas_uv:     [0.0; 4],
                fg:           config.colors.cursor_fg,
                bg:           config.colors.cursor_bg,
                glyph_offset,
                glyph_size,
                flags:        FLAG_CURSOR,
                _pad:         0,
            });
        }
    }

    instances
}
