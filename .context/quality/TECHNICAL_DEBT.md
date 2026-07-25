# Technical Debt Registry

**Last Updated:** 2026-07-25
**Open Items:** 7
**Critical (P0):** 0 | **P1:** 4 | **P2:** 3 | **P3:** 0 | **Deferred:** 2 | **Watch:** 3

> Completed and resolved debt was archived in [`TECHNICAL_DEBT_archive.md`](./TECHNICAL_DEBT_archive.md).

---

## Priority Definitions

| Priority | Definition | SLA |
|----------|------------|-----|
| P0 | Blocking development or causing incidents | Immediate |
| P1 | Significant impact on velocity or correctness | This sprint |
| P2 | Moderate impact, workaround exists | This quarter |
| P3 | Minor, address when convenient | Backlog |

---

## Open Debt Items

### P1 — High Priority

**GRAPH-ARCH-01 — OPEN.** `Config` is a top cross-community bottleneck (`70` edges, high betweenness).  
**Impact:** broad coupling across app, llm, ui, term, renderer.  
**Remediation:** split into bounded sub-configs and expose narrower read views.  
**Next slice:** extract an isolated `LlmConfig` access boundary.
GRAPH-ARCH-01 progress: first slice done — added LLM runtime config view and migrated UiManager rewire flow.

**GRAPH-ARCH-02 — OPEN.** `FontConfig` is over-connected (`50` edges) across runtime and non-runtime surfaces.  
**Impact:** font changes carry unnecessary blast radius.  
**Remediation:** split runtime font settings from benchmark/dev-only settings.  
**Next slice:** introduce a minimal `RuntimeFontConfig` for render path call-sites.

**GRAPH-ARCH-03 — OPEN.** `App` remains a high-centrality orchestrator (`48` edges).  
**Impact:** increased regression risk and low change isolation.  
**Remediation:** push non-core orchestration into dedicated domain coordinators.  
**Next slice:** extract one coordinator from `about_to_wait` responsibilities.

**GRAPH-COH-01 — OPEN.** Low cohesion in `Src Config` (~0.063) and `Src App` (~0.070).  
**Impact:** weak module boundaries and slower maintenance.  
**Remediation:** split by responsibility and remove cross-cutting utility drift.  
**Next slice:** define module ownership map + boundary checklist.

### P2 — Medium Priority

**GRAPH-CYCLE-01 — OPEN (VALIDATE-FIRST).** Reported cycles: `src/font/freetype_lcd.rs <-> src/renderer/lcd_atlas.rs` and `src/platform/battery.rs` self-cycle.  
**Impact:** potential dependency-order fragility.  
**Remediation:** verify cycles, then break with interface boundary or shared neutral module.  
**Next slice:** run focused import-dependency audit and confirm true-positive status.

**GRAPH-DOC-01 — OPEN (VALIDATE-FIRST).** 20 isolated nodes and 8 thin communities indicate graph/documentation gaps.  
**Impact:** weaker architecture traceability and navigation quality.  
**Remediation:** add explicit cross-references for high-value isolated components.  
**Next slice:** resolve top 5 isolated operational nodes.

**GRAPH-AMB-01 — OPEN (VALIDATE-FIRST).** Ambiguous relation between Phase 4 focus and Phase 9 completion context.  
**Impact:** inconsistent strategic context signal.  
**Remediation:** align context files to one explicit current focus line.  
**Next slice:** reconcile `AGENTS.md` and `.context/core/ACTIVE_CONTEXT.md` phase statements.

---

## Watch

- **AUDIT-CLEAN-02** — Re-evaluate only if `ContextAction` grows materially.
- **AUDIT-PERF-10** — Re-run benchmark watch after the next hot-path optimization wave.
- **TD-P9-07** — Remove `cargo audit` ignore when upstream `winit`/Wayland dependency chain upgrades `quick-xml`.

## Deferred (Requires hardware/platform-specific profiling)

- **TD-PERF-03** — GPU dirty-rect tracking (deferred to Phase 2+).
- **TD-PERF-05** — Dynamic glyph-atlas memory strategy (deferred to Phase 2+ cross-platform work).
