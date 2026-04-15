# Technical Debt Registry

**Last Updated:** 2026-04-10
**Open Items:** 30
**Critical (P0):** 0 | **P1:** 5 | **P2:** 16 | **P3:** 9

> Resolved items are in [TECHNICAL_DEBT_archive.md](./TECHNICAL_DEBT_archive.md).

## Priority Definitions

| Priority | Definition | SLA |
|----------|------------|-----|
| P0 | Blocking development or causing incidents | Immediate |
| P1 | Significant impact on velocity or correctness | This sprint |
| P2 | Moderate impact, workaround exists | This quarter |
| P3 | Minor, address when convenient | Backlog |

---

## Auditoría de Performance — 2026-04-10

Sesión de auditoría completa con objetivo declarado: **hacer de PetruTerm el terminal más rápido del mercado**. Los findings se agrupan por área (renderer / hot path no-renderer) y se priorizan por impacto medido en el frame budget de 16.6 ms (60 fps) y 8.3 ms (120 ps). Cada item trazable a `archivo:línea` y verificado leyendo el código fuente.

**Metodología:** lectura estática del hot path (`app::mod::RedrawRequested`, `app::renderer`, `renderer::gpu`, `font::shaper`, `renderer::atlas`, `app::input`, `term::pty`, `app::mux`) más cruces con `user_event`, `about_to_wait` y las rutas de I/O síncrona. No se ejecutó profiling dinámico (Instruments, tracy) — ver TD-PERF-30 para la recomendación.

---

## P1 — Alta prioridad (hot path, impacto medible)

### TD-PERF-03: Upload completo del instance buffer a GPU cada frame
- **Archivo:** `src/app/mod.rs:612` → `src/renderer/gpu.rs:312`
- **Nota:** En Apple Silicon (M2/M4) con unified memory, `write_buffer` es un memcpy en memoria compartida. ~800 KB a 60 fps = ~48 MB/s frente a 100+ GB/s de bandwidth — 0.05% del bus. **No es cuello de botella real hoy.** Sería relevante en GPUs discretas con PCIe.
- **Fix futuro:** Dirty-rect tracking por fila para reducir volumen de upload. Dejar para Phase 2+ (cross-platform).

---

### TD-PERF-06: Doble rasterización de glifos cuando LCD AA está habilitado
- **Archivo:** `src/app/renderer.rs:195-204`
- **Descripción:** En `build_instances`, para cada glifo shaped se llama primero a `rasterize_lcd_to_atlas` (línea 197) y a continuación, **incondicionalmente**, a `rasterize_to_atlas` (línea 203). Cuando el glifo ya existe en el atlas LCD, el resultado de `rasterize_to_atlas` se descarta en la rama `if lcd_entry.is_none()` (línea 205-222), pero la rasterización y el upload al atlas sRGB ya ocurrieron. Cada glifo único paga 2× el costo de rasterización + upload al atlas + ocupa espacio en dos atlas distintos.
- **Impacto:** En un cache-miss storm (tras `evict_cold` o cambio de font), se duplica el trabajo de rasterización — visiblemente peor cuando se combina con TD-PERF-07.
- **Fix:** Intercambiar el orden: intentar `rasterize_lcd_to_atlas` primero y, si devuelve `Some`, saltar la llamada a `rasterize_to_atlas` completamente. Solo rasterizar al atlas sRGB como fallback cuando el path LCD falle (glifo de emoji, fuente sin curvas, etc.). Ajustar la lógica que hoy consume `swash_entry.is_color` para el flag `FLAG_COLOR_GLYPH` — emoji siempre va por el path sRGB.
- **Severidad:** P1 — en el hot path de cache-miss, el 50% del trabajo es desperdicio.

---

