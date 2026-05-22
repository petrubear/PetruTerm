use super::*;
use rust_i18n::t;

// Bundles the geometry + color parameters shared by build_panel_messages and its helpers.
struct PanelMsgParams<'a> {
    panel_id: usize,
    history_start_row: usize,
    sep_row: usize,
    config: &'a Config,
    font: &'a crate::config::schema::FontConfig,
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
}

impl RenderContext {
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
        use crate::llm::chat_panel::MAX_FILE_ROWS;
        use std::fmt::Write as _;

        let panel_cols = panel.width_cols as usize;
        if panel_cols == 0 || screen_rows < 8 {
            return;
        }

        // ── Colors (from active theme) ────────────────────────────────────────
        let actual_panel_bg = config.colors.background;
        let panel_bg = [0.0; 4]; // transparent

        let user_fg = config.colors.ansi[6];
        let asst_fg = config.colors.foreground;
        let input_fg = config.colors.foreground;

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
        } else if matches!(
            panel.state,
            crate::llm::chat_panel::PanelState::AwaitingConfirm
        ) {
            // ── Confirmation view: diff preview + [y]/[n] ────────────────────
            use crate::llm::chat_panel::ConfirmDisplay;
            use crate::llm::diff::DiffKind;

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
        } else if let crate::llm::chat_panel::PanelState::ConfirmAction(action) = &panel.state {
            // ── Inline action confirm card ────────────────────────────────────
            use crate::llm::agent_action::AgentAction;

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
            let msg_params = PanelMsgParams {
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
            };
            self.build_panel_messages(panel, &msg_params, &mut fmt_buf);
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

    fn build_panel_messages(
        &mut self,
        panel: &ChatPanel,
        p: &PanelMsgParams,
        fmt_buf: &mut String,
    ) {
        use crate::llm::chat_panel::{word_wrap, PanelState};
        use std::fmt::Write as _;

        let PanelMsgParams {
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
        } = *p;

        let history_rows = sep_row.saturating_sub(history_start_row);

        // W-5: Zero state — empty panel, idle
        if panel.messages.is_empty() && matches!(panel.state, PanelState::Idle) {
            self.draw_panel_zero_state(panel, p, fmt_buf);
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
                crate::llm::ChatRole::User => (user_fg, Some(user_accent), user_bg),
                crate::llm::ChatRole::Assistant => (asst_fg, Some(asst_accent), asst_bg),
                crate::llm::ChatRole::System => continue,
                crate::llm::ChatRole::Tool(_) => continue,
            };
            let prefix = "        "; // 8 spaces — keeps msg_inner_w (sub 8) correct
            let prefix_len = 8usize;
            for ann in panel.wrapped_message(msg_idx).iter() {
                let is_code = matches!(ann.kind, BlockKind::CodeBlock);
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
                let is_code = matches!(ann.kind, BlockKind::CodeBlock);
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
            self.draw_suggestion_pills(panel, suggestion_rows, p, fmt_buf);
        }

        self.scratch_lines = all_lines;
    }

    // W-5: Zero state rendering — empty panel, idle.
    fn draw_panel_zero_state(
        &mut self,
        panel: &ChatPanel,
        p: &PanelMsgParams,
        fmt_buf: &mut String,
    ) {
        let PanelMsgParams {
            history_start_row,
            sep_row,
            config,
            font,
            co,
            panel_cols,
            panel_bg,
            sep_fg,
            pad_y,
            cw,
            ch,
            px,
            pw,
            ..
        } = *p;

        let center = (history_start_row + sep_row) / 2;
        let icon_row = center.saturating_sub(3);
        let text_row = center.saturating_sub(1);
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
                self.push_shaped_row(&row_text, config.colors.ui_accent, panel_bg, r, co, panel_cols, font);
            } else if r == text_row {
                let msg = "Ask a question below";
                let msg_w = msg.chars().count();
                let pad = panel_cols.saturating_sub(msg_w) / 2;
                fmt_buf.clear();
                fmt_buf.extend(std::iter::repeat_n(' ', pad));
                fmt_buf.push_str(msg);
                self.push_shaped_row(fmt_buf, config.colors.ui_muted, panel_bg, r, co, panel_cols, font);
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
                let (border_color, fill_color, text_fg) = if is_hovered {
                    (config.colors.ui_accent, config.colors.ui_surface_active, config.colors.foreground)
                } else {
                    (config.colors.ui_muted, config.colors.ui_surface, dim(config.colors.foreground, 0.15))
                };
                let pill_x = px + pill_margin;
                let pill_y = pad_y + r as f32 * ch;
                let pill_w = pw - 2.0 * pill_margin;
                self.rect_instances.push(RoundedRectInstance {
                    rect: [pill_x - pill_border, pill_y - pill_border, pill_w + 2.0 * pill_border, ch + 2.0 * pill_border],
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
                self.push_shaped_row(fmt_buf, text_fg, panel_bg, r, co, panel_cols, font);
            } else {
                self.push_shaped_row("", sep_fg, panel_bg, r, co, panel_cols, font);
            }
        }
    }

    // W-7: Suggestion pill rows just above sep_row.
    fn draw_suggestion_pills(
        &mut self,
        panel: &ChatPanel,
        suggestion_rows: usize,
        p: &PanelMsgParams,
        fmt_buf: &mut String,
    ) {
        let PanelMsgParams {
            sep_row,
            config,
            font,
            co,
            panel_cols,
            pad_y,
            cw,
            ch,
            px,
            pw,
            ..
        } = *p;
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
                (config.colors.ui_accent, config.colors.ui_surface_active, config.colors.foreground)
            } else {
                (config.colors.ui_muted, config.colors.ui_surface, dim(config.colors.foreground, 0.15))
            };
            let pill_x = px + pill_margin;
            let pill_y = pad_y + r as f32 * ch;
            let pill_w = pw - 2.0 * pill_margin;
            self.rect_instances.push(RoundedRectInstance {
                rect: [pill_x - pill_border, pill_y - pill_border, pill_w + 2.0 * pill_border, ch + 2.0 * pill_border],
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
        let input_fg = config.colors.foreground;

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
}
