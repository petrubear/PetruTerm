use crate::app::mux::Mux;
use crate::app::renderer::RenderContext;
use crate::app::ui::UiManager;
use crate::config::Config;
use crate::llm::chat_panel::PanelState;
use crate::ui::palette::Action;
use alacritty_terminal::term::TermMode;
use std::collections::HashMap;
use std::time::Instant;
use winit::event::{ElementState, KeyEvent, Modifiers};
use winit::event_loop::ActiveEventLoop;
use winit::keyboard::{Key, KeyCode, NamedKey, PhysicalKey};

pub mod key_map;

/// Active separator drag: identifies which Split is being resized via mouse.
pub struct SeparatorDragState {
    /// Stable ID of the Split node being dragged — does not change as layout updates.
    pub node_id: u32,
}

/// Manages keyboard and mouse input state, including the leader key and cursor blinking.
pub struct InputHandler {
    pub modifiers: Modifiers,
    pub leader_active: bool,
    pub leader_deadline: Option<Instant>,
    pub leader_timeout_ms: u64,
    /// Maps leader-key characters (e.g. "a", "%") → Action, built from `config.keys`.
    pub leader_map: HashMap<String, Action>,

    // Mouse state
    pub mouse_pos: (f64, f64),
    pub mouse_left_pressed: bool,
    /// True once the mouse moves between LMB press and release. Used to decide
    /// whether a click was a drag (keep selection) or a plain click (clear it,
    /// so a single-cell selection doesn't linger with inverted colours).
    pub mouse_dragged: bool,
    pub scroll_pixel_accum: f64,
    /// Consecutive click count (1 = single, 2 = double, 3+ = triple) for selection type.
    pub click_count: u32,
    pub last_click_time: Instant,
    pub last_click_cell: (usize, usize),
    /// Active separator drag (mouse drag resize). Set on LMB press near a separator.
    pub dragging_separator: Option<SeparatorDragState>,

    // Cursor blink state
    pub cursor_blink_on: bool,
    pub cursor_last_blink: Instant,

    /// Set when a <leader>+Option+Arrow pane-resize keybind fires so that
    /// `app/mod.rs` knows to call `resize_terminals_for_panel` afterward.
    pub pane_ratio_adjusted: bool,
    /// Set when <leader>+e+e fires so `app/mod.rs` can toggle the sidebar.
    pub toggle_sidebar_requested: bool,
    /// Pane resize mode: activated by <leader>+Option+Arrow. While active, any
    /// arrow key (with or without Option) continues resizing. Any other key exits.
    pub resize_mode: bool,
    /// Rolling buffer of printable chars sent to the PTY since the last Enter/Ctrl-C.
    /// Used only for snippet Tab-trigger matching — cleared on newline, backspace trims.
    input_echo: String,
    /// Timestamp of the last keypress that was forwarded to the terminal.
    /// Used to measure input-to-pixel latency under RUST_LOG=petruterm=debug.
    pub last_key_instant: Option<std::time::Instant>,
    /// Set when the first key of a two-key leader sequence is pressed (e.g. 'W' for workspace).
    pub leader_prefix: Option<char>,
}

impl InputHandler {
    pub fn new(config: &Config) -> Self {
        let leader_map = config
            .keys
            .iter()
            .filter(|kb| kb.mods.eq_ignore_ascii_case("LEADER"))
            .filter_map(|kb| {
                let action = kb.action.parse::<Action>().ok()?;
                Some((kb.key.clone(), action))
            })
            .collect();

        Self {
            modifiers: Modifiers::default(),
            leader_active: false,
            leader_deadline: None,
            leader_timeout_ms: config.leader.timeout_ms,
            leader_map,
            mouse_pos: (0.0, 0.0),
            mouse_left_pressed: false,
            mouse_dragged: false,
            scroll_pixel_accum: 0.0,
            click_count: 0,
            last_click_time: Instant::now(),
            last_click_cell: (usize::MAX, usize::MAX),
            dragging_separator: None,
            cursor_blink_on: true,
            cursor_last_blink: Instant::now(),
            pane_ratio_adjusted: false,
            toggle_sidebar_requested: false,
            resize_mode: false,
            input_echo: String::new(),
            last_key_instant: None,
            leader_prefix: None,
        }
    }

