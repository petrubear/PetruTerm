# Active Context

**Current Focus:** Phase 2 — AI Layer + UX Polish
**Last Active:** 2026-04-03
**Priority:** P1

## Current State

**Phase 1 complete. TD-040 leader key system complete as of 2026-04-03.**

### Phase 1 Milestones ✓
- **Performance:** 80% CPU reduction (RowCache) and 95% GPU bandwidth reduction (dirty-row tracking). ✓
- **Security:** Secret scrubbing in shell history and `secrecy` crate for API keys. ✓
- **Architecture:** `App` decomposed into `RenderContext`, `Mux`, `UiManager`, `InputHandler`. ✓
- **UX:** Command palette, xterm key mapping, seamless powerline rendering, ligatures, nvim/tmux verified. ✓
- **AI Panel:** Functional, redesigned UI, keybind `<leader>a`. ✓
- **Leader Key System:** All custom keybinds via leader — fully Lua-configurable. ✓

### Bug Fixes Applied (2026-04-03)
- Mouse selection now accounts for `display_offset` — works correctly when scrolled. ✓
- Selection extends into scrollback when scrolling while mouse button held. ✓
- Typing delay eliminated — `user_event` calls `request_redraw()` on PTY data. ✓
- Font memory: ~200 KB/frame allocation removed from per-frame `scaled_font_config`. ✓
- `locate_via_font_kit` no longer loads all font variants — uses `select_best_match`. ✓

### Next Steps (Phase 2)
- LLM provider configuration via Lua (`config/default/llm.lua`)
- Streaming response improvements
- Shell context enrichment (cwd, last command)
- Inline command insertion from AI response

## Files to Reference
- `src/app/input/mod.rs` — Leader key dispatch; system keybinds.
- `src/ui/palette/actions.rs` — `Action` enum + `FromStr`; add new actions here.
- `config/default/keybinds.lua` — Single source of truth for all custom keybinds.
- `src/app/ui.rs` — `handle_palette_action` — action execution logic.
- `src/config/lua.rs` — Lua parsing for `config.leader`, `config.keys`.
- `src/llm/` — LLM providers and chat panel state.
- `.context/quality/TECHNICAL_DEBT.md` — Known debt registry.
