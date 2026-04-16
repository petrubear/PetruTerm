# Session State

**Last Updated:** 2026-04-15
**Session Focus:** Phase 3.5 — Performance Sprint

## Branch: `master`

## Estado actual

**Phase 1–3 COMPLETE. Phase 3.5 (performance + memory) EN PROGRESO.**

## Build

- **cargo check:** PASS — 0 errores, 0 warnings (verificado 2026-04-15, commit b28165e)

---

## Historial de commits Phase 3.5

| Commit | Descripción |
|--------|-------------|
| `b28165e` | [TD-PERF-12/13] scratch_lines reuse + frame_counter spinner |
| `b5372a8` | [TD-PERF-11] incremental text search |
| `3270614` | [TD-PERF-10] split panel render: content cache + live input rows |
| `334dee0` | [TD-PERF-06/07/09] skip double rasterization + lazy row cache + mtime guard |
| `9a2fcb8` | [TD-MEM-06/07] byte_to_col_buf shrink + ChatPanel message cap |
| `64a23c8` | chore: context files Phase 3.5 memory sprint |
| `188c1e8` | [TD-MEM-01/02] atlas eviction reclaims physical space + LCD eviction |
| `d0c3b1b` | [TD-MEM-03] rebuild atlas bind groups after atlas.clear() |
| `6730465` | [TD-MEM-05/08] word_cache LRU + terminal_shell_ctxs cleanup |
| `86683c7` | Phase 3.5-E/H: PTY QoS + Lua cache + native build profile |
| `3030c29` | Phase 3.5-D: scratch buffers + mimalloc |

---

## Deuda técnica resuelta en Phase 3.5

### Memory (todos los P1 resueltos)
- TD-MEM-01, 02, 03, 05, 06, 07, 08 — RESUELTOS
- TD-MEM-04 — falso positivo (usa `get_image_uncached`, no crece)

### Performance
- TD-PERF-06, 07, 08, 09, 10, 11, 12, 13 — RESUELTOS

---

## Próxima sesión — candidatos sugeridos

### Quick wins (1-2 h)

1. **TD-PERF-20** — Truncación O(n) con `chars().count()` → `char_indices().nth(N)` zero-alloc
   - `src/app/renderer.rs:464` (spinner — ya resuelto), `662, 663, 754` (truncación paths/hints)
   - Fix de 3 líneas por sitio, cero riesgo

2. **TD-PERF-19** — `poll_git_branch` sin guard de vuelo en `src/app/ui.rs:265-293`
   - Añadir `git_branch_in_flight: bool` al estado; guard de una línea

3. **TD-PERF-18** — Tokio pool `new_multi_thread()` → `.worker_threads(2)` en `src/app/ui.rs:93`
   - Cambio de 1 línea; 2 workers suficientes para I/O-bound tasks

### Impacto medio (2-4 h)

4. **TD-PERF-15** — Clipboard async: `arboard` bloquea event loop en paste grande
   - `src/app/mod.rs:703,709`, `src/app/input/mod.rs:481,488`, `src/app/mux.rs:134,136`
   - Mover a `tokio::task::spawn_blocking`; paste via canal

5. **TD-PERF-16** — Hash key tab bar / status bar recalculado por frame
   - `src/app/mod.rs:454-461` (tab_key), `mod.rs:554-568` (sb_key)
   - Cachear inputs previos como tupla copiable; `==` directo antes del hash

6. **TD-PERF-22** — Search highlight O(matches) por celda
   - `src/app/mux.rs:441-454` → `HashMap<i32, Vec<(col_start, col_end)>>` indexado por línea

### Siguiente milestone (Phase 4)
- Phase 3.5 exit criteria: todos los P1 y P2 de alto impacto resueltos
- TD-OP-02 (P1): fragile Nerd Font glyph ID override — abierto
- Phase 4 (plugins): Lua API pública, auto-scan `~/.config/petruterm/plugins/`

---

## Sesiones anteriores (resumen)

### 2026-04-15 — Phase 3.5 Performance + Memory
- Memory audit completo: 12 items (TD-MEM-01..12 + TD-MEM-19)
- Todos los P1 de memory resueltos (TD-MEM-01..03, 05..08)
- Performance sprint: TD-PERF-06..13 resueltos
- PTY thread QoS, Lua bytecode cache, release-native profile

### 2026-04-10 — Exit code per-pane + shell integration
- `poll_pty_events()` devuelve IDs con datos + IDs que salieron
- `terminal_shell_ctxs` per-PID con fallback al global
- Click en badge rojo → context menu con detalles
- `ContextAction::Label` — fila no-interactiva
