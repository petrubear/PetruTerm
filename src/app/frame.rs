use anyhow::Result;
use winit::event_loop::ActiveEventLoop;

use super::mux::{FlagHintOverlay, GhostOverlay, Mux, SyntaxOverlay};
use super::renderer::{RenderContext, SidebarDrawParams};
use super::App;
use crate::ui::PaneInfo;

impl App {
    pub(super) fn request_redraw(&mut self) {
        self.needs_redraw = true;
    }

    pub(super) fn flush_redraw_request(&mut self) {
        if !self.needs_redraw {
            return;
        }
        // Enforce max_fps cap. If too soon since last frame, leave needs_redraw=true
        // so about_to_wait schedules a WaitUntil at the next frame deadline.
        let fps = self.config.max_fps.max(1) as u64;
        let interval = std::time::Duration::from_nanos(1_000_000_000 / fps);
        if self.last_frame_at.elapsed() < interval {
            return;
        }
        self.needs_redraw = false;
        self.last_frame_at = std::time::Instant::now();
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }

    pub(super) fn close_exited_terminals(&mut self, exited: Vec<usize>) -> bool {
        if exited.is_empty() {
            return false;
        }
        for tid in exited {
            self.terminal_shell_ctxs.remove(&tid);
            self.ui.remove_terminal_state(tid);
            if let Some(rc) = &mut self.render_ctx {
                rc.row_caches.remove(&tid);
            }
            if self.mux.close_terminal(tid) {
                return true;
            }
        }
        self.apply_tab_bar_padding();
        self.resize_terminals_for_panel();
        self.fire_lua_event("terminal_exit");
        false
    }

    pub(super) fn resize_terminals_for_panel(&mut self) {
        let viewport = self.viewport_rect();
        let (cell_w, cell_h) = self.cell_dims();
        self.mux.resize_all(
            viewport,
            self.config.scrollback_lines as usize,
            cell_w,
            cell_h,
        );
        // Panel layout depends on term_cols/screen_rows — rebuild instances after resize.
        self.ui.panel_mut().dirty = true;
    }

    pub(super) fn flush_pending_pty_run(&mut self) {
        if let Some(cmd) = self.ui.pending_pty_run.take() {
            if let Some(terminal) = self.mux.active_terminal() {
                let mut data = cmd.into_bytes();
                data.push(b'\n');
                terminal.write_input(&data);
            }
        }
    }

    pub(super) fn flush_pending_agent_action(&mut self) {
        use crate::llm::agent_action::AgentAction;
        use crate::llm::ChatMessage;
        let Some(action) = self.ui.pending_agent_action.take() else {
            return;
        };
        match action {
            AgentAction::RunCommand { cmd, .. } => {
                let note = format!("Running: `{cmd}`");
                if let Some(terminal) = self.mux.active_terminal() {
                    let mut data = cmd.into_bytes();
                    data.push(b'\n');
                    terminal.write_input(&data);
                }
                let panel = self.ui.panel_mut();
                panel.messages.push(ChatMessage::assistant(note));
                panel.dirty = true;
            }
            AgentAction::OpenFile { path } => {
                let abs = self
                    .mux
                    .active_cwd()
                    .or_else(|| std::env::current_dir().ok())
                    .unwrap_or_default()
                    .join(&path);
                let p = if abs.exists() {
                    abs.to_string_lossy().into_owned()
                } else {
                    path.clone()
                };
                let _ = std::process::Command::new("open").arg(&p).spawn();
                let note = format!("Opening: `{path}`");
                let panel = self.ui.panel_mut();
                panel.messages.push(ChatMessage::assistant(note));
                panel.dirty = true;
            }
            AgentAction::ExplainOutput { last_n_lines } => {
                let output = self.mux.last_terminal_lines(last_n_lines);
                if output.is_empty() {
                    return;
                }
                if !self.ui.is_panel_visible() {
                    self.ui.panel_mut().open();
                    self.resize_terminals_for_panel();
                }
                self.ui.panel_focused = true;
                self.ui.panel_mut().input =
                    format!("Explain this terminal output:\n```\n{output}\n```");
                let cwd = self
                    .mux
                    .active_cwd()
                    .or_else(|| std::env::current_dir().ok())
                    .unwrap_or_default();
                self.ui.submit_ai_query(self.wakeup_proxy.clone(), cwd);
            }
        }
    }

    pub(super) fn flush_pending_paste(&mut self) {
        if let Some(text) = self.ui.poll_pending_paste() {
            if let Some(terminal) = self.mux.active_terminal() {
                if terminal.bracketed_paste_mode() {
                    let mut data = b"\x1b[200~".to_vec();
                    data.extend_from_slice(text.as_bytes());
                    data.extend_from_slice(b"\x1b[201~");
                    terminal.write_input(&data);
                } else {
                    terminal.write_input(text.as_bytes());
                }
            }
        }
    }

