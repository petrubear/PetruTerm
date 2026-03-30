# Active Context

**Current Focus:** Architectural Refactoring (God Object cleanup)
**Last Active:** 2026-03-30
**Target Completion:** Address major architectural debt (TD-034)
**Priority:** P2

## Current State

**Major Architectural Refactoring complete as of 2026-03-30.**
The 2000-line `App` struct in `src/app/mod.rs` has been decomposed into modular, focused managers following Clean Architecture principles and WezTerm's inspiration.

### Modular Components ✓
- **`RenderContext`** (`src/app/renderer.rs`): Owns WGPU resources, shaper, and orchestrates frame building. ✓
- **`Mux`** (`src/app/mux.rs`): Multiplexer managing terminals, tabs, panes, and PTY polling. ✓
- **`UiManager`** (`src/app/ui.rs`): Handles overlays (Palette, Chat Panel) and AI provider integration. ✓
- **`InputHandler`** (`src/app/input.rs`): Manages keyboard/mouse state, leader key, and cursor blinking. ✓
- **`App`** (`src/app/mod.rs`): Lean coordinator implementing `winit::ApplicationHandler`. ✓

### Remaining Priority
1. **TD-035** — UI/Terminal Coupling: Further decouple layout from terminal core (ongoing).
2. **TD-039** — ANSI key mapping improvement.

## Next Session Scope

### Priority Order
1. **Validation:** Thoroughly test all UI interactions (Palette, Chat, Tabs) in the new modular structure.
2. **Cleanup:** Address the remaining 30+ warnings (dead code, unused imports) across the project.
3. **TD-039:** Implement a data-driven ANSI key mapping system.

## Files to Reference
- `src/app/mod.rs` — new entry point coordinator.
- `src/app/renderer.rs` — GPU & Shaping logic.
- `src/app/mux.rs` — Terminal multiplexer.
- `src/app/ui.rs` — Overlay & AI management.
- `src/app/input.rs` — Input state machine.