### TD-PERF-07: Invalidación global de row caches al evictar el atlas
- **Archivo:** `src/app/mod.rs:381-391`
- **Descripción:** Cada frame se llama `rc.renderer.atlas.next_epoch()` y, si el atlas supera el 90% de ocupación, `evict_cold(60)` devuelve el número de glifos expulsados. Cuando es `>0`, se ejecuta `rc.clear_all_row_caches()` que vacía el `HashMap<terminal_id, RowCache>` completo — **todas** las filas de **todos** los paneles pierden su cache. El siguiente frame debe re-shapear y re-rasterizar el contenido visible entero, causando un pico de decenas de ms (jank visible).
- **Peor caso:** un `cat archivo_grande` o `less` saturando el atlas con glifos frescos dispara evicción cada N frames → stutter periódico.
- **Fix:** Invalidación selectiva. El atlas debe devolver los `cache_key`s evictados (`Vec<CacheKey>`), y el row cache debe mantener un índice inverso `cache_key → set<(terminal_id, row)>` para invalidar solo las filas afectadas. Alternativa más simple: mantener un `atlas_generation: u64` por fila y, en `evict_cold`, incrementar una generación global; filas con generación distinta se revalidan perezosamente.
- **Severidad:** P1 — stutter visible en escenarios de uso real.

---

### TD-PERF-08: `PresentMode::Fifo` bloquea el frame al vsync con latencia máxima de 2 frames
- **Archivo:** `src/renderer/gpu.rs:126`
- **Descripción:** El surface se configura con `present_mode: wgpu::PresentMode::Fifo` y `desired_maximum_frame_latency: 2`. Fifo es sin tearing pero bloquea `get_current_texture()` hasta el próximo vblank; con 2 frames de latencia máxima, el input-to-pixel worst-case es ~33 ms a 60 Hz. Para el objetivo "terminal más rápido" esto es el techo duro de latencia.
- **Benchmark de referencia:** Alacritty + kitty usan `PresentMode::Mailbox` o equivalente por defecto para minimizar latencia.
- **Fix:** (a) Cambiar a `Mailbox` cuando esté disponible en `caps.present_modes`, con fallback a `Fifo`. (b) Reducir `desired_maximum_frame_latency` a 1 (hay un hit de performance si el GPU va justo, pero reduce latencia a la mitad). (c) Exponer como `config.performance.present_mode` en Lua (`"lowest_latency" | "smooth" | "power_save"`).
- **Severidad:** P1 — afecta métrica declarada del producto (latencia de tecleo).

---

### TD-PERF-09: Lectura síncrona de disco del shell context por cada evento PTY
- **Archivo:** `src/app/mod.rs:86-95, 303, 352` → `src/llm/shell_context.rs:43-53`
- **Descripción:** `update_terminal_shell_ctx(id)` llama a `ShellContext::load_for_pid(pid)`, que ejecuta `std::fs::read_to_string(~/.cache/petruterm/shell-context-{pid}.json)` + `serde_json::from_str`. Esto se dispara:
  1. En `user_event` (línea 303) — por cada batch de datos PTY entrante.
  2. En `RedrawRequested` (línea 352) — redundantemente, por cada frame que también proviene de actividad PTY.
  Sin mtime cache: se relee el archivo aunque el shell no haya escrito nada nuevo. Para TUIs de alta frecuencia (vim/tmux/htop/`watch`), son decenas de syscalls `open`+`read`+`close` por segundo **en el hilo del event loop**.
- **Fix:** Cache con mtime guard en `terminal_shell_ctxs`:
  ```rust
  struct CachedCtx { ctx: ShellContext, mtime: SystemTime }
  ```
  Antes de `read_to_string`, hacer `metadata()?.modified()?` y comparar con el mtime cacheado; si coincide, reutilizar. Más: deduplicar las dos llamadas por frame — una sola en `user_event` es suficiente si `RedrawRequested` se dispara tras ese evento.
- **Severidad:** P1 — syscalls en el hilo del render loop con frecuencia proporcional a la velocidad del TUI activo.

---

## P2 — Prioridad media

### TD-PERF-04: `scan_files()` síncrono en el hilo principal al abrir el file picker
- **Archivo:** `src/llm/chat_panel.rs` → `open_file_picker()` / `scan_files()`
- **Descripción:** Al abrir el file picker (`Tab`), se llama `scan_files(cwd, depth=3)` síncronamente en el event loop. En un monorepo grande bloquea el render durante decenas de ms.
- **Fix:** Mover a `tokio::task::spawn_blocking`, enviar resultado por canal, mostrar spinner mientras carga.

