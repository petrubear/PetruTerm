# Session State

**Last Updated:** 2026-04-16
**Session Focus:** Phase 3.5 — Visual regression + perf fixes

## Branch: `master`

## Estado actual

**Phase 1–3 COMPLETE. Phase 3.5 (performance + memory) SPRINT EN CIERRE.**
**P1 fixes: TD-RENDER-02, TD-PERF-36, TD-RENDER-01 (real fix), TD-RENDER-03 resueltos.**

## Build

- **cargo check:** PASS — 0 errores, 0 warnings (verificado 2026-04-16 post-RENDER-01-real)
- **cargo test --lib:** PASS (9 tests)

---

## Historial de commits Phase 3.5 (cont.)

| Commit | Descripción |
|--------|-------------|
| `e83e731` | [TD-RENDER-02] fix: force panel rebuild during Loading/Streaming to stabilize spinner |
| `05b7a5d` | [TD-PERF-36] fix: warn on instance buffer overflow + raise rect cap |
| `4e75c54` | chore: archive resolved debt + reprioritize kiro/codex audit |
| `b28165e` | [TD-PERF-12/13] scratch_lines reuse + frame_counter spinner |
| `b5372a8` | [TD-PERF-11] incremental text search |
| `3270614` | [TD-PERF-10] split panel render: content cache + live input rows |
| `334dee0` | [TD-PERF-06/07/09] skip double rasterization + lazy row cache + mtime guard |
| `9a2fcb8` | [TD-MEM-06/07] byte_to_col_buf shrink + ChatPanel message cap |
| `64a23c8` | chore: context files Phase 3.5 memory sprint |
| `188c1e8` | [TD-MEM-01/02] atlas eviction reclaims physical space + LCD eviction |
| `d0c3b1b` | [TD-MEM-03] rebuild atlas bind groups after atlas.clear() |
| `6730465` | [TD-MEM-05/08] word_cache LRU + terminal_shell_ctxs cleanup |

---

## Deuda técnica resuelta en Phase 3.5

### Memory (todos los P1 resueltos)
- TD-MEM-01, 02, 03, 05, 06, 07, 08 — RESUELTOS
- TD-MEM-04 — **falso positivo** (usa `get_image_uncached`, no crece; ver archive)

### Performance (P1 completados)
- TD-PERF-06, 07, 08, 09, 10, 11, 12, 13 — RESUELTOS
- TD-PERF-36 — RESUELTO (warn on overflow + MAX_RECT_INSTANCES → 1024)

### Render (P1 completados)
- TD-RENDER-02 — RESUELTO (force rebuild during Loading/Streaming)
- TD-RENDER-01 — VERIFICADO (todos los mutation points marcan dirty = true)

---

---

## Siguiente — Quick wins P2

### P2 sugeridos (post-render fixes)
3. **TD-PERF-32** — Mover `colors_scratch` a `RenderContext` (`src/app/renderer.rs:191`)
4. **TD-PERF-20** — Truncación `char_indices().nth(N)` en `src/app/renderer.rs:662,663,754`
5. **TD-PERF-19** — `poll_git_branch` in-flight guard en `src/app/ui.rs:265`
6. **TD-PERF-36** — `MAX_RECT_INSTANCES` warning + increase a 1 024 (`src/renderer/gpu.rs:20`)

### Milestone
- Phase 3.5 exit: P1 completos ✓. Opcionalmente: P2 quick wins (PERF-32, PERF-20, PERF-19).
- Phase 4 (plugins): Lua API pública, auto-scan `~/.config/petruterm/plugins/`

---

## Sesiones anteriores (resumen)

### 2026-04-16 (tarde) — TD-RENDER-01 REAL FIX + TD-RENDER-03
- **TD-RENDER-01 real root cause (user verified)**: el shader-level discard previo
  era defensivo; el bug real era en `build_instances` (`src/app/renderer.rs`). El
  shaper's `try_word_cached_shape` y `shape_line_harfbuzz` descartan celdas con
  espacio. Cualquier línea con chars disparadores de ligaduras (`= < > - | + * / ~ ! : .`)
  va por el word-cached path. Celdas-espacio con bg ≠ default (widgets nvim, status
  bar, selección) quedaban sin vértice → clear color → franjas horizontales.
  - **Fix**: pre-pase en `build_instances` emite un vértice bg-only por cada celda
    con `bg ≠ default_bg`, antes del bucle de glyphs.
- **TD-RENDER-03 (nuevo)**: selección de 1 celda persistía al soltar click sin drag
  (bg blanco en la posición del mouse). Fix: flag `mouse_dragged` en InputHandler;
  en `Released` sin drag llama `terminal.clear_selection()`.
- User verification: ✓ rayas desaparecen, ✓ celda blanca resuelta

### 2026-04-16 (mañana) — P1 Rendering fixes (partial)
- TD-RENDER-02 ✓ (force rebuild during Loading/Streaming for smooth spinner)
- TD-PERF-36 ✓ (warn on overflow, MAX_RECT_INSTANCES 256→1024)
- TD-RENDER-01 intento #1 (shader discard uv≈[0,0]) — no resolvía el bug real
- 6 commits, cargo check PASS

### 2026-04-15 — Phase 3.5 Debt audit + cleanup
- Dos regresiones visuales identificadas en screenshot: TD-RENDER-01/02 (P1)
- Deuda técnica: archivados 12 items resueltos (MEM + PERF)
- Reprioritización auditoría kiro/codex: PERF-36 subido a P1, PERF-32 bajado a P2, varios P2→P3

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
