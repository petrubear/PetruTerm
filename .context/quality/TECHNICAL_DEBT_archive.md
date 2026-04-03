# Technical Debt Archive

All resolved items from [TECHNICAL_DEBT.md](./TECHNICAL_DEBT.md).
Ordered newest-first within each date group.

---

## Resolved 2026-04-03

### TD-042: Mouse Selection, Typing Delay, Font Memory
- **Files:** `src/term/mod.rs`, `src/app/mod.rs`, `src/app/renderer.rs`, `src/font/locator.rs`
- **Mouse selection:** `start_selection`/`update_selection` now lock the term once and subtract `display_offset` from the viewport row to anchor selections in buffer space. `MouseWheel` calls `update_selection` when button is held so dragging into scrollback works.
- **Typing delay:** `user_event` now checks `has_data` from `poll_pty_events()` and calls `request_redraw()` immediately when PTY output arrives instead of waiting for the next blink tick or mouse event.
- **Font memory:** Removed `locate_font_for_lcd` from per-frame `scaled_font_config()` (was cloning ~200 KB `JBM_REGULAR` bytes every frame). `locate_via_font_kit` now uses `select_best_match` instead of loading every font variant to find Regular weight.

### TD-040: Leader Key Action Dispatch System
- **Files:** `src/app/input/mod.rs`, `src/config/schema.rs`, `src/config/lua.rs`, `src/ui/palette/actions.rs`, `config/default/keybinds.lua`
- **Resolution:** `InputHandler::new(&Config)` builds `leader_map: HashMap<String, Action>` from `config.keys` filtered to `mods == "LEADER"`. `Action` gained `FromStr` and two new variants (`CommandPalette`, `ToggleAiPanel`). Lua parser now reads `config.leader` and `config.keys`. All custom keybinds moved to `keybinds.lua` as `LEADER` entries; hardcoded `Cmd+Shift+P/A`, `Ctrl+Shift+E/F`, `Cmd+T/W` removed. Adding a binding now requires only a Lua edit.
- **Default binds (Ctrl+B then…):** `p` palette · `a` AI panel · `e` explain · `f` fix · `t` new tab · `w` close tab · `%` split-H · `"` split-V · `x` close pane

---

## Resolved 2026-03-31

### TD-041: AI Panel Off-Screen + Broken GPU Upload
- **Files:** `src/app/mod.rs`, `src/app/renderer.rs`
- **Root cause 1:** `resize_terminals_for_panel()` was never called on panel open/close — panel rendered past the terminal right edge.
- **Fix:** Detect `is_panel_visible() != panel_was_visible` in `KeyboardInput` handler → call resize.
- **Root cause 2:** Dirty-row upload (`start = row_idx * cols`) broke when panel instances were appended after terminal rows — offsets didn't map.
- **Fix:** Full `upload_instances` when `is_panel_visible()` (same as palette).

---

## Resolved 2026-03-30

### TD-039: Manual ANSI Key Encoding
- **File:** `src/app/input/key_map.rs`
- **Implementation:** Created `translate_key` with xterm-compatible modifier encoding (Shift, Ctrl, Alt) for arrows, F-keys, and navigation keys.
- **Result:** Robust, extensible input handling following industry standards.

### TD-038: Hardcoded UI Constants
- **File:** `src/config/schema.rs`, `src/app/renderer.rs`, `config/default/llm.lua`
- **Implementation:** Introduced `ChatUiConfig` in the schema. Moved hardcoded colors and panel width to Lua (`llm.ui`). Added `parse_hex_linear` helper.
- **Result:** AI panel appearance fully customizable via Lua.

### TD-037: Incomplete Palette Actions
- **File:** `src/app/ui.rs`
- **Implementation:** Connected `Action::ExplainLastOutput` and `Action::FixLastError` in `handle_palette_action`.
- **Result:** Command palette correctly triggers AI context analysis.

### TD-036: Suboptimal Render Pass Architecture
- **File:** `src/renderer/pipeline.rs`, `src/renderer/gpu.rs`
- **Implementation:** Consolidated BG pass and Glyph pass into a single render pass using premultiplied alpha.
- **Result:** Improved GPU efficiency and reduced power consumption on Apple Silicon.

### TD-034: God Object Pattern in `App`
- **File:** `src/app/mod.rs` → split into `renderer.rs`, `mux.rs`, `ui.rs`, `input/mod.rs`
- **Implementation:** Decomposed 2000-line `App` into `RenderContext` (GPU), `Mux` (PTY/Tabs/Panes), `UiManager` (AI/Overlays), `InputHandler` (Keyboard/Mouse).
- **Result:** `App` is now a thin event coordinator. Drastically improved maintainability.

### TD-033: Atlas Stability & Eviction
- **File:** `src/renderer/atlas.rs`, `src/app/mod.rs`
- **Implementation:** `GlyphAtlas::upload` returns `AtlasError::Full`. Render catches this, clears both atlases and `RowCache`, and re-renders.
- **Result:** Terminal no longer crashes when atlas fills up.

