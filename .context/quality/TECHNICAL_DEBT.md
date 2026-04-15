# Technical Debt Registry

**Last Updated:** 2026-04-15
**Open Items:** 38
**Critical (P0):** 0 | **P1:** 5 | **P2:** 21 | **P3:** 12

> Resolved items are in [TECHNICAL_DEBT_archive.md](./TECHNICAL_DEBT_archive.md).

## Priority Definitions

| Priority | Definition | SLA |
|----------|------------|-----|
| P0 | Blocking development or causing incidents | Immediate |
| P1 | Significant impact on velocity or correctness | This sprint |
| P2 | Moderate impact, workaround exists | This quarter |
| P3 | Minor, address when convenient | Backlog |

---

## Auditoría de Memory Leaks — 2026-04-15

Sesión de auditoría con objetivo declarado: **diagnosticar el consumo de 20 GB de RAM tras un día de uso continuo**. Se auditaron todos los módulos con crecimiento de memoria no acotado: renderer (atlas GPU), font/shaper (caches de glifos), term (scrollback, PTY), ui (chat panel), llm (streaming, agent), config (watcher) y app (estado global). Cada item incluye archivo, mecanismo de leak y fix propuesto.

**Metodología:** lectura estática de todos los módulos fuente. Se identificaron estructuras de datos sin límite de capacidad, caches sin evicción, recursos no liberados al cerrar panes/tabs, y tareas tokio que pueden quedar colgadas.

---

## P1 — Alta prioridad (crecimiento ilimitado, impacto directo en RAM)

### TD-MEM-01: `evict_cold()` en GlyphAtlas no reclaima espacio físico en la textura GPU
- **Archivo:** `src/renderer/atlas.rs:evict_cold()`
- **Descripción:** `evict_cold()` elimina entradas del `HashMap<CacheKey, AtlasEntry>` pero **no resetea `cursor_x`/`cursor_y`**. El espacio físico en la textura de 64 MiB (4096×4096 RGBA) no se recupera. Los glifos evictados se re-rasterizarán y se subirán a posiciones nuevas, llenando progresivamente la textura hasta que `upload()` devuelva `AtlasError::Full`. En ese punto solo `clear()` puede recuperar el espacio, pero `clear()` recrea la textura y requiere re-rasterizar todo el contenido visible (stutter visible). Con uso continuo de 24 h, el atlas se llena y se vacía cíclicamente, pero cada ciclo de `clear()` causa un pico de CPU.
- **Impacto:** La textura de 64 MiB está siempre ocupada en VRAM. No es un leak de RAM del proceso, pero sí de VRAM. El problema real es que `evict_cold()` da una falsa sensación de que el espacio se recupera cuando no es así.
- **Fix:** Implementar compactación real del atlas: al evictar, marcar regiones como libres y reutilizarlas para nuevos uploads. Alternativa más simple: cuando `evict_cold()` elimina >50% de las entradas, llamar a `clear()` y re-subir solo las entradas calientes (las que sobrevivieron la evicción). Esto convierte el `clear()` de "catastrófico" a "selectivo".
- **Severidad:** P1 — la textura de 64 MiB nunca se reduce; `evict_cold()` es una ilusión de gestión de memoria.

---

### TD-MEM-02: `LcdGlyphAtlas` no tiene evicción — se llena y lanza error sin recuperación automática
- **Archivo:** `src/renderer/lcd_atlas.rs:upload()`
- **Descripción:** `LcdGlyphAtlas` (2048×2048 = 16 MiB) no tiene ningún mecanismo de evicción. Solo tiene `clear()`. Cuando el atlas se llena, `upload()` devuelve `Err("LCD glyph atlas is full")`. No hay código que llame a `clear()` automáticamente en respuesta a este error — el error se propaga y los glifos LCD dejan de renderizarse silenciosamente. Con uso continuo (muchos glifos únicos, cambios de fuente, múltiples tabs), el atlas se llena y el LCD AA queda roto hasta reiniciar.
- **Fix:** Añadir el mismo mecanismo de evicción por época que tiene `GlyphAtlas`: campo `last_used: u64` en `LcdAtlasEntry`, `next_epoch()`, `evict_cold(max_age)`. Cuando `upload()` falla con `Full`, llamar a `evict_cold()` y reintentar. Si sigue lleno, llamar a `clear()` y re-subir las entradas calientes.
- **Severidad:** P1 — el LCD AA se rompe silenciosamente tras uso prolongado.

