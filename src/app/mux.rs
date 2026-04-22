use crate::config::Config;
use crate::term::{PtyEvent, Terminal};
use crate::ui::search_bar::SearchMatch;
use crate::ui::{PaneInfo, PaneManager, PaneSeparator, Rect, TabManager};

/// A named workspace that groups a set of tabs and panes.
pub struct Workspace {
    pub id: usize,
    pub name: String,
}

/// Archived tabs+panes for an inactive workspace (used during workspace switch).
struct WorkspaceData {
    id: usize,
    tabs: TabManager,
    panes: Vec<PaneManager>,
}
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Column, Line, Point};
use alacritty_terminal::selection::SelectionRange;
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::vte::ansi::{Color as AnsiColor, Rgb};
use anyhow::Result;
use winit::event_loop::EventLoopProxy;

/// Highlight colors injected into cell data for search matches.
const SEARCH_MATCH_FG: AnsiColor = AnsiColor::Spec(Rgb {
    r: 40,
    g: 42,
    b: 54,
}); // Dracula bg (dark)
const SEARCH_MATCH_BG: AnsiColor = AnsiColor::Spec(Rgb {
    r: 241,
    g: 250,
    b: 140,
}); // Dracula yellow
const SEARCH_CURRENT_BG: AnsiColor = AnsiColor::Spec(Rgb {
    r: 255,
    g: 184,
    b: 108,
}); // Dracula orange

/// Manages multiple terminal instances, tabs, panes, and workspaces.
pub struct Mux {
    /// Active workspace's tab manager (direct field — all existing callers unchanged).
    pub tabs: TabManager,
    /// Active workspace's pane managers — one per tab (direct field).
    pub panes: Vec<PaneManager>,
    pub terminals: Vec<Option<Terminal>>, // indexed by terminal_id
    pub next_terminal_id: usize,
    /// Terminal IDs closed by cmd_close_tab / cmd_close_pane (TD-MEM-08).
    /// App drains this after each input cycle to clean up per-terminal state.
    pub closed_ids: Vec<usize>,
    /// Ordered list of all workspaces (metadata only).
    pub workspaces: Vec<Workspace>,
    pub active_workspace_id: usize,
    next_workspace_id: usize,
    /// tabs+panes for inactive workspaces; restored on switch.
    inactive_workspaces: Vec<WorkspaceData>,
}

impl Mux {
    pub fn new() -> Self {
        Self {
            tabs: TabManager::new(),
            panes: Vec::new(),
            terminals: Vec::new(),
            next_terminal_id: 0,
            closed_ids: Vec::new(),
            workspaces: vec![Workspace { id: 0, name: "main".to_string() }],
            active_workspace_id: 0,
            next_workspace_id: 1,
            inactive_workspaces: Vec::new(),
        }
    }

    pub fn active_tab_index(&self) -> usize {
        self.tabs.active_index()
    }

    pub fn active_pane_count(&self) -> usize {
        let idx = self.active_tab_index();
        self.panes
            .get(idx)
            .map(|p| p.root.leaf_ids().len())
            .unwrap_or(0)
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

    #[allow(clippy::too_many_arguments)]
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
        let terminal = Terminal::new(
            config,
            cols,
            rows,
            cell_w,
            cell_h,
            wakeup_proxy,
            working_directory,
        )?;
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
        let terminal_id =
            self.open_terminal(config, cols, rows, cell_w, cell_h, wakeup_proxy, None)?;
        self.panes.push(PaneManager::new(viewport, terminal_id));
        log::info!("Opened initial tab {tab_id}, terminal {terminal_id}");
        Ok(())
    }

