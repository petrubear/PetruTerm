# Technical Debt Registry

**Last Updated:** 2026-04-16
**Open Items:** 43
**Critical (P0):** 0 | **P1:** 0 | **P2:** 24 | **P3:** 19

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

### TD-MEM-10: `file_picker_items` no se limpia al cerrar el file picker
- **Archivo:** `src/llm/chat_panel.rs:close_file_picker()`
- **Descripción:** `close_file_picker()` pone `file_picker_open = false` pero no limpia `file_picker_items`. El `Vec<PathBuf>` con todos los archivos escaneados permanece hasta la próxima apertura.
- **Fix:** `self.file_picker_items.clear(); self.file_picker_items.shrink_to_fit();` en `close_file_picker()`.
- **Severidad:** P2 — menor pero fácil.

---

### TD-MEM-11: `SkimMatcherV2` instanciado en cada llamada a `filtered_picker_items()`
- **Archivo:** `src/llm/chat_panel.rs:filtered_picker_items()`
- **Descripción:** Se llama en cada frame mientras el file picker está abierto. `SkimMatcherV2::default()` tiene costo de inicialización no trivial y aloca internamente — decenas de alocaciones/liberaciones por segundo.
- **Fix:** Campo `matcher: SkimMatcherV2` en `ChatPanel`, inicializado una vez en `new()`.
- **Severidad:** P2 — alocaciones innecesarias en el render loop.

---

### TD-MEM-12: Task de streaming LLM puede quedar colgada si el usuario cierra el panel
- **Archivo:** `src/app/ui.rs` (spawn del task de streaming), `src/llm/openrouter.rs:stream()`
- **Descripción:** Si el usuario cierra el panel durante streaming, el task tokio continúa hasta que el stream se agota o el timeout de 120 s expira. La conexión HTTP permanece abierta. Con múltiples queries canceladas, pueden acumularse varios tasks.
- **Fix:** `tokio::task::JoinHandle` + `CancellationToken` (tokio-util). Al cerrar el panel, cancelar el token del task anterior. El task hace `select!` entre el stream y el token de cancelación.
- **Severidad:** P2 — con uso intensivo puede acumular tasks y conexiones HTTP.

---

### TD-MEM-13: `api_messages` en el agent loop crece con cada round de tool calls
- **Archivo:** `src/llm/tools.rs`
- **Descripción:** El agent loop acumula `api_messages: Vec<Value>` con cada round (hasta 10 rounds). Con archivos grandes adjuntos y 10 rounds, el Vec puede acumular varios MB de JSON transitorio.
- **Fix:** (a) Limitar el tamaño de resultados de `ReadFile` a 50 000 chars con truncación explícita. (b) Limitar rounds a 5.
- **Severidad:** P2 — proporcional al uso del AI agent con archivos grandes.

---

### TD-MEM-19: Cursor blink, reloj y git polling corren sin foco de ventana
- **Archivo:** `src/app/mod.rs` (cursor blink timer), `src/app/ui.rs` (status bar clock, `poll_git_branch`)
- **Descripción:** Los tres timers corren continuamente independientemente del foco. ~60 000 redraws evitables en 8 h con la ventana en background. Presionan el `SwashCache` (TD-MEM-04) con misses de glifos del reloj y status bar.
- **Fix:** Escuchar `WindowEvent::Focused(bool)`. Cuando `window_focused = false`: no procesar blink, no redrawn por reloj, no spawnear `poll_git_branch`. Cambiar `ControlFlow` a `Wait` cuando sin foco y sin PTY activo.
- **Severidad:** P2 — elimina decenas de miles de redraws evitables; reduce presión sobre SwashCache en idle.

---

### TD-MEM-20: `UiManager.chat_panels` retiene historial de terminales cerrados
- **Archivo:** `src/app/ui.rs:set_active_terminal()`, `src/app/mod.rs:close_exited_terminals()`
- **Origen:** codex
- **Descripción:** `chat_panels: HashMap<usize, ChatPanel>` no elimina entradas cuando un pane/tab se cierra. El `ChatPanel` completo de terminales muertos — `messages`, `wrapped_cache`, `attached_files`, `streaming_buf` — permanece en RAM indefinidamente.
- **Fix:** `UiManager::remove_terminal_state(terminal_id)` llamado en los mismos puntos donde se limpia `terminal_shell_ctxs`. Si el panel activo pertenece al terminal eliminado, redirigir `active_panel_id`.
- **Severidad:** P2 — crecimiento acumulativo silencioso en sesiones largas con tabs/panes efímeros.

