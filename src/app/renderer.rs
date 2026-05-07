use anyhow::Result;
use rust_i18n::t;
use std::collections::HashMap;
use std::sync::Arc;
use winit::window::Window;

use crate::app::mux::Workspace;
use crate::config::Config;
use crate::font::{build_font_system, TextShaper};
use crate::llm::ai_block::{AiBlock, AiState, AI_BLOCK_ROWS};
use crate::llm::chat_panel::{
    header_action_label, header_actions_start_col, ChatPanel, HeaderAction,
};
use crate::llm::markdown::{AnnotatedLine, BlockKind, ParseState, SpanKind, TokenKind};
use crate::renderer::cell::{CellVertex, FLAG_COLOR_GLYPH, FLAG_CURSOR, FLAG_LCD};
use crate::renderer::rounded_rect::RoundedRectInstance;
use crate::renderer::GpuRenderer;
use crate::term::color::resolve_color;
use crate::term::{CursorInfo, CursorShape};
use crate::ui::info_overlay::InfoOverlay;
use crate::ui::search_bar::SearchBar;
use crate::ui::{CommandPalette, PaneSeparator, Tab};
use alacritty_terminal::vte::ansi::Color as AnsiColor;

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
    pub cell_data_scratch: Vec<(
        String,
        Vec<(
            alacritty_terminal::vte::ansi::Color,
            alacritty_terminal::vte::ansi::Color,
        )>,
    )>,
    /// Which terminal's data is currently in `cell_data_scratch`.
    /// Used to detect tab/pane switches and force a full grid re-read instead of
    /// trusting damage-skip data that belongs to a different terminal.
    pub scratch_terminal_id: Option<usize>,
    /// Scratch buffers for `push_shaped_row` — reused per call to avoid hot-path allocs (TD-PERF-13).
    pub scratch_chars: Vec<char>,
    pub scratch_str: String,
    pub scratch_colors: Vec<([f32; 4], [f32; 4])>,
    /// Per-pane color resolve scratch — avoids Vec alloc per pane per frame (TD-PERF-32).
    pub colors_scratch: Vec<([f32; 4], [f32; 4])>,
    /// Incremental streaming wrap cache — avoids re-wrapping the full buf each token (TD-PERF-37).
    streaming_stable_lines: Vec<AnnotatedLine>,
    /// ParseState carried across stable-line boundaries for streaming markdown.
    streaming_fence_state: ParseState,
    /// Byte offset in streaming_buf up to which streaming_stable_lines is valid.
    streaming_stable_end: usize,
    /// Panel id and width used for the current streaming cache entry.
    streaming_cache_key: Option<(usize, usize)>,
    /// General-purpose format scratch for callers of `push_shaped_row` (TD-PERF-13).
    /// Kept separate from `scratch_str` (used inside push_shaped_row) to avoid borrow conflicts.
    pub fmt_buf: String,
    /// Reusable gap-fill buffer for the status bar spacer (TD-PERF-35).
    gap_buf: String,
    /// Reusable line buffer for `build_chat_panel_instances` — avoids Vec realloc per rebuild.
    /// Strings inside are reused across frames when capacity permits (TD-PERF-13).
    #[allow(clippy::type_complexity)]
    pub scratch_lines: Vec<(
        String,
        [f32; 4],
        Option<[f32; 4]>,
        Vec<(usize, usize, [f32; 4])>,
        Option<[f32; 4]>, // full-width message background tint (W-1)
    )>,
    pub panel_rect_cache: Vec<RoundedRectInstance>,
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
    /// Ring buffer of the last 120 input-to-pixel latency samples in milliseconds.
    pub latency_samples: std::collections::VecDeque<f32>,
    pub shape_cache_hits: u64,
    pub shape_cache_misses: u64,
    pub last_instance_count: usize,
    /// overlay_start from the last full frame — used by the blink fast path so it
    /// can keep overlay rendering correct without a full rebuild.
    pub last_overlay_start: usize,
    /// Bytes written to GPU buffers in the current frame (instances + LCD + rects).
    pub last_gpu_upload_bytes: usize,

    // ── Static-geometry caches (TD-PERF-08/09/10/16) ────────────────────────────
    // Scroll bar: ~50 CellVertex per frame, no HarfBuzz. Keyed by scroll state.
    pub scroll_bar_state: Option<(usize, usize, usize, usize)>,
    pub scroll_bar_cache: Vec<CellVertex>,
    // Tab bar: HarfBuzz per tab name. Cached inputs checked directly (no hash) (TD-PERF-16).
    pub tab_bar_instances_cache: Vec<CellVertex>,
    pub tab_bar_rects_cache: Vec<RoundedRectInstance>,
    pub tab_bar_inputs: Option<(usize, usize, bool, bool)>, // (active_index, total_cols, sidebar_visible, panel_visible)
    pub tab_bar_titles: Vec<String>,
    pub tab_bar_rename_input: Option<String>,
    // Status bar: HarfBuzz per segment. Keyed by hash of all segment inputs.
    pub status_bar_key: u64,
    pub status_bar_instances_cache: Vec<CellVertex>,
    pub status_bar_rect_cache: Vec<RoundedRectInstance>,
    pub sidebar_instances_cache: Vec<CellVertex>,
    pub sidebar_rect_cache: Vec<RoundedRectInstance>,
    pub sidebar_cache_key: Option<u64>,
    /// Reusable per-frame pane layout buffer — avoids a Vec<PaneInfo> alloc every frame (TD-PERF-40).
    pub pane_infos: Vec<crate::ui::PaneInfo>,
}

