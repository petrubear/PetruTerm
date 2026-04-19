# Technical Debt Registry

**Last Updated:** 2026-04-19
**Open Items:** 11
**Critical (P0):** 0 | **P1:** 0 | **P2:** 11 | **P3:** 0

> Resolved items are in [TECHNICAL_DEBT_archive.md](./TECHNICAL_DEBT_archive.md).

## Priority Definitions

| Priority | Definition | SLA |
|----------|------------|-----|
| P0 | Blocking development or causing incidents | Immediate |
| P1 | Significant impact on velocity or correctness | This sprint |
| P2 | Moderate impact, workaround exists | This quarter |
| P3 | Minor, address when convenient | Backlog |

---

## P1 — Alta prioridad

_Ninguno abierto. Todos los P1 cerrados 2026-04-16 (TD-RENDER-01/02/03, TD-PERF-36). Ver archive._

---

## P2 — Prioridad media

### TD-MEM-09: Scrollback por pane sin límite efectivo en sesiones largas con muchos tabs
- **Archivo:** `src/term/mod.rs:Terminal::new()` — `scrolling_history: config.scrollback_lines`
- **Descripción:** Con el default de 10 000 líneas y ~200 bytes/línea, cada terminal ocupa ~2 MB de scrollback. Con 10 tabs × 2 panes = 20 terminales, son ~40 MB. Con `scrollback_lines = 50 000` (valor sugerido en el README), son 200 MB.
- **Fix:** (a) Reducir el default a 5 000. (b) Documentar el impacto en `perf.lua`. (c) Límite global de scrollback total (ej. 100 MB) distribuido entre terminales activos.
- **Severidad:** P2 — no es un leak sino un diseño con alto consumo base. Configurable.

---

_TD-MEM-10, 11, 12 — RESUELTOS 2026-04-17. Ver archivo._

---

_TD-MEM-13 — RESUELTO 2026-04-19. `MAX_CHARS=50_000` en `tools.rs:175`; `MAX_TOOL_ROUNDS=5` en `ui.rs:687`._

---

_TD-MEM-19 — RESUELTO 2026-04-18. `window_focused: bool` en App; blink + git poll suspendidos en focus loss; ControlFlow::Wait. Ver archive._

---

_TD-MEM-20, 21 — RESUELTOS 2026-04-17. Ver archivo._

---

_TD-MEM-23 — RESUELTO 2026-04-19. `agent_step` ya toma `&[Value]`; llamada en `ui.rs:690` pasa `&api_msgs`. Sin clone por round._

---

_TD-PERF-04 — RESUELTO 2026-04-19. `open_file_picker_async` usa `std::thread::spawn` + `crossbeam_channel::bounded(1)`; `poll_file_scan` drena sin bloquear._

---

### TD-PERF-05: Atlas de glifos siempre 64 MB de VRAM desde arranque
- **Archivo:** `src/renderer/atlas.rs:GlyphAtlas::new()`
- **Descripción:** Textura RGBA 4096×4096 = 64 MB de VRAM al arranque. Menos crítico en Apple Silicon (unified memory); importante para Phase 2+ con GPUs discretas.
- **Fix:** Empezar con 1024×1024 = 4 MB y crecer dinámicamente. Requiere recrear textura + re-subir glifos calientes.
- **Severidad:** P2 — bajo impacto hoy, bloqueante para cross-platform (Phase 2).

---

### TD-PERF-14: Scroll bar construido como N `CellVertex` (uno por fila)
- **Archivo:** `src/app/renderer.rs:1219-1230`
- **Descripción:** El scroll bar emite `screen_rows` instancias `CellVertex` — hasta 60 por scroll bar. Son semánticamente 2 rectángulos (track + thumb) que podrían ser 2 `RoundedRectInstance`.
- **Fix:** Migrar `build_scroll_bar_instances` a `rect_instances` (2 rects). Elimina 60 `CellVertex` del glyph pipeline.
- **Severidad:** P2 — mitigado por cache existente; simplifica el pipeline.

---

_TD-PERF-15 — RESUELTO 2026-04-19. Cmd+C/V ya eran async (thread::spawn). ClipboardStore/Load (OSC 52) en `mux.rs` migrados a thread::spawn; ClipboardLoad reinyecta resultado como PtyWrite via `Pty.tx` (nuevo campo `pub tx: Sender<PtyEvent>` en `pty.rs`)._

---

_TD-PERF-16 — RESUELTO 2026-04-19. Tab bar ahora cachea inputs copiables (`active_index`, `total_cols`, `titles`, `rename_input`) y compara directamente sin hash. Eliminadas 20-50 alocaciones Vec por frame. Status bar sigue con hash (menos frecuente). Commit `67a340a`._

---

_TD-PERF-17 — RESUELTO 2026-04-17. Ver archivo._

---

### TD-PERF-19: `poll_git_branch` sin guard de tarea en curso
- **Archivo:** `src/app/ui.rs:265-293`
- **Descripción:** Si `git branch --show-current` tarda >5 s (NFS, repo enorme), el siguiente frame cumplirá el TTL y disparará otro spawn, encadenando tasks tokio redundantes.
- **Fix:** `git_branch_in_flight: bool` activado al spawn, desactivado al recibir resultado. Solo spawnear si `!in_flight && (cwd_changed || ttl_expired)`.
- **Severidad:** P2 — raro pero acumula tasks en casos patológicos.

---

_TD-PERF-20 — RESUELTO 2026-04-17. Ver archivo._

---

_TD-PERF-21 — RESUELTO 2026-04-19. `last_filter_query` + path incremental en `filter()` (`palette/mod.rs:157-185`). O(prev_results) cuando query extiende el anterior._

