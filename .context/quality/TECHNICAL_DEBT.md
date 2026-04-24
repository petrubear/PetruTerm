# Technical Debt Registry

**Last Updated:** 2026-04-24
**Open Items:** 2
**Critical (P0):** 0 | **P1:** 0 | **P2:** 0 | **P3:** 3

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

_TD-MEM-27 — FALSO POSITIVO 2026-04-23. Kiro lo marcó P0 pero el código ya tiene límites: `MAX_MESSAGES=200` en `chat_panel.rs:9`, `mark_done()` drena `messages` y `wrapped_cache` en sync (`chat_panel.rs:317-322`), `streaming_buf.clear()` en `mark_done()` y `close()`. No hay leak real._

---

## P1 — Alta prioridad

_Ninguno abierto. Todos los P1 cerrados 2026-04-16 (TD-RENDER-01/02/03, TD-PERF-36). Ver archive._

---

## P2 — Prioridad media

_TD-RENDER-04 — RESUELTO 2026-04-23. El fringe LCD bajo selección ya no ocurre: `collect_grid_cells_for` (mux.rs:612-616) aplica la inversión fg/bg antes de construir vértices, así los vértices LCD reciben el `bg` correcto. `calculate_row_hash` incluye colores → cache invalida al activar selección. `can_skip=false` cuando `sel_range.is_some()` → damage-skip no interfiere. `selection_bg`/`selection_fg` del config existen pero no se usan (inversión simple siempre), esto es un feature gap menor sin artefacto visual._

### TD-PERF-38: PTY buffer overflow sin backpressure efectivo — RESUELTO 2026-04-23
Buffer `crossbeam_channel::bounded` aumentado de 256 → 1024 en `src/term/pty.rs:141`.
Reduce drásticamente los `pty_backpressure_hit` en comandos de salida masiva (`cat`, `ls -R`).
Backpressure real (pausa/resume signals al proceso hijo) diferido como mejora futura.

---

_TD-MEM-09 — RESUELTO 2026-04-19. Default reducido de 10 000 a 5 000 líneas (`schema.rs:74`, `perf.lua`). Comentario en perf.lua documenta impacto (~1 MB/pane, ~20 MB con 20 panes). Límite global diferido al backlog (requiere coordinación entre terminales)._

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

_TD-MEM-26 — RESUELTO 2026-04-23. `FreeTypeLcdRasterizer::new()` en `freetype_lcd.rs`: reemplazado `load_face_from_file()?` por `match` explícito que llama `FT_Done_FreeType(library)` antes de propagar el error. `FreeTypeCmapLookup::new()` en `shaper.rs` ya tenía todos los paths correctos._

---

_TD-MEM-27 — ABIERTO 2026-04-23. LLM chat panel sin límites de memoria activa. Ver auditoría 2026-04-23._

---

_TD-PERF-04 — RESUELTO 2026-04-19. `open_file_picker_async` usa `std::thread::spawn` + `crossbeam_channel::bounded(1)`; `poll_file_scan` drena sin bloquear._

---

_TD-PERF-05: Atlas de glifos siempre 64 MB de VRAM desde arranque
- **Archivo:** `src/renderer/atlas.rs:GlyphAtlas::new()`
- **Descripción:** Textura RGBA 4096×4096 = 64 MB de VRAM al arranque. Menos crítico en Apple Silicon (unified memory); importante para Phase 2+ con GPUs discretas.
- **Fix:** Empezar con 1024×1024 = 4 MB y crecer dinámicamente. Requiere recrear textura + re-subir glifos calientes.
- **Severidad:** P2 — **DIFERIDO a Phase 2** (cross-platform). Sin impacto medible en Apple Silicon unified memory.

---

_TD-PERF-14 — RESUELTO 2026-04-19. `build_scroll_bar_instances` reemplazado: loop de N `CellVertex` → 2 `CellVertex` (track + thumb). El shader `vs_bg` con `FLAG_CURSOR` usa `glyph_size` en píxeles para el rect completo; thumb se dibuja encima del track en orden painter's. Eliminadas hasta 60 instancias por frame._

---

