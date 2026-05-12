use super::*;

impl RenderContext {
    /// Build and append cell instances for one pane's terminal.
    ///
    /// Instances are APPENDED to `self.instances` (not cleared); call `begin_frame()` first.
    /// `col_offset` and `row_offset` position this pane within the global grid coordinate space.
    ///
    /// Two sequential phases:
    ///   Phase 1 (serial): resolve colors, hash, shape+rasterize cache misses, populate row_caches.
    ///   Phase 2 (serial): apply pane offsets from cache into output buffers.
    ///
    /// NOTE: rayon was evaluated for Phase 2 (vertex offset application) but found to be 14x
    /// slower than serial at 200 rows due to fork-join overhead (~130 µs) exceeding the work
    /// (~10 µs). Rayon is reserved for higher-granularity tasks (search, batch rasterization).
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

        let n = cell_data.len();

        // Ensure the per-terminal row cache exists and has enough slots.
        {
            let cache = self
                .row_caches
                .entry(terminal_id)
                .or_insert_with(RowCache::new);
            if cache.rows.len() < n {
                cache.rows.resize(n, None);
            }
        }

        // ── Phase 1: serial — shape + rasterize cache misses, populate row_caches ──────
        //
        // No vertex emission here. All emission happens in phase 2 so that
        // the cache is fully populated before the parallel read pass begins.
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

            // Cache hit: increment counter and skip shaping for this row.
            let is_hit = self
                .row_caches
                .get(&terminal_id)
                .and_then(|c| c.rows.get(row_idx))
                .and_then(|e| e.as_ref())
                .is_some_and(|e| e.hash == row_hash);

            if is_hit {
                self.shape_cache_hits = self.shape_cache_hits.saturating_add(1);
                continue;
            }
            self.shape_cache_misses = self.shape_cache_misses.saturating_add(1);

            // Cache miss: shape and rasterize (must remain serial — shaper and atlas
            // are not thread-safe; wgpu::Queue writes cannot be parallelized here).
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
                    let (atlas, color_atlas, queue) = self.renderer.atlases_and_queue();
                    let se = self.shaper.rasterize_to_atlas(
                        glyph.cache_key,
                        atlas,
                        color_atlas,
                        queue,
                    )?;
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

            // Store local coordinates in cache. Emission happens in phase 2.
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

        // ── Phase 2: emit all rows with pane offset applied ──────────────────────────
        if let Some(cache) = self.row_caches.get(&terminal_id) {
            let rows = &cache.rows[..n.min(cache.rows.len())];
            let co = col_offset as f32;
            for (row_idx, entry_opt) in rows.iter().enumerate() {
                let Some(entry) = entry_opt.as_ref() else {
                    continue;
                };
                let ro = (row_offset + row_idx) as f32;
                for inst in &entry.instances {
                    self.instances.push(CellVertex {
                        grid_pos: [inst.grid_pos[0] + co, ro],
                        ..*inst
                    });
                }
                for inst in &entry.lcd_instances {
                    self.lcd_instances.push(CellVertex {
                        grid_pos: [inst.grid_pos[0] + co, ro],
                        ..*inst
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
    pub fn build_pane_separators(
        &mut self,
        separators: &[PaneSeparator],
        pad_x: f32,
        pad_y: f32,
        colors: &crate::config::schema::ColorScheme,
    ) {
        let sep_color = colors.ui_border;
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
                color: sep_color,
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

        // Boundary edges (no adjacent separator) are pushed far off-screen so they are
        // GPU-clipped. Using ±cell_dimension is insufficient — the window padding and tab
        // bar area still fall within screen bounds, so the stroke remains visible there.
        // 9999 px is beyond any realistic display size and guarantees clipping.
        // Separator-adjacent edges use `inset` so the stroke sits in the separator gap.
        let off = 9999.0_f32;
        let x = if focused.col_offset == 0 {
            -off
        } else {
            focused.pane_rect.x + inset
        };
        let y = if focused.pad_top {
            focused.pane_rect.y + inset
        } else {
            -off
        };
        let right = if focused.pad_right {
            focused.pane_rect.x + focused.pane_rect.w - inset
        } else {
            focused.pane_rect.x + focused.pane_rect.w + off
        };
        let bottom = if focused.pad_bottom {
            focused.pane_rect.y + focused.pane_rect.h - inset
        } else {
            focused.pane_rect.y + focused.pane_rect.h + off
        };

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
}
