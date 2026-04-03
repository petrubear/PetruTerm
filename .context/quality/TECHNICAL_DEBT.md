# Technical Debt Registry

**Last Updated:** 2026-04-03
**Open Items:** 1
**Critical (P0):** 0 | **P1:** 0 | **P2:** 1 | **P3:** 0

> Resolved items are in [TECHNICAL_DEBT_archive.md](./TECHNICAL_DEBT_archive.md).

## Priority Definitions

| Priority | Definition | SLA |
|----------|------------|-----|
| P0 | Blocking development or causing incidents | Immediate |
| P1 | Significant impact on velocity or correctness | This sprint |
| P2 | Moderate impact, workaround exists | This quarter |
| P3 | Minor, address when convenient | Backlog |

---

## P0 - Critical

_None_

---

## P1 - High Priority

_None_

---

## P2 - Medium Priority

### TD-035: Tight Coupling between UI and Terminal (Architecture)
- **File:** `src/app/ui.rs`, `src/app/mux.rs`, `src/ui/`
- **Issue:** `App` manually iterates over panes and terminals for resizing and event polling. UI layout logic is not sufficiently isolated from terminal state. No trait boundary between the UI layer and terminal instances.
- **Fix:** Define a clear trait-based interface for UI components to interact with terminal instances, enabling easier testing and alternative UI backends.
- **WezTerm Inspiration:** WezTerm uses a decoupled model where `Pane` (terminal state) is distinct from the windowing layer, communicating via events and shared state.

---

## P3 - Low Priority

_None_

---

## Open Debt Summary

| ID | Title | Priority | Area |
|----|-------|----------|------|
| TD-035 | Tight Coupling UI ↔ Terminal | P2 | Architecture |