---

### TD-MEM-21: `row_caches` retiene entradas de terminales cerrados
- **Archivo:** `src/app/renderer.rs:RenderContext` — campo `row_caches: HashMap<usize, RowCache>`
- **Origen:** kiro
- **Descripción:** Cuando un pane/tab se cierra, `close_exited_terminals` y `cmd_close_pane` limpian `terminal_shell_ctxs` pero **no notifican a `RenderContext`**. La entrada `RowCache` del terminal cerrado permanece en el HashMap. Cada `RowCache` contiene `Vec<Option<RowCacheEntry>>` con `Vec<CellVertex>` por fila (~40 filas × ~80 celdas). Con muchos ciclos de apertura/cierre, el HashMap crece sin límite.
- **Fix:** `rc.row_caches.remove(&tid)` en `close_exited_terminals()` y en el drenado de `mux.closed_ids`, junto con la limpieza existente de `terminal_shell_ctxs`.
- **Severidad:** P2 — crecimiento acumulativo silencioso en sesiones largas.

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

### TD-PERF-17: Config hot-reload sin debounce
- **Archivo:** `src/config/watcher.rs` → `src/app/mod.rs:219-233`
- **Descripción:** Editores como Neovim con `atomic_save` generan 2-3 eventos `notify` por guardado. Cada evento dispara reparse Lua completo + validación + rebuild de palette + potencial invalidación de atlas.
- **Fix:** Debounce de 300 ms. Primer evento arma un `Instant` futuro; eventos subsecuentes lo reinician; reload real cuando el timer expira.
- **Severidad:** P2 — reloads duplicados durante edición activa del config.

---

### TD-PERF-19: `poll_git_branch` sin guard de tarea en curso
- **Archivo:** `src/app/ui.rs:265-293`
- **Descripción:** Si `git branch --show-current` tarda >5 s (NFS, repo enorme), el siguiente frame cumplirá el TTL y disparará otro spawn, encadenando tasks tokio redundantes.
- **Fix:** `git_branch_in_flight: bool` activado al spawn, desactivado al recibir resultado. Solo spawnear si `!in_flight && (cwd_changed || ttl_expired)`.
- **Severidad:** P2 — raro pero acumula tasks en casos patológicos.

---

### TD-PERF-20: `char_indices` truncación en hot paths (spinner resuelto por TD-PERF-13)
- **Archivo:** `src/app/renderer.rs:662,663,754` (truncación de paths/hints)
- **Descripción:** La truncación de paths y hints usa `chars().take(N).collect::<String>()`, alocando un string nuevo para cortar a N chars. El spinner fue resuelto en TD-PERF-13 (frame_counter). Solo queda la truncación.
- **Fix:** `s.char_indices().nth(N).map(|(i, _)| &s[..i]).unwrap_or(s)` — cero alocación, O(N) en chars.
- **Severidad:** P2 — micro, se suma con los demás scratch buffers del render path.

---

### TD-PERF-21: Palette fuzzy matcher re-filtra la lista completa en cada tecla
- **Archivo:** `src/ui/palette/mod.rs:77-79,137-152`
- **Descripción:** Cada keystroke ejecuta `SkimMatcherV2::fuzzy_match()` sobre todos los actions, hace sort y reemplaza el resultado. Sin filtrado incremental ni caché.
- **Fix:** Cachear `last_query` + `last_results`. Si el query nuevo empieza con el viejo (append de char), filtrar `last_results` en lugar del set completo. O(prev_results) en lugar de O(all_actions).
- **Severidad:** P2 — se nota con 500+ actions (plugins en Phase 4).

---

### TD-PERF-22: Highlight de search lookup O(matches) por celda en el render
- **Archivo:** `src/app/mux.rs:441-454` (`search_highlight_at`), llamado en `collect_grid_cells_for`
- **Descripción:** Para cada celda visible se recorre el `Vec<SearchMatch>` entero. Con 100 matches × 3 200 celdas (80×40), son 320 000 comparaciones por frame.
- **Fix:** `HashMap<i32, Vec<(col_start, col_end, match_idx)>>` indexado por `grid_line`, construido una sola vez cuando los matches cambian. Lookup O(1) por celda.
- **Severidad:** P2 — solo activo con search visible; degrada frame rate cuando activo.

