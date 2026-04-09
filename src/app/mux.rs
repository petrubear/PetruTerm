use anyhow::Result;
use crate::config::Config;
use crate::term::{Terminal, PtyEvent};
use crate::ui::{PaneInfo, PaneSeparator, PaneManager, TabManager, Rect};
use winit::event_loop::EventLoopProxy;
use alacritty_terminal::vte::ansi::Color as AnsiColor;
use alacritty_terminal::index::{Column, Line, Point};
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::selection::SelectionRange;

/// Manages multiple terminal instances, tabs, and panes (Multiplexer).
pub struct Mux {
    pub tabs: TabManager,
    pub panes: Vec<PaneManager>,           // one PaneManager per tab
    pub terminals: Vec<Option<Terminal>>,  // indexed by terminal_id
    pub next_terminal_id: usize,
}

impl Mux {
    pub fn new() -> Self {
        Self {
            tabs: TabManager::new(),
            panes: Vec::new(),
            terminals: Vec::new(),
            next_terminal_id: 0,
        }
    }

    pub fn active_tab_index(&self) -> usize {
        self.tabs.active_index()
    }

    pub fn active_pane_count(&self) -> usize {
        let idx = self.active_tab_index();
        self.panes.get(idx).map(|p| p.root.leaf_ids().len()).unwrap_or(0)
    }

    pub fn focused_terminal_id(&self) -> usize {
        let idx = self.active_tab_index();
        self.panes.get(idx).map(|p| p.focused_terminal).unwrap_or(0)
    }

    pub fn active_terminal(&self) -> Option<&Terminal> {
        let tid = self.focused_terminal_id();
        self.terminals.get(tid)?.as_ref()
    }

    #[allow(dead_code)]
    pub fn active_terminal_mut(&mut self) -> Option<&mut Terminal> {
        let tid = self.focused_terminal_id();
        self.terminals.get_mut(tid)?.as_mut()
    }

    /// Returns the current working directory of the active terminal's shell process.
    /// Uses OS proc APIs (macOS: proc_pidinfo; Linux: /proc/pid/cwd).
    pub fn active_cwd(&self) -> Option<std::path::PathBuf> {
        let pid = self.active_terminal()?.child_pid;
        crate::term::process_cwd(pid)
    }

    pub fn active_terminal_size(&self) -> (usize, usize) {
        if let Some(t) = self.active_terminal() {
            return (t.cols as usize, t.rows as usize);
        }
        (80, 24)
    }

