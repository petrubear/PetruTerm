# Active Context

**Current Focus:** **Phase 3.5 — Performance Sprint** ⚡ (prerequisito antes de Phase 4)
**Last Active:** 2026-04-14

## Estado actual del proyecto

**Phase 1–3 COMPLETE. Phase 3.5 (performance) NEXT. Phase 4 (plugins) pospuesta hasta Phase 3.5 exit.**

> Todas las features de Phase 1–3 verificadas (render, PTY, teclado, ratón, clipboard, cursor, resize, scrollback, scroll bar, trackpad, ligatures, nvim/tmux, emoji, AI panel+inline block, leader key, LLM providers, tab bar, pane splits+resize, status bar, snippets, themes, command palette, context menu, Cmd+K, Cmd+F). Ver `build_phases_archive.md` para el checklist completo.

## Deuda técnica abierta (post-auditoría 2026-04-10)

**30 items** — ver [`TECHNICAL_DEBT.md`](../quality/TECHNICAL_DEBT.md). Top P1:

| ID | Descripción | Prio |
|----|-------------|------|
| TD-PERF-30 | Sin infra de profiling — **prerequisito de todo** | P1 |
| TD-PERF-06 | Doble rasterización LCD+Swash por glifo | P1 |
| TD-PERF-07 | `clear_all_row_caches` en atlas evict (reshape storm) | P1 |
| TD-PERF-08 | `PresentMode::Fifo` + latency=2 (techo ~33 ms) | P1 |
| TD-PERF-09 | Shell context disk read por cada evento PTY | P1 |

**Orden de ataque:** Sub-phase A (measurement) primero. Nada se optimiza sin baseline.

## Próximos pasos

1. **Phase 3.5 Sub-phase A — Measurement first.** Instalar `criterion`, crear `benches/`, añadir `tracing` + HUD F12 + latency probe + GPU timestamps. Documentar `PROFILING.md`.
2. **Phase 3.5 Sub-phase B — Idle zero-cost.** Cursor como overlay independiente, damage tracking, `ControlFlow::Wait`.
3. **Sub-phases C-H** según mediciones de A.
4. **Phase 4 (plugins)** queda pospuesto hasta Phase 3.5 exit criteria.

Ver [`build_phases.md`](../specs/build_phases.md) para el plan completo con exit criteria por sub-phase.

## Keybinds actuales

| Tecla | Acción |
|-------|--------|
| `Cmd+C / Cmd+V` | Copy / paste |
| `Cmd+Q` | Quit |
| `Cmd+K` | Clear screen + scrollback |
| `Cmd+F` | Abrir/cerrar búsqueda de texto |
| `Cmd+1-9` | Cambiar a tab N |
| `^B c` | New tab |
| `^B &` | Close tab |
| `^B n/b` | Next/prev tab |
| `^B ,` | Rename active tab |
| `^B %` | Split horizontal |
| `^B "` | Split vertical |
| `^B x` | Close pane |
| `^B h/j/k/l` | Focus pane (vim-style) |
| `^B Option+←→↑↓` | Resize pane |
| `^B a` | Abrir / cerrar AI panel |
| `^B A` | Mover focus terminal ↔ chat |
| `^B e` | Explain last output |
| `^B f` | Fix last error |
| `^B z` | Undo last write |
| `^B o` | Command palette |
| `Ctrl+Space` | Inline AI block |
| Right-click | Context menu |
