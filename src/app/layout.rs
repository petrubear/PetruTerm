use anyhow::Result;

use super::App;
use crate::config::schema::TitleBarStyle;
use crate::ui::Rect;

impl App {
    pub(super) fn tab_bar_visible(&self) -> bool {
        // The unified titlebar bar is always present in Custom mode for traffic lights clearance.
        if self.config.window.title_bar_style == TitleBarStyle::Custom {
            return true;
        }
        self.mux.tabs.tab_count() > 1
    }

    pub(super) fn tab_bar_height_px(&self) -> f32 {
        let sf = self
            .render_ctx
            .as_ref()
            .map(|rc| rc.scale_factor)
            .unwrap_or(1.0);
        if self.config.window.title_bar_style == TitleBarStyle::Custom {
            super::TITLEBAR_HEIGHT * sf
        } else if self.mux.tabs.tab_count() > 1 {
            self.cell_dims().1 as f32
        } else {
            0.0
        }
    }

    pub(super) fn status_bar_height_px(&self) -> f32 {
        if self.config.status_bar.enabled {
            self.cell_dims().1 as f32
        } else {
            0.0
        }
    }

    /// Update the GPU uniform padding to account for the tab bar (or lack thereof).
    /// Call whenever tab count crosses the 1<->2 boundary, or on initial setup.
    pub(super) fn apply_tab_bar_padding(&mut self) {
        if let Some(rc) = &mut self.render_ctx {
            let title_h = if self.config.window.title_bar_style == TitleBarStyle::Custom {
                super::TITLEBAR_HEIGHT * rc.scale_factor
            } else if self.mux.tabs.tab_count() > 1 {
                rc.shaper.cell_height
            } else {
                0.0
            };
            let pad = &self.config.window.padding;
            let sidebar_px = if self.sidebar.visible {
                super::SIDEBAR_COLS as f32 * rc.shaper.cell_width + super::SIDEBAR_MARGIN
            } else {
                0.0
            };
            rc.renderer
                .set_padding(pad.left as f32 + sidebar_px, pad.top as f32 + title_h);
        }
    }

    pub(super) fn default_grid_size(&self) -> (u16, u16) {
        if let Some(rc) = &self.render_ctx {
            let (w, h) = rc.renderer.size();
            let (cell_w, cell_h) = self.cell_dims();
            let pad = &self.config.window.padding;
            let panel_px = if self.ui.is_panel_visible() {
                self.chat_panel_width_px()
            } else {
                0.0
            };
            let sidebar_px = self.sidebar_width_px();
            let tab_h = self.tab_bar_height_px();
            let sb_h = self.status_bar_height_px();
            let cols = ((w as f32 - pad.left as f32 - pad.right as f32 - panel_px - sidebar_px)
                / cell_w as f32)
                .max(1.0) as u16;
            let rows = ((h as f32 - pad.top as f32 - pad.bottom as f32 - tab_h - sb_h)
                / cell_h as f32)
                .max(1.0) as u16;
            (cols, rows)
        } else {
            (120, 40)
        }
    }

    pub(super) fn chat_panel_width_px(&self) -> f32 {
        let (cell_w, _) = self.cell_dims();
        self.ui.panel().width_cols as f32 * cell_w as f32
    }

    pub(super) fn sidebar_width_px(&self) -> f32 {
        if self.sidebar.visible {
            let (cell_w, _) = self.cell_dims();
            super::SIDEBAR_COLS as f32 * cell_w as f32 + super::SIDEBAR_MARGIN
        } else {
            0.0
        }
    }

    pub(super) fn cell_dims(&self) -> (u16, u16) {
        self.render_ctx
            .as_ref()
            .map(|rc| (rc.shaper.cell_width as u16, rc.shaper.cell_height as u16))
            .unwrap_or((8, 16))
    }

