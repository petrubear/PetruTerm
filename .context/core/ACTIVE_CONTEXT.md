# Active Context

**Current Focus:** Graph-driven architecture debt remediation.
**Last Active:** 2026-07-25
**Branch:** `master`
**Status:** In progress (7 open debt items).
**Next Task:** Execute remediation slices for coupling and cohesion.

## Work To Do (Open)

### P1 — High priority

1. **GRAPH-ARCH-01** — Break `Config` into bounded sub-config domains and reduce cross-module coupling.
2. **GRAPH-ARCH-02** — Isolate runtime font configuration from benchmark/dev-only font settings.
3. **GRAPH-ARCH-03** — Reduce `App` orchestration breadth by moving non-core coordination into dedicated managers.
4. **GRAPH-COH-01** — Increase cohesion of `Src Config` and `Src App` by enforcing clearer module boundaries.

### P2 — Medium priority

1. **GRAPH-CYCLE-01** *(Validate first)* — Confirm and break reported import cycles (`freetype_lcd` <-> `lcd_atlas`, battery self-cycle).
2. **GRAPH-DOC-01** *(Validate first)* — Resolve isolated nodes/thin communities with explicit context links.
3. **GRAPH-AMB-01** *(Validate first)* — Reconcile ambiguous Phase 4 <-> Phase 9 context relation across context files.

## Execution Notes

- Keep changes incremental: one debt item per PR slice.
- Re-run graphify after each remediation slice to verify reduced centrality/cycle/ambiguity signals.
- Keep architecture context files aligned with the current implementation focus.
- GRAPH-ARCH-01 first slice complete: added LLM runtime config view and migrated UiManager rewire flow.
