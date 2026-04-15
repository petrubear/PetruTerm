# Session State

**Last Updated:** 2026-04-14
**Session Focus:** Phase 3.5 — Performance Sprint (pendiente iniciar)

## Branch: `master`

## Estado actual

**Phase 1–3 COMPLETE. Phase 3.5 es la siguiente prioridad.**

Próxima acción: Sub-phase A (Measurement Infrastructure) — ningún fix de performance se toca antes de tener baseline.

## Build
- **cargo build:** PASS — 0 errores, 0 warnings (último verificado 2026-04-10)

## Deuda técnica abierta

30 items (todos TD-PERF-06..30 + TD-MAINT-01). Ver `TECHNICAL_DEBT.md`. Top P1:
- TD-PERF-06 — doble rasterización LCD+Swash
- TD-PERF-07 — reshape storm en atlas evict
- TD-PERF-08 — PresentMode::Fifo (techo ~33 ms)
- TD-PERF-09 — shell context disk read por evento PTY
- TD-PERF-30 — sin infra de profiling (prerequisito)

---

## Sesiones anteriores

### 2026-04-10 — Exit code per-pane + click para detalles
- `poll_pty_events()` → `(Vec<usize>, Vec<usize>)` (IDs con datos + IDs que salieron)
- `terminal_shell_ctxs: HashMap<usize, ShellContext>` — contexto por terminal_id
- Shell integration: `shell-context-$$.json` per-PID con fallback al global
- Click en badge rojo → context menu con exit code + comando + "Copy command"
- `ContextAction::Label` — fila no-interactiva (dim, sin hover)
- Archivos: `shell-integration.zsh`, `llm/shell_context.rs`, `app/mux.rs`, `app/mod.rs`, `ui/context_menu.rs`, `app/renderer.rs`
