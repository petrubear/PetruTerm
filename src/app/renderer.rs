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
use crate::ui::search_bar::SearchBar;

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
}

impl RowCache {
    pub fn new() -> Self {
        Self { rows: Vec::new() }
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
    /// Capture of `term_cols` when panel cache was built. If it changes, mark panel dirty.
    pub panel_cache_term_cols: usize,
    /// Scratch buffer for `collect_grid_cells_for` — reused across frames (TD-PERF-12).
    pub cell_data_scratch: Vec<(String, Vec<(alacritty_terminal::vte::ansi::Color, alacritty_terminal::vte::ansi::Color)>)>,
    /// Scratch buffers for `push_shaped_row` — reused per call to avoid hot-path allocs (TD-PERF-13).
    pub scratch_chars: Vec<char>,
    pub scratch_str: String,
    pub scratch_colors: Vec<([f32; 4], [f32; 4])>,
    /// Per-pane color resolve scratch — avoids Vec alloc per pane per frame (TD-PERF-32).
    pub colors_scratch: Vec<([f32; 4], [f32; 4])>,
    /// Incremental streaming wrap cache — avoids re-wrapping the full buf each token (TD-PERF-37).
    streaming_stable_lines: Vec<String>,
    /// Byte offset in streaming_buf up to which streaming_stable_lines is valid.
    streaming_stable_end: usize,
    /// Panel id and width used for the current streaming cache entry.
    streaming_cache_key: Option<(usize, usize)>,
    /// General-purpose format scratch for callers of `push_shaped_row` (TD-PERF-13).
    /// Kept separate from `scratch_str` (used inside push_shaped_row) to avoid borrow conflicts.
    pub fmt_buf: String,
    /// Reusable line buffer for `build_chat_panel_instances` — avoids Vec realloc per rebuild.
    /// Strings inside are reused across frames when capacity permits (TD-PERF-13).
    pub scratch_lines: Vec<(String, [f32; 4])>,
    /// Incremented each rendered frame; used for spinner animation to avoid O(n) chars().count().
    pub frame_counter: u64,
    /// Rounded rect instances for the tab bar pills and status bar background.
    pub rect_instances: Vec<RoundedRectInstance>,

    // ── Cursor overlay (fast blink path) ─────────────────────────────────────
    /// Number of non-cursor instances after the last full frame build.
    /// The cursor vertex is always appended at this slot so it can be
    /// updated in-place on blink without rebuilding the whole cell buffer.
    pub content_end: usize,
    /// Cursor vertex template (blink=on state) from the last full frame.
    /// None when the cursor is hidden or shape is Hidden.
    /// Reused on blink-only fast renders to avoid a full rebuild.
    pub cursor_vertex_template: Option<CellVertex>,

    // ── Debug HUD (F12) ───────────────────────────────────────────────────────
    pub hud_visible: bool,
    /// Ring buffer of the last 120 frame times in milliseconds.
    pub frame_times: std::collections::VecDeque<f32>,
    pub shape_cache_hits: u64,
    pub shape_cache_misses: u64,
    pub last_instance_count: usize,
    /// Bytes written to GPU buffers in the current frame (instances + LCD + rects).
    pub last_gpu_upload_bytes: usize,

    // ── Static-geometry caches (TD-PERF-08/09/10) ────────────────────────────
    // Scroll bar: ~50 CellVertex per frame, no HarfBuzz. Keyed by scroll state.
    pub scroll_bar_state: Option<(usize, usize, usize, usize)>,
    pub scroll_bar_cache: Vec<CellVertex>,
    // Tab bar: HarfBuzz per tab name. Keyed by hash of tab titles + layout inputs.
    pub tab_bar_key: u64,
    pub tab_bar_instances_cache: Vec<CellVertex>,
    pub tab_bar_rects_cache: Vec<RoundedRectInstance>,
    // Status bar: HarfBuzz per segment. Keyed by hash of all segment inputs.
    pub status_bar_key: u64,
    pub status_bar_instances_cache: Vec<CellVertex>,
    pub status_bar_rect_cache: Vec<RoundedRectInstance>,
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

        let mut shaper = TextShaper::new(Some(&renderer.device()), font_system, actual_family, face_id, font_path, &scaled_font, lcd_atlas);
        
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
            panel_cache_term_cols: 0,
            panel_instances_cache: Vec::new(),
            cell_data_scratch: Vec::new(),
            scratch_chars: Vec::new(),
            scratch_str: String::new(),
            scratch_colors: Vec::new(),
            colors_scratch: Vec::new(),
            streaming_stable_lines: Vec::new(),
            streaming_stable_end: 0,
            streaming_cache_key: None,
            fmt_buf: String::new(),
            scratch_lines: Vec::new(),
            frame_counter: 0,
            rect_instances: Vec::new(),
            hud_visible: false,
            frame_times: std::collections::VecDeque::new(),
            shape_cache_hits: 0,
            shape_cache_misses: 0,
            last_instance_count: 0,
            last_gpu_upload_bytes: 0,
            scroll_bar_state: None,
            scroll_bar_cache: Vec::new(),
            tab_bar_key: 0,
            tab_bar_instances_cache: Vec::new(),
            tab_bar_rects_cache: Vec::new(),
            status_bar_key: 0,
            status_bar_instances_cache: Vec::new(),
            status_bar_rect_cache: Vec::new(),
            content_end: 0,
            cursor_vertex_template: None,
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

    /// Drop all per-terminal row caches (used after atlas eviction).
    pub fn clear_all_row_caches(&mut self) {
        self.row_caches.clear();
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
        terminal_id: usize,
        col_offset: usize,
        row_offset: usize,
    ) -> Result<(), crate::renderer::atlas::AtlasError> {
        #[cfg(feature = "profiling")]
        let _span = tracing::info_span!("build_instances", rows = cell_data.len()).entered();

        // Retrieve or create the per-terminal row cache.
        let cache = self.row_caches.entry(terminal_id).or_insert_with(RowCache::new);
        if cache.rows.len() < cell_data.len() {
            cache.rows.resize(cell_data.len(), None);
        }

        for (row_idx, (text, raw_colors)) in cell_data.iter().enumerate() {
            self.colors_scratch.clear();
            self.colors_scratch.extend(raw_colors.iter().map(|(fg, bg)| {
                (
                    resolve_color(*fg, &config.colors),
                    resolve_color(*bg, &config.colors),
                )
            }));
            let colors: &[([f32; 4], [f32; 4])] = &self.colors_scratch;

            let row_hash = calculate_row_hash(text, colors);

            // Cache hit: copy local-coordinate instances and apply pane offset.
            if let Some(Some(entry)) = self.row_caches.get(&terminal_id).and_then(|c| c.rows.get(row_idx)) {
                if entry.hash == row_hash {
                    self.shape_cache_hits = self.shape_cache_hits.saturating_add(1);
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
            self.shape_cache_misses = self.shape_cache_misses.saturating_add(1);

            // Cache miss: shape and rasterize.
            let mut row_instances: Vec<CellVertex> = Vec::new();
            let mut row_lcd_instances: Vec<CellVertex> = Vec::new();

            let shaped = self.shaper.shape_line(text, colors, font);

            let default_bg = config.colors.background;

            // BG pre-pass: emit a background-only vertex for every cell whose bg
            // differs from the default. The shaper's word-cached and HarfBuzz
            // paths drop space runs, so without this pass any space cell with a
            // non-default bg (widget backgrounds, status/command lines, selection,
            // search highlight, etc.) would show the GPU clear colour instead of
            // its real bg, producing horizontal "stripes" between letters and rows.
            for (col, (_fg, bg)) in colors.iter().enumerate() {
                if colors_approx_eq(*bg, default_bg) { continue; }
                row_instances.push(CellVertex {
                    grid_pos: [col as f32, row_idx as f32],
                    atlas_uv: [0.0; 4],
                    fg: [0.0; 4],
                    bg: *bg,
                    glyph_offset: [0.0; 2],
                    glyph_size: [0.0; 2],
                    flags: 0,
                    _pad: 0,
                });
            }

            for glyph in &shaped.glyphs {
                // Fast path: space cells with the default background color produce no
                // visible glyph and the GPU clear already fills them with the correct
                // color — skip the vertex entirely. This avoids an atlas lookup and a
                // vertex upload for every trailing space on every terminal line.
                if glyph.ch == ' ' && colors_approx_eq(glyph.bg, default_bg) {
                    continue;
                }

                let lcd_entry = if let Some(queue) = self.renderer.lcd_queue() {
                    self.shaper.rasterize_lcd_to_atlas(glyph.cache_key, glyph.ch, queue)
                } else {
                    None
                };

                // Skip Swash rasterization when LCD succeeded — saves rasterization + atlas
                // upload for every text glyph on a cache miss. Color emoji never produce an
                // LCD entry and always fall through to the Swash path (TD-PERF-06).
                let (atlas_uv, glyph_offset, glyph_size, color_flag) = if lcd_entry.is_none() {
                    let (atlas, queue) = self.renderer.atlas_and_queue();
                    let se = self.shaper.rasterize_to_atlas(glyph.cache_key, atlas, queue)?;
                    let ox = se.bearing_x as f32;
                    let oy = shaped.ascent - se.bearing_y as f32;
                    let gw = se.width as f32;
                    let gh = se.height as f32;
                    let y0 = oy.max(0.0);
                    let y1 = (oy + gh).min(self.shaper.cell_height);
                    let flag = if se.is_color { FLAG_COLOR_GLYPH } else { 0 };
                    if y1 <= y0 || gw == 0.0 || gh == 0.0 {
                        ([0.0f32; 4], [0.0f32; 2], [0.0f32; 2], flag)
                    } else {
                        let fy0 = (y0 - oy) / gh;
                        let fy1 = (y1 - oy) / gh;
                        let [u0, v0, u1, v1] = se.uv;
                        ([u0, v0 + fy0 * (v1 - v0), u1, v0 + fy1 * (v1 - v0)], [ox, y0], [gw, y1 - y0], flag)
                    }
                } else {
                    // LCD path: emit a background-only vertex (zeroed UVs; GPU reads bg color).
                    ([0.0f32; 4], [0.0f32; 2], [0.0f32; 2], 0)
                };
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

        Ok(())
    }

    /// Emit the cursor vertex for the focused terminal pane.
    ///
    /// Must be called AFTER `build_instances` for all panes and BEFORE any overlay
    /// instances so that `content_end` accurately marks the cell/cursor boundary.
    /// Stores a blink-on template in `cursor_vertex_template` for the fast blink path.
    pub fn build_cursor_instance(
        &mut self,
        info: &CursorInfo,
        blink_on: bool,
        col_offset: usize,
        row_offset: usize,
        config: &Config,
    ) {
        let cw = self.shaper.cell_width;
        let ch = self.shaper.cell_height;
        let (glyph_offset, glyph_size) = match info.shape {
            CursorShape::Block | CursorShape::HollowBlock => ([0.0f32, 0.0], [cw, ch]),
            CursorShape::Underline => ([0.0, (ch - 2.0).max(0.0)], [cw, 2.0]),
            CursorShape::Beam      => ([0.0, 0.0], [2.0, ch]),
            CursorShape::Hidden    => { self.cursor_vertex_template = None; return; }
        };
        if !info.visible { self.cursor_vertex_template = None; return; }
        let v = CellVertex {
            grid_pos:     [(col_offset + info.col) as f32, (row_offset + info.row) as f32],
            atlas_uv:     [0.0; 4],
            fg:           config.colors.cursor_fg,
            bg:           config.colors.cursor_bg,
            glyph_offset,
            glyph_size,
            flags:        FLAG_CURSOR,
            _pad:         0,
        };
        self.cursor_vertex_template = Some(v);
        if blink_on {
            self.instances.push(v);
        }
    }

    /// Draw 1-pixel separator lines between panes.
    ///
    /// Each separator is a single `RoundedRectInstance` (1×N or N×1 pixels) instead of
    /// emitting one `CellVertex` per row/column. `pad_x`/`pad_y` are the physical-pixel
    /// offsets applied by the cell shader uniform (window padding + tab bar height).
    pub fn build_pane_separators(&mut self, separators: &[PaneSeparator], pad_x: f32, pad_y: f32) {
        const SEP_COLOR: [f32; 4] = [0.35, 0.30, 0.48, 1.0]; // dim purple
        let ch = self.shaper.cell_height;
        let cw = self.shaper.cell_width;
        for sep in separators {
            let x = pad_x + sep.col as f32 * cw;
            let y = pad_y + sep.row as f32 * ch;
            let (w, h) = if sep.vertical {
                (1.0_f32, sep.length as f32 * ch)
            } else {
                (sep.length as f32 * cw, 1.0_f32)
            };
            self.rect_instances.push(RoundedRectInstance {
                rect:   [x, y, w, h],
                color:  SEP_COLOR,
                radius: 0.0,
                _pad:   [0.0; 3],
            });
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

        // Step 1: push one background-coverage vertex per cell.
        // This guarantees full-row background even when shape_line collapses
        // space runs into a single glyph entry (skipping intermediate cells).
        // In the bg_pipeline these produce full-cell rects; in the glyph_pipeline
        // they produce zero-area quads that are immediately discarded.
        for i in 0..width {
            self.instances.push(CellVertex {
                grid_pos: [(col_offset + i) as f32, row as f32],
                atlas_uv: [0.0; 4],
                fg,
                bg,
                glyph_offset: [0.0; 2],
                glyph_size:   [0.0; 2],
                flags: 0,
                _pad: 0,
            });
        }

        // Step 2: push glyph vertices for visible characters.
        // Reuse scratch buffers to avoid per-call allocations (TD-PERF-13).
        // We take ownership temporarily so the borrow checker allows calling &mut self methods below.
        let mut scratch_chars = std::mem::take(&mut self.scratch_chars);
        let mut scratch_str = std::mem::take(&mut self.scratch_str);
        let mut scratch_colors = std::mem::take(&mut self.scratch_colors);

        scratch_chars.clear();
        scratch_chars.extend(text.chars().take(width));
        let len = scratch_chars.len();
        scratch_str.clear();
        scratch_str.extend(scratch_chars.iter().copied().chain(std::iter::repeat_n(' ', width.saturating_sub(len))));

        scratch_colors.clear();
        scratch_colors.extend((0..width).map(|_| (fg, bg)));

        let shaped = self.shaper.shape_line(&scratch_str, &scratch_colors, font);

        // Restore scratch buffers.
        self.scratch_chars = scratch_chars;
        self.scratch_str = scratch_str;
        self.scratch_colors = scratch_colors;

        for glyph in shaped.glyphs {
            if glyph.col >= width { continue; }

            let (atlas, queue) = self.renderer.atlas_and_queue();
            let entry = match self.shaper.rasterize_to_atlas(glyph.cache_key, atlas, queue) {
                Ok(e) => e,
                Err(_) => continue, // skip; bg-coverage vertex already pushed above
            };

            let ox = entry.bearing_x as f32;
            let oy = shaped.ascent - entry.bearing_y as f32;
            let gw = entry.width as f32;
            let gh = entry.height as f32;

            // Skip zero-size glyphs (spaces): bg-coverage vertex already handles bg.
            if gw == 0.0 || gh == 0.0 { continue; }

            let y0 = oy.max(0.0);
            let y1 = (oy + gh).min(self.shaper.cell_height);
            if y1 <= y0 { continue; }

            let fy0 = (y0 - oy) / gh;
            let fy1 = (y1 - oy) / gh;
            let [u0, v0, u1, v1] = entry.uv;
            let atlas_uv    = [u0, v0 + fy0 * (v1 - v0), u1, v0 + fy1 * (v1 - v0)];
            let glyph_offset = [ox, y0];
            let glyph_size   = [gw, y1 - y0];

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
        panel_id: usize,
        panel_focused: bool,
        file_picker_focused: bool,
        config: &Config,
        font: &crate::config::schema::FontConfig,
        term_cols: usize,
        screen_rows: usize,
        cursor_blink_on: bool,
    ) {
        use crate::llm::chat_panel::{word_wrap, ConfirmDisplay, MAX_FILE_ROWS, PanelState};
        use crate::llm::diff::DiffKind;
        use crate::llm::ChatRole;
        use std::fmt::Write as _;

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
        const ERR_FG:     [f32; 4] = [1.00, 0.33, 0.33, 1.0]; // red
        const SEP_FG:     [f32; 4] = [0.27, 0.28, 0.36, 1.0]; // current-line
        const DIM_FG:     [f32; 4] = [0.50, 0.47, 0.60, 1.0]; // dimmed (file list)
        const RUN_FG:     [f32; 4] = [0.50, 0.98, 0.60, 1.0]; // green — run bar
        const FILE_FG:    [f32; 4] = [0.78, 0.92, 0.65, 1.0]; // light green — attached files
        const PICK_SEL:   [f32; 4] = [0.58, 0.50, 1.00, 1.0]; // purple — picker highlight
        const PICK_FG:    [f32; 4] = [0.80, 0.80, 0.90, 1.0]; // soft white — picker items

        const SPIN: [&str; 8] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧"];
        let spin = SPIN[(self.frame_counter / 4) as usize % 8];

        let co = term_cols; // grid column where panel begins
        let border_fg = if panel_focused { BORDER_FG } else { BORDER_DIM };

        // ── Fixed bottom rows (always present) ───────────────────────────────
        // input_row1/2 and hints_row are rendered by build_chat_panel_input_rows (TD-PERF-10).
        let sep_row = screen_rows - 4;

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
        {
            let mut buf = std::mem::take(&mut self.fmt_buf);
            buf.clear();
            let _ = write!(buf, "{}{}{}", left, title, "─".repeat(dashes));
            self.push_shaped_row(&buf, border_fg, panel_bg, 0, co, panel_cols, font);
            self.fmt_buf = buf;
        }

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
                    const WARN_FG: [f32; 4] = [1.00, 0.72, 0.20, 1.0]; // amber
                    // Detect potentially destructive patterns (TD-034).
                    let is_risky = ["rm ", "rm\t", "rm -", ":(){", "dd ", "mkfs",
                                    "curl | sh", "curl|sh", "wget | sh", "wget|sh",
                                    "chmod -R 777", "> /dev/"]
                        .iter().any(|p| cmd.contains(p));
                    let (title, title_fg) = if is_risky {
                        ("│ \u{26a0} Run command (destructive):", WARN_FG)
                    } else {
                        ("│ Run command:", BORDER_FG)
                    };
                    // Row 1: title
                    self.push_shaped_row(title, title_fg, panel_bg, 1, co, panel_cols, font);
                    // Row 2: command
                    let max_cmd = panel_cols.saturating_sub(5);
                    let cmd_trunc = cmd.char_indices().nth(max_cmd).map(|(i, _)| &cmd[..i]).unwrap_or(cmd);
                    let cmd_line = format!("│   {}", cmd_trunc);
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
                    let trimmed = if let Some((i, _)) = name.char_indices().nth(max_w) {
                        let cut = name.char_indices().nth(max_w.saturating_sub(1)).map(|(j, _)| j).unwrap_or(i);
                        format!("{}…", &name[..cut])
                    } else { name };
                    let line = format!("│   {}", trimmed);
                    self.push_shaped_row(&line, DIM_FG, panel_bg, 2 + i, co, panel_cols, font);
                }
                // Thin separator after file section (use pre-built cache from ChatPanel — TD-PERF-13)
                self.push_shaped_row(&panel.thin_separator_cache, SEP_FG, panel_bg, 1 + file_section_rows, co, panel_cols, font);
            }

            // History area: rows after file section up to sep_row
            let history_start_row = 1 + if file_section_rows > 0 { file_section_rows + 1 } else { 0 };
            let history_rows = sep_row.saturating_sub(history_start_row);
            let msg_inner_w = panel_cols.saturating_sub(8);

            // Reuse scratch_lines across frames — Vec capacity is kept, String capacity reused
            // when the line count is stable (common case). Avoids ~N allocs per rebuild (TD-PERF-13).
            let mut all_lines = std::mem::take(&mut self.scratch_lines);
            let mut line_idx: usize = 0;

            // Helper: write `prefix + content` into all_lines[line_idx], reusing String capacity.
            macro_rules! push_line {
                ($prefix:expr, $content:expr, $color:expr) => {{
                    let p: &str = $prefix;
                    let c: &str = $content;
                    if line_idx < all_lines.len() {
                        let (s, col) = &mut all_lines[line_idx];
                        s.clear();
                        s.push_str(p);
                        s.push_str(c);
                        *col = $color;
                    } else {
                        let mut s = String::with_capacity(p.len() + c.len());
                        s.push_str(p);
                        s.push_str(c);
                        all_lines.push((s, $color));
                    }
                    line_idx += 1;
                }};
            }

            // Use pre-wrapped lines from the cache (TD-PERF-05).
            // ensure_wrap_cache() is called in mod.rs before this function runs.
            for (msg_idx, msg) in panel.messages.iter().enumerate() {
                let (first_p, cont_p, fg) = match msg.role {
                    ChatRole::User      => ("│  You  ", "│       ", user_fg),
                    ChatRole::Assistant => ("│   AI  ", "│       ", asst_fg),
                    ChatRole::System    => continue,
                    ChatRole::Tool(_)   => continue,
                };
                for (i, line) in panel.wrapped_message(msg_idx).iter().enumerate() {
                    let p = if i == 0 { first_p } else { cont_p };
                    push_line!(p, line.as_str(), fg);
                }
                push_line!("│", "", SEP_FG);
            }

            if panel.is_streaming() && !panel.streaming_buf.is_empty() {
                let buf = &panel.streaming_buf;
                let cache_key = (panel_id, msg_inner_w);

                // Invalidate if panel or width changed, or buf was reset (new query).
                if self.streaming_cache_key != Some(cache_key)
                    || self.streaming_stable_end > buf.len()
                {
                    self.streaming_stable_lines.clear();
                    self.streaming_stable_end = 0;
                    self.streaming_cache_key = Some(cache_key);
                }

                // Advance stable prefix to the end of the last complete line (TD-PERF-37).
                let new_stable_end = buf[self.streaming_stable_end..]
                    .rfind('\n')
                    .map(|i| self.streaming_stable_end + i + 1)
                    .unwrap_or(self.streaming_stable_end);

                if new_stable_end > self.streaming_stable_end {
                    let seg = &buf[self.streaming_stable_end..new_stable_end];
                    self.streaming_stable_lines.extend(word_wrap(seg, msg_inner_w));
                    self.streaming_stable_end = new_stable_end;
                }

                // Re-wrap only the partial last line (no newline yet) — O(partial_len).
                let partial = &buf[self.streaming_stable_end..];
                let partial_lines = if partial.is_empty() { vec![] } else { word_wrap(partial, msg_inner_w) };

                let all_lines = self.streaming_stable_lines.iter().map(|s| s.as_str())
                    .chain(partial_lines.iter().map(|s| s.as_str()));
                for (i, line) in all_lines.enumerate() {
                    let p = if i == 0 { "│   AI  " } else { "│       " };
                    push_line!(p, line, STREAM_FG);
                }
            }

            if matches!(panel.state, PanelState::Loading) {
                let mut buf = std::mem::take(&mut self.fmt_buf);
                buf.clear();
                let _ = std::fmt::write(&mut buf, format_args!("│   {}  Thinking\u{2026}", spin));
                push_line!("", buf.as_str(), STREAM_FG);
                self.fmt_buf = buf;
            }

            if let PanelState::Error(ref err) = panel.state {
                let wrapped = word_wrap(err, msg_inner_w);
                for (i, line) in wrapped.iter().enumerate() {
                    let p = if i == 0 { "│  \u{2717}    " } else { "│       " };
                    push_line!(p, line.as_str(), ERR_FG);
                }
            }

            if panel.is_idle() {
                if let Some(cmd) = panel.last_assistant_command() {
                    let max_cmd_w = panel_cols.saturating_sub(5);
                    push_line!("│", "", SEP_FG);
                    let mut buf = std::mem::take(&mut self.fmt_buf);
                    buf.clear();
                    buf.push_str("│ \u{23ce}  ");
                    let cmd_chars: usize = cmd.chars().count();
                    if cmd_chars > max_cmd_w {
                        let end = cmd.char_indices().nth(max_cmd_w.saturating_sub(1))
                            .map(|(i, _)| i).unwrap_or(cmd.len());
                        buf.push_str(&cmd[..end]);
                        buf.push('…');
                    } else {
                        buf.push_str(&cmd);
                    }
                    push_line!("", buf.as_str(), RUN_FG);
                    self.fmt_buf = buf;
                }
            }

            let total_lines = line_idx;
            // Shrink logical length without dropping capacity.
            all_lines.truncate(total_lines);

            let visible_start = total_lines
                .saturating_sub(history_rows + panel.scroll_offset);

            for i in 0..history_rows {
                let row = history_start_row + i;
                let (text, fg) = all_lines
                    .get(visible_start + i)
                    .map(|(t, f)| (t.as_str(), *f))
                    .unwrap_or(("│", SEP_FG));
                self.push_shaped_row(text, fg, panel_bg, row, co, panel_cols, font);
            }

            self.scratch_lines = all_lines;
        }

        // ── Separator (use pre-built cache from ChatPanel — TD-PERF-13) ─────
        self.push_shaped_row(&panel.separator_cache, SEP_FG, panel_bg, sep_row, co, panel_cols, font);
    }

    /// Build only the input field and key-hint row for the chat panel.
    ///
    /// Called every frame when the panel is visible, regardless of `ChatPanel::dirty`.
    /// The content section (header, messages, separator) is cached separately, so cursor
    /// blink no longer triggers a full reshape of message history (TD-PERF-10).
    #[allow(clippy::too_many_arguments)]
    pub fn build_chat_panel_input_rows(
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
        use crate::llm::chat_panel::{wrap_input, ConfirmDisplay, PanelState};
        use crate::llm::ChatRole;

        let panel_cols = panel.width_cols as usize;
        if panel_cols == 0 || screen_rows < 6 { return; }

        let panel_bg  = config.llm.ui.background;
        let input_fg  = config.llm.ui.input_fg;

        const HINT_FG:    [f32; 4] = [0.38, 0.44, 0.64, 1.0];
        const DIM_FG:     [f32; 4] = [0.50, 0.47, 0.60, 1.0];

        let co         = term_cols;
        let hints_row  = screen_rows - 1;
        let input_row2 = screen_rows - 2;
        let input_row1 = screen_rows - 3;

        // ── Input field (or confirmation prompt) ─────────────────────────────
        if matches!(panel.state, PanelState::AwaitingConfirm) {
            let confirm_kind = match panel.confirm_display.as_ref() {
                Some(ConfirmDisplay::Run { .. }) => "run",
                _ => "write",
            };
            let (yes_label, no_label) = if confirm_kind == "run" {
                ("[y] Run", "[n] Cancel")
            } else {
                ("[y] Apply", "[n] Reject")
            };
            const CONFIRM_YES: [f32; 4] = [0.50, 0.98, 0.60, 1.0];
            const CONFIRM_NO:  [f32; 4] = [1.00, 0.47, 0.47, 1.0];
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
            let n = input_lines.len();
            let (vis1, vis2) = if n >= 2 {
                (input_lines[n - 2].clone(), input_lines[n - 1].clone())
            } else {
                (input_lines.first().cloned().unwrap_or_default(), String::new())
            };
            let line1 = format!("│ \u{25b8}  {}", vis1);
            let line2 = format!("│    {}", vis2);
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
        let spin = SPIN[(self.frame_counter / 4) as usize % 8];

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
                    let display = if let Some((i, _)) = cmd.char_indices().nth(max_cmd) {
                        let cut = cmd.char_indices().nth(max_cmd.saturating_sub(1)).map(|(j, _)| j).unwrap_or(i);
                        format!("{}…", &cmd[..cut])
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

        let max_visible = palette_height - 1; // rows available for results (below query row)
        // Keep selected item in view: scroll down when it goes past the last visible row.
        let scroll_offset = if palette.selected >= max_visible {
            palette.selected - max_visible + 1
        } else {
            0
        };

        for i in 0..max_visible {
            let result_idx = scroll_offset + i;
            let row = start_row + 1 + i;
            let is_selected = result_idx == palette.selected;
            let current_bg = if is_selected { highlight_bg } else { bg };

            if let Some(action) = palette.results.get(result_idx) {
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

        let sep_fg = [0.3, 0.3, 0.5, 1.0];

        let label_fg: [f32; 4] = [0.65, 0.65, 0.80, 1.0]; // dim, non-interactive

        for (i, item) in menu.items.iter().enumerate() {
            let row = menu.row + i;

            if item.is_separator() {
                // Render a full-width horizontal rule.
                let rule = "─".repeat(width);
                self.push_shaped_row(&rule, sep_fg, bg, row, menu.col, width, font);
                continue;
            }

            if item.action == crate::ui::context_menu::ContextAction::Label {
                let label_text = format!("  {}", item.label);
                self.push_shaped_row(&label_text, label_fg, bg, row, menu.col, width, font);
                continue;
            }

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
    /// Render the status bar as a 1-row strip with a visual height extension below it.
    ///
    /// `row` is the terminal grid row index where the bar text appears (= `total_rows`).
    /// `pad_y` and `win_w` are used to render a full-width background rect that extends
    /// `SB_EXTRA_PX` physical pixels below the cell row, making the bar look taller.
    ///
    /// Left segments are rendered with › separators; right segments are
    /// right-aligned with │ separators. The gap between them fills with `bar_bg`.
    pub fn build_status_bar_instances(
        &mut self,
        bar: &crate::ui::status_bar::StatusBar,
        font: &crate::config::schema::FontConfig,
        total_cols: usize,
        row: usize,
        pad_y: f32,
        win_w: f32,
    ) {
        use crate::ui::status_bar::StatusBar;

        const SB_EXTRA_PX: f32 = 8.0;

        let bar_bg  = StatusBar::bar_bg();

        // Full-width background rect: covers the cell row + SB_EXTRA_PX extension below.
        // Renders before cell backgrounds (rect pass is first), filling left/right padding
        // areas and the extension strip with the bar's background color.
        {
            let cell_h = self.shaper.cell_height;
            let bar_y  = pad_y + row as f32 * cell_h;
            self.rect_instances.push(crate::renderer::rounded_rect::RoundedRectInstance {
                rect:   [0.0, bar_y, win_w, cell_h + SB_EXTRA_PX],
                color:  bar_bg,
                radius: 0.0,
                _pad:   [0.0; 3],
            });
        }
        use crate::config::schema::StatusBarStyle;
        let powerline = bar.style == StatusBarStyle::Powerline;
        let plain_sep_fg = [0.40, 0.40, 0.55, 1.0];

        // ── Left side ────────────────────────────────────────────────────────
        let mut col = 0usize;
        for (i, seg) in bar.left.iter().enumerate() {
            let text = &seg.text;
            let len = text.chars().count();
            if col + len > total_cols { break; }
            self.push_shaped_row(text, seg.fg, seg.bg, row, col, len, font);
            col += len;

            // Separator between segments (not after last).
            if i + 1 < bar.left.len() {
                let next_bg = bar.left[i + 1].bg;
                if powerline {
                    // Powerline: "" with fg = current segment bg, bg = next segment bg.
                    let arrow = StatusBar::pl_left_arrow();
                    if col + 1 > total_cols { break; }
                    self.push_shaped_row(arrow, seg.bg, next_bg, row, col, 1, font);
                    col += 1;
                } else {
                    let sep = " › ";
                    let sep_len = sep.chars().count();
                    if col + sep_len > total_cols { break; }
                    self.push_shaped_row(sep, plain_sep_fg, next_bg, row, col, sep_len, font);
                    col += sep_len;
                }
            }
        }

        // ── Right side (compute total width first, then render right-aligned) ─
        let rsep_w = bar.right_sep_width();
        // In Powerline mode a leading "" transitions from bar_bg to the first right segment.
        let leading_arrow = powerline && !bar.right.is_empty();
        let right_total: usize =
            (if leading_arrow { 1 } else { 0 })
            + bar.right.iter().map(|s| s.text.chars().count()).sum::<usize>()
            + bar.right.len().saturating_sub(1) * rsep_w;

        let right_start = total_cols.saturating_sub(right_total);

        // Fill gap between left and right with bar_bg.
        if right_start > col {
            let gap = right_start - col;
            self.push_shaped_row(&" ".repeat(gap), bar_bg, bar_bg, row, col, gap, font);
        }

        let mut rcol = right_start;

        // Powerline leading arrow before first right segment.
        if leading_arrow {
            let first_bg = bar.right[0].bg;
            self.push_shaped_row(StatusBar::pl_right_arrow(), first_bg, bar_bg, row, rcol, 1, font);
            rcol += 1;
        }

        for (i, seg) in bar.right.iter().enumerate() {
            let text = &seg.text;
            let len = text.chars().count();
            if rcol + len > total_cols { break; }
            self.push_shaped_row(text, seg.fg, seg.bg, row, rcol, len, font);
            rcol += len;

            if i + 1 < bar.right.len() {
                if powerline {
                    // Powerline: "" with fg = next segment bg, bg = current segment bg.
                    let next_bg = bar.right[i + 1].bg;
                    if rcol + 1 > total_cols { break; }
                    self.push_shaped_row(StatusBar::pl_right_arrow(), next_bg, seg.bg, row, rcol, 1, font);
                    rcol += 1;
                } else {
                    if rcol + rsep_w > total_cols { break; }
                    self.push_shaped_row(" │ ", plain_sep_fg, bar_bg, row, rcol, rsep_w, font);
                    rcol += rsep_w;
                }
            }
        }
    }

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
        // When `Some`, the active tab pill shows this input string with a cursor instead of its title.
        rename_input: Option<&str>,
    ) {
        // bar_bg is applied via the renderer clear color (TD-014); no fill needed here.
        let _ = bar_bg;

        if tabs.is_empty() || total_cols == 0 { return; }

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

            // Title text " name " (max 14 chars); rename prompt replaces title for the active tab.
            let raw = if is_active {
                if let Some(input) = rename_input {
                    format!(" {}▌ ", input)
                } else {
                    format!(" {} ", tab.title)
                }
            } else {
                format!(" {} ", tab.title)
            };
            let title: String = raw.chars().take(16).collect(); // +2 for rename cursor
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

/// Pack an `[f32; 4]` RGBA color into a `u32` by quantizing each channel to 8 bits.
/// Used by `calculate_row_hash` to avoid hashing raw float bytes (which can differ
/// for semantically identical colors due to NaN/subnormal representations).
#[inline]
fn pack_color(c: [f32; 4]) -> u32 {
    let r = (c[0].clamp(0.0, 1.0) * 255.0) as u32;
    let g = (c[1].clamp(0.0, 1.0) * 255.0) as u32;
    let b = (c[2].clamp(0.0, 1.0) * 255.0) as u32;
    let a = (c[3].clamp(0.0, 1.0) * 255.0) as u32;
    (r << 24) | (g << 16) | (b << 8) | a
}

/// Approximate color equality using 8-bit quantization (same as `pack_color`).
#[inline]
fn colors_approx_eq(a: [f32; 4], b: [f32; 4]) -> bool {
    pack_color(a) == pack_color(b)
}

fn calculate_row_hash(text: &str, colors: &[([f32; 4], [f32; 4])]) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = rustc_hash::FxHasher::default();
    text.hash(&mut hasher);
    for (fg, bg) in colors {
        // Hash all 4 channels of each color packed into a u32 (not just red).
        pack_color(*fg).hash(&mut hasher);
        pack_color(*bg).hash(&mut hasher);
    }
    hasher.finish()
}

// ── Search bar overlay ────────────────────────────────────────────────────────

impl RenderContext {
    /// Render a 1-row search bar overlay at the top-right corner of the terminal.
    ///
    /// Shows: `  / query /  N / M  ↑↓ esc `
    /// Width adapts to the query length with a minimum of 24 columns.
    pub fn build_search_bar_instances(
        &mut self,
        search: &SearchBar,
        font: &crate::config::schema::FontConfig,
        total_cols: usize,
        total_rows: usize,
    ) {
        if total_cols == 0 || total_rows == 0 { return; }

        const BAR_BG:    [f32; 4] = [0.22, 0.22, 0.30, 1.0]; // subdued dark
        const QUERY_FG:  [f32; 4] = [0.97, 0.97, 0.95, 1.0]; // near-white
        const COUNT_FG:  [f32; 4] = [0.95, 0.98, 0.55, 1.0]; // Dracula yellow
        const HINT_FG:   [f32; 4] = [0.38, 0.44, 0.64, 1.0]; // comment gray
        const CURSOR_FG: [f32; 4] = [0.58, 0.50, 1.00, 1.0]; // Dracula purple

        let count_label = search.count_label();

        // Build the bar text:  "  query_  N / M  ↑↓ esc "
        // We render it in 3 segments with different colors.
        let query_display = format!(" /{}/", search.query);
        let count_display = if count_label.is_empty() {
            String::new()
        } else {
            format!("  {}  ", count_label)
        };
        let hint = " ↑↓ esc ";

        let bar_width = (query_display.chars().count()
            + count_display.chars().count()
            + hint.chars().count())
            .max(24)
            .min(total_cols);

        let col_offset = total_cols.saturating_sub(bar_width);
        let row = 0usize; // top row

        // Segment 1: query
        let q_width = query_display.chars().count().min(bar_width);
        self.push_shaped_row(&query_display, QUERY_FG, BAR_BG, row, col_offset, q_width, font);

        // Segment 2: match count
        let mut seg_offset = col_offset + q_width;
        if !count_display.is_empty() {
            let c_width = count_display.chars().count().min(bar_width.saturating_sub(q_width));
            self.push_shaped_row(&count_display, COUNT_FG, BAR_BG, row, seg_offset, c_width, font);
            seg_offset += c_width;
        }

        // Segment 3: hint
        let remaining = bar_width.saturating_sub(seg_offset - col_offset);
        if remaining > 0 {
            self.push_shaped_row(hint, HINT_FG, BAR_BG, row, seg_offset, remaining, font);
        }

        // Cursor blink at end of query (a 1-cell colored block)
        let cursor_col = col_offset + 1 + search.query.chars().count() + 1; // after the /query
        if cursor_col < col_offset + q_width {
            self.instances.push(CellVertex {
                grid_pos: [cursor_col as f32, row as f32],
                atlas_uv: [0.0; 4],
                fg: CURSOR_FG,
                bg: CURSOR_FG,
                glyph_offset: [0.0; 2],
                glyph_size: [0.0; 2],
                flags: 0,
                _pad: 0,
            });
        }

        let _ = CURSOR_FG; // suppress if unused after cursor removal
    }
}

// ── Debug HUD ─────────────────────────────────────────────────────────────────

impl RenderContext {
    /// Render the debug HUD overlay in the top-left corner (F12 toggle).
    ///
    /// Shows frame time statistics, shape cache hit/miss ratio, instance count,
    /// and atlas fill percentage. Uses `push_shaped_row` so it shares the same
    /// overlay render pass as the palette and search bar.
    pub fn build_debug_hud_instances(
        &mut self,
        font: &crate::config::schema::FontConfig,
    ) {
        if !self.hud_visible { return; }

        const HUD_BG:   [f32; 4] = [0.08, 0.08, 0.14, 0.90]; // dark semi-transparent
        const TITLE_FG: [f32; 4] = [0.58, 0.50, 1.00, 1.0];  // Dracula purple
        const VALUE_FG: [f32; 4] = [0.95, 0.98, 0.55, 1.0];  // Dracula yellow
        const WARN_FG:  [f32; 4] = [1.00, 0.47, 0.47, 1.0];  // red for high frame times

        let hud_width = 44usize;

        // ── Frame time statistics from ring buffer ───────────────────────────
        let (avg_ms, p50_ms, p95_ms) = if self.frame_times.is_empty() {
            (0.0f32, 0.0f32, 0.0f32)
        } else {
            let mut sorted: Vec<f32> = self.frame_times.iter().copied().collect();
            let n = sorted.len();
            let avg = sorted.iter().sum::<f32>() / n as f32;
            sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            let p50 = sorted[n / 2];
            let p95 = sorted[(n * 95 / 100).min(n - 1)];
            (avg, p50, p95)
        };

        // ── Shape cache ──────────────────────────────────────────────────────
        let total_shapes = self.shape_cache_hits + self.shape_cache_misses;
        let hit_pct = if total_shapes == 0 {
            0u32
        } else {
            (self.shape_cache_hits * 100 / total_shapes) as u32
        };

        // ── Atlas fill ───────────────────────────────────────────────────────
        let atlas_pct = self.renderer.atlas.current_fill_percent();
        let shape_hits = self.shape_cache_hits;
        let shape_misses = self.shape_cache_misses;
        let instance_count = self.last_instance_count;
        let upload_kb = self.last_gpu_upload_bytes as f32 / 1024.0;

        // ── Build HUD text lines ─────────────────────────────────────────────
        let frame_fg = if avg_ms > 16.67 { WARN_FG } else { VALUE_FG };

        let hud_lines: Vec<(String, [f32; 4])> = vec![
            (format!(" F12 HUD"), TITLE_FG),
            (format!(" {:10} {:.1}ms  p50:{:.1}ms  p95:{:.1}ms", "frame", avg_ms, p50_ms, p95_ms), frame_fg),
            (format!(" {:10} hits={} miss={} ({}%)", "shape", shape_hits, shape_misses, hit_pct), VALUE_FG),
            (format!(" {:10} {}", "instances", instance_count), VALUE_FG),
            (format!(" {:10} {:.1}%", "atlas", atlas_pct), VALUE_FG),
            (format!(" {:10} {:.1} KB/frame", "upload", upload_kb), VALUE_FG),
        ];

        for (row, (text, fg)) in hud_lines.iter().enumerate() {
            self.push_shaped_row(text, *fg, HUD_BG, row, 0, hud_width, font);
        }
    }
}
