# Active Context

**Current Focus:** Project Audit & Performance Optimization
**Last Active:** 2026-03-30
**Target Completion:** Address critical tech debt (Security/Performance)
**Priority:** P1

## Current State

**Audit & Core Optimizations complete as of 2026-03-30.**
A comprehensive audit identified 12 new technical debt items (TD-028 to TD-039). Critical performance and stability issues were addressed immediately.

### Performance & Stability Fixes ✓
- **TD-028: Row-Level Shaping Cache** — Implemented `RowCache` in `App`. Rows are hashed (text + colors); cached shaped glyphs and GPU instances are reused if the hash matches. HarfBuzz is now only called for "dirty" rows. ✓
- **TD-029: $O(N)$ Column Calculation** — `TextShaper::shape_line` now uses incremental character counts instead of $O(N^2)$ `chars().count()` calls. ✓
- **TD-033: Atlas Eviction Strategy** — Implemented "flush and start over" strategy. `GlyphAtlas::upload` returns `AtlasError::Full`, triggering a full atlas/cache clear and re-render. ✓
- **TD-032: GPU Upload Optimization** — Cached `CellVertex` data at the row level reduces per-frame CPU-side calculations. ✓

### Remaining High Priority (P1)
1. **TD-030** — Secret Leakage to LLM Provider: Sanitization of `last_command` in `ShellContext`.
2. **TD-031** — Insecure API Key Storage: Wrap keys in `secrecy` crate or fetch from system keychain.

### Rendering Quality (Phase 3)
- **TD-027** — Powerline separator vivid rendering — OPEN. Hybrid bg-aware premul in `fs_main` is best current approach.

## Next Session Scope

### Priority Order
1. **TD-030 / TD-031** — Security hardening (Secret leakage & API key protection).
2. **TD-034 / TD-035** — Architectural refactoring: Decompose `App` god-object and decouple UI from terminal.
3. **TD-036** — Render pass consolidation for Tiled Deferred GPUs.

## Files to Reference
- `.context/quality/TECHNICAL_DEBT.md` — updated with 12 new items and 4 resolutions.
- `src/app.rs` — contains new `RowCache` and updated `build_instances` / `RedrawRequested`.
- `src/renderer/atlas.rs` — new `AtlasError` and `upload` Result.
- `src/font/shaper.rs` — $O(N)$ optimization.
