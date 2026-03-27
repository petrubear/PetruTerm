# Session State

**Last Updated:** 2026-03-27
**Session Focus:** Phase 2 — AI Layer (in progress)

## Phase 1 Status: COMPLETE ✓

All acceptance criteria verified on M4 Max, 2026-03-27.
Build: 0 errors, ~24 warnings (dead code stubs only).
Bundle: dist/PetruTerm.app, 18 MB, ad-hoc signed, icon embedded.

### Phase 1 Final Commits
- `d70c00d` fix: Ctrl key encoding + reverse-video (TD-016, TD-017)
- `4883895` fix: scroll rendering ignores display_offset + padding top 44→60
- `7bee09b` fix: ligature rendering — allow negative glyph bearing_x

### Phase 1 Root Cause Notes (preserved for reference)
- **Ligatures:** JetBrains Mono uses calt, not true ligatures. Second glyph in a
  sequence has negative bearing_x (e.g. bx=-16, bm=32px for `==`). `clamp_glyph_to_cell`
  was doing `x0 = ox.max(0.0)` which stripped the left extension and shifted UV.
  Fix: remove X-axis clamping entirely — only Y clamped for row-bleed (TD-012).
- **Scroll:** `grid()[Line(row)]` ignores display_offset. Fix: `Line(row - display_offset)`.
- **Trackpad:** PixelDelta.y is in logical points. Fix: divide by `cell_h / scale_factor`.

## Build Status
- **cargo build:** PASS — 0 errors
- **bundle:** PASS — dist/PetruTerm.app

## Phase 2 Progress

### Infrastructure (complete)
- [x] `mod llm` wired in `main.rs`; `async-trait`, `futures-util` added to Cargo.toml
- [x] `LlmProvider` trait (`src/llm/mod.rs`) — `complete()` + `stream()` async, Arc-based
- [x] `ChatMessage` / `ChatRole` types in `src/llm/mod.rs`
- [x] OpenRouter provider (`src/llm/openrouter.rs`) — SSE streaming, OPENROUTER_API_KEY env
- [x] Default model: `meta-llama/llama-3.1-8b-instruct:free` (free tier for testing)
- [x] tokio Runtime + crossbeam channel for async streaming (`App` struct)

### Chat panel UI (complete)
- [x] `ChatPanel` state machine (`src/llm/chat_panel.rs`) — multi-turn `Vec<ChatMessage>`
  history, `streaming_buf`, `scroll_offset`, `word_wrap`, `wrap_input`, `titled_separator`
- [x] `panel_focused: bool` in `App` — independent focus for terminal and panel
- [x] Right-side layout split — `default_grid_size()` and `viewport_rect()` subtract
  `panel_cols * cell_w` when panel is open; `resize_terminals_for_panel()` on toggle
- [x] `push_shaped_row` helper + `build_chat_panel_instances` — panel rendered at
  `col_offset = term_cols`, same `CellVertex` pipeline, zero shader changes
- [x] Header: `"⚡ Petrubot"` — dims when terminal has focus, bright when panel focused
- [x] History area: scrollable `Vec<ChatMessage>`, word-wrapped, User/AI color-coded
- [x] Streaming in-progress tokens shown in history area in amber
- [x] Input area: 2 rows, character-based wrap (`wrap_input`), cursor `▋` at end
- [x] Hints row: context-sensitive keybind hints

### Keybinds (current)
- [x] `Ctrl+C` — open panel (panel closed) / close panel (panel focused) /
  SIGINT to PTY (panel open, terminal focused — no conflict)
- [x] `Ctrl+V` — switch focus terminal ↔ panel (when panel open); falls through to PTY otherwise
- [x] `Esc` — dismiss error state in panel
- [x] `Enter` in panel — submit query (input non-empty) / run last AI command (input empty)
- [x] Mouse wheel over panel — scrolls chat history; over terminal — scrolls scrollback

### Bug fixes (this session)
- [x] TD-019: Space key in panel input — explicit `NamedKey::Space` match
- [x] TD-020: Render rewritten from scratch — panel at right of terminal, not overlay
- [x] TD-021: `WindowEvent::DroppedFile` — panel focused → append path to input;
  terminal focused → write path bytes to PTY

### Remaining Phase 2
- [ ] Shell integration (`shell-integration.zsh`) — CWD, exit codes, command boundaries
- [ ] Ctrl+Shift+E / Ctrl+Shift+F (explain last output / fix last error)
- [ ] Ollama + LMStudio providers
- [ ] TD-022: Agent mode (chat with CWD/file context) — P3, needs design doc first
- [ ] TD-023: Leader key for panel actions — P3, replaces Ctrl+C/V with prefix system

## Key Technical Decisions (stable)

### Phase 1
- Surface: non-sRGB `Bgra8Unorm` on Metal
- Atlas: `Rgba8Unorm`, mask as `[a,a,a,255]`, shader samples `.r`
- Scale: `window.scale_factor()` = 2.0 on M4 Max; 15pt → 30pt → 18×36px cell
- Cursor: `FLAG_CURSOR = 0x08`; `vs_bg` partial rect for underline/beam
- Blink: 530ms toggle in `about_to_wait`, reset on keypress
- Shell exit: `Event::ChildExit(i32)` — alacritty_terminal 0.25.1
- EventLoopProxy: `wakeup.send_event(())` wakes NSApp immediately
- Custom title bar: `HasWindowHandle → ns_view → [view window]` + FullSizeContentView
- Working dir: `dirs::home_dir()` → PtyOptions
- Ligature rendering: bearing_x passed raw (no X clamp); Y clamped to cell_height only

### Phase 2
- Chat panel width: `PANEL_COLS = 55` terminal cell columns (configurable via `ChatPanel.width_cols`)
- Panel rendering: `col_offset = term_cols` — panel cells placed in grid column space beyond
  the terminal; same shader, same `CellVertex` pipeline, no scissor rects needed
- Multi-turn context: full `Vec<ChatMessage>` history sent to LLM on every submit
- Async streaming: tokio task → crossbeam channel → `poll_ai_events()` in `about_to_wait`
- Focus model: `panel_focused: bool` on `App`; Ctrl+C/V cycle focus without closing panel