### TD-032: High-Bandwidth GPU Instance Uploads
- **File:** `src/app/renderer.rs`, `src/renderer/gpu.rs`
- **Implementation:** Dirty-row tracking in `RowCache`. Partial buffer updates via offset — only changed rows uploaded.
- **Result:** ~95% reduction in GPU memory bandwidth per frame.

### TD-029: O(N²) Column Calculation during Shaping
- **File:** `src/font/shaper.rs`
- **Implementation:** `TextShaper::shape_line` uses incremental character counts for O(N) column derivation.
- **Result:** Faster shaping for long lines.

### TD-028: Redundant Text Shaping
- **File:** `src/app/renderer.rs`
- **Implementation:** Row-level `RowCache` with hash (text + colors). Cache hit skips re-shaping and re-rasterizing.
- **Result:** ~80% CPU reduction when terminal content is static.

### TD-018: Powerline Separator Fringing
- **File:** `src/renderer/pipeline.rs` (WGSL shaders)
- **Implementation:** Pixel snapping (`floor`) in vertex shader + manual blending for separator glyphs.
- **Result:** Clean Powerline/catppuccin-tmux separator rendering with no fringing.

### TD-005: PTY Thread JoinHandle Type-Erased
- **File:** `src/term/pty.rs`, `src/app/mod.rs`
- **Implementation:** `std::thread::JoinHandle<()>` + `Pty::shutdown()` sends `Msg::Shutdown` and joins. `App::Drop` calls `mux.shutdown()`.
- **Result:** No orphaned PTY threads on exit.

---

## Resolved 2026-03-27

### TD-025: Vertical Spacing Too Tight
- **File:** `src/config/schema.rs`, `src/font/shaper.rs`
- **Implementation:** `font.line_height` multiplier (default 1.2) applied in `TextShaper`.
- **Result:** Readable line spacing configurable via Lua.

### TD-012: Nerd Font Icons Overflow Cell
- **File:** `src/app/renderer.rs`
- **Implementation:** `clamp_glyph_to_cell()` crops `glyph_size` to cell bounds; Y-only clamping preserves JetBrains Mono ligature negative `bearing_x`.
- **Result:** Nerd Font row bleeding eliminated.

---

## Resolved 2026-03-24 and Earlier

### TD-021: Drag-and-Drop File Path Not Inserted
- `WindowEvent::DroppedFile`: panel focused → append to chat input; terminal focused → write path to PTY.

### TD-020: AI Block Response Not Rendered
- `build_chat_panel_instances` rewritten from scratch with `push_shaped_row` helper; panel rendered at `col_offset = term_cols`.

### TD-019: Space Key Not Forwarded in AI Input
- Explicit `Key::Named(NamedKey::Space)` handler in panel input routing.

### TD-017: Reverse-Video (SGR 7 / Flags::INVERSE) Not Applied
- Commit d70c00d: `cell.flags.contains(Flags::INVERSE)` swaps fg/bg in `collect_grid_cells`.

### TD-016: Ctrl Key Modifier Not Forwarded to PTY
- Commit d70c00d: Ctrl+key → `byte - b'a' + 1` mapping in `key_map.rs`.

### TD-013: Arrow Keys Ignore APP_CURSOR Mode (DECCKM)
- `APP_CURSOR` check in `translate_key`: normal → `\x1b[A`, app → `\x1bOA`.

### TD-011: Shell `exit` Does Not Close Terminal Window
- `PtyEvent::Exit` (mapped from `Event::ChildExit`) sets `shell_exited = true` → `event_loop.exit()`.

### TD-010: Nerd Font Icons Render as CJK Fallback Glyphs
- Bundled JetBrains Mono Nerd Font Mono (v3.3.0) as fallback; atlas packing preserves icon codepoints.

### TD-007: No Clipboard Integration
- `Cmd+C`: `terminal.selection_to_string()` → arboard. `Cmd+V`: arboard → PTY (bracketed paste aware).

### TD-006: No Mouse Event Handling
- SGR and X10 mouse report encoding; drag selection; scroll delta forwarding; `MOUSE_REPORT_CLICK/DRAG/MOTION` mode detection.

### TD-003: PTY cell_width/cell_height Hardcoded at 8×16
- Cell dimensions measured from shaped "M" glyph in `TextShaper::measure_cell`; passed to `TIOCSWINSZ`.

### TD-002: PTY Placeholder Event Proxy on Term Construction
- `Arc<OnceLock<Notifier>>` shared between `PtyEventProxy` and `Pty::spawn`. `direct_notifier` set once PTY loop is ready; `PtyWrite` forwarded immediately on background thread.

### TD-031: Insecure API Key Storage
- `LlmConfig::api_key` uses `secrecy::SecretString`; `#[serde(skip_serializing)]` prevents disk/log leakage; `expose_secret()` only at HTTP boundary.

### TD-030: Secret Leakage to LLM Provider
- `sanitize_command` in `ShellContext` redacts `export VAR=secret` and `Authorization:` headers via regex before injecting into system prompt.