    pub(super) fn open_initial_tab(&mut self) -> Result<()> {
        let viewport = self.viewport_rect();
        let (cols, rows) = self.default_grid_size();
        let (cell_w, cell_h) = self.cell_dims();
        self.mux.open_initial_tab(
            &self.config,
            viewport,
            cols,
            rows,
            cell_w,
            cell_h,
            self.wakeup_proxy.clone(),
        )
    }

    pub(super) fn viewport_rect(&self) -> Rect {
        let pad = &self.config.window.padding;
        let tab_h = self.tab_bar_height_px();
        let sb_h = self.status_bar_height_px();
        if let Some(rc) = &self.render_ctx {
            let (w, h) = rc.renderer.size();
            let panel_px = if self.ui.is_panel_visible() {
                self.chat_panel_width_px()
            } else {
                0.0
            };
            let sidebar_px = self.sidebar_width_px();
            Rect {
                x: pad.left as f32 + sidebar_px,
                y: pad.top as f32 + tab_h,
                w: (w as f32 - pad.left as f32 - pad.right as f32 - panel_px - sidebar_px).max(0.0),
                h: (h as f32 - pad.top as f32 - pad.bottom as f32 - tab_h - sb_h).max(0.0),
            }
        } else {
            let sidebar_px = self.sidebar_width_px();
            Rect {
                x: pad.left as f32 + sidebar_px,
                y: pad.top as f32 + tab_h,
                w: 800.0,
                h: 600.0,
            }
        }
    }

    pub(super) fn pixel_to_cell(&self, x: f64, y: f64) -> (usize, usize) {
        // Subtract tab bar height so y is relative to the content viewport top.
        let tab_h = self.tab_bar_height_px() as f64;
        let (raw_col, raw_row) = self.input.pixel_to_cell(
            x - self.sidebar_width_px() as f64,
            y - tab_h,
            &self.config,
            &self.render_ctx,
        );
        // Subtract the focused pane's offset to convert viewport coords to terminal-local coords.
        let (cell_w, cell_h) = self.cell_dims();
        let viewport = self.viewport_rect();
        let (col_off, row_off) =
            self.mux
                .focused_pane_offset(viewport, cell_w as f32, cell_h as f32);
        let (term_cols, term_rows) = self.mux.active_terminal_size();
        (
            raw_col
                .saturating_sub(col_off)
                .min(term_cols.saturating_sub(1)),
            raw_row
                .saturating_sub(row_off)
                .min(term_rows.saturating_sub(1)),
        )
    }

    pub(super) fn panel_hit_cell(&self, x: f64, y: f64) -> Option<(usize, usize)> {
        if !self.ui.is_panel_visible() {
            return None;
        }
        let (cell_w, cell_h) = self.cell_dims();
        let viewport = self.viewport_rect();
        let panel_left = viewport.x as f64 + viewport.w as f64;
        let panel_top = viewport.y as f64;
        let panel_width = self.chat_panel_width_px() as f64;
        let panel_height = viewport.h as f64;
        if x < panel_left
            || x >= panel_left + panel_width
            || y < panel_top
            || y >= panel_top + panel_height
        {
            return None;
        }
        let panel_col = ((x - panel_left) / cell_w as f64).floor().max(0.0) as usize;
        let panel_row = ((y - panel_top) / cell_h as f64).floor().max(0.0) as usize;
        if panel_col >= self.ui.panel().width_cols as usize {
            return None;
        }
        Some((panel_col, panel_row))
    }

