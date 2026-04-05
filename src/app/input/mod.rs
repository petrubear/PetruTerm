use std::collections::HashMap;
use std::time::Instant;
use winit::event::{Modifiers, ElementState, KeyEvent};
use winit::event_loop::ActiveEventLoop;
use winit::keyboard::{Key, NamedKey};
use crate::config::Config;
use crate::app::mux::Mux;
use crate::app::ui::UiManager;
use crate::llm::chat_panel::PanelState;
use crate::app::renderer::RenderContext;
use crate::ui::palette::Action;
use alacritty_terminal::term::TermMode;

pub mod key_map;

/// Manages keyboard and mouse input state, including the leader key and cursor blinking.
pub struct InputHandler {
    pub modifiers: Modifiers,
    pub leader_active: bool,
    pub leader_timer: Option<Instant>,
    pub leader_timeout_ms: u64,
    /// Maps leader-key characters (e.g. "a", "%") → Action, built from `config.keys`.
    pub leader_map: HashMap<String, Action>,

    // Mouse state
    pub mouse_pos: (f64, f64),
    pub mouse_left_pressed: bool,
    pub scroll_pixel_accum: f64,

    // Cursor blink state
    pub cursor_blink_on: bool,
    pub cursor_last_blink: Instant,
}

impl InputHandler {
    pub fn new(config: &Config) -> Self {
        let leader_map = config.keys.iter()
            .filter(|kb| kb.mods.to_ascii_uppercase() == "LEADER")
            .filter_map(|kb| {
                let action = kb.action.parse::<Action>().ok()?;
                Some((kb.key.clone(), action))
            })
            .collect();

        Self {
            modifiers: Modifiers::default(),
            leader_active: false,
            leader_timer: None,
            leader_timeout_ms: config.leader.timeout_ms,
            leader_map,
            mouse_pos: (0.0, 0.0),
            mouse_left_pressed: false,
            scroll_pixel_accum: 0.0,
            cursor_blink_on: true,
            cursor_last_blink: Instant::now(),
        }
    }

    pub fn update_cursor_blink(&mut self) -> bool {
        const BLINK_MS: u64 = 530;
        if self.cursor_last_blink.elapsed() >= std::time::Duration::from_millis(BLINK_MS) {
            self.cursor_blink_on = !self.cursor_blink_on;
            self.cursor_last_blink = Instant::now();
            return true;
        }
        false
    }

    pub fn pixel_to_cell(&self, x: f64, y: f64, config: &Config, render_ctx: &Option<RenderContext>, mux: &Mux) -> (usize, usize) {
        let pad = &config.window.padding;
        let (cw, ch) = render_ctx.as_ref()
            .map(|rc| (rc.shaper.cell_width as f64, rc.shaper.cell_height as f64))
            .unwrap_or((8.0, 16.0));
        let col = ((x - pad.left as f64) / cw).floor().max(0.0) as usize;
        let row = ((y - pad.top as f64) / ch).floor().max(0.0) as usize;
        let (term_cols, term_rows) = mux.active_terminal_size();
        (col.min(term_cols.saturating_sub(1)), row.min(term_rows.saturating_sub(1)))
    }

    pub fn send_mouse_report(&self, button: u8, col: usize, row: usize, pressed: bool, mux: &Mux) {
        let Some(terminal) = mux.active_terminal() else { return };
        let (any_mouse, sgr, _) = terminal.mouse_mode_flags();
        if !any_mouse { return; }
        if sgr {
            let c = if pressed { 'M' } else { 'm' };
            terminal.write_input(format!("\x1b[<{button};{};{}{c}", col+1, row+1).as_bytes());
        } else if pressed {
            let b = button.saturating_add(32);
            let x = ((col+1) as u8).saturating_add(32).min(255);
            let y = ((row+1) as u8).saturating_add(32).min(255);
            terminal.write_input(&[0x1b, b'[', b'M', b, x, y]);
        }
    }

