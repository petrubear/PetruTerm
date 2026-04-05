use anyhow::Result;
use std::sync::Arc;
use winit::window::Window;

use crate::config::Config;
use crate::font::{build_font_system, ShapedGlyph, TextShaper};
use crate::renderer::cell::{CellVertex, FLAG_CURSOR, FLAG_LCD};
use crate::renderer::GpuRenderer;
use crate::term::{CursorInfo, CursorShape};
use crate::term::color::resolve_color;
use alacritty_terminal::vte::ansi::Color as AnsiColor;
use crate::llm::chat_panel::{ChatPanel, PanelState};
use crate::ui::CommandPalette;

/// Cache for a single shaped row to avoid re-shaping every frame.
#[derive(Clone)]
pub struct RowCacheEntry {
    pub hash: u64,
    pub glyphs: Vec<ShapedGlyph>,
    pub instances: Vec<CellVertex>,
    pub lcd_instances: Vec<CellVertex>,
}

/// Tracks shaped data for every visible row in the active terminal viewport.
pub struct RowCache {
    pub rows: Vec<Option<RowCacheEntry>>,
    pub dirty_rows: Vec<bool>,
    pub terminal_id: Option<usize>,
    pub font_hash: u64,
}

impl RowCache {
    pub fn new() -> Self {
        Self { rows: Vec::new(), dirty_rows: Vec::new(), terminal_id: None, font_hash: 0 }
    }

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
    pub row_cache: RowCache,
    pub instances: Vec<CellVertex>,
    pub lcd_instances: Vec<CellVertex>,
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
            row_cache: RowCache::new(),
            instances: Vec::new(),
            lcd_instances: Vec::new(),
        })
    }

    /// Returns the font config with size scaled to physical pixels.
    pub fn scaled_font_config(&self, config: &Config) -> crate::config::schema::FontConfig {
        let mut cfg = config.font.clone();
        cfg.size *= self.scale_factor;
        cfg
    }

    pub fn build_instances(
        &mut self,
        cell_data: &[(String, Vec<(AnsiColor, AnsiColor)>)],
        config: &Config,
        font: &crate::config::schema::FontConfig,
        cursor: Option<&CursorInfo>,
        cursor_blink_on: bool,
        terminal_id: usize,
    ) -> Result<(), crate::renderer::atlas::AtlasError> {
        self.instances.clear();
        self.lcd_instances.clear();

        if self.row_cache.rows.len() < cell_data.len() {
            self.row_cache.rows.resize(cell_data.len(), None);
            self.row_cache.dirty_rows.resize(cell_data.len(), true);
        }

        if self.row_cache.terminal_id != Some(terminal_id) {
            self.row_cache.clear();
            self.row_cache.terminal_id = Some(terminal_id);
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

            if let Some(Some(entry)) = self.row_cache.rows.get(row_idx) {
                if entry.hash == row_hash {
                    self.instances.extend_from_slice(&entry.instances);
                    self.lcd_instances.extend_from_slice(&entry.lcd_instances);
                    continue;
                }
            }

            self.row_cache.dirty_rows[row_idx] = true;
            let mut row_instances = Vec::new();
            let mut row_lcd_instances = Vec::new();

            let shaped = self.shaper.shape_line(text, &colors, font);

            for glyph in &shaped.glyphs {
                // Try LCD rasterization first (when LCD AA is enabled).
                // If it succeeds, the LCD instance paints the glyph and the swash
                // instance is reduced to background-only (glyph_size = 0) so we
                // never double-render the same character.
                let lcd_entry = if let Some(queue) = self.renderer.lcd_queue() {
                    self.shaper
                        .rasterize_lcd_to_atlas(glyph.cache_key, glyph.ch, queue)
                } else {
                    None
                };

                // Swash instance — always emitted for the background color rect.
                // Only carries glyph data when LCD did not produce an entry.
                let (atlas, queue) = self.renderer.atlas_and_queue();
                let swash_entry = self.shaper.rasterize_to_atlas(glyph.cache_key, atlas, queue)?;

                let (atlas_uv, glyph_offset, glyph_size) = if lcd_entry.is_none() {
                    // No LCD — use swash glyph.
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
                    // LCD handles the glyph — swash provides background only.
                    ([0.0f32; 4], [0.0; 2], [0.0; 2])
                };

                row_instances.push(CellVertex {
                    grid_pos: [glyph.col as f32, row_idx as f32],
                    atlas_uv,
                    fg: glyph.fg,
                    bg: glyph.bg,
                    glyph_offset,
                    glyph_size,
                    flags: 0,
                    _pad: 0,
                });

                // LCD instance — emitted only when FreeType successfully rasterized the glyph.
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

            self.instances.extend_from_slice(&row_instances);
            self.lcd_instances.extend_from_slice(&row_lcd_instances);

            self.row_cache.rows[row_idx] = Some(RowCacheEntry {
                hash: row_hash,
                glyphs: shaped.glyphs,
                instances: row_instances,
                lcd_instances: row_lcd_instances,
            });
        }

        self.row_cache.terminal_id = Some(terminal_id);

        if let Some(info) = cursor {
            if info.visible && cursor_blink_on {
                let cw = self.shaper.cell_width;
                let ch = self.shaper.cell_height;

                let (glyph_offset, glyph_size) = match info.shape {
                    CursorShape::Block | CursorShape::HollowBlock => {
                        ([0.0f32, 0.0], [cw, ch])
                    }
                    CursorShape::Underline => {
                        ([0.0, (ch - 2.0).max(0.0)], [cw, 2.0])
                    }
                    CursorShape::Beam => {
                        ([0.0, 0.0], [2.0, ch])
                    }
                    CursorShape::Hidden => ([0.0; 2], [0.0; 2]),
                };

                self.instances.push(CellVertex {
                    grid_pos:     [info.col as f32, info.row as f32],
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
            .chain(std::iter::repeat(' ').take(width.saturating_sub(len)))
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
                    width: 0, height: 0, bearing_x: 0, bearing_y: 0,
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

            self.instances.push(CellVertex {
                grid_pos: [(col_offset + glyph.col) as f32, row as f32],
                atlas_uv,
                fg,
                bg,
                glyph_offset,
                glyph_size,
                flags: 0,
                _pad: 0,
            });
        }
    }

    pub fn build_chat_panel_instances(
        &mut self,
        panel: &ChatPanel,
        panel_focused: bool,
        config: &Config,
        font: &crate::config::schema::FontConfig,
        term_cols: usize,
        screen_rows: usize,
        cursor_blink_on: bool,
    ) {
        use crate::llm::chat_panel::{word_wrap, wrap_input};
        use crate::llm::ChatRole;

        let panel_cols = panel.width_cols as usize;
        if panel_cols == 0 || screen_rows < 6 { return; }

        // ── Colors (Dracula Pro palette) ─────────────────────────────────────
        let panel_bg = config.llm.ui.background;
        let user_fg  = config.llm.ui.user_fg;
        let asst_fg  = config.llm.ui.assistant_fg;
        let input_fg = config.llm.ui.input_fg;

        const BORDER_FG: [f32; 4] = [0.58, 0.50, 1.00, 1.0]; // purple
        const BORDER_DIM:[f32; 4] = [0.32, 0.28, 0.50, 1.0]; // dimmed purple
        const STREAM_FG: [f32; 4] = [0.95, 0.98, 0.55, 1.0]; // yellow
        const HINT_FG:   [f32; 4] = [0.38, 0.44, 0.64, 1.0]; // comment gray
        const ERR_FG:    [f32; 4] = [1.00, 0.33, 0.33, 1.0]; // red
        const SEP_FG:    [f32; 4] = [0.27, 0.28, 0.36, 1.0]; // current-line
        const DIM_FG:    [f32; 4] = [0.50, 0.47, 0.60, 1.0]; // dimmed input

        // Braille spinner cycles as streaming buffer grows
        const SPIN: [&str; 8] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧"];
        let spin = SPIN[panel.streaming_buf.chars().count() % 8];

        // ── Layout ───────────────────────────────────────────────────────────
        // Bottom 4 rows: separator, input line 1, input line 2, hints
        let hints_row  = screen_rows - 1;
        let input_row2 = screen_rows - 2;
        let input_row1 = screen_rows - 3;
        let sep_row    = screen_rows - 4;
        // Rows 1..sep_row are the scrollable message history
        let history_rows = sep_row.saturating_sub(1);

        // "│  You  " / "│   AI  " prefix = 8 chars; inner content fills the rest
        let msg_inner_w = panel_cols.saturating_sub(8);
        let co = term_cols; // grid column where panel begins

        let border_fg = if panel_focused { BORDER_FG } else { BORDER_DIM };

        // ── Header ───────────────────────────────────────────────────────────
        let title = " Petrubot ";
        let left  = "│───";
        let dashes = panel_cols.saturating_sub(left.chars().count() + title.chars().count());
        let header = format!("{}{}{}", left, title, "─".repeat(dashes));
        self.push_shaped_row(&header, border_fg, panel_bg, 0, co, panel_cols, font);

        // ── Build message lines ───────────────────────────────────────────────
        let mut all_lines: Vec<(String, [f32; 4])> = Vec::new();

        for msg in &panel.messages {
            let (first_p, cont_p, fg) = match msg.role {
                ChatRole::User      => ("│  You  ", "│       ", user_fg),
                ChatRole::Assistant => ("│   AI  ", "│       ", asst_fg),
                ChatRole::System    => continue,
            };
            let wrapped = word_wrap(&msg.content, msg_inner_w);
            for (i, line) in wrapped.iter().enumerate() {
                let p = if i == 0 { first_p } else { cont_p };
                all_lines.push((format!("{}{}", p, line), fg));
            }
            // blank line between messages
            all_lines.push(("│".to_string(), SEP_FG));
        }

        // Streaming tokens (in-flight assistant response)
        if panel.is_streaming() && !panel.streaming_buf.is_empty() {
            let wrapped = word_wrap(&panel.streaming_buf, msg_inner_w);
            for (i, line) in wrapped.iter().enumerate() {
                let p = if i == 0 { "│   AI  " } else { "│       " };
                all_lines.push((format!("{}{}", p, line), STREAM_FG));
            }
        }

        // Loading placeholder (waiting for first token)
        if matches!(panel.state, PanelState::Loading) {
            all_lines.push((format!("│   {}  Thinking\u{2026}", spin), STREAM_FG));
        }

        // Error
        if let PanelState::Error(ref err) = panel.state {
            let wrapped = word_wrap(err, msg_inner_w);
            for (i, line) in wrapped.iter().enumerate() {
                let p = if i == 0 { "│  \u{2717}    " } else { "│       " }; // ✗
                all_lines.push((format!("{}{}", p, line), ERR_FG));
            }
        }

        // ── Render visible history ────────────────────────────────────────────
        let visible_start = all_lines.len()
            .saturating_sub(history_rows + panel.scroll_offset);

        for i in 0..history_rows {
            let row = 1 + i;
            let (text, fg) = all_lines
                .get(visible_start + i)
                .map(|(t, f)| (t.as_str(), *f))
                .unwrap_or(("│", SEP_FG));
            self.push_shaped_row(text, fg, panel_bg, row, co, panel_cols, font);
        }

        // ── Separator ────────────────────────────────────────────────────────
        let sep = format!("│{}", "─".repeat(panel_cols.saturating_sub(1)));
        self.push_shaped_row(&sep, SEP_FG, panel_bg, sep_row, co, panel_cols, font);

        // ── Input field ──────────────────────────────────────────────────────
        // "│ ▸  " = 5 chars; remaining width for text
        let input_inner_w = panel_cols.saturating_sub(5);
        let mut input_display = panel.input.clone();
        if panel_focused && cursor_blink_on && panel.is_idle() {
            input_display.push('\u{258b}'); // ▋ block cursor
        }
        let input_lines = wrap_input(&input_display, input_inner_w);
        let inp_fg = if panel_focused { input_fg } else { DIM_FG };
        let line1 = format!("│ \u{25b8}  {}", input_lines.first().cloned().unwrap_or_default()); // ▸
        let line2 = format!("│    {}", input_lines.get(1).cloned().unwrap_or_default());
        self.push_shaped_row(&line1, inp_fg, panel_bg, input_row1, co, panel_cols, font);
        self.push_shaped_row(&line2, inp_fg, panel_bg, input_row2, co, panel_cols, font);

        // ── Key hints ────────────────────────────────────────────────────────
        let has_assistant = panel.messages.iter().any(|m| matches!(m.role, ChatRole::Assistant));
        let hints = if !panel_focused {
            "│ Cmd+Shift+A: focus / close"
        } else {
            match &panel.state {
                PanelState::Idle if !panel.input.trim().is_empty()
                    => "│ Enter: send   Esc: close",
                PanelState::Idle if has_assistant
                    => "│ Enter: run last cmd   Esc: close",
                PanelState::Idle
                    => "│ Enter: send   Esc: close",
                PanelState::Loading | PanelState::Streaming
                    => "│ streaming\u{2026}   Cmd+Shift+A: close",
                PanelState::Error(_)
                    => "│ Esc: dismiss   Cmd+Shift+A: close",
                PanelState::Hidden => "│",
            }
        };
        self.push_shaped_row(hints, HINT_FG, panel_bg, hints_row, co, panel_cols, font);
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

        for i in 0..(palette_height - 1) {
            let row = start_row + 1 + i;
            let is_selected = i == palette.selected;
            let current_bg = if is_selected { highlight_bg } else { bg };

            let text = if let Some(action) = palette.results.get(i) {
                format!("  {}", action.name)
            } else {
                String::new()
            };

            self.push_shaped_row(&text, fg, current_bg, row, start_col, palette_width, font);
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