---


### TD-MEM-04: `SwashCache` de cosmic-text crece indefinidamente sin límite
- **Archivo:** `src/font/shaper.rs:TextShaper` (campo `swash_cache: SwashCache`)
- **Descripción:** `SwashCache` de la crate `cosmic-text` es un cache interno de imágenes rasterizadas de glifos. No tiene límite de tamaño documentado ni API de evicción pública. Con uso continuo (muchos glifos únicos, emoji, fallback fonts), el cache crece indefinidamente. Es el candidato más probable para el consumo de 20 GB tras 24 h: cada glifo único rasterizado a cada tamaño ocupa ~1-4 KB en el cache, y con miles de glifos únicos (código fuente, logs, emoji) el total puede ser varios GB.
- **Fix:** Reemplazar `SwashCache` con una implementación propia que use `swash` directamente con un **LRU cache acotado en bytes** (no en número de entradas, ya que los glifos tienen tamaño variable: ~4.5 KB texto LCD, ~6 KB emoji). El límite se expresa en MB y es configurable en `perf.lua`:
  ```lua
  config.glyph_cache_mb = 50  -- default; rango recomendado: 20–200
  ```
  La implementación lleva un contador `used_bytes: usize` que se actualiza en cada insert/evict. Al insertar un glifo nuevo, si `used_bytes + glyph_size > limit_bytes`, se evictan entradas LRU hasta tener espacio. Con 50 MB el cache contiene ~10 700 entradas (mix típico), suficiente para cubrir el working set de una sesión de trabajo completa sin misses tras el warm-up inicial.
- **Decisión de diseño:** Se eligió **50 MB como default** porque: (a) es insignificante en hardware moderno (Mac con 16-32 GB RAM), (b) cubre el working set completo de una sesión típica (~5 000-8 000 glifos únicos) con margen, (c) elimina prácticamente todos los cache misses después del primer pass sobre el contenido, y (d) es 400× menos que el leak actual. 100 MB solo aporta beneficio con uso intensivo de CJK o muchas fuentes de fallback simultáneas.
- **Severidad:** P1 — candidato principal del leak de 20 GB. Sin límite de tamaño, crece con cada glifo único visto.
- **Validación:** El cache puede crecer durante uso activo pero **no debe crecer cuando la terminal está idle**. Los únicos drivers de crecimiento en idle son: cursor blink (cada 530 ms), actualización del reloj en la status bar (cada 1 s), y git branch polling (cada ~5 s) — todos con un conjunto de glifos acotado y estable que se agota rápidamente. Para verificar el fix:
  1. **Cota de tamaño:** el RSS del proceso no debe superar el valor de `config.glyph_cache_mb` (default 50 MB) atribuible al cache de glifos, independientemente del tiempo de sesión o variedad de glifos vistos.
  2. **Idle no crece:** dejar la terminal idle 30 min (con status bar y cursor blink activos) y medir RSS al inicio y al final — debe ser estable (±1 MB).
  3. **Uso intensivo acotado:** abrir un archivo de código grande con `less` o `bat`, hacer scroll completo (máximos glifos únicos), y verificar que el RSS no supera el límite configurado. Al repetir el scroll, el RSS no debe crecer (los glifos ya están en cache o se evictan los más fríos).
  4. **Evicción no causa stutter:** cuando el LRU evicta entradas (cache lleno), el frame time no debe superar 16 ms — los glifos evictados se re-rasterizan bajo demanda sin pico visible.
  5. **Configurabilidad:** cambiar `config.glyph_cache_mb = 20` en `perf.lua`, recargar config en caliente, y verificar que el cache se reduce al nuevo límite evictando entradas frías.