---

### TD-PERF-05: Atlas de glifos siempre 64 MB de VRAM desde el arranque
- **Archivo:** `src/renderer/atlas.rs` → `GlyphAtlas::new()`
- **Descripción:** Textura RGBA de 4096×4096 = 64 MB de VRAM al arranque, aunque la mayoría nunca se use. Menos relevante en Apple Silicon (unified memory); importante en Phase 2+ con GPUs discretas.
- **Fix:** Empezar con 1024×1024 = 4 MB y crecer dinámicamente al 4096×4096 cuando se acerque al límite. Requiere recrear textura + re-subir glifos calientes.

---

### TD-PERF-10: Cursor blink invalida el cache entero del panel de chat
- **Archivo:** `src/app/mod.rs:964-974` (cursor blink toggle) → `src/app/renderer.rs:707-709`
- **Descripción:** Cada 530 ms, `update_cursor_blink()` devuelve `true` y se marca `panel.dirty = true`. Eso dispara una reconstrucción completa de `panel_instances_cache` en el siguiente `RedrawRequested`: re-word-wrap, re-shape, re-rasterize de todos los mensajes del historial aunque el contenido no haya cambiado. Para una conversación de 50 mensajes, son 50 invocaciones a `shape_line` (HarfBuzz) + N word-wrap passes ~2 veces por segundo sin razón.
- **Fix:** Renderizar el cursor del input del panel como un `RoundedRectInstance` overlay independiente, controlado por un buffer separado que se actualiza sin invalidar el cache de texto. El toggle de blink solo debe tocar 1 rect, no la `dirty` flag global del panel.
- **Severidad:** P2 — alto consumo de CPU mientras el panel de chat está abierto con focus.

---

### TD-PERF-11: Text search re-escanea el grid entero en cada tecla
- **Archivo:** `src/app/mux.rs:389-424` (`search_active_terminal`), llamado desde `src/app/mod.rs:403`
- **Descripción:** Por cada keystroke en la barra de búsqueda se recorren `(-history)..screen_rows` filas (típicamente 10 000 con scrollback), y por cada fila se construye un `Vec<char>` de todas las columnas con `to_lowercase()` aplicado por celda (~80 columnas). Esto son ~800 K operaciones char-lowercase + alocaciones por keystroke, síncronas en el event loop, sin caché entre queries similares.
- **Fix:** (a) Precomputar una vez la representación lowercase del grid en un `Vec<Vec<char>>` cacheado, invalidado solo cuando el grid cambia (usar el `damage` de alacritty_terminal). (b) Búsqueda incremental: cuando el query nuevo extiende el anterior (`new.starts_with(old)`), filtrar solo los matches previos en lugar de re-escanear. (c) Cache LRU `HashMap<String, Vec<SearchMatch>>` con capacidad máxima de 64 queries.
- **Severidad:** P2 — se nota como lag al tipear rápido en la barra de búsqueda.

---

### TD-PERF-12: Allocaciones repetidas en `push_shaped_row` para cada fila de overlay UI
- **Archivo:** `src/app/renderer.rs:343-424` (concretamente líneas 374, 376-379, 381)
- **Descripción:** `push_shaped_row` se invoca ~40 veces por frame dirty de chat panel / status bar / AI block. Cada invocación aloca: `Vec<char>` (línea 374), `String` padded (línea 376-379), y `Vec<([f32;4],[f32;4])>` colors (línea 381). Son ~120 allocaciones/heap frees por reconstrucción de panel.
- **Fix:** Mover los tres buffers a `RenderContext` como `scratch_chars: Vec<char>`, `scratch_padded: String`, `scratch_colors: Vec<(...)>`. `clear()` al inicio de cada `push_shaped_row` y reusar la capacidad.
- **Severidad:** P2 — alocaciones en cascada presionan el allocator y thrashean L1/L2.

---