    /// Poll PTY events for all terminals.
    /// Returns `(terminals_with_data, exited_terminal_ids)`.
    /// `terminals_with_data` lists every terminal ID that received a `DataReady` event
    /// so callers can update per-terminal state (e.g. shell context) for the right pane.
    pub fn poll_pty_events(&mut self) -> (Vec<usize>, Vec<usize>) {
        let mut data_ids: Vec<usize> = Vec::new();
        let mut exited: Vec<usize> = Vec::new();
        for (id, terminal_slot) in self.terminals.iter_mut().enumerate() {
            let Some(terminal) = terminal_slot else {
                continue;
            };
            loop {
                use crossbeam_channel::TryRecvError;
                match terminal.pty.rx.try_recv() {
                    Ok(event) => match event {
                        PtyEvent::DataReady => {
                            if !data_ids.contains(&id) {
                                data_ids.push(id);
                            }
                        }
                        PtyEvent::TitleChanged(t) => {
                            log::debug!("PTY title: {t}");
                        }
                        PtyEvent::Exit => {
                            log::info!("PTY shell exited (terminal {id}).");
                            exited.push(id);
                        }
                        PtyEvent::Bell => {}
                        PtyEvent::ClipboardStore(text) => {
                            std::thread::spawn(move || {
                                let _ =
                                    arboard::Clipboard::new().and_then(|mut cb| cb.set_text(text));
                            });
                        }
                        PtyEvent::ClipboardLoad(fmt) => {
                            let tx = terminal.pty.tx.clone();
                            std::thread::spawn(move || {
                                let text = arboard::Clipboard::new()
                                    .ok()
                                    .and_then(|mut cb| cb.get_text().ok())
                                    .unwrap_or_default();
                                let _ = tx.send(PtyEvent::PtyWrite(fmt(&text)));
                            });
                        }
                        PtyEvent::PtyWrite(text) => {
                            terminal.write_input(text.as_bytes());
                        }
                    },
                    Err(TryRecvError::Disconnected) => {
                        log::warn!("PTY channel disconnected.");
                        break;
                    }
                    Err(TryRecvError::Empty) => break,
                }
            }
        }
        (data_ids, exited)
    }