    pub(super) fn handle_redraw(&mut self, event_loop: &ActiveEventLoop) {
        // Skip rendering when the window is occluded or minimized.
        if self.window_occluded {
            return;
        }

        #[cfg(feature = "profiling")]
        let _span = tracing::info_span!("redraw_frame").entered();

        let frame_start = std::time::Instant::now();

        if let Some(rc) = &mut self.render_ctx {
            rc.frame_counter = rc.frame_counter.wrapping_add(1);
        }

        self.check_config_reload();
        let (data_ids, exited) = self.mux.poll_pty_events();
        self.mux.apply_osc133_events();
        if self.close_exited_terminals(exited) {
            event_loop.exit();
            return;
        }
        for id in &data_ids {
            self.update_terminal_shell_ctx(*id);
        }
        let panel_ai = self.ui.poll_ai_events();
        let block_ai = self.ui.poll_ai_block_events();
        if panel_ai.completed {
            self.fire_lua_event("ai_response");
        }
        let had_ai = panel_ai.changed;
        let had_ai_block = block_ai.changed;
        if panel_ai.more || block_ai.more {
            self.request_redraw();
        }
        self.flush_pending_pty_run();
        self.flush_pending_agent_action();
        self.flush_pending_paste();
        self.ui.poll_file_scan();
        self.ui.poll_branch_scan();

        // ── Fast blink path ─────────────────────────────────────────────────────
        // When only cursor blink changed (no PTY data, no AI events, no panel
        // cursor), skip the full cell rebuild and update just the cursor vertex.
        // The GPU instance buffer retains cell content from the previous full frame.
        let blink_only = self.cursor_blink_dirty
            && data_ids.is_empty()
            && !had_ai
            && !had_ai_block
            && !self.pending_pty_redraw;
        if blink_only {
            self.cursor_blink_dirty = false;
            if let Some(rc) = &mut self.render_ctx {
                let blink_on = self.input.cursor_blink_on;
                // Upload cursor (or a transparent placeholder) at content_end so the
                // status bar / tab bar / scroll bar at content_end+1.. remain in the
                // GPU buffer and are drawn. cell_count = last_instance_count draws
                // everything; last_overlay_start preserves correct overlay split.
                if let Some(v) = rc.cursor_vertex_template {
                    let upload_v = if blink_on {
                        v
                    } else {
                        // Transparent cursor — shader discards when bg.a < 0.01.
                        crate::renderer::cell::CellVertex {
                            bg: [0.0, 0.0, 0.0, 0.0],
                            ..v
                        }
                    };
                    rc.renderer
                        .upload_instances(std::slice::from_ref(&upload_v), rc.content_end);
                }
                rc.renderer.set_cell_count(rc.last_instance_count);
                rc.renderer.set_overlay_start(rc.last_overlay_start);
                let _ = rc.renderer.render();
            }
            return;
        }

        // Sync per-pane chat panel to the focused terminal.
        let terminal_id = self.mux.focused_terminal_id();
        self.ui.set_active_terminal(terminal_id);

        // Compute viewport and per-pane layout.
        let viewport = self.viewport_rect();
        let (cell_w, cell_h) = self.cell_dims();
        self.separator_snapshot =
            self.mux
                .active_pane_separators(viewport, cell_w as f32, cell_h as f32);

        // Viewport-wide dimensions for overlay positioning.
        let total_cols = (viewport.w / cell_w as f32).floor() as usize;
        let total_rows = (viewport.h / cell_h as f32).floor() as usize;
        // Capture status bar layout values before the mutable borrow of render_ctx.
        let tab_bar_vis = self.tab_bar_visible();
        let sb_pad_y = self.config.window.padding.top as f32 + self.tab_bar_height_px();
        // Capture sidebar width before render_ctx is mutably borrowed.
        let sidebar_px_snapshot = self.sidebar_width_px();
        // Snapshot the active pane's shell context before the render_ctx borrow.
        let sb_exit_code = self.active_shell_ctx().and_then(|c| {
            if c.last_exit_code != 0 {
                Some(c.last_exit_code)
            } else {
                None
            }
        });
        let sb_exit_code_raw = self
            .active_shell_ctx()
            .map(|c| c.last_exit_code)
            .unwrap_or(0);
        // Focused pane dimensions (scroll bar, AI block anchor).
        let (term_cols, term_rows) = self.mux.active_terminal_size();
        // Rebuild MCP tools cache before the render_ctx borrow if dirty (AUDIT-PERF-03).
        if self.mcp_tools_dirty {
            self.rebuild_mcp_cache();
        }
        // H-1: pre-compute link underline rect before the mutable render_ctx borrow.
        let link_underline_rect: Option<crate::renderer::rounded_rect::RoundedRectInstance> =
            self.hover_link.as_ref().and_then(|link| {
                let scale = self.render_ctx.as_ref()?.scale_factor;
                let cw = cell_w as f32;
                let ch = cell_h as f32;
                let (col_off, row_off) = self.mux.focused_pane_offset(viewport, cw, ch);
                let pad_x = self.config.window.padding.left as f32 + sidebar_px_snapshot;
                let x = pad_x + (col_off + link.col_start) as f32 * cw;
                let y = sb_pad_y + (row_off + link.row) as f32 * ch + ch - 1.5 * scale;
                let w = (link.col_end - link.col_start) as f32 * cw;
                Some(crate::renderer::rounded_rect::RoundedRectInstance {
                    rect: [x, y, w, 1.5 * scale],
                    color: self.config.colors.ui_accent,
                    radius: 0.0,
                    border_width: 0.0,
                    _pad: [0.0; 2],
                })
            });

        if let Some(rc) = &mut self.render_ctx {
            // Advance epoch once per frame so LRU eviction can age unused entries.
            rc.renderer.atlas.next_epoch();
            rc.renderer.color_atlas.next_epoch();
            if let Some(lcd) = rc.renderer.get_lcd_atlas() {
                lcd.borrow_mut().next_epoch();
            }

            // Proactive eviction: when the main atlas is 90% full, drop entries not
            // touched in the last 60 frames (~1 second at 60fps).
            if rc.renderer.atlas.is_near_full() {
                let evicted = rc.renderer.atlas.evict_cold(60);
                if evicted > 0 {
                    log::debug!("Atlas eviction: removed {} stale glyphs", evicted);
                }
                // Check cursor position unconditionally — evict_cold() only clears the
                // logical map; the physical cursor does not move back. If the cursor is
                // still near the atlas boundary (whether or not eviction freed anything),
                // a full clear is necessary to avoid an imminent AtlasError::Full stutter.
                if rc.renderer.atlas.cursor_fill_ratio() > 0.75 {
                    rc.renderer.atlas.clear(&rc.renderer.device());
                    rc.renderer.color_atlas.clear(&rc.renderer.device());
                    if let Some(lcd) = rc.renderer.get_lcd_atlas() {
                        lcd.borrow_mut().clear(&rc.renderer.device());
                        rc.shaper.clear_lcd_rasterizer_cache();
                    }
                    rc.renderer.rebuild_atlas_bind_groups();
                    rc.atlas_generation += 1;
                    rc.clear_all_row_caches();
                    log::debug!("Atlas: preemptive clear (cursor_fill_ratio > 0.75)");
                }
            }

            // Proactive eviction for the color atlas (emoji).
            if rc.renderer.color_atlas.is_near_full() {
                let evicted = rc.renderer.color_atlas.evict_cold(60);
                if evicted > 0 {
                    log::debug!("Color atlas: evicted {} cold emoji glyphs", evicted);
                }
                if rc.renderer.color_atlas.cursor_fill_ratio() > 0.75 {
                    rc.renderer.color_atlas.clear(&rc.renderer.device());
                    rc.renderer.rebuild_atlas_bind_groups();
                    rc.clear_all_row_caches();
                }
            }

            // Proactive eviction for the LCD atlas.
            // is_near_full() for LCD is cursor-based (>80% of height); evict_cold()
            // only clears the logical map, so the cursor stays put. If still near full
            // after eviction, a clear is necessary to prevent AtlasError::Full.
            if let Some(lcd) = rc.renderer.get_lcd_atlas() {
                if lcd.borrow().is_near_full() {
                    let evicted = lcd.borrow_mut().evict_cold(60);
                    if evicted > 0 {
                        log::debug!("LCD atlas: evicted {} cold glyphs", evicted);
                    }
                    if lcd.borrow().is_near_full() {
                        lcd.borrow_mut().clear(&rc.renderer.device());
                        rc.shaper.clear_lcd_rasterizer_cache();
                        rc.renderer.rebuild_atlas_bind_groups();
                        rc.clear_all_row_caches();
                        log::debug!(
                            "LCD atlas: preemptive clear (cursor still near full after eviction)"
                        );
                    }
                }
            }

            let scaled_font = rc.scaled_font_config(&self.config);

            // ── Search: run query if dirty, scroll to current match ──────────────
            let active_tid = self.mux.focused_terminal_id();
            if self.ui.search_bar.visible && self.ui.search_bar.dirty {
                let query = self.ui.search_bar.query.clone();
                if query.is_empty() {
                    self.ui.search_bar.set_matches(Vec::new(), false);
                } else {
                    // Incremental path: when the new query extends the previous one,
                    // filter existing matches instead of scanning the full grid (TD-PERF-11).
                    let prev_query = self.ui.search_bar.last_query.clone();
                    let can_filter = !self.ui.search_bar.matches.is_empty()
                        && !self.ui.search_bar.matches_truncated
                        && query.starts_with(prev_query.as_str())
                        && !prev_query.is_empty();
                    let (matches, truncated) = if can_filter {
                        self.mux.filter_matches(&self.ui.search_bar.matches, &query)
                    } else {
                        self.mux.search_active_terminal(&query)
                    };
                    self.ui.search_bar.set_matches(matches, truncated);
                }
                self.ui.search_bar.last_query = query;
                self.ui.search_bar.dirty = false;
            }
            if self.ui.search_bar.visible && self.ui.search_bar.scroll_needed {
                if let Some(m) = self.ui.search_bar.current_match().cloned() {
                    if let Some(terminal) = self.mux.active_terminal() {
                        let (disp_off, _) = terminal.scrollback_info();
                        let screen_rows = terminal.rows as i32;
                        // Target: center the match in the viewport.
                        let target_offset = (screen_rows / 2 - m.grid_line).max(0) as usize;
                        let delta = disp_off as i32 - target_offset as i32;
                        if delta != 0 {
                            terminal.scroll_display(-delta);
                        }
                    }
                }
                self.ui.search_bar.scroll_needed = false;
            }

            // ── Per-pane layout (reuse Vec allocation from rc — TD-PERF-40) ─────
            let mut pane_infos = std::mem::take(&mut rc.pane_infos);
            self.mux.fill_active_pane_infos(
                viewport,
                cell_w as f32,
                cell_h as f32,
                &mut pane_infos,
            );

            // Zoom: if a pane is zoomed, keep only it and expand to fill viewport.
            if let Some(zoomed_tid) = self.mux.zoomed_pane {
                if let Some(idx) = pane_infos.iter().position(|p| p.terminal_id == zoomed_tid) {
                    let cols = (viewport.w / cell_w as f32).floor() as usize;
                    let rows = (viewport.h / cell_h as f32).floor() as usize;
                    let mut zoomed = pane_infos[idx];
                    zoomed.col_offset = 0;
                    zoomed.row_offset = 0;
                    zoomed.cols = cols;
                    zoomed.rows = rows;
                    zoomed.pane_rect = viewport;
                    zoomed.focused = true;
                    zoomed.pad_right = false;
                    zoomed.pad_bottom = false;
                    zoomed.pad_top = false;
                    pane_infos.clear();
                    pane_infos.push(zoomed);
                } else {
                    // Zoomed pane no longer in active tab — clear zoom.
                    self.mux.zoomed_pane = None;
                }
            }

            // ── Build cell instances for every pane ──────────────────────────────
            let search_arg = if self.ui.search_bar.visible {
                Some(&self.ui.search_bar)
            } else {
                None
            };
            let render_result = build_all_pane_instances(
                rc,
                &pane_infos,
                &self.mux,
                &self.config,
                &scaled_font,
                self.input.cursor_blink_on,
                search_arg,
                active_tid,
            );

            if let Err(crate::renderer::atlas::AtlasError::Full) = render_result {
                // Atlas full — clear everything and retry.
                rc.renderer.atlas.clear(&rc.renderer.device());
                rc.renderer.color_atlas.clear(&rc.renderer.device());
                if let Some(atlas) = rc.renderer.get_lcd_atlas() {
                    atlas.borrow_mut().clear(&rc.renderer.device());
                    // LCD atlas clear invalidates the rasterizer's local cache (TD-MEM-02).
                    rc.shaper.clear_lcd_rasterizer_cache();
                }
                // Bind groups held stale wgpu TextureViews after clear() (TD-MEM-03).
                rc.renderer.rebuild_atlas_bind_groups();
                rc.clear_all_row_caches();
                rc.atlas_generation += 1;
                let _ = build_all_pane_instances(
                    rc,
                    &pane_infos,
                    &self.mux,
                    &self.config,
                    &scaled_font,
                    self.input.cursor_blink_on,
                    search_arg,
                    active_tid,
                );
            }

            // Pane separator lines (hidden when a pane is zoomed).
            let sep_pad_x = self.config.window.padding.left as f32 + sidebar_px_snapshot;
            if self.mux.zoomed_pane.is_none() {
                rc.build_pane_separators(
                    &self.separator_snapshot,
                    sep_pad_x,
                    sb_pad_y,
                    &self.config.colors,
                );
            }
            // Focus border — only when there are multiple panes.
            if pane_infos.len() > 1 {
                if let Some(focused) = pane_infos.iter().find(|p| p.focused) {
                    let tab_accent = self.mux.tabs.active_tab().and_then(|t| t.accent_color);
                    rc.build_focus_border(focused, &self.config.colors, tab_accent);
                }
            }

            // B-3/B-4: OSC 133 command block backgrounds, exit-code text pills.
            rc.build_block_instances(
                &pane_infos,
                &self.mux,
                &self.config.colors,
                self.hover_block,
                sep_pad_x,
                sb_pad_y,
                &scaled_font,
            );

            // H-1: push pre-computed link underline rect.
            if let Some(rect) = link_underline_rect {
                rc.rect_instances.push(rect);
            }

            rc.pane_infos = pane_infos;

            // ── Tab bar / unified titlebar (always shown in Custom mode) ────────
            let renaming = self.ui.is_renaming_tab();
            if tab_bar_vis || renaming {
                let tab_total_cols = total_cols
                    + if self.ui.is_panel_visible() {
                        self.ui.panel().width_cols as usize
                    } else {
                        0
                    };
                let rename_input = self.ui.tab_rename_text();
                let active_idx = self.mux.tabs.active_index();

                // Fast comparison: check copiable inputs before hashing (TD-PERF-16).
                let inputs_match = if let Some((
                    cached_idx,
                    cached_cols,
                    cached_sidebar_visible,
                    cached_panel_visible,
                )) = rc.tab_bar_inputs
                {
                    cached_idx == active_idx
                        && cached_cols == tab_total_cols
                        && cached_sidebar_visible == self.sidebar.visible
                        && cached_panel_visible == self.ui.is_panel_visible()
                        && rc.tab_bar_rename_input.as_deref() == rename_input
                } else {
                    false
                };

                let titles_match = {
                    use std::hash::{Hash, Hasher};
                    let mut h = rustc_hash::FxHasher::default();
                    for tab in self.mux.tabs.tabs() {
                        tab.title.hash(&mut h);
                    }
                    rc.tab_bar_titles_hash == h.finish()
                };

                let tab_key_changed = !inputs_match || !titles_match;

                if !tab_key_changed && !rc.tab_bar_instances_cache.is_empty() {
                    // Tabs unchanged — append cached instances (TD-PERF-09/16).
                    rc.instances.extend_from_slice(&rc.tab_bar_instances_cache);
                    rc.rect_instances.extend_from_slice(&rc.tab_bar_rects_cache);
                } else {
                    let inst_start = rc.instances.len();
                    let rect_start = rc.rect_instances.len();
                    let win_w = rc.renderer.size().0 as f32;
                    let gpu_pad_y = super::TITLEBAR_HEIGHT * rc.scale_factor
                        + self.config.window.padding.top as f32;
                    rc.build_tab_bar_instances(
                        self.mux.tabs.tabs(),
                        active_idx,
                        &scaled_font,
                        tab_total_cols,
                        win_w,
                        self.config.window.padding.left as f32 + sidebar_px_snapshot,
                        gpu_pad_y,
                        self.config.colors.background,
                        self.sidebar.visible,
                        self.ui.is_panel_visible(),
                        rename_input,
                        &self.config.colors,
                    );
                    rc.tab_bar_instances_cache.clear();
                    rc.tab_bar_instances_cache
                        .extend_from_slice(&rc.instances[inst_start..]);
                    rc.tab_bar_rects_cache.clear();
                    rc.tab_bar_rects_cache
                        .extend_from_slice(&rc.rect_instances[rect_start..]);

                    // Cache the inputs for next frame comparison (TD-PERF-16).
                    rc.tab_bar_inputs = Some((
                        active_idx,
                        tab_total_cols,
                        self.sidebar.visible,
                        self.ui.is_panel_visible(),
                    ));
                    {
                        use std::hash::{Hash, Hasher};
                        let mut h = rustc_hash::FxHasher::default();
                        for tab in self.mux.tabs.tabs() {
                            tab.title.hash(&mut h);
                        }
                        rc.tab_bar_titles_hash = h.finish();
                    }
                    rc.tab_bar_rename_input = rename_input.map(|s| s.to_string());
                }
            }

            // ── Scroll bar (overlays right edge of terminal) ─────────────────────
            let focused_pad_right = rc
                .pane_infos
                .iter()
                .find(|p| p.focused)
                .is_some_and(|p| p.pad_right);
            if self.config.enable_scroll_bar {
                if let Some(terminal) = self.mux.active_terminal() {
                    let (disp_off, hist) = terminal.scrollback_info();
                    let sb_state = (disp_off, hist, term_rows, term_cols, focused_pad_right);
                    if rc.scroll_bar_state.as_ref() == Some(&sb_state) {
                        // Geometry unchanged — append cached instances (TD-PERF-08).
                        rc.instances.extend_from_slice(&rc.scroll_bar_cache);
                    } else {
                        let start = rc.instances.len();
                        rc.build_scroll_bar_instances(
                            disp_off,
                            hist,
                            term_rows,
                            term_cols,
                            focused_pad_right,
                            &self.config.colors,
                        );
                        rc.scroll_bar_cache.clear();
                        rc.scroll_bar_cache
                            .extend_from_slice(&rc.instances[start..]);
                        rc.scroll_bar_state = Some(sb_state);
                    }
                }
            }

            // ── Chat panel (side panel) ───────────────────────────────────────────
            let panel_visible = self.ui.is_panel_visible();
            if panel_visible {
                let panel_dirty = self.ui.panel_mut().dirty;
                let panel_focused = self.ui.panel_focused;
                let file_picker_focused = self.ui.file_picker_focused;
                let blink = self.input.cursor_blink_on;
                // If window was resized, term_cols changed — invalidate panel cache.
                let panel_cols_changed = rc.panel_cache_term_cols != total_cols;
                if panel_dirty || panel_cols_changed {
                    let panel_start = rc.instances.len();
                    let rect_start = rc.rect_instances.len();
                    self.ui.panel_mut().dirty = false;
                    // Pre-wrap message lines once per dirty rebuild (TD-PERF-05).
                    let msg_wrap_w = self.ui.panel().width_cols.saturating_sub(8) as usize;
                    self.ui.panel_mut().ensure_wrap_cache(msg_wrap_w);
                    // Pre-build separator cache (TD-PERF-13).
                    let panel_cols = self.ui.panel().width_cols as usize;
                    self.ui.panel_mut().separator(panel_cols);
                    rc.build_chat_panel_instances(
                        self.ui.panel(),
                        self.ui.active_panel_id(),
                        panel_focused,
                        file_picker_focused,
                        &self.config,
                        &scaled_font,
                        total_cols,
                        total_rows,
                        blink,
                        self.config.window.padding.left as f32 + sidebar_px_snapshot,
                        sb_pad_y,
                    );
                    rc.panel_instances_cache.clear();
                    rc.panel_instances_cache
                        .extend_from_slice(&rc.instances[panel_start..]);
                    rc.panel_rect_cache.clear();
                    rc.panel_rect_cache
                        .extend_from_slice(&rc.rect_instances[rect_start..]);
                    rc.panel_cache_term_cols = total_cols;
                } else {
                    rc.instances.extend_from_slice(&rc.panel_instances_cache);
                    rc.rect_instances.extend_from_slice(&rc.panel_rect_cache);
                }
                // Input rows (2 lines + hints) are always rebuilt fresh — cursor
                // blink only touches these 3 rows, not the full message history (TD-PERF-10).
                rc.build_chat_panel_input_rows(
                    self.ui.panel(),
                    panel_focused,
                    file_picker_focused,
                    &self.config,
                    &scaled_font,
                    total_cols,
                    total_rows,
                    blink,
                    self.config.window.padding.left as f32 + sidebar_px_snapshot,
                    sb_pad_y,
                );
                // W-8: resize handle — thin accent line on panel left edge when
                // hovering or dragging. Not cached; redrawn every frame (cheap).
                let show_resize_handle =
                    self.sidebar.panel_resize_hover || self.sidebar.panel_resize_drag;
                let resize_dragging = self.sidebar.panel_resize_drag;
                if show_resize_handle {
                    let mut accent = self.config.colors.ui_accent;
                    accent[3] = if resize_dragging { 1.0 } else { 0.5 };
                    rc.rect_instances
                        .push(crate::renderer::rounded_rect::RoundedRectInstance {
                            rect: [
                                viewport.x + viewport.w,
                                viewport.y,
                                2.0 * rc.scale_factor,
                                viewport.h,
                            ],
                            color: accent,
                            radius: 0.0,
                            border_width: 0.0,
                            _pad: [0.0; 2],
                        });
                }
            }

            // ── Inline AI block (overlays bottom rows) ──────────────────────────
            let block_visible = self.ui.is_block_visible();
            if block_visible && self.ui.ai_block.dirty {
                self.ui.ai_block.dirty = false;
                rc.build_ai_block_instances(
                    &self.ui.ai_block,
                    &scaled_font,
                    total_cols,
                    total_rows,
                    &self.config.colors,
                );
            }

            // ── Status bar ───────────────────────────────────────────────────────
            if self.config.status_bar.enabled {
                // Use cached CWD and exit code (TD-PERF-01, TD-PERF-02).
                // Git branch is polled on an independent timer in about_to_wait (TD-PERF-19).
                let leader_resize_mode = (self.input.leader_active
                    && self.input.modifiers.state().alt_key())
                    || self.input.resize_mode
                    || self.input.dragging_separator.is_some();
                let sb_total_cols = total_cols
                    + if panel_visible {
                        self.ui.panel().width_cols as usize
                    } else {
                        0
                    };
                let sb_win_w = rc.renderer.size().0 as f32;
                // Key: all inputs that affect segment text + layout (TD-PERF-10).
                // Include current minute so the time widget invalidates the cache
                // at each minute boundary (the WaitUntil in about_to_wait wakes us).
                let sb_key = {
                    let flags = [
                        self.input.leader_active as u8,
                        leader_resize_mode as u8,
                        sb_exit_code_raw as u8,
                        self.mux.zoomed_pane.is_some() as u8,
                        self.battery_status
                            .as_ref()
                            .map(|s| s.percent)
                            .unwrap_or(255),
                        self.battery_status
                            .as_ref()
                            .map(|s| s.on_battery as u8)
                            .unwrap_or(0),
                    ];
                    let col_bytes = sb_total_cols.to_le_bytes();
                    let row_bytes = total_rows.to_le_bytes();
                    let win_w_bits = sb_win_w.to_bits().to_le_bytes();
                    let cwd_bytes = self
                        .cached_cwd
                        .as_ref()
                        .and_then(|p| p.to_str())
                        .unwrap_or("")
                        .as_bytes();
                    let branch_bytes = self.ui.git_branch_cache.as_deref().unwrap_or("").as_bytes();
                    let leader_bytes = self.config.leader.key.as_bytes();
                    let mins_now = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| (d.as_secs() / 60) as u32)
                        .unwrap_or(0);
                    let mins_bytes = mins_now.to_le_bytes();
                    static_hash(&[
                        &flags,
                        &col_bytes,
                        &row_bytes,
                        &win_w_bits,
                        cwd_bytes,
                        branch_bytes,
                        leader_bytes,
                        &mins_bytes,
                    ])
                };
                if rc.status_bar_key == sb_key && !rc.status_bar_instances_cache.is_empty() {
                    // Inputs unchanged — append cached instances (TD-PERF-10).
                    rc.instances
                        .extend_from_slice(&rc.status_bar_instances_cache);
                    rc.rect_instances
                        .extend_from_slice(&rc.status_bar_rect_cache);
                } else {
                    let inst_start = rc.instances.len();
                    let rect_start = rc.rect_instances.len();
                    let battery = self
                        .battery_status
                        .as_ref()
                        .map(|s| (s.percent, s.on_battery));
                    let sb_colors = self.config.colors.status_bar_colors();
                    let bar = crate::ui::status_bar::StatusBar::build(
                        self.input.leader_active,
                        leader_resize_mode,
                        &self.config.leader.key,
                        self.cached_cwd.as_deref(),
                        self.ui.git_branch_cache.as_deref(),
                        sb_exit_code,
                        self.mux.zoomed_pane.is_some(),
                        self.config.status_bar.style.clone(),
                        battery,
                        &sb_colors,
                    );
                    rc.build_status_bar_instances(
                        &bar,
                        &scaled_font,
                        sb_total_cols,
                        total_rows,
                        sb_pad_y,
                        sb_win_w,
                        &self.config.colors,
                    );
                    rc.status_bar_instances_cache.clear();
                    rc.status_bar_instances_cache
                        .extend_from_slice(&rc.instances[inst_start..]);
                    rc.status_bar_rect_cache.clear();
                    rc.status_bar_rect_cache
                        .extend_from_slice(&rc.rect_instances[rect_start..]);
                    rc.status_bar_key = sb_key;
                }
            }

            if self.sidebar.visible {
                let counts = self.mux.workspace_tab_pane_counts();
                rc.build_workspace_sidebar_instances(&SidebarDrawParams {
                    workspaces: self.mux.workspaces(),
                    active_workspace_id: self.mux.active_workspace_id,
                    nav_cursor: self.sidebar.nav_cursor,
                    rename_input: self.sidebar.rename_input.as_deref(),
                    sidebar_cols: super::SIDEBAR_COLS,
                    counts: &counts,
                    sidebar_left_px: self.config.window.padding.left as f32,
                    sidebar_top_px: sb_pad_y,
                    sidebar_bottom_pad_px: self.config.window.padding.bottom as f32,
                    font: &scaled_font,
                    colors: &self.config.colors,
                    active_section: self.sidebar.active_section,
                    mcp_servers: &self.mcp_tools_cache,
                    mcp_scroll: self.sidebar.mcp_scroll,
                    skills: self.ui.skill_manager.skills(),
                    skills_scroll: self.sidebar.skills_scroll,
                    steering_files: self.ui.steering_manager.files(),
                    steering_scroll: self.sidebar.steering_scroll,
                });
                let sidebar_sep_x = self.config.window.padding.left as f32
                    + super::SIDEBAR_COLS as f32 * rc.shaper.cell_width;
                let sidebar_sep_y = sb_pad_y;
                let sidebar_sep_h = (rc.renderer.size().1 as f32
                    - sidebar_sep_y
                    - self.config.window.padding.bottom as f32)
                    .max(0.0);
                rc.rect_instances
                    .push(crate::renderer::rounded_rect::RoundedRectInstance {
                        rect: [sidebar_sep_x, sidebar_sep_y, 1.0, sidebar_sep_h],
                        color: self.config.colors.ui_muted,
                        radius: 0.0,
                        border_width: 0.0,
                        _pad: [0.0; 2],
                    });
            }

            // ── Overlays (search bar, palette, context menu) ─────────────────────
            // Record the split point so the GPU renders these in a separate pass.
            let overlay_start = rc.instances.len();
            if self.ui.search_bar.visible {
                rc.build_search_bar_instances(
                    &self.ui.search_bar,
                    &scaled_font,
                    total_cols,
                    total_rows,
                    &self.config.colors,
                );
            }
            if self.ui.palette.visible {
                let palette_cols = total_cols
                    + if panel_visible {
                        self.ui.panel().width_cols as usize
                    } else {
                        0
                    };
                rc.build_palette_instances(
                    &self.ui.palette,
                    &scaled_font,
                    palette_cols,
                    total_rows,
                    self.config.window.padding.left as f32 + sidebar_px_snapshot,
                    sb_pad_y,
                    &self.config.colors,
                );
            }
            if self.ui.context_menu.visible {
                rc.build_context_menu_instances(
                    &self.ui.context_menu,
                    &scaled_font,
                    total_cols,
                    total_rows,
                    &self.config.colors,
                );
            }
            if self.info_overlay.visible {
                rc.build_info_overlay_instances(
                    &self.info_overlay,
                    &scaled_font,
                    total_cols,
                    total_rows,
                    self.config.window.padding.left as f32 + sidebar_px_snapshot,
                    sb_pad_y,
                    &self.config.colors,
                );
            }

            // ── Toast notification ──────────────────────────────────────────────
            let toast_active = self
                .toast
                .as_ref()
                .is_some_and(|(_, deadline)| std::time::Instant::now() < *deadline);
            if toast_active {
                if let Some((msg, _)) = &self.toast {
                    let pad_x = self.config.window.padding.left as f32 + sidebar_px_snapshot;
                    rc.build_toast_instances(
                        msg,
                        &scaled_font,
                        total_cols,
                        pad_x,
                        sb_pad_y,
                        &self.config.colors,
                    );
                }
            } else {
                self.toast = None;
            }

            // ── Debug HUD (F12) — rendered last so it appears above all overlays ─
            if rc.hud_visible {
                rc.build_debug_hud_instances(&scaled_font, &self.config.colors);
            }

            // ── GPU upload ──────────────────────────────────────────────────────
            rc.last_instance_count = rc.instances.len();
            rc.last_overlay_start = overlay_start;
            {
                use crate::renderer::cell::CellVertex;
                use crate::renderer::rounded_rect::RoundedRectInstance;
                let instance_bytes = rc.instances.len() * std::mem::size_of::<CellVertex>();
                let lcd_bytes = rc.lcd_instances.len() * std::mem::size_of::<CellVertex>();
                let rect_bytes =
                    rc.rect_instances.len() * std::mem::size_of::<RoundedRectInstance>();
                rc.last_gpu_upload_bytes = instance_bytes + lcd_bytes + rect_bytes;
            }
            rc.renderer.upload_rect_instances(&rc.rect_instances);
            rc.renderer.set_overlay_start(overlay_start);
            rc.renderer.upload_instances(&rc.instances, 0);
            rc.renderer.set_cell_count(rc.instances.len());
            rc.renderer.upload_lcd_instances(&rc.lcd_instances);
            let _ = rc.renderer.render();

            // ── Input-to-pixel latency probe (RUST_LOG=petruterm=debug) ─────────
            // Only logged when PTY data arrived this frame (echo of a keypress).
            if !data_ids.is_empty() {
                if let Some(t) = self.input.last_key_instant.take() {
                    let latency_ms = t.elapsed().as_secs_f32() * 1000.0;
                    log::debug!("input-to-pixel: {:.1}ms", latency_ms);
                    rc.latency_samples.push_back(latency_ms);
                    if rc.latency_samples.len() > 120 {
                        rc.latency_samples.pop_front();
                    }
                }
            }

            // ── Frame time tracking for HUD ─────────────────────────────────────
            let frame_ms = frame_start.elapsed().as_secs_f32() * 1000.0;
            rc.frame_times.push_back(frame_ms);
            if rc.frame_times.len() > 120 {
                rc.frame_times.pop_front();
            }

            // Suppress unused warning for scroll-bar focused dimensions.
            let _ = (term_cols, term_rows);
        }
    }
}

