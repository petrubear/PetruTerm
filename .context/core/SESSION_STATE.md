# Session State

**Last Updated:** 2026-04-18
**Session Focus:** Phase 3.5 — Tier 1 leaks + Tier 4 quick wins + Tier 2 hot path

## Branch: `master`

## Estado actual

**Phase 1–3 COMPLETE. Phase 3.5: Tier 1 CERRADO, Tier 2 CERRADO, Tier 4 CERRADO.**
**Pendiente: Tier 0 (benchmarks no bloqueados), Tier 3 (idle zero-cost), Tier 5 (arquitectura).**

## Build

- **cargo check:** PASS — 0 errores, 0 warnings (verificado 2026-04-17)
- **cargo test --lib:** PASS (9 tests)

---

## Historial de commits Phase 3.5 (cont.)

| Commit | Descripción |
|--------|-------------|
| (pendiente) | [Tier1+2+4] fix: Tier 1 leaks + Tier 4 quick wins + Tier 2 hot path |

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

## Roadmap priorizado (2026-04-17)

Orden por impacto y dependencias. Razonamiento: REC-PERF-04 exige métricas antes
de optimizar; los leaks sin acotar bloquean sesiones largas; las ganancias O(n²)
se notan en UX inmediatamente; idle cost requiere overlay infra; arquitectura
pesada va al final para evitar regresiones sin baseline.

### Tier 0 — Infraestructura de medición (bloquea Tier 2+)
- [x] `benches/search.rs` — proxy sintético (2026-04-16); baselines en `PROFILING.md`
- [ ] Bench `build_instances` — bloqueado por acoplamiento winit; extraer CPU path
- [ ] Bench `rasterize_to_atlas` — bloqueado por `wgpu::Queue`; headless adapter o aislar CPU path
- [ ] Latency probe p50/p95/p99 end-to-end
- [ ] CI gating criterion (regresión >5% falla build)

### Tier 1 — COMPLETO (2026-04-17)
- **TD-MEM-20** `chat_panels` retiene tabs cerrados — RESUELTO
- **TD-MEM-21** `row_caches` retiene terminales cerrados — RESUELTO
- **TD-MEM-12** Tasks LLM colgadas al cerrar panel — RESUELTO

### Tier 2 — COMPLETO (2026-04-17)
- **TD-PERF-37** `word_wrap` O(n²) → incremental (`streaming_stable_lines` en `RenderContext`) — RESUELTO
- **TD-PERF-22** Search highlight O(matches)/celda → `FxHashMap<i32, Vec<...>>` O(1)/celda — RESUELTO
- **TD-PERF-34** `FxHasher` en `static_hash` + `calculate_row_hash` — RESUELTO
- **TD-PERF-31** `ConfirmDisplay::for_write` movido al task async (fuera del event loop) — RESUELTO

### Tier 3 — COMPLETO (2026-04-18)
- **TD-MEM-19** Pausar timers sin foco — RESUELTO (`window_focused` flag + ControlFlow::Wait)
- Cursor overlay independiente — RESUELTO (`content_end` + `cursor_vertex_template` + fast blink path)
- Damage tracking — RESUELTO (REC-PERF-03: `TermDamage` API en `collect_grid_cells_for`)

### Tier 4 — COMPLETO (2026-04-17, salvo TD-PERF-16)
- **TD-PERF-32** `colors_scratch` → `RenderContext` — RESUELTO
- **TD-PERF-33** `filtered_picker_items` devuelve `Vec<&PathBuf>` (render loop zero-clone) — RESUELTO
- **TD-PERF-20** Truncación con `char_indices().nth(N)` — RESUELTO
- **TD-MEM-10** `file_picker_items.clear()+shrink_to_fit()` en `close_file_picker` — RESUELTO
- **TD-MEM-11** `matcher: SkimMatcherV2` campo en `ChatPanel` — RESUELTO
- **TD-PERF-17** Debounce 300 ms en `check_config_reload` — RESUELTO
- **TD-PERF-16** Cache inputs tab/status key — POSTERGADO (hash ya actúa como cache; refactor mayor)

### Tier 5 — Arquitectura pesada (último)
- Sub-E: rayon per-pane + `rtrb` PTY
- Sub-G: atlas split, ring buffer, unificar bg+glyph pass
- Sub-H: PGO con workload real

### Próximo trabajo
- **Tier 0** (desbloqueado): latency probe p50/p95/p99, CI gating. Benches de `build_instances` y `rasterize_to_atlas` siguen bloqueados.
- **Tier 5**: arquitectura pesada (rayon, atlas split, PGO) — al final.
- **TD-OP-02** (P1 abierto): Nerd Font glyph ID override frágil.

### Milestone
- Phase 3.5 exit: Tier 0 completo (métricas validan KPIs) + Tier 3 (idle zero-cost)
- Phase 4 (plugins): bloqueada hasta Phase 3.5 exit criteria

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