    /// Given a pixel x coordinate, return which tab index is under the cursor in the tab bar.
    pub(super) fn hit_test_tab_bar(&self, x_px: f64) -> Option<usize> {
        let (cell_w, _) = self.cell_dims();
        let sf = self
            .render_ctx
            .as_ref()
            .map(|rc| rc.scale_factor as f64)
            .unwrap_or(1.0);
        let tabs_start_x = if self.config.window.title_bar_style == TitleBarStyle::Custom {
            158.0 * sf
        } else {
            self.config.window.padding.left as f64
        };
        if x_px < tabs_start_x {
            return None; // click in the buttons zone, not a tab
        }
        let click_col = ((x_px - tabs_start_x) / cell_w as f64).floor() as usize;
        let mut col = 0usize;
        for (i, tab) in self.mux.tabs.tabs().iter().enumerate() {
            col += 1; // gap before pill
            col += format!(" {} ", i + 1).chars().count(); // badge
            let raw = format!(" {} ", tab.title);
            col += raw.chars().take(14).count();
            if click_col < col {
                return Some(i);
            }
        }
        None
    }

    pub(super) fn mouse_in_panel(&self) -> bool {
        if !self.ui.is_panel_visible() {
            return false;
        }
        self.panel_hit_cell(self.input.mouse_pos.0, self.input.mouse_pos.1)
            .is_some()
    }

    pub(super) fn near_panel_left_edge(&self, x: f64) -> bool {
        if !self.ui.is_panel_visible() {
            return false;
        }
        let viewport = self.viewport_rect();
        let panel_left = (viewport.x + viewport.w) as f64;
        let cell_w = self.cell_dims().0 as f64;
        x >= panel_left - cell_w && x < panel_left + cell_w
    }

    /// Return `(terminal_id, block_id)` if the pixel is inside any row of a completed
    /// command block. Used for hover highlight.
    pub(super) fn block_at_cursor(&self, x: f32, y: f32) -> Option<(usize, usize)> {
        let rc = self.render_ctx.as_ref()?;
        let cell_h = rc.shaper.cell_height;

        for info in &rc.pane_infos {
            let pane = info.pane_rect;
            if x < pane.x || x >= pane.x + pane.w {
                continue;
            }
            if y < pane.y || y >= pane.y + pane.h {
                continue;
            }

            let Some(terminal) = self
                .mux
                .terminals
                .get(info.terminal_id)
                .and_then(|t| t.as_ref())
            else {
                continue;
            };
            let (display_offset, history_size) = terminal.scrollback_info();
            let vp_row = ((y - pane.y) / cell_h) as i64;
            let abs_row = vp_row + history_size as i64 - display_offset as i64;

            if let Some(block) = terminal.block_manager.block_at_absolute_row(abs_row) {
                return Some((info.terminal_id, block.id));
            }
        }
        None
    }

    /// Return `(terminal_id, block_id)` if the pixel is over the exit-code indicator
    /// pill of a completed block. Used exclusively for right-click context menu.
    pub(super) fn block_indicator_at_pixel(&self, x: f32, y: f32) -> Option<(usize, usize)> {
        let rc = self.render_ctx.as_ref()?;
        let cell_w = rc.shaper.cell_width;
        let cell_h = rc.shaper.cell_height;

        for info in &rc.pane_infos {
            let pane = info.pane_rect;
            if x < pane.x || x >= pane.x + pane.w {
                continue;
            }
            if y < pane.y || y >= pane.y + pane.h {
                continue;
            }

            let Some(terminal) = self
                .mux
                .terminals
                .get(info.terminal_id)
                .and_then(|t| t.as_ref())
            else {
                continue;
            };
            let (display_offset, history_size) = terminal.scrollback_info();
            let rows = (pane.h / cell_h) as usize;

            for block in
                terminal
                    .block_manager
                    .blocks_in_viewport(history_size, display_offset, rows)
            {
                let Some(output_end) = block.output_end else {
                    continue;
                };
                let h = history_size as i64;
                let d = display_offset as i64;
                let r = rows as i64;
                let last_vp = (output_end - h + d).clamp(0, r - 1) as f32;

                // Hit zone: right 10 columns of the last output row.
                // Covers the widest possible pill " x 255 " (7 chars) + 1-cell margin.
                let hit_x = pane.x + pane.w - 10.0 * cell_w;
                let hit_y = pane.y + last_vp * cell_h;
                if x >= hit_x && y >= hit_y && y < hit_y + cell_h {
                    return Some((info.terminal_id, block.id));
                }
            }
        }
        None
    }

