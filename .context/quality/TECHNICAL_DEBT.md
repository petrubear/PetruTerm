# Technical Debt Registry

**Last Updated:** 2026-05-05
**Open Items:** 0
**Critical (P0):** 0 | **P1:** 0 | **P2:** 0 | **P3:** 0 open, 2 deferred

> Resolved items are in [TECHNICAL_DEBT_archive.md](./TECHNICAL_DEBT_archive.md).

## Priority Definitions

| Priority | Definition | SLA |
|----------|------------|-----|
| P0 | Blocking development or causing incidents | Immediate |
| P1 | Significant impact on velocity or correctness | This sprint |
| P2 | Moderate impact, workaround exists | This quarter |
| P3 | Minor, address when convenient | Backlog |

---

## P0 — Crítico

_Ninguno abierto._

---

## P1 — Alta prioridad

_Ninguno abierto._

---

## P2 — Prioridad media

**TD-PERF-05** — DIFERIDO a Phase 2 (cross-platform). Atlas de glifos 64 MB de VRAM desde arranque. En Apple Silicon (unified memory) no es medible. Fix requiere textura dinámica — pospuesto para GPUs discretas.

---

## P3 — Prioridad baja / Backlog

**TD-UX-01** — RESUELTO (2026-04-30). `config.notifications.style = "toast" | "native"`. Implementación en `src/platform/notifications.rs` via `UNUserNotificationCenter` (`objc2-user-notifications`). Despacho en `App::dispatch_notification`. `NSUserNotificationUsageDescription` agregado al Info.plist en `scripts/bundle.sh` (2026-05-05).

**TD-PERF-03** — DIFERIDO a Phase 2+. `write_buffer` es memcpy en Apple Silicon unified memory — no medible. Dirty-rect tracking aplica solo con GPUs discretas.

**TD-PERF-29** — RESUELTO (2026-05-05). `mimalloc` ya estaba implementado (`Cargo.toml:108`, `main.rs:13-15`). Benches criterion corren correctamente: `build_frame_miss` 32µs, `rasterize_line_ascii` 31µs, `shape_line_unicode` 5.2µs. La nota "diferido" era incorrecta.

---

## Guía activa

### REC-PERF-04: Medir antes de optimizar
Ningún fix P2/P3 debe implementarse sin profiling previo. El HUD F12 + benches criterion son las herramientas. Ver `term_specs.md §15` para frame budget targets.