_TD-PERF-15 — RESUELTO 2026-04-19. Cmd+C/V ya eran async (thread::spawn). ClipboardStore/Load (OSC 52) en `mux.rs` migrados a thread::spawn; ClipboardLoad reinyecta resultado como PtyWrite via `Pty.tx` (nuevo campo `pub tx: Sender<PtyEvent>` en `pty.rs`)._

---

_TD-PERF-16 — RESUELTO 2026-04-19. Tab bar ahora cachea inputs copiables (`active_index`, `total_cols`, `titles`, `rename_input`) y compara directamente sin hash. Eliminadas 20-50 alocaciones Vec por frame. Status bar sigue con hash (menos frecuente). Commit `67a340a`._

---

_TD-PERF-17 — RESUELTO 2026-04-17. Ver archivo._

---

_TD-PERF-19 — RESUELTO 2026-04-19. `git_branch_in_flight: bool` en `ui.rs`; guard activo al spawn, desactivado al recibir resultado. Timeout 30 s de recuperación. Ver commits `9fd235a`, `a8867a7`, `d1daa80`._

---

_TD-PERF-20 — RESUELTO 2026-04-17. Ver archivo._

---

_TD-PERF-21 — RESUELTO 2026-04-19. `last_filter_query` + path incremental en `filter()` (`palette/mod.rs:157-185`). O(prev_results) cuando query extiende el anterior._

---

_TD-PERF-22, 31, 32, 33, 34, 37 — RESUELTOS 2026-04-17. Ver archivo._

---

_TD-PERF-38 — RESUELTO 2026-04-23. `bounded::<PtyEvent>(256)` → `bounded::<PtyEvent>(1024)` en `pty.rs:141`. Reduce `pty_backpressure_hit` en salida masiva. Backpressure real (pause/resume signals) diferido._

---

## P3 — Prioridad baja / Backlog

_TD-GPU-01 — RESUELTO 2026-04-24. `GpuRenderer::new()` ahora lee `config.gpu_preference` (enum `LowPower/HighPerformance/None`) en lugar de hardcodear `PowerPreference::HighPerformance`. Default cambiado a `"low_power"` en `perf.lua`. Present mode cambia automáticamente a `Fifo` al activar battery saver y vuelve a `Mailbox`/`FifoRelaxed` en AC (runtime, sin reinicio)._

---

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

_TD-MEM-28 — FALSO POSITIVO 2026-04-23. El `cache` en `FreeTypeLcdRasterizer` almacena solo UV pointers (16 bytes/entry), no glyph data. El atlas real (`LcdGlyphAtlas`) tiene `evict_cold()` y `clear()`. `clear_local_cache()` existe en `freetype_lcd.rs`. No hay leak._

_TD-MEM-29 — FALSO POSITIVO 2026-04-23. Buffer GPU fijo de 2.5 MB por diseño, no heap leak. Overflow manejado explícitamente en `gpu.rs` con bounds check y bail. No es deuda tecnica._

### TD-MEM-30: Bytecode cache de Lua no limpiado
- **Archivos:** `src/config/lua.rs`
- **Descripción:** El bytecode cache se almacena en `~/.cache/petruterm/lua-bc/` pero nunca se limpia. Con cada actualización de la app, los archivos `.luac` antiguos se acumulan.
- **Root cause:** `load_or_compile_config` escribe cache pero no hay mecanismo para eliminar archivos `.luac` no modificados en los últimos 30 días.
- **Impacto:** Uso gradual de espacio en disco (~10-50 KB por archivo, acumulación a largo plazo).
- **Fix propuesto:** Implementar una política de limpieza que elimine archivos `.luac` no modificados en los últimos 30 días. Opcional: verificar versión del binario antes de usar cache.
- **Severidad:** P3 — espacio en disco no es crítico, pero limpieza es buena práctica.
- **Auditoría:** 2026-04-23

_TD-PERF-39 — FALSO POSITIVO 2026-04-23. `text.split(' ')` es un iterador lazy, sin allocaciones. `try_word_cached_shape` almacena `&str` refs, no `String`. La clave de `word_cache` es un hash (u64), no copia del string. `word_cache` es `LruCache(1024)` con capacidad fija._

