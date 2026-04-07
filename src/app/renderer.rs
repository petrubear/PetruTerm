use anyhow::Result;
use std::collections::HashMap;
use std::sync::Arc;
use winit::window::Window;

use crate::config::Config;
use crate::font::{build_font_system, TextShaper};
use crate::renderer::cell::{CellVertex, FLAG_COLOR_GLYPH, FLAG_CURSOR, FLAG_LCD};
use crate::renderer::rounded_rect::RoundedRectInstance;
use crate::renderer::GpuRenderer;
use crate::term::{CursorInfo, CursorShape};
use crate::term::color::resolve_color;
use alacritty_terminal::vte::ansi::Color as AnsiColor;
use crate::llm::chat_panel::ChatPanel;
use crate::llm::ai_block::{AiBlock, AiState, AI_BLOCK_ROWS};
use crate::ui::{CommandPalette, PaneSeparator, Tab};

/// Cache for a single shaped row to avoid re-shaping every frame.
#[derive(Clone)]
pub struct RowCacheEntry {
    pub hash: u64,
    pub instances: Vec<CellVertex>,
    pub lcd_instances: Vec<CellVertex>,
}

/// Tracks shaped data for every visible row in one terminal's viewport.
pub struct RowCache {
    pub rows: Vec<Option<RowCacheEntry>>,
    pub dirty_rows: Vec<bool>,
}

impl RowCache {
    pub fn new() -> Self {
        Self { rows: Vec::new(), dirty_rows: Vec::new() }
    }

    #[allow(dead_code)]
    pub fn clear(&mut self) {
        for r in &mut self.rows { *r = None; }
        for d in &mut self.dirty_rows { *d = true; }
    }
}

/// Manages GPU resources, font shaping, and the rendering loop.
pub struct RenderContext {
    pub renderer: GpuRenderer,
    pub shaper: TextShaper,
    pub scale_factor: f32,
    pub atlas_generation: usize,
    /// Per-terminal row caches, keyed by terminal_id.
    pub row_caches: HashMap<usize, RowCache>,
    pub instances: Vec<CellVertex>,
    pub lcd_instances: Vec<CellVertex>,
    /// Cached GPU instances for the AI chat panel — rebuilt only when `ChatPanel::dirty`.
    pub panel_instances_cache: Vec<CellVertex>,
    /// Rounded rect instances for the tab bar pills (TD-013), cleared each frame.
    pub rect_instances: Vec<RoundedRectInstance>,
}

impl RenderContext {
    pub async fn new(window: Arc<Window>, config: &Config) -> Result<Self> {
        let renderer = GpuRenderer::new(window.clone(), config).await?;
        let scale_factor = window.scale_factor() as f32;

        let mut scaled_font = config.font.clone();
        scaled_font.size *= scale_factor;
        crate::font::loader::locate_font_for_lcd(&mut scaled_font);

        let (font_system, actual_family, face_id, font_path) = build_font_system(&scaled_font)?;
        let lcd_atlas = renderer.get_lcd_atlas();

        let mut shaper = TextShaper::new(&renderer.device(), font_system, actual_family, face_id, font_path, &scaled_font, lcd_atlas);
        
        // Finalize renderer setup with shaper info
        let mut renderer = renderer;
        renderer.set_cell_size(shaper.cell_width, shaper.cell_height);
        if let Some(atlas) = shaper.lcd_atlas.take() {
            renderer.set_lcd_atlas(atlas);
        }

        Ok(Self {
            renderer,
            shaper,
            scale_factor,
            atlas_generation: 0,
            row_caches: HashMap::new(),
            instances: Vec::new(),
            lcd_instances: Vec::new(),
            panel_instances_cache: Vec::new(),
            rect_instances: Vec::new(),
        })
    }

    /// Returns the font config with size scaled to physical pixels.
    pub fn scaled_font_config(&self, config: &Config) -> crate::config::schema::FontConfig {
        let mut cfg = config.font.clone();
        cfg.size *= self.scale_factor;
        cfg
    }

    /// Clear per-frame instance buffers. Call once before rendering all panes.
    pub fn begin_frame(&mut self) {
        self.instances.clear();
        self.lcd_instances.clear();
        self.rect_instances.clear();
    }

    /// Mark every row in every terminal cache as dirty (forces reshape next frame).
    pub fn mark_all_rows_dirty(&mut self) {
        for cache in self.row_caches.values_mut() {
            cache.dirty_rows.fill(true);
        }
    }

    /// Drop all per-terminal row caches (used after atlas eviction).
    pub fn clear_all_row_caches(&mut self) {
        self.row_caches.clear();
    }

    /// Reset dirty flags after a completed frame for all cached terminals.
    pub fn reset_row_dirty_flags(&mut self) {
        for cache in self.row_caches.values_mut() {
            cache.dirty_rows.fill(false);
        }
    }