### TD-PERF-13: `format!` spam en `build_chat_panel_instances` (~40+ llamadas por rebuild)
- **Archivo:** `src/app/renderer.rs:488, 496, 540-541, 578, 601-606, 634, 643, 648, 655, 666, 684, 701-702, 722-734, 751-754, 811, 819, 824, 831`
- **Descripción:** Cada rebuild dirty del chat panel genera decenas de strings temporales vía `format!`: prefijos de rol, paths truncados, separadores con `"─".repeat(n)`, mensajes de status, hints, tokens. Todos se descartan inmediatamente después del shape.
- **Fix:** Un `String` scratch compartido en `RenderContext`, limpio al inicio de cada push (`buf.clear(); write!(buf, "...")`). Separadores (`"─".repeat(w)`) se cachean al tamaño en el propio `ChatPanel` y solo se regeneran en resize.
- **Severidad:** P2 — contribuye a la presión del allocator junto con TD-PERF-12.

---

### TD-PERF-14: Scroll bar construido como `N` `CellVertex` (uno por fila)
- **Archivo:** `src/app/renderer.rs:1219-1230`
- **Descripción:** El scroll bar emite `screen_rows` instancias `CellVertex` — hasta 60 instancias para un terminal de 60 filas. Es semánticamente incorrecto: se trata de 2 rectángulos (track + thumb) que podrían representarse como 2 `RoundedRectInstance`. El cache existente (`scroll_bar_cache`) mitiga el costo de reconstrucción pero el buffer GPU sigue llevando 60 vértices por scroll bar.
- **Fix:** Migrar `build_scroll_bar_instances` a empujar a `rect_instances` (2 rects: track completo + thumb). Elimina el upload de 60 `CellVertex` por frame, reduce trabajo del glyph pipeline.
- **Severidad:** P2 — impacto bajo por el cache, pero simplifica el pipeline.

---

### TD-PERF-15: Clipboard (`arboard`) bloquea el event loop en copy/paste
- **Archivo:** `src/app/mod.rs:703, 709` (context menu) → `src/app/input/mod.rs:481, 488` (Cmd+C/V) → `src/app/mux.rs:134, 136`
- **Descripción:** `arboard::Clipboard::new()` + `.get_text()` / `.set_text()` hacen IPC síncrona al pasteboard server de macOS. Para pastes grandes (>1 MB, ej. pegar un log) el event loop se congela durante cientos de ms, perdiendo frames y eventos de input.
- **Fix:** Mover las operaciones de clipboard a `tokio::task::spawn_blocking`. Para paste, el flujo queda: (1) spawn task, (2) esperar resultado vía canal, (3) en el callback, escribir al PTY con bracketed-paste wrapping. Para copy, fire-and-forget suficiente.
- **Severidad:** P2 — jank visible en pegas grandes.

---

### TD-PERF-16: Hash keys de tab bar y status bar se recalculan por frame aunque el resultado esté cacheado
- **Archivo:** `src/app/mod.rs:454-461` (tab_key), `mod.rs:554-568` (sb_key)
- **Descripción:** Los caches de tab bar y status bar almacenan el resultado final (`tab_bar_instances_cache`, `status_bar_instances_cache`) pero la **key** se recomputa cada frame: build de `Vec<&[u8]>` con múltiples slots, hash de títulos completos, etc. Son ~20-50 allocaciones por frame solo para el key compare.
- **Fix:** Cachear los inputs previos (tupla de valores copiables) y hacer un `==` directo antes de llegar a computar el hash. Si todos los inputs son bit-idénticos al frame anterior, saltar el hash por completo.
- **Severidad:** P2 — micro pero sistemático en cada frame.

---

### TD-PERF-17: Config hot-reload sin debounce
- **Archivo:** `src/config/watcher.rs` → `src/app/mod.rs:219-233` (`check_config_reload`)
- **Descripción:** Muchos editores (VS Code, Neovim con `atomic_save`) escriben el archivo como "write temp → rename → touch", generando 2-3 eventos `notify` en ms. Cada evento dispara un reparse completo del `.lua` vía `mlua` + validación de schema + rebuild de palette + potencial invalidación de atlas.
- **Fix:** Debounce de 300 ms. En el primer evento, arma un `Instant` futuro; eventos subsecuentes reinician el timer; el reload real dispara cuando el timer expira sin más eventos.
- **Severidad:** P2 — reloads duplicados durante edición activa del config.

---