/// Build and upload cell instances for every pane in the active tab.
/// Calls `rc.begin_frame()` first to clear previous frame's instances.
#[allow(clippy::too_many_arguments)]
fn build_all_pane_instances(
    rc: &mut RenderContext,
    pane_infos: &[PaneInfo],
    mux: &Mux,
    config: &crate::config::Config,
    font: &crate::config::schema::FontConfig,
    cursor_blink_on: bool,
    search_bar: Option<&crate::ui::SearchBar>,
    active_terminal_id: usize,
) -> Result<(), crate::renderer::atlas::AtlasError> {
    rc.begin_frame();
    // Take the scratch buffer out of rc so we can mutably borrow rc (build_instances)
    // while the buffer is filled by mux. Returned at the end of the function.
    let mut cell_data_scratch = std::mem::take(&mut rc.cell_data_scratch);
    let mut last_scratch_tid = rc.scratch_terminal_id.take();
    for info in pane_infos {
        // If the terminal changed (tab switch or split panes with different terminals),
        // clear the scratch buffer so collect_grid_cells_for treats all rows as damaged.
        // Without this, undamaged-row skipping retains stale data from the previous
        // terminal, causing TUI app content to bleed into unrelated tabs.
        let terminal_changed = last_scratch_tid != Some(info.terminal_id);
        if terminal_changed {
            cell_data_scratch.clear();
        }
        // Pass search highlight info only for the active pane with a non-empty query.
        let search_arg = search_bar.and_then(|sb| {
            if !sb.query.is_empty()
                && !sb.matches.is_empty()
                && info.terminal_id == active_terminal_id
            {
                Some((sb.matches.as_slice(), sb.current))
            } else {
                None
            }
        });
        // Compute syntax overlay for the active input line (I-2).
        let syntax_overlay = if !config.input_syntax_highlight {
            None
        } else {
            mux.terminals
                .get(info.terminal_id)
                .and_then(|s| s.as_ref())
                .and_then(|t| {
                    if !t.input_shadow.active {
                        return None;
                    }
                    let cursor = t.cursor_info();
                    if !cursor.visible {
                        return None;
                    }
                    let shadow = &t.input_shadow;
                    let cursor_as_col = shadow.buf[..shadow.cursor].chars().count();
                    let cmd_start_col = cursor.col.saturating_sub(cursor_as_col);
                    let cmd_valid = {
                        use crate::term::tokenizer::tokenize_command;
                        use crate::term::tokenizer::TokenKind;
                        tokenize_command(&shadow.buf)
                            .into_iter()
                            .find(|tok| tok.kind == TokenKind::Command)
                            .and_then(|tok| shadow.cmd_resolver.resolve(&shadow.buf[tok.range]))
                    };
                    let fg = crate::term::tokenizer::build_syntax_fg(
                        &shadow.buf,
                        cmd_valid,
                        &config.colors,
                    );
                    Some(SyntaxOverlay {
                        viewport_row: cursor.row,
                        cmd_start_col,
                        fg,
                    })
                })
        };
        // Compute ghost text overlay (I-3): history completion suffix after cursor.
        // Skipped when config.input_ghost_text = false (e.g. user has zsh-autosuggestions).
        let ghost_overlay = if !config.input_ghost_text {
            None
        } else {
            mux.terminals
                .get(info.terminal_id)
                .and_then(|s| s.as_ref())
                .and_then(|t| {
                    let shadow = &t.input_shadow;
                    let ghost_text = shadow.ghost.as_ref()?;
                    if !shadow.active || shadow.cursor != shadow.buf.len() {
                        return None;
                    }
                    let cursor = t.cursor_info();
                    if !cursor.visible {
                        return None;
                    }
                    let muted = config.colors.ui_muted;
                    let r = (muted[0] * 255.0).round() as u8;
                    let g = (muted[1] * 255.0).round() as u8;
                    let b = (muted[2] * 255.0).round() as u8;
                    Some(GhostOverlay {
                        viewport_row: cursor.row,
                        start_col: cursor.col,
                        chars: ghost_text.chars().collect(),
                        fg: alacritty_terminal::vte::ansi::Color::Spec(
                            alacritty_terminal::vte::ansi::Rgb { r, g, b },
                        ),
                    })
                })
        };
        // Compute flag hint overlay (I-4): description shown below cursor when last token is a flag.
        let flag_hint_overlay = mux
            .terminals
            .get(info.terminal_id)
            .and_then(|s| s.as_ref())
            .and_then(|t| {
                let shadow = &t.input_shadow;
                if !shadow.active || shadow.buf.is_empty() {
                    return None;
                }
                let cursor = t.cursor_info();
                if !cursor.visible || cursor.row + 1 >= t.rows as usize {
                    return None;
                }
                use crate::term::tokenizer::{tokenize_command, TokenKind};
                let tokens = tokenize_command(&shadow.buf);
                let last = tokens.last()?;
                if last.kind != TokenKind::Flag {
                    return None;
                }
                let flag_text = &shadow.buf[last.range.clone()];
                let cmd_text = tokens
                    .iter()
                    .find(|t| t.kind == TokenKind::Command)
                    .map(|t| &shadow.buf[t.range.clone()])
                    .unwrap_or("");
                let desc = crate::term::flag_db::lookup_flag(cmd_text, flag_text)?;
                // Align hint with flag start column in the terminal grid.
                let cursor_as_col = shadow.buf[..shadow.cursor].chars().count();
                let cmd_start_col = cursor.col.saturating_sub(cursor_as_col);
                let flag_col_in_buf = shadow.buf[..last.range.start].chars().count();
                let flag_start_col = cmd_start_col + flag_col_in_buf;
                let hint: String = format!("{}  {}", flag_text, desc);
                let muted = config.colors.ui_muted;
                let r = (muted[0] * 255.0).round() as u8;
                let g = (muted[1] * 255.0).round() as u8;
                let b = (muted[2] * 255.0).round() as u8;
                Some(FlagHintOverlay {
                    viewport_row: cursor.row + 1,
                    start_col: flag_start_col,
                    chars: hint.chars().collect(),
                    fg: alacritty_terminal::vte::ansi::Color::Spec(
                        alacritty_terminal::vte::ansi::Rgb { r, g, b },
                    ),
                })
            });
        mux.collect_grid_cells_for(
            info.terminal_id,
            &mut cell_data_scratch,
            search_arg,
            terminal_changed,
            syntax_overlay.as_ref(),
            ghost_overlay.as_ref(),
            flag_hint_overlay.as_ref(),
        );
        let cell_data = &cell_data_scratch[..];
        rc.build_instances(
            cell_data,
            config,
            font,
            info.terminal_id,
            info.col_offset,
            info.row_offset,
        )?;
        last_scratch_tid = Some(info.terminal_id);
    }
    rc.cell_data_scratch = cell_data_scratch;
    rc.scratch_terminal_id = last_scratch_tid;

    // Record content boundary before cursor — used by the fast blink path.
    rc.content_end = rc.instances.len();

    // Emit cursor for the focused pane (always after content_end).
    if let Some(info) = pane_infos.iter().find(|i| i.focused) {
        if let Some(cursor) = mux
            .terminals
            .get(info.terminal_id)
            .and_then(|s| s.as_ref())
            .map(|t| t.cursor_info())
        {
            rc.build_cursor_instance(
                &cursor,
                cursor_blink_on,
                info.col_offset,
                info.row_offset,
                config,
            );
        } else {
            rc.cursor_vertex_template = None;
        }
    } else {
        rc.cursor_vertex_template = None;
    }

    Ok(())
}

/// Hash a sequence of byte slices into a single u64.
/// Used to detect whether static-geometry inputs (tab bar, status bar, scroll bar)
/// changed since the last frame, so we can skip rebuilding their GPU instances.
fn static_hash(parts: &[&[u8]]) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = rustc_hash::FxHasher::default();
    for p in parts {
        p.hash(&mut h);
    }
    h.finish()
}
