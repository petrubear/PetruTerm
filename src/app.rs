use anyhow::Result;
use std::sync::Arc;
use winit::application::ApplicationHandler;
use winit::event::{ElementState, KeyEvent, Modifiers, WindowEvent};
use winit::event_loop::ActiveEventLoop;
use winit::keyboard::{Key, NamedKey};
use winit::window::{Window, WindowAttributes, WindowId};

use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Column, Line};
use alacritty_terminal::vte::ansi::Color as AnsiColor;

use crate::config::{self, Config};
use crate::config::watcher::ConfigWatcher;
use crate::font::{build_font_system, TextShaper};
use crate::renderer::cell::CellVertex;
use crate::renderer::GpuRenderer;
use crate::term::color::resolve_color;
use crate::term::Terminal;
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
}

impl App {
    pub fn new(config: Config) -> Self {
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
        }
    }

    /// Allocate a new terminal pane within the current tab.
    fn open_terminal(&mut self, viewport: Option<Rect>) -> Result<usize> {
        let (cols, rows) = self.default_grid_size();
        let terminal = Terminal::new(&self.config, cols, rows)?;
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
            let cell_w = 8.0f32;   // placeholder until font metrics are ready
            let cell_h = 16.0f32;
            let pad = &self.config.window.padding;
            let cols = ((w as f32 - pad.left as f32 - pad.right as f32) / cell_w).max(1.0) as u16;
            let rows = ((h as f32 - pad.top as f32 - pad.bottom as f32) / cell_h).max(1.0) as u16;
            (cols, rows)
        } else {
            (120, 40)
        }
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
            let viewport = self.viewport_rect();
            match Terminal::new(&self.config, 80, 24) {
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
        // Convert the logical key to bytes and write to the active PTY.
        let bytes: Option<Vec<u8>> = match &event.logical_key {
            Key::Character(s)              => Some(s.as_bytes().to_vec()),
            Key::Named(NamedKey::Enter)    => Some(b"\r".to_vec()),
            Key::Named(NamedKey::Backspace)=> Some(b"\x7f".to_vec()),
            Key::Named(NamedKey::Escape)   => Some(b"\x1b".to_vec()),
            Key::Named(NamedKey::Tab)      => Some(b"\t".to_vec()),
            Key::Named(NamedKey::ArrowUp)  => Some(b"\x1b[A".to_vec()),
            Key::Named(NamedKey::ArrowDown)=> Some(b"\x1b[B".to_vec()),
            Key::Named(NamedKey::ArrowRight)=>Some(b"\x1b[C".to_vec()),
            Key::Named(NamedKey::ArrowLeft)=> Some(b"\x1b[D".to_vec()),
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

    fn poll_pty_events(&self) -> bool {
        let mut has_data = false;
        for terminal in self.terminals.iter().flatten() {
            while let Ok(event) = terminal.pty.rx.try_recv() {
                use crate::term::PtyEvent;
                match event {
                    PtyEvent::DataReady => { has_data = true; }
                    PtyEvent::TitleChanged(t) => {
                        log::debug!("PTY title: {t}");
                    }
                    PtyEvent::Exit => {
                        log::info!("PTY shell exited.");
                    }
                    PtyEvent::Bell => {}
                }
            }
        }
        has_data
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
            let mut result = Vec::with_capacity(rows);

            for row in 0..rows {
                let mut text = String::with_capacity(cols);
                let mut colors: Vec<(AnsiColor, AnsiColor)> = Vec::with_capacity(cols);

                for col in 0..cols {
                    let cell = &term.grid()[Line(row as i32)][Column(col)];
                    let ch = if cell.c == '\0' { ' ' } else { cell.c };
                    text.push(ch);
                    colors.push((cell.fg, cell.bg));
                }
                result.push((text, colors));
            }
            result
        })
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

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return; // Already initialized.
        }

        let mut attrs = WindowAttributes::default().with_title("PetruTerm");

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
                let _ = self.poll_pty_events();

                // Collect cells (releases term lock immediately).
                let cell_data = self.collect_grid_cells();
                let scaled_font = self.scaled_font_config();

                if let (Some(renderer), Some(shaper)) =
                    (&mut self.renderer, &mut self.shaper)
                {
                    let instances = build_instances(
                        &cell_data,
                        shaper,
                        renderer,
                        &self.config,
                        &scaled_font,
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

            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        let has_data = self.poll_pty_events();
        if has_data {
            if let Some(window) = &self.window {
                window.request_redraw();
            }
        }
    }
}

/// Build the GPU instance list from raw terminal cell data.
///
/// Shapes each row with cosmic-text, rasterizes glyphs into the atlas,
/// and emits one `CellVertex` per glyph. Cells with no visible glyph
/// (space, control chars) still get a background vertex so bg colors render.
fn build_instances(
    cell_data: &[(String, Vec<(AnsiColor, AnsiColor)>)],
    shaper: &mut TextShaper,
    renderer: &mut GpuRenderer,
    config: &Config,
    font: &crate::config::schema::FontConfig,
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
                Some(e) => (
                    e.uv,
                    [e.bearing_x as f32, shaped.ascent - e.bearing_y as f32],
                    [e.width as f32, e.height as f32],
                ),
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

    instances
}