---

### TD-PERF-31: Diff de confirmación de escritura calculado síncronamente en el event loop
- **Archivo:** `src/app/ui.rs:poll_ai_events()`, `src/llm/chat_panel.rs:ConfirmDisplay::for_write()`
- **Origen:** codex
- **Descripción:** Cuando el agente propone `write_file`, `poll_ai_events()` llama `ConfirmDisplay::for_write()` en el hilo principal: `std::fs::read_to_string(path)` + `diff_lines()` + `compress_diff()`. Para archivos grandes, mete I/O y CPU intensivos dentro del event loop de winit, causando freeze de UI visible.
- **Fix:** Precomputar el diff en el task async antes de enviar el evento, o en un worker dedicado. Enviar a la UI solo el resultado comprimido. Añadir límites por bytes/líneas.
- **Severidad:** P2 — stall interactivo en una ruta frecuente del panel AI.

---

### TD-PERF-32: `colors_scratch` re-alocado por llamada a `build_pane_instances`
- **Archivo:** `src/app/renderer.rs:191` — `let mut colors_scratch: Vec<([f32;4],[f32;4])> = Vec::with_capacity(cols);`
- **Origen:** kiro (descripción ajustada — kiro dijo "per row", verificado como "per pane per call")
- **Descripción:** `colors_scratch` se declara como local dentro de `build_pane_instances`, no en `RenderContext`. Aloca una vez por pane por frame con cache-miss. Con 3 panes visibles, son 3 alocaciones de heap por frame aunque ninguna fila haya cambiado. Los otros scratch buffers (`scratch_chars`, `scratch_str`, `scratch_colors`) ya viven en `RenderContext`.
- **Fix:** Mover `colors_scratch` a `RenderContext` como campo `pub colors_scratch: Vec<([f32;4],[f32;4])>`. En `build_pane_instances`, `.clear()` + `.extend()` en lugar de `Vec::with_capacity()`.
- **Severidad:** P2 — alocación evitable por pane; fácil de corregir siguiendo el patrón existente.

---

### TD-PERF-33: `filtered_picker_items()` clona todos los `PathBuf` en cada frame
- **Archivo:** `src/llm/chat_panel.rs:filtered_picker_items()`
- **Origen:** kiro
- **Descripción:** Llamada en cada frame mientras el file picker está abierto. Clona cada `PathBuf` candidato en el Vec `scored` y luego en el `collect()` final. En un proyecto con 500 archivos, son 500+ clones de `PathBuf` por frame (~30 000 clones/s mientras el picker está abierto).
- **Fix:** Cambiar firma a `filtered_picker_items(&self) -> Vec<&PathBuf>` — devolver referencias. El caller solo necesita leer los paths para mostrarlos.
- **Severidad:** P2 — 30 000+ alocaciones de heap por segundo mientras el picker está abierto.

---

### TD-PERF-34: `static_hash()` y `calculate_row_hash()` usan SipHash-1-3 (DoS-resistant, innecesario)
- **Archivo:** `src/app/mod.rs:static_hash()`, `src/app/renderer.rs:calculate_row_hash()`
- **Origen:** kiro
- **Descripción:** `DefaultHasher` en Rust usa SipHash-1-3, diseñado para resistencia a DoS. Para cache keys internas (títulos de tabs, CWD, branch name, contenido de filas), SipHash es innecesariamente lento. `calculate_row_hash()` se llama en el hot path del renderer (una vez por fila con cache-miss). FxHash o AHash son 2-4× más rápidos para inputs cortos.
- **Fix:** Añadir `rustc-hash` (FxHash). Reemplazar `DefaultHasher` en `static_hash()` y `calculate_row_hash()` con `FxHasher`. Una línea por función; no afecta correctness.
- **Severidad:** P2 — `calculate_row_hash` en hot path; FxHash ~3× más rápido para strings cortos.

---