    /// Given a panel-relative row index, return which zero-state pill (0 or 1) that row maps to,
    /// or None if it's not on a pill.
    pub(super) fn zero_state_hover_for_row(&self, panel_row: usize) -> Option<u8> {
        let (_, cell_h) = self.cell_dims();
        let screen_rows = if let Some(rc) = &self.render_ctx {
            let (_, h) = rc.renderer.size();
            let pad_top = self.config.window.padding.top as f32;
            let pad_bottom = self.config.window.padding.bottom as f32;
            let tab_h = self.tab_bar_height_px();
            let sb_h = self.status_bar_height_px();
            ((h as f32 - pad_top - pad_bottom - tab_h - sb_h) / cell_h as f32).floor() as usize
        } else {
            return None;
        };
        let panel = self.ui.panel();
        let sep_row = screen_rows.saturating_sub(6);
        let file_count = panel.attached_files.len();
        let file_section_rows = if file_count == 0 {
            0
        } else {
            1 + file_count.min(crate::llm::chat_panel::MAX_FILE_ROWS)
        };
        let history_start_row = 1 + if file_section_rows > 0 {
            file_section_rows + 1
        } else {
            0
        };
        let center = (history_start_row + sep_row) / 2;
        let pill1_row = center + 2;
        let pill2_row = center + 3;
        if panel_row == pill1_row {
            Some(0)
        } else if panel_row == pill2_row {
            Some(1)
        } else {
            None
        }
    }

    /// Return which W-7 suggestion pill (0 or 1) a panel-relative row maps to, or None.
    pub(super) fn suggestion_hover_for_row(&self, panel_row: usize) -> Option<u8> {
        let (_, cell_h) = self.cell_dims();
        let screen_rows = if let Some(rc) = &self.render_ctx {
            let (_, h) = rc.renderer.size();
            let pad_top = self.config.window.padding.top as f32;
            let pad_bottom = self.config.window.padding.bottom as f32;
            let tab_h = self.tab_bar_height_px();
            let sb_h = self.status_bar_height_px();
            ((h as f32 - pad_top - pad_bottom - tab_h - sb_h) / cell_h as f32).floor() as usize
        } else {
            return None;
        };
        let sep_row = screen_rows.saturating_sub(6);
        // Pills sit at sep_row-2 and sep_row-1.
        if sep_row < 2 {
            return None;
        }
        let pill1_row = sep_row - 2;
        let pill2_row = sep_row - 1;
        if panel_row == pill1_row {
            Some(0)
        } else if panel_row == pill2_row {
            Some(1)
        } else {
            None
        }
    }

    /// B-4: copy the output text of `hover_block` to the clipboard.
    pub(super) fn copy_hover_block_output(&mut self) {
        if let Some((tid, bid)) = self.hover_block {
            if let Some(text) = self.mux.block_output_text(tid, bid) {
                if !text.is_empty() {
                    std::thread::spawn(move || {
                        let _ = arboard::Clipboard::new().and_then(|mut cb| cb.set_text(text));
                    });
                }
            }
        }
    }

    /// B-4: re-run the command of `hover_block` by writing it to the PTY.
    pub(super) fn rerun_hover_block_command(&mut self) {
        if let Some((tid, bid)) = self.hover_block {
            let cmd = self
                .mux
                .terminals
                .get(tid)
                .and_then(|t| t.as_ref())
                .and_then(|t| t.block_manager.find_block_by_id(bid))
                .filter(|b| !b.command_text.is_empty())
                .map(|b| b.command_text.clone());
            if let Some(cmd_text) = cmd {
                if let Some(terminal) = self.mux.terminals.get_mut(tid).and_then(|t| t.as_mut()) {
                    let input = format!("{}\n", cmd_text);
                    terminal.write_input(input.as_bytes());
                }
            }
        }
    }
}
