# Technical Debt Registry

**Last Updated:** 2026-04-19
**Open Items:** 22
**Critical (P0):** 0 | **P1:** 0 | **P2:** 12 | **P3:** 10

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

### TD-MEM-13: `api_messages` en el agent loop crece con cada round de tool calls
- **Archivo:** `src/llm/tools.rs`
- **Descripción:** El agent loop acumula `api_messages: Vec<Value>` con cada round (hasta 10 rounds). Con archivos grandes adjuntos y 10 rounds, el Vec puede acumular varios MB de JSON transitorio.
- **Fix:** (a) Limitar el tamaño de resultados de `ReadFile` a 50 000 chars con truncación explícita. (b) Limitar rounds a 5.
- **Severidad:** P2 — proporcional al uso del AI agent con archivos grandes.

---

_TD-MEM-19 — RESUELTO 2026-04-18. `window_focused: bool` en App; blink + git poll suspendidos en focus loss; ControlFlow::Wait. Ver archive._

---

_TD-MEM-20, 21 — RESUELTOS 2026-04-17. Ver archivo._

---

### TD-MEM-23: `api_msgs` clona el historial completo en cada round del agent loop
- **Archivo:** `src/app/ui.rs:submit_ai_query()` — `provider.agent_step(api_msgs.clone(), &tool_specs).await`
- **Origen:** kiro
- **Descripción:** El agent loop llama a `agent_step(api_msgs.clone(), ...)` en cada round (hasta 10). `api_msgs` incluye el mensaje de sistema (con todos los archivos adjuntos, hasta 1 MB) más el historial de conversación. Con 10 rounds y archivos adjuntos grandes, son hasta 100 MB de alocaciones transitorias por query.
- **Fix:** Cambiar la firma de `agent_step` para aceptar `&[Value]` en lugar de `Vec<Value>`. El provider construye el body JSON directamente desde la referencia.
- **Severidad:** P2 — hasta 100 MB de alocaciones transitorias por query con archivos grandes.

---

### TD-PERF-04: `scan_files()` síncrono en el hilo principal al abrir el file picker
- **Archivo:** `src/llm/chat_panel.rs` → `open_file_picker()` / `scan_files()`
- **Descripción:** Al abrir el file picker (`Tab`), `scan_files(cwd, depth=3)` bloquea el render. En un monorepo grande puede tomar decenas de ms.
- **Fix:** `tokio::task::spawn_blocking`; spinner mientras carga; enviar resultado por canal.
- **Severidad:** P2 — stutter al abrir el picker en repos grandes.

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

### TD-PERF-15: Clipboard (`arboard`) bloquea el event loop en copy/paste
- **Archivo:** `src/app/mod.rs:703,709`, `src/app/input/mod.rs:481,488`, `src/app/mux.rs:134,136`
- **Descripción:** `arboard::Clipboard::new()` + `.get_text()` / `.set_text()` hacen IPC síncrona al pasteboard de macOS. Para pastes grandes (>1 MB) el event loop se congela durante cientos de ms.
- **Fix:** `tokio::task::spawn_blocking` para operaciones de clipboard. Para paste: spawn task → resultado por canal → escribir al PTY con bracketed-paste. Para copy: fire-and-forget.
- **Severidad:** P2 — jank visible en pastes grandes.

---

### TD-PERF-16: Hash keys de tab bar y status bar se recalculan por frame aunque el resultado esté cacheado
- **Archivo:** `src/app/mod.rs:454-461` (tab_key), `mod.rs:554-568` (sb_key)
- **Descripción:** La cache almacena el resultado final pero la key se recomputa cada frame: `Vec<&[u8]>` + hash de títulos completos. ~20-50 alocaciones por frame solo para el compare.
- **Fix:** Cachear los inputs previos (tupla de valores copiables) y hacer `==` directo antes de llegar al hash. Si bit-idénticos al frame anterior, saltar el hash.
- **Severidad:** P2 — micro pero sistemático en cada frame.

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

