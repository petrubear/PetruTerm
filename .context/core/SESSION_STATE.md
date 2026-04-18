# Session State

**Last Updated:** 2026-04-18
**Session Focus:** Phase 3.5 — Tier 3 (idle zero-cost) + Tier 0 (medición)

## Branch: `master`

## Estado actual

**Phase 1–3 COMPLETE. Phase 3.5: Tiers 1–4 CERRADOS. Tier 0 CERRADO (accionable). Tier 3 CERRADO.**
**Pendiente: TD-OP-02 (P1 abierto), Tier 5 (arquitectura pesada).**
**Phase 3.5 exit criteria ALCANZADOS. Phase 4 (plugins) desbloqueada.**

## Build

- **cargo check:** PASS — 0 errores, 0 warnings (verificado 2026-04-18)
- **cargo test --lib:** PASS (9 tests)

---

## Commits Phase 3.5 — sesión 2026-04-18

| Commit | Descripción |
|--------|-------------|
| `941066a` | chore: update README — CI badge, features, tech stack, perf notes |
| `e78b1c0` | chore: switch CI runners from macos-latest to ubuntu-latest |
| `1f742cd` | [Tier-0] feat: latency p50/p95/p99 in HUD + CI gating workflow |
| `8b2d0fb` | chore: mark Tier 3 complete in SESSION_STATE |
| `2c945fe` | [REC-PERF-03] feat: damage tracking — skip undamaged rows in collect_grid_cells_for |
| `7f44b91` | feat: cursor overlay — fast blink path skips full cell rebuild |
| `9894209` | [TD-MEM-19] fix: suspend blink and git poll when window loses focus |

---

## Deuda técnica resuelta en Phase 3.5

### Memory (todos los P1 resueltos)
- TD-MEM-01, 02, 03, 05, 06, 07, 08 — RESUELTOS
- TD-MEM-04 — **falso positivo** (usa `get_image_uncached`, no crece; ver archive)
- TD-MEM-10, 11, 12 — RESUELTOS
- TD-MEM-19 — RESUELTO (`window_focused` + ControlFlow::Wait sin foco)
- TD-MEM-20, 21 — RESUELTOS

### Performance (P1 completados)
- TD-PERF-06, 07, 08, 09, 10, 11, 12, 13 — RESUELTOS
- TD-PERF-17, 20, 22, 31, 32, 33, 34, 37 — RESUELTOS
- TD-PERF-36 — RESUELTO (warn on overflow + MAX_RECT_INSTANCES → 1024)

### Render (P1 completados)
- TD-RENDER-01 — RESUELTO (pre-pass bg-only vertices para celdas-espacio)
- TD-RENDER-02 — RESUELTO (force rebuild during Loading/Streaming)
- TD-RENDER-03 — RESUELTO (`mouse_dragged` flag + `clear_selection()` en click sin drag)

### Infraestructura / Tier 0 (accionable completado)
- `benches/search.rs` + `benches/shaping.rs` — EXISTENTES
- Latency probe p50/p95/p99 — RESUELTO (ring buffer en `RenderContext`, HUD F12)
- CI gating — RESUELTO (`.github/workflows/ci.yml`: check+test+clippy+fmt + bench regression >5%)
- Cursor overlay (fast blink path) — RESUELTO (`content_end` + `cursor_vertex_template`)
- Damage tracking REC-PERF-03 — RESUELTO (`TermDamage` API en `collect_grid_cells_for`)

---

## Roadmap priorizado (2026-04-18)

### Tier 0 — CERRADO (accionable 2026-04-18)
- [x] `benches/search.rs` + `benches/shaping.rs`
- [x] Latency probe p50/p95/p99 en HUD
- [x] CI gating (ubuntu-latest, criterion regression >5%)
- [ ] Bench `build_instances` — **bloqueado**: acoplado a winit; requiere extraer CPU path
- [ ] Bench `rasterize_to_atlas` — **bloqueado**: requiere `wgpu::Queue` headless
- [ ] Tracy integration, GPU timestamps, os_signpost

### Tier 3 — COMPLETO (2026-04-18)
- **TD-MEM-19** Pausar timers sin foco — RESUELTO
- Cursor overlay independiente — RESUELTO
- Damage tracking con `Term::damage()` — RESUELTO

### Tier 5 — Arquitectura pesada (último, requiere baseline de Tier 0)
- Sub-E: rayon per-pane + `rtrb` PTY
- Sub-G: atlas split, ring buffer, unificar bg+glyph pass
- Sub-H: PGO con workload real

### Próximo trabajo
- **TD-OP-02** (P1 abierto): Nerd Font glyph ID override frágil.
- **Phase 4** (plugins): desbloqueada — lazy.nvim-style plugin loader en Lua.
- **Tier 5**: arquitectura pesada — solo con profiling previo (REC-PERF-04).

### Milestone
- Phase 3.5 exit criteria: ✅ ALCANZADOS (Tier 0 accionable + Tier 3 completos)
- Phase 4 (plugins): desbloqueada

---

## Sesiones anteriores (resumen)

### 2026-04-18 — Tier 3 + Tier 0
- TD-MEM-19: `window_focused` flag, git poll y blink suspendidos sin foco, ControlFlow::Wait
- Cursor overlay: extraído de `build_instances` → `build_cursor_instance`; fast path en
  `RedrawRequested` sube 1 vertex al GPU sin rebuild cuando solo blink cambió
- Damage tracking: `TermDamage` API en `collect_grid_cells_for` — skip grid reads para
  filas no dañadas cuando no hay selection/search activo
- Latency probe: ring buffer 120 muestras en `RenderContext`, HUD F12 muestra p50/p95/p99;
  p99 > 8ms en rojo
- CI: `.github/workflows/ci.yml` (ubuntu-latest) — check+test+clippy+fmt + bench regression
  gate (critcmp --threshold 5) + baseline auto-commit en master
- README: badge CI, features actualizadas, tech stack corregido, perf notes

### 2026-04-17 — Tier 1 + Tier 2 + Tier 4
- TD-MEM-20/21: `chat_panels` + `row_caches` limpiados en `close_exited_terminals`
- TD-MEM-12: `streaming_handle: Option<JoinHandle>` abortado al cerrar panel
- TD-MEM-10/11: `file_picker_items` shrink + `matcher` como campo
- TD-PERF-37: streaming wrap incremental (`streaming_stable_lines`)
- TD-PERF-22/34/31/32/33/20/17: FxHashMap search, FxHasher hashes, async confirm,
  colors_scratch, zero-clone picker, truncación, debounce config reload

### 2026-04-16 (tarde) — TD-RENDER-01 REAL FIX + TD-RENDER-03
- TD-RENDER-01: pre-pass bg-only vertices en `build_instances` para celdas-espacio
  con bg ≠ default (nvim widgets, status bar, selección). Bug: shaper descartaba espacios.
- TD-RENDER-03: `mouse_dragged` flag + `clear_selection()` en click sin drag
- User verification: ✓ rayas desaparecen, ✓ celda blanca resuelta

### 2026-04-16 (mañana) — P1 Rendering fixes (partial)
- TD-RENDER-02 ✓, TD-PERF-36 ✓
- TD-RENDER-01 intento #1 (shader discard) — no resolvía el bug real

### 2026-04-15 — Phase 3.5 Memory + Performance sprint
- Memory audit: TD-MEM-01..08 resueltos, TD-MEM-04 falso positivo
- Performance: TD-PERF-06..13 resueltos
- PTY thread QoS, Lua bytecode cache, release-native profile