    /// Handle a terminal exit: close just the pane if multiple panes exist in the tab,
    /// or close the whole tab if it was the last pane.
    /// Returns `true` if no tabs remain (caller should exit the app).
    pub fn close_terminal(&mut self, terminal_id: usize) -> bool {
        // Find the tab by searching leaf IDs (not focused_terminal, which may differ).
        let tab_idx = self
            .panes
            .iter()
            .position(|p| p.root.leaf_ids().contains(&terminal_id));
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
            if let Some(slot) = self.terminals.get_mut(terminal_id) {
                *slot = None;
            }
        }
        self.tabs.is_empty()
    }

    #[allow(dead_code)]
    pub fn collect_grid_cells(&self) -> Vec<(String, Vec<(AnsiColor, AnsiColor)>)> {
        let Some(terminal) = self.active_terminal() else {
            return vec![];
        };

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
                    let (fg, bg) = if cell.flags.contains(Flags::INVERSE) {
                        (cell.bg, cell.fg)
                    } else {
                        (cell.fg, cell.bg)
                    };
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

    pub fn last_terminal_lines(&self, n: usize) -> String {
        let Some(terminal) = self.active_terminal() else {
            return String::new();
        };

        terminal.with_term(|term| {
            let rows = term.screen_lines();
            let cols = term.columns();
            let start = rows.saturating_sub(n);
            let mut lines = Vec::new();
            for row in start..rows {
                let mut text = String::new();
                for col in 0..cols {
                    let cell = &term.grid()[alacritty_terminal::index::Line(row as i32)]
                        [alacritty_terminal::index::Column(col)];
                    text.push(if cell.c == '\0' { ' ' } else { cell.c });
                }
                lines.push(text.trim_end().to_string());
            }
            lines.join("\n")
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub fn cmd_new_tab(
        &mut self,
        config: &Config,
        viewport: Rect,
        cols: u16,
        rows: u16,
        cell_w: u16,
        cell_h: u16,
        wakeup_proxy: EventLoopProxy<()>,
        working_directory: Option<std::path::PathBuf>,
    ) {
        match self.open_terminal(
            config,
            cols,
            rows,
            cell_w,
            cell_h,
            wakeup_proxy,
            working_directory,
        ) {
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
                if let Some(slot) = self.terminals.get_mut(tid) {
                    *slot = None;
                }
                self.closed_ids.push(tid);
            }
            self.panes.remove(active);
        }
    }

    /// TD-018: Create the terminal first; only mutate the pane tree on success.
    #[allow(clippy::too_many_arguments)]
    pub fn cmd_split(
        &mut self,
        config: &Config,
        dir: crate::ui::SplitDir,
        cols: u16,
        rows: u16,
        cell_w: u16,
        cell_h: u16,
        wakeup_proxy: EventLoopProxy<()>,
        working_directory: Option<std::path::PathBuf>,
    ) {
        match Terminal::new(
            config,
            cols,
            rows,
            cell_w,
            cell_h,
            wakeup_proxy,
            working_directory,
        ) {
            Ok(terminal) => {
                let new_id = self.next_terminal_id;
                self.next_terminal_id += 1;
                if self.terminals.len() <= new_id {
                    self.terminals.resize_with(new_id + 1, || None);
                }
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
                if let Some(slot) = self.terminals.get_mut(closed_id) {
                    *slot = None;
                }
                self.closed_ids.push(closed_id);
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
        let active = self.tabs.active_index();
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
    pub fn resize_all(&mut self, viewport: Rect, scrollback: usize, cell_w: u16, cell_h: u16) {
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
                    t.resize(
                        info.cols as u16,
                        info.rows as u16,
                        scrollback,
                        cell_w,
                        cell_h,
                    );
                }
            }
        }
        // Inactive tabs: terminals are resized when their tab becomes active.
    }

    /// Return layout info for each leaf pane in the active tab.
    pub fn active_pane_infos(&self, viewport: Rect, cell_w: f32, cell_h: f32) -> Vec<PaneInfo> {
        let tab_idx = self.active_tab_index();
        self.panes
            .get(tab_idx)
            .map(|p| p.pane_infos(viewport, cell_w, cell_h))
            .unwrap_or_default()
    }

    /// Return separator lines between panes in the active tab.
    pub fn active_pane_separators(
        &self,
        viewport: Rect,
        cell_w: f32,
        cell_h: f32,
    ) -> Vec<PaneSeparator> {
        let tab_idx = self.active_tab_index();
        self.panes
            .get(tab_idx)
            .map(|p| p.pane_separators(viewport, cell_w, cell_h))
            .unwrap_or_default()
    }

    /// Read the terminal grid for a specific terminal ID into a pre-allocated buffer.
    ///
    /// Reuses the outer `Vec` and inner `String`/`Vec` allocations from previous calls,
    /// eliminating per-frame heap allocations once the buffer has reached steady state.
    ///
    /// When `search` is `Some`, cells that match the query are recolored in-place.
    pub fn collect_grid_cells_for(
        &self,
        terminal_id: usize,
        buf: &mut Vec<(String, Vec<(AnsiColor, AnsiColor)>)>,
        search: Option<(&[SearchMatch], usize)>,
        force_full: bool,
    ) {
        let Some(Some(terminal)) = self.terminals.get(terminal_id) else {
            buf.clear();
            return;
        };

        // Build line-indexed search map once — O(matches) — so per-cell lookup is O(1) (TD-PERF-22).
        // Each entry: (col_start, col_end_exclusive, is_current_match).
        let search_idx: rustc_hash::FxHashMap<i32, Vec<(usize, usize, bool)>> =
            if let Some((matches, current_idx)) = search {
                let mut idx: rustc_hash::FxHashMap<i32, Vec<(usize, usize, bool)>> =
                    rustc_hash::FxHashMap::default();
                for (i, m) in matches.iter().enumerate() {
                    idx.entry(m.grid_line).or_default().push((
                        m.col,
                        m.col + m.len,
                        i == current_idx,
                    ));
                }
                idx
            } else {
                rustc_hash::FxHashMap::default()
            };

        let mut term = terminal.term.lock();
        let rows = term.screen_lines();
        let cols = term.columns();
        let display_offset = term.grid().display_offset() as i32;
        let sel_range = term.selection.as_ref().and_then(|s| s.to_range(&*term));

        // Damage-aware skip: when no selection or search is active, read damage info
        // and skip undamaged rows — their stale data in `buf` will produce the same hash
        // as last frame, giving a row-cache hit in build_instances without grid reads.
        // REC-PERF-03: integrates alacritty_terminal's TermDamage API.
        let can_skip = !force_full && sel_range.is_none() && search.is_none();
        let damage_set: Option<rustc_hash::FxHashSet<usize>> = {
            use alacritty_terminal::term::TermDamage;
            match term.damage() {
                TermDamage::Full => None,
                TermDamage::Partial(iter) if can_skip => Some(iter.map(|l| l.line).collect()),
                _ => None,
            }
        };
        term.reset_damage();

        // Resize to exact row count, keeping existing allocations.
        if buf.len() < rows {
            buf.resize_with(rows, || {
                (String::with_capacity(cols), Vec::with_capacity(cols))
            });
        } else {
            buf.truncate(rows);
        }

        for (row, (text, colors)) in buf.iter_mut().enumerate() {
            // Skip undamaged rows: retain previous-frame data so the row-cache hash matches.
            if let Some(ref ds) = damage_set {
                if !ds.contains(&row) {
                    continue;
                }
            }

            text.clear();
            colors.clear();
            let grid_line = Line(row as i32 - display_offset);
            for col in 0..cols {
                let cell = &term.grid()[grid_line][Column(col)];
                text.push(if cell.c == '\0' { ' ' } else { cell.c });
                let (fg, bg) = if cell.flags.contains(Flags::INVERSE) {
                    (cell.bg, cell.fg)
                } else {
                    (cell.fg, cell.bg)
                };
                let (fg, bg) = if cell_in_selection(grid_line, Column(col), &sel_range) {
                    (bg, fg)
                } else {
                    (fg, bg)
                };
                let (fg, bg) =
                    search_highlight_at(grid_line.0, col, &search_idx).unwrap_or((fg, bg));
                colors.push((fg, bg));
            }
        }
    }

    /// Incremental filter: given matches from a previous (shorter) query, verify each one
    /// against the new (longer) query. O(prev_matches × query_len) instead of O(rows × cols).
    /// Only valid when `new_query.starts_with(prev_query)` — caller is responsible for this check.
    pub fn filter_matches(&self, prev: &[SearchMatch], new_query: &str) -> Vec<SearchMatch> {
        use alacritty_terminal::index::{Column, Line};
        if new_query.is_empty() || prev.is_empty() {
            return Vec::new();
        }
        let q_lower = new_query.to_lowercase();
        let q_chars: Vec<char> = q_lower.chars().collect();
        let q_len = q_chars.len();
        let Some(terminal) = self.active_terminal() else {
            return Vec::new();
        };
        terminal.with_term(|term| {
            let cols = term.columns();
            prev.iter()
                .filter_map(|m| {
                    if m.col + q_len > cols {
                        return None;
                    }
                    let line = Line(m.grid_line);
                    for (i, &qc) in q_chars.iter().enumerate() {
                        let c = term.grid()[line][Column(m.col + i)].c;
                        let c = if c == '\0' { ' ' } else { c };
                        if c.to_lowercase().next().unwrap_or(c) != qc {
                            return None;
                        }
                    }
                    Some(SearchMatch {
                        grid_line: m.grid_line,
                        col: m.col,
                        len: q_len,
                    })
                })
                .collect()
        })
    }

    /// Search all visible rows and scrollback history for `query` (case-insensitive).
    /// Returns matches sorted from oldest history to current screen.
    pub fn search_active_terminal(&self, query: &str) -> Vec<SearchMatch> {
        if query.is_empty() {
            return Vec::new();
        }
        let query_lower = query.to_lowercase();
        let query_len = query.chars().count();
        let Some(terminal) = self.active_terminal() else {
            return Vec::new();
        };

        terminal.with_term(|term| {
            let screen_rows = term.screen_lines() as i32;
            let history = term.grid().history_size() as i32;
            let cols = term.columns();
            let query_chars: Vec<char> = query_lower.chars().collect();
            let mut matches = Vec::new();

            for grid_row in (-history)..screen_rows {
                let line = Line(grid_row);
                // Build a char-indexed row: index i == terminal column i.
                let row_chars: Vec<char> = (0..cols)
                    .map(|col| {
                        let c = term.grid()[line][Column(col)].c;
                        let c = if c == '\0' { ' ' } else { c };
                        // Lowercase per char for case-insensitive matching.
                        c.to_lowercase().next().unwrap_or(c)
                    })
                    .collect();

                // Slide a window of query_len over the char array.
                // Each index is a terminal column — no byte-offset ambiguity.
                for col in 0..row_chars.len().saturating_sub(query_chars.len() - 1) {
                    if row_chars[col..col + query_chars.len()] == query_chars[..] {
                        matches.push(SearchMatch {
                            grid_line: grid_row,
                            col,
                            len: query_len,
                        });
                    }
                }
            }
            matches
        })
    }

    /// Index of the active workspace in `self.workspaces`.
    pub fn active_workspace_index(&self) -> usize {
        self.workspaces
            .iter()
            .position(|w| w.id == self.active_workspace_id)
            .unwrap_or(0)
    }

    /// Archive the active workspace and activate a new empty one.
    /// Caller must open an initial tab after calling this.
    pub fn cmd_new_workspace(&mut self, name: String) {
        let id = self.next_workspace_id;
        self.next_workspace_id += 1;
        self.workspaces.push(Workspace { id, name });
        self.inactive_workspaces.push(WorkspaceData {
            id: self.active_workspace_id,
            tabs: std::mem::replace(&mut self.tabs, TabManager::new()),
            panes: std::mem::take(&mut self.panes),
        });
        self.active_workspace_id = id;
    }

    /// Close the active workspace. No-op if only one workspace remains.
    /// Caller must drain `closed_ids` to clean up external state.
    pub fn cmd_close_workspace(&mut self) {
        if self.workspaces.len() <= 1 {
            return;
        }
        let all_tids: Vec<usize> = self.panes.iter().flat_map(|pm| pm.root.leaf_ids()).collect();
        for tid in all_tids {
            if let Some(slot) = self.terminals.get_mut(tid) {
                *slot = None;
            }
            self.closed_ids.push(tid);
        }
        let pos = self.active_workspace_index();
        self.workspaces.remove(pos);
        let target_pos = pos.min(self.workspaces.len() - 1);
        let target_id = self.workspaces[target_pos].id;
        if let Some(idx) = self.inactive_workspaces.iter().position(|d| d.id == target_id) {
            let data = self.inactive_workspaces.remove(idx);
            self.tabs = data.tabs;
            self.panes = data.panes;
        }
        self.active_workspace_id = target_id;
    }

    pub fn cmd_rename_workspace(&mut self, name: String) {
        if let Some(w) = self.workspaces.iter_mut().find(|w| w.id == self.active_workspace_id) {
            w.name = name;
        }
    }

    pub fn cmd_switch_workspace(&mut self, id: usize) {
        if id == self.active_workspace_id {
            return;
        }
        self.inactive_workspaces.push(WorkspaceData {
            id: self.active_workspace_id,
            tabs: std::mem::replace(&mut self.tabs, TabManager::new()),
            panes: std::mem::take(&mut self.panes),
        });
        if let Some(idx) = self.inactive_workspaces.iter().position(|d| d.id == id) {
            let data = self.inactive_workspaces.remove(idx);
            self.tabs = data.tabs;
            self.panes = data.panes;
            self.active_workspace_id = id;
        } else {
            // Target not found — undo the archive (shouldn't happen in practice).
            let data = self.inactive_workspaces.pop().unwrap();
            self.tabs = data.tabs;
            self.panes = data.panes;
        }
    }

    pub fn cmd_next_workspace(&mut self) {
        let pos = self.active_workspace_index();
        let id = self.workspaces[(pos + 1) % self.workspaces.len()].id;
        self.cmd_switch_workspace(id);
    }

    pub fn cmd_prev_workspace(&mut self) {
        let pos = self.active_workspace_index();
        let len = self.workspaces.len();
        let id = self.workspaces[if pos == 0 { len - 1 } else { pos - 1 }].id;
        self.cmd_switch_workspace(id);
    }

    pub fn shutdown(&mut self) {
        for terminal in self.terminals.iter_mut().flatten() {
            terminal.pty.shutdown();
        }
    }
}

fn cell_in_selection(line: Line, col: Column, sel_range: &Option<SelectionRange>) -> bool {
    let Some(range) = sel_range else { return false };
    if range.is_block {
        line >= range.start.line
            && line <= range.end.line
            && col >= range.start.column
            && col <= range.end.column
    } else {
        let pt = Point::new(line, col);
        pt >= range.start && pt <= range.end
    }
}

/// Return overridden (fg, bg) colors if (grid_line, col) falls inside any search match.
/// Uses a pre-built per-line index for O(1) line lookup + O(matches_on_line) range check (TD-PERF-22).
fn search_highlight_at(
    grid_line: i32,
    col: usize,
    idx: &rustc_hash::FxHashMap<i32, Vec<(usize, usize, bool)>>,
) -> Option<(AnsiColor, AnsiColor)> {
    for &(start, end, is_current) in idx.get(&grid_line)? {
        if col >= start && col < end {
            let bg = if is_current {
                SEARCH_CURRENT_BG
            } else {
                SEARCH_MATCH_BG
            };
            return Some((SEARCH_MATCH_FG, bg));
        }
    }
    None
}
