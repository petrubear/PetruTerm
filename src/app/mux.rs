use anyhow::Result;
use crate::config::Config;
use crate::term::{Terminal, PtyEvent};
use crate::ui::{PaneManager, TabManager, Rect};
use winit::event_loop::EventLoopProxy;
use alacritty_terminal::vte::ansi::Color as AnsiColor;
use alacritty_terminal::index::{Column, Line, Point};
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::selection::{SelectionRange, SelectionType};

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

    pub fn focused_terminal_id(&self) -> usize {
        let idx = self.active_tab_index();
        self.panes.get(idx).map(|p| p.focused_terminal).unwrap_or(0)
    }

    pub fn active_terminal(&self) -> Option<&Terminal> {
        let tid = self.focused_terminal_id();
        self.terminals.get(tid)?.as_ref()
    }

    pub fn active_terminal_mut(&mut self) -> Option<&mut Terminal> {
        let tid = self.focused_terminal_id();
        self.terminals.get_mut(tid)?.as_mut()
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
    ) -> Result<usize> {
        let terminal = Terminal::new(config, cols, rows, cell_w, cell_h, wakeup_proxy)?;
        let id = self.next_terminal_id;
        self.next_terminal_id += 1;

        if self.terminals.len() <= id {
            self.terminals.resize_with(id + 1, || None);
        }
        self.terminals[id] = Some(terminal);
        Ok(id)
    }

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
        let terminal_id = self.open_terminal(config, cols, rows, cell_w, cell_h, wakeup_proxy)?;
        let pane_mgr = PaneManager::new(viewport);
        self.panes.push(pane_mgr);
        log::info!("Opened initial tab {tab_id}, terminal {terminal_id}");
        Ok(())
    }

    pub fn poll_pty_events(&mut self) -> (bool, bool) {
        let mut has_data = false;
        let mut shell_exited = false;
        for terminal in self.terminals.iter_mut().flatten() {
            loop {
                use crossbeam_channel::TryRecvError;
                match terminal.pty.rx.try_recv() {
                    Ok(event) => match event {
                        PtyEvent::DataReady => { has_data = true; }
                        PtyEvent::TitleChanged(t) => { log::debug!("PTY title: {t}"); }
                        PtyEvent::Exit => { log::info!("PTY shell exited."); shell_exited = true; }
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
        (has_data, shell_exited)
    }

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

    pub fn cmd_new_tab(&mut self, config: &Config, viewport: Rect, cols: u16, rows: u16, cell_w: u16, cell_h: u16, wakeup_proxy: EventLoopProxy<()>) {
        let _tab_id = self.tabs.new_tab("zsh");
        if self.open_terminal(config, cols, rows, cell_w, cell_h, wakeup_proxy).is_ok() {
            self.panes.push(PaneManager::new(viewport));
        }
    }

    pub fn cmd_close_tab(&mut self) {
        if let Some(tab) = self.tabs.active_tab() {
            self.tabs.close_tab(tab.id);
        }
    }

    pub fn cmd_split(&mut self, config: &Config, dir: crate::ui::SplitDir, cols: u16, rows: u16, cell_w: u16, cell_h: u16, wakeup_proxy: EventLoopProxy<()>) {
        let active = self.tabs.active_index();
        if let Some(pane_mgr) = self.panes.get_mut(active) {
            let new_id = pane_mgr.split(dir);
            if let Ok(terminal) = Terminal::new(config, cols, rows, cell_w, cell_h, wakeup_proxy) {
                if self.terminals.len() <= new_id { self.terminals.resize_with(new_id + 1, || None); }
                self.terminals[new_id] = Some(terminal);
            }
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

    /// Resize all panes and terminals in response to a window resize or panel open/close.
    /// Encapsulates iteration over `panes` and `terminals` so callers don't reach into internals.
    pub fn resize_all(
        &mut self,
        viewport: Rect,
        cols: u16,
        rows: u16,
        scrollback: usize,
        cell_w: u16,
        cell_h: u16,
    ) {
        for pane_mgr in &mut self.panes {
            pane_mgr.resize(viewport);
        }
        for terminal in self.terminals.iter_mut().flatten() {
            terminal.resize(cols, rows, scrollback, cell_w, cell_h);
        }
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