---


### TD-MEM-06: `byte_to_col_buf` en `TextShaper` crece al tamaño máximo de línea visto y nunca se reduce
- **Archivo:** `src/font/shaper.rs:shape_line_harfbuzz()` — `self.byte_to_col_buf.resize(n + 1, 0)`
- **Descripción:** `byte_to_col_buf` es un `Vec<usize>` que se redimensiona con `resize()` al tamaño de la línea actual. `Vec::resize` solo crece, nunca reduce la capacidad. Si en algún momento se shapea una línea muy larga (ej. un log de 10 000 bytes en una sola línea), el buffer queda con esa capacidad para siempre. Con múltiples panes/tabs, cada `TextShaper` tiene su propio buffer. No es un leak grave por sí solo, pero contribuye al consumo base.
- **Fix:** Añadir `self.byte_to_col_buf.shrink_to(MAX_REASONABLE_LINE)` después de cada uso si `n < capacity / 4` (shrink cuando la línea actual es mucho más corta que la capacidad). Alternativa: usar `Vec::with_capacity(n + 1)` local en lugar del buffer reutilizable — la alocación es barata para líneas cortas y evita el problema de capacidad permanente.
- **Severidad:** P1 — menor por sí solo, pero se multiplica por el número de panes/tabs abiertos.

---

### TD-MEM-07: `messages` en `ChatPanel` crece indefinidamente — sin límite de historial
- **Archivo:** `src/llm/chat_panel.rs:ChatPanel` (campo `messages: Vec<ChatMessage>`)
- **Descripción:** El historial de conversación (`messages`) y su cache de wrapped lines (`wrapped_cache`) crecen sin límite. Cada mensaje del asistente puede contener cientos de líneas (respuestas largas, diffs, código). Con uso intensivo del panel de AI durante 24 h, el historial puede acumular decenas de MB. `wrapped_cache` crece en paralelo (un `Vec<String>` por mensaje). `attached_files` y `attached_file_chars` también crecen sin límite si el usuario adjunta muchos archivos.
- **Fix:** (a) Limitar `messages` a las últimas N entradas (ej. 200 mensajes). Al truncar, eliminar también las entradas correspondientes de `wrapped_cache`. (b) Limitar `attached_files` a un máximo razonable (ej. 20 archivos). (c) Añadir un botón "Clear history" en la UI del panel.
- **Severidad:** P1 — con uso intensivo del AI panel, el historial puede acumular decenas de MB.

---


## P2 — Prioridad media

### TD-MEM-09: Scrollback por pane sin límite efectivo en sesiones largas con muchos tabs
- **Archivo:** `src/term/mod.rs:Terminal::new()` — `scrolling_history: config.scrollback_lines`
- **Descripción:** Cada `Terminal` tiene su propio scrollback buffer gestionado por `alacritty_terminal`. Con el default de 10 000 líneas y ~200 bytes/línea, cada terminal ocupa ~2 MB de scrollback. Con 10 tabs × 2 panes = 20 terminales, son ~40 MB solo de scrollback. Con `scrollback_lines = 50000` (valor que el README sugiere como ejemplo), son 200 MB. En sesiones de 24 h con muchos tabs abiertos, el scrollback es el mayor consumidor de RAM del proceso.
- **Fix:** (a) Reducir el default de `scrollback_lines` de 10 000 a 5 000. (b) Documentar el impacto de memoria en `perf.lua`. (c) Implementar un límite global de scrollback total (ej. 100 MB) distribuido entre los terminales activos, reduciendo el límite por terminal cuando hay muchos abiertos.
- **Severidad:** P2 — no es un leak sino un diseño con alto consumo base. Configurable por el usuario.

---