### TD-PERF-18: Tokio runtime con pool de threads por defecto (num_cpus)
- **Archivo:** `src/app/ui.rs:93-96`
- **Descripción:** `tokio::runtime::Builder::new_multi_thread().enable_all().build()` crea un worker pool del tamaño `num_cpus::get()` — típicamente 8-16 workers en una M4. Para PetruTerm, las tareas async son: (a) requests HTTP al LLM (raras), (b) `git branch` async (1/5s), (c) futuras lecturas de archivos. Todas son I/O-bound, no CPU-bound. Un pool grande = context switches innecesarios + memory overhead (~2 MB de stack por worker).
- **Fix:** `Builder::new_multi_thread().worker_threads(2).enable_all().build()`. Alternativa: `new_current_thread()` + `spawn_blocking` para el handful de llamadas bloqueantes. Menos complejidad, latencia más predecible.
- **Severidad:** P2 — overhead constante de memoria + scheduling noise.

---

### TD-PERF-19: `poll_git_branch` no guarda flag de fetch en curso
- **Archivo:** `src/app/ui.rs:265-293`
- **Descripción:** Si un `git branch --show-current` tarda >5 s (filesystem lento, NFS montado, repo enorme), el siguiente frame cumplirá el TTL y disparará otro spawn, encadenando tareas tokio redundantes. Sin guard.
- **Fix:** Añadir `git_branch_in_flight: bool` que se activa al spawn y se desactiva al recibir el resultado. Solo spawnear si `!in_flight && (cwd_changed || ttl_expired)`.
- **Severidad:** P2 — raro pero genera acumulación de tasks en casos patológicos.

---

### TD-PERF-20: `chars().count()` en hot paths para spinners y truncación
- **Archivo:** `src/app/renderer.rs:464` (spinner), `662, 663, 754` (truncación)
- **Descripción:** El spinner de loading calcula su índice con `panel.streaming_buf.chars().count() % 8` — walk O(n) del string entero cada frame solo para un animation frame. La truncación de paths/hints hace `chars().take(N).collect::<String>()` para cortar a N chars, alocando un string nuevo.
- **Fix:** (a) Spinner: usar un `frame_counter: u64` en `RenderContext` incrementado en `about_to_wait`, índice = `(frame_counter / 4) % 8`. (b) Truncación: `char_indices().nth(N).map(|(i, _)| &s[..i]).unwrap_or(s)` — cero alocación, O(N) en chars, no en bytes totales.
- **Severidad:** P2 — micro pero se suma con TD-PERF-12/13.

---

### TD-PERF-21: Palette fuzzy matcher re-filtra la lista completa en cada tecla
- **Archivo:** `src/ui/palette/mod.rs:77-79, 137-152` (`type_char` / `filter`)
- **Descripción:** Cada keystroke en el palette ejecuta `SkimMatcherV2::fuzzy_match()` sobre los ~100+ actions completos, hace sort y reemplaza el resultado. No hay filtrado incremental ni caché.
- **Fix:** Cachear `last_query` + `last_results`. Si el query nuevo empieza con el viejo (append de char), filtrar `last_results` en lugar del set completo. Reduce el trabajo de O(all_actions) a O(prev_results).
- **Severidad:** P2 — se nota con 500+ actions (plugins en Phase 4).

---

### TD-PERF-22: Highlight de search lookup O(matches) por celda en el render
- **Archivo:** `src/app/mux.rs:441-454` (`search_highlight_at`), llamado en el loop de `collect_grid_cells_for`
- **Descripción:** Durante el render del grid, para cada celda visible se recorre el `Vec<SearchMatch>` entero para detectar si está dentro de un match (línea 447). Con 100 matches × 3200 celdas (80×40), son 320 000 comparaciones por frame.
- **Fix:** Preconstruir un `HashMap<i32, Vec<(col_start, col_end, match_idx)>>` indexado por `grid_line` una sola vez cuando los matches cambian. Lookup O(1) por celda.
- **Severidad:** P2 — solo activo con search visible, pero degrada frame rate cuando activo.

---

## P3 — Prioridad baja / micro-opts