    /// Build and append cell instances for one pane's terminal.
    ///
    /// Instances are APPENDED to `self.instances` (not cleared); call `begin_frame()` first.
    /// `col_offset` and `row_offset` position this pane within the global grid coordinate space.
    #[allow(clippy::too_many_arguments)]
    pub fn build_instances(
        &mut self,
        cell_data: &[(String, Vec<(AnsiColor, AnsiColor)>)],
        config: &Config,
        font: &crate::config::schema::FontConfig,
        cursor: Option<&CursorInfo>,
        cursor_blink_on: bool,
        terminal_id: usize,
        col_offset: usize,
        row_offset: usize,
    ) -> Result<(), crate::renderer::atlas::AtlasError> {
        // Retrieve or create the per-terminal row cache.
        let cache = self.row_caches.entry(terminal_id).or_insert_with(RowCache::new);
        if cache.rows.len() < cell_data.len() {
            cache.rows.resize(cell_data.len(), None);
            cache.dirty_rows.resize(cell_data.len(), true);
        }

        let mut colors_scratch: Vec<([f32; 4], [f32; 4])> = Vec::with_capacity(256);

        for (row_idx, (text, raw_colors)) in cell_data.iter().enumerate() {
            colors_scratch.clear();
            colors_scratch.extend(raw_colors.iter().map(|(fg, bg)| {
                (
                    resolve_color(*fg, &config.colors),
                    resolve_color(*bg, &config.colors),
                )
            }));
            let colors: &[([f32; 4], [f32; 4])] = &colors_scratch;

            let row_hash = calculate_row_hash(text, colors);

            // Cache hit: copy local-coordinate instances and apply pane offset.
            if let Some(Some(entry)) = self.row_caches.get(&terminal_id).and_then(|c| c.rows.get(row_idx)) {
                if entry.hash == row_hash {
                    let co = col_offset as f32;
                    let ro = (row_offset + row_idx) as f32;
                    for inst in &entry.instances {
                        let mut v = *inst;
                        v.grid_pos[0] += co;
                        v.grid_pos[1] = ro;
                        self.instances.push(v);
                    }
                    for inst in &entry.lcd_instances {
                        let mut v = *inst;
                        v.grid_pos[0] += co;
                        v.grid_pos[1] = ro;
                        self.lcd_instances.push(v);
                    }
                    continue;
                }
            }

            // Cache miss: shape and rasterize.
            if let Some(cache) = self.row_caches.get_mut(&terminal_id) {
                if row_idx < cache.dirty_rows.len() {
                    cache.dirty_rows[row_idx] = true;
                }
            }
            let mut row_instances: Vec<CellVertex> = Vec::new();
            let mut row_lcd_instances: Vec<CellVertex> = Vec::new();

            let shaped = self.shaper.shape_line(text, colors, font);

            for glyph in &shaped.glyphs {
                let lcd_entry = if let Some(queue) = self.renderer.lcd_queue() {
                    self.shaper.rasterize_lcd_to_atlas(glyph.cache_key, glyph.ch, queue)
                } else {
                    None
                };

                let (atlas, queue) = self.renderer.atlas_and_queue();
                let swash_entry = self.shaper.rasterize_to_atlas(glyph.cache_key, atlas, queue)?;

                let (atlas_uv, glyph_offset, glyph_size) = if lcd_entry.is_none() {
                    let ox = swash_entry.bearing_x as f32;
                    let oy = shaped.ascent - swash_entry.bearing_y as f32;
                    let gw = swash_entry.width as f32;
                    let gh = swash_entry.height as f32;
                    let y0 = oy.max(0.0);
                    let y1 = (oy + gh).min(self.shaper.cell_height);
                    if y1 <= y0 || gw == 0.0 || gh == 0.0 {
                        ([0.0f32; 4], [0.0; 2], [0.0; 2])
                    } else {
                        let fy0 = (y0 - oy) / gh;
                        let fy1 = (y1 - oy) / gh;
                        let [u0, v0, u1, v1] = swash_entry.uv;
                        ([u0, v0 + fy0 * (v1 - v0), u1, v0 + fy1 * (v1 - v0)], [ox, y0], [gw, y1 - y0])
                    }
                } else {
                    ([0.0f32; 4], [0.0; 2], [0.0; 2])
                };

                let color_flag = if swash_entry.is_color { FLAG_COLOR_GLYPH } else { 0 };
                // Store LOCAL coordinates in the cache (col within pane, row within pane).
                row_instances.push(CellVertex {
                    grid_pos: [glyph.col as f32, row_idx as f32],
                    atlas_uv,
                    fg: glyph.fg,
                    bg: glyph.bg,
                    glyph_offset,
                    glyph_size,
                    flags: color_flag,
                    _pad: 0,
                });

                if let Some(entry) = lcd_entry {
                    let ox = entry.bearing_x as f32;
                    let oy = shaped.ascent - entry.bearing_y as f32;
                    let gw = entry.width as f32;
                    let gh = entry.height as f32;
                    let y0 = oy.max(0.0);
                    let y1 = (oy + gh).min(self.shaper.cell_height);
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

            // Emit with pane offset applied.
            let co = col_offset as f32;
            let ro = (row_offset + row_idx) as f32;
            for inst in &row_instances {
                let mut v = *inst;
                v.grid_pos[0] += co;
                v.grid_pos[1] = ro;
                self.instances.push(v);
            }
            for inst in &row_lcd_instances {
                let mut v = *inst;
                v.grid_pos[0] += co;
                v.grid_pos[1] = ro;
                self.lcd_instances.push(v);
            }

            // Store local coordinates in cache.
            if let Some(cache) = self.row_caches.get_mut(&terminal_id) {
                if row_idx < cache.rows.len() {
                    cache.rows[row_idx] = Some(RowCacheEntry {
                        hash: row_hash,
                        instances: row_instances,
                        lcd_instances: row_lcd_instances,
                    });
                }
            }
        }

        // Cursor (only for the focused pane, where cursor is Some).
        if let Some(info) = cursor {
            if info.visible && cursor_blink_on {
                let cw = self.shaper.cell_width;
                let ch = self.shaper.cell_height;
                let (glyph_offset, glyph_size) = match info.shape {
                    CursorShape::Block | CursorShape::HollowBlock => ([0.0f32, 0.0], [cw, ch]),
                    CursorShape::Underline => ([0.0, (ch - 2.0).max(0.0)], [cw, 2.0]),
                    CursorShape::Beam      => ([0.0, 0.0], [2.0, ch]),
                    CursorShape::Hidden    => ([0.0; 2], [0.0; 2]),
                };
                self.instances.push(CellVertex {
                    grid_pos:     [(col_offset + info.col) as f32, (row_offset + info.row) as f32],
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

    /// Draw 1-pixel separator lines between panes.
    pub fn build_pane_separators(&mut self, separators: &[PaneSeparator]) {
        const SEP_COLOR: [f32; 4] = [0.35, 0.30, 0.48, 1.0]; // dim purple
        let ch = self.shaper.cell_height;
        let cw = self.shaper.cell_width;
        for sep in separators {
            if sep.vertical {
                // 1px left edge at column `sep.col`, spanning `sep.length` rows.
                for i in 0..sep.length {
                    self.instances.push(CellVertex {
                        grid_pos:     [sep.col as f32, (sep.row + i) as f32],
                        atlas_uv:     [0.0; 4],
                        fg:           [0.0; 4],
                        bg:           SEP_COLOR,
                        glyph_offset: [0.0, 0.0],
                        glyph_size:   [1.0, ch],
                        flags:        FLAG_CURSOR,
                        _pad:         0,
                    });
                }
            } else {
                // 1px top edge at row `sep.row`, spanning `sep.length` columns.
                for i in 0..sep.length {
                    self.instances.push(CellVertex {
                        grid_pos:     [(sep.col + i) as f32, sep.row as f32],
                        atlas_uv:     [0.0; 4],
                        fg:           [0.0; 4],
                        bg:           SEP_COLOR,
                        glyph_offset: [0.0, 0.0],
                        glyph_size:   [cw, 1.0],
                        flags:        FLAG_CURSOR,
                        _pad:         0,
                    });
                }
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn push_shaped_row(
        &mut self,
        text: &str,
        fg: [f32; 4],
        bg: [f32; 4],
        row: usize,
        col_offset: usize,
        width: usize,
        font: &crate::config::schema::FontConfig,
    ) {
        if width == 0 { return; }

        let chars: Vec<char> = text.chars().take(width).collect();
        let len = chars.len();
        let padded: String = chars
            .into_iter()
            .chain(std::iter::repeat_n(' ', width.saturating_sub(len)))
            .collect();

        let colors: Vec<([f32; 4], [f32; 4])> = (0..width).map(|_| (fg, bg)).collect();
        let shaped = self.shaper.shape_line(&padded, &colors, font);

        for glyph in shaped.glyphs {
            if glyph.col >= width { continue; }

            let (atlas, queue) = self.renderer.atlas_and_queue();
            let entry = match self.shaper.rasterize_to_atlas(glyph.cache_key, atlas, queue) {
                Ok(e) => e,
                Err(_) => crate::renderer::atlas::AtlasEntry {
                    uv: [0.0; 4],
                    width: 0, height: 0, bearing_x: 0, bearing_y: 0, is_color: false,
                    last_used: 0,
                },
            };

            let ox = entry.bearing_x as f32;
            let oy = shaped.ascent - entry.bearing_y as f32;
            let gw = entry.width as f32;
            let gh = entry.height as f32;

            let y0 = oy.max(0.0);
            let y1 = (oy + gh).min(self.shaper.cell_height);

            let (atlas_uv, glyph_offset, glyph_size) = if y1 <= y0 || gw == 0.0 || gh == 0.0 {
                ([0.0f32; 4], [0.0; 2], [0.0; 2])
            } else {
                let fy0 = (y0 - oy) / gh;
                let fy1 = (y1 - oy) / gh;
                let [u0, v0, u1, v1] = entry.uv;
                ([u0, v0 + fy0 * (v1 - v0), u1, v0 + fy1 * (v1 - v0)], [ox, y0], [gw, y1 - y0])
            };

            let color_flag = if entry.is_color { FLAG_COLOR_GLYPH } else { 0 };
            self.instances.push(CellVertex {
                grid_pos: [(col_offset + glyph.col) as f32, row as f32],
                atlas_uv,
                fg,
                bg,
                glyph_offset,
                glyph_size,
                flags: color_flag,
                _pad: 0,
            });
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn build_chat_panel_instances(
        &mut self,
        panel: &ChatPanel,
        panel_focused: bool,
        file_picker_focused: bool,
        config: &Config,
        font: &crate::config::schema::FontConfig,
        term_cols: usize,
        screen_rows: usize,
        cursor_blink_on: bool,
    ) {
        use crate::llm::chat_panel::{word_wrap, wrap_input, ConfirmDisplay, MAX_FILE_ROWS, PanelState};
        use crate::llm::diff::DiffKind;
        use crate::llm::ChatRole;

        let panel_cols = panel.width_cols as usize;
        if panel_cols == 0 || screen_rows < 6 { return; }

        // ── Colors (Dracula Pro palette) ─────────────────────────────────────
        let panel_bg = config.llm.ui.background;
        let user_fg  = config.llm.ui.user_fg;
        let asst_fg  = config.llm.ui.assistant_fg;
        let input_fg = config.llm.ui.input_fg;

        const BORDER_FG:  [f32; 4] = [0.58, 0.50, 1.00, 1.0]; // purple
        const BORDER_DIM: [f32; 4] = [0.32, 0.28, 0.50, 1.0]; // dimmed purple
        const STREAM_FG:  [f32; 4] = [0.95, 0.98, 0.55, 1.0]; // yellow
        const HINT_FG:    [f32; 4] = [0.38, 0.44, 0.64, 1.0]; // comment gray
        const ERR_FG:     [f32; 4] = [1.00, 0.33, 0.33, 1.0]; // red
        const SEP_FG:     [f32; 4] = [0.27, 0.28, 0.36, 1.0]; // current-line
        const DIM_FG:     [f32; 4] = [0.50, 0.47, 0.60, 1.0]; // dimmed input
        const RUN_FG:     [f32; 4] = [0.50, 0.98, 0.60, 1.0]; // green — run bar
        const FILE_FG:    [f32; 4] = [0.78, 0.92, 0.65, 1.0]; // light green — attached files
        const PICK_SEL:   [f32; 4] = [0.58, 0.50, 1.00, 1.0]; // purple — picker highlight
        const PICK_FG:    [f32; 4] = [0.80, 0.80, 0.90, 1.0]; // soft white — picker items

        const SPIN: [&str; 8] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧"];
        let spin = SPIN[panel.streaming_buf.chars().count() % 8];

        let co = term_cols; // grid column where panel begins
        let border_fg = if panel_focused { BORDER_FG } else { BORDER_DIM };

        // ── Fixed bottom rows (always present) ───────────────────────────────
        let hints_row  = screen_rows - 1;
        let input_row2 = screen_rows - 2;
        let input_row1 = screen_rows - 3;
        let sep_row    = screen_rows - 4;

        // ── File section height (0 when no files attached) ───────────────────
        // header row ("│ Selected (N files)") + one row per file, capped at MAX_FILE_ROWS
        let file_count = panel.attached_files.len();
        let file_section_rows = if file_count == 0 {
            0
        } else {
            1 + file_count.min(MAX_FILE_ROWS)
        };

        // ── Row 0: panel header ───────────────────────────────────────────────
        let title = " Petrubot ";
        let left  = "│───";
        let dashes = panel_cols.saturating_sub(left.chars().count() + title.chars().count());
        let header = format!("{}{}{}", left, title, "─".repeat(dashes));
        self.push_shaped_row(&header, border_fg, panel_bg, 0, co, panel_cols, font);

        // ── File picker overlay (replaces history area) ───────────────────────
        if panel.file_picker_open {
            // Row 1: search input
            let q = &panel.file_picker_query;
            let q_display = if file_picker_focused && cursor_blink_on {
                format!("│ > {}\u{258b}", q)
            } else {
                format!("│ > {}", q)
            };
            self.push_shaped_row(&q_display, input_fg, panel_bg, 1, co, panel_cols, font);

            // Rows 2..sep_row: filtered file list
            let filtered = panel.filtered_picker_items();
            let list_rows = sep_row.saturating_sub(2);
            for i in 0..list_rows {
                let row = 2 + i;
                if let Some(path) = filtered.get(i) {
                    let name = path.to_string_lossy();
                    let max_w = panel_cols.saturating_sub(5);
                    let trimmed = if name.chars().count() > max_w {
                        format!("…{}", &name[name.len().saturating_sub(max_w - 1)..])
                    } else {
                        name.into_owned()
                    };
                    let attached = panel.attached_files.iter().any(|p| p.ends_with(path));
                    let marker = if attached { "✓ " } else { "  " };
                    let (text, fg) = if i == panel.file_picker_cursor {
                        (format!("│ ▸ {}{}", marker, trimmed), PICK_SEL)
                    } else {
                        (format!("│   {}{}", marker, trimmed), PICK_FG)
                    };
                    self.push_shaped_row(&text, fg, panel_bg, row, co, panel_cols, font);
                } else {
                    self.push_shaped_row("│", SEP_FG, panel_bg, row, co, panel_cols, font);
                }
            }
        } else if matches!(panel.state, PanelState::AwaitingConfirm) {
            // ── Confirmation view: diff preview + [y]/[n] ────────────────────
            const ADD_FG:  [f32; 4] = [0.50, 0.98, 0.60, 1.0]; // green
            const REM_FG:  [f32; 4] = [1.00, 0.47, 0.47, 1.0]; // red
            const CTX_FG2: [f32; 4] = [0.60, 0.60, 0.70, 1.0]; // dimmed context

            match panel.confirm_display.as_ref() {
                Some(ConfirmDisplay::Write { path, diff, added, removed }) => {
                    // Row 1: title
                    let rel_path = std::path::Path::new(path)
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or(path.as_str());
                    let title_line = format!("│ Write: {} (+{added} -{removed})", rel_path);
                    let title_trimmed: String = title_line.chars().take(panel_cols).collect();
                    self.push_shaped_row(&title_trimmed, BORDER_FG, panel_bg, 1, co, panel_cols, font);

                    // Rows 2..sep_row: diff lines
                    let diff_rows = sep_row.saturating_sub(2);
                    for i in 0..diff_rows {
                        let row = 2 + i;
                        if let Some(dl) = diff.get(i) {
                            let (prefix, fg) = match dl.kind {
                                DiffKind::Added   => ("│ + ", ADD_FG),
                                DiffKind::Removed => ("│ - ", REM_FG),
                                DiffKind::Context => ("│   ", CTX_FG2),
                            };
                            let max_w = panel_cols.saturating_sub(prefix.chars().count());
                            let text: String = dl.text.chars().take(max_w).collect();
                            let line = format!("{prefix}{text}");
                            self.push_shaped_row(&line, fg, panel_bg, row, co, panel_cols, font);
                        } else {
                            self.push_shaped_row("│", SEP_FG, panel_bg, row, co, panel_cols, font);
                        }
                    }
                }
                Some(ConfirmDisplay::Run { cmd }) => {
                    // Row 1: title
                    self.push_shaped_row("│ Run command:", BORDER_FG, panel_bg, 1, co, panel_cols, font);
                    // Row 2: command
                    let cmd_line = format!("│   {}", cmd.chars().take(panel_cols.saturating_sub(5)).collect::<String>());
                    self.push_shaped_row(&cmd_line, ADD_FG, panel_bg, 2, co, panel_cols, font);
                    // Rest: empty
                    for row in 3..sep_row {
                        self.push_shaped_row("│", SEP_FG, panel_bg, row, co, panel_cols, font);
                    }
                }
                None => {
                    for row in 1..sep_row {
                        self.push_shaped_row("│", SEP_FG, panel_bg, row, co, panel_cols, font);
                    }
                }
            }
        } else {
            // ── Normal view: file section + message history ───────────────────

            // File section (rows 1..1+file_section_rows)
            if file_section_rows > 0 {
                // Header: "│ Selected (N files)"
                let fhdr = format!("│ Selected ({} file{})", file_count, if file_count == 1 { "" } else { "s" });
                self.push_shaped_row(&fhdr, FILE_FG, panel_bg, 1, co, panel_cols, font);
                // File list
                for (i, path) in panel.attached_files.iter().take(MAX_FILE_ROWS).enumerate() {
                    let name = path.file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_else(|| path.to_string_lossy().into_owned());
                    let max_w = panel_cols.saturating_sub(6);
                    let trimmed = if name.chars().count() > max_w {
                        format!("{}…", name.chars().take(max_w.saturating_sub(1)).collect::<String>())
                    } else { name };
                    let line = format!("│   {}", trimmed);
                    self.push_shaped_row(&line, DIM_FG, panel_bg, 2 + i, co, panel_cols, font);
                }
                // Thin separator after file section
                let fsep = format!("│{}", "╌".repeat(panel_cols.saturating_sub(1)));
                self.push_shaped_row(&fsep, SEP_FG, panel_bg, 1 + file_section_rows, co, panel_cols, font);
            }

            // History area: rows after file section up to sep_row
            let history_start_row = 1 + if file_section_rows > 0 { file_section_rows + 1 } else { 0 };
            let history_rows = sep_row.saturating_sub(history_start_row);
            let msg_inner_w = panel_cols.saturating_sub(8);

            let mut all_lines: Vec<(String, [f32; 4])> = Vec::new();

            for msg in &panel.messages {
                let (first_p, cont_p, fg) = match msg.role {
                    ChatRole::User      => ("│  You  ", "│       ", user_fg),
                    ChatRole::Assistant => ("│   AI  ", "│       ", asst_fg),
                    ChatRole::System    => continue,
                    ChatRole::Tool(_)   => continue,
                };
                let wrapped = word_wrap(&msg.content, msg_inner_w);
                for (i, line) in wrapped.iter().enumerate() {
                    let p = if i == 0 { first_p } else { cont_p };
                    all_lines.push((format!("{}{}", p, line), fg));
                }
                all_lines.push(("│".to_string(), SEP_FG));
            }

            if panel.is_streaming() && !panel.streaming_buf.is_empty() {
                let wrapped = word_wrap(&panel.streaming_buf, msg_inner_w);
                for (i, line) in wrapped.iter().enumerate() {
                    let p = if i == 0 { "│   AI  " } else { "│       " };
                    all_lines.push((format!("{}{}", p, line), STREAM_FG));
                }
            }

            if matches!(panel.state, PanelState::Loading) {
                all_lines.push((format!("│   {}  Thinking\u{2026}", spin), STREAM_FG));
            }

            if let PanelState::Error(ref err) = panel.state {
                let wrapped = word_wrap(err, msg_inner_w);
                for (i, line) in wrapped.iter().enumerate() {
                    let p = if i == 0 { "│  \u{2717}    " } else { "│       " };
                    all_lines.push((format!("{}{}", p, line), ERR_FG));
                }
            }

            if panel.is_idle() {
                if let Some(cmd) = panel.last_assistant_command() {
                    let max_cmd_w = panel_cols.saturating_sub(5);
                    let display_cmd = if cmd.chars().count() > max_cmd_w {
                        format!("{}…", cmd.chars().take(max_cmd_w.saturating_sub(1)).collect::<String>())
                    } else { cmd };
                    all_lines.push(("│".to_string(), SEP_FG));
                    all_lines.push((format!("│ \u{23ce}  {}", display_cmd), RUN_FG));
                }
            }

            let visible_start = all_lines.len()
                .saturating_sub(history_rows + panel.scroll_offset);

            for i in 0..history_rows {
                let row = history_start_row + i;
                let (text, fg) = all_lines
                    .get(visible_start + i)
                    .map(|(t, f)| (t.as_str(), *f))
                    .unwrap_or(("│", SEP_FG));
                self.push_shaped_row(text, fg, panel_bg, row, co, panel_cols, font);
            }
        }

        // ── Separator ────────────────────────────────────────────────────────
        let sep = format!("│{}", "─".repeat(panel_cols.saturating_sub(1)));
        self.push_shaped_row(&sep, SEP_FG, panel_bg, sep_row, co, panel_cols, font);

        // ── Input field (or confirmation prompt) ─────────────────────────────
        if matches!(panel.state, PanelState::AwaitingConfirm) {
            // Show confirmation buttons instead of the normal input
            let confirm_kind = match panel.confirm_display.as_ref() {
                Some(ConfirmDisplay::Run { .. }) => "run",
                _ => "write",
            };
            let (yes_label, no_label) = if confirm_kind == "run" {
                ("[y] Run", "[n] Cancel")
            } else {
                ("[y] Apply", "[n] Reject")
            };
            const CONFIRM_YES: [f32; 4] = [0.50, 0.98, 0.60, 1.0]; // green
            const CONFIRM_NO:  [f32; 4] = [1.00, 0.47, 0.47, 1.0]; // red
            let yes_line = format!("│  {yes_label}");
            let no_line  = format!("│  {no_label}");
            self.push_shaped_row(&yes_line, CONFIRM_YES, panel_bg, input_row1, co, panel_cols, font);
            self.push_shaped_row(&no_line,  CONFIRM_NO,  panel_bg, input_row2, co, panel_cols, font);
        } else {
            let input_inner_w = panel_cols.saturating_sub(5);
            let mut input_display = panel.input.clone();
            if panel_focused && !file_picker_focused && cursor_blink_on && panel.is_idle() {
                input_display.push('\u{258b}');
            }
            let input_lines = wrap_input(&input_display, input_inner_w);
            let inp_fg = if panel_focused && !file_picker_focused { input_fg } else { DIM_FG };
            let line1 = format!("│ \u{25b8}  {}", input_lines.first().cloned().unwrap_or_default());
            let line2 = format!("│    {}", input_lines.get(1).cloned().unwrap_or_default());
            self.push_shaped_row(&line1, inp_fg, panel_bg, input_row1, co, panel_cols, font);
            self.push_shaped_row(&line2, inp_fg, panel_bg, input_row2, co, panel_cols, font);
        }

        // ── Key hints + token count ───────────────────────────────────────────
        let tokens = panel.estimated_tokens();
        let has_assistant = panel.messages.iter().any(|m| matches!(m.role, ChatRole::Assistant));
        let hints: String = if file_picker_focused {
            format!("│ ↑↓ navigate   Enter: attach   Tab: close  Tokens: {tokens}")
        } else if !panel_focused {
            format!("│ <Leader>a: focus   Esc: close   Tokens: {tokens}")
        } else {
            let base = match &panel.state {
                PanelState::Idle if !panel.input.trim().is_empty()
                    => "│ Enter: send   Tab: files   Esc: close",
                PanelState::Idle if has_assistant
                    => "│ Enter: run \u{23ce}   Tab: files",
                PanelState::Idle
                    => "│ Enter: send   Tab: files   Esc: close",
                PanelState::Loading | PanelState::Streaming
                    => "│ streaming\u{2026}",
                PanelState::Error(_)
                    => "│ Esc: dismiss",
                PanelState::AwaitingConfirm
                    => "│ y/Enter: confirm   n/Esc: reject",
                PanelState::Hidden => "│",
            };
            format!("{base}   Tokens: {tokens}")
        };
        // Truncate hints to panel width
        let hints_display: String = hints.chars().take(panel_cols).collect();
        self.push_shaped_row(&hints_display, HINT_FG, panel_bg, hints_row, co, panel_cols, font);
    }

    /// Render the inline AI block, overlaying the bottom `AI_BLOCK_ROWS` rows of the terminal.
    /// Instances are appended after the terminal rows so they render on top.
    pub fn build_ai_block_instances(
        &mut self,
        block: &AiBlock,
        font: &crate::config::schema::FontConfig,
        term_cols: usize,
        screen_rows: usize,
    ) {
        use crate::llm::chat_panel::word_wrap;

        if screen_rows < AI_BLOCK_ROWS + 1 || term_cols < 4 { return; }

        let w = term_cols;
        let sep_row   = screen_rows - AI_BLOCK_ROWS;
        let input_row = screen_rows - AI_BLOCK_ROWS + 1;
        let resp_row  = screen_rows - AI_BLOCK_ROWS + 2;
        let hint_row  = screen_rows - AI_BLOCK_ROWS + 3;

        const BLOCK_BG:  [f32; 4] = [0.11, 0.11, 0.18, 1.0]; // dark bg
        const BORDER_FG: [f32; 4] = [0.58, 0.50, 1.00, 1.0]; // purple
        const INPUT_FG:  [f32; 4] = [0.95, 0.95, 0.95, 1.0]; // white
        const RESP_FG:   [f32; 4] = [0.50, 0.98, 0.60, 1.0]; // green
        const STREAM_FG: [f32; 4] = [0.95, 0.98, 0.55, 1.0]; // yellow
        const HINT_FG:   [f32; 4] = [0.38, 0.44, 0.64, 1.0]; // dim gray
        const ERR_FG:    [f32; 4] = [1.00, 0.33, 0.33, 1.0]; // red

        const SPIN: [&str; 8] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧"];
        let spin = SPIN[block.response.chars().count() % 8];

        // Separator
        let title = " AI ";
        let side = (w.saturating_sub(title.chars().count())) / 2;
        let sep = format!("{}{}{}", "─".repeat(side), title, "─".repeat(w.saturating_sub(side + title.chars().count())));
        self.push_shaped_row(&sep, BORDER_FG, BLOCK_BG, sep_row, 0, w, font);

        // Input row: "⚡ > <query>[cursor]"
        let cursor = if matches!(block.state, AiState::Typing) { "▋" } else { "" };
        let query_row = format!("⚡ > {}{}", block.query, cursor);
        self.push_shaped_row(&query_row, INPUT_FG, BLOCK_BG, input_row, 0, w, font);

        // Response + hint rows
        match &block.state {
            AiState::Typing => {
                self.push_shaped_row("", BLOCK_BG, BLOCK_BG, resp_row, 0, w, font);
                self.push_shaped_row("  Enter: send   Esc: cancel", HINT_FG, BLOCK_BG, hint_row, 0, w, font);
            }
            AiState::Loading => {
                self.push_shaped_row(&format!("  {} thinking\u{2026}", spin), STREAM_FG, BLOCK_BG, resp_row, 0, w, font);
                self.push_shaped_row("  Esc: cancel", HINT_FG, BLOCK_BG, hint_row, 0, w, font);
            }
            AiState::Streaming => {
                let lines = word_wrap(&block.response, w.saturating_sub(4));
                let line = format!("  \u{2192} {}", lines.first().cloned().unwrap_or_default()); // →
                self.push_shaped_row(&line, STREAM_FG, BLOCK_BG, resp_row, 0, w, font);
                self.push_shaped_row(&format!("  {} streaming\u{2026}   Esc: cancel", spin), HINT_FG, BLOCK_BG, hint_row, 0, w, font);
            }
            AiState::Done => {
                if let Some(cmd) = block.command_to_run() {
                    let max_cmd = w.saturating_sub(5);
                    let display = if cmd.chars().count() > max_cmd {
                        format!("{}…", cmd.chars().take(max_cmd.saturating_sub(1)).collect::<String>())
                    } else { cmd };
                    self.push_shaped_row(&format!("  \u{2192} {}", display), RESP_FG, BLOCK_BG, resp_row, 0, w, font);
                } else {
                    let lines = word_wrap(&block.response, w.saturating_sub(4));
                    let line = format!("  {}", lines.first().cloned().unwrap_or_default());
                    self.push_shaped_row(&line, RESP_FG, BLOCK_BG, resp_row, 0, w, font);
                }
                self.push_shaped_row("  Enter: run \u{23ce}   Esc: dismiss", HINT_FG, BLOCK_BG, hint_row, 0, w, font);
            }
            AiState::Error(err) => {
                let lines = word_wrap(err, w.saturating_sub(4));
                let line = format!("  \u{2717} {}", lines.first().cloned().unwrap_or_default()); // ✗
                self.push_shaped_row(&line, ERR_FG, BLOCK_BG, resp_row, 0, w, font);
                self.push_shaped_row("  Esc: dismiss", HINT_FG, BLOCK_BG, hint_row, 0, w, font);
            }
            AiState::Hidden => {}
        }
    }

    pub fn build_palette_instances(
        &mut self,
        palette: &CommandPalette,
        font: &crate::config::schema::FontConfig,
        total_cols: usize,
        total_rows: usize,
    ) {
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

        let prompt = format!(" > {}▋", palette.query);
        self.push_shaped_row(&prompt, prompt_fg, bg, start_row, start_col, palette_width, font);

        let keybind_fg = [0.5, 0.5, 0.7, 1.0];

        for i in 0..(palette_height - 1) {
            let row = start_row + 1 + i;
            let is_selected = i == palette.selected;
            let current_bg = if is_selected { highlight_bg } else { bg };

            if let Some(action) = palette.results.get(i) {
                // Name on the left, keybind right-aligned.
                let name_text = format!("  {}", action.name);
                self.push_shaped_row(&name_text, fg, current_bg, row, start_col, palette_width, font);

                if let Some(kb) = &action.keybind {
                    // Pad keybind to right edge with one space margin.
                    let kb_display = format!("{} ", kb);
                    let kb_len = kb_display.chars().count();
                    if kb_len < palette_width {
                        let kb_col = start_col + palette_width - kb_len;
                        // Use transparent bg so name bg shows through.
                        self.push_shaped_row(&kb_display, keybind_fg, current_bg, row, kb_col, kb_len, font);
                    }
                }
            } else {
                self.push_shaped_row("", fg, current_bg, row, start_col, palette_width, font);
            }
        }
    }

    /// Render the right-click context menu as a floating popup at `menu.col/row`.
    pub fn build_context_menu_instances(
        &mut self,
        menu: &crate::ui::context_menu::ContextMenu,
        font: &crate::config::schema::FontConfig,
        total_cols: usize,
        total_rows: usize,
    ) {
        use crate::ui::context_menu::CONTEXT_MENU_WIDTH;

        if !menu.visible || menu.items.is_empty() { return; }

        let width = CONTEXT_MENU_WIDTH;
        let height = menu.items.len();

        if menu.col + width > total_cols || menu.row + height > total_rows { return; }

        let bg          = [0.05, 0.05, 0.10, 0.97];
        let fg          = [1.0,  1.0,  1.0,  1.0];
        let hover_bg    = [0.2,  0.2,  0.4,  1.0];
        let keybind_fg  = [0.5,  0.5,  0.7,  1.0];

        for (i, item) in menu.items.iter().enumerate() {
            let row = menu.row + i;
            let is_hovered = menu.hovered == Some(i);
            let current_bg = if is_hovered { hover_bg } else { bg };

            // Name on the left.
            let name_text = format!("  {}", item.label);
            self.push_shaped_row(&name_text, fg, current_bg, row, menu.col, width, font);

            // Keybind right-aligned.
            if let Some(kb) = &item.keybind {
                let kb_display = format!("{} ", kb);
                let kb_len = kb_display.chars().count();
                if kb_len < width {
                    let kb_col = menu.col + width - kb_len;
                    self.push_shaped_row(&kb_display, keybind_fg, current_bg, row, kb_col, kb_len, font);
                }
            }
        }
    }
}

impl RenderContext {
    /// Render the tab bar at grid row -1 (one cell row above the terminal).
    /// Requires `set_padding` to have shifted padding.y up by one cell_height.
    ///
    /// TD-013: Each tab is rendered as a rounded pill (via RoundedRectPipeline)
    ///         with text overlaid using transparent-bg cell instances.
    /// TD-014: The bar background comes from the window clear color (config.colors.background),
    ///         so `bar_bg` is acknowledged here but not used directly for fill.
    #[allow(clippy::too_many_arguments)]
    pub fn build_tab_bar_instances(
        &mut self,
        tabs: &[Tab],
        active_idx: usize,
        font: &crate::config::schema::FontConfig,
        total_cols: usize,
        pad_left: f32,
        pad_top: f32,
        bar_bg: [f32; 4],
    ) {
        // bar_bg is applied via the renderer clear color (TD-014); no fill needed here.
        let _ = bar_bg;

        if tabs.is_empty() || total_cols == 0 { return; }

        // Clear rect instances for this frame
        self.rect_instances.clear();

        // Dracula palette (pill colors)
        const ACTIVE_PILL:   [f32; 4] = [0.74, 0.58, 0.98, 1.0]; // Dracula purple #bd93f9
        const ACTIVE_FG:     [f32; 4] = [0.97, 0.97, 0.95, 1.0]; // near-white
        const INACTIVE_PILL: [f32; 4] = [0.27, 0.28, 0.35, 1.0]; // Dracula current-line
        const INACTIVE_FG:   [f32; 4] = [0.61, 0.64, 0.75, 1.0]; // comment gray
        let transparent = [0.0f32; 4];

        let cell_w = self.shaper.cell_width;
        let cell_h = self.shaper.cell_height;
        let radius = (cell_h / 3.0).round();
        let pill_y = pad_top + 2.0;
        let pill_h = cell_h - 4.0;

        let mut col = 0usize;

        for (i, tab) in tabs.iter().enumerate() {
            if col >= total_cols { break; }

            let is_active = i == active_idx;
            let pill_color = if is_active { ACTIVE_PILL } else { INACTIVE_PILL };
            let fg = if is_active { ACTIVE_FG } else { INACTIVE_FG };

            // 1-cell gap — window bg (clear color) shows through
            col += 1;
            if col >= total_cols { break; }

            // Badge text " N "
            let badge = format!(" {} ", i + 1);
            let badge_w = badge.chars().count().min(total_cols - col);

            // Title text " name " (max 14 chars)
            let raw = format!(" {} ", tab.title);
            let title: String = raw.chars().take(14).collect();
            let title_w = title.chars().count().min(total_cols.saturating_sub(col + badge_w));

            let pill_w = (badge_w + title_w) as f32 * cell_w;
            let pill_x = pad_left + col as f32 * cell_w;

            // Emit rounded rect for the pill background
            if pill_w > 0.0 {
                self.rect_instances.push(RoundedRectInstance {
                    rect:   [pill_x, pill_y, pill_w, pill_h],
                    color:  pill_color,
                    radius,
                    _pad:   [0.0; 3],
                });
            }

            // Badge text with transparent bg
            let badge_w_clamped = badge_w.min(total_cols - col);
            if badge_w_clamped > 0 {
                let start = self.instances.len();
                self.push_shaped_row(&badge, fg, transparent, 0, col, badge_w_clamped, font);
                for inst in &mut self.instances[start..] { inst.grid_pos[1] = -1.0; }
            }
            col += badge_w_clamped;
            if col >= total_cols { break; }

            // Title text with transparent bg
            let title_w_clamped = title_w.min(total_cols - col);
            if title_w_clamped > 0 {
                let start = self.instances.len();
                self.push_shaped_row(&title, fg, transparent, 0, col, title_w_clamped, font);
                for inst in &mut self.instances[start..] { inst.grid_pos[1] = -1.0; }
            }
            col += title_w_clamped;
        }
        // No trailing fill needed — window bg (clear color = bar_bg from TD-014) shows through
    }

    /// Render a scroll bar on the right edge of the terminal (overlays rightmost ~6px of the
    /// last terminal column). Only emits instances when history_size > 0.
    pub fn build_scroll_bar_instances(
        &mut self,
        display_offset: usize,
        history_size: usize,
        screen_rows: usize,
        term_cols: usize,
    ) {
        if history_size == 0 || screen_rows == 0 || term_cols == 0 { return; }

        const SCROLLBAR_PX: f32 = 6.0;
        const TRACK_COLOR: [f32; 4] = [0.18, 0.17, 0.24, 1.0];
        const THUMB_COLOR: [f32; 4] = [0.40, 0.37, 0.55, 1.0];

        let cell_w = self.shaper.cell_width;
        let cell_h = self.shaper.cell_height;

        // Thumb height: proportional to visible rows vs total (visible + history)
        let total_lines = screen_rows + history_size;
        let thumb_rows = (((screen_rows as f32 / total_lines as f32) * screen_rows as f32)
            .max(1.0)
            .ceil() as usize)
            .min(screen_rows);

        // Thumb position: display_offset=0 → thumb at bottom, display_offset=max → thumb at top
        let slack = screen_rows.saturating_sub(thumb_rows);
        let scroll_frac = (display_offset as f32 / history_size as f32).clamp(0.0, 1.0);
        let thumb_start = ((1.0 - scroll_frac) * slack as f32).round() as usize;
        let thumb_end = thumb_start + thumb_rows;

        let col = (term_cols - 1) as f32;
        let glyph_offset = [cell_w - SCROLLBAR_PX, 0.0];
        let glyph_size   = [SCROLLBAR_PX, cell_h];

        for row in 0..screen_rows {
            let color = if row >= thumb_start && row < thumb_end { THUMB_COLOR } else { TRACK_COLOR };
            self.instances.push(CellVertex {
                grid_pos:     [col, row as f32],
                atlas_uv:     [0.0; 4],
                fg:           [0.0; 4],
                bg:           color,
                glyph_offset,
                glyph_size,
                flags:        FLAG_CURSOR,
                _pad:         0,
            });
        }
    }
}

fn calculate_row_hash(text: &str, colors: &[([f32; 4], [f32; 4])]) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    text.hash(&mut hasher);
    for (fg, bg) in colors {
        ((fg[0] * 255.0) as u32).hash(&mut hasher);
        ((bg[0] * 255.0) as u32).hash(&mut hasher);
    }
    hasher.finish()
}