### TD-MEM-10: `file_picker_items` no se limpia al cerrar el file picker
- **Archivo:** `src/llm/chat_panel.rs:close_file_picker()`
- **Descripción:** `close_file_picker()` solo pone `file_picker_open = false` pero no limpia `file_picker_items`. El `Vec<PathBuf>` con todos los archivos escaneados del CWD permanece en memoria hasta la próxima apertura del picker (que lo reemplaza). En proyectos grandes (monorepos con miles de archivos), este Vec puede ocupar varios MB innecesariamente mientras el picker está cerrado.
- **Fix:** En `close_file_picker()`, añadir `self.file_picker_items.clear(); self.file_picker_items.shrink_to_fit();` para liberar la memoria inmediatamente al cerrar.
- **Severidad:** P2 — menor pero fácil de corregir.

---

### TD-MEM-11: `filtered_picker_items()` crea `SkimMatcherV2` en cada llamada (cada frame con picker abierto)
- **Archivo:** `src/llm/chat_panel.rs:filtered_picker_items()`
- **Descripción:** `filtered_picker_items()` instancia `SkimMatcherV2::default()` en cada llamada. Esta función se llama en cada frame mientras el file picker está abierto (para renderizar la lista filtrada). `SkimMatcherV2` tiene un costo de inicialización no trivial y aloca internamente. Son decenas de alocaciones/liberaciones por segundo mientras el picker está abierto.
- **Fix:** Cachear el matcher como campo de `ChatPanel` (`matcher: SkimMatcherV2`) inicializado una sola vez en `new()`. El matcher es stateless entre queries, por lo que puede reutilizarse sin problema.
- **Severidad:** P2 — alocaciones innecesarias en el render loop.

---

### TD-MEM-12: Tokio task de streaming LLM puede quedar colgada si el usuario cierra el panel
- **Archivo:** `src/app/ui.rs` (spawn del task de streaming), `src/llm/openrouter.rs:stream()`
- **Descripción:** Cuando el usuario inicia una query al LLM, se spawnea un tokio task que consume el `TokenStream` (conexión HTTP SSE abierta). Si el usuario cierra el panel durante el streaming, el task continúa ejecutándose hasta que el stream se agota o el timeout de 120 s expira. Durante ese tiempo, la conexión HTTP permanece abierta y el task ocupa memoria (buffer de tokens, estado del stream). Con múltiples queries canceladas, pueden acumularse varios tasks colgados.
- **Fix:** Usar `tokio::task::JoinHandle` + `CancellationToken` (de la crate `tokio-util`). Al cerrar el panel o iniciar una nueva query, cancelar el token del task anterior. El task debe hacer `select!` entre el stream y el token de cancelación para terminar limpiamente.
- **Severidad:** P2 — en uso normal (una query a la vez) el impacto es bajo, pero con uso intensivo puede acumular tasks y conexiones.

---

### TD-MEM-13: `api_messages` en el agent loop crece con cada tool call round
- **Archivo:** `src/llm/tools.rs` (función `agent_step` o equivalente)
- **Descripción:** El agent loop acumula `api_messages: Vec<Value>` con cada round de tool calls (hasta 10 rounds). Cada round agrega el mensaje del asistente + los resultados de las tools. Con archivos grandes adjuntos (ej. `ReadFile` de un archivo de 100 KB) y 10 rounds, el Vec puede acumular varios MB de JSON. Este Vec se crea por query y se descarta al terminar, pero durante la query ocupa memoria proporcional al número de rounds × tamaño de los resultados.
- **Fix:** (a) Limitar el tamaño de los resultados de `ReadFile` a N caracteres (ej. 50 000) con truncación explícita. (b) Limitar el número de rounds a 5 en lugar de 10. (c) Usar streaming JSON en lugar de acumular todo en memoria (más complejo).
- **Severidad:** P2 — impacto proporcional al uso del AI agent con archivos grandes.

---