### TD-PERF-40: Vec no reutilizados en render loop
- **Archivos:** `src/app/renderer.rs` (RenderContext)
- **Descripción:** Aunque hay `cell_data_scratch`, `scratch_chars`, `scratch_str`, etc., algunos métodos como `build_all_pane_instances()` crean nuevos `Vec` en cada frame para `pane_infos`.
- **Root cause:** `pane_infos` se construye en `build_all_pane_instances` llamando a `active_pane_infos` que puede allocar nuevos Vecs.
- **Impacto:** Allocation overhead en cada frame, especialmente problemático a 120 FPS.
- **Fix propuesto:** Mover `pane_infos` a `RenderContext` como buffer reutilizable. Opcional: usar `SmallVec` para paneles pequeños (≤ 4 paneles).
- **Severidad:** P3 — allocation barata en Rust, pero evitable.
- **Auditoría:** 2026-04-23

---

## Resumen de auditoría 2026-04-23

| ID | Prioridad | Descripción | Estado |
|----|-----------|-------------|--------|
| TD-MEM-26 | P0 | FreeType memory leaks | RESUELTO |
| TD-MEM-27 | P0 | LLM chat panel sin límites | FALSO POSITIVO |
| TD-PERF-38 | P2 | PTY buffer overflow sin backpressure | RESUELTO |
| TD-MEM-28 | P3 | FreeTypeLcdRasterizer cache sin límite | FALSO POSITIVO |
| TD-MEM-29 | P3 | CellVertex overhead | FALSO POSITIVO |
| TD-MEM-30 | P3 | Bytecode cache Lua no limpiado | ABIERTO |
| TD-PERF-39 | P3 | String allocation hot path | FALSO POSITIVO |
| TD-PERF-40 | P3 | Vec no reutilizados | ABIERTO |
| TD-MCP-01  | P3 | MCP config no hot-reloadable | ABIERTO |

---

_TD-MCP-01 — RESUELTO 2026-04-24. Hot-reload de `mcp.json` implementado en D-5: `config/watcher.rs` extiende filtro a `.json`; `app/mod.rs` añade `mcp_watcher` (notify sobre `.petruterm/`) + `mcp_reload_at` debounce; `app/ui.rs` `reload_mcp(cwd)` reemplaza el `Arc<McpManager>` en caliente._

---

## Recomendaciones generales (no-issues, direcciones estratégicas)

### REC-PERF-01: Pre-shape ASCII range al arranque — RESUELTO 2026-04-24
`init_ascii_glyph_cache()` ahora se llama eagerly en `TextShaper::new()` (no lazy).
`warmup_atlas()` rasteriza los 95 glyphs ASCII imprimibles al atlas en `RenderContext::new()`
justo después de `set_cell_size`, eliminando cache-misses en el primer frame renderizado.
"Never evict" diferido — requeriría nuevo flag en `GlyphAtlas`; el atlas de 64 MB es suficientemente
grande para que los ASCII no sean eviccionados en la práctica.

### REC-PERF-02: `parking_lot::Mutex` en lugar de `std::sync::Mutex` — RESUELTO 2026-04-24
`parking_lot = "0.12"` añadido a `Cargo.toml`. Reemplazado en los 3 sitios donde se usaba
`std::sync::Mutex`: `font/freetype_lcd.rs`, `font/loader.rs` (FONT_PATH_CACHE estático),
`llm/copilot.rs` (JWT cache). Ningún lock se mantiene a través de `.await`, seguro en los 3 casos.
`.lock().unwrap()` → `.lock()` (parking_lot no devuelve Result).

### REC-PERF-03: Damage tracking de alacritty_terminal — RESUELTO 2026-04-18
`collect_grid_cells_for` integra `TermDamage` API. Filas no dañadas se saltan
cuando no hay selection/search activo. Ver commit `2c945fe`.

### REC-PERF-04: Medir antes de optimizar
**Ningún fix P2/P3 debe implementarse sin profiling previo**. Instalar TD-PERF-30 primero. Algunos items pueden resultar irrelevantes en la práctica; otros no detectados aquí pueden ser los verdaderos cuellos de botella.

### REC-PERF-05: Frame budget explícito — RESUELTO 2026-04-24
Documentado en `.context/specs/term_specs.md` §15 con tabla de targets, metodología de medición,
referencia al HUD F12, y descripción del CI criterion gate.

### REC-PERF-06: Criterion CI gating — RESUELTO (ya implementado)
`benches/` con shaping/search/build_instances. CI regression gate con critcmp >5%. Ver ci.yml.
