# Active Context

**Current Focus:** Final Polish & Input Hardening
**Last Active:** 2026-03-30
**Target Completion:** Resolve remaining low-priority debt (TD-039)
**Priority:** P3

## Current State

**Robust Input Mapping (TD-039) complete as of 2026-03-30.**
Replaced manual key sequence encoding with a structured `translate_key` system that supports xterm-compatible modifier encoding for Arrows, Functional keys, and Navigation keys.

### Modular Components ✓
- **`RenderContext`** (`src/app/renderer.rs`): GPU & Shaping manager. ✓
- **`Mux`** (`src/app/mux.rs`): Multiplexer manager. ✓
- **`UiManager`** (`src/app/ui.rs`): AI & Overlay manager. ✓
- **`InputHandler`** (`src/app/input/mod.rs`): Input & Interaction manager. ✓
- **`KeyMap`** (`src/app/input/key_map.rs`): ANSI sequence translator. ✓

### Remaining Debt
1. **TD-035** — UI/Terminal Coupling: Decouple layout from terminal core.
2. **Dead Code Cleanup** — 30+ warnings still present from the major refactor.

## Next Session Scope

### Priority Order
1. **Cleanup:** Systematic pruning of unused methods, fields, and imports introduced during the decomposition of `App`.
2. **Persistence:** Add session persistence for AI chat history (optional).
3. **TD-035:** Implement a trait-based interface for the Mux to talk to UI components.

## Files to Reference
- `src/app/input/key_map.rs` — new ANSI translation logic.
- `src/app/input/mod.rs` — updated to use `key_map`.
- `.context/quality/TECHNICAL_DEBT.md` — 12 total items resolved today.