### TD-PERF-37: `word_wrap` re-envuelve el `streaming_buf` completo en cada token
- **Archivo:** `src/app/renderer.rs:729` — `let wrapped = word_wrap(&panel.streaming_buf, msg_inner_w);`
- **Origen:** kiro
- **Descripción:** Durante streaming, `build_chat_panel_instances` llama `word_wrap(&panel.streaming_buf, msg_inner_w)` en cada frame donde el panel está dirty (es decir, en cada token). `word_wrap` itera todo el contenido desde el principio construyendo un `Vec<String>` nuevo. Con respuestas de 500 tokens, son 500 llamadas a `word_wrap` procesando buffers de 1-500 tokens = O(n²) trabajo total.
- **Fix:** `streaming_wrapped: Vec<String>` en `ChatPanel` actualizado incrementalmente: cuando llega un nuevo token, solo re-envolver la última línea parcial.
- **Severidad:** P2 — lag creciente en respuestas largas de LLM.

---

## P3 — Prioridad baja / Backlog

### TD-MEM-14: `ConfigWatcher` usa `mpsc::channel()` unbounded
- **Archivo:** `src/config/watcher.rs:ConfigWatcher::new()`
- **Descripción:** El watcher usa un canal unbounded. En `git checkout` con muchos archivos `.lua`, pueden acumularse cientos de eventos antes de que `poll()` los drene. `poll()` solo retorna el último, por lo que los intermedios son descartados — el canal actúa como buffer innecesario.
- **Fix:** `mpsc::sync_channel(1)` con `try_send`. Si ya hay un evento pendiente, descartar el nuevo.
- **Severidad:** P3 — menor en uso normal; downgraded desde P2.

---

### TD-MEM-15: `FreeTypeCmapLookup` mantiene `FT_Library` + `FT_Face` por `TextShaper`
- **Archivo:** `src/font/shaper.rs:FreeTypeCmapLookup`
- **Descripción:** Cada `TextShaper` crea una instancia de FreeType. `Drop` los libera correctamente. En la práctica hay un solo `TextShaper` global; impacto mínimo.
- **Fix:** Si en el futuro se crean múltiples shapers, compartir via `Arc<Mutex<...>>`.
- **Severidad:** P3 — impacto mínimo con arquitectura actual.

---

### TD-MEM-16: `ascii_glyph_cache` es un array fijo de 128 `u32` — documentar
- **Archivo:** `src/font/shaper.rs:TextShaper`
- **Descripción:** No es un problema de memoria. Documentar el límite explícitamente por si se añaden rangos futuros (Latin Extended, Cyrillic).
- **Fix:** Comentario `// Fixed-size: 512 bytes. Extend to HashMap if non-ASCII fast paths are needed.`
- **Severidad:** P3 — documentación, no un bug.

---

### TD-MEM-17: `streaming_buf` no se limpia al cerrar el panel durante streaming
- **Archivo:** `src/llm/chat_panel.rs:close()`
- **Descripción:** `close()` pone `state = Hidden` pero no limpia `streaming_buf`. Al reabrir el panel, puede mostrar contenido stale.
- **Fix:** `self.streaming_buf.clear();` en `close()`.
- **Severidad:** P3 — confusión UX menor.

---

### TD-MEM-18: `separator_cache` y `thin_separator_cache` no se liberan al cerrar el panel
- **Archivo:** `src/llm/chat_panel.rs:ChatPanel`
- **Descripción:** Strings proporcionales al ancho del panel (~50 chars), permanecen si el panel no se reabre. ~100 bytes. Negligible.
- **Fix:** `clear()` en `close()`. Impacto mínimo.
- **Severidad:** P3 — negligible.

---

### TD-MEM-22: Canales AI `ai_tx`/`ai_rx` y `block_tx`/`block_rx` son unbounded
- **Archivo:** `src/app/ui.rs:UiManager::new()` — líneas 96, 97, 109
- **Descripción:** En condiciones normales, el main thread drena los tokens en cada frame (~16 ms). Con modelos a 100 tokens/s, se acumulan como máximo 1-2 tokens por frame. El riesgo práctico es mínimo. El patrón es arquitectónicamente incorrecto pero no causa problemas observables.
- **Fix:** `crossbeam_channel::bounded(256)` para `ai_tx`/`ai_rx`; `bounded(64)` para `block_tx`/`block_rx`. Manejar `SendError` (canal lleno) descartando el token.
- **Severidad:** P3 — downgraded desde P2; bounded por la tasa de tokens y el frame rate.

---

