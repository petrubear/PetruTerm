# Session State

**Last Updated:** 2026-03-27
**Session Focus:** Phase 2 — AI Layer (in progress)

## Phase 1 Status: COMPLETE ✓

All acceptance criteria verified on M4 Max, 2026-03-27.
Build: 0 errors, ~19 warnings (dead code stubs only).
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
- [x] `LlmProvider` trait (`src/llm/mod.rs`) — `complete()` + `stream()` async, Arc-based
- [x] OpenRouter provider (`src/llm/openrouter.rs`) — SSE streaming, OPENROUTER_API_KEY env
- [x] Default model: `openrouter/auto:free` (schema.rs + llm.lua)
- [x] `ChatPanel` state machine (`src/llm/chat_panel.rs`) — replaces AiBlock; multi-turn history, streaming_buf, scroll_offset
- [x] `Ctrl+Space` toggle keybind (app.rs)
- [x] Keyboard routing to chat panel when active — Space explicitly handled (TD-019 fixed)
- [x] tokio Runtime + crossbeam channel for async streaming (App struct)
- [x] `submit_ai_query` — multi-turn: builds full message history, streams via channel
- [x] `chat_panel_run_command` — writes last assistant command to PTY, closes panel
- [x] `poll_ai_events` — drains channel in `about_to_wait`
- [x] `build_chat_panel_instances` — right-side panel; `push_shaped_row` helper; header + scrollable history + input + hints (TD-020 rewritten clean)
- [x] **TD-021:** `WindowEvent::DroppedFile` — panel open → append to input; panel closed → paste to PTY
- [x] **TD-019:** Space key in panel input — explicit `NamedKey::Space` match
- [x] **TD-020:** Render rewritten from scratch — panel at col `term_cols..term_cols+panel_cols`, no overlay hack
- [x] Panel layout split — `default_grid_size()` and `viewport_rect()` subtract `panel_cols * cell_w` when panel is open; `resize_terminals_for_panel()` called on open/close
- [ ] Shell integration (`shell-integration.zsh`)
- [ ] Ctrl+Shift+E / Ctrl+Shift+F (explain/fix)
- [ ] Ollama + LMStudio providers

## Phase 2 Kickoff: AI Layer

### Implementation Order
1. **`LlmProvider` trait** — `complete()` + `stream()` async (tokio + reqwest)
2. **Providers** — OpenRouter, Ollama, LMStudio (all OpenAI-compat)
3. **`llm.lua` config** — `provider`, `model`, `api_key`, `base_url`, `features`, `enabled`
4. **Shell integration** (`shell-integration.zsh`) — CWD, exit codes, command boundaries
5. **Inline AI block UI** — overlay `⚡ AI >`, streaming token-by-token render
6. **Toggle** — `Ctrl+Space` keybind + palette "Enable/Disable AI Features"
7. **Feature 1: NL → Shell Command** — natural language → command + `[⏎ Run]` `[Edit]` `[Explain]`
8. **Feature 2: Explain Last Output** — `Ctrl+Shift+E`
9. **Feature 3: Fix Last Error** — exit-code indicator + `Ctrl+Shift+F`
10. **Feature 4: Context-Aware Chat** — multi-turn, CWD + shell history context, per-pane

### Key Technical Constraints
- LLM requests: async tokio tasks, never block main thread
- Streaming: reqwest SSE → channel → render loop inserts tokens into AI block
- `config.llm.enabled = false` must disable all AI features cleanly
- Shell integration hooks into PTY via OSC sequences or sidecar file

## Key Technical Decisions (stable, Phase 1)
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