### TD-PERF-21: Palette fuzzy matcher re-filtra la lista completa en cada tecla
- **Archivo:** `src/ui/palette/mod.rs:77-79,137-152`
- **Descripción:** Cada keystroke ejecuta `SkimMatcherV2::fuzzy_match()` sobre todos los actions, hace sort y reemplaza el resultado. Sin filtrado incremental ni caché.
- **Fix:** Cachear `last_query` + `last_results`. Si el query nuevo empieza con el viejo (append de char), filtrar `last_results` en lugar del set completo. O(prev_results) en lugar de O(all_actions).
- **Severidad:** P2 — se nota con 500+ actions (plugins en Phase 4).

---

_TD-PERF-22, 31, 32, 33, 34, 37 — RESUELTOS 2026-04-17. Ver archivo._

---

## P3 — Prioridad baja / Backlog

_TD-MEM-14 — RESUELTO 2026-04-19. `sync_channel(1)` + `try_send`; eventos extras descartados silenciosamente. Ver archivo._

---

### TD-MEM-15: `FreeTypeCmapLookup` mantiene `FT_Library` + `FT_Face` por `TextShaper`
- **Archivo:** `src/font/shaper.rs:FreeTypeCmapLookup`
- **Descripción:** Cada `TextShaper` crea una instancia de FreeType. `Drop` los libera correctamente. En la práctica hay un solo `TextShaper` global; impacto mínimo.
- **Fix:** Si en el futuro se crean múltiples shapers, compartir via `Arc<Mutex<...>>`.
- **Severidad:** P3 — impacto mínimo con arquitectura actual.

---

_TD-MEM-16, 17, 18 — RESUELTOS 2026-04-19. Ver archivo._

---

_TD-MEM-22 — RESUELTO 2026-04-19. `bounded(256)` para `ai_tx`/`ai_rx`; `bounded(64)` para `block_tx`/`block_rx`. Ver archivo._

---

_TD-MEM-24 — RESUELTO 2026-04-19. `undo_stack: VecDeque`; `pop_front()` en evicción; `push_back()` en push; `pop_back()` en undo. Ver archivo._

---

_TD-MEM-25 — RESUELTO 2026-04-19. `bounded(1)` en `git_tx`/`git_rx`; send ya ignora error con `let _ =`. Ver archivo._

---

### TD-PERF-03: Upload completo del instance buffer a GPU cada frame
- **Archivo:** `src/app/mod.rs:612` → `src/renderer/gpu.rs:312`
- **Descripción:** En Apple Silicon (M2/M4) con unified memory, `write_buffer` es un memcpy en memoria compartida. ~800 KB a 60 fps = ~48 MB/s frente a 100+ GB/s de bandwidth — 0.05% del bus. **No es cuello de botella real en Apple Silicon.** Relevante solo en Phase 2+ con GPUs discretas.
- **Fix futuro:** Dirty-rect tracking por fila para reducir volumen de upload. Dejar para Phase 2+ (cross-platform).
- **Severidad:** P3 — downgraded desde P1; no es un bottleneck medible en el target hardware actual.

---

_TD-PERF-18 — RESUELTO 2026-04-19. `worker_threads(2)` en tokio builder. Ver archivo._

---

_TD-PERF-23 — RESUELTO 2026-04-19. `leader_timer` → `leader_deadline: Option<Instant>`; set `Instant::now() + Duration::from_millis(timeout_ms)` on activate; check `Instant::now() >= deadline`. Ver archivo._

---

### TD-PERF-24: Separator hit-test rehace geometría en cada `CursorMoved`
- **Archivo:** `src/app/mod.rs:244-268` (`separator_at_pixel`)
- **Descripción:** Cada movimiento del mouse reconstruye `mux.active_pane_separators()` para el hit-test. La geometría ya fue calculada en el frame anterior.
- **Fix:** Cachear `pane_separators_snapshot: Vec<PaneSeparator>` del último render; invalidar solo en resize/split/close.
- **Severidad:** P3 — overhead por movimiento de mouse.

---