---

_TD-PERF-22, 31, 32, 33, 34, 37 — RESUELTOS 2026-04-17. Ver archivo._

---

## P3 — Prioridad baja / Backlog

_TD-MEM-14 — RESUELTO 2026-04-19. `sync_channel(1)` + `try_send`; eventos extras descartados silenciosamente. Ver archivo._

---

_TD-MEM-15 — RESUELTO 2026-04-19. Comentario en `FreeTypeCmapLookup` documenta el patrón y la guía de Arc<Mutex> para futura expansión. Sin cambio funcional. Ver archivo._

---

_TD-MEM-16, 17, 18 — RESUELTOS 2026-04-19. Ver archivo._

---

_TD-MEM-22 — RESUELTO 2026-04-19. `bounded(256)` para `ai_tx`/`ai_rx`; `bounded(64)` para `block_tx`/`block_rx`. Ver archivo._

---

_TD-MEM-24 — RESUELTO 2026-04-19. `undo_stack: VecDeque`; `pop_front()` en evicción; `push_back()` en push; `pop_back()` en undo. Ver archivo._

---

_TD-MEM-25 — RESUELTO 2026-04-19. `bounded(1)` en `git_tx`/`git_rx`; send ya ignora error con `let _ =`. Ver archivo._

---

_TD-PERF-03 — DIFERIDO a Phase 2+. En Apple Silicon `write_buffer` es memcpy en unified memory (~48 MB/s vs 100+ GB/s bus); no medible. Dirty-rect tracking aplica solo con GPUs discretas (cross-platform)._

---

_TD-PERF-18 — RESUELTO 2026-04-19. `worker_threads(2)` en tokio builder. Ver archivo._

---

_TD-PERF-23 — RESUELTO 2026-04-19. `leader_timer` → `leader_deadline: Option<Instant>`; set `Instant::now() + Duration::from_millis(timeout_ms)` on activate; check `Instant::now() >= deadline`. Ver archivo._

---

_TD-PERF-24 — RESUELTO 2026-04-19. `separator_snapshot: Vec<PaneSeparator>` en App; actualizado en el render path; `separator_at_pixel` usa el snapshot en lugar de recomputar. Ver archivo._

---

_TD-PERF-25 — RESUELTO 2026-04-19. Palette abre inmediato con placeholder; `std::thread::spawn` corre `list_git_branches_sync`; `poll_branch_scan` rellena items al llegar. `Action::Noop` añadido para el placeholder. Ver archivo._

---

_TD-PERF-26 — RESUELTO 2026-04-19. `bounded(256)`; `try_send` + `log::debug!(pty_backpressure_hit)` cuando lleno; wakeup siempre enviado para que el main thread drene. Ver archivo._

---

_TD-PERF-28 — RESUELTO 2026-04-19. Las 3 llamadas en el hot path de shaping (shaper.rs líneas 793, 822, 833) guardadas con `log::log_enabled!(Debug)`. Resto son escalares baratos o cold paths — no requieren guard._

---

_TD-PERF-29 — DIFERIDO a TD-PERF-30 (benchmarks). `mimalloc` requiere criterion baseline previo para validar ganancia real. Implementar después de TD-PERF-30._

---

_TD-PERF-35 — RESUELTO 2026-04-19. `gap_buf: String` en `RenderContext`; `mem::take` + `extend(repeat(' ').take(gap))` + restore. Ver archivo._

---

_TD-MAINT-01 — RESUELTO 2026-04-19. `cargo-audit` instalado y ejecutado en CI (`.github/workflows/ci.yml` job `check`). Ver archivo._

---

_TD-PERF-30 — RESUELTO (ya implementado). `benches/` existe con shaping/search/build_instances/rasterize. CI regression gate con critcmp >5%. Feature flag `profiling` con tracing-tracy. HUD F12 con frame times + latency p50/p95/p99. Ver ci.yml y benches/._

---

## Recomendaciones generales (no-issues, direcciones estratégicas)

### REC-PERF-01: Pre-shape ASCII range al arranque
El 95%+ de los glifos tipeados son ASCII imprimible (32-126). Pre-shape + pre-rasterize al cargar la fuente y marcar esas entradas como "hot / never evict" en el atlas. Elimina cache-misses para el caso dominante.

### REC-PERF-02: `parking_lot::Mutex` en lugar de `std::sync::Mutex`
En macOS `parking_lot` es ~2× más rápido en paths no contendidos. Relevante si aparece contention con PTY reader + main thread. Auditar dónde se usa `Arc<Mutex<...>>`.

### REC-PERF-03: Damage tracking de alacritty_terminal — RESUELTO 2026-04-18
`collect_grid_cells_for` integra `TermDamage` API. Filas no dañadas se saltan
cuando no hay selection/search activo. Ver commit `2c945fe`.

### REC-PERF-04: Medir antes de optimizar
**Ningún fix P2/P3 debe implementarse sin profiling previo**. Instalar TD-PERF-30 primero. Algunos items pueden resultar irrelevantes en la práctica; otros no detectados aquí pueden ser los verdaderos cuellos de botella.

### REC-PERF-05: Frame budget explícito
Documentar en `.context/specs/term_specs.md`:
- **Input-to-pixel p99:** < 8 ms (un frame a 120 Hz).
- **Steady-state idle:** 0 trabajo (no dirty → no redraw).
- **Cache-miss cold start:** < 16 ms (un frame a 60 Hz).
- **Atlas evict + reshape storm:** < 50 ms.

### REC-PERF-06: Criterion CI gating
Baseline almacenado en `target/criterion/baseline/`. PR falla si `shape_line` regresa >5%, `build_instances` >3%, `search` >10%.
