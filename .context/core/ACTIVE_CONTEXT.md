# Active Context

**Current Focus:** GPU Performance & PTY Stability
**Last Active:** 2026-03-30
**Target Completion:** Address remaining high-impact performance debt (TD-032, TD-036, TD-005)
**Priority:** P1

## Current State

**GPU Performance & PTY Stability optimizations complete as of 2026-03-30.**
Significant improvements to memory bandwidth, power efficiency, and process management were implemented.

### Performance & Stability Fixes ✓
- **TD-032: GPU Dirty-Row Tracking** — `GpuRenderer` now supports partial buffer updates. `App` only uploads terminal rows that actually changed, drastically reducing memory traffic. ✓
- **TD-036: Render Pass Consolidation** — BG and Glyph passes merged into a single pass ("terminal pass"). Prevents tile memory reloads on Apple Silicon. ✓
- **TD-005: Clean PTY Shutdown** — Replaced type-erased thread handles with `std::thread::JoinHandle`. `App::drop` now triggers a graceful shutdown of all shell processes. ✓
- **TD-030/TD-031: Security Hardening** — Secret scrubbing and `secrecy` storage implemented. ✓
- **TD-028/TD-029: Shaping Optimizations** — Row caching and $O(N)$ tracking implemented. ✓

### Remaining High Priority (P1)
- **TD-034** — "God Object" in `App`: Refactor into `RenderContext`, `InputHandler`, and `PaneManager`. (Next major focus).

## Next Session Scope

### Priority Order
1. **TD-034 / TD-035** — Architectural refactoring: Decompose `App` god-object and decouple UI from terminal.
2. **TD-037** — Wire up Palette actions (Explain/Fix) to AI logic.
3. **TD-038 / TD-039** — UI polish: Move constants to Lua and improve ANSI key mapping.

## Files to Reference
- `.context/quality/TECHNICAL_DEBT.md` — updated with 9 resolutions.
- `src/renderer/gpu.rs` — consolidated render passes and partial uploads.
- `src/app.rs` — implementation of dirty-row tracking and clean drop.
- `src/term/pty.rs` — thread join handle implementation.
