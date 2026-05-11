use super::*;

impl RenderContext {
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

        let keybind_fg = colors.ui_muted;

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

        let mut bg = colors.ui_overlay;
        bg[3] = 1.0;
        let fg = colors.foreground;
        let mut hover_bg = colors.ui_surface_hover;
        hover_bg[3] = 1.0;
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

        let sb_colors = colors.status_bar_colors();
        let bar_bg = StatusBar::bar_bg(&sb_colors);

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
        let titlebar_h = crate::app::TITLEBAR_HEIGHT * sf;

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
        pad_right: bool,
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

        // When there is a right separator, the pane has an unused pad cell between the last
        // content column and the separator.  Use that cell so the scrollbar sits flush against
        // the border rather than leaving a cell-wide gap.
        let col = if pad_right {
            term_cols as f32
        } else {
            (term_cols - 1) as f32
        };
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