### TD-MEM-24: `undo_stack` usa `Vec::remove(0)` — O(n) shift en cada evicción
- **Archivo:** `src/app/ui.rs:216` — `self.undo_stack.remove(0)`
- **Origen:** kiro
- **Descripción:** Con `MAX_UNDO = 10`, el O(10) shift es imperceptible. El patrón es incorrecto: una cola FIFO debe usar `VecDeque` con `pop_front()` O(1).
- **Fix:** `undo_stack: VecDeque<(PathBuf, String)>`. `remove(0)` → `pop_front()`. `push` → `push_back()`. `pop()` en `cmd_undo_last_write` → `pop_back()`.
- **Severidad:** P3 — downgraded desde P2; zero impacto práctico con MAX_UNDO=10.

---

### TD-MEM-25: Canal `git_tx`/`git_rx` en `UiManager` es unbounded
- **Archivo:** `src/app/ui.rs:UiManager::new()`
- **Origen:** kiro
- **Descripción:** Con el in-flight guard (`git_branch_in_flight`), solo hay a lo sumo un resultado pendiente. El canal unbounded es funcionalmente equivalente a `bounded(1)` en condiciones normales.
- **Fix:** `crossbeam_channel::bounded(1)`. Si el canal está lleno, descartar el resultado nuevo (ya hay uno más reciente).
- **Severidad:** P3 — corrección de patrón; impacto mínimo en condiciones normales.

---

### TD-PERF-03: Upload completo del instance buffer a GPU cada frame
- **Archivo:** `src/app/mod.rs:612` → `src/renderer/gpu.rs:312`
- **Descripción:** En Apple Silicon (M2/M4) con unified memory, `write_buffer` es un memcpy en memoria compartida. ~800 KB a 60 fps = ~48 MB/s frente a 100+ GB/s de bandwidth — 0.05% del bus. **No es cuello de botella real en Apple Silicon.** Relevante solo en Phase 2+ con GPUs discretas.
- **Fix futuro:** Dirty-rect tracking por fila para reducir volumen de upload. Dejar para Phase 2+ (cross-platform).
- **Severidad:** P3 — downgraded desde P1; no es un bottleneck medible en el target hardware actual.

---

### TD-PERF-18: Tokio runtime con pool de threads por defecto (num_cpus)
- **Archivo:** `src/app/ui.rs:93-96`
- **Descripción:** El pool crea `num_cpus::get()` workers (típicamente 8-16 en M4). Las tasks de PetruTerm son todas I/O-bound (requests HTTP al LLM, git branch, lecturas de archivo). Un pool grande = context switches innecesarios + ~2 MB de stack por worker.
- **Fix:** `Builder::new_multi_thread().worker_threads(2).enable_all().build()`.
- **Severidad:** P3 — downgraded desde P2; overhead constante pero marginal en Apple Silicon.

---

### TD-PERF-23: Leader key timeout con `Instant::elapsed()` por keystroke
- **Archivo:** `src/app/input/mod.rs:159-163`
- **Descripción:** Cada keystroke evalúa `if t.elapsed() > timeout_ms` — `elapsed()` llama a `SystemTime::now()` (syscall en macOS).
- **Fix:** `leader_deadline: Instant` almacenado al activar el leader; comparar `Instant::now() >= leader_deadline` solo cuando se necesite.
- **Severidad:** P3 — overhead durante typing rápido con leader activo.

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

### TD-PERF-35: `" ".repeat(gap)` aloca un `String` en cada rebuild del status bar
- **Archivo:** `src/app/renderer.rs:1184`
- **Descripción:** `self.push_shaped_row(&" ".repeat(gap), ...)` — el status bar tiene cache, por lo que esta alocación solo ocurre en rebuilds (cambio de CWD, branch, exit code, resize). En condiciones normales es raro.
- **Fix:** Campo `gap_buf: String` en `RenderContext` reutilizable (`clear()` + `extend(repeat(' ').take(gap))`).
- **Severidad:** P3 — downgraded desde P2; detrás de cache; alocación infrecuente.

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

### REC-PERF-03: Damage tracking de alacritty_terminal
`alacritty_terminal::Term` expone `damage()` con las filas modificadas desde el último reset. Hoy `collect_grid_cells` itera todas las filas visibles. Integrar damage permitiría saltar filas no tocadas.

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
