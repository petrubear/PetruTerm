use anyhow::Result;
use std::sync::Arc;
use winit::application::ApplicationHandler;
use winit::event::{ElementState, KeyEvent, Modifiers, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::ActiveEventLoop;
use winit::keyboard::{Key, NamedKey};
use winit::window::{Window, WindowAttributes, WindowId};

use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Column, Line, Point};
use alacritty_terminal::selection::{SelectionRange, SelectionType};
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::term::TermMode;
use alacritty_terminal::vte::ansi::Color as AnsiColor;

use winit::event_loop::EventLoopProxy;

use crate::config::{self, Config};
use crate::config::schema::TitleBarStyle;
use crate::config::watcher::ConfigWatcher;
use crate::font::{build_font_system, ShapedGlyph, TextShaper};
use crate::llm::chat_panel::{AiEvent, ChatPanel, PanelState};
use crate::llm::shell_context::ShellContext;
use crate::renderer::cell::{CellVertex, FLAG_CURSOR, FLAG_LCD};
use crate::renderer::GpuRenderer;
use crate::term::color::resolve_color;
use crate::term::{CursorInfo, CursorShape, Terminal};
use crate::ui::{CommandPalette, PaneManager, Rect, SplitDir, TabManager};

/// Cache for a single shaped row to avoid re-shaping every frame.
#[derive(Clone)]
struct RowCacheEntry {
    /// Hash of the row's text and colors.
    hash: u64,
    /// The resulting shaped glyphs for this row.
    glyphs: Vec<ShapedGlyph>,
    /// The physical pixel instances for this row (cached to avoid re-building CellVertex).
    instances: Vec<CellVertex>,
    /// The LCD instances for this row.
    lcd_instances: Vec<CellVertex>,
}

/// Tracks shaped data for every visible row in the active terminal viewport.
struct RowCache {
    /// Indexed by (viewport_row).
    rows: Vec<Option<RowCacheEntry>>,
    /// Tracks which rows changed since the last upload to GPU.
    dirty_rows: Vec<bool>,
    /// Tracks which terminal ID this cache is valid for.
    terminal_id: Option<usize>,
    /// Tracks font config hash to invalidate on font change.
    font_hash: u64,
}

impl RowCache {
    fn new() -> Self {
        Self { rows: Vec::new(), dirty_rows: Vec::new(), terminal_id: None, font_hash: 0 }
    }

    fn clear(&mut self) {
        for r in &mut self.rows { *r = None; }
        for d in &mut self.dirty_rows { *d = true; }
    }
}

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

    // Performance: Shaping & Instance Cache
    row_cache: RowCache,
    /// Generation counter for the glyph atlas — incremented when atlas is cleared.
    atlas_generation: usize,

    // UI overlays
    palette: CommandPalette,

    // AI layer
    chat_panel: ChatPanel,
    panel_focused: bool,
    llm_provider: Option<Arc<dyn crate::llm::LlmProvider>>,
    tokio_rt: tokio::runtime::Runtime,
    ai_tx: crossbeam_channel::Sender<AiEvent>,
    ai_rx: crossbeam_channel::Receiver<AiEvent>,

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

    // Reusable per-frame GPU instance buffer. Capacity grows to the high-water
    // mark and is never freed between frames, eliminating the ~200 KB/frame
    // Vec<CellVertex> allocation that would otherwise occur at 60 fps.
    instances: Vec<CellVertex>,
    // Separate instance buffer for LCD glyphs (drawn with LCD pipeline).
    lcd_instances: Vec<CellVertex>,
}