impl RenderContext {
    pub async fn new(window: Arc<Window>, config: &Config) -> Result<Self> {
        let renderer = GpuRenderer::new(window.clone(), config).await?;
        let scale_factor = window.scale_factor() as f32;

        let mut scaled_font = config.font.clone();
        scaled_font.size *= scale_factor;
        crate::font::loader::locate_font_for_lcd(&mut scaled_font);

        let (font_system, actual_family, face_id, font_path, face_index) =
            build_font_system(&scaled_font)?;
        let lcd_atlas = renderer.get_lcd_atlas();

        let mut shaper = TextShaper::new(
            Some(&renderer.device()),
            font_system,
            crate::font::shaper::TextShaperConfig {
                actual_family,
                font_id: face_id,
                font_path,
                face_index,
                font_config: &scaled_font,
                lcd_atlas,
            },
        );

        // Finalize renderer setup with shaper info
        let mut renderer = renderer;
        renderer.set_cell_size(shaper.cell_width, shaper.cell_height);

        // Pre-rasterize printable ASCII into the atlas to eliminate cold-start cache misses (REC-PERF-01).
        {
            let (atlas, queue) = renderer.atlas_and_queue();
            shaper.warmup_atlas(atlas, queue);
        }
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
            panel_rect_cache: Vec::new(),
            cell_data_scratch: Vec::new(),
            scratch_terminal_id: None,
            scratch_chars: Vec::new(),
            scratch_str: String::new(),
            scratch_colors: Vec::new(),
            colors_scratch: Vec::new(),
            streaming_stable_lines: Vec::new(),
            streaming_fence_state: ParseState::default(),
            streaming_stable_end: 0,
            streaming_cache_key: None,
            fmt_buf: String::new(),
            gap_buf: String::new(),
            scratch_lines: Vec::new(),
            frame_counter: 0,
            rect_instances: Vec::new(),
            hud_visible: false,
            frame_times: std::collections::VecDeque::new(),
            latency_samples: std::collections::VecDeque::new(),
            shape_cache_hits: 0,
            shape_cache_misses: 0,
            last_instance_count: 0,
            last_overlay_start: 0,
            last_gpu_upload_bytes: 0,
            scroll_bar_state: None,
            scroll_bar_cache: Vec::new(),
            tab_bar_instances_cache: Vec::new(),
            tab_bar_rects_cache: Vec::new(),
            tab_bar_inputs: None,
            tab_bar_titles: Vec::new(),
            tab_bar_rename_input: None,
            status_bar_key: 0,
            status_bar_instances_cache: Vec::new(),
            status_bar_rect_cache: Vec::new(),
            sidebar_instances_cache: Vec::new(),
            sidebar_rect_cache: Vec::new(),
            sidebar_cache_key: None,
            pane_infos: Vec::new(),
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
        // Periodic capacity shrink — every 300 frames, reclaim memory if a capacity spike
        // (e.g. large terminal or chat message) left buffers bloated (AUDIT-MEM-02, MEM-03).
        if self.frame_counter.is_multiple_of(300) {
            fn shrink_vec<T>(v: &mut Vec<T>) {
                if !v.is_empty() && v.capacity() > v.len() * 3 {
                    v.shrink_to(v.len() * 2);
                }
            }
            fn shrink_str(s: &mut String) {
                const MAX: usize = 880; // TYPICAL_COLS * 4
                if s.capacity() > MAX * 2 {
                    s.shrink_to(MAX);
                }
            }
            shrink_vec(&mut self.instances);
            shrink_vec(&mut self.lcd_instances);
            shrink_vec(&mut self.panel_instances_cache);
            shrink_vec(&mut self.rect_instances);
            shrink_vec(&mut self.scratch_chars);
            shrink_vec(&mut self.scratch_colors);
            shrink_vec(&mut self.colors_scratch);
            shrink_str(&mut self.scratch_str);
            shrink_str(&mut self.fmt_buf);
        }
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
        let cache = self
            .row_caches
            .entry(terminal_id)
            .or_insert_with(RowCache::new);
        if cache.rows.len() < cell_data.len() {
            cache.rows.resize(cell_data.len(), None);
        }

        for (row_idx, (text, raw_colors)) in cell_data.iter().enumerate() {
            self.colors_scratch.clear();
            self.colors_scratch
                .extend(raw_colors.iter().map(|(fg, bg)| {
                    (
                        resolve_color(*fg, &config.colors),
                        resolve_color(*bg, &config.colors),
                    )
                }));
            let colors: &[([f32; 4], [f32; 4])] = &self.colors_scratch;

            let row_hash = calculate_row_hash(text, colors);

            // Cache hit: copy local-coordinate instances and apply pane offset.
            if let Some(Some(entry)) = self
                .row_caches
                .get(&terminal_id)
                .and_then(|c| c.rows.get(row_idx))
            {
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
                if colors_approx_eq(*bg, default_bg) {
                    continue;
                }
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
                    self.shaper
                        .rasterize_lcd_to_atlas(glyph.cache_key, glyph.ch, queue)
                } else {
                    None
                };

                // Skip Swash rasterization when LCD succeeded — saves rasterization + atlas
                // upload for every text glyph on a cache miss. Color emoji never produce an
                // LCD entry and always fall through to the Swash path (TD-PERF-06).
                let (atlas_uv, glyph_offset, glyph_size, color_flag) = if lcd_entry.is_none() {
                    let (atlas, queue) = self.renderer.atlas_and_queue();
                    let se = self
                        .shaper
                        .rasterize_to_atlas(glyph.cache_key, atlas, queue)?;
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
                        (
                            [u0, v0 + fy0 * (v1 - v0), u1, v0 + fy1 * (v1 - v0)],
                            [ox, y0],
                            [gw, y1 - y0],
                            flag,
                        )
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
            CursorShape::Beam => ([0.0, 0.0], [2.0, ch]),
            CursorShape::Hidden => {
                self.cursor_vertex_template = None;
                return;
            }
        };
        if !info.visible {
            self.cursor_vertex_template = None;
            return;
        }
        let v = CellVertex {
            grid_pos: [
                (col_offset + info.col) as f32,
                (row_offset + info.row) as f32,
            ],
            atlas_uv: [0.0; 4],
            fg: config.colors.cursor_fg,
            bg: config.colors.cursor_bg,
            glyph_offset,
            glyph_size,
            flags: FLAG_CURSOR,
            _pad: 0,
        };
        self.cursor_vertex_template = Some(v);
        if blink_on {
            self.instances.push(v);
        }

        // fs_lcd blends in.bg explicitly (unlike fs_main which uses premultiplied
        // alpha over the framebuffer). LCD glyph vertices store the cell's original
        // bg, which is correct for normal rendering but wrong when a BLOCK cursor BG
        // pass paints cursor_bg over the full cell. Only patch for block shapes — beam
        // and underline cursors cover a small fraction of the cell, so the glyph bg
        // should remain the cell's original bg (most of the glyph sits on default_bg).
        if matches!(info.shape, CursorShape::Block | CursorShape::HollowBlock) {
            let cursor_bg = config.colors.cursor_bg;
            let cursor_gp = v.grid_pos;
            for lcd_v in &mut self.lcd_instances {
                if lcd_v.grid_pos == cursor_gp {
                    lcd_v.bg = cursor_bg;
                }
            }
        }
    }

    /// Draw 1-pixel separator lines between panes.
    ///
    /// Each separator is a single `RoundedRectInstance` (1×N or N×1 pixels) instead of
    /// emitting one `CellVertex` per row/column. `pad_x`/`pad_y` are the physical-pixel
    /// offsets applied by the cell shader uniform (window padding + tab bar height).
    pub fn build_pane_separators(&mut self, separators: &[PaneSeparator], pad_x: f32, pad_y: f32) {
        const SEP_COLOR: [f32; 4] = [0.165, 0.165, 0.184, 1.0]; // #2a2a2f border
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
                rect: [x, y, w, h],
                color: SEP_COLOR,
                radius: 0.0,
                border_width: 0.0,
                _pad: [0.0; 2],
            });
        }
    }

    /// Draw a rounded accent outline around the focused pane. Only called when pane_count > 1.
    /// Uses pane_rect (snapped to cell grid) to align exactly with separator lines.
    pub fn build_focus_border(
        &mut self,
        focused: &crate::ui::PaneInfo,
        colors: &crate::config::schema::ColorScheme,
        tab_accent: Option<[f32; 4]>,
    ) {
        let [r, g, b, _] = tab_accent.unwrap_or(colors.ui_accent);
        let focus_color = [r, g, b, 0.85];
        let border = 1.5 * self.scale_factor;
        let radius = 6.0 * self.scale_factor;
        let inset = border * 0.5;

        let cell_w = self.shaper.cell_width;

        // For panes at the left viewport edge (col_offset == 0), pane_rect.x equals the
        // text start position — there is no separator gap on that side.  Shift the border
        // rect's left edge one cell outward so the stroke falls in the window margin rather
        // than overlapping the first column of text.  Out-of-bounds pixels are GPU-clipped,
        // so the left border line disappears behind the window frame (same visual as a pane
        // with a left separator, where the stroke already sits in the separator gap).
        //
        // The top edge is NOT shifted: the viewport's top padding (tab bar + window chrome)
        // provides enough vertical space so the top stroke doesn't visibly overlap text, and
        // shifting upward would push the border into the title bar / traffic-light area.
        let x = if focused.col_offset == 0 {
            focused.pane_rect.x - cell_w
        } else {
            focused.pane_rect.x + inset
        };
        let y = focused.pane_rect.y + inset;
        let right = focused.pane_rect.x + focused.pane_rect.w - inset;
        let bottom = focused.pane_rect.y + focused.pane_rect.h - inset;

        self.rect_instances.push(RoundedRectInstance {
            rect: [x, y, right - x, bottom - y],
            color: focus_color,
            radius,
            border_width: border,
            _pad: [0.0; 2],
        });
    }

    /// Render OSC 133 command blocks for all panes.
    /// Draws per block: subtle bg rect, exit-code text pill on last row.
    /// Only completed blocks (with output_end) are rendered.
    /// `hover_block`: if Some((terminal_id, block_id)), that block is highlighted.
    #[allow(clippy::too_many_arguments)]
    pub fn build_block_instances(
        &mut self,
        pane_infos: &[crate::ui::PaneInfo],
        mux: &crate::app::Mux,
        colors: &crate::config::schema::ColorScheme,
        hover_block: Option<(usize, usize)>,
        pad_x: f32,
        pad_y: f32,
        font: &crate::config::schema::FontConfig,
    ) {
        let cell_w = self.shaper.cell_width;
        let cell_h = self.shaper.cell_height;
        let sf = self.scale_factor;

        let mut bg_color = colors.ui_surface;
        bg_color[3] = 0.06;
        let mut hover_bg_color = colors.ui_surface_hover;
        hover_bg_color[3] = 0.14;
        // Badge design: dark theme surface bg + bright theme-color text.
        // ui_surface_active = selection_bg (e.g. Dracula's #454158) — already in the palette.
        // Text uses the theme's own success/error colors at full brightness → no off-palette tones.
        let pill_bg = colors.ui_surface_active;
        let success_fg = colors.ui_success; // e.g. Dracula #50FA7B
        let error_fg = colors.ansi[1]; // ANSI red,  e.g. Dracula #FF5555
        let transparent = [0.0f32; 4];

        for info in pane_infos {
            let Some(terminal) = mux.terminals.get(info.terminal_id).and_then(|s| s.as_ref())
            else {
                continue;
            };

            if terminal.is_alt_screen() {
                continue;
            }

            let (display_offset, history_size) = terminal.scrollback_info();
            let rows = info.pane_rect.h / cell_h;
            let blocks = terminal.block_manager.blocks_in_viewport(
                history_size,
                display_offset,
                rows as usize,
            );
            if blocks.is_empty() {
                continue;
            }

            let h = history_size as i64;
            let d = display_offset as i64;
            let r = rows as i64;
            let pane_x = info.pane_rect.x;
            let pane_y = info.pane_rect.y;
            let pane_w = info.pane_rect.w;

            // Pane origin and size in grid coordinates.
            let pane_col0 = ((pane_x - pad_x) / cell_w).round() as usize;
            let pane_row0 = ((pane_y - pad_y) / cell_h).round() as usize;
            let pane_cols = (pane_w / cell_w).round() as usize;

            for block in blocks {
                let Some(output_end) = block.output_end else {
                    continue;
                };

                let vp_start = (block.prompt_row - h + d).clamp(0, r - 1) as f32;
                let vp_end = (output_end - h + d + 1).clamp(1, r) as f32;
                let block_h = (vp_end - vp_start) * cell_h;
                let block_y = pane_y + vp_start * cell_h;

                let is_hovered = hover_block == Some((info.terminal_id, block.id));
                let block_bg = if is_hovered { hover_bg_color } else { bg_color };

                // Background rect: subtle tint across full pane width.
                self.rect_instances.push(RoundedRectInstance {
                    rect: [pane_x, block_y, pane_w, block_h],
                    color: block_bg,
                    radius: 0.0,
                    border_width: 0.0,
                    _pad: [0.0; 2],
                });

                // Exit code pill: rounded text badge right-aligned on last visible row.
                // Format: " ✓ 0 " (success) or " ✗ 127 " (error).
                let last_vp = (output_end - h + d).clamp(0, r - 1) as usize;
                let exit_code = block.exit_code.unwrap_or(-1);
                let (icon, text_fg) = if exit_code == 0 {
                    ("✓", success_fg)
                } else {
                    ("✗", error_fg)
                };
                let pill_text = format!(" {} {} ", icon, exit_code);
                let pill_len = pill_text.chars().count();

                // Grid position: right-aligned with 1-cell margin from pane edge.
                let pill_col = pane_col0 + pane_cols.saturating_sub(pill_len + 1);
                let pill_grid_row = pane_row0 + last_vp;
                let pill_x = pad_x + pill_col as f32 * cell_w;
                let pill_y = pad_y + pill_grid_row as f32 * cell_h;

                // Rounded rect: theme's selection/surface color as bg.
                self.rect_instances.push(RoundedRectInstance {
                    rect: [pill_x, pill_y, pill_len as f32 * cell_w, cell_h],
                    color: pill_bg,
                    radius: sf * 3.0,
                    border_width: 0.0,
                    _pad: [0.0; 2],
                });

                // Text in theme success/error color; transparent bg reveals the rounded rect.
                self.push_shaped_row(
                    &pill_text,
                    text_fg,
                    transparent,
                    pill_grid_row,
                    pill_col,
                    pill_len,
                    font,
                );
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
        if width == 0 {
            return;
        }

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
                glyph_size: [0.0; 2],
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
        scratch_str.extend(
            scratch_chars
                .iter()
                .copied()
                .chain(std::iter::repeat_n(' ', width.saturating_sub(len))),
        );

        scratch_colors.clear();
        scratch_colors.extend((0..width).map(|_| (fg, bg)));

        let shaped = self.shaper.shape_line(&scratch_str, &scratch_colors, font);

        // Restore scratch buffers.
        self.scratch_chars = scratch_chars;
        self.scratch_str = scratch_str;
        self.scratch_colors = scratch_colors;

        for glyph in shaped.glyphs {
            if glyph.col >= width {
                continue;
            }

            let (atlas, queue) = self.renderer.atlas_and_queue();
            let entry = match self
                .shaper
                .rasterize_to_atlas(glyph.cache_key, atlas, queue)
            {
                Ok(e) => e,
                Err(_) => continue, // skip; bg-coverage vertex already pushed above
            };

            let ox = entry.bearing_x as f32;
            let oy = shaped.ascent - entry.bearing_y as f32;
            let gw = entry.width as f32;
            let gh = entry.height as f32;

            // Skip zero-size glyphs (spaces): bg-coverage vertex already handles bg.
            if gw == 0.0 || gh == 0.0 {
                continue;
            }

            let y0 = oy.max(0.0);
            let y1 = (oy + gh).min(self.shaper.cell_height);
            if y1 <= y0 {
                continue;
            }

            let fy0 = (y0 - oy) / gh;
            let fy1 = (y1 - oy) / gh;
            let [u0, v0, u1, v1] = entry.uv;
            let atlas_uv = [u0, v0 + fy0 * (v1 - v0), u1, v0 + fy1 * (v1 - v0)];
            let glyph_offset = [ox, y0];
            let glyph_size = [gw, y1 - y0];

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
    pub fn push_md_line(
        &mut self,
        text: &str,
        base_fg: [f32; 4],
        spans: &[(usize, usize, [f32; 4])],
        bg: [f32; 4],
        row: usize,
        col_offset: usize,
        width: usize,
        font: &crate::config::schema::FontConfig,
    ) {
        if spans.is_empty() {
            self.push_shaped_row(text, base_fg, bg, row, col_offset, width, font);
            return;
        }
        let chars: Vec<char> = text.chars().collect();
        let total_chars = chars.len();

        // Build segment boundaries from span edges. Use a HashSet for O(1) dedup
        // instead of Vec::contains (O(n) per insert → O(n²) total).
        let mut boundary_set: rustc_hash::FxHashSet<usize> = rustc_hash::FxHashSet::default();
        boundary_set.insert(0);
        boundary_set.insert(total_chars);
        for &(s, e, _) in spans {
            if s > 0 {
                boundary_set.insert(s);
            }
            if e < total_chars {
                boundary_set.insert(e);
            }
        }
        let mut boundaries: Vec<usize> = boundary_set.into_iter().collect();
        boundaries.sort_unstable();

        let mut col = col_offset;
        for window in boundaries.windows(2) {
            let seg_start = window[0];
            let seg_end = window[1];
            if seg_start >= seg_end {
                continue;
            }
            let seg_text: String = chars[seg_start..seg_end].iter().collect();
            let seg_len = seg_end - seg_start;

            let seg_fg = spans
                .iter()
                .find(|&&(s, e, _)| s <= seg_start && e >= seg_end)
                .map(|&(_, _, fg)| fg)
                .unwrap_or(base_fg);

            let available = (col_offset + width).saturating_sub(col);
            if available == 0 {
                break;
            }
            let seg_w = seg_len.min(available);

            self.push_shaped_row(&seg_text, seg_fg, bg, row, col, seg_w, font);
            col += seg_len;
            if col >= col_offset + width {
                break;
            }
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
        pad_x: f32,
        pad_y: f32,
    ) {
        use crate::llm::agent_action::AgentAction;
        use crate::llm::chat_panel::{ConfirmDisplay, PanelState, MAX_FILE_ROWS};
        use crate::llm::diff::DiffKind;
        use std::fmt::Write as _;

        let panel_cols = panel.width_cols as usize;
        if panel_cols == 0 || screen_rows < 8 {
            return;
        }

        // ── Colors (Dracula Pro palette) ─────────────────────────────────────
        let actual_panel_bg = config.colors.background;
        let panel_bg = [0.0; 4]; // transparent

        let user_fg = config.llm.ui.user_fg;
        let asst_fg = config.llm.ui.assistant_fg;
        let input_fg = config.llm.ui.input_fg;

        let border_fg = config.colors.ui_accent;
        let stream_fg = config.colors.ansi[3];
        let err_fg = config.colors.ansi[1];
        let sep_fg = config.colors.ui_muted;
        let dim_fg = config.colors.ui_muted;
        let file_fg = config.colors.brights[2];
        let pick_sel = config.colors.ui_accent;
        let pick_fg = config.colors.foreground;

        let co = term_cols; // grid column where panel begins

        // ── Background Rect ──────────────────────────────────────────────────
        let radius = 10.0 * self.scale_factor;
        let border = 1.0 * self.scale_factor;
        let cw = self.shaper.cell_width;
        let ch = self.shaper.cell_height;
        let px = pad_x + co as f32 * cw;
        let py = pad_y;
        let pw = panel_cols as f32 * cw;
        let ph = screen_rows as f32 * ch;

        self.rect_instances
            .push(crate::renderer::rounded_rect::RoundedRectInstance {
                rect: [px - border, py, pw + 2.0 * border, ph],
                color: sep_fg, // border
                radius: radius + border,
                border_width: 0.0,
                _pad: [0.0; 2],
            });
        self.rect_instances
            .push(crate::renderer::rounded_rect::RoundedRectInstance {
                rect: [px, py, pw, ph],
                color: actual_panel_bg,
                radius,
                border_width: 0.0,
                _pad: [0.0; 2],
            });

        // ── Fixed bottom rows (always present) ───────────────────────────────
        // input_row1..4 and hints_row are rendered by build_chat_panel_input_rows (TD-PERF-10).
        let sep_row = screen_rows - 6;

        // ── File section height (0 when no files attached) ───────────────────
        // header row ("│ Selected (N files)") + one row per file, capped at MAX_FILE_ROWS
        let file_count = panel.attached_files.len();
        let file_section_rows = if file_count == 0 {
            0
        } else {
            1 + file_count.min(MAX_FILE_ROWS)
        };
        let mut fmt_buf = std::mem::take(&mut self.fmt_buf);

        self.build_panel_header(
            panel,
            panel_focused,
            config,
            font,
            panel_bg,
            co,
            panel_cols,
            &mut fmt_buf,
        );

        // ── File picker overlay (replaces history area) ───────────────────────
        if panel.file_picker_open {
            // Row 1: search input
            let q = &panel.file_picker_query;
            fmt_buf.clear();
            let _ = write!(&mut fmt_buf, "  > {q}");
            if file_picker_focused && cursor_blink_on {
                fmt_buf.push('\u{258b}');
            }
            self.push_shaped_row(&fmt_buf, input_fg, panel_bg, 1, co, panel_cols, font);

            // Rows 2..sep_row: filtered file list
            let filtered = panel.filtered_picker_items();
            let list_rows = sep_row.saturating_sub(2);
            for i in 0..list_rows {
                let row = 2 + i;
                if let Some(path) = filtered.get(i) {
                    let name = path.to_string_lossy();
                    let max_w = panel_cols.saturating_sub(5);
                    let trimmed = if name.chars().count() > max_w {
                        fmt_buf.clear();
                        fmt_buf.push('…');
                        fmt_buf.push_str(&name[name.len().saturating_sub(max_w - 1)..]);
                        fmt_buf.clone()
                    } else {
                        name.into_owned()
                    };
                    let attached = panel.attached_files.iter().any(|p| p.ends_with(path));
                    let marker = if attached { "✓ " } else { "  " };
                    fmt_buf.clear();
                    if i == panel.file_picker_cursor {
                        fmt_buf.push_str("  ▸ ");
                    } else {
                        fmt_buf.push_str("    ");
                    }
                    fmt_buf.push_str(marker);
                    fmt_buf.push_str(&trimmed);
                    self.push_shaped_row(
                        &fmt_buf,
                        if i == panel.file_picker_cursor {
                            pick_sel
                        } else {
                            pick_fg
                        },
                        panel_bg,
                        row,
                        co,
                        panel_cols,
                        font,
                    );
                } else {
                    self.push_shaped_row("", sep_fg, panel_bg, row, co, panel_cols, font);
                }
            }
        } else if matches!(panel.state, PanelState::AwaitingConfirm) {
            // ── Confirmation view: diff preview + [y]/[n] ────────────────────
            let add_fg = config.colors.ui_success;
            let rem_fg = config.colors.ansi[1];
            let ctx_fg2 = dim(config.colors.foreground, 0.25);

            match panel.confirm_display.as_ref() {
                Some(ConfirmDisplay::Write {
                    path,
                    diff,
                    added,
                    removed,
                }) => {
                    // Row 1: title
                    let rel_path = std::path::Path::new(path)
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or(path.as_str());
                    fmt_buf.clear();
                    let _ = write!(&mut fmt_buf, "  Write: {rel_path} (+{added} -{removed})");
                    let title_trimmed: String = fmt_buf.chars().take(panel_cols).collect();
                    self.push_shaped_row(
                        &title_trimmed,
                        border_fg,
                        panel_bg,
                        1,
                        co,
                        panel_cols,
                        font,
                    );

                    // Rows 2..sep_row: diff lines
                    let diff_rows = sep_row.saturating_sub(2);
                    for i in 0..diff_rows {
                        let row = 2 + i;
                        if let Some(dl) = diff.get(i) {
                            let (prefix, fg) = match dl.kind {
                                DiffKind::Added => ("  + ", add_fg),
                                DiffKind::Removed => ("  - ", rem_fg),
                                DiffKind::Context => ("    ", ctx_fg2),
                            };
                            let max_w = panel_cols.saturating_sub(prefix.chars().count());
                            let text: String = dl.text.chars().take(max_w).collect();
                            fmt_buf.clear();
                            fmt_buf.push_str(prefix);
                            fmt_buf.push_str(&text);
                            self.push_shaped_row(&fmt_buf, fg, panel_bg, row, co, panel_cols, font);
                        } else {
                            self.push_shaped_row("", sep_fg, panel_bg, row, co, panel_cols, font);
                        }
                    }
                }
                Some(ConfirmDisplay::Run { cmd }) => {
                    let warn_fg = config.colors.ansi[3];
                    // Detect potentially destructive patterns (TD-034).
                    let is_risky = [
                        "rm ",
                        "rm\t",
                        "rm -",
                        ":(){",
                        "dd ",
                        "mkfs",
                        "curl | sh",
                        "curl|sh",
                        "wget | sh",
                        "wget|sh",
                        "chmod -R 777",
                        "> /dev/",
                    ]
                    .iter()
                    .any(|p| cmd.contains(p));
                    let (run_title, title_fg) = if is_risky {
                        (t!("ai.run_command_destructive"), warn_fg)
                    } else {
                        (t!("ai.run_command"), border_fg)
                    };
                    // Row 1: title
                    self.push_shaped_row(&run_title, title_fg, panel_bg, 1, co, panel_cols, font);
                    // Row 2: command
                    let max_cmd = panel_cols.saturating_sub(5);
                    let cmd_trunc = cmd
                        .char_indices()
                        .nth(max_cmd)
                        .map(|(i, _)| &cmd[..i])
                        .unwrap_or(cmd);
                    fmt_buf.clear();
                    fmt_buf.push_str("    ");
                    fmt_buf.push_str(cmd_trunc);
                    self.push_shaped_row(&fmt_buf, add_fg, panel_bg, 2, co, panel_cols, font);
                    // Rest: empty
                    for row in 3..sep_row {
                        self.push_shaped_row("", sep_fg, panel_bg, row, co, panel_cols, font);
                    }
                }
                None => {
                    for row in 1..sep_row {
                        self.push_shaped_row("", sep_fg, panel_bg, row, co, panel_cols, font);
                    }
                }
            }
        } else if let PanelState::ConfirmAction(action) = &panel.state {
            // ── Inline action confirm card ────────────────────────────────────
            let accent = config.colors.ui_accent;
            let muted = config.colors.ui_muted;
            let ok_fg = config.colors.ui_success;
            match action {
                AgentAction::RunCommand { cmd, explanation } => {
                    self.push_shaped_row(
                        "  Run this command?",
                        border_fg,
                        panel_bg,
                        1,
                        co,
                        panel_cols,
                        font,
                    );
                    let max_cmd = panel_cols.saturating_sub(5);
                    let cmd_trunc = cmd
                        .char_indices()
                        .nth(max_cmd)
                        .map(|(i, _)| &cmd[..i])
                        .unwrap_or(cmd.as_str());
                    fmt_buf.clear();
                    fmt_buf.push_str("  $ ");
                    fmt_buf.push_str(cmd_trunc);
                    self.push_shaped_row(&fmt_buf, ok_fg, panel_bg, 2, co, panel_cols, font);
                    if !explanation.is_empty() {
                        let max_ex = panel_cols.saturating_sub(4);
                        let ex_trunc = explanation
                            .char_indices()
                            .nth(max_ex)
                            .map(|(i, _)| &explanation[..i])
                            .unwrap_or(explanation.as_str());
                        fmt_buf.clear();
                        fmt_buf.push_str("  ");
                        fmt_buf.push_str(ex_trunc);
                        self.push_shaped_row(&fmt_buf, muted, panel_bg, 3, co, panel_cols, font);
                        for row in 4..sep_row {
                            self.push_shaped_row("", sep_fg, panel_bg, row, co, panel_cols, font);
                        }
                    } else {
                        for row in 3..sep_row {
                            self.push_shaped_row("", sep_fg, panel_bg, row, co, panel_cols, font);
                        }
                    }
                }
                AgentAction::OpenFile { path } => {
                    self.push_shaped_row(
                        "  Open file?",
                        border_fg,
                        panel_bg,
                        1,
                        co,
                        panel_cols,
                        font,
                    );
                    let max_p = panel_cols.saturating_sub(4);
                    let p_trunc = path
                        .char_indices()
                        .nth(max_p)
                        .map(|(i, _)| &path[..i])
                        .unwrap_or(path.as_str());
                    fmt_buf.clear();
                    fmt_buf.push_str("  ");
                    fmt_buf.push_str(p_trunc);
                    self.push_shaped_row(&fmt_buf, accent, panel_bg, 2, co, panel_cols, font);
                    for row in 3..sep_row {
                        self.push_shaped_row("", sep_fg, panel_bg, row, co, panel_cols, font);
                    }
                }
                AgentAction::ExplainOutput { last_n_lines } => {
                    fmt_buf.clear();
                    let _ = write!(&mut fmt_buf, "  Explain last {last_n_lines} lines?");
                    let title: String = fmt_buf.chars().take(panel_cols).collect();
                    self.push_shaped_row(&title, border_fg, panel_bg, 1, co, panel_cols, font);
                    for row in 2..sep_row {
                        self.push_shaped_row("", sep_fg, panel_bg, row, co, panel_cols, font);
                    }
                }
            }
        } else {
            let history_start_row = self.build_panel_file_section(
                panel,
                file_count,
                file_section_rows,
                file_fg,
                dim_fg,
                sep_fg,
                panel_bg,
                font,
                co,
                panel_cols,
                &mut fmt_buf,
            );
            self.build_panel_messages(
                panel,
                panel_id,
                history_start_row,
                sep_row,
                config,
                font,
                co,
                panel_cols,
                panel_bg,
                actual_panel_bg,
                user_fg,
                asst_fg,
                stream_fg,
                err_fg,
                sep_fg,
                pad_x,
                pad_y,
                cw,
                ch,
                px,
                pw,
                &mut fmt_buf,
            );
        }

        // Separator row is intentionally empty — the card's rounded top edge
        // provides the visual break. Rendering the │────… characters looks ugly.
        self.push_shaped_row("", sep_fg, panel_bg, sep_row, co, panel_cols, font);
        self.fmt_buf = fmt_buf;
    }

    #[allow(clippy::too_many_arguments)]
    fn build_panel_header(
        &mut self,
        panel: &ChatPanel,
        panel_focused: bool,
        config: &Config,
        font: &crate::config::schema::FontConfig,
        panel_bg: [f32; 4],
        co: usize,
        panel_cols: usize,
        fmt_buf: &mut String,
    ) {
        use std::fmt::Write as _;

        let provider = &config.llm.provider;
        let model = &config.llm.model;
        let short_model = short_chat_header_model_name(model);
        let left_w = (3 + short_model.chars().count()).min(panel_cols);
        fmt_buf.clear();
        let _ = write!(fmt_buf, "{provider}:{model}");
        let center_full = fmt_buf.clone();
        fmt_buf.clear();
        let _ = write!(
            fmt_buf,
            "{} {} {}",
            header_action_label(HeaderAction::Restart),
            header_action_label(HeaderAction::Copy),
            header_action_label(HeaderAction::Close),
        );
        let right_start =
            header_actions_start_col(panel_cols, !panel.messages.is_empty()).unwrap_or(panel_cols);
        let right_w = if panel.messages.is_empty() {
            0
        } else {
            fmt_buf.chars().count()
        };
        let center_slot_start = (left_w + 1).min(panel_cols);
        let center_slot_end = right_start.saturating_sub(1);
        let center_slot_w = center_slot_end.saturating_sub(center_slot_start);
        let center = truncate_chars(&center_full, center_slot_w);
        let center_w = center.chars().count();
        let center_start = center_slot_start + center_slot_w.saturating_sub(center_w) / 2;

        fmt_buf.clear();
        let _ = write!(fmt_buf, " ✦ {short_model}");
        self.push_shaped_row(
            fmt_buf,
            config.colors.ui_accent,
            panel_bg,
            0,
            co,
            panel_cols,
            font,
        );
        if center_w > 0 {
            self.push_shaped_row(
                &center,
                config.colors.ui_muted,
                panel_bg,
                0,
                co + center_start,
                panel_cols.saturating_sub(center_start),
                font,
            );
        }
        if right_w > 0 {
            fmt_buf.clear();
            let _ = write!(
                fmt_buf,
                "{} {} {}",
                header_action_label(HeaderAction::Restart),
                header_action_label(HeaderAction::Copy),
                header_action_label(HeaderAction::Close),
            );
            self.push_shaped_row(
                fmt_buf,
                if panel_focused {
                    config.colors.foreground
                } else {
                    dim(config.colors.foreground, 0.15)
                },
                panel_bg,
                0,
                co + right_start,
                panel_cols.saturating_sub(right_start),
                font,
            );
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn build_panel_file_section(
        &mut self,
        panel: &ChatPanel,
        file_count: usize,
        file_section_rows: usize,
        file_fg: [f32; 4],
        dim_fg: [f32; 4],
        sep_fg: [f32; 4],
        panel_bg: [f32; 4],
        font: &crate::config::schema::FontConfig,
        co: usize,
        panel_cols: usize,
        fmt_buf: &mut String,
    ) -> usize {
        use crate::llm::chat_panel::MAX_FILE_ROWS;

        if file_section_rows > 0 {
            // Header: "│ Selected (N files)"
            let fhdr = t!(
                "ai.selected_files",
                count = file_count,
                suffix = if file_count == 1 { "" } else { "s" }
            )
            .to_string();
            self.push_shaped_row(&fhdr, file_fg, panel_bg, 1, co, panel_cols, font);
            // File list
            for (i, path) in panel.attached_files.iter().take(MAX_FILE_ROWS).enumerate() {
                let name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| path.to_string_lossy().into_owned());
                let max_w = panel_cols.saturating_sub(6);
                let trimmed = if let Some((i, _)) = name.char_indices().nth(max_w) {
                    let cut = name
                        .char_indices()
                        .nth(max_w.saturating_sub(1))
                        .map(|(j, _)| j)
                        .unwrap_or(i);
                    fmt_buf.clear();
                    fmt_buf.push_str(&name[..cut]);
                    fmt_buf.push('…');
                    fmt_buf.clone()
                } else {
                    name
                };
                fmt_buf.clear();
                fmt_buf.push_str("    ");
                fmt_buf.push_str(&trimmed);
                self.push_shaped_row(fmt_buf, dim_fg, panel_bg, 2 + i, co, panel_cols, font);
            }
            // Thin separator after file section (use pre-built cache from ChatPanel — TD-PERF-13)
            self.push_shaped_row(
                &panel.thin_separator_cache,
                sep_fg,
                panel_bg,
                1 + file_section_rows,
                co,
                panel_cols,
                font,
            );
        }

        1 + if file_section_rows > 0 {
            file_section_rows + 1
        } else {
            0
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn build_panel_messages(
        &mut self,
        panel: &ChatPanel,
        panel_id: usize,
        history_start_row: usize,
        sep_row: usize,
        config: &Config,
        font: &crate::config::schema::FontConfig,
        co: usize,
        panel_cols: usize,
        panel_bg: [f32; 4],
        actual_panel_bg: [f32; 4],
        user_fg: [f32; 4],
        asst_fg: [f32; 4],
        stream_fg: [f32; 4],
        err_fg: [f32; 4],
        sep_fg: [f32; 4],
        pad_x: f32,
        pad_y: f32,
        cw: f32,
        ch: f32,
        px: f32,
        pw: f32,
        fmt_buf: &mut String,
    ) {
        use crate::llm::chat_panel::{word_wrap, PanelState};
        use crate::llm::ChatRole;
        use std::fmt::Write as _;

        let history_rows = sep_row.saturating_sub(history_start_row);

        // W-5: Zero state — empty panel, idle
        if panel.messages.is_empty() && matches!(panel.state, PanelState::Idle) {
            // Layout: icon gets extra breathing room above and below.
            let center = (history_start_row + sep_row) / 2;
            let icon_row = center.saturating_sub(3); // ✦ with 1 empty row below it
            let text_row = center.saturating_sub(1); // subtitle
                                                     // center row = empty gap between subtitle and pills
            let pill1_row = center + 2;
            let pill2_row = center + 3;

            let pill_margin = 8.0 * cw;
            let pill_radius = 4.0 * self.scale_factor;
            let pill_border = 1.0 * self.scale_factor;

            for r in history_start_row..sep_row {
                if r == icon_row {
                    let pad = panel_cols.saturating_sub(1) / 2;
                    let mut row_text = " ".repeat(pad);
                    row_text.push('✦');
                    self.push_shaped_row(
                        &row_text,
                        config.colors.ui_accent,
                        panel_bg,
                        r,
                        co,
                        panel_cols,
                        font,
                    );
                } else if r == text_row {
                    let msg = "Ask a question below";
                    let msg_w = msg.chars().count();
                    let pad = panel_cols.saturating_sub(msg_w) / 2;
                    fmt_buf.clear();
                    fmt_buf.extend(std::iter::repeat_n(' ', pad));
                    fmt_buf.push_str(msg);
                    self.push_shaped_row(
                        fmt_buf,
                        config.colors.ui_muted,
                        panel_bg,
                        r,
                        co,
                        panel_cols,
                        font,
                    );
                } else if r == pill1_row || r == pill2_row {
                    let (label, hover_idx) = if r == pill1_row {
                        ("[ Fix last error ]", 0u8)
                    } else {
                        ("[ Explain command ]", 1u8)
                    };
                    let label_w = label.chars().count();
                    let pad = panel_cols.saturating_sub(label_w) / 2;
                    fmt_buf.clear();
                    fmt_buf.extend(std::iter::repeat_n(' ', pad));
                    fmt_buf.push_str(label);

                    let is_hovered = panel.zero_state_hover == Some(hover_idx);
                    // Two-rect pill: border outer + fill inner (same pattern as W-2 card).
                    let (border_color, fill_color, text_fg) = if is_hovered {
                        (
                            config.colors.ui_accent,
                            config.colors.ui_surface_active,
                            config.colors.foreground,
                        )
                    } else {
                        (
                            config.colors.ui_muted,
                            config.colors.ui_surface,
                            dim(config.colors.foreground, 0.15),
                        )
                    };
                    let pill_x = px + pill_margin;
                    let pill_y = pad_y + r as f32 * ch;
                    let pill_w = pw - 2.0 * pill_margin;
                    // Border rect (slightly larger).
                    self.rect_instances.push(RoundedRectInstance {
                        rect: [
                            pill_x - pill_border,
                            pill_y - pill_border,
                            pill_w + 2.0 * pill_border,
                            ch + 2.0 * pill_border,
                        ],
                        color: border_color,
                        radius: pill_radius + pill_border,
                        border_width: 0.0,
                        _pad: [0.0; 2],
                    });
                    // Fill rect.
                    self.rect_instances.push(RoundedRectInstance {
                        rect: [pill_x, pill_y, pill_w, ch],
                        color: fill_color,
                        radius: pill_radius,
                        border_width: 0.0,
                        _pad: [0.0; 2],
                    });
                    self.push_shaped_row(fmt_buf, text_fg, panel_bg, r, co, panel_cols, font);
                } else {
                    self.push_shaped_row("", sep_fg, panel_bg, r, co, panel_cols, font);
                }
            }
            return;
        }

        let msg_inner_w = panel_cols.saturating_sub(8);

        // Reuse scratch_lines across frames — Vec capacity is kept, String capacity reused
        // when the line count is stable (common case). Avoids ~N allocs per rebuild (TD-PERF-13).
        let mut all_lines = std::mem::take(&mut self.scratch_lines);
        let mut line_idx: usize = 0;

        // Helper: write `prefix + content` into all_lines[line_idx], reusing String capacity.
        macro_rules! push_line {
            ($prefix:expr, $content:expr, $color:expr, $accent:expr, $spans:expr, $bg:expr) => {{
                let p: &str = $prefix;
                let c: &str = $content;
                if line_idx < all_lines.len() {
                    let (s, col, acc, sp, bg) = &mut all_lines[line_idx];
                    s.clear();
                    s.push_str(p);
                    s.push_str(c);
                    *col = $color;
                    *acc = $accent;
                    *sp = $spans;
                    *bg = $bg;
                } else {
                    let mut s = String::with_capacity(p.len() + c.len());
                    s.push_str(p);
                    s.push_str(c);
                    all_lines.push((s, $color, $accent, $spans, $bg));
                }
                line_idx += 1;
            }};
        }

        // Use pre-wrapped lines from the cache (TD-PERF-05).
        // ensure_wrap_cache() is called in mod.rs before this function runs.
        let user_accent = [0.20, 0.60, 0.98, 1.0]; // Blue accent for user
        let asst_accent = [0.306, 0.788, 0.690, 1.0]; // Teal/green accent for AI

        // W-1: full-width message background tints (15% warm for user, 10% cool for assistant).
        let b = actual_panel_bg;
        let user_bg: Option<[f32; 4]> = Some([
            b[0] * 0.85 + user_fg[0] * 0.15,
            b[1] * 0.85 + user_fg[1] * 0.15,
            b[2] * 0.85 + user_fg[2] * 0.15,
            1.0,
        ]);
        let asst_bg: Option<[f32; 4]> = Some([
            b[0] * 0.90 + asst_accent[0] * 0.10,
            b[1] * 0.90 + asst_accent[1] * 0.10,
            b[2] * 0.90 + asst_accent[2] * 0.10,
            1.0,
        ]);

        // W-3: track code block spans (start, end) in all_lines index space.
        let mut code_spans: Vec<(usize, usize)> = Vec::new();
        let mut in_code = false;
        let mut code_start = 0usize;

        for (msg_idx, msg) in panel.messages.iter().enumerate() {
            let (fg, accent, msg_bg) = match msg.role {
                ChatRole::User => (user_fg, Some(user_accent), user_bg),
                ChatRole::Assistant => (asst_fg, Some(asst_accent), asst_bg),
                ChatRole::System => continue,
                ChatRole::Tool(_) => continue,
            };
            let prefix = "        "; // 8 spaces — keeps msg_inner_w (sub 8) correct
            let prefix_len = 8usize;
            for ann in panel.wrapped_message(msg_idx).iter() {
                let is_code = matches!(ann.kind, BlockKind::CodeBlock { .. });
                if is_code && !in_code {
                    in_code = true;
                    code_start = line_idx;
                } else if !is_code && in_code {
                    code_spans.push((code_start, line_idx));
                    in_code = false;
                }
                let line_fg = resolve_line_fg(&ann.kind, fg, &config.colors);
                let resolved_spans: Vec<(usize, usize, [f32; 4])> = ann
                    .spans
                    .iter()
                    .map(|&(s, e, ref sk)| {
                        (
                            s + prefix_len,
                            e + prefix_len,
                            resolve_span_fg(sk, line_fg, &config.colors),
                        )
                    })
                    .collect();
                push_line!(
                    prefix,
                    ann.display.as_str(),
                    line_fg,
                    accent,
                    resolved_spans,
                    msg_bg
                );
            }
            if in_code {
                code_spans.push((code_start, line_idx));
                in_code = false;
            }
            push_line!("", "", sep_fg, None, vec![], None);
        }

        if panel.is_streaming() && !panel.streaming_buf.is_empty() {
            let buf = &panel.streaming_buf;
            let cache_key = (panel_id, msg_inner_w);

            // Invalidate if panel or width changed, or buf was reset (new query).
            if self.streaming_cache_key != Some(cache_key) || self.streaming_stable_end > buf.len()
            {
                self.streaming_stable_lines.clear();
                self.streaming_fence_state = ParseState::default();
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
                let new_lines = crate::llm::markdown::parse_markdown(
                    seg,
                    msg_inner_w,
                    &mut self.streaming_fence_state,
                );
                self.streaming_stable_lines.extend(new_lines);
                self.streaming_stable_end = new_stable_end;
            }

            // Re-wrap only the partial last line (no newline yet) — O(partial_len).
            let partial = &buf[self.streaming_stable_end..];
            let partial_lines = if partial.is_empty() {
                vec![]
            } else {
                word_wrap(partial, msg_inner_w)
            };

            let stream_prefix = "        ";
            let stream_prefix_len = 8usize;

            // Stable annotated lines
            for ann in self.streaming_stable_lines.iter() {
                let is_code = matches!(ann.kind, BlockKind::CodeBlock { .. });
                if is_code && !in_code {
                    in_code = true;
                    code_start = line_idx;
                } else if !is_code && in_code {
                    code_spans.push((code_start, line_idx));
                    in_code = false;
                }
                let line_fg = resolve_line_fg(&ann.kind, stream_fg, &config.colors);
                let resolved_spans: Vec<(usize, usize, [f32; 4])> = ann
                    .spans
                    .iter()
                    .map(|&(s, e, ref sk)| {
                        (
                            s + stream_prefix_len,
                            e + stream_prefix_len,
                            resolve_span_fg(sk, line_fg, &config.colors),
                        )
                    })
                    .collect();
                push_line!(
                    stream_prefix,
                    ann.display.as_str(),
                    line_fg,
                    Some(asst_accent),
                    resolved_spans,
                    asst_bg
                );
            }
            // Partial plain-text lines (no newline yet — not parsed through markdown)
            for line in partial_lines.iter() {
                push_line!(
                    stream_prefix,
                    line.as_str(),
                    stream_fg,
                    Some(asst_accent),
                    vec![],
                    asst_bg
                );
            }
            if in_code {
                code_spans.push((code_start, line_idx));
            }
        }

        if matches!(panel.state, PanelState::Loading) {
            fmt_buf.clear();
            let _ = write!(fmt_buf, "        ⟳  {}", t!("ai.thinking"));
            push_line!(
                "",
                fmt_buf.as_str(),
                stream_fg,
                Some(asst_accent),
                vec![],
                asst_bg
            );
        }

        if let PanelState::Error(ref err) = panel.state {
            let wrapped = word_wrap(err, msg_inner_w);
            for (i, line) in wrapped.iter().enumerate() {
                let p = if i == 0 {
                    "   \u{2717}    "
                } else {
                    "        "
                };
                push_line!(p, line.as_str(), err_fg, None, vec![], None);
            }
        }

        let total_lines = line_idx;
        // Shrink logical length without dropping capacity.
        all_lines.truncate(total_lines);

        // W-7: reserve 2 rows at the bottom for suggestion pills when active.
        let suggestion_rows = if panel.show_suggestions
            && !panel.messages.is_empty()
            && matches!(panel.state, PanelState::Idle)
        {
            2usize
        } else {
            0
        };
        let effective_history_rows = history_rows.saturating_sub(suggestion_rows);

        let visible_start =
            total_lines.saturating_sub(effective_history_rows + panel.scroll_offset);
        let visible_end = visible_start + effective_history_rows;

        let accent_x = pad_x + co as f32 * cw + 2.0 * self.scale_factor;

        // W-3: code block bg rects and left accent stripes.
        {
            let code_bg = config.colors.ui_surface_active;
            let mut code_stripe = config.colors.ui_accent;
            code_stripe[3] = 0.8;
            for &(cs, ce) in &code_spans {
                let vis_cs = cs.max(visible_start);
                let vis_ce = ce.min(visible_end);
                if vis_cs >= vis_ce {
                    continue;
                }
                let row_s = history_start_row + (vis_cs - visible_start);
                let row_e = history_start_row + (vis_ce - visible_start);
                let ry = pad_y + row_s as f32 * ch;
                let rh = (row_e - row_s) as f32 * ch;
                self.rect_instances.push(RoundedRectInstance {
                    rect: [px, ry, pw, rh],
                    color: code_bg,
                    radius: 3.0 * self.scale_factor,
                    border_width: 0.0,
                    _pad: [0.0; 2],
                });
                self.rect_instances.push(RoundedRectInstance {
                    rect: [
                        accent_x - self.scale_factor,
                        ry,
                        2.0 * self.scale_factor,
                        rh,
                    ],
                    color: code_stripe,
                    radius: self.scale_factor,
                    border_width: 0.0,
                    _pad: [0.0; 2],
                });
            }
        }

        for i in 0..effective_history_rows {
            let row = history_start_row + i;
            let (text, fg, accent, spans_ref, msg_bg) = all_lines
                .get(visible_start + i)
                .map(|(t, f, a, sp, bg)| (t.as_str(), *f, *a, sp.as_slice(), *bg))
                .unwrap_or(("", sep_fg, None, &[][..], None));

            // W-1: full-width message background tint (painter's order — before glyphs).
            if let Some(bg) = msg_bg {
                self.rect_instances.push(RoundedRectInstance {
                    rect: [px, pad_y + row as f32 * ch, pw, ch],
                    color: bg,
                    radius: 0.0,
                    border_width: 0.0,
                    _pad: [0.0; 2],
                });
            }

            self.push_md_line(text, fg, spans_ref, panel_bg, row, co, panel_cols, font);

            if let Some(color) = accent {
                self.rect_instances.push(RoundedRectInstance {
                    rect: [
                        accent_x,
                        pad_y + row as f32 * ch,
                        3.0 * self.scale_factor,
                        ch,
                    ],
                    color,
                    radius: 1.5 * self.scale_factor,
                    border_width: 0.0,
                    _pad: [0.0; 2],
                });
            }
        }

        // W-7: suggestion pill rows just above sep_row.
        if suggestion_rows > 0 {
            let pill_margin = 8.0 * cw;
            let pill_radius = 4.0 * self.scale_factor;
            let pill_border = 1.0 * self.scale_factor;
            let pill_labels = ["[ Fix last error ]", "[ Explain more ]"];
            for (hover_idx, label) in pill_labels.iter().enumerate() {
                let r = sep_row - suggestion_rows + hover_idx;
                let label_w = label.chars().count();
                let pad = panel_cols.saturating_sub(label_w) / 2;
                fmt_buf.clear();
                fmt_buf.extend(std::iter::repeat_n(' ', pad));
                fmt_buf.push_str(label);

                let is_hovered = panel.suggestion_hover == Some(hover_idx as u8);
                let (border_color, fill_color, text_fg) = if is_hovered {
                    (
                        config.colors.ui_accent,
                        config.colors.ui_surface_active,
                        config.colors.foreground,
                    )
                } else {
                    (
                        config.colors.ui_muted,
                        config.colors.ui_surface,
                        dim(config.colors.foreground, 0.15),
                    )
                };
                let pill_x = px + pill_margin;
                let pill_y = pad_y + r as f32 * ch;
                let pill_w = pw - 2.0 * pill_margin;
                self.rect_instances.push(RoundedRectInstance {
                    rect: [
                        pill_x - pill_border,
                        pill_y - pill_border,
                        pill_w + 2.0 * pill_border,
                        ch + 2.0 * pill_border,
                    ],
                    color: border_color,
                    radius: pill_radius + pill_border,
                    border_width: 0.0,
                    _pad: [0.0; 2],
                });
                self.rect_instances.push(RoundedRectInstance {
                    rect: [pill_x, pill_y, pill_w, ch],
                    color: fill_color,
                    radius: pill_radius,
                    border_width: 0.0,
                    _pad: [0.0; 2],
                });
                self.push_shaped_row(fmt_buf, text_fg, fill_color, r, co, panel_cols, font);
            }
        }

        self.scratch_lines = all_lines;
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
        pad_x: f32,
        pad_y: f32,
    ) {
        use crate::llm::chat_panel::{wrap_input, ConfirmDisplay, PanelState};
        use crate::llm::ChatRole;
        use std::fmt::Write as _;

        let panel_cols = panel.width_cols as usize;
        if panel_cols == 0 || screen_rows < 8 {
            return;
        }

        let panel_bg = [0.0; 4]; // transparent
        let input_fg = config.llm.ui.input_fg;

        let hint_fg = config.colors.ui_muted;
        let dim_fg = config.colors.ui_muted;

        let co = term_cols;
        let hints_row = screen_rows - 1;
        let input_row4 = screen_rows - 2;
        let input_row3 = screen_rows - 3;
        let input_row2 = screen_rows - 4;
        let input_row1 = screen_rows - 5;
        let mut fmt_buf = std::mem::take(&mut self.fmt_buf);

        // ── W-2: Input card background + border ──────────────────────────────
        {
            let cw = self.shaper.cell_width;
            let ch = self.shaper.cell_height;
            let px = pad_x + co as f32 * cw;
            let card_y = pad_y + input_row1 as f32 * ch;
            let pw = panel_cols as f32 * cw;
            let card_h = 4.0 * ch;
            let radius = 4.0 * self.scale_factor;
            let border = 1.0 * self.scale_factor;

            // Subtle card: slightly lighter than the panel bg, not the purple selection color.
            let b = config.colors.background;
            let card_bg = [
                (b[0] + 0.06).min(1.0),
                (b[1] + 0.06).min(1.0),
                (b[2] + 0.06).min(1.0),
                1.0,
            ];
            let border_color = config.colors.ui_muted;

            // Border rect (slightly larger, drawn first).
            self.rect_instances.push(RoundedRectInstance {
                rect: [
                    px - border,
                    card_y - border,
                    pw + 2.0 * border,
                    card_h + 2.0 * border,
                ],
                color: border_color,
                radius: radius + border,
                border_width: 0.0,
                _pad: [0.0; 2],
            });
            // Card background.
            self.rect_instances.push(RoundedRectInstance {
                rect: [px, card_y, pw, card_h],
                color: card_bg,
                radius,
                border_width: 0.0,
                _pad: [0.0; 2],
            });
        }

        // ── Input field (or confirmation prompt) ─────────────────────────────
        if matches!(
            panel.state,
            PanelState::AwaitingConfirm | PanelState::ConfirmAction(_)
        ) {
            let confirm_yes = config.colors.ui_success;
            let confirm_always = config.colors.ui_accent;
            let confirm_no = config.colors.ansi[1];
            if matches!(panel.state, PanelState::ConfirmAction(_)) {
                // Three-option layout for inline actions.
                self.push_shaped_row(
                    "   [y] Run once",
                    confirm_yes,
                    panel_bg,
                    input_row1,
                    co,
                    panel_cols,
                    font,
                );
                self.push_shaped_row(
                    "   [a] Always allow",
                    confirm_always,
                    panel_bg,
                    input_row2,
                    co,
                    panel_cols,
                    font,
                );
                self.push_shaped_row(
                    "   [n] Cancel",
                    confirm_no,
                    panel_bg,
                    input_row3,
                    co,
                    panel_cols,
                    font,
                );
            } else {
                // Two-option layout for tool-call confirms (write_file / run_command).
                let (yes_label, no_label) = match panel.confirm_display.as_ref() {
                    Some(ConfirmDisplay::Run { .. }) => ("[y] Run", "[n] Cancel"),
                    _ => ("[y] Apply", "[n] Reject"),
                };
                self.push_shaped_row(
                    {
                        fmt_buf.clear();
                        fmt_buf.push_str("   ");
                        fmt_buf.push_str(yes_label);
                        &fmt_buf
                    },
                    confirm_yes,
                    panel_bg,
                    input_row2,
                    co,
                    panel_cols,
                    font,
                );
                self.push_shaped_row(
                    {
                        fmt_buf.clear();
                        fmt_buf.push_str("   ");
                        fmt_buf.push_str(no_label);
                        &fmt_buf
                    },
                    confirm_no,
                    panel_bg,
                    input_row3,
                    co,
                    panel_cols,
                    font,
                );
            }
        } else {
            let input_inner_w = panel_cols.saturating_sub(5);
            let show_cursor =
                panel_focused && !file_picker_focused && cursor_blink_on && panel.is_idle();
            let cursor_chars = panel.input.chars().count().min(panel.input_cursor);

            let cursor_storage: String;
            let input_display: &str = if show_cursor {
                let bp = panel
                    .input
                    .char_indices()
                    .nth(cursor_chars)
                    .map(|(b, _)| b)
                    .unwrap_or(panel.input.len());
                let mut s = panel.input.clone();
                s.insert(bp, '\u{258b}');
                cursor_storage = s;
                &cursor_storage
            } else {
                &panel.input
            };

            let input_lines = wrap_input(input_display, input_inner_w);
            let n = input_lines.len();

            let inp_fg = if panel_focused && !file_picker_focused {
                input_fg
            } else {
                dim_fg
            };

            // cursor_visual_pos gives the exact (line, col) using wrap_width-aware logic.
            let cursor_line = if show_cursor {
                input_lines
                    .iter()
                    .position(|l| l.contains('\u{258b}'))
                    .unwrap_or(n.saturating_sub(1))
            } else if panel_focused && !file_picker_focused && panel.is_idle() {
                panel.cursor_visual_pos(input_inner_w).0
            } else {
                n.saturating_sub(1)
            };

            // 4-line viewport: cursor stays at the bottom when scrolled past row 3.
            let vis_start = cursor_line.saturating_sub(3);
            let vis = [
                idx_or_default(&input_lines, vis_start),
                idx_or_default(&input_lines, vis_start + 1),
                idx_or_default(&input_lines, vis_start + 2),
                idx_or_default(&input_lines, vis_start + 3),
            ];
            let rows = [input_row1, input_row2, input_row3, input_row4];
            for (i, (line, row)) in vis.iter().zip(rows.iter()).enumerate() {
                let prefix = if i == 0 { "  \u{25b8}  " } else { "     " };
                fmt_buf.clear();
                fmt_buf.push_str(prefix);
                fmt_buf.push_str(line);
                self.push_shaped_row(&fmt_buf, inp_fg, panel_bg, *row, co, panel_cols, font);
            }
        }

        // ── Key hints + context usage bar ────────────────────────────────────
        let usage_hint = build_usage_hint(panel);
        let has_assistant = panel
            .messages
            .iter()
            .any(|m| matches!(m.role, ChatRole::Assistant));
        let hints: String = if file_picker_focused {
            fmt_buf.clear();
            let _ = write!(
                &mut fmt_buf,
                "  ↑↓ navigate   Enter: attach   Tab: close  {usage_hint}"
            );
            fmt_buf.clone()
        } else if !panel_focused {
            fmt_buf.clear();
            let _ = write!(
                &mut fmt_buf,
                "  <Leader>A: focus   <Leader>a c: clear   {usage_hint}"
            );
            fmt_buf.clone()
        } else {
            let base = match &panel.state {
                PanelState::Idle if !panel.input.trim().is_empty() => {
                    "  Enter: send   Tab: files   Leader+a a: close"
                }
                PanelState::Idle if has_assistant => "  Enter: run \u{23ce}   Tab: files",
                PanelState::Idle => "  Enter: send   Tab: files   Leader+a a: close",
                PanelState::Loading | PanelState::Streaming => "  streaming\u{2026}",
                PanelState::Error(_) => "  Esc: dismiss",
                PanelState::AwaitingConfirm => "  y/Enter: confirm   n/Esc: reject",
                PanelState::ConfirmAction(_) => "  y: run once   a: always   n/Esc: cancel",
                PanelState::Hidden => " ",
            };
            fmt_buf.clear();
            let _ = write!(&mut fmt_buf, "{base}   {usage_hint}");
            fmt_buf.clone()
        };
        let hints_display: String = hints.chars().take(panel_cols).collect();
        self.push_shaped_row(
            &hints_display,
            hint_fg,
            panel_bg,
            hints_row,
            co,
            panel_cols,
            font,
        );
        self.fmt_buf = fmt_buf;
    }

    /// Render the inline AI block, overlaying the bottom `AI_BLOCK_ROWS` rows of the terminal.
    /// Instances are appended after the terminal rows so they render on top.
    pub fn build_ai_block_instances(
        &mut self,
        block: &AiBlock,
        font: &crate::config::schema::FontConfig,
        term_cols: usize,
        screen_rows: usize,
        colors: &crate::config::schema::ColorScheme,
    ) {
        use crate::llm::chat_panel::word_wrap;

        if screen_rows < AI_BLOCK_ROWS + 1 || term_cols < 4 {
            return;
        }

        let w = term_cols;
        let sep_row = screen_rows - AI_BLOCK_ROWS;
        let input_row = screen_rows - AI_BLOCK_ROWS + 1;
        let resp_row = screen_rows - AI_BLOCK_ROWS + 2;
        let hint_row = screen_rows - AI_BLOCK_ROWS + 3;

        let block_bg = colors.background;
        let ai_border_fg = colors.ui_accent;
        let input_fg = colors.foreground;
        let resp_fg = colors.ui_success;
        let stream_fg = colors.ansi[3];
        let ai_hint_fg = colors.ui_muted;
        let err_fg = colors.ansi[1];

        const SPIN: [&str; 8] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧"];
        let spin = SPIN[(self.frame_counter / 4) as usize % 8];

        // Separator
        let title = " AI ";
        let side = (w.saturating_sub(title.chars().count())) / 2;
        let sep = format!(
            "{}{}{}",
            "─".repeat(side),
            title,
            "─".repeat(w.saturating_sub(side + title.chars().count()))
        );
        self.push_shaped_row(&sep, ai_border_fg, block_bg, sep_row, 0, w, font);

        // Input row: "⚡ > <query>[cursor]"
        let cursor = if matches!(block.state, AiState::Typing) {
            "▋"
        } else {
            ""
        };
        let query_row = format!("⚡ > {}{}", block.query, cursor);
        self.push_shaped_row(&query_row, input_fg, block_bg, input_row, 0, w, font);

        // Response + hint rows
        match &block.state {
            AiState::Typing => {
                self.push_shaped_row("", block_bg, block_bg, resp_row, 0, w, font);
                self.push_shaped_row(
                    "  Enter: send   Esc: cancel",
                    ai_hint_fg,
                    block_bg,
                    hint_row,
                    0,
                    w,
                    font,
                );
            }
            AiState::Loading => {
                self.push_shaped_row(
                    &format!("  {} thinking\u{2026}", spin),
                    stream_fg,
                    block_bg,
                    resp_row,
                    0,
                    w,
                    font,
                );
                self.push_shaped_row("  Esc: cancel", ai_hint_fg, block_bg, hint_row, 0, w, font);
            }
            AiState::Streaming => {
                let lines = word_wrap(&block.response, w.saturating_sub(4));
                let line = format!("  \u{2192} {}", idx_or_default(&lines, 0)); // →
                self.push_shaped_row(&line, stream_fg, block_bg, resp_row, 0, w, font);
                self.push_shaped_row(
                    &format!("  {} streaming\u{2026}   Esc: cancel", spin),
                    ai_hint_fg,
                    block_bg,
                    hint_row,
                    0,
                    w,
                    font,
                );
            }
            AiState::Done => {
                if let Some(cmd) = block.command_to_run() {
                    let max_cmd = w.saturating_sub(5);
                    let display = if let Some((i, _)) = cmd.char_indices().nth(max_cmd) {
                        let cut = cmd
                            .char_indices()
                            .nth(max_cmd.saturating_sub(1))
                            .map(|(j, _)| j)
                            .unwrap_or(i);
                        format!("{}…", &cmd[..cut])
                    } else {
                        cmd
                    };
                    self.push_shaped_row(
                        &format!("  \u{2192} {}", display),
                        resp_fg,
                        block_bg,
                        resp_row,
                        0,
                        w,
                        font,
                    );
                } else {
                    let lines = word_wrap(&block.response, w.saturating_sub(4));
                    let line = format!("  {}", idx_or_default(&lines, 0));
                    self.push_shaped_row(&line, resp_fg, block_bg, resp_row, 0, w, font);
                }
                self.push_shaped_row(
                    "  Enter: run \u{23ce}   Esc: dismiss",
                    ai_hint_fg,
                    block_bg,
                    hint_row,
                    0,
                    w,
                    font,
                );
            }
            AiState::Error(err) => {
                let lines = word_wrap(err, w.saturating_sub(4));
                let line = format!("  \u{2717} {}", idx_or_default(&lines, 0)); // ✗
                self.push_shaped_row(&line, err_fg, block_bg, resp_row, 0, w, font);
                self.push_shaped_row("  Esc: dismiss", ai_hint_fg, block_bg, hint_row, 0, w, font);
            }
            AiState::Hidden => {}
        }
    }

    #[allow(clippy::too_many_arguments)]
    #[allow(clippy::too_many_arguments)]
    pub fn build_workspace_sidebar_instances(
        &mut self,
        workspaces: &[Workspace],
        active_workspace_id: usize,
        nav_cursor: usize,
        rename_input: Option<&str>,
        sidebar_cols: usize,
        counts: &[(usize, usize)],
        sidebar_left_px: f32,
        sidebar_top_px: f32,
        sidebar_bottom_pad_px: f32,
        font: &crate::config::schema::FontConfig,
        colors: &crate::config::schema::ColorScheme,
        active_section: u8,
        mcp_servers: &[(String, Vec<String>)],
        mcp_scroll: usize,
        skills: &[crate::llm::skills::SkillMeta],
        skills_scroll: usize,
        steering_files: &[(String, String)],
        steering_scroll: usize,
    ) {
        if sidebar_cols == 0 {
            return;
        }

        let actual_sidebar_bg = colors.ui_surface;
        const SIDEBAR_BG: [f32; 4] = [0.0; 4]; // transparent
        let sidebar_item_active_bg = colors.ui_surface_active;
        let sidebar_item_hover_bg = colors.ui_surface_hover;
        let sidebar_fg = colors.foreground;
        let sidebar_dim_fg = colors.ui_muted;
        let sidebar_dot_active = colors.ui_accent;
        let sidebar_sep_fg = colors.ui_muted;

        let cw = self.shaper.cell_width;
        let ch = self.shaper.cell_height;
        let sidebar_px = sidebar_cols as f32 * cw;
        let (_win_w, win_h) = self.renderer.size();
        let visible_h = (win_h as f32 - sidebar_top_px - sidebar_bottom_pad_px).max(0.0);
        let total_rows = (visible_h / ch).floor() as usize;

        // Section height proportions: 40% workspace, 20% each for MCP/Skills/Steering.
        let ws_rows = (total_rows * 40 / 100).max(4);
        let mcp_rows = (total_rows * 20 / 100).max(3);
        let skills_rows = (total_rows * 20 / 100).max(3);
        let steering_rows = total_rows
            .saturating_sub(ws_rows + mcp_rows + skills_rows)
            .max(3);
        let mcp_start = ws_rows;
        let skills_start = ws_rows + mcp_rows;
        let steering_start = ws_rows + mcp_rows + skills_rows;

        let counts_hash: usize = counts
            .iter()
            .map(|(t, p)| t.wrapping_mul(10_000).wrapping_add(*p))
            .sum();

        let key: u64 = {
            use std::hash::{Hash, Hasher};
            let mut h = rustc_hash::FxHasher::default();
            workspaces.len().hash(&mut h);
            active_workspace_id.hash(&mut h);
            nav_cursor.hash(&mut h);
            counts_hash.hash(&mut h);
            active_section.hash(&mut h);
            mcp_scroll.hash(&mut h);
            skills_scroll.hash(&mut h);
            steering_scroll.hash(&mut h);
            mcp_servers.len().hash(&mut h);
            skills.len().hash(&mut h);
            steering_files.len().hash(&mut h);
            h.finish()
        };

        if rename_input.is_none() && self.sidebar_cache_key == Some(key) {
            self.instances
                .extend_from_slice(&self.sidebar_instances_cache);
            self.rect_instances
                .extend_from_slice(&self.sidebar_rect_cache);
            return;
        }

        let inst_start = self.instances.len();
        let rect_start = self.rect_instances.len();

        let radius = 10.0 * self.scale_factor;
        let border = 1.0 * self.scale_factor;
        let visible_sidebar_px = sidebar_px - (8.0 * self.scale_factor);

        // Outer border + background.
        self.rect_instances.push(RoundedRectInstance {
            rect: [
                sidebar_left_px - border,
                sidebar_top_px - border,
                visible_sidebar_px + 2.0 * border,
                visible_h + 2.0 * border,
            ],
            color: sidebar_sep_fg,
            radius: radius + border,
            border_width: 0.0,
            _pad: [0.0; 2],
        });
        self.rect_instances.push(RoundedRectInstance {
            rect: [
                sidebar_left_px,
                sidebar_top_px,
                visible_sidebar_px,
                visible_h,
            ],
            color: actual_sidebar_bg,
            radius,
            border_width: 0.0,
            _pad: [0.0; 2],
        });

        let push_sidebar_row =
            |this: &mut Self, text: &str, fg: [f32; 4], bg: [f32; 4], row: usize| {
                let start = this.instances.len();
                this.push_shaped_row(text, fg, bg, row, 0, sidebar_cols, font);
                for inst in &mut this.instances[start..] {
                    inst.grid_pos[0] -= sidebar_cols as f32;
                }
            };

        // Helper: push a thin horizontal separator line at the top of a section row.
        let push_section_sep = |this: &mut Self, row: usize| {
            let sep_y = sidebar_top_px + row as f32 * ch;
            this.rect_instances.push(RoundedRectInstance {
                rect: [
                    sidebar_left_px,
                    sep_y,
                    visible_sidebar_px,
                    1.0 * this.scale_factor,
                ],
                color: sidebar_sep_fg,
                radius: 0.0,
                border_width: 0.0,
                _pad: [0.0; 2],
            });
        };

        // ── Workspace section (rows 0..ws_rows) ──────────────────────────────
        let ws_section_active = active_section == 0;
        let ws_header_fg = if ws_section_active {
            sidebar_fg
        } else {
            sidebar_dim_fg
        };
        let mut header = " Workspaces".to_string();
        let header_chars = header.chars().count();
        if sidebar_cols > header_chars + 2 {
            header.push_str(&" ".repeat(sidebar_cols - header_chars - 2));
        }
        header.push('+');
        push_sidebar_row(self, &header, ws_header_fg, SIDEBAR_BG, 0);

        for (idx, ws) in workspaces.iter().enumerate() {
            let base_row = 1 + idx * 2;
            if base_row + 1 >= ws_rows {
                break;
            }
            let selected = idx == nav_cursor;
            let active = ws.id == active_workspace_id;

            if active || selected {
                let row_bg = if active {
                    sidebar_item_active_bg
                } else {
                    sidebar_item_hover_bg
                };
                let margin_x = 8.0 * self.scale_factor;
                let margin_y = 2.0 * self.scale_factor;
                let pill_px = sidebar_left_px + margin_x;
                let pill_py = sidebar_top_px + (base_row as f32 * ch) + margin_y;
                let pill_pw = visible_sidebar_px - 2.0 * margin_x;
                let pill_ph = 2.0 * ch - 2.0 * margin_y;
                self.rect_instances.push(RoundedRectInstance {
                    rect: [pill_px, pill_py, pill_pw, pill_ph],
                    color: row_bg,
                    radius: 6.0 * self.scale_factor,
                    border_width: 0.0,
                    _pad: [0.0; 2],
                });
                if active {
                    let accent_w = 3.0 * self.scale_factor;
                    let accent_h = pill_ph - 8.0 * self.scale_factor;
                    let accent_py = pill_py + 4.0 * self.scale_factor;
                    self.rect_instances.push(RoundedRectInstance {
                        rect: [pill_px, accent_py, accent_w, accent_h],
                        color: sidebar_dot_active,
                        radius: 1.5 * self.scale_factor,
                        border_width: 0.0,
                        _pad: [0.0; 2],
                    });
                }
            }

            let name = if selected {
                if let Some(input) = rename_input {
                    format!("{input}_")
                } else {
                    ws.name.clone()
                }
            } else {
                ws.name.clone()
            };
            let name_fg = if !ws_section_active {
                sidebar_dim_fg
            } else if active {
                [1.0, 1.0, 1.0, 1.0]
            } else {
                sidebar_fg
            };
            let trimmed_name: String = name.chars().take(sidebar_cols.saturating_sub(4)).collect();
            let mut line = format!("   {trimmed_name}");
            let line_w = line.chars().count();
            if line_w < sidebar_cols {
                line.push_str(&" ".repeat(sidebar_cols - line_w));
            }
            push_sidebar_row(self, &line, name_fg, SIDEBAR_BG, base_row);

            let (tabs, panes) = counts.get(idx).copied().unwrap_or((0, 0));
            let tabs_str = if tabs == 1 {
                "1 tab".to_string()
            } else {
                format!("{tabs} tabs")
            };
            let panes_str = if panes == 1 {
                "1 pane".to_string()
            } else {
                format!("{panes} panes")
            };
            let mut subtitle = format!("   {tabs_str} · {panes_str}");
            let sub_w = subtitle.chars().count();
            if sub_w < sidebar_cols {
                subtitle.push_str(&" ".repeat(sidebar_cols - sub_w));
            }
            push_sidebar_row(self, &subtitle, sidebar_dim_fg, SIDEBAR_BG, base_row + 1);
        }

        // ── MCP section (rows mcp_start .. mcp_start+mcp_rows) ───────────────
        push_section_sep(self, mcp_start);
        let mcp_section_active = active_section == 1;
        let mcp_header_fg = if mcp_section_active {
            sidebar_fg
        } else {
            sidebar_dim_fg
        };
        let mcp_item_fg = if mcp_section_active {
            sidebar_fg
        } else {
            sidebar_dim_fg
        };
        push_sidebar_row(self, " MCP SERVERS", mcp_header_fg, SIDEBAR_BG, mcp_start);
        let mcp_items_start = mcp_start + 1;
        let mcp_available = mcp_rows.saturating_sub(1);
        if mcp_servers.is_empty() {
            push_sidebar_row(
                self,
                "  no servers connected",
                sidebar_dim_fg,
                SIDEBAR_BG,
                mcp_items_start,
            );
        } else {
            let visible = &mcp_servers[mcp_scroll.min(mcp_servers.len())..];
            for (i, (server, tools)) in visible.iter().enumerate() {
                if i >= mcp_available {
                    break;
                }
                let row = mcp_items_start + i;
                let label = format!("  {} ({} tools)", server, tools.len());
                let trimmed: String = label.chars().take(sidebar_cols).collect();
                let is_cursor = active_section == 1 && i == 0;
                if is_cursor {
                    let margin = 4.0 * self.scale_factor;
                    self.rect_instances.push(RoundedRectInstance {
                        rect: [
                            sidebar_left_px + margin,
                            sidebar_top_px + row as f32 * ch + margin * 0.5,
                            visible_sidebar_px - 2.0 * margin,
                            ch - margin,
                        ],
                        color: sidebar_item_active_bg,
                        radius: 4.0 * self.scale_factor,
                        border_width: 0.0,
                        _pad: [0.0; 2],
                    });
                }
                push_sidebar_row(self, &trimmed, mcp_item_fg, SIDEBAR_BG, row);
            }
        }

        // ── Skills section (rows skills_start .. skills_start+skills_rows) ────
        push_section_sep(self, skills_start);
        let skills_section_active = active_section == 2;
        let skills_header_fg = if skills_section_active {
            sidebar_fg
        } else {
            sidebar_dim_fg
        };
        let skills_item_fg = if skills_section_active {
            sidebar_fg
        } else {
            sidebar_dim_fg
        };
        push_sidebar_row(self, " SKILLS", skills_header_fg, SIDEBAR_BG, skills_start);
        let skills_items_start = skills_start + 1;
        let skills_available = skills_rows.saturating_sub(1);
        if skills.is_empty() {
            push_sidebar_row(
                self,
                "  no skills loaded",
                sidebar_dim_fg,
                SIDEBAR_BG,
                skills_items_start,
            );
        } else {
            let visible = &skills[skills_scroll.min(skills.len())..];
            for (i, skill) in visible.iter().enumerate() {
                if i >= skills_available {
                    break;
                }
                let row = skills_items_start + i;
                let label = format!("  {}", skill.name);
                let trimmed: String = label.chars().take(sidebar_cols).collect();
                let is_cursor = active_section == 2 && i == 0;
                if is_cursor {
                    let margin = 4.0 * self.scale_factor;
                    self.rect_instances.push(RoundedRectInstance {
                        rect: [
                            sidebar_left_px + margin,
                            sidebar_top_px + row as f32 * ch + margin * 0.5,
                            visible_sidebar_px - 2.0 * margin,
                            ch - margin,
                        ],
                        color: sidebar_item_active_bg,
                        radius: 4.0 * self.scale_factor,
                        border_width: 0.0,
                        _pad: [0.0; 2],
                    });
                }
                push_sidebar_row(self, &trimmed, skills_item_fg, SIDEBAR_BG, row);
            }
        }

        // ── Steering section (rows steering_start .. steering_start+steering_rows)
        push_section_sep(self, steering_start);
        let steering_section_active = active_section == 3;
        let steering_header_fg = if steering_section_active {
            sidebar_fg
        } else {
            sidebar_dim_fg
        };
        let steering_item_fg = if steering_section_active {
            sidebar_fg
        } else {
            sidebar_dim_fg
        };
        push_sidebar_row(
            self,
            " STEERING",
            steering_header_fg,
            SIDEBAR_BG,
            steering_start,
        );
        let steering_items_start = steering_start + 1;
        let steering_available = steering_rows.saturating_sub(1);
        if steering_files.is_empty() {
            push_sidebar_row(
                self,
                "  no steering files",
                sidebar_dim_fg,
                SIDEBAR_BG,
                steering_items_start,
            );
        } else {
            let visible = &steering_files[steering_scroll.min(steering_files.len())..];
            for (i, (name, _)) in visible.iter().enumerate() {
                if i >= steering_available {
                    break;
                }
                let row = steering_items_start + i;
                let display = name.strip_suffix(".md").unwrap_or(name.as_str());
                let label = format!("  {display}");
                let trimmed: String = label.chars().take(sidebar_cols).collect();
                let is_cursor = active_section == 3 && i == 0;
                if is_cursor {
                    let margin = 4.0 * self.scale_factor;
                    self.rect_instances.push(RoundedRectInstance {
                        rect: [
                            sidebar_left_px + margin,
                            sidebar_top_px + row as f32 * ch + margin * 0.5,
                            visible_sidebar_px - 2.0 * margin,
                            ch - margin,
                        ],
                        color: sidebar_item_active_bg,
                        radius: 4.0 * self.scale_factor,
                        border_width: 0.0,
                        _pad: [0.0; 2],
                    });
                }
                push_sidebar_row(self, &trimmed, steering_item_fg, SIDEBAR_BG, row);
            }
        }

        self.sidebar_instances_cache.clear();
        self.sidebar_instances_cache
            .extend_from_slice(&self.instances[inst_start..]);
        self.sidebar_rect_cache.clear();
        self.sidebar_rect_cache
            .extend_from_slice(&self.rect_instances[rect_start..]);
        self.sidebar_cache_key = if rename_input.is_none() {
            Some(key)
        } else {
            None
        };
    }

    #[allow(clippy::too_many_arguments)]
    pub fn build_palette_instances(
        &mut self,
        palette: &CommandPalette,
        font: &crate::config::schema::FontConfig,
        total_cols: usize,
        total_rows: usize,
        pad_x: f32,
        pad_y: f32,
        colors: &crate::config::schema::ColorScheme,
    ) {
        let palette_width = 60_usize;
        let palette_height = 15_usize;

        if total_cols < palette_width || total_rows < palette_height {
            return;
        }

        let start_col = (total_cols - palette_width) / 2;
        let start_row = (total_rows - palette_height) / 2;

        let bg = {
            let [r, g, b, _] = colors.ui_overlay;
            [r, g, b, 0.95]
        };
        let transparent = [0.0f32; 4];
        let fg = colors.foreground;
        let highlight_bg = colors.ui_surface_active;
        let prompt_fg = colors.ui_accent;
        let border_color = colors.ui_muted;

        let cw = self.shaper.cell_width;
        let ch = self.shaper.cell_height;
        let px = pad_x + start_col as f32 * cw;
        let py = pad_y + start_row as f32 * ch;
        let pw = palette_width as f32 * cw;
        let ph = palette_height as f32 * ch;
        let radius = 12.0 * self.scale_factor;
        let border = 1.0 * self.scale_factor;

        // Border rect (drawn first — behind panel bg)
        self.rect_instances.push(RoundedRectInstance {
            rect: [
                px - border,
                py - border,
                pw + 2.0 * border,
                ph + 2.0 * border,
            ],
            color: border_color,
            radius: radius + border,
            border_width: 0.0,
            _pad: [0.0; 2],
        });
        // Panel background rect with rounded corners
        self.rect_instances.push(RoundedRectInstance {
            rect: [px, py, pw, ph],
            color: bg,
            radius,
            border_width: 0.0,
            _pad: [0.0; 2],
        });

        let prompt = format!("   > {}▋", palette.query);
        self.push_shaped_row(
            &prompt,
            prompt_fg,
            transparent,
            start_row,
            start_col,
            palette_width,
            font,
        );

        let keybind_fg = [0.420, 0.420, 0.478, 1.0]; // #6b6b7a muted

        let max_visible = palette_height - 1;
        let scroll_offset = if palette.selected >= max_visible {
            palette.selected - max_visible + 1
        } else {
            0
        };

        for i in 0..max_visible {
            let result_idx = scroll_offset + i;
            let row = start_row + 1 + i;
            let is_selected = result_idx == palette.selected;
            let current_bg = if is_selected {
                highlight_bg
            } else {
                transparent
            };

            if let Some(action) = palette.results.get(result_idx) {
                if is_selected {
                    // Highlight row: pill-style rect with padding
                    self.rect_instances.push(RoundedRectInstance {
                        rect: [
                            px + 6.0 * self.scale_factor,
                            pad_y + row as f32 * ch,
                            pw - 12.0 * self.scale_factor,
                            ch,
                        ],
                        color: highlight_bg,
                        radius: 6.0 * self.scale_factor,
                        border_width: 0.0,
                        _pad: [0.0; 2],
                    });
                }

                let name_text = format!("    {}", action.name);
                self.push_shaped_row(
                    &name_text,
                    fg,
                    current_bg,
                    row,
                    start_col,
                    palette_width,
                    font,
                );

                if let Some(kb) = &action.keybind {
                    let kb_display = format!("{} ", kb);
                    let kb_len = kb_display.chars().count();
                    if kb_len < palette_width {
                        let kb_col = start_col + palette_width - kb_len;
                        self.push_shaped_row(
                            &kb_display,
                            keybind_fg,
                            current_bg,
                            row,
                            kb_col,
                            kb_len,
                            font,
                        );
                    }
                }
            } else {
                self.push_shaped_row("", fg, transparent, row, start_col, palette_width, font);
            }
        }
    }

    /// Render the info overlay for sidebar items (MCP / Skills / Steering).
    /// Displays markdown content with the same syntax highlighting as the chat panel.
    #[allow(clippy::too_many_arguments)]
    pub fn build_info_overlay_instances(
        &mut self,
        overlay: &InfoOverlay,
        font: &crate::config::schema::FontConfig,
        total_cols: usize,
        total_rows: usize,
        pad_x: f32,
        pad_y: f32,
        colors: &crate::config::schema::ColorScheme,
    ) {
        let ow = total_cols.clamp(30, 80);
        let oh = (total_rows.saturating_sub(4)).clamp(8, 36);
        if total_cols < ow || total_rows < oh {
            return;
        }

        let start_col = (total_cols.saturating_sub(ow)) / 2;
        let start_row = (total_rows.saturating_sub(oh)) / 2;

        let bg = {
            let [r, g, b, _] = colors.ui_overlay;
            [r, g, b, 0.97]
        };
        let transparent = [0.0f32; 4];
        let fg = colors.foreground;
        let border_color = colors.ui_muted;
        let accent = colors.ui_accent;

        let cw = self.shaper.cell_width;
        let ch = self.shaper.cell_height;
        let px = pad_x + start_col as f32 * cw;
        let py = pad_y + start_row as f32 * ch;
        let pw = ow as f32 * cw;
        let ph = oh as f32 * ch;
        let radius = 12.0 * self.scale_factor;
        let border = 1.0 * self.scale_factor;

        // Border + background
        self.rect_instances.push(RoundedRectInstance {
            rect: [
                px - border,
                py - border,
                pw + 2.0 * border,
                ph + 2.0 * border,
            ],
            color: border_color,
            radius: radius + border,
            border_width: 0.0,
            _pad: [0.0; 2],
        });
        self.rect_instances.push(RoundedRectInstance {
            rect: [px, py, pw, ph],
            color: bg,
            radius,
            border_width: 0.0,
            _pad: [0.0; 2],
        });

        // Title bar separator
        self.rect_instances.push(RoundedRectInstance {
            rect: [
                px + 4.0 * self.scale_factor,
                py + ch,
                pw - 8.0 * self.scale_factor,
                1.0 * self.scale_factor,
            ],
            color: border_color,
            radius: 0.0,
            border_width: 0.0,
            _pad: [0.0; 2],
        });

        // Title row
        let title = format!("  {}", overlay.title);
        self.push_shaped_row(&title, accent, transparent, start_row, start_col, ow, font);

        // Content area: rows start_row+1 .. start_row+oh-1 (last row = footer hint)
        let content_rows = oh.saturating_sub(2); // -1 title, -1 footer
        let scroll = overlay.scroll;
        let content_col = start_col + 1;
        let content_width = ow.saturating_sub(2);

        for i in 0..content_rows {
            let line_idx = scroll + i;
            let row = start_row + 1 + i;
            if let Some(line) = overlay.lines.get(line_idx) {
                let line_fg = resolve_line_fg(&line.kind, fg, colors);
                // Resolve spans to (start, end, color) tuples
                let resolved: Vec<(usize, usize, [f32; 4])> = line
                    .spans
                    .iter()
                    .map(|(s, e, kind)| (*s, *e, resolve_span_fg(kind, line_fg, colors)))
                    .collect();
                self.push_md_line(
                    &line.display,
                    line_fg,
                    &resolved,
                    transparent,
                    row,
                    content_col,
                    content_width,
                    font,
                );
            } else {
                self.push_shaped_row("", fg, transparent, row, content_col, content_width, font);
            }
        }

        // Footer: scroll hint + Esc to close
        let footer_row = start_row + oh - 1;
        let can_scroll_down = scroll + content_rows < overlay.lines.len();
        let can_scroll_up = scroll > 0;
        let scroll_hint = match (can_scroll_up, can_scroll_down) {
            (true, true) => "j/k scroll",
            (true, false) => "k scroll up",
            (false, true) => "j scroll down",
            (false, false) => "",
        };
        let footer = if scroll_hint.is_empty() {
            format!("{:width$}Esc close  ", "", width = ow.saturating_sub(10))
        } else {
            format!(
                "{:width$}{}  Esc close  ",
                "",
                scroll_hint,
                width = ow.saturating_sub(scroll_hint.len() + 12)
            )
        };
        self.push_shaped_row(
            &footer,
            border_color,
            transparent,
            footer_row,
            start_col,
            ow,
            font,
        );
    }

    /// Render the right-click context menu as a floating popup at `menu.col/row`.
    pub fn build_context_menu_instances(
        &mut self,
        menu: &crate::ui::context_menu::ContextMenu,
        font: &crate::config::schema::FontConfig,
        total_cols: usize,
        total_rows: usize,
        colors: &crate::config::schema::ColorScheme,
    ) {
        use crate::ui::context_menu::CONTEXT_MENU_WIDTH;

        if !menu.visible || menu.items.is_empty() {
            return;
        }

        let width = CONTEXT_MENU_WIDTH;
        let height = menu.items.len();

        if menu.col + width > total_cols || menu.row + height > total_rows {
            return;
        }

        let bg = colors.ui_overlay;
        let fg = colors.foreground;
        let hover_bg = colors.ui_surface_hover;
        let keybind_fg = colors.ui_muted;

        let sep_fg = colors.ui_muted;

        let label_fg = colors.ui_muted;

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

            // Name on the left — with optional colored swatch prefix.
            if let Some(swatch) = item.swatch_color {
                self.push_shaped_row("● ", swatch, current_bg, row, menu.col + 1, 2, font);
                let name_text = format!(" {}", item.label);
                self.push_shaped_row(
                    &name_text,
                    fg,
                    current_bg,
                    row,
                    menu.col + 3,
                    width.saturating_sub(3),
                    font,
                );
            } else {
                let name_text = format!("  {}", item.label);
                self.push_shaped_row(&name_text, fg, current_bg, row, menu.col, width, font);
            }

            // Keybind right-aligned.
            if let Some(kb) = &item.keybind {
                let kb_display = format!("{} ", kb);
                let kb_len = kb_display.chars().count();
                if kb_len < width {
                    let kb_col = menu.col + width - kb_len;
                    self.push_shaped_row(
                        &kb_display,
                        keybind_fg,
                        current_bg,
                        row,
                        kb_col,
                        kb_len,
                        font,
                    );
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
    #[allow(clippy::too_many_arguments)]
    pub fn build_status_bar_instances(
        &mut self,
        bar: &crate::ui::status_bar::StatusBar,
        font: &crate::config::schema::FontConfig,
        total_cols: usize,
        row: usize,
        pad_y: f32,
        win_w: f32,
        colors: &crate::config::schema::ColorScheme,
    ) {
        use crate::ui::status_bar::StatusBar;

        const SB_EXTRA_PX: f32 = 8.0;

        let bar_bg = StatusBar::bar_bg();

        // Full-width background rect: covers the cell row + SB_EXTRA_PX extension below.
        // Renders before cell backgrounds (rect pass is first), filling left/right padding
        // areas and the extension strip with the bar's background color.
        {
            let cell_h = self.shaper.cell_height;
            let bar_y = pad_y + row as f32 * cell_h;
            self.rect_instances
                .push(crate::renderer::rounded_rect::RoundedRectInstance {
                    rect: [0.0, bar_y, win_w, cell_h + SB_EXTRA_PX],
                    color: bar_bg,
                    radius: 0.0,
                    border_width: 0.0,
                    _pad: [0.0; 2],
                });
        }
        use crate::config::schema::StatusBarStyle;
        let powerline = bar.style == StatusBarStyle::Powerline;
        let plain_sep_fg = colors.ui_muted;

        // ── Left side ────────────────────────────────────────────────────────
        let mut col = 0usize;
        for (i, seg) in bar.left.iter().enumerate() {
            let text = &seg.text;
            let len = text.chars().count();
            if col + len > total_cols {
                break;
            }
            self.push_shaped_row(text, seg.fg, seg.bg, row, col, len, font);
            col += len;

            // Separator between segments (not after last).
            if i + 1 < bar.left.len() {
                let next_bg = bar.left[i + 1].bg;
                if powerline {
                    // Powerline: "" with fg = current segment bg, bg = next segment bg.
                    let arrow = StatusBar::pl_left_arrow();
                    if col + 1 > total_cols {
                        break;
                    }
                    self.push_shaped_row(arrow, seg.bg, next_bg, row, col, 1, font);
                    col += 1;
                } else {
                    let sep = " › ";
                    let sep_len = sep.chars().count();
                    if col + sep_len > total_cols {
                        break;
                    }
                    self.push_shaped_row(sep, plain_sep_fg, next_bg, row, col, sep_len, font);
                    col += sep_len;
                }
            }
        }

        // ── Right side (compute total width first, then render right-aligned) ─
        let rsep_w = bar.right_sep_width();
        // In Powerline mode a leading "" transitions from bar_bg to the first right segment.
        let leading_arrow = powerline && !bar.right.is_empty();
        let right_total: usize = (if leading_arrow { 1 } else { 0 })
            + bar
                .right
                .iter()
                .map(|s| s.text.chars().count())
                .sum::<usize>()
            + bar.right.len().saturating_sub(1) * rsep_w;

        let right_start = total_cols.saturating_sub(right_total);

        // Fill gap between left and right with bar_bg.
        if right_start > col {
            let gap = right_start - col;
            let mut buf = std::mem::take(&mut self.gap_buf);
            buf.clear();
            buf.extend(std::iter::repeat_n(' ', gap));
            self.push_shaped_row(&buf, bar_bg, bar_bg, row, col, gap, font);
            self.gap_buf = buf;
        }

        let mut rcol = right_start;

        // Powerline leading arrow before first right segment.
        if leading_arrow {
            let first_bg = bar.right[0].bg;
            self.push_shaped_row(
                StatusBar::pl_right_arrow(),
                first_bg,
                bar_bg,
                row,
                rcol,
                1,
                font,
            );
            rcol += 1;
        }

        for (i, seg) in bar.right.iter().enumerate() {
            let text = &seg.text;
            let len = text.chars().count();
            if rcol + len > total_cols {
                break;
            }
            self.push_shaped_row(text, seg.fg, seg.bg, row, rcol, len, font);
            rcol += len;

            if i + 1 < bar.right.len() {
                if powerline {
                    // Powerline: "" with fg = next segment bg, bg = current segment bg.
                    let next_bg = bar.right[i + 1].bg;
                    if rcol + 1 > total_cols {
                        break;
                    }
                    self.push_shaped_row(
                        StatusBar::pl_right_arrow(),
                        next_bg,
                        seg.bg,
                        row,
                        rcol,
                        1,
                        font,
                    );
                    rcol += 1;
                } else {
                    if rcol + rsep_w > total_cols {
                        break;
                    }
                    self.push_shaped_row(" │ ", plain_sep_fg, bar_bg, row, rcol, rsep_w, font);
                    rcol += rsep_w;
                }
            }
        }
    }

    /// Render the unified titlebar: traffic lights reserve, control buttons, and tab pills.
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
        win_w: f32,
        pad_left: f32,
        gpu_pad_y: f32,
        bar_bg: [f32; 4],
        sidebar_visible: bool,
        panel_visible: bool,
        // When `Some`, the active tab pill shows this input string with a cursor instead of its title.
        rename_input: Option<&str>,
        colors: &crate::config::schema::ColorScheme,
    ) {
        let _ = bar_bg;

        // Unified titlebar layout constants (pixels, logical):
        //   [0..76]   traffic lights reserve (native macOS buttons)
        //   [80..102] sidebar toggle button
        //   [106..128] AI panel toggle button
        //   [132..win_w-100] tab pills
        //   [win_w-100..win_w] right-side info reserve
        // All pixel constants are in logical points; scale to physical pixels.
        let sf = self.scale_factor;
        let traffic_lights_reserve = 76.0 * sf;
        let btn_w = 22.0 * sf;
        let btn_y = 4.0 * sf;
        let btn_h = 22.0 * sf;
        let btn_gap = 4.0 * sf;
        let sidebar_btn_x = traffic_lights_reserve + btn_gap;
        let ai_btn_x = sidebar_btn_x + btn_w + btn_gap;
        let tabs_start_x = ai_btn_x + btn_w + btn_gap;
        let right_reserve = 100.0 * sf;
        let titlebar_h = super::TITLEBAR_HEIGHT * sf;

        let active_fg = colors.foreground;
        let inactive_fg = colors.ui_muted;
        let btn_color = colors.ui_surface;
        let transparent = [0.0f32; 4];

        let cell_w = self.shaper.cell_width;
        let cell_h = self.shaper.cell_height;
        let radius = (btn_h / 4.0).round();

        // Pill geometry (vertically centered in titlebar_h):
        let pill_y = btn_y;
        let pill_h = titlebar_h - 2.0 * btn_y;

        // Text row: position text vertically centered in the pill.
        let text_top_y = pill_y + (pill_h - cell_h).max(0.0) / 2.0;
        let text_row_f = (text_top_y - gpu_pad_y) / cell_h;

        let btn_active = {
            let [r, g, b, _] = colors.ui_accent;
            [r * 0.6, g * 0.6, b * 0.6, 1.0]
        };

        // ── Left control buttons ─────────────────────────────────────────
        self.rect_instances.push(RoundedRectInstance {
            rect: [sidebar_btn_x, btn_y, btn_w, btn_h],
            color: if sidebar_visible {
                btn_active
            } else {
                btn_color
            },
            radius,
            border_width: 0.0,
            _pad: [0.0; 2],
        });
        self.rect_instances.push(RoundedRectInstance {
            rect: [ai_btn_x, btn_y, btn_w, btn_h],
            color: if panel_visible { btn_active } else { btn_color },
            radius,
            border_width: 0.0,
            _pad: [0.0; 2],
        });
        // ── Button icons ─────────────────────────────────────────────────
        let mk_icon_x =
            |btn_x: f32| -> f32 { (btn_x + (btn_w - cell_w) / 2.0 - pad_left) / cell_w };
        let icon_dim = colors.ui_muted;
        let icon_lit = colors.foreground;
        let sidebar_icon_col = mk_icon_x(sidebar_btn_x);
        let ai_icon_col = mk_icon_x(ai_btn_x);

        let push_btn_icon = |this: &mut Self, glyph: &str, grid_x: f32, fg: [f32; 4]| {
            let start = this.instances.len();
            this.push_shaped_row(glyph, fg, transparent, 0, 0, 1, font);
            for inst in &mut this.instances[start..] {
                inst.grid_pos[0] = grid_x;
                inst.grid_pos[1] = text_row_f;
            }
        };
        push_btn_icon(
            self,
            "≡",
            sidebar_icon_col,
            if sidebar_visible { icon_lit } else { icon_dim },
        );
        push_btn_icon(
            self,
            "✦",
            ai_icon_col,
            if panel_visible { icon_lit } else { icon_dim },
        );

        // ── Flat tabs (only when 2+ tabs) ────────────────────────────────
        if tabs.len() <= 1 || total_cols == 0 {
            return;
        }

        // Active tab: flat subtle rect, not a pill. Inactive: transparent bg.
        let active_tab_bg = colors.ui_surface_active;
        let flat_radius = 2.0 * sf;

        let effective_tabs_start = tabs_start_x.max(pad_left);
        let tabs_start_col = ((effective_tabs_start - pad_left) / cell_w).ceil().max(0.0) as usize;
        let tab_end_col = {
            let avail_w = (win_w - right_reserve).max(effective_tabs_start);
            (((avail_w - pad_left) / cell_w).max(0.0)) as usize
        };
        let max_cols = tab_end_col.min(total_cols);

        let mut col = tabs_start_col;

        for (i, tab) in tabs.iter().enumerate() {
            if col >= max_cols {
                break;
            }

            let is_active = i == active_idx;
            let fg = if is_active { active_fg } else { inactive_fg };

            col += 1; // gap before tab
            if col >= max_cols {
                break;
            }

            // Combined flat label: "title: N" (e.g. "zsh: 1")
            let raw_label = if is_active {
                if let Some(input) = rename_input {
                    format!(" {}▌ ", input)
                } else {
                    format!(" {}: {} ", tab.title, i + 1)
                }
            } else {
                format!(" {}: {} ", tab.title, i + 1)
            };
            let label: String = raw_label.chars().take(18).collect();
            let label_w = label.chars().count().min(max_cols - col);

            let tab_x = pad_left + col as f32 * cell_w;
            let tab_w = label_w as f32 * cell_w;

            let tab_accent = tab.accent_color.unwrap_or(colors.ui_accent);
            if tab_w > 0.0 {
                let underline_h = (1.5 * sf).max(1.0);
                if is_active {
                    // Active: background + accent underline
                    self.rect_instances.push(RoundedRectInstance {
                        rect: [tab_x, pill_y, tab_w, pill_h],
                        color: active_tab_bg,
                        radius: flat_radius,
                        border_width: 0.0,
                        _pad: [0.0; 2],
                    });
                    self.rect_instances.push(RoundedRectInstance {
                        rect: [tab_x, pill_y + pill_h - underline_h, tab_w, underline_h],
                        color: tab_accent,
                        radius: 0.0,
                        border_width: 0.0,
                        _pad: [0.0; 2],
                    });
                } else if tab.accent_color.is_some() {
                    // Inactive with custom color: underline only
                    self.rect_instances.push(RoundedRectInstance {
                        rect: [tab_x, pill_y + pill_h - underline_h, tab_w, underline_h],
                        color: tab_accent,
                        radius: 0.0,
                        border_width: 0.0,
                        _pad: [0.0; 2],
                    });
                }
            }

            if label_w > 0 {
                let start = self.instances.len();
                self.push_shaped_row(&label, fg, transparent, 0, col, label_w, font);
                for inst in &mut self.instances[start..] {
                    inst.grid_pos[1] = text_row_f;
                }
            }
            col += label_w;
        }
    }

    /// Render a scroll bar on the right edge of the terminal (overlays rightmost ~6px of the
    /// last terminal column). Only emits instances when history_size > 0.
    pub fn build_scroll_bar_instances(
        &mut self,
        display_offset: usize,
        history_size: usize,
        screen_rows: usize,
        term_cols: usize,
        colors: &crate::config::schema::ColorScheme,
    ) {
        if history_size == 0 || screen_rows == 0 || term_cols == 0 {
            return;
        }

        const SCROLLBAR_PX: f32 = 6.0;
        let track_color = colors.ui_surface;
        let thumb_color = colors.ui_muted;

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

        let col = (term_cols - 1) as f32;
        let x_off = [cell_w - SCROLLBAR_PX, 0.0];

        // Track — 1 rect covering the full scroll bar column height.
        self.instances.push(CellVertex {
            grid_pos: [col, 0.0],
            atlas_uv: [0.0; 4],
            fg: [0.0; 4],
            bg: track_color,
            glyph_offset: x_off,
            glyph_size: [SCROLLBAR_PX, screen_rows as f32 * cell_h],
            flags: FLAG_CURSOR,
            _pad: 0,
        });

        // Thumb — 1 rect drawn on top of track (painter's order → overwrites track pixels).
        self.instances.push(CellVertex {
            grid_pos: [col, thumb_start as f32],
            atlas_uv: [0.0; 4],
            fg: [0.0; 4],
            bg: thumb_color,
            glyph_offset: x_off,
            glyph_size: [SCROLLBAR_PX, thumb_rows as f32 * cell_h],
            flags: FLAG_CURSOR,
            _pad: 0,
        });
    }
}

fn idx_or_default<T: Clone + Default>(slice: &[T], i: usize) -> T {
    slice.get(i).cloned().unwrap_or_default()
}

fn resolve_line_fg(
    kind: &BlockKind,
    base_fg: [f32; 4],
    colors: &crate::config::schema::ColorScheme,
) -> [f32; 4] {
    match kind {
        BlockKind::Heading(1) => colors.ui_accent,
        BlockKind::Heading(2) => colors.ansi[6], // cyan
        BlockKind::Heading(_) => colors.ansi[3], // yellow
        BlockKind::CodeBlock { .. } => colors.ansi[2], // green
        _ => base_fg,
    }
}

fn resolve_span_fg(
    span_kind: &SpanKind,
    line_fg: [f32; 4],
    colors: &crate::config::schema::ColorScheme,
) -> [f32; 4] {
    match span_kind {
        SpanKind::Bold => brighten(line_fg, 0.2),
        SpanKind::Italic => dim(line_fg, 0.15),
        SpanKind::Code => colors.ansi[2],
        SpanKind::Syntax(TokenKind::Keyword) => colors.ansi[5], // magenta/purple
        SpanKind::Syntax(TokenKind::StringLit) => colors.ansi[3], // yellow
        SpanKind::Syntax(TokenKind::Comment) => colors.ui_muted,
        SpanKind::Syntax(TokenKind::Number) => colors.ansi[6], // cyan
        SpanKind::Syntax(TokenKind::Operator) => dim(line_fg, 0.1),
        SpanKind::Syntax(TokenKind::Default) => line_fg,
    }
}

fn brighten(c: [f32; 4], amount: f32) -> [f32; 4] {
    [
        (c[0] + amount).min(1.0),
        (c[1] + amount).min(1.0),
        (c[2] + amount).min(1.0),
        c[3],
    ]
}

fn dim(c: [f32; 4], amount: f32) -> [f32; 4] {
    [
        (c[0] - amount).max(0.0),
        (c[1] - amount).max(0.0),
        (c[2] - amount).max(0.0),
        c[3],
    ]
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
        colors: &crate::config::schema::ColorScheme,
    ) {
        if total_cols == 0 || total_rows == 0 {
            return;
        }

        let bar_bg = colors.ui_surface;
        let query_fg = colors.foreground;
        let count_fg = colors.ui_accent;
        let hint_fg = colors.ui_muted;
        let cursor_fg = colors.ui_accent;

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

        let bar_width =
            (query_display.chars().count() + count_display.chars().count() + hint.chars().count())
                .max(24)
                .min(total_cols);

        let col_offset = total_cols.saturating_sub(bar_width);
        let row = 0usize; // top row

        // Segment 1: query
        let q_width = query_display.chars().count().min(bar_width);
        self.push_shaped_row(
            &query_display,
            query_fg,
            bar_bg,
            row,
            col_offset,
            q_width,
            font,
        );

        // Segment 2: match count
        let mut seg_offset = col_offset + q_width;
        if !count_display.is_empty() {
            let c_width = count_display
                .chars()
                .count()
                .min(bar_width.saturating_sub(q_width));
            self.push_shaped_row(
                &count_display,
                count_fg,
                bar_bg,
                row,
                seg_offset,
                c_width,
                font,
            );
            seg_offset += c_width;
        }

        // Segment 3: hint
        let remaining = bar_width.saturating_sub(seg_offset - col_offset);
        if remaining > 0 {
            self.push_shaped_row(hint, hint_fg, bar_bg, row, seg_offset, remaining, font);
        }

        // Cursor blink at end of query (a 1-cell colored block)
        let cursor_col = col_offset + 1 + search.query.chars().count() + 1; // after the /query
        if cursor_col < col_offset + q_width {
            self.instances.push(CellVertex {
                grid_pos: [cursor_col as f32, row as f32],
                atlas_uv: [0.0; 4],
                fg: cursor_fg,
                bg: cursor_fg,
                glyph_offset: [0.0; 2],
                glyph_size: [0.0; 2],
                flags: 0,
                _pad: 0,
            });
        }
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
        colors: &crate::config::schema::ColorScheme,
    ) {
        if !self.hud_visible {
            return;
        }

        let hud_bg = colors.ui_overlay;
        let title_fg = colors.ui_accent;
        let value_fg = colors.ansi[3];
        let warn_fg = colors.ansi[1];

        let hud_width = 56usize;

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

        // ── Latency percentiles ──────────────────────────────────────────────
        let (lat_p50, lat_p95, lat_p99) = if self.latency_samples.len() < 2 {
            (0.0f32, 0.0f32, 0.0f32)
        } else {
            let mut s: Vec<f32> = self.latency_samples.iter().copied().collect();
            let n = s.len();
            s.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            (
                s[n / 2],
                s[(n * 95 / 100).min(n - 1)],
                s[(n * 99 / 100).min(n - 1)],
            )
        };

        // ── Shape cache ──────────────────────────────────────────────────────
        let total_shapes = self.shape_cache_hits + self.shape_cache_misses;
        let hit_pct = (self.shape_cache_hits * 100)
            .checked_div(total_shapes)
            .unwrap_or(0) as u32;

        // ── Atlas fill ───────────────────────────────────────────────────────
        let atlas_pct = self.renderer.atlas.current_fill_percent();
        let shape_hits = self.shape_cache_hits;
        let shape_misses = self.shape_cache_misses;
        let instance_count = self.last_instance_count;
        let upload_kb = self.last_gpu_upload_bytes as f32 / 1024.0;

        // ── Build HUD text lines ─────────────────────────────────────────────
        let frame_fg = if avg_ms > 16.67 { warn_fg } else { value_fg };

        let lat_fg = if lat_p99 > 8.0 { warn_fg } else { value_fg };
        let n_samples = self.latency_samples.len();

        let hud_lines: Vec<(String, [f32; 4])> = vec![
            (" F12 HUD".to_string(), title_fg),
            (
                format!(
                    " {:10} {:.1}ms  p50:{:.1}ms  p95:{:.1}ms",
                    "frame", avg_ms, p50_ms, p95_ms
                ),
                frame_fg,
            ),
            (
                format!(
                    " {:10} p50:{:.1}ms  p95:{:.1}ms  p99:{:.1}ms  n={}",
                    "latency", lat_p50, lat_p95, lat_p99, n_samples
                ),
                lat_fg,
            ),
            (
                format!(
                    " {:10} hits={} miss={} ({}%)",
                    "shape", shape_hits, shape_misses, hit_pct
                ),
                value_fg,
            ),
            (format!(" {:10} {}", "instances", instance_count), value_fg),
            (format!(" {:10} {:.1}%", "atlas", atlas_pct), value_fg),
            (
                format!(" {:10} {:.1} KB/frame", "upload", upload_kb),
                value_fg,
            ),
        ];

        for (row, (text, fg)) in hud_lines.iter().enumerate() {
            self.push_shaped_row(text, *fg, hud_bg, row, 0, hud_width, font);
        }
    }

    /// Render a toast notification in the top-right corner.
    pub fn build_toast_instances(
        &mut self,
        msg: &str,
        font: &crate::config::schema::FontConfig,
        total_cols: usize,
        pad_x: f32,
        pad_y: f32,
        colors: &crate::config::schema::ColorScheme,
    ) {
        let toast_width = (msg.len() + 4).min(total_cols);
        if toast_width == 0 || total_cols < toast_width {
            return;
        }

        let bg = colors.ui_overlay;
        let fg = colors.foreground;
        let border_color = colors.ui_accent;

        let cw = self.shaper.cell_width;
        let ch = self.shaper.cell_height;
        let v_pad = ch * 0.4;
        let rect_h = ch + v_pad * 2.0;
        let start_col = total_cols - toast_width;
        let px = pad_x + start_col as f32 * cw;
        // text renders at text_y; rect is centered around it with v_pad above and below
        let text_y = pad_y + ch * 0.5;
        let py = text_y - v_pad;
        let pw = toast_width as f32 * cw;
        let radius = 10.0 * self.scale_factor;
        let border = 1.5 * self.scale_factor;

        self.rect_instances.push(RoundedRectInstance {
            rect: [
                px - border,
                py - border,
                pw + 2.0 * border,
                rect_h + 2.0 * border,
            ],
            color: border_color,
            radius: radius + border,
            border_width: 0.0,
            _pad: [0.0; 2],
        });
        self.rect_instances.push(RoundedRectInstance {
            rect: [px, py, pw, rect_h],
            color: bg,
            radius,
            border_width: 0.0,
            _pad: [0.0; 2],
        });

        let label = format!("  {msg}  ");
        // Offset text down by v_pad to center it vertically inside the taller rect.
        self.push_shaped_row(&label, fg, [0.0; 4], 0, start_col, toast_width, font);
    }
}

/// Build the token usage hint string for the chat panel hint row.
///
/// Shows actual API token counts (prompt/completion) and a progress bar when
/// context window size is known. Falls back to char-based estimation otherwise.
fn build_usage_hint(panel: &ChatPanel) -> String {
    let prompt = panel.last_prompt_tokens;
    let completion = panel.last_completion_tokens;

    if prompt > 0 || completion > 0 {
        if let Some(window) = panel.context_window {
            let total = prompt + completion;
            let pct = ((total as f32 / window as f32) * 100.0).min(100.0) as u32;
            let filled = ((total as f32 / window as f32) * 8.0).min(8.0) as usize;
            let bar: String = (0..8)
                .map(|i| if i < filled { '\u{2588}' } else { '\u{2591}' })
                .collect();
            let window_k = window / 1_000;
            format!("[{bar}]{pct}%  \u{2191}{prompt} \u{2193}{completion} ({window_k}k)")
        } else {
            format!("\u{2191}{prompt} \u{2193}{completion}")
        }
    } else {
        let estimated = panel.estimated_tokens();
        format!("~{estimated} tks")
    }
}

fn short_chat_header_model_name(model: &str) -> String {
    let stripped = model
        .rsplit('/')
        .next()
        .unwrap_or(model)
        .rsplit(':')
        .next()
        .unwrap_or(model);
    truncate_chars(stripped, 8)
}

fn truncate_chars(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    if max_chars == 0 {
        return String::new();
    }
    if max_chars == 1 {
        return "…".to_string();
    }
    let mut out: String = text.chars().take(max_chars - 1).collect();
    out.push('…');
    out
}
