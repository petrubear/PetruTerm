# Active Context

**Current Focus:** Phase 2 — AI Layer
**Last Active:** 2026-03-27
**Target Completion:** Phase 2 MVP
**Priority:** P0

## Current State

**Phase 1 complete as of commit 7bee09b (2026-03-27):**
All acceptance criteria verified on M4 Max.

### Phase 1 Verified ✓
- Dracula Pro background `#22212c` ✓
- JetBrains Mono Nerd Font Mono 15pt, 18×36px at 2× Retina ✓
- zsh + Starship, keyboard input (including Ctrl keys), `ls` output ✓
- Mouse: drag selection, scroll wheel (trackpad+mouse), SGR/X10 reporting ✓
- Clipboard: Cmd+C/V, OSC 52, bracketed paste ✓
- Cursor: block/underline/beam, 530ms blink, resets on keypress ✓
- PTY resize: uses actual cell px from TextShaper ✓
- Shell exit: `exit` / Ctrl+D closes window ✓
- Nerd Font icons: clamped to cell height, no row bleeding ✓
- Config hot-reload ✓
- Custom title bar: transparent, traffic lights, draggable ✓
- Launch directory: opens in `~` ✓
- .app bundle: `dist/PetruTerm.app` (18 MB, ad-hoc signed) ✓
- App icon: Dracula purple chevron + cursor ✓
- Scrollback: 110k lines, display_offset-aware rendering ✓
- Top padding: 60px physical clears traffic lights ✓
- Arrow keys APP_CURSOR mode (atuin, nvim, tmux) ✓
- Reverse-video (SGR 7 / Flags::INVERSE) ✓
- nvim: colors, cursor, input, scroll ✓
- tmux: attach, split, scroll, Ctrl+B prefix ✓
- Font ligatures: `->` `=>` `==` `===` `!=` `>=` `|>` ✓

## Scope

### Phase 2 — AI Layer
New files to create:
- `src/llm/mod.rs` — module root, re-exports
- `src/llm/provider.rs` — `LlmProvider` trait
- `src/llm/openrouter.rs` — OpenRouter provider
- `src/llm/ollama.rs` — Ollama provider
- `src/llm/lmstudio.rs` — LMStudio provider
- `src/llm/engine.rs` — engine: manages active provider, spawns requests
- `src/llm/context.rs` — shell context builder (CWD, history, last output)
- `src/ui/ai_block.rs` — inline AI block UI overlay
- `config/default/llm.lua` — default LLM config
- `scripts/shell-integration.zsh` — PTY shell integration hooks

Files to modify:
- `src/app.rs` — AI keybinds, AI block render, feature dispatch
- `src/ui/mod.rs` — export ai_block
- `src/main.rs` — wire tokio runtime for LLM tasks
- `Cargo.toml` — add reqwest (already present), serde_json

### Out of Scope (Phase 3)
- `src/plugins/` — Phase 3
- `src/snippets/` — Phase 3
- `src/ui/statusbar/` — Phase 3

## Acceptance Criteria (Phase 2)
- [ ] `cargo build` — zero errors
- [ ] `config.llm.enabled = false` compiles and disables all AI cleanly
- [ ] OpenRouter provider: streams a response given api_key + model
- [ ] Ollama provider: streams a response from localhost
- [ ] `Ctrl+Space` opens inline AI block
- [ ] AI block renders streaming tokens in real-time
- [ ] NL → shell command works end-to-end (type → get suggestion → run)
- [ ] Explain Last Output: `Ctrl+Shift+E` explains terminal output
- [ ] Fix Last Error: indicator on non-zero exit, `Ctrl+Shift+F` suggests fix
- [ ] Context-Aware Chat: multi-turn with CWD + history context

## Technical Reference
- LLM async: `tokio::spawn` from `App` via `Arc<tokio::runtime::Handle>`
- Streaming: reqwest `Response::bytes_stream()` → SSE parse → `mpsc::channel` → render
- Shell context: OSC custom sequences or `~/.cache/petruterm/shell-context.json`
- AI block position: bottom of active pane, above prompt line

## Files to Reference
- `.context/specs/build_phases.md` — Phase 2 deliverables checklist
- `.context/specs/term_specs.md` — authoritative spec
- `.context/quality/TECHNICAL_DEBT.md` — open debt items
- `.context/core/SESSION_STATE.md` — session notes + Phase 2 order