### TD-MEM-14: `ConfigWatcher` usa `mpsc::channel()` unbounded — puede acumular eventos
- **Archivo:** `src/config/watcher.rs:ConfigWatcher::new()`
- **Descripción:** El watcher usa `std::sync::mpsc::channel()` (unbounded). Si el filesystem genera muchos eventos rápidamente (ej. un editor que guarda con `atomic_save` generando 3-5 eventos por guardado, o un `git checkout` que toca muchos archivos `.lua`), el canal puede acumular cientos de eventos antes de que `poll()` los drene. `poll()` drena todos pero solo retorna el último, por lo que los eventos intermedios se descartan — el canal actúa como buffer innecesario.
- **Fix:** Usar `mpsc::sync_channel(1)` (bounded con capacidad 1) con `try_send` en el closure del watcher. Si el canal ya tiene un evento pendiente, descartar el nuevo (ya hay una notificación de cambio pendiente). Esto elimina el buffer ilimitado y reduce la presión de memoria.
- **Severidad:** P2 — menor en uso normal, pero puede acumular en escenarios de muchos cambios de filesystem.

---

### TD-MEM-19: Cursor blink, reloj y git polling corren aunque la ventana no tenga foco
- **Archivo:** `src/app/mod.rs` (cursor blink timer), `src/app/ui.rs` (status bar clock, `poll_git_branch`)
- **Descripción:** Los tres timers periódicos corren continuamente sin importar si la ventana tiene foco o si la máquina está suspendida. Esto causa redraws innecesarios (~2/s por blink + 1/s por reloj + 1/5s por git) que presionan el `SwashCache` (TD-MEM-04) con misses de glifos del reloj y la status bar, y mantienen el GPU activo sin razón. En una sesión de 8 h con la ventana en background, son ~60 000 redraws evitables.
- **Fix:**
  1. Escuchar `WindowEvent::Focused(bool)` de winit y guardar `window_focused: bool` en `App`.
  2. Cuando `window_focused = false`: no procesar ticks de cursor blink, no disparar redraw por el reloj, no spawnear `poll_git_branch`.
  3. Cambiar `ControlFlow` a `Wait` cuando la ventana no tiene foco y no hay PTY activo escribiendo — el event loop se suspende hasta el próximo evento real en lugar de correr a 60 fps.
  4. Al recuperar el foco (`Focused(true)`): forzar un redraw inmediato para actualizar el reloj y mostrar el cursor en estado visible (no en fase "apagado" del blink).
  5. Para sleep/wake: no se requiere manejo especial — los timers de tokio se pausan con el system clock durante el sleep; al despertar, winit emite `Resumed` y todo vuelve a arrancar.
- **Severidad:** P2 — elimina decenas de miles de redraws evitables en sesiones largas con la ventana en background; reduce la presión sobre el `SwashCache` en idle.
- **Validación:**
  1. Con la ventana en background, medir CPU usage de PetruTerm con `Activity Monitor` — debe ser 0% (o cercano) si no hay PTY activo.
  2. El `SwashCache` no debe crecer mientras la ventana está en background.
  3. Al recuperar el foco, el reloj muestra la hora correcta en el primer frame y el cursor aparece visible.

---

## P3 — Prioridad baja

### TD-MEM-15: `FreeTypeCmapLookup` mantiene `FT_Library` + `FT_Face` por `TextShaper`
- **Archivo:** `src/font/shaper.rs:FreeTypeCmapLookup`
- **Descripción:** Cada `TextShaper` crea una instancia de `FreeTypeCmapLookup` que mantiene un `FT_Library` y un `FT_Face` en memoria durante toda la vida del shaper. El `Drop` impl los libera correctamente. El problema es que si se crean múltiples `TextShaper` (ej. uno por pane con fuentes diferentes), cada uno tiene su propia instancia de FreeType. En la práctica hay un solo `TextShaper` global, por lo que el impacto es mínimo.
- **Fix:** Si en el futuro se crean múltiples shapers, compartir una única instancia de `FreeTypeCmapLookup` via `Arc<Mutex<...>>` o `Rc<RefCell<...>>`.
- **Severidad:** P3 — impacto mínimo con la arquitectura actual (un solo shaper).

---

