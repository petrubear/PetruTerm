# Active Context

**Current Focus:** Project Security Hardening
**Last Active:** 2026-03-30
**Target Completion:** Fix critical security tech debt (TD-030/TD-031)
**Priority:** P1

## Current State

**Security Hardening complete as of 2026-03-30.**
Critical security vulnerabilities related to LLM integration were addressed using secret-protecting types and command sanitization.

### Security & Core Optimizations ✓
- **TD-030: LLM Secret Scrubbing** — Added regex-based sanitization to `ShellContext` to redact secrets (exports, tokens, auth headers) from shell history before sending to LLM provider. ✓
- **TD-031: Secure API Key Storage** — Switched LLM API keys to `secrecy::SecretString` and skipped serialization to prevent memory, disk, and log leakage. ✓
- **TD-028: Row-Level Shaping Cache** — Implemented `RowCache` in `App`. HarfBuzz is now only called for "dirty" rows. ✓
- **TD-029: $O(N)$ Column Calculation** — Shaping speed optimized. ✓
- **TD-033: Atlas Eviction Strategy** — Stability improved with automated atlas resets. ✓

### Remaining High Priority (P1)
- **TD-034** — "God Object" in `App`: Refactor into `RenderContext`, `InputHandler`, and `PaneManager`.
- **TD-035** — UI/Terminal Coupling: Decouple layout from terminal core.

## Next Session Scope

### Priority Order
1. **TD-034 / TD-035** — Architectural refactoring: Decompose `App` god-object and decouple UI from terminal.
2. **TD-036** — Render pass consolidation for Tiled Deferred GPUs (bandwidth optimization).
3. **TD-032** — GPU Dirty-row tracking (further performance optimization).

## Files to Reference
- `.context/quality/TECHNICAL_DEBT.md` — updated with 6 resolutions.
- `src/llm/shell_context.rs` — contains command sanitization logic.
- `src/config/schema.rs` — uses `SecretString` for keys.
- `src/llm/openrouter.rs` — updated to use `ExposeSecret`.