### TD-PERF-23: Leader key timeout con `Instant::elapsed()` por keystroke
- **Archivo:** `src/app/input/mod.rs:159-163`
- **Descripción:** Cada keystroke evalúa `if t.elapsed() > timeout_ms` — `elapsed()` llama a `SystemTime::now()` (syscall en macOS). Acumula overhead durante typing rápido.
- **Fix:** Almacenar `leader_deadline: Instant` una sola vez al activar el leader y comparar con `Instant::now() >= leader_deadline` solo cuando se necesite.

---

### TD-PERF-24: Separator hit-test rehace geometría en cada `CursorMoved`
- **Archivo:** `src/app/mod.rs:244-268` (`separator_at_pixel`)
- **Descripción:** Cada movimiento del mouse reconstruye `mux.active_pane_separators()` para el hit-test. La geometría ya fue calculada en el frame anterior y está disponible en `RenderContext`.
- **Fix:** Cachear `pane_separators_snapshot: Vec<PaneSeparator>` del último render; invalidar solo en resize / split / close.

---

### TD-PERF-25: Branch picker blocking con `block_on(list_git_branches)`
- **Archivo:** `src/app/ui.rs:319` (`open_branch_picker`)
- **Descripción:** Al abrir el branch picker, `block_on(list_git_branches(cwd))` ejecuta `git branch --list` síncronamente en el event loop. En repos grandes (>1000 branches) stallea 100-1000 ms.
- **Fix:** Abrir el picker inmediatamente con un spinner, spawn tokio task, rellenar cuando el resultado llegue.

---

### TD-PERF-26: PTY channel unbounded sin backpressure
- **Archivo:** `src/term/pty.rs:119` (`crossbeam_channel::unbounded`)
- **Descripción:** El canal entre el reader thread del PTY y el main loop es `unbounded`. Si el main loop no drena por un frame (evento pesado), el productor acumula sin límite. Correcto pero sin señal de backpressure.
- **Fix:** Usar `bounded(256)` con `send_timeout`. Si se llena, loguear warning (`pty_backpressure_hit`) y forzar un drain inmediato. Útil para detectar issues en producción.

---

### TD-PERF-27: Profile release sin `target-cpu=native` para builds locales
- **Archivo:** `Cargo.toml:89-94`
- **Descripción:** `[profile.release]` tiene `lto = true`, `codegen-units = 1`, `strip = true` — excelente para binarios distribuidos. Pero no hay un perfil `release-native` con `target-cpu=native` que habilite SIMD específico del host (AVX-512/NEON avanzado) para los dev builds locales donde no importa la portabilidad.
- **Fix:** Añadir `.cargo/config.toml` con un alias o un `[profile.release-native]` que herede de release y exporte `rustflags = ["-C", "target-cpu=native"]`. Usar para benchmarks locales.

---

### TD-PERF-28: Log macros con formato evaluado antes del level filter
- **Archivo:** varios (`src/app/mod.rs:389, 294, 325` etc.)
- **Descripción:** Código tipo `log::debug!("Atlas eviction: removed {} stale glyphs", evicted)` — el formato se evalúa antes del filtro de nivel. En release con `RUST_LOG=info`, `evicted` aún es leído pero no hay format heavy. Donde sí habría problema es con strings grandes o iteradores costosos pasados como args.
- **Fix:** Donde el formato sea caro, envolver en `if log::log_enabled!(log::Level::Debug) { log::debug!(...) }`. Auditoría caso por caso cuando se detecte.

---

### TD-PERF-29: Allocator global por defecto (`std::alloc::System`)
- **Archivo:** `src/main.rs` (no se sobreescribe)
- **Descripción:** Rust usa el allocator del sistema por defecto. Alacritty y otros terminales de alto rendimiento usan `jemalloc` o `mimalloc` por mejor comportamiento bajo multi-thread y menor fragmentación. Las allocaciones en el render path (ver TD-PERF-12, -13) presionan el allocator.
- **Fix:** Evaluar `mimalloc` como `#[global_allocator]`. Benchmark antes/después con `criterion` en `shape_line` + `build_instances`. Alacritty reporta ~5-15% en sus hot paths.

---