### TD-MEM-16: `ascii_glyph_cache` es un array fijo de 128 `u32` — OK pero documentar
- **Archivo:** `src/font/shaper.rs:TextShaper` (campo `ascii_glyph_cache: [u32; 128]`)
- **Descripción:** El cache ASCII es un array de tamaño fijo (512 bytes). No es un problema de memoria, pero si en el futuro se añaden más rangos (ej. Latin Extended, Cyrillic), el array podría crecer. Documentar el límite explícitamente.
- **Fix:** Añadir un comentario `// Fixed-size: 512 bytes. Extend to HashMap if non-ASCII fast paths are needed.`
- **Severidad:** P3 — documentación, no un bug.

---

### TD-MEM-17: `streaming_buf` en `ChatPanel` no se limpia si el panel se cierra durante streaming
- **Archivo:** `src/llm/chat_panel.rs:close()`
- **Descripción:** `close()` solo pone `state = Hidden` pero no limpia `streaming_buf`. Si el usuario cierra el panel mientras el LLM está streamando, `streaming_buf` retiene todos los tokens recibidos hasta ese momento. Al reabrir el panel, `streaming_buf` aún contiene el contenido parcial de la query anterior.
- **Fix:** En `close()`, añadir `self.streaming_buf.clear();` para liberar la memoria y evitar mostrar contenido stale al reabrir.
- **Severidad:** P3 — menor, pero causa confusión UX al reabrir el panel.

---

### TD-MEM-18: `separator_cache` y `thin_separator_cache` no se liberan al cerrar el panel
- **Archivo:** `src/llm/chat_panel.rs:ChatPanel` (campos `separator_cache`, `thin_separator_cache`)
- **Descripción:** Los separadores cacheados son strings de longitud proporcional al ancho del panel (típicamente ~50 chars). Se reconstruyen solo cuando el ancho cambia. No es un problema de memoria significativo, pero si el panel se cierra y no se reabre, los strings permanecen en memoria innecesariamente.
- **Fix:** En `close()`, opcionalmente limpiar los caches de separadores. Impacto mínimo (~100 bytes).
- **Severidad:** P3 — negligible.

---

## Resumen de impacto estimado

| ID | Módulo | Tipo | RAM estimada tras 24 h | Prioridad |
|----|--------|------|------------------------|-----------|
| TD-MEM-04 | `SwashCache` | Crecimiento ilimitado | **~10-15 GB** (candidato principal) | P1 |
| TD-MEM-09 | Scrollback | Alto consumo base | ~40-200 MB (según tabs) | P2 |
| TD-MEM-07 | Chat history | Crecimiento ilimitado | ~10-100 MB (según uso AI) | P1 |
| TD-MEM-01 | GlyphAtlas VRAM | No reclaima espacio | 64 MiB VRAM permanente | P1 |
| TD-MEM-02 | LcdGlyphAtlas | Sin evicción | 16 MiB VRAM + LCD roto | P1 |
| ~~TD-MEM-05~~ | `word_cache` | ~~Miss storm periódico~~ | ~~resuelto~~ | ~~P1~~ |
| TD-MEM-06 | `byte_to_col_buf` | Crece sin reducir | ~1-10 MB (según líneas largas) | P1 |
| ~~TD-MEM-08~~ | `terminal_shell_ctxs` | ~~Leak por terminal cerrado~~ | ~~resuelto~~ | ~~P1~~ |
| ~~TD-MEM-03~~ | Bind groups stale | ~~Correctness bug~~ | ~~resuelto~~ | ~~P1~~ |
| TD-MEM-12 | Tokio tasks colgados | Tasks no cancelados | ~10-50 MB (según queries canceladas) | P2 |
| TD-MEM-13 | Agent `api_messages` | Crece por round | ~10-50 MB por query con archivos grandes | P2 |

**Causa más probable del consumo de 20 GB:** `SwashCache` (TD-MEM-04) combinado con scrollback alto (TD-MEM-09) y chat history sin límite (TD-MEM-07). Atacar TD-MEM-04 primero.

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
