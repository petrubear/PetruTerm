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
- [x] Shell integration (`scripts/shell-integration.zsh`) — CWD, exit codes written to
  `~/.cache/petruterm/shell-context.json` via preexec/precmd hooks
- [x] `src/llm/shell_context.rs` — `ShellContext::load()` reads JSON; injected into system
  message on every query; `explain_last_output()` + `fix_last_error()` helpers
- [x] Ctrl+Shift+E / Ctrl+Shift+F — wired in `handle_key_input`; auto-opens panel + submits
- [x] Ollama + LMStudio providers — `src/llm/openai_compat.rs` (`OpenAICompatProvider`);
  `ollama` defaults to `http://localhost:11434/v1`, `lmstudio` to `http://localhost:1234/v1`
- [x] TD-024: Mouse text selection — `cell_in_selection()` helper; `collect_grid_cells` inverts
  fg/bg for selected cells; `start_selection` guarded behind `!any_mouse`; window drag moved
  to `window.drag_window()` on clicks in pad_top zone; `setMovableByWindowBackground: NO`
- [ ] TD-022: Agent mode (chat with CWD/file context) — P3, needs design doc first
- [ ] TD-023: Leader key for panel actions — P3, replaces Ctrl+C/V with prefix system

## Phase 2 Status: COMPLETE ✓ (2026-03-27)
All deliverables implemented. Verified: OpenRouter, LMStudio streaming; shell integration;
Ctrl+Shift+E/F; mouse text selection with visual highlight.

### Post-Phase-2 Bug Fixes (2026-03-27)
- [x] TD-018: Powerline separator colour fringing — premultiplied alpha in fragment shader
  (returns `vec4(fg*alpha, alpha)`); blend state changed to `One/OneMinusSrcAlpha`.
  Alpha-0 edge pixels are fully transparent, bg pass shows through — no fringing.
  Right-edge X clamp intentionally NOT applied: double-wide Nerd Font icons (MonoLisa NF
  non-Mono) need to overflow their cell; premultiplied alpha makes the overflow invisible.

## Session Close Notes (2026-03-27)
Phase 2 complete. No new code this close-out session — only debt registry updates.

### Debt Added (next session priorities)
- **TD-025:** Line spacing too tight — investigate WezTerm `line_height` multiplier;
  add `font.line_height: f32` to `FontConfig`; propagate to cell_height in TextShaper.
- **TD-026:** Glyph antialiasing quality — compare WezTerm LCD subpixel pipeline
  (`wezterm-font/src/rasterizer/`); three-tier fix: gamma → bg-aware blend → full LCD AA.
  Note: bg-aware blend was started and reverted (commit `2d2b7da`) due to premul conflict;
  re-evaluate once premul pipeline is stable.

### Post-Phase-2 Quality Fixes (2026-03-27)
- [x] TD-025: Line spacing — `font.line_height: f32` (default 1.2) added to `FontConfig`;
  `TextShaper::new` uses `size * line_height` for `Metrics`; `cell_height` propagates to PTY.
  Configurable from Lua: `font = { line_height = 1.4 }`.

### Next Session Start
- Tackle TD-026 (antialiasing) — research WezTerm sources first.

## Session Notes (2026-03-30)

### TD-026 — Antialiasing Quality (ALL COMPLETE)
All three levels implemented:
- **TD-026a** (2026-03-30): Greyscale gamma correction.
- **TD-026b** (2026-03-30): Background-aware blending (`fs_bg_aware` + `CellPipelineBgAware`).
- **TD-026c** (2026-03-30): LCD subpixel AA — FreeType `FT_RENDER_MODE_LCD`, `LcdGlyphAtlas`, `fs_lcd`.

### Powerline Rendering Regression (2026-03-30)
Minimax delegated TD-026 introduced `fs_bg_aware` as the PRIMARY glyph pass (REPLACE blend),
which re-introduced TD-018 fringing and broke powerline overflow. Fix applied:

**Root causes found and fixed:**
1. Glyph pass was using `bg_aware_pipeline` (REPLACE blend) → fringing on overflow glyphs.
   Fixed: reverted to `pipeline.cell_pipeline` (One/OneMinusSrcAlpha premul).
2. `fs_main` was converting fg to linear before premultiplying — linear values written to
   sRGB framebuffer, blend mixed color spaces → vivid wrong colors.
   Fixed: remove sRGB→linear from `fs_main`, keep corrected_alpha only.
3. With correct premul, powerline arrows appeared darker than WezTerm (swash alpha < CoreText).
   Fix: hybrid bg-aware premul in `fs_main`:
   `mix(bg_lin, fg_lin, ca)` in linear space, output `(rgb*ca, ca)`.
   Solid pixels → ca=1 → pure fg (vivid). Transparent pixels → vec4(0) → pass-through (no fringing).
   PARTIAL FIX — still slightly less vivid than WezTerm. Tracked as TD-027.

### TD-027 — Powerline Vivid Rendering (OPEN, P3)
Best hypothesis: swash greyscale coverage < 1.0 for solid Nerd Font fill glyphs; CoreText reaches
1.0. Next steps: (1) log max-alpha per glyph to confirm; (2) try gamma 1/2.2 instead of 1/1.4;
(3) render bg to separate texture, bind in glyph pass for true framebuffer-read bg-aware blend.
See TECHNICAL_DEBT.md TD-027 for full investigation notes.

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
- Shell context: `ShellContext::load()` reads `~/.cache/petruterm/shell-context.json`; injected
  into system message on every query; script auto-installed to `~/.config/petruterm/` on launch
- Mouse selection: `SelectionRange::to_range(term)` per frame; fg/bg inverted for selected cells;
  title bar drag via `window.drag_window()` when click y < pad_top; content drag = selection
- Providers: OpenRouter (SSE + auth), Ollama + LMStudio via `OpenAICompatProvider` (no auth)

### Glyph Rendering (updated post-Phase-2)
- **Premultiplied alpha:** glyph pass fragment shader outputs `vec4(fg*alpha, alpha)`; blend
  state `One/OneMinusSrcAlpha`. Alpha-0 glyph edge pixels are transparent — bg pass shows through.
  This alone fixes powerline separator fringing without needing X-axis clipping.
- **X axis: never clamped.** JetBrains Mono calt ligatures use negative bearing_x to extend left;
  MonoLisa Nerd Font non-Mono double-wide icons need to extend right. Both are correct.
- **Y clamp:** `y1 = min(oy+gh, cell_height)` prevents row bleeding (TD-012).
