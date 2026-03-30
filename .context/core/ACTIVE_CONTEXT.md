# Active Context

**Current Focus:** AI UX & UI Config
**Last Active:** 2026-03-30
**Target Completion:** Address UX/Implementation debt (TD-037, TD-038)
**Priority:** P3

## Current State

**AI UX & UI Config debt resolved as of 2026-03-30.**
Palette actions are now fully functional and the AI panel UI is configurable via Lua.

### UX & Implementation Fixes ✓
- **TD-037: Palette AI Integration** — Wired "Explain Last Output" and "Fix Last Error" palette actions to the actual LLM query logic in `App`. ✓
- **TD-038: Configurable AI UI** — Created `ChatUiConfig` and added `llm.ui` table to Lua. Hardcoded colors and widths in `build_chat_panel_instances` replaced with config values. ✓
- **Security & Performance** — All high-priority audit items (TD-028 to TD-033, TD-036, TD-005) were resolved earlier this session. ✓

### Remaining High Priority (P1)
- **None.** All P1 items from the audit are resolved.

### Remaining Medium Priority (P2)
- **TD-034** — "God Object" in `App`: Refactor into `RenderContext`, `InputHandler`, and `PaneManager`.
- **TD-035** — UI/Terminal Coupling: Decouple layout from terminal core.

## Next Session Scope

### Priority Order
1. **TD-034 / TD-035** — Major architectural refactoring (The Big Cleanup).
2. **TD-039** — ANSI key mapping improvement.

## Files to Reference
- `.context/quality/TECHNICAL_DEBT.md` — updated with 11 total resolutions.
- `src/app.rs` — wiring of actions and use of UI config.
- `src/config/schema.rs` — new `ChatUiConfig` struct.
- `src/config/lua.rs` — hex parser and table loader.