    pub fn open_terminal(
        &mut self,
        config: &Config,
        cols: u16,
        rows: u16,
        cell_w: u16,
        cell_h: u16,
        wakeup_proxy: EventLoopProxy<()>,
        working_directory: Option<std::path::PathBuf>,
    ) -> Result<usize> {
        let terminal = Terminal::new(config, cols, rows, cell_w, cell_h, wakeup_proxy, working_directory)?;
        let id = self.next_terminal_id;
        self.next_terminal_id += 1;

        if self.terminals.len() <= id {
            self.terminals.resize_with(id + 1, || None);
        }
        self.terminals[id] = Some(terminal);
        Ok(id)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn open_initial_tab(
        &mut self,
        config: &Config,
        viewport: Rect,
        cols: u16,
        rows: u16,
        cell_w: u16,
        cell_h: u16,
        wakeup_proxy: EventLoopProxy<()>,
    ) -> Result<()> {
        let tab_id = self.tabs.new_tab("zsh");
        let terminal_id = self.open_terminal(config, cols, rows, cell_w, cell_h, wakeup_proxy, None)?;
        self.panes.push(PaneManager::new(viewport, terminal_id));
        log::info!("Opened initial tab {tab_id}, terminal {terminal_id}");
        Ok(())
    }

    /// Poll PTY events for all terminals.
    /// Returns `(has_data, exited_terminal_ids)`.
    pub fn poll_pty_events(&mut self) -> (bool, Vec<usize>) {
        let mut has_data = false;
        let mut exited: Vec<usize> = Vec::new();
        for (id, terminal_slot) in self.terminals.iter_mut().enumerate() {
            let Some(terminal) = terminal_slot else { continue };
            loop {
                use crossbeam_channel::TryRecvError;
                match terminal.pty.rx.try_recv() {
                    Ok(event) => match event {
                        PtyEvent::DataReady => { has_data = true; }
                        PtyEvent::TitleChanged(t) => { log::debug!("PTY title: {t}"); }
                        PtyEvent::Exit => { log::info!("PTY shell exited (terminal {id})."); exited.push(id); }
                        PtyEvent::Bell => {}
                        PtyEvent::ClipboardStore(text) => { if let Ok(mut cb) = arboard::Clipboard::new() { let _ = cb.set_text(text); } }
                        PtyEvent::ClipboardLoad(fmt) => {
                            let text = arboard::Clipboard::new().ok().and_then(|mut cb| cb.get_text().ok()).unwrap_or_default();
                            terminal.write_input(fmt(&text).as_bytes());
                        }
                        PtyEvent::PtyWrite(text) => { terminal.write_input(text.as_bytes()); }
                    },
                    Err(TryRecvError::Disconnected) => { log::warn!("PTY channel disconnected."); break; }
                    Err(TryRecvError::Empty) => break,
                }
            }
        }
        (has_data, exited)
    }

    /// Handle a terminal exit: close just the pane if multiple panes exist in the tab,
    /// or close the whole tab if it was the last pane.
    /// Returns `true` if no tabs remain (caller should exit the app).
    pub fn close_terminal(&mut self, terminal_id: usize) -> bool {
        // Find the tab by searching leaf IDs (not focused_terminal, which may differ).
        let tab_idx = self.panes.iter().position(|p| p.root.leaf_ids().contains(&terminal_id));
        if let Some(tab_idx) = tab_idx {
            let has_other_panes = self.panes[tab_idx].root.leaf_ids().len() > 1;
            if has_other_panes {
                // Multiple panes: remove only the exited pane, keep the tab alive.
                self.panes[tab_idx].close_specific(terminal_id);
            } else {
                // Last pane in this tab: close the whole tab.
                if let Some(tab) = self.tabs.tabs().get(tab_idx) {
                    let tab_id = tab.id;
                    self.tabs.close_tab(tab_id);
                }
                self.panes.remove(tab_idx);
            }
            if let Some(slot) = self.terminals.get_mut(terminal_id) { *slot = None; }
        }
        self.tabs.is_empty()
    }

    #[allow(dead_code)]
    pub fn collect_grid_cells(&self) -> Vec<(String, Vec<(AnsiColor, AnsiColor)>)> {
        let Some(terminal) = self.active_terminal() else { return vec![]; };

        terminal.with_term(|term| {
            let rows = term.screen_lines();
            let cols = term.columns();
            let display_offset = term.grid().display_offset() as i32;
            let sel_range = term.selection.as_ref().and_then(|s| s.to_range(term));
            let mut result = Vec::with_capacity(rows);

            for row in 0..rows {
                let mut text = String::with_capacity(cols);
                let mut colors = Vec::with_capacity(cols);
                let grid_line = Line(row as i32 - display_offset);

                for col in 0..cols {
                    let cell = &term.grid()[grid_line][Column(col)];
                    text.push(if cell.c == '\0' { ' ' } else { cell.c });
                    let (fg, bg) = if cell.flags.contains(Flags::INVERSE) { (cell.bg, cell.fg) } else { (cell.fg, cell.bg) };
                    let (fg, bg) = if cell_in_selection(grid_line, Column(col), &sel_range) { (bg, fg) } else { (fg, bg) };
                    colors.push((fg, bg));
                }
                result.push((text, colors));
            }
            result
        })
    }

    pub fn last_terminal_lines(&self, n: usize) -> String {
        let Some(terminal) = self.active_terminal() else { return String::new(); };

        terminal.with_term(|term| {
            let rows = term.screen_lines();
            let cols = term.columns();
            let start = rows.saturating_sub(n);
            let mut lines = Vec::new();
            for row in start..rows {
                let mut text = String::new();
                for col in 0..cols {
                    let cell = &term.grid()[alacritty_terminal::index::Line(row as i32)][alacritty_terminal::index::Column(col)];
                    text.push(if cell.c == '\0' { ' ' } else { cell.c });
                }
                lines.push(text.trim_end().to_string());
            }
            lines.join("\n")
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub fn cmd_new_tab(&mut self, config: &Config, viewport: Rect, cols: u16, rows: u16, cell_w: u16, cell_h: u16, wakeup_proxy: EventLoopProxy<()>, working_directory: Option<std::path::PathBuf>) {
        match self.open_terminal(config, cols, rows, cell_w, cell_h, wakeup_proxy, working_directory) {
            Ok(terminal_id) => {
                self.tabs.new_tab("zsh");
                self.panes.push(PaneManager::new(viewport, terminal_id));
            }
            Err(e) => log::error!("Failed to open terminal for new tab: {e}"),
        }
    }

    /// TD-017: Close the active tab and clean up its pane tree and all owned terminals.
    pub fn cmd_close_tab(&mut self) {
        let active = self.tabs.active_index();
        if let Some(tab) = self.tabs.active_tab() {
            self.tabs.close_tab(tab.id);
        }
        if active < self.panes.len() {
            for tid in self.panes[active].root.leaf_ids() {
                if let Some(slot) = self.terminals.get_mut(tid) { *slot = None; }
            }
            self.panes.remove(active);
        }
    }

    /// TD-018: Create the terminal first; only mutate the pane tree on success.
    #[allow(clippy::too_many_arguments)]
    pub fn cmd_split(&mut self, config: &Config, dir: crate::ui::SplitDir, cols: u16, rows: u16, cell_w: u16, cell_h: u16, wakeup_proxy: EventLoopProxy<()>, working_directory: Option<std::path::PathBuf>) {
        match Terminal::new(config, cols, rows, cell_w, cell_h, wakeup_proxy, working_directory) {
            Ok(terminal) => {
                let new_id = self.next_terminal_id;
                self.next_terminal_id += 1;
                if self.terminals.len() <= new_id { self.terminals.resize_with(new_id + 1, || None); }
                self.terminals[new_id] = Some(terminal);
                let active = self.tabs.active_index();
                if let Some(pane_mgr) = self.panes.get_mut(active) {
                    pane_mgr.split(dir, new_id);
                }
            }
            Err(e) => log::error!("Failed to create terminal for split: {e}"),
        }
    }

    pub fn cmd_close_pane(&mut self) {
        let active = self.tabs.active_index();
        if let Some(pane_mgr) = self.panes.get_mut(active) {
            if let Some(closed_id) = pane_mgr.close_focused() {
                if let Some(slot) = self.terminals.get_mut(closed_id) { *slot = None; }
            }
        }
    }

    pub fn cmd_focus_pane_dir(&mut self, dir: crate::ui::panes::FocusDir) {
        let active = self.tabs.active_index();
        if let Some(pane_mgr) = self.panes.get_mut(active) {
            pane_mgr.focus_dir(dir);
        }
    }

    /// Resize the focused pane by moving its nearest ancestor separator `delta` in `dir`.
    pub fn cmd_adjust_pane_ratio(&mut self, dir: crate::ui::panes::FocusDir, delta: f32) {
        let active  = self.tabs.active_index();
        let focused = self.focused_terminal_id();
        if let Some(pane_mgr) = self.panes.get_mut(active) {
            pane_mgr.adjust_ratio(focused, dir, delta);
        }
    }

    /// Drag the separator `(is_vert, sep_key)` to the current mouse position.
    #[allow(clippy::too_many_arguments)]
    pub fn cmd_drag_separator(&mut self, node_id: u32, mouse_x: f32, mouse_y: f32) {
        let active = self.tabs.active_index();
        if let Some(pane_mgr) = self.panes.get_mut(active) {
            pane_mgr.drag_separator(node_id, mouse_x, mouse_y);
        }
    }

    /// Resize all panes and terminals. The active tab's panes are resized to their
    /// individual rect-derived dimensions; inactive tabs keep their last layout.
    pub fn resize_all(
        &mut self,
        viewport: Rect,
        scrollback: usize,
        cell_w: u16,
        cell_h: u16,
    ) {
        // Relayout every pane tree to the new viewport.
        for pane_mgr in &mut self.panes {
            pane_mgr.resize(viewport);
        }
        // Active tab: resize each pane's terminal to its own pane dimensions.
        let active = self.active_tab_index();
        if let Some(pane_mgr) = self.panes.get(active) {
            let infos = pane_mgr.pane_infos(viewport, cell_w as f32, cell_h as f32);
            for info in infos {
                if let Some(Some(t)) = self.terminals.get_mut(info.terminal_id) {
                    t.resize(info.cols as u16, info.rows as u16, scrollback, cell_w, cell_h);
                }
            }
        }
        // Inactive tabs: terminals are resized when their tab becomes active.
    }

    /// Return layout info for each leaf pane in the active tab.
    pub fn active_pane_infos(&self, viewport: Rect, cell_w: f32, cell_h: f32) -> Vec<PaneInfo> {
        let tab_idx = self.active_tab_index();
        self.panes.get(tab_idx)
            .map(|p| p.pane_infos(viewport, cell_w, cell_h))
            .unwrap_or_default()
    }

    /// Return separator lines between panes in the active tab.
    pub fn active_pane_separators(&self, viewport: Rect, cell_w: f32, cell_h: f32) -> Vec<PaneSeparator> {
        let tab_idx = self.active_tab_index();
        self.panes.get(tab_idx)
            .map(|p| p.pane_separators(viewport, cell_w, cell_h))
            .unwrap_or_default()
    }

    /// Read the terminal grid for a specific terminal ID (for multi-pane rendering).
    pub fn collect_grid_cells_for(&self, terminal_id: usize) -> Vec<(String, Vec<(AnsiColor, AnsiColor)>)> {
        let Some(Some(terminal)) = self.terminals.get(terminal_id) else { return vec![]; };
        terminal.with_term(|term| {
            let rows = term.screen_lines();
            let cols = term.columns();
            let display_offset = term.grid().display_offset() as i32;
            let sel_range = term.selection.as_ref().and_then(|s| s.to_range(term));
            let mut result = Vec::with_capacity(rows);
            for row in 0..rows {
                let mut text = String::with_capacity(cols);
                let mut colors = Vec::with_capacity(cols);
                let grid_line = Line(row as i32 - display_offset);
                for col in 0..cols {
                    let cell = &term.grid()[grid_line][Column(col)];
                    text.push(if cell.c == '\0' { ' ' } else { cell.c });
                    let (fg, bg) = if cell.flags.contains(Flags::INVERSE) { (cell.bg, cell.fg) } else { (cell.fg, cell.bg) };
                    let (fg, bg) = if cell_in_selection(grid_line, Column(col), &sel_range) { (bg, fg) } else { (fg, bg) };
                    colors.push((fg, bg));
                }
                result.push((text, colors));
            }
            result
        })
    }

    pub fn shutdown(&mut self) {
        for terminal in self.terminals.iter_mut().flatten() {
            terminal.pty.shutdown();
        }
    }
}

fn cell_in_selection(line: Line, col: Column, sel_range: &Option<SelectionRange>) -> bool {
    let Some(range) = sel_range else { return false };
    if range.is_block { line >= range.start.line && line <= range.end.line && col >= range.start.column && col <= range.end.column }
    else { let pt = Point::new(line, col); pt >= range.start && pt <= range.end }
}
