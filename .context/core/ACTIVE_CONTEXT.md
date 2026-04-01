# Active Context

**Current Focus:** Phase 2 — AI Layer
**Last Active:** 2026-03-31
**Priority:** P1

## Current State

**Phase 1 complete. AI Panel functional as of 2026-03-31.**

### Phase 1 Milestones ✓
- **Performance:** 80% CPU reduction (RowCache) and 95% GPU bandwidth reduction (dirty-row tracking). ✓
- **Security:** Secret scrubbing in shell history and `secrecy` crate for API keys. ✓
- **Architecture:** `App` decomposed into `RenderContext`, `Mux`, `UiManager`, `InputHandler`. ✓
- **UX:** Command palette, xterm key mapping, seamless powerline rendering, ligatures, nvim/tmux verified. ✓
- **AI Panel:** Fixed (was rendering off-screen), redesigned UI, keybind `Cmd+Shift+A`. ✓

### Next Steps (Phase 2)
- LLM provider configuration via Lua (`config/default/llm.lua`)
- Streaming response improvements
- Shell context enrichment (cwd, last command)
- Inline command insertion from AI response

## Files to Reference
- `src/app/` — Modular core (renderer, mux, ui, input).
- `src/llm/` — LLM providers and chat panel state.
- `src/app/renderer.rs` — `build_chat_panel_instances` for panel UI.
- `config/default/keybinds.lua` — All keybinds including AI panel.
- `.context/quality/TECHNICAL_DEBT.md` — Known debt registry.