    /// Update click count for multi-click detection. Returns 1 / 2 / 3+ based on timing and position.
    pub fn register_click(&mut self, cell: (usize, usize)) -> u32 {
        const DOUBLE_CLICK_MS: u128 = 500;
        let same_cell = self.last_click_cell == cell;
        let within_time = self.last_click_time.elapsed().as_millis() < DOUBLE_CLICK_MS;
        self.click_count = if same_cell && within_time {
            (self.click_count + 1).min(3)
        } else {
            1
        };
        self.last_click_time = Instant::now();
        self.last_click_cell = cell;
        self.click_count
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

    pub fn pixel_to_cell(
        &self,
        x: f64,
        y: f64,
        config: &Config,
        render_ctx: &Option<RenderContext>,
        mux: &Mux,
    ) -> (usize, usize) {
        let pad = &config.window.padding;
        let (cw, ch) = render_ctx
            .as_ref()
            .map(|rc| (rc.shaper.cell_width as f64, rc.shaper.cell_height as f64))
            .unwrap_or((8.0, 16.0));
        // Offset y origin past the tab bar row when it is visible (2+ tabs).
        let tab_h = if mux.tabs.tab_count() > 1 { ch } else { 0.0 };
        let col = ((x - pad.left as f64) / cw).floor().max(0.0) as usize;
        let row = ((y - (pad.top as f64 + tab_h)) / ch).floor().max(0.0) as usize;
        let (term_cols, term_rows) = mux.active_terminal_size();
        (
            col.min(term_cols.saturating_sub(1)),
            row.min(term_rows.saturating_sub(1)),
        )
    }

    pub fn send_mouse_report(&self, button: u8, col: usize, row: usize, pressed: bool, mux: &Mux) {
        let Some(terminal) = mux.active_terminal() else {
            return;
        };
        let (any_mouse, sgr, _) = terminal.mouse_mode_flags();
        if !any_mouse {
            return;
        }
        if sgr {
            let c = if pressed { 'M' } else { 'm' };
            terminal.write_input(format!("\x1b[<{button};{};{}{c}", col + 1, row + 1).as_bytes());
        } else if pressed {
            let b = button.saturating_add(32);
            let x = ((col + 1) as u8).saturating_add(32);
            let y = ((row + 1) as u8).saturating_add(32);
            terminal.write_input(&[0x1b, b'[', b'M', b, x, y]);
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn handle_key_input(
        &mut self,
        event: &KeyEvent,
        event_loop: &ActiveEventLoop,
        config: &mut Config,
        mux: &mut Mux,
        ui: &mut UiManager,
        render_ctx: &mut Option<RenderContext>,
        window: Option<&winit::window::Window>,
        wakeup_proxy: winit::event_loop::EventLoopProxy<()>,
    ) {
        if event.state != ElementState::Pressed {
            return;
        }
        self.cursor_blink_on = true;
        self.cursor_last_blink = Instant::now();

        // Close the context menu on any key press.
        if ui.context_menu.visible {
            ui.context_menu.close();
        }

        if self.leader_active {
            if let Some(deadline) = self.leader_deadline {
                if Instant::now() >= deadline {
                    self.leader_active = false;
                    self.leader_deadline = None;
                    self.leader_prefix = None;
                }
            }
        }

        let cmd = self.modifiers.state().super_key();
        let ctrl = self.modifiers.state().control_key();
        let shift = self.modifiers.state().shift_key();

        // ── Pane resize mode — hold Option + press arrows to resize ──────────
        // Activated by <leader>+Option+Arrow. Active while Option is held;
        // ModifiersChanged clears resize_mode when Option is released.
        if self.resize_mode && self.modifiers.state().alt_key() {
            use crate::ui::panes::FocusDir;
            let dir_opt = match &event.logical_key {
                Key::Named(NamedKey::ArrowLeft) => Some(FocusDir::Left),
                Key::Named(NamedKey::ArrowRight) => Some(FocusDir::Right),
                Key::Named(NamedKey::ArrowUp) => Some(FocusDir::Up),
                Key::Named(NamedKey::ArrowDown) => Some(FocusDir::Down),
                _ => match &event.physical_key {
                    PhysicalKey::Code(KeyCode::ArrowLeft) => Some(FocusDir::Left),
                    PhysicalKey::Code(KeyCode::ArrowRight) => Some(FocusDir::Right),
                    PhysicalKey::Code(KeyCode::ArrowUp) => Some(FocusDir::Up),
                    PhysicalKey::Code(KeyCode::ArrowDown) => Some(FocusDir::Down),
                    _ => None,
                },
            };
            if let Some(dir) = dir_opt {
                mux.cmd_adjust_pane_ratio(dir, 0.05);
                self.pane_ratio_adjusted = true;
                return;
            }
        }

        // ── Tab rename prompt ────────────────────────────────────────────────
        if ui.is_renaming_tab() {
            match &event.logical_key {
                Key::Named(NamedKey::Escape) => {
                    ui.tab_rename_cancel();
                }
                Key::Named(NamedKey::Enter) => {
                    ui.tab_rename_confirm(mux);
                }
                Key::Named(NamedKey::Backspace) => {
                    ui.tab_rename_backspace();
                }
                Key::Named(NamedKey::Space) => {
                    ui.tab_rename_type(' ');
                }
                Key::Character(s) if !cmd && !ctrl => {
                    for ch in s.chars() {
                        ui.tab_rename_type(ch);
                    }
                }
                _ => {}
            }
            return;
        }

        // ── Workspace rename prompt ──────────────────────────────────────────
        if ui.is_renaming_workspace() {
            match &event.logical_key {
                Key::Named(NamedKey::Escape) => {
                    ui.workspace_rename_cancel();
                }
                Key::Named(NamedKey::Enter) => {
                    ui.workspace_rename_confirm(mux);
                }
                Key::Named(NamedKey::Backspace) => {
                    ui.workspace_rename_backspace();
                }
                Key::Named(NamedKey::Space) => {
                    ui.workspace_rename_type(' ');
                }
                Key::Character(s) if !cmd && !ctrl => {
                    for ch in s.chars() {
                        ui.workspace_rename_type(ch);
                    }
                }
                _ => {}
            }
            return;
        }

        // ── Leader key activation — checked BEFORE panel/palette handlers so that
        // Ctrl+B always activates the leader even when the AI panel is focused.
        if ctrl && !shift && !cmd {
            if let Key::Character(s) = &event.logical_key {
                if s.as_str() == config.leader.key.as_str() {
                    self.leader_active = true;
                    self.leader_deadline = Some(
                        Instant::now() + std::time::Duration::from_millis(self.leader_timeout_ms),
                    );
                    return;
                }
            }
        }

        // ── Leader key dispatch ───────────────────────────────────────────────
        if self.leader_active {
            // Modifier key presses (Shift, Ctrl, Alt, Super) must not consume the
            // leader — otherwise pressing e.g. ^B % (which requires Shift) would
            // have the Shift keydown event silently discard the pending leader.
            if matches!(
                &event.logical_key,
                Key::Named(
                    NamedKey::Shift
                        | NamedKey::Alt
                        | NamedKey::Control
                        | NamedKey::Super
                        | NamedKey::Meta
                        | NamedKey::Hyper
                )
            ) {
                return;
            }
            self.leader_active = false;
            self.leader_deadline = None;
            let leader_prefix = self.leader_prefix.take();

            // <leader> + Option/Alt + ←→↑↓ → resize pane (TD-042).
            // On macOS, Option+Arrow may arrive as Key::Character (OS word-nav transform),
            // so fall back to physical_key when logical_key is not a Named arrow (TD-045).
            let alt = self.modifiers.state().alt_key();
            if alt {
                use crate::ui::panes::FocusDir;
                let dir_opt = match &event.logical_key {
                    Key::Named(named) => match named {
                        NamedKey::ArrowLeft => Some(FocusDir::Left),
                        NamedKey::ArrowRight => Some(FocusDir::Right),
                        NamedKey::ArrowUp => Some(FocusDir::Up),
                        NamedKey::ArrowDown => Some(FocusDir::Down),
                        _ => None,
                    },
                    _ => match &event.physical_key {
                        PhysicalKey::Code(KeyCode::ArrowLeft) => Some(FocusDir::Left),
                        PhysicalKey::Code(KeyCode::ArrowRight) => Some(FocusDir::Right),
                        PhysicalKey::Code(KeyCode::ArrowUp) => Some(FocusDir::Up),
                        PhysicalKey::Code(KeyCode::ArrowDown) => Some(FocusDir::Down),
                        _ => None,
                    },
                };
                if let Some(dir) = dir_opt {
                    mux.cmd_adjust_pane_ratio(dir, 0.05);
                    self.pane_ratio_adjusted = true;
                    self.resize_mode = true; // stay in resize mode for subsequent arrows
                    return;
                }
            }

            if let Key::Character(s) = &event.logical_key {
                // ── Sub-leader dispatch ───────────────────────────────────────
                match leader_prefix {
                    // 'a' prefix → AI actions.
                    Some('a') => {
                        let action = match s.as_str() {
                            "a" => Some(Action::FocusAiPanel),
                            "e" => Some(Action::ExplainLastOutput),
                            "f" => Some(Action::FixLastError),
                            "z" => Some(Action::UndoLastWrite),
                            _ => None,
                        };
                        if let Some(action) = action {
                            if let Some(rc) = render_ctx.as_mut() {
                                ui.handle_palette_action(
                                    action,
                                    mux,
                                    rc,
                                    config,
                                    window,
                                    wakeup_proxy,
                                );
                            }
                        }
                        return;
                    }
                    // 'e' prefix → explorer / sidebar.
                    Some('e') => {
                        if s.as_str() == "e" {
                            self.toggle_sidebar_requested = true;
                        }
                        return;
                    }
                    // 'W' prefix → workspace actions.
                    Some('W') => {
                        let action = match s.as_str() {
                            "n" => Some(Action::NewWorkspace),
                            "&" => Some(Action::CloseWorkspace),
                            "," => Some(Action::RenameWorkspace),
                            "j" => Some(Action::NextWorkspace),
                            "k" => Some(Action::PrevWorkspace),
                            _ => None,
                        };
                        if let Some(action) = action {
                            if let Some(rc) = render_ctx.as_mut() {
                                ui.handle_palette_action(
                                    action,
                                    mux,
                                    rc,
                                    config,
                                    window,
                                    wakeup_proxy,
                                );
                            }
                        }
                        return;
                    }
                    _ => {}
                }

                // ── Single-key leader dispatch ────────────────────────────────
                // Enter prefix mode for 'a' (AI) and 'e' (explorer).
                match s.as_str() {
                    "a" | "e" | "W" => {
                        let prefix = s.chars().next().unwrap();
                        self.leader_prefix = Some(prefix);
                        self.leader_active = true;
                        self.leader_deadline = Some(
                            Instant::now()
                                + std::time::Duration::from_millis(self.leader_timeout_ms),
                        );
                        return;
                    }
                    _ => {}
                }
                // Leader + 1-9: select tab by index (hardcoded, like Cmd+1-9)
                if let Ok(n) = s.parse::<usize>() {
                    if (1..=9).contains(&n) {
                        mux.tabs.switch_to_index(n - 1);
                        return;
                    }
                }
                let key = s.to_ascii_lowercase();
                let action = self
                    .leader_map
                    .get(s.as_str())
                    .or_else(|| self.leader_map.get(key.as_str()))
                    .cloned();
                if let Some(action) = action {
                    if action == Action::Quit {
                        event_loop.exit();
                        return;
                    }
                    if let Some(rc) = render_ctx.as_mut() {
                        ui.handle_palette_action(action, mux, rc, config, window, wakeup_proxy);
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
                Key::Named(NamedKey::Escape) => {
                    ui.ai_block.close();
                }
                Key::Named(NamedKey::Enter) => {
                    if ui.ai_block.is_typing() {
                        ui.submit_ai_block_query(wakeup_proxy);
                    } else if ui.ai_block.is_done() {
                        ui.run_ai_block_command(mux);
                    }
                }
                Key::Named(NamedKey::Backspace) => ui.ai_block.backspace(),
                Key::Named(NamedKey::Space) => ui.ai_block.type_char(' '),
                Key::Character(s) => {
                    for ch in s.chars() {
                        ui.ai_block.type_char(ch);
                    }
                }
                _ => {}
            }
            return;
        }

        // ── Chat panel input ─────────────────────────────────────────────────
        if ui.is_panel_visible() && ui.panel_focused && !cmd {
            // ── Confirmation prompt mode ──────────────────────────────────────
            if matches!(
                ui.panel().state,
                crate::llm::chat_panel::PanelState::AwaitingConfirm
            ) {
                match &event.logical_key {
                    Key::Character(s) if s.as_str() == "y" => ui.confirm_yes(),
                    Key::Named(NamedKey::Enter) => ui.confirm_yes(),
                    Key::Character(s) if s.as_str() == "n" => ui.confirm_no(),
                    Key::Named(NamedKey::Escape) => ui.confirm_no(),
                    _ => {}
                }
                return;
            }

            // ── File picker mode ──────────────────────────────────────────────
            if ui.file_picker_focused {
                match &event.logical_key {
                    Key::Named(NamedKey::Escape) | Key::Named(NamedKey::Tab) => {
                        ui.panel_mut().close_file_picker();
                        ui.file_picker_focused = false;
                    }
                    Key::Named(NamedKey::Enter) => {
                        let cwd = mux
                            .active_cwd()
                            .or_else(|| std::env::current_dir().ok())
                            .unwrap_or_default();
                        let filtered: Vec<std::path::PathBuf> = ui
                            .panel()
                            .filtered_picker_items()
                            .into_iter()
                            .cloned()
                            .collect();
                        ui.panel_mut().picker_confirm(&cwd, &filtered);
                    }
                    Key::Named(NamedKey::ArrowUp) => ui.panel_mut().picker_move_up(),
                    Key::Named(NamedKey::ArrowDown) => {
                        let len = ui.panel().filtered_picker_items().len();
                        ui.panel_mut().picker_move_down(len);
                    }
                    Key::Named(NamedKey::Backspace) => ui.panel_mut().picker_backspace(),
                    Key::Named(NamedKey::Space) => ui.panel_mut().picker_type_char(' '),
                    Key::Character(s) => {
                        for ch in s.chars() {
                            ui.panel_mut().picker_type_char(ch);
                        }
                    }
                    _ => {}
                }
                return;
            }

            // ── Chat input mode ───────────────────────────────────────────────
            match &event.logical_key {
                Key::Named(NamedKey::Escape) => {
                    if matches!(ui.panel().state, PanelState::Error(_)) {
                        ui.panel_mut().dismiss_error();
                    } else {
                        // Escape unfocuses the panel (returns to terminal) without closing it.
                        // Use leader+a or /q to close.
                        ui.panel_focused = false;
                        ui.file_picker_focused = false;
                    }
                }
                Key::Named(NamedKey::Enter) => {
                    if shift {
                        ui.panel_mut().type_char('\n');
                    } else if ui.panel().is_idle() {
                        let input = ui.panel().input.trim().to_string();
                        match input.as_str() {
                            "/q" | "/quit" => {
                                ui.panel_mut().close();
                                ui.panel_focused = false;
                                ui.file_picker_focused = false;
                            }
                            "" => {
                                ui.chat_panel_run_command(mux);
                            }
                            _ => {
                                let cwd = mux
                                    .active_cwd()
                                    .or_else(|| std::env::current_dir().ok())
                                    .unwrap_or_default();
                                ui.submit_ai_query(wakeup_proxy, cwd);
                            }
                        }
                    }
                }
                Key::Named(NamedKey::Tab) => {
                    // Open file picker — CWD from the active terminal's shell process.
                    let cwd = mux
                        .active_cwd()
                        .or_else(|| std::env::current_dir().ok())
                        .unwrap_or_default();
                    ui.open_file_picker_async(cwd);
                    ui.file_picker_focused = true;
                }
                Key::Named(NamedKey::Backspace) => ui.panel_mut().backspace(),
                Key::Named(NamedKey::Space) => ui.panel_mut().type_char(' '),
                Key::Character(s)
                    if ctrl
                        && s.as_str() == "s"
                        && ui.panel().is_idle()
                        && !ui.panel().input.trim().is_empty() =>
                {
                    // Ctrl+S: submit query (alternative to Enter).
                    let cwd = mux
                        .active_cwd()
                        .or_else(|| std::env::current_dir().ok())
                        .unwrap_or_default();
                    ui.submit_ai_query(wakeup_proxy, cwd);
                }
                Key::Character(s) => {
                    for ch in s.chars() {
                        ui.panel_mut().type_char(ch);
                    }
                }
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
                        ui.handle_palette_action(action, mux, rc, config, window, wakeup_proxy);
                    }
                }
                Key::Named(NamedKey::ArrowUp) => ui.palette.select_up(),
                Key::Named(NamedKey::ArrowDown) => ui.palette.select_down(),
                Key::Named(NamedKey::Backspace) => ui.palette.backspace(),
                Key::Character(s) => {
                    for ch in s.chars() {
                        ui.palette.type_char(ch);
                    }
                }
                _ => {}
            }
            return;
        }

        // ── Cmd+F — open search bar ──────────────────────────────────────────────
        if cmd && !shift && !ctrl {
            if let Key::Character(s) = &event.logical_key {
                if s.as_str() == "f" {
                    if ui.search_bar.visible {
                        ui.search_bar.close();
                    } else {
                        ui.search_bar.open();
                    }
                    return;
                }
            }
        }

        // ── Search bar input ─────────────────────────────────────────────────
        if ui.search_bar.visible && !cmd {
            match &event.logical_key {
                Key::Named(NamedKey::Escape) => {
                    ui.search_bar.close();
                }
                Key::Named(NamedKey::Enter) => {
                    if shift {
                        ui.search_bar.prev_match();
                    } else {
                        ui.search_bar.next_match();
                    }
                }
                Key::Named(NamedKey::Backspace) => ui.search_bar.backspace(),
                Key::Named(NamedKey::Space) => ui.search_bar.type_char(' '),
                Key::Character(s) => {
                    for ch in s.chars() {
                        ui.search_bar.type_char(ch);
                    }
                }
                _ => {}
            }
            return;
        }

        // ── F12 — toggle debug HUD ───────────────────────────────────────────
        if let Key::Named(NamedKey::F12) = &event.logical_key {
            if let Some(rc) = render_ctx.as_mut() {
                rc.hud_visible = !rc.hud_visible;
            }
            return;
        }

        if cmd && !shift && !ctrl {
            if let Key::Character(s) = &event.logical_key {
                match s.as_str() {
                    // System clipboard — always Cmd+C / Cmd+V, not configurable via leader.
                    "q" => {
                        event_loop.exit();
                        return;
                    }
                    "k" => {
                        if let Some(terminal) = mux.active_terminal() {
                            // Clear screen and scrollback, move cursor home.
                            terminal.write_input(b"\x1b[H\x1b[2J\x1b[3J");
                        }
                        return;
                    }
                    "c" => {
                        if let Some(terminal) = mux.active_terminal() {
                            if let Some(text) = terminal.selection_text() {
                                std::thread::spawn(move || {
                                    let _ = arboard::Clipboard::new()
                                        .and_then(|mut cb| cb.set_text(text));
                                });
                            }
                        }
                        return;
                    }
                    "v" => {
                        ui.request_paste_async(wakeup_proxy.clone());
                        return;
                    }
                    // Cmd+1-9: switch tab by index (standard macOS pattern).
                    _ => {
                        if let Ok(n) = s.parse::<usize>() {
                            if (1..=9).contains(&n) {
                                mux.tabs.switch_to_index(n - 1);
                                return;
                            }
                        }
                    }
                }
            }
        }

        // Tab: try snippet trigger expansion first; fall through to PTY on no match.
        if event.logical_key == Key::Named(NamedKey::Tab)
            && !self.modifiers.state().shift_key()
            && !self.modifiers.state().control_key()
            && self.try_expand_snippet(config, mux)
        {
            return;
        }

        self.send_key_to_active_terminal(event, mux, config.keyboard.option_as_meta);
    }

    /// Try to expand a snippet trigger from `input_echo`. If the last contiguous
    /// non-whitespace word matches a trigger, write backspaces + body and return true.
    /// Returns false if no trigger matched (caller should send a regular Tab).
    fn try_expand_snippet(&mut self, config: &Config, mux: &Mux) -> bool {
        if config.snippets.iter().all(|s| s.trigger.is_none()) {
            return false;
        }
        // Extract the last word (non-space sequence) from the echo buffer.
        let word: &str = self
            .input_echo
            .trim_end()
            .rsplit(|c: char| c.is_whitespace())
            .next()
            .unwrap_or("");
        if word.is_empty() {
            return false;
        }

        if let Some(snippet) = config
            .snippets
            .iter()
            .find(|s| s.trigger.as_deref() == Some(word))
        {
            if let Some(terminal) = mux.active_terminal() {
                // Erase the trigger word with backspaces.
                let backspaces = vec![0x7fu8; word.len()];
                terminal.scroll_to_bottom();
                terminal.write_input(&backspaces);
                terminal.write_input(snippet.body.as_bytes());
                // Clear the word from the echo buffer.
                let trim_len = self.input_echo.trim_end().len();
                let new_len = trim_len.saturating_sub(word.len());
                self.input_echo.truncate(new_len);
            }
            return true;
        }
        false
    }

    pub fn send_key_to_active_terminal(
        &mut self,
        event: &KeyEvent,
        mux: &Mux,
        option_as_meta: bool,
    ) {
        let mode = mux
            .active_terminal()
            .map(|t| *t.term.lock().mode())
            .unwrap_or(TermMode::empty());

        if let Some(data) =
            key_map::translate_key(&event.logical_key, self.modifiers, mode, option_as_meta)
        {
            // Update the echo buffer for snippet trigger detection.
            match &event.logical_key {
                Key::Named(NamedKey::Enter) | Key::Named(NamedKey::Escape) => {
                    self.input_echo.clear();
                }
                Key::Named(NamedKey::Backspace) => {
                    self.input_echo.pop();
                }
                // Only track printable chars; reset on Ctrl sequences (data != s.as_bytes()).
                Key::Character(s) if data == s.as_bytes() => {
                    self.input_echo.push_str(s);
                    // Cap buffer to avoid unbounded growth.
                    if self.input_echo.len() > 256 {
                        let keep = self.input_echo.len() - 256;
                        self.input_echo.drain(..keep);
                    }
                }
                _ => {}
            }

            if let Some(terminal) = mux.active_terminal() {
                terminal.scroll_to_bottom();
                self.last_key_instant = Some(std::time::Instant::now());
                terminal.write_input(&data);
                terminal.clear_selection();
            }
        }
    }
}