    pub fn handle_key_input(
        &mut self,
        event: &KeyEvent,
        event_loop: &ActiveEventLoop,
        config: &Config,
        mux: &mut Mux,
        ui: &mut UiManager,
        render_ctx: &mut Option<RenderContext>,
        window: Option<&winit::window::Window>,
        wakeup_proxy: winit::event_loop::EventLoopProxy<()>,
    ) {
        if event.state != ElementState::Pressed { return; }
        self.cursor_blink_on = true;
        self.cursor_last_blink = Instant::now();

        if self.leader_active {
            if let Some(t) = self.leader_timer {
                if t.elapsed().as_millis() > self.leader_timeout_ms as u128 {
                    self.leader_active = false;
                    self.leader_timer = None;
                }
            }
        }

        let cmd = self.modifiers.state().super_key();
        let ctrl = self.modifiers.state().control_key();
        let shift = self.modifiers.state().shift_key();

        // ── Leader key activation — checked BEFORE panel/palette handlers so that
        // Ctrl+B always activates the leader even when the AI panel is focused.
        if ctrl && !shift && !cmd {
            if let Key::Character(s) = &event.logical_key {
                if s.as_str() == config.leader.key.as_str() {
                    self.leader_active = true;
                    self.leader_timer = Some(Instant::now());
                    return;
                }
            }
        }

        // ── Leader key dispatch ───────────────────────────────────────────────
        if self.leader_active {
            self.leader_active = false;
            self.leader_timer = None;
            if let Key::Character(s) = &event.logical_key {
                let key = s.to_ascii_lowercase();
                let action = self.leader_map.get(s.as_str())
                    .or_else(|| self.leader_map.get(key.as_str()))
                    .cloned();
                if let Some(action) = action {
                    if action == Action::Quit {
                        event_loop.exit();
                        return;
                    }
                    if let Some(rc) = render_ctx.as_mut() {
                        let mut cfg_temp = config.clone();
                        ui.handle_palette_action(action, mux, rc, &mut cfg_temp, window, wakeup_proxy);
                    }
                }
            }
            return;
        }

        // ── Ctrl+Space — toggle inline AI block ──────────────────────────────
        if ctrl && !shift && !cmd {
            if let Key::Named(NamedKey::Space) = &event.logical_key {
                if ui.ai_block.is_visible() {
                    ui.ai_block.close();
                } else {
                    ui.ai_block.open();
                }
                return;
            }
        }

        // ── Inline AI block input ────────────────────────────────────────────
        if ui.ai_block.is_visible() && !cmd {
            match &event.logical_key {
                Key::Named(NamedKey::Escape) => { ui.ai_block.close(); }
                Key::Named(NamedKey::Enter) => {
                    if ui.ai_block.is_typing() {
                        ui.submit_ai_block_query(wakeup_proxy);
                    } else if ui.ai_block.is_done() {
                        ui.run_ai_block_command(mux);
                    }
                }
                Key::Named(NamedKey::Backspace) => ui.ai_block.backspace(),
                Key::Named(NamedKey::Space)     => ui.ai_block.type_char(' '),
                Key::Character(s) => { for ch in s.chars() { ui.ai_block.type_char(ch); } }
                _ => {}
            }
            return;
        }

        // ── Chat panel input ─────────────────────────────────────────────────
        if ui.is_panel_visible() && ui.panel_focused && !cmd {
            match &event.logical_key {
                Key::Named(NamedKey::Escape) => {
                    if matches!(ui.panel().state, PanelState::Error(_)) { ui.panel_mut().dismiss_error(); }
                    else if !ui.panel().is_streaming() { ui.panel_mut().close(); ui.panel_focused = false; }
                }
                Key::Named(NamedKey::Enter) => {
                    if ui.panel().is_idle() {
                        if ui.panel().input.trim().is_empty() { ui.chat_panel_run_command(mux); }
                        else { ui.submit_ai_query(wakeup_proxy); }
                    }
                }
                Key::Named(NamedKey::Backspace) => ui.panel_mut().backspace(),
                Key::Named(NamedKey::Space)     => ui.panel_mut().type_char(' '),
                Key::Character(s) => { for ch in s.chars() { ui.panel_mut().type_char(ch); } }
                _ => {}
            }
            return;
        }

        if ui.palette.visible {
            match &event.logical_key {
                Key::Named(NamedKey::Escape) => ui.palette.close(),
                Key::Named(NamedKey::Enter) => { 
                    if let Some(action) = ui.palette.confirm() { 
                        let rc = render_ctx.as_mut().expect("RenderContext");
                        let mut cfg_temp = config.clone(); 
                        ui.handle_palette_action(action, mux, rc, &mut cfg_temp, window, wakeup_proxy);
                    } 
                }
                Key::Named(NamedKey::ArrowUp) => ui.palette.select_up(),
                Key::Named(NamedKey::ArrowDown) => ui.palette.select_down(),
                Key::Named(NamedKey::Backspace) => ui.palette.backspace(),
                Key::Character(s) => { for ch in s.chars() { ui.palette.type_char(ch); } }
                _ => {}
            }
            return;
        }

        if cmd && !shift && !ctrl {
            if let Key::Character(s) = &event.logical_key {
                match s.as_str() {
                    // System clipboard — always Cmd+C / Cmd+V, not configurable via leader.
                    "q" => { event_loop.exit(); return; }
                    "c" => {
                        if let Some(terminal) = mux.active_terminal() {
                            if let Some(text) = terminal.selection_text() {
                                if let Ok(mut cb) = arboard::Clipboard::new() { let _ = cb.set_text(text); }
                            }
                        }
                        return;
                    }
                    "v" => {
                        if let Some(terminal) = mux.active_terminal() {
                            if let Ok(text) = arboard::Clipboard::new().and_then(|mut cb| cb.get_text()) {
                                if terminal.bracketed_paste_mode() {
                                    let mut data = b"\x1b[200~".to_vec();
                                    data.extend_from_slice(text.as_bytes());
                                    data.extend_from_slice(b"\x1b[201~");
                                    terminal.write_input(&data);
                                } else { terminal.write_input(text.as_bytes()); }
                            }
                        }
                        return;
                    }
                    // Cmd+1-9: switch tab by index (standard macOS pattern).
                    _ => { if let Ok(n) = s.parse::<usize>() { if n >= 1 && n <= 9 { mux.tabs.switch_to_index(n-1); return; } } }
                }
            }
        }

        self.send_key_to_active_terminal(event, mux);
    }

    pub fn send_key_to_active_terminal(&self, event: &KeyEvent, mux: &Mux) {
        let mode = mux.active_terminal().map(|t| *t.term.lock().mode()).unwrap_or(TermMode::empty());
        
        if let Some(data) = key_map::translate_key(&event.logical_key, self.modifiers, mode) {
            if let Some(terminal) = mux.active_terminal() {
                terminal.write_input(&data);
            }
        }
    }
}
