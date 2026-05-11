use anyhow::Result;
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

mod chat;
mod overlay;
mod terminal;

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
    pub(super) streaming_stable_lines: Vec<AnnotatedLine>,
    /// ParseState carried across stable-line boundaries for streaming markdown.
    pub(super) streaming_fence_state: ParseState,
    /// Byte offset in streaming_buf up to which streaming_stable_lines is valid.
    pub(super) streaming_stable_end: usize,
    /// Panel id and width used for the current streaming cache entry.
    pub(super) streaming_cache_key: Option<(usize, usize)>,
    /// General-purpose format scratch for callers of `push_shaped_row` (TD-PERF-13).
    /// Kept separate from `scratch_str` (used inside push_shaped_row) to avoid borrow conflicts.
    pub fmt_buf: String,
    /// Reusable gap-fill buffer for the status bar spacer (TD-PERF-35).
    pub(super) gap_buf: String,
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
    pub scroll_bar_state: Option<(usize, usize, usize, usize, bool)>,
    pub scroll_bar_cache: Vec<CellVertex>,
    // Tab bar: HarfBuzz per tab name. Cached inputs checked directly (no hash) (TD-PERF-16).
    pub tab_bar_instances_cache: Vec<CellVertex>,
    pub tab_bar_rects_cache: Vec<RoundedRectInstance>,
    pub tab_bar_inputs: Option<(usize, usize, bool, bool)>, // (active_index, total_cols, sidebar_visible, panel_visible)
    /// FxHash of all tab titles concatenated. Replaces Vec<String> to avoid per-frame alloc (AUDIT-PERF-07).
    pub tab_bar_titles_hash: u64,
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
            tab_bar_titles_hash: 0,
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
}

// ── Free functions (used by child modules via `super::`) ──────────────────────

pub(super) fn idx_or_default<T: Clone + Default>(slice: &[T], i: usize) -> T {
    slice.get(i).cloned().unwrap_or_default()
}

pub(super) fn resolve_line_fg(
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

pub(super) fn resolve_span_fg(
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

pub(super) fn brighten(c: [f32; 4], amount: f32) -> [f32; 4] {
    [
        (c[0] + amount).min(1.0),
        (c[1] + amount).min(1.0),
        (c[2] + amount).min(1.0),
        c[3],
    ]
}

pub(super) fn dim(c: [f32; 4], amount: f32) -> [f32; 4] {
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
pub(super) fn pack_color(c: [f32; 4]) -> u32 {
    let r = (c[0].clamp(0.0, 1.0) * 255.0) as u32;
    let g = (c[1].clamp(0.0, 1.0) * 255.0) as u32;
    let b = (c[2].clamp(0.0, 1.0) * 255.0) as u32;
    let a = (c[3].clamp(0.0, 1.0) * 255.0) as u32;
    (r << 24) | (g << 16) | (b << 8) | a
}

/// Approximate color equality using 8-bit quantization (same as `pack_color`).
#[inline]
pub(super) fn colors_approx_eq(a: [f32; 4], b: [f32; 4]) -> bool {
    pack_color(a) == pack_color(b)
}

pub(super) fn calculate_row_hash(text: &str, colors: &[([f32; 4], [f32; 4])]) -> u64 {
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

pub(super) fn short_chat_header_model_name(model: &str) -> String {
    let stripped = model
        .rsplit('/')
        .next()
        .unwrap_or(model)
        .rsplit(':')
        .next()
        .unwrap_or(model);
    truncate_chars(stripped, 8)
}

pub(super) fn truncate_chars(text: &str, max_chars: usize) -> String {
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