impl App {
    pub fn new(config: Config, wakeup_proxy: EventLoopProxy<()>) -> Self {
        let config_watcher = config::config_dir()
            .exists()
            .then(|| ConfigWatcher::new(&config::config_dir()).ok())
            .flatten();

        let leader_timeout_ms = config.leader.timeout_ms;

        let (ai_tx, ai_rx) = crossbeam_channel::unbounded::<AiEvent>();
        let tokio_rt = tokio::runtime::Runtime::new().expect("tokio runtime");

        let llm_provider: Option<Arc<dyn crate::llm::LlmProvider>> = if config.llm.enabled {
            match crate::llm::build_provider(&config.llm) {
                Ok(p) => {
                    log::info!("LLM provider '{}' ready (model: {}).", config.llm.provider, config.llm.model);
                    Some(p)
                }
                Err(e) => {
                    log::warn!("LLM init failed: {e}");
                    None
                }
            }
        } else {
            None
        };

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
            row_cache: RowCache::new(),
            atlas_generation: 0,
            palette: CommandPalette::new(),
            chat_panel: ChatPanel::new(),
            panel_focused: false,
            llm_provider,
            tokio_rt,
            ai_tx,
            ai_rx,
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
            instances: Vec::new(),
            lcd_instances: Vec::new(),
        }
    }

    /// Allocate a new terminal pane within the current tab.
    fn open_terminal(&mut self, _viewport: Option<Rect>) -> Result<usize> {
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
            let panel_px = if self.chat_panel.is_visible() {
                self.chat_panel.width_cols as f32 * cell_w as f32
            } else {
                0.0
            };
            let cols = ((w as f32 - pad.left as f32 - pad.right as f32 - panel_px) / cell_w as f32).max(1.0) as u16;
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
        crate::font::loader::locate_font_for_lcd(&mut cfg);
        cfg
    }

    fn viewport_rect(&self) -> Rect {
        let pad = &self.config.window.padding;
        if let Some(renderer) = &self.renderer {
            let (w, h) = renderer.size();
            let (cell_w, _) = self.cell_dims();
            let panel_px = if self.chat_panel.is_visible() {
                self.chat_panel.width_cols as f32 * cell_w as f32
            } else {
                0.0
            };
            Rect {
                x: pad.left as f32,
                y: pad.top as f32,
                w: (w as f32 - pad.left as f32 - pad.right as f32 - panel_px).max(0.0),
                h: (h as f32 - pad.top as f32 - pad.bottom as f32).max(0.0),
            }
        } else {
            Rect { x: pad.left as f32, y: pad.top as f32, w: 800.0, h: 600.0 }
        }
    }

    // ── Chat panel ───────────────────────────────────────────────────────────

    /// Resize all terminals to account for the current panel state.
    ///
    /// Must be called whenever the panel opens or closes.
    fn resize_terminals_for_panel(&mut self) {
        let viewport = self.viewport_rect();
        for pane_mgr in &mut self.panes {
            pane_mgr.resize(viewport);
        }
        let (cols, rows) = self.default_grid_size();
        let (cell_w, cell_h) = self.cell_dims();
        let scrollback = self.config.scrollback_lines as usize;
        for terminal in self.terminals.iter_mut().flatten() {
            terminal.resize(cols, rows, scrollback, cell_w, cell_h);
        }
    }

    /// Submit the current panel input to the LLM provider via a tokio task.
    fn submit_ai_query(&mut self) {
        let Some(_user_content) = self.chat_panel.submit_input() else { return };

        let Some(provider) = self.llm_provider.clone() else {
            self.chat_panel.mark_error(
                "LLM not configured — set llm.enabled = true in llm.lua".into(),
            );
            return;
        };

        // Build multi-turn message history including the new user message
        // (already pushed into chat_panel.messages by submit_input).
        let mut system_text = String::from(
            "You are a helpful terminal assistant. When asked for a shell command, \
             respond with ONLY the command — no explanation, no markdown fences. \
             For general questions, respond concisely.",
        );
        if let Some(ctx) = ShellContext::load() {
            let ctx_str = ctx.format_for_system_message();
            if !ctx_str.is_empty() {
                system_text.push_str("\n\nShell context:\n");
                system_text.push_str(&ctx_str);
            }
        }
        let mut messages = vec![crate::llm::ChatMessage::system(system_text)];
        messages.extend(self.chat_panel.messages.iter().cloned());

        let tx     = self.ai_tx.clone();
        let wakeup = self.wakeup_proxy.clone();

        self.tokio_rt.spawn(async move {
            use futures_util::StreamExt;
            match provider.stream(messages).await {
                Err(e) => {
                    let _ = tx.send(AiEvent::Error(e.to_string()));
                    let _ = wakeup.send_event(());
                }
                Ok(mut stream) => {
                    while let Some(result) = stream.next().await {
                        match result {
                            Ok(tok) => { let _ = tx.send(AiEvent::Token(tok)); }
                            Err(e)  => {
                                let _ = tx.send(AiEvent::Error(e.to_string()));
                                let _ = wakeup.send_event(());
                                return;
                            }
                        }
                        let _ = wakeup.send_event(());
                    }
                    let _ = tx.send(AiEvent::Done);
                    let _ = wakeup.send_event(());
                }
            }
        });
    }

    /// Return the last `n` lines of the active terminal viewport as plain text.
    /// Reads the bottom of the current screen (no scrollback history).
    fn last_terminal_lines(&self, n: usize) -> String {
        let active = self.tabs.active_index();
        let pane_mgr = match self.panes.get(active) {
            Some(p) => p,
            None => return String::new(),
        };
        let terminal = match self.terminals.get(pane_mgr.focused_terminal).and_then(|t| t.as_ref()) {
            Some(t) => t,
            None => return String::new(),
        };

        terminal.with_term(|term| {
            let rows = term.screen_lines();
            let cols = term.columns();
            let start = rows.saturating_sub(n);
            let mut lines = Vec::with_capacity(rows - start);
            for row in start..rows {
                let mut text = String::with_capacity(cols);
                for col in 0..cols {
                    let cell = &term.grid()[alacritty_terminal::index::Line(row as i32)][alacritty_terminal::index::Column(col)];
                    text.push(if cell.c == '\0' { ' ' } else { cell.c });
                }
                let trimmed = text.trim_end().to_string();
                if !trimmed.is_empty() {
                    lines.push(trimmed);
                }
            }
            lines.join("\n")
        })
    }

    /// Open the panel and submit "Explain this terminal output" with the last 30
    /// lines of the current viewport as context.
    fn explain_last_output(&mut self) {
        let output = self.last_terminal_lines(30);
        if output.is_empty() {
            return;
        }
        if !self.chat_panel.is_visible() {
            self.chat_panel.open();
            self.resize_terminals_for_panel();
        }
        self.panel_focused = true;
        self.chat_panel.input = format!("Explain this terminal output:\n```\n{}\n```", output);
        self.submit_ai_query();
    }

    /// Open the panel and submit a "fix this error" query using the last 30 lines
    /// of the current viewport plus shell context (exit code, last command).
    fn fix_last_error(&mut self) {
        let output = self.last_terminal_lines(30);
        let ctx = ShellContext::load();

        let query = match &ctx {
            Some(c) if !c.last_command.is_empty() => format!(
                "The command `{}` failed (exit code {}). Here's the output:\n```\n{}\n```\nHow do I fix this?",
                c.last_command, c.last_exit_code, output
            ),
            _ => format!(
                "This command failed. Here's the output:\n```\n{}\n```\nHow do I fix this?",
                output
            ),
        };

        if !self.chat_panel.is_visible() {
            self.chat_panel.open();
            self.resize_terminals_for_panel();
        }
        self.panel_focused = true;
        self.chat_panel.input = query;
        self.submit_ai_query();
    }

    /// Write the last AI-suggested command to the active PTY and close the panel.
    fn chat_panel_run_command(&mut self) {
        if let Some(cmd) = self.chat_panel.last_assistant_command() {
            let mut data = cmd.into_bytes();
            data.push(b'\r');
            if let Some(terminal) = self.active_terminal() {
                terminal.write_input(&data);
            }
            self.chat_panel.close();
            self.panel_focused = false;
            self.resize_terminals_for_panel();
        }
    }

    /// Drain the AI event channel and update the panel state.
    /// Returns true if anything changed (caller should request a redraw).
    fn poll_ai_events(&mut self) -> bool {
        use crossbeam_channel::TryRecvError;
        let mut changed = false;
        loop {
            match self.ai_rx.try_recv() {
                Ok(event) => {
                    changed = true;
                    match event {
                        AiEvent::Token(tok) => self.chat_panel.append_token(&tok),
                        AiEvent::Done       => self.chat_panel.mark_done(),
                        AiEvent::Error(e)   => {
                            log::error!("LLM error: {e}");
                            self.chat_panel.mark_error(e);
                        }
                    }
                }
                Err(TryRecvError::Empty | TryRecvError::Disconnected) => break,
            }
        }
        changed
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
            Action::ToggleAiMode | Action::EnableAiFeatures => {
                if !self.chat_panel.is_visible() {
                    self.chat_panel.open();
                    self.panel_focused = true;
                    self.resize_terminals_for_panel();
                } else {
                    self.panel_focused = true;
                }
            }
            Action::DisableAiFeatures => {
                self.chat_panel.close();
                self.panel_focused = false;
                self.resize_terminals_for_panel();
            }
            Action::ExplainLastOutput => {
                self.explain_last_output();
            }
            Action::FixLastError => {
                self.fix_last_error();
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

        // Ctrl+Shift+E — Explain Last Output (open panel + auto-submit).
        // Ctrl+Shift+F — Fix Last Error   (open panel + auto-submit with context).
        if ctrl && shift {
            if let Key::Character(s) = &event.logical_key {
                match s.as_str().to_ascii_lowercase().as_str() {
                    "e" => { self.explain_last_output(); return; }
                    "f" => { self.fix_last_error();       return; }
                    _ => {}
                }
            }
        }

        // Ctrl+C — open or close the AI panel.
        //
        // Behaviour by state:
        //   Panel closed                → open panel + focus it.
        //                                 NOTE: this intercepts Ctrl+C before it reaches
        //                                 the PTY. If a process is running, prefer using
        //                                 the leader key (TD-023) instead.
        //   Panel open + panel focused  → close panel.
        //   Panel open + term focused   → fall through; PTY receives Ctrl+C (SIGINT).
        //
        // Ctrl+V — switch focus between terminal and panel (when panel is open).
        //   Panel not visible           → fall through; PTY receives Ctrl+V.
        if ctrl {
            if let Key::Character(s) = &event.logical_key {
                match s.as_str() {
                    "c" | "C" => {
                        if !self.chat_panel.is_visible() {
                            self.chat_panel.open();
                            self.panel_focused = true;
                            self.resize_terminals_for_panel();
                            return;
                        } else if self.panel_focused {
                            self.chat_panel.close();
                            self.panel_focused = false;
                            self.resize_terminals_for_panel();
                            return;
                        }
                        // terminal focused + panel open → fall through as SIGINT
                    }
                    "v" | "V" => {
                        if self.chat_panel.is_visible() {
                            self.panel_focused = !self.panel_focused;
                            return;
                        }
                        // panel not visible → fall through
                    }
                    _ => {}
                }
            }
        }

        // Chat panel focused — route input to it (Cmd shortcuts still pass through).
        if self.chat_panel.is_visible() && self.panel_focused && !cmd {
            match &event.logical_key {
                Key::Named(NamedKey::Escape) => {
                    if matches!(self.chat_panel.state, PanelState::Error(_)) {
                        self.chat_panel.dismiss_error();
                    } else if !self.chat_panel.is_streaming() {
                        self.chat_panel.close();
                        self.panel_focused = false;
                        self.resize_terminals_for_panel();
                    }
                }
                Key::Named(NamedKey::Enter) => {
                    if self.chat_panel.is_idle() {
                        if self.chat_panel.input.trim().is_empty() {
                            // Empty input + Enter → run last assistant command
                            self.chat_panel_run_command();
                        } else {
                            self.submit_ai_query();
                        }
                    }
                }
                Key::Named(NamedKey::Backspace) => self.chat_panel.backspace(),
                Key::Named(NamedKey::Space)     => self.chat_panel.type_char(' '),
                Key::Character(s) => {
                    for c in s.chars() { self.chat_panel.type_char(c); }
                }
                _ => {}
            }
            return;
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
        let ctrl = self.modifiers.state().control_key();

        // Convert the logical key to bytes and write to the active PTY.
        // Arrow keys send application sequences (\x1bO_) when APP_CURSOR is set,
        // otherwise normal ANSI sequences (\x1b[_). This is required for atuin,
        // nvim, tmux, and any readline/ZLE widget that activates DECCKM.
        let bytes: Option<Vec<u8>> = match &event.logical_key {
            Key::Character(s) => {
                // When Ctrl is held, convert single ASCII printable chars to control bytes.
                // Ctrl+A=0x01 .. Ctrl+Z=0x1A; also Ctrl+[=0x1B, Ctrl+\=0x1C, etc.
                if ctrl {
                    let ch = s.chars().next().unwrap_or('\0');
                    let byte = ch as u8;
                    if byte.is_ascii_alphabetic() {
                        Some(vec![byte.to_ascii_lowercase() & 0x1F])
                    } else if matches!(byte, b'[' | b'\\' | b']' | b'^' | b'_' | b' ') {
                        Some(vec![byte & 0x1F])
                    } else {
                        Some(s.as_bytes().to_vec())
                    }
                } else {
                    Some(s.as_bytes().to_vec())
                }
            }
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
                    // without sending PtyEvent::Exit.
                    Err(TryRecvError::Disconnected) => {
                        log::warn!("PTY channel disconnected.");
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

            // Pre-compute the selection range once per frame.
            let sel_range: Option<SelectionRange> =
                term.selection.as_ref().and_then(|s| s.to_range(term));

            let mut result = Vec::with_capacity(rows);

            for row in 0..rows {
                let mut text = String::with_capacity(cols);
                let mut colors: Vec<(AnsiColor, AnsiColor)> = Vec::with_capacity(cols);
                let grid_line = Line(row as i32 - display_offset);

                for col in 0..cols {
                    let cell = &term.grid()[grid_line][Column(col)];
                    let ch = if cell.c == '\0' { ' ' } else { cell.c };
                    text.push(ch);

                    let (fg, bg) = if cell.flags.contains(Flags::INVERSE) {
                        (cell.bg, cell.fg)
                    } else {
                        (cell.fg, cell.bg)
                    };

                    // Invert colors for selected cells.
                    let (fg, bg) = if cell_in_selection(grid_line, Column(col), &sel_range) {
                        (bg, fg)
                    } else {
                        (fg, bg)
                    };

                    colors.push((fg, bg));
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

    /// Returns true when the mouse cursor is positioned over the chat panel area.
    fn mouse_in_panel(&self) -> bool {
        if !self.chat_panel.is_visible() { return false; }
        let (cw, _) = self.cell_dims();
        let pad_left = self.config.window.padding.left as f64;
        let (term_cols, _) = self.active_terminal_size();
        let term_right_px = pad_left + term_cols as f64 * cw as f64;
        self.mouse_pos.0 >= term_right_px
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

/// Returns true if `(line, col)` falls within `sel_range`.
/// Uses lexicographic ordering for simple selections and bounding-box for block selections.
fn cell_in_selection(line: Line, col: Column, sel_range: &Option<SelectionRange>) -> bool {
    let Some(range) = sel_range else { return false };
    if range.is_block {
        line >= range.start.line && line <= range.end.line
            && col >= range.start.column && col <= range.end.column
    } else {
        let pt = Point::new(line, col);
        pt >= range.start && pt <= range.end
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
        let ai_changed = self.poll_ai_events();
        if has_data || ai_changed {
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
                let lcd_atlas = self.renderer.as_mut().and_then(|r| r.get_lcd_atlas());
                let mut shaper = if let Some(r) = &mut self.renderer {
                    TextShaper::new(&r.device(), fs, &scaled_font, lcd_atlas)
                } else {
                    return;
                };

                // Share LCD atlas between TextShaper (rasterizer) and GpuRenderer (rendering)
                if let Some(r) = &mut self.renderer {
                    r.set_cell_size(shaper.cell_width, shaper.cell_height);
                    if let Some(atlas) = shaper.lcd_atlas.take() {
                        r.set_lcd_atlas(atlas);
                    }
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
                let (term_cols, term_rows) = self.active_terminal_size();
                let panel_visible = self.chat_panel.is_visible();
                let active_tab_idx = self.tabs.active_index();
                let terminal_id = self.panes.get(active_tab_idx).map(|p| p.focused_terminal).unwrap_or(0);

                if let (Some(renderer), Some(shaper)) =
                    (&mut self.renderer, &mut self.shaper)
                {
                    // Attempt to build instances. If the atlas is full, clear it and retry once.
                    let result = build_instances(
                        &mut self.instances,
                        &mut self.lcd_instances,
                        &cell_data,
                        shaper,
                        renderer,
                        &self.config,
                        &scaled_font,
                        cursor.as_ref(),
                        self.cursor_blink_on,
                        &mut self.row_cache,
                        terminal_id,
                    );

                    if let Err(crate::renderer::atlas::AtlasError::Full) = result {
                        log::warn!("Glyph atlas full; clearing and re-rendering (generation {}).", self.atlas_generation + 1);
                        renderer.atlas.clear(&renderer.device());
                        if let Some(atlas) = renderer.get_lcd_atlas() {
                            atlas.borrow_mut().clear(&renderer.device());
                        }
                        self.row_cache.clear();
                        self.atlas_generation += 1;

                        // Second attempt with clean atlas.
                        let _ = build_instances(
                            &mut self.instances,
                            &mut self.lcd_instances,
                            &cell_data,
                            shaper,
                            renderer,
                            &self.config,
                            &scaled_font,
                            cursor.as_ref(),
                            self.cursor_blink_on,
                            &mut self.row_cache,
                            terminal_id,
                        );
                    }

                    if panel_visible {
                        build_chat_panel_instances(
                            &mut self.instances,
                            &self.chat_panel,
                            self.panel_focused,
                            shaper,
                            renderer,
                            &self.config,
                            &scaled_font,
                            term_cols,
                            term_rows,
                        );
                    }

                    if self.palette.visible {
                        self.row_cache.dirty_rows.fill(true); // Force full upload for overlay
                        build_palette_instances(
                            &mut self.instances,
                            &self.palette,
                            shaper,
                            renderer,
                            &self.config,
                            &scaled_font,
                            term_cols + if panel_visible { self.chat_panel.width_cols as usize } else { 0 },
                            term_rows,
                        );
                    }

                    // TD-032: Dirty-row tracking.
                    let cols = term_cols as usize + if panel_visible { self.chat_panel.width_cols as usize } else { 0 };
                    
                    if self.palette.visible {
                        // If palette is visible, bypass optimization and upload everything.
                        // Overlays like the palette don't align with the row-based cache.
                        renderer.upload_instances(&self.instances, 0);
                    } else {
                        // Standard optimized path for terminal content.
                        for (row_idx, is_dirty) in self.row_cache.dirty_rows.iter_mut().enumerate() {
                            if *is_dirty {
                                let start = row_idx * cols;
                                let end = (start + cols).min(self.instances.len());
                                if start < self.instances.len() {
                                    renderer.upload_instances(&self.instances[start..end], start);
                                }
                                *is_dirty = false;
                            }
                        }
                    }

                    // Cursor is not cached, upload it every frame at the end of the buffer.
                    if self.instances.len() > 0 {
                        let cursor_idx = self.instances.len() - 1;
                        renderer.upload_instances(&self.instances[cursor_idx..], cursor_idx);
                    }

                    renderer.set_cell_count(self.instances.len());
                    renderer.upload_lcd_instances(&self.lcd_instances);
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
                        let pad_top = self.config.window.padding.top as f64;
                        if self.mouse_pos.1 < pad_top {
                            // Click in the title bar zone — move the window.
                            if let Some(window) = &self.window {
                                let _ = window.drag_window();
                            }
                            return;
                        }
                        self.mouse_left_pressed = true;
                        let any_mouse = self.active_terminal()
                            .map(|t| t.mouse_mode_flags().0)
                            .unwrap_or(false);
                        if !any_mouse {
                            // Local selection — only when the app (not the PTY process) owns mouse.
                            if let Some(terminal) = self.active_terminal() {
                                terminal.start_selection(col, row, SelectionType::Simple);
                            }
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
                        // On macOS, PixelDelta.y is NEGATIVE when swiping up (natural
                        // scrolling). Negate so the sign matches LineDelta convention:
                        // positive = scroll up = show older history.
                        -pos.y / ch_logical
                    }
                };
                self.scroll_pixel_accum += delta_lines;
                let lines = self.scroll_pixel_accum.trunc() as i32;
                self.scroll_pixel_accum -= lines as f64;
                if lines == 0 { return; }

                // If the mouse is over the chat panel, scroll the panel history.
                // Convention matches the terminal: lines > 0 (swipe up) = older content.
                if self.mouse_in_panel() {
                    if lines > 0 {
                        self.chat_panel.scroll_down(lines as usize);
                    } else {
                        self.chat_panel.scroll_up((-lines) as usize);
                    }
                    if let Some(window) = &self.window {
                        window.request_redraw();
                    }
                    return;
                }

                let (col, row) = self.pixel_to_cell(self.mouse_pos.0, self.mouse_pos.1);
                let (any_mouse, _sgr, _motion) = self.active_terminal()
                    .map(|t| t.mouse_mode_flags())
                    .unwrap_or((false, false, false));

                if any_mouse {
                    // Forward scroll as mouse button reports.
                    // btn=64 = wheel-up (toward top of file/history)
                    // btn=65 = wheel-down (toward bottom)
                    // Convention: lines > 0 = finger swiped up (macOS natural scroll).
                    // With natural scrolling, swipe-up = "push content up" = see content
                    // below = scroll DOWN in the application → btn=65.
                    // Note: if tmux scroll appears reversed after this change, swap back to
                    // `if lines > 0 { 64 } else { 65 }` — tmux and vim may disagree.
                    let btn = if lines > 0 { 65u8 } else { 64u8 };
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

            WindowEvent::DroppedFile(path) => {
                let path_str = path.to_string_lossy().into_owned();
                if self.chat_panel.is_visible() {
                    // Append path to chat input (panel has focus)
                    self.chat_panel.append_path(&path_str);
                } else {
                    // Paste path directly to the active PTY
                    if let Some(terminal) = self.active_terminal() {
                        terminal.write_input(path_str.as_bytes());
                    }
                }
                if let Some(window) = &self.window {
                    window.request_redraw();
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
        let ai_changed = self.poll_ai_events();
        if has_data || ai_changed {
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

/// Rasterize a single text row into `CellVertex` instances.
///
/// `col_offset` shifts every glyph's grid column by that amount, which places
/// panel rows to the right of the terminal without any shader changes.
fn push_shaped_row(
    text: &str,
    fg: [f32; 4],
    bg: [f32; 4],
    row: usize,
    col_offset: usize,
    width: usize,
    shaper: &mut TextShaper,
    renderer: &mut GpuRenderer,
    font: &crate::config::schema::FontConfig,
    instances: &mut Vec<CellVertex>,
) {
    if width == 0 { return; }

    // 1. Prepare text and colors
    let chars: Vec<char> = text.chars().take(width).collect();
    let len = chars.len();
    let padded: String = chars
        .into_iter()
        .chain(std::iter::repeat(' ').take(width.saturating_sub(len)))
        .collect();

    let colors: Vec<([f32; 4], [f32; 4])> = (0..width).map(|_| (fg, bg)).collect();
    let shaped = shaper.shape_line(&padded, &colors, font);

    // 2. Map glyphs to grid columns.
    for glyph in shaped.glyphs {
        if glyph.col >= width { continue; }

        let (atlas, queue) = renderer.atlas_and_queue();
        let entry = match shaper.rasterize_to_atlas(glyph.cache_key, atlas, queue) {
            Ok(e) => e,
            Err(_) => crate::renderer::atlas::AtlasEntry {
                uv: [0.0; 4],
                width: 0, height: 0, bearing_x: 0, bearing_y: 0,
            },
        };

        // UI overlay rendering: apply the SAME math as terminal cells (clamping + UV shift)
        let ox = entry.bearing_x as f32;
        let oy = shaped.ascent - entry.bearing_y as f32;
        let gw = entry.width as f32;
        let gh = entry.height as f32;

        let y0 = oy.max(0.0);
        let y1 = (oy + gh).min(shaper.cell_height);

        let (atlas_uv, glyph_offset, glyph_size) = if y1 <= y0 || gw == 0.0 || gh == 0.0 {
            ([0.0f32; 4], [0.0; 2], [0.0; 2])
        } else {
            let fy0 = (y0 - oy) / gh;
            let fy1 = (y1 - oy) / gh;
            let [u0, v0, u1, v1] = entry.uv;
            ([u0, v0 + fy0 * (v1 - v0), u1, v0 + fy1 * (v1 - v0)], [ox, y0], [gw, y1 - y0])
        };

        instances.push(CellVertex {
            grid_pos: [(col_offset + glyph.col) as f32, row as f32],
            atlas_uv,
            fg,
            bg,
            glyph_offset,
            glyph_size,
            flags: 0,
            _pad: 0,
        });
    }
}

/// Build GPU instances for the right-side chat panel.
///
/// The panel occupies columns `[term_cols .. term_cols + panel.width_cols]`
/// using the same `CellVertex` pipeline as the terminal — no shader changes.
/// The terminal must have already been resized to `term_cols` columns.
///
/// Layout (bottom-up):
/// - Row N-1 : hints
/// - Row N-2 : input line 2 (continuation / blank)
/// - Row N-3 : input line 1 (" > …")
/// - Row N-4 : separator
/// - Rows 1..N-5 : scrollable message history
/// - Row 0  : header
fn build_chat_panel_instances(
    instances: &mut Vec<CellVertex>,
    panel: &ChatPanel,
    panel_focused: bool,
    shaper: &mut TextShaper,
    renderer: &mut GpuRenderer,
    config: &Config,
    font: &crate::config::schema::FontConfig,
    term_cols: usize,
    screen_rows: usize,
) {
    use crate::llm::chat_panel::{PanelState, titled_separator, word_wrap, wrap_input};
    use crate::llm::ChatRole;

    let panel_cols = panel.width_cols as usize;
    // Need at least: header + separator + 2 input rows + hints = 5
    if panel_cols == 0 || screen_rows < 5 { return; }

    let ui_cfg = &config.llm.ui;
    let panel_bg       = ui_cfg.background;
    let user_fg        = ui_cfg.user_fg;
    let asst_fg        = ui_cfg.assistant_fg;
    let input_fg       = ui_cfg.input_fg;

    const SEP_FG:         [f32; 4] = [0.35, 0.30, 0.52, 1.0];
    const HEADER_FOCUS:   [f32; 4] = [0.75, 0.65, 1.00, 1.0];
    const HEADER_UNFOCUS: [f32; 4] = [0.42, 0.38, 0.58, 1.0]; // dim when terminal has focus
    const STREAM_FG:      [f32; 4] = [0.95, 0.88, 0.45, 1.0];
    const INPUT_DIM:      [f32; 4] = [0.55, 0.52, 0.65, 1.0]; // dim when not focused
    const HINT_FG:        [f32; 4] = [0.42, 0.40, 0.52, 1.0];
    const ERR_FG:         [f32; 4] = [1.00, 0.55, 0.45, 1.0];

    // Fixed row indices (2 input rows)
    let hints_row  = screen_rows - 1;
    let input_row2 = screen_rows - 2;
    let input_row1 = screen_rows - 3;
    let sep_row    = screen_rows - 4;
    let history_start = 1_usize;
    let history_end   = sep_row;
    let history_rows  = history_end.saturating_sub(history_start);

    // ── Build rendered history lines ─────────────────────────────────────────
    let inner_w = panel_cols.saturating_sub(6);
    let mut all_lines: Vec<(String, [f32; 4])> = Vec::new();

    for msg in &panel.messages {
        let (prefix, cont, fg) = match msg.role {
            ChatRole::User      => (" You  ", "      ", user_fg),
            ChatRole::Assistant => ("  AI  ", "      ", asst_fg),
            ChatRole::System    => continue,
        };
        let wrapped = word_wrap(&msg.content, inner_w);
        for (i, line) in wrapped.iter().enumerate() {
            let p = if i == 0 { prefix } else { cont };
            all_lines.push((format!("{p}{line}"), fg));
        }
        all_lines.push((String::new(), HINT_FG));
    }

    if matches!(panel.state, PanelState::Streaming) {
        if panel.streaming_buf.is_empty() {
            all_lines.push(("  AI  …".to_string(), STREAM_FG));
        } else {
            let wrapped = word_wrap(&panel.streaming_buf, inner_w);
            for (i, line) in wrapped.iter().enumerate() {
                let p = if i == 0 { "  AI  " } else { "      " };
                all_lines.push((format!("{p}{line}"), STREAM_FG));
            }
        }
    } else if matches!(panel.state, PanelState::Loading) {
        all_lines.push(("  AI  waiting…".to_string(), STREAM_FG));
    }

    let total = all_lines.len();
    let visible_start = if total > history_rows {
        (total - history_rows).saturating_sub(panel.scroll_offset)
    } else {
        0
    };

    // ── Build wrapped input lines ────────────────────────────────────────────
    // Input area is 2 rows. We wrap by characters and show the last 2 lines,
    // appending the cursor block to the very last character.
    let input_prefix = " > ";
    let prefix_len = input_prefix.chars().count();
    let input_inner_w = panel_cols.saturating_sub(prefix_len);

    let (input_line1, input_line2, input_fg) = match &panel.state {
        PanelState::Error(e) => {
            let msg: String = e.chars().take(panel_cols.saturating_sub(4)).collect();
            (format!(" ! {msg}"), String::new(), ERR_FG)
        }
        _ => {
            let fg = if panel_focused { input_fg } else { INPUT_DIM };
            let raw = if panel_focused && panel.is_idle() {
                format!("{}▋", panel.input)
            } else {
                panel.input.clone()
            };
            let lines = wrap_input(&raw, input_inner_w);
            let total_lines = lines.len();
            // Always show 2 rows; take the last 2 wrapped lines
            let l1 = if total_lines >= 2 {
                format!("{}{}", if total_lines == 2 { input_prefix } else { "   " },
                        lines[total_lines - 2])
            } else {
                // Only 1 line — show on row1 with prefix
                lines.first().map(|l| format!("{input_prefix}{l}")).unwrap_or_default()
            };
            let l2 = if total_lines >= 2 {
                format!("   {}", lines[total_lines - 1])
            } else {
                String::new()
            };
            (l1, l2, fg)
        }
    };

    // ── Assemble rows ────────────────────────────────────────────────────────
    let co = term_cols;
    let header_fg = if panel_focused { HEADER_FOCUS } else { HEADER_UNFOCUS };

    let mut push = |text: &str, fg, row, instances: &mut Vec<CellVertex>| {
        push_shaped_row(text, fg, panel_bg, row, co, panel_cols, shaper, renderer, font, instances);
    };

    // Row 0: header (bright when focused, dim when terminal has focus)
    push(&titled_separator("⚡ Petrubot", panel_cols), header_fg, 0, instances);

    // History rows
    for i in 0..history_rows {
        let row = history_start + i;
        let (text, fg) = all_lines
            .get(visible_start + i)
            .map(|(t, f)| (t.as_str(), *f))
            .unwrap_or(("", panel_bg));
        push(text, fg, row, instances);
    }

    // Separator
    push(&"─".repeat(panel_cols), SEP_FG, sep_row, instances);

    // Input rows (wrapped, 2 lines)
    push(&input_line1, input_fg, input_row1, instances);
    push(&input_line2, input_fg, input_row2, instances);

    // Hints
    let hints = if !panel_focused {
        " [Ctrl+V] focus chat  [Ctrl+C] close"
    } else {
        match &panel.state {
            PanelState::Idle if panel.input.trim().is_empty()
                && panel.messages.iter().any(|m| matches!(m.role, ChatRole::Assistant))
                => " [Enter] run last  [Ctrl+C] close",
            PanelState::Idle      => " [Enter] send  [Ctrl+C] close",
            PanelState::Streaming |
            PanelState::Loading   => " [Ctrl+C] close",
            PanelState::Error(_)  => " [Esc] dismiss",
            PanelState::Hidden    => "",
        }
    };
    push(hints, HINT_FG, hints_row, instances);
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

    // Disable window drag from content area — dragging is handled explicitly
    // via Window::drag_window() only when the click is in the title bar zone.
    let () = msg_send![ns_win, setMovableByWindowBackground: Bool::NO];
}


fn calculate_row_hash(text: &str, colors: &[([f32; 4], [f32; 4])]) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    text.hash(&mut hasher);
    // Rough hash of colors to avoid full float bit-pattern hashing
    for (fg, bg) in colors {
        ((fg[0] * 255.0) as u32).hash(&mut hasher);
        ((bg[0] * 255.0) as u32).hash(&mut hasher);
    }
    hasher.finish()
}

/// Build the GPU instance list from raw terminal cell data.
fn build_instances(
    instances: &mut Vec<CellVertex>,
    lcd_instances: &mut Vec<CellVertex>,
    cell_data: &[(String, Vec<(AnsiColor, AnsiColor)>)],
    shaper: &mut TextShaper,
    renderer: &mut GpuRenderer,
    config: &Config,
    font: &crate::config::schema::FontConfig,
    cursor: Option<&CursorInfo>,
    cursor_blink_on: bool,
    row_cache: &mut RowCache,
    terminal_id: usize,
) -> Result<(), crate::renderer::atlas::AtlasError> {
    instances.clear();
    lcd_instances.clear();

    if row_cache.rows.len() < cell_data.len() {
        row_cache.rows.resize(cell_data.len(), None);
        row_cache.dirty_rows.resize(cell_data.len(), true);
    }

    if row_cache.terminal_id != Some(terminal_id) {
        row_cache.clear();
        row_cache.terminal_id = Some(terminal_id);
    }

    // Scratch buffer for per-row color resolution.
    let mut colors_scratch: Vec<([f32; 4], [f32; 4])> = Vec::with_capacity(256);

    for (row_idx, (text, raw_colors)) in cell_data.iter().enumerate() {
        // Resolve colors.
        colors_scratch.clear();
        colors_scratch.extend(raw_colors.iter().map(|(fg, bg)| {
            (
                resolve_color(*fg, &config.colors),
                resolve_color(*bg, &config.colors),
            )
        }));
        let colors: &[([f32; 4], [f32; 4])] = &colors_scratch;

        let row_hash = calculate_row_hash(text, colors);

        if let Some(Some(entry)) = row_cache.rows.get(row_idx) {
            if entry.hash == row_hash {
                instances.extend_from_slice(&entry.instances);
                lcd_instances.extend_from_slice(&entry.lcd_instances);
                continue;
            }
        }

        // Cache miss: Shape and build instances for this row.
        row_cache.dirty_rows[row_idx] = true;
        let mut row_instances = Vec::new();
        let mut row_lcd_instances = Vec::new();

        let shaped = shaper.shape_line(text, &colors, font);

        for glyph in &shaped.glyphs {
            let (atlas, queue) = renderer.atlas_and_queue();
            let entry = shaper.rasterize_to_atlas(glyph.cache_key, atlas, queue)?;

            let (atlas_uv, glyph_offset, glyph_size) = {
                let ox = entry.bearing_x as f32;
                let oy = shaped.ascent - entry.bearing_y as f32;
                let gw = entry.width as f32;
                let gh = entry.height as f32;
                let y0 = oy.max(0.0);
                let y1 = (oy + gh).min(shaper.cell_height);
                if y1 <= y0 || gw == 0.0 || gh == 0.0 {
                    ([0.0f32; 4], [0.0; 2], [0.0; 2])
                } else {
                    let fy0 = (y0 - oy) / gh;
                    let fy1 = (y1 - oy) / gh;
                    let [u0, v0, u1, v1] = entry.uv;
                    ([u0, v0 + fy0 * (v1 - v0), u1, v0 + fy1 * (v1 - v0)], [ox, y0], [gw, y1 - y0])
                }
            };

            row_instances.push(CellVertex {
                grid_pos: [glyph.col as f32, row_idx as f32],
                atlas_uv,
                fg: glyph.fg,
                bg: glyph.bg,
                glyph_offset,
                glyph_size,
                flags: 0,
                _pad: 0,
            });

            if renderer.has_lcd() {
                if let Some((_lcd_atlas, queue)) = renderer.lcd_atlas_and_queue() {
                    if let Some(entry) = shaper.rasterize_lcd_to_atlas(glyph.ch, queue) {
                        let ox = (entry.bearing_x * 3) as f32;
                        let oy = shaped.ascent - entry.bearing_y as f32;
                        let gw = entry.width as f32;
                        let gh = entry.height as f32;
                        let y0 = oy.max(0.0);
                        let y1 = (oy + gh).min(shaper.cell_height);

                        if y1 > y0 && gw > 0.0 && gh > 0.0 {
                            let fy0 = (y0 - oy) / gh;
                            let fy1 = (y1 - oy) / gh;
                            let [u0, v0, u1, v1] = entry.uv;
                            row_lcd_instances.push(CellVertex {
                                grid_pos: [glyph.col as f32, row_idx as f32],
                                atlas_uv: [u0, v0 + fy0 * (v1 - v0), u1, v0 + fy1 * (v1 - v0)],
                                fg: glyph.fg,
                                bg: glyph.bg,
                                glyph_offset: [ox, y0],
                                glyph_size: [gw, y1 - y0],
                                flags: FLAG_LCD,
                                _pad: 0,
                            });
                        }
                    }
                }
            }
        }

        instances.extend_from_slice(&row_instances);
        lcd_instances.extend_from_slice(&row_lcd_instances);

        row_cache.rows[row_idx] = Some(RowCacheEntry {
            hash: row_hash,
            glyphs: shaped.glyphs,
            instances: row_instances,
            lcd_instances: row_lcd_instances,
        });
    }

    row_cache.terminal_id = Some(terminal_id);

    // Cursor instance — always appended (not cached).
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
    Ok(())
}

/// Build GPU instances for the command palette overlay.
fn build_palette_instances(
    instances: &mut Vec<CellVertex>,
    palette: &CommandPalette,
    shaper: &mut TextShaper,
    renderer: &mut GpuRenderer,
    _config: &Config,
    font: &crate::config::schema::FontConfig,
    total_cols: usize,
    total_rows: usize,
) {
    use crate::ui::palette::PaletteAction;

    let palette_width = 60_usize;
    let palette_height = 15_usize;

    if total_cols < palette_width || total_rows < palette_height {
        return;
    }

    let start_col = (total_cols - palette_width) / 2;
    let start_row = (total_rows - palette_height) / 2;

    let bg = [0.05, 0.05, 0.10, 0.95];
    let fg = [1.0, 1.0, 1.0, 1.0];
    let highlight_bg = [0.2, 0.2, 0.4, 1.0];
    let prompt_fg = [0.5, 0.8, 1.0, 1.0];

    // 1. Draw input line
    let prompt = format!(" > {}▋", palette.query);
    push_shaped_row(&prompt, prompt_fg, bg, start_row, start_col, palette_width, shaper, renderer, font, instances);

    // 2. Draw results
    for i in 0..(palette_height - 1) {
        let row = start_row + 1 + i;
        let is_selected = i == palette.selected;
        let current_bg = if is_selected { highlight_bg } else { bg };

        let text = if let Some(action) = palette.results.get(i) {
            format!("  {}", action.name)
        } else {
            String::new()
        };

        push_shaped_row(&text, fg, current_bg, row, start_col, palette_width, shaper, renderer, font, instances);
    }
}

impl Drop for App {
    fn drop(&mut self) {
        log::info!("App dropping; shutting down PTYs.");
        for terminal in self.terminals.iter_mut().flatten() {
            terminal.pty.shutdown();
        }
    }
}