### TD-PERF-25: Branch picker blocking con `block_on(list_git_branches)`
- **Archivo:** `src/app/ui.rs:319`
- **Descripción:** Al abrir el branch picker, `block_on(list_git_branches(cwd))` ejecuta `git branch --list` síncronamente. En repos con >1 000 branches puede tomar 100-1 000 ms.
- **Fix:** Abrir el picker inmediatamente con spinner; spawn tokio task; rellenar cuando el resultado llegue.
- **Severidad:** P3 — raro en repos normales; bloqueante en repos con muchas branches.

---

### TD-PERF-26: PTY channel unbounded sin backpressure
- **Archivo:** `src/term/pty.rs:119`
- **Descripción:** El canal entre el reader thread del PTY y el main loop es `unbounded`. Si el main loop no drena por un frame pesado, el productor acumula sin límite. Correcto pero sin señal de backpressure.
- **Fix:** `bounded(256)` con `send_timeout`. Si se llena, loguear `pty_backpressure_hit` y forzar drain.
- **Severidad:** P3 — útil para detectar issues en producción; impacto bajo en condiciones normales.

---

### TD-PERF-28: Log macros con formato evaluado antes del level filter
- **Archivo:** Varios (`src/app/mod.rs:389,294,325` etc.)
- **Descripción:** `log::debug!(...)` evalúa el formato antes del filtro de nivel. En release con `RUST_LOG=info`, los args se leen pero no hay formato pesado. Solo relevante donde el argumento sea costoso de computar (iteradores, strings grandes).
- **Fix:** Caso por caso: `if log::log_enabled!(log::Level::Debug) { log::debug!(...) }` donde el argumento sea caro.
- **Severidad:** P3 — auditoría caso por caso; impacto bajo en release con RUST_LOG=info.

---

### TD-PERF-29: Allocator global por defecto (`std::alloc::System`)
- **Archivo:** `src/main.rs`
- **Descripción:** Alacritty y otros terminales de alto rendimiento usan `jemalloc` o `mimalloc` para mejor comportamiento bajo multi-thread y menor fragmentación.
- **Fix:** Evaluar `mimalloc` como `#[global_allocator]`. Benchmark con `criterion` en `shape_line` + `build_instances` antes/después. Alacritty reporta ~5-15% en sus hot paths.
- **Severidad:** P3 — evaluar con profiling real; no implementar sin benchmarks previos.

---

_TD-PERF-35 — RESUELTO 2026-04-19. `gap_buf: String` en `RenderContext`; `mem::take` + `extend(repeat(' ').take(gap))` + restore. Ver archivo._

---

### TD-MAINT-01: `cargo-audit` no instalado — sin escaneo de CVEs
- **Descripción:** ~40 deps directas, cientos transitivas. Sin escaneo de RustSec no hay alerta de CVEs conocidos.
- **Fix:** `cargo install cargo-audit`; `cargo audit` en CI. Considerar `cargo-deny` para políticas de licencias + advisories.
- **Severidad:** P3 — mantenimiento; bajo riesgo en desarrollo activo, mayor en producción.

---

### TD-PERF-30: Sin infraestructura de profiling ni benchmarks de regresión
- **Archivo:** No existe `benches/` ni crate de tracing.
- **Descripción:** El objetivo "terminal más rápido" no es verificable sin métricas. No hay benchmarks `criterion`, no hay integración con `tracy`/`puffin`, no hay counters expuestos (frame time, atlas fill%, instance count, input-to-pixel latency).
- **Fix:**
  1. `benches/shaping.rs`, `benches/rendering.rs`, `benches/search.rs` con `criterion`. CI falla si regresión >5%.
  2. Feature flag `profiling` con `tracing` + `tracing-tracy`.
  3. HUD debug con `F12`: frame time, shape cache hit%, atlas fill%, instance count, RSS.
  4. Latency probe: key-press → first pixel changed, reportar p50/p95/p99.
- **Severidad:** P3 — sin métricas, cualquier "fix de performance" puede ser regresión disfrazada.

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