### TD-MAINT-01: `cargo-audit` no instalado — sin escaneo de CVEs en dependencias
- **Descripción:** ~40 deps directas, cientos transitivas. Sin escaneo de RustSec no hay alerta de CVEs conocidos.
- **Fix:** `cargo install cargo-audit`; añadir `cargo audit` a CI. Considerar `cargo-deny` para políticas de licencias + advisories.

---

### TD-PERF-30: Sin infraestructura de profiling ni benchmarks de regresión
- **Archivo:** No existe `benches/` ni crate de tracing.
- **Descripción:** El objetivo "terminal más rápido" no es verificable sin métricas. Hoy:
  - No hay benchmarks `criterion` para `shape_line`, `build_instances`, `search_active_terminal`.
  - No hay integración con `tracy` o `puffin` para ver flamegraphs en vivo.
  - No hay counters expuestos (frame time, shape cache hit rate, atlas fill %, instance count).
  - No hay medición de input-to-pixel latency (el KPI principal del producto).
- **Fix (multiparte):**
  1. **Microbenchmarks `criterion`:** `benches/shaping.rs`, `benches/rendering.rs`, `benches/search.rs`. CI falla si regresión > 5%.
  2. **Profiling en vivo:** integrar `tracing` + `tracing-tracy` bajo feature flag `profiling`. `tracy` client overlay muestra frame time + spans.
  3. **HUD de debug:** tecla `F12` muestra overlay con: last frame time, shape cache hit%, atlas fill%, instance count, memory RSS. Baseline para discusión.
  4. **Latency probe:** instrumentar el path key-press → first pixel changed con timestamps. Reportar p50/p95/p99 cada N segundos.
  5. **Profiling session checklist:** documento en `.context/quality/PROFILING.md` con recetas para Instruments.app (Time Profiler + Metal System Trace) y `cargo flamegraph`.

---

## Recomendaciones generales (no-issues, direcciones estratégicas)

Estas no son items de deuda sino direcciones a considerar al atacar la deuda anterior. Se dejan documentadas para futuras sessions.

### REC-PERF-01: Pre-shape ASCII range al arranque
El 95%+ de los glifos tipeados son ASCII imprimible (32-126). Pre-shape + pre-rasterize este rango una vez al cargar la fuente y marcar esas entradas como "hot / never evict" en el atlas. Elimina cache-misses para el caso dominante.

### REC-PERF-02: `parking_lot::Mutex` en lugar de `std::sync::Mutex`
En macOS `parking_lot` es ~2× más rápido en paths no contendidos gracias a un fast-path inline. Relevante si aparece contention con PTY reader + main thread. Audit previo: ver dónde se usa `Arc<Mutex<...>>` actualmente (alacritty_terminal lo tiene internamente).

### REC-PERF-03: Damage tracking de alacritty_terminal
`alacritty_terminal::Term` expone `damage()` que devuelve las filas modificadas desde el último reset. Hoy `collect_grid_cells` itera todas las filas visibles. Integrar damage permitiría saltar filas no tocadas directamente (combinable con el row cache existente: cache-miss solo en filas damage).

### REC-PERF-04: Medir antes de optimizar
**Ninguno de los fixes P1/P2 debe implementarse sin profiling previo**. Instalar TD-PERF-30 primero: mediciones reales dictan el orden de ataque. Algunos items de este registro (ej. TD-PERF-16) pueden resultar irrelevantes en la práctica y otros (no detectados aquí) pueden ser los verdaderos cuellos de botella.

### REC-PERF-05: Frame budget explícito
Documentar en `.context/specs/term_specs.md` el frame budget objetivo:
- **Input-to-pixel p99:** < 8 ms (un frame a 120 Hz).
- **Steady-state idle:** 0 trabajo (no dirty → no redraw).
- **Cache-miss cold start:** < 16 ms (un frame a 60 Hz).
- **Atlas evict + reshape storm:** < 50 ms.

Sin budget no hay pass/fail para PRs de performance.

### REC-PERF-06: Criterion CI gating
Correr criterion en CI con baseline almacenado (`target/criterion/baseline/`). PR falla si `shape_line` regresa > 5%, `build_instances` > 3%, `search` > 10%. Protege contra regresiones accidentales.

---
