# Technical Debt Archive

All resolved items from [TECHNICAL_DEBT.md](./TECHNICAL_DEBT.md).
Ordered newest-first within each date group.

---

## Resolved 2026-04-23 — Auditoría de performance y memoria

### TD-MEM-26: FreeType memory leaks en FreeTypeCmapLookup y FreeTypeLcdRasterizer
- **Archivos:** `src/font/shaper.rs`, `src/font/freetype_lcd.rs`
- **Descripción:** Los structs `FreeTypeCmapLookup` y `FreeTypeLcdRasterizer` manejan punteros crudos de FreeType (`FT_Library`, `FT_Face`) pero no hay verificación de errores en todas las rutas de creación. Si falla la inicialización, los recursos pueden no liberarse correctamente.
- **Root cause:** Constructores usan `unsafe` blocks con `FT_Init_FreeType`, `FT_New_Face`, etc., pero en caminos de error (ej. path no UTF-8, FT_Set_Char_Size falla), no se llama a `FT_Done_Face`/`FT_Done_FreeType` antes de retornar `None`.
- **Impacto:** Fugas de memoria en la biblioteca FreeType, acumulación de memoria no gestionada por Rust.
- **Fix propuesto:** Asegurar que todos los constructores verifiquen errores y llamen a las funciones de limpieza correspondientes (`FT_Done_Face`, `FT_Done_FreeType`) en todos los caminos de error. Usar `defer!` o patrón `Guard` para liberación automática.
- **Severidad:** P0 — leak crítico en paths de error, pero raro en producción (normalmente los archivos existen y la fuente es válida).
- **Auditoría:** 2026-04-23
- **Estado:** ABIERTO

### TD-MEM-27: LLM chat panel sin límites de memoria
- **Archivos:** `src/llm/chat_panel.rs`
- **Descripción:** El `ChatPanel` mantiene un `messages: Vec<ChatMessage>` con `MAX_MESSAGES = 200`, pero el `streaming_buf` y el `wrapped_cache` pueden acumular memoria sin control si el usuario mantiene conversaciones largas o adjunta archivos grandes. `estimated_tokens()` divide chars/4 pero no hay límite real de memoria.
- **Root cause:** No hay límite de memoria total para el panel (ej. 10MB). Archivos adjuntos pueden ser grandes (hasta 512KB cada uno, 1MB total documentado en archive TD-030, pero sin límite de memoria activa).
- **Impacto:** Consumo de memoria lineal con la duración de la sesión, especialmente problemático cuando se adjuntan archivos grandes o conversaciones largas.
- **Fix propuesto:** Implementar un límite de memoria total para el panel (ej. 10MB) y descartar mensajes antiguos cuando se exceda. Opcional: truncar `streaming_buf` cuando supere un umbral.
- **Severidad:** P0 — impacto medible en sesiones largas, pero no bloqueante.
- **Auditoría:** 2026-04-23
- **Estado:** ABIERTO

### TD-PERF-38: PTY buffer overflow sin backpressure efectivo
- **Archivos:** `src/term/pty.rs`
- **Descripción:** El canal `crossbeam_channel::bounded::<PtyEvent>(256)` tiene un tamaño fijo de 256 mensajes. Si el terminal genera salida rápida (ej. `ls -R`, `cat` de archivo grande), el buffer se llena y los eventos se descartan con `pty_backpressure_hit`.
- **Root cause:** `try_send` devuelve `Err` silenciosamente cuando el canal está lleno. No hay mecanismo para notificar al proceso hijo que debe pausar la salida.
- **Impacto:** Pérdida de datos de salida del terminal, comportamiento inconsistente en comandos con mucha salida.
- **Fix propuesto:** Aumentar el tamaño del buffer (ej. 1024) O implementar un mecanismo de backpressure que notifique al proceso hijo para pausar la salida (ej. pausa/resume signals).
- **Severidad:** P2 — noticeable en cargas de trabajo intensas, pero workaround: usar `ls | head`.
- **Auditoría:** 2026-04-23
- **Estado:** ABIERTO

---

## Resolved 2026-04-18 — TD-OP-02 (3rd iteration): font_index threading

### TD-OP-02: FreeType cmap opens wrong face for .ttc collection fonts
- **Files:** `src/font/locator.rs`, `src/font/loader.rs`, `src/font/shaper.rs`, `benches/shaping.rs`
- **Root cause:** `locate_via_font_kit` discarded `font_index` from font_kit's `Handle::Path`, hardcoding `0`. `FT_New_Face` always opened face 0. `build_font_system` matched by path only, ignoring `FaceInfo.index`. For `.ttc` fonts with the target face at index > 0, FreeType and fontdb both used the wrong face, producing incorrect PUA glyph IDs.
- **Fix:** `locate_via_font_kit` now captures `font_index` from `Handle::Path`. `build_font_system` matches by path AND `face.index == font_location.index`. `FreeTypeCmapLookup::new` takes `face_index: u32` and passes it to `FT_New_Face`. `build_font_system` returns `(FontSystem, String, fontdb::ID, PathBuf, u32)`. `TextShaper::new` takes and threads `face_index: u32`.
- `#[allow(dead_code)]` on `FontPath.index` removed (field is now used).

---

## Resolved 2026-04-16 — Render P1 real fixes (TD-RENDER-01 real, TD-RENDER-03)

### TD-RENDER-01: Franjas de bg distintas en filas con texto vs sin texto (real root cause)
- **Archivos:** `src/app/renderer.rs:239` (`build_instances`)
- **Root cause:** `try_word_cached_shape` (`src/font/shaper.rs:573`) hace
  `text.split(' ')` y descarta los espacios — nunca emite un `ShapedGlyph` para
  ellos. Cualquier línea con un char disparador de ligaduras (`= < > - | + * / ~ ! : .`)
  cae por el word-cached path. `try_ascii_fast_path` sí emite glyphs para espacios,
  pero las líneas típicas de TUI (lazy.nvim, status bar, command line) contienen
  `:` o `.` y no van por el fast path.
- **Efecto:** en `build_instances` el bucle iteraba solo sobre `shaped.glyphs`.
  Celdas con espacio y `bg ≠ default_bg` (widgets flotantes de nvim, status bars,
  command line, selección, search highlight) quedaban sin vértice emitido.
  Resultado: el clear color del GPU se veía entre letras y entre filas → franjas
  horizontales y aspecto "intermitente" del bg.
- **Fix:** pre-pase en `build_instances` antes del bucle de glyphs. Itera `colors`
  y emite un vértice bg-only (`atlas_uv=0`, `glyph_size=0`) por cada celda con
  `!colors_approx_eq(bg, default_bg)`. El `bg_pipeline` pinta el rect completo de
  la celda; el `cell_pipeline` colapsa a área cero y no produce fragments.
- **NO confundir con el fix anterior:** el shader-level discard `if uv ≈ [0,0]`
  agregado previamente era defensivo y NO resolvía este bug — los vértices
  simplemente no se emitían.
- **Regression guard:** si alguien toca `build_instances` o el shaper y quita el
  pre-pase, el bug vuelve para cualquier línea con ligature-trigger chars.

### TD-RENDER-03: Celda blanca persistente donde estuvo el puntero del mouse
- **Archivos:** `src/app/input/mod.rs:32-37`, `src/app/mod.rs:991-1025`
- **Root cause:** `start_selection` en `LMB Pressed` crea un `Selection::new` con
  `start == end` (1 celda). `Selection::to_range()` retorna `Some(range)` no
  vacío; `cell_in_selection` matchea esa celda y el renderer invierte fg/bg → bg
  blanco con fg oscuro. Sin drag, nunca se limpiaba → celda blanca persistente.
- **Fix:** flag `mouse_dragged: bool` en `InputHandler`. Reset en `LMB Pressed`,
  set en `CursorMoved` cuando corre `update_selection`. En `LMB Released` sin
  drag → `terminal.clear_selection()` (solo si `mouse_mode_flags().0 == false`,
  para no pisar apps con mouse reporting propio).

---

## Resolved 2026-04-15 — Phase 3.5 PERF sprint (TD-PERF-06 through TD-PERF-13, TD-MEM-01/02/06/07)

### TD-MEM-01: `evict_cold()` en GlyphAtlas no reclaima espacio físico
- **Fix:** `evict_cold()` ahora llama a `clear()` + re-upload de entradas calientes cuando elimina >50% de las entradas. El espacio físico en la textura de 64 MiB se recupera efectivamente.

### TD-MEM-02: `LcdGlyphAtlas` sin evicción
- **Fix:** Epoch-based eviction añadida al `LcdGlyphAtlas` con el mismo patrón que `GlyphAtlas`. `last_used: u64` en `LcdAtlasEntry`, `next_epoch()`, `evict_cold(max_age)`. Cuando `upload()` falla con Full, llama a `evict_cold()` y reintenta.

### TD-MEM-04: `SwashCache` de cosmic-text — falso positivo de leak
- **Resolución:** El código usa `get_image_uncached()`, no `get_image()`. `SwashCache` nunca cachea en la ruta actual — cada llamada va directo al rasterizador sin acumulación. No hay leak de RAM. El consumo de 20 GB tenía otras causas (TD-MEM-07 + TD-MEM-09). Nota: usar `get_image_uncached` implica re-rasterizar el mismo glifo en cada frame sin cache; un LRU acotado sería una mejora de *performance*, no de memoria — candidato para un ítem TD-PERF futuro.

### TD-MEM-03: Bind groups stale tras atlas clear
- **Fix:** Re-creación de bind groups en el path de `clear()`. No había sección independiente; referenciado solo en la tabla de resumen.

### TD-MEM-05: `word_cache` miss storm periódico
- **Fix:** Resuelto. No había sección independiente; referenciado solo en la tabla de resumen.

### TD-MEM-06: `byte_to_col_buf` crece al tamaño máximo de línea visto y nunca se reduce
- **Fix:** `shrink_to((n+1)*2)` cuando `capacity > 4 × need`. Evita crecimiento permanente tras líneas excepcionalmente largas.

### TD-MEM-07: `messages` en `ChatPanel` crece indefinidamente
- **Fix:** `drain oldest` en `mark_done()` cuando `messages.len() >= MAX_MESSAGES`. `wrapped_cache[..drop]` se sincroniza en el mismo punto.

### TD-MEM-08: `terminal_shell_ctxs` leak por terminal cerrado
- **Fix:** Resuelto vía `Mux.closed_ids` pattern. No había sección independiente.

### TD-PERF-06: Doble rasterización de glifos cuando LCD AA está habilitado
- **Fix:** `rasterize_to_atlas` se omite cuando `lcd_entry.is_some()`. Solo emoji (que siempre devuelve `None` en LCD) sigue el path sRGB como fallback.

### TD-PERF-07: Invalidación global de row caches al evictar el atlas
- **Fix:** `clear_all_row_caches()` movido a la rama `clear()` únicamente. `evict_cold()` deja la textura intacta — los UVs siguen siendo válidos para entradas no evictadas.

### TD-PERF-08: `PresentMode::Fifo` con latencia máxima de 2 frames
- **Fix:** Ya implementado en el sprint: Mailbox → FifoRelaxed → Fifo con fallback. `desired_maximum_frame_latency = 1`.

### TD-PERF-09: Lectura síncrona de disco del shell context por cada evento PTY
- **Fix:** `terminal_shell_ctxs: HashMap<usize, (ShellContext, SystemTime)>` con mtime guard. `metadata().modified()` comparado antes de `read_to_string`; si coincide, se reutiliza el valor cacheado.

### TD-PERF-10: Cursor blink invalida el cache entero del panel de chat
- **Fix:** `build_chat_panel_instances` dividido en content cache (reconstruido solo cuando cambia el contenido) + filas de input reconstruidas frescas cada frame. El blink ya no marca el panel `dirty`.

### TD-PERF-11: Text search re-escanea el grid entero en cada tecla
- **Fix:** `filter_matches()` en `Mux` — búsqueda incremental cuando el query extiende el anterior (`new.starts_with(old)`): filtra los matches previos en lugar de re-escanear el grid completo.

### TD-PERF-12 (kiro): Allocaciones repetidas en `push_shaped_row`
- **Fix:** `scratch_chars`, `scratch_str`, `scratch_colors` movidos a `RenderContext`. `push_shaped_row` los toma con `mem::take`, reutiliza la capacidad y los devuelve.

### TD-PERF-13 (kiro): `format!` spam en `build_chat_panel_instances` + spinner O(n)
- **Fix:** `scratch_lines: Vec<(String, [f32;4])>` en `RenderContext` con reuse index-based por `push_str`. Spinner cambiado de `chars().count() % 8` (O(n)) a `frame_counter / 4 % 8` (O(1)); `frame_counter: u64` incrementado en `RedrawRequested`.

---

## Resolved 2026-04-10 — Full debt sprint (P1/P2/P3)

### TD-OP-01: `unsafe impl Send` en `TextShaper`
- **Fix:** Eliminado. `cargo check` pasa sin errores — el compilador nunca requirió `Send` en el path real (winit event loop en macOS no exige `Send` en el handler).

### TD-OP-02: Override de glyph ID de Nerd Font frágil
- **Fix:** `TextShaper.primary_face_ids: HashSet<fontdb::ID>` reemplaza `font_id` único. En `new()`, todos los IDs de la familia primaria se recopilan con comparación case-insensitive. La condición de override usa `!primary_face_ids.contains(&glyph.font_id)`.

### TD-OP-03: Atlas de glyphs sin eviction LRU
- **Fix:** La infraestructura ya existía. Gap: `atlas.get` no actualizaba `last_used` en cache hits. Cambiado a `atlas.get_and_touch`. Eliminado `atlas.get` (dead code).

### TD-PERF-01: `ShellContext::load()` — 60 file reads/segundo
- **Fix:** `App.cached_exit_code: Option<i32>`, refrescado solo en `about_to_wait` cuando `more_data == true`.

### TD-PERF-02: `active_cwd()` — 60 syscalls `proc_pidinfo`/segundo
- **Fix:** `App.cached_cwd: Option<PathBuf>`, refrescado en PTY data y en cambio de terminal/tab. Helper `refresh_status_cache()`.

### TD-PERF-04: `dirty_rows` dead code
- **Fix:** `RowCache.dirty_rows`, `mark_all_rows_dirty()`, `reset_row_dirty_flags()` eliminados. El cache usa hash-based invalidation y nunca leyó estos campos.

### TD-PERF-05: `word_wrap()` múltiples veces por frame
- **Fix:** `ChatPanel.wrapped_cache: Vec<Vec<String>>` + `wrapped_cache_width`. `ensure_wrap_cache(width)` llamado una vez por dirty rebuild.

### TD-PERF-06: `panel_instances_cache` usa `to_vec()` en rebuild
- **Fix:** `clear() + extend_from_slice` — el Vec retiene capacidad entre frames.

### TD-PERF-07: `process_cwd()` Vec<u8> 1024 bytes
- **Fix:** `std::slice::from_raw_parts(vip_path.as_ptr() as *const u8, 1024)`. Cero heap allocation.

### TD-PERF-08/09/10: Scroll/tab/status bar sin cache
- **Fix:** Caches de instancias GPU en `RenderContext` con key hash. `extend_from_slice` en idle.

### TD-PERF-11: `char_chunks()` Vec<char> intermedio
- **Fix:** Loop directo sobre `s.chars()` con `String::with_capacity(width)` + `mem::take`.

### TD-PERF-12: `collect_grid_cells_for()` N allocs/frame
- **Fix:** Signature cambiada a `(&self, id, buf: &mut Vec<...>)`. Buffer en `RenderContext.cell_data_scratch`. `mem::take` en `build_all_pane_instances` para split de borrow.

### TD-PERF-13: `byte_to_col` Vec por cache miss
- **Fix:** `TextShaper.byte_to_col_buf: Vec<usize>`. `resize(n+1, 0)` + fill in-place.

### TD-PERF-14: `colors_scratch` capacidad 256 hardcoded
- **Fix:** `Vec::with_capacity(cell_data.first().map(|(_, c)| c.len()).unwrap_or(256))`.

### TD-PERF-15: Separadores de pane emiten N instancias
- **Fix:** `build_pane_separators` emite 1 `RoundedRectInstance` por separador (radius=0). Recibe `pad_x`/`pad_y` para coordenadas físicas.

---

## Resolved 2026-04-09 (batch 3)

### TD-047: Sin padding entre terminal y status bar
- **File:** `src/app/mod.rs` `status_bar_height_px()`
- **Fix:** `const SB_PAD_PX: f32 = 4.0` — `status_bar_height_px()` retorna `cell_h + SB_PAD_PX` en lugar de `cell_h`. El `viewport_rect().h` se reduce automáticamente en 4px, dejando una franja de 4px (cubierta con `bg_color` = fondo del terminal) entre el último row del grid y el status bar. El PTY nunca renderiza en esa franja.

### TD-046: Status bar no indica modo resize
- **Files:** `src/ui/status_bar.rs`, `src/app/mod.rs`
- **Fix:** `StatusBar::build` recibe un nuevo parámetro `leader_resize_mode: bool`. Se añade `BG_LEADER_RESIZE = [1.00, 0.72, 0.22, 1.0]` (naranja Dracula). Cuando `leader_active && modifiers.alt_key()`, el segmento muestra " RESIZE " en naranja. Calculado inline en el render loop (sin campo en `InputHandler`).

---

## Resolved 2026-04-09 (batch 2)

### TD-045: Keyboard pane resize no funcionaba (Option+Arrow en macOS)
- **File:** `src/app/input/mod.rs`
- **Bug:** En macOS, `Option+Arrow` puede llegar como `Key::Character` en lugar de `Key::Named`, por lo que el match solo sobre `logical_key` nunca encontraba la dirección.
- **Fix:** Añadir imports `PhysicalKey`, `KeyCode`; en el bloque `if alt`, hacer match primero sobre `logical_key` y, si no es `Named`, hacer fallback a `physical_key` (siempre refleja la tecla física sin transformar).

### TD-044: Mouse separator drag — zona de hit ±3px demasiado pequeña
- **File:** `src/app/mod.rs` `separator_at_pixel`
- **Fix:** Umbral aumentado de ±3.0 a ±8.0 px físicos en ambas ramas (vertical/horizontal). El comentario del doc también fue actualizado.

### TD-043: AI panel input — texto en fila incorrecta (regresión de TD-041)
- **File:** `src/app/renderer.rs` ~l.709
- **Bug:** El fix de TD-041 dejó `vis1 = ""` siempre que `n==1`, moviendo el texto a la fila sin marcador `►`.
- **Fix:** `let (vis1, vis2) = if n >= 2 { (lines[n-2], lines[n-1]) } else { (lines[0], String::new()) }` — cuando `n==1`, el texto va en la fila con `►` y la segunda fila queda vacía.

---

## Resolved 2026-04-09

### TD-042: Pane resize — keyboard + mouse drag
- **Files:** `src/ui/panes.rs`, `src/app/mux.rs`, `src/app/input/mod.rs`, `src/app/mod.rs`
- **Keyboard:** `PaneManager::adjust_ratio(focused_id, dir, delta)` — depth-first ancestor search finds the nearest Split whose `SplitDir` matches the arrow axis, adjusts `ratio ±0.05`, re-layouts. `Mux::cmd_adjust_pane_ratio` wired from leader dispatch (`<leader>+Option+←→↑↓`). `pane_ratio_adjusted` flag triggers `resize_terminals_for_panel` after key event.
- **Mouse:** `SeparatorDragState { is_vert, key }` in `InputHandler`. `App::separator_at_pixel` detects ±3px hit. `Left::Pressed` starts drag; `CursorMoved` calls `drag_split_ratio` → `drag_separator` → `resize_terminals_for_panel` live; `Left::Released` finalises.
- **Known remaining bugs:** TD-043 (AI input regression), TD-044 (mouse hit area too small), TD-045 (keyboard not triggering), TD-046 (no resize-mode indicator).
- **Polish (2026-04-09):** Keyboard resize now uses `resize_mode: bool` — hold Option to keep resizing, release to stop (cleared in `ModifiersChanged`). Mouse drag bug fixed: separator identified by `node_id: u32` (stable atomic counter on `PaneNode::Split`) instead of col/row — col/row changed after every `layout()` call, breaking subsequent drags. Status bar shows RESIZE during both keyboard (`resize_mode`) and mouse (`dragging_separator.is_some()`) resize.

### TD-041: Chat panel input row — display duplicado (fix 2)
- **File:** `src/app/renderer.rs` ~l.709
- **Bug:** `n.saturating_sub(2)` y `n.saturating_sub(1)` eran ambos `0` cuando `n==1` → ambas filas mostraban el mismo texto.
- **Fix:** `vis1 = if n >= 2 { input_lines[n-2] } else { String::new() }` — cuando `n==1`, fila con ► queda vacía y el texto va en la fila inferior.
- **Regression:** TD-043 — el texto debería ir en la fila con `►` (vis1), no en vis2.

### TD-033: Fallback stream filtra mensajes tool inválidos
- **File:** `src/llm/providers/` + `src/app/ui.rs`
- **Fix:** Fallback stream tras agotar tool rounds filtra `role:"tool"` y mensajes assistant con content vacío (solo `tool_calls`) antes de enviar al LLM.

### TD-030: File attachment size cap
- **File:** `src/llm/chat_panel.rs`
- **Fix:** 512 KB/archivo y 1 MB total; nota de truncado añadida al contexto del system message.

### TD-029: cwd.canonicalize() en macOS
- **File:** `src/app/ui.rs` (`submit_ai_query`)
- **Fix:** `cwd.canonicalize()` llamado una vez antes del spawn para resolver `/var` → `/private/var` en macOS.

### TD-031: EXPORT_REGEX + AUTH_REGEX como LazyLock
- **File:** `src/llm/shell_context.rs`
- **Fix:** `LazyLock<Regex>` estático — compilados una vez por proceso.

### TD-035: Doble get_mut en render loop
- **File:** `src/app/renderer.rs`
- **Fix:** Dos `get_mut` separados (dirty marking + store) fusionados en uno solo.

### TD-036: Hot-reload lee archivo completo para extraer versión
- **File:** `src/config/watcher.rs` o similar
- **Fix:** `update_managed_configs` lee solo los primeros 256 bytes para extraer el tag de versión.

### TD-037: undo_stack sin límite
- **File:** `src/llm/chat_panel.rs`
- **Fix:** Limitado a 10 entradas con política FIFO.

### TD-038: Errores LLM sin contexto accionable
- **File:** `src/llm/providers/`
- **Fix:** 401 → API key, 429 → rate limit, 404 → modelo no encontrado, 500 → server error, context length.

### TD-034: run_command sin indicador de riesgo
- **File:** `src/app/ui.rs` + renderer
- **Fix:** Indicador ⚠ ámbar para patrones destructivos (`rm -rf`, `dd`, `curl|sh`, etc.) en confirmación.

---

## Resolved 2026-04-08

### TD-026: Status bar — GPU-rendered segmented bar
- **File:** `src/ui/status_bar.rs`, `src/app/mod.rs`, `src/app/renderer.rs`
- **Implementation:** `StatusBar::build(...)` produces left/right segments rendered by GPU. Left: leader-mode indicator, CWD, git branch. Right: exit code, date/time. Git branch polled async con 5s TTL cache. Toggle via `ToggleStatusBar` palette action. Phase 3 P2 complete.

### TD-027: Tab rename via `<leader>,`
- **Files:** `src/ui/tabs.rs`, `src/app/ui.rs`, `src/app/input/mod.rs`, `src/app/renderer.rs`
- **Implementation:** Inline rename prompt en el tab pill activo. El texto reemplaza el título con cursor `▌`; Enter confirma, Esc cancela. `TabManager::rename_active()` aplica la etiqueta.

---

## Resolved 2026-04-07

### TD-025: Mouse tab-bar click omitía resize
- **File:** `src/app/mod.rs`
- **Bug:** `switch_to_index()` sin `resize_terminals_for_panel()` → PTY del tab nuevo mantenía row count anterior; contenido se desbordaba debajo del área visible.
- **Fix:** `resize_terminals_for_panel()` añadido tras `switch_to_index()` en el hit handler del tab bar.

### TD-028: Trackpad scroll muy lento en Retina
- **File:** `src/app/mod.rs`
- **Bug:** `MouseScrollDelta::PixelDelta.y` está en puntos lógicos; se dividía por `cell_h` en px físicos → ~0.5 líneas/evento en 2× Retina.
- **Fix:** Dividir por `cell_h / scale_factor`. Auto-scroll al fondo en cada keypress (`scroll_to_bottom()` antes de `write_input`).

---

## Resolved 2026-04-06

### TD-022: 36 clippy violations
- **Fix:** `cargo clippy --all-targets --all-features -- -D warnings` ahora pasa limpio. 36 lints corregidos.

### TD-021: title_bar_style + llm.ui.width_cols no propagados
- **Fix:** `title_bar_style` parseado desde Lua. `llm.ui.width_cols` propagado a todos los nuevos `ChatPanel`.

### TD-020: rewire_llm_provider en hot-reload
- **Fix:** `check_config_reload()` y `ReloadConfig` palette action llaman `rewire_llm_provider()`.

### TD-019: submit_ai_query panel_id race
- **Fix:** `panel_id` capturado antes del spawn; todos los AI events tageados; `poll_ai_events()` enruta correctamente.

### TD-018: cmd_split mutaba pane tree antes de crear terminal
- **Fix:** `Terminal::new()` se llama primero; pane tree solo se muta si tiene éxito.

### TD-017: cmd_close_tab dejaba terminal slots activos
- **Fix:** Cada `terminal_id` de la tab se pone a `None` antes de `panes.remove`.

### TD-OP-02: is_pua() subranges redundantes
- **Fix:** Subranges redundantes eliminados; bloque BMP PUA cubre todos los Nerd Font icons.

### TD-OP-03: GlyphAtlas sin evicción
- **Fix:** Atlas 4096×4096 con epoch-based LRU eviction.

### TD-OP-01: unsafe impl Sync en TextShaper
- **Fix:** `unsafe impl Sync` eliminado; `Send` mantenido con comentario SAFETY.

### TD-016: last_assistant_command() incluía tool-status lines
- **Fix:** Filtra líneas con `⟳`/`✓` antes de retornar el comando.

---

## Resolved 2026-04-05

### TD-015: Shift+Enter / Shift+Tab encoding
- **Fix:** Shift+Enter → `\x1b[13;2u`, Shift+Tab → `\x1b[Z`.

### TD-013: Rounded tab pills
- **Fix:** `RoundedRectPipeline` + SDF WGSL shader para pills redondeados.

### TD-014: Tab bar background
- **Fix:** Background hereda `config.colors.background`.

---

## Resolved 2026-04-03

### TD-042: Mouse Selection, Typing Delay, Font Memory
- **Files:** `src/term/mod.rs`, `src/app/mod.rs`, `src/app/renderer.rs`, `src/font/locator.rs`
- **Mouse selection:** `start_selection`/`update_selection` now lock the term once and subtract `display_offset` from the viewport row to anchor selections in buffer space. `MouseWheel` calls `update_selection` when button is held so dragging into scrollback works.
- **Typing delay:** `user_event` now checks `has_data` from `poll_pty_events()` and calls `request_redraw()` immediately when PTY output arrives instead of waiting for the next blink tick or mouse event.
- **Font memory:** Removed `locate_font_for_lcd` from per-frame `scaled_font_config()` (was cloning ~200 KB `JBM_REGULAR` bytes every frame). `locate_via_font_kit` now uses `select_best_match` instead of loading every font variant to find Regular weight.

### TD-040: Leader Key Action Dispatch System
- **Files:** `src/app/input/mod.rs`, `src/config/schema.rs`, `src/config/lua.rs`, `src/ui/palette/actions.rs`, `config/default/keybinds.lua`
- **Resolution:** `InputHandler::new(&Config)` builds `leader_map: HashMap<String, Action>` from `config.keys` filtered to `mods == "LEADER"`. `Action` gained `FromStr` and two new variants (`CommandPalette`, `ToggleAiPanel`). Lua parser now reads `config.leader` and `config.keys`. All custom keybinds moved to `keybinds.lua` as `LEADER` entries; hardcoded `Cmd+Shift+P/A`, `Ctrl+Shift+E/F`, `Cmd+T/W` removed. Adding a binding now requires only a Lua edit.
- **Default binds (Ctrl+B then…):** `p` palette · `a` AI panel · `e` explain · `f` fix · `t` new tab · `w` close tab · `%` split-H · `"` split-V · `x` close pane

---

## Resolved 2026-04-07

### TD-024: Leader+h/j/k/l Vim-Style Pane Focus Navigation
- **Files:** `src/ui/panes.rs`, `src/ui/palette/actions.rs`, `src/app/mux.rs`, `src/app/ui.rs`, `src/config/lua.rs`, `config/default/keybinds.lua`
- **Resolution:** `PaneManager::focus_dir(dir: FocusDir)` uses center-point geometry to find the nearest pane in the given direction. `Action::FocusPane(FocusDir)` variant added with `FromStr` entries `FocusPaneLeft/Right/Up/Down`. `Mux::cmd_focus_pane_dir()` dispatches to the active tab's pane manager. Default keybinds wired in `keybinds.lua` (config version bump 2→3). Palette entries added for all four directions.

### TD-023: setMovableByWindowBackground Already NO
- **Resolution:** Already applied at `src/app/mod.rs:203` (`Bool::NO`). Registry was stale. No code change needed.

---

## Resolved 2026-03-31

### TD-041: AI Panel Off-Screen + Broken GPU Upload
- **Files:** `src/app/mod.rs`, `src/app/renderer.rs`
- **Root cause 1:** `resize_terminals_for_panel()` was never called on panel open/close — panel rendered past the terminal right edge.
- **Fix:** Detect `is_panel_visible() != panel_was_visible` in `KeyboardInput` handler → call resize.
- **Root cause 2:** Dirty-row upload (`start = row_idx * cols`) broke when panel instances were appended after terminal rows — offsets didn't map.
- **Fix:** Full `upload_instances` when `is_panel_visible()` (same as palette).

---

## Resolved 2026-03-30

### TD-039: Manual ANSI Key Encoding
- **File:** `src/app/input/key_map.rs`
- **Implementation:** Created `translate_key` with xterm-compatible modifier encoding (Shift, Ctrl, Alt) for arrows, F-keys, and navigation keys.
- **Result:** Robust, extensible input handling following industry standards.

### TD-038: Hardcoded UI Constants
- **File:** `src/config/schema.rs`, `src/app/renderer.rs`, `config/default/llm.lua`
- **Implementation:** Introduced `ChatUiConfig` in the schema. Moved hardcoded colors and panel width to Lua (`llm.ui`). Added `parse_hex_linear` helper.
- **Result:** AI panel appearance fully customizable via Lua.

### TD-037: Incomplete Palette Actions
- **File:** `src/app/ui.rs`
- **Implementation:** Connected `Action::ExplainLastOutput` and `Action::FixLastError` in `handle_palette_action`.
- **Result:** Command palette correctly triggers AI context analysis.

### TD-036: Suboptimal Render Pass Architecture
- **File:** `src/renderer/pipeline.rs`, `src/renderer/gpu.rs`
- **Implementation:** Consolidated BG pass and Glyph pass into a single render pass using premultiplied alpha.
- **Result:** Improved GPU efficiency and reduced power consumption on Apple Silicon.

### TD-034: God Object Pattern in `App`
- **File:** `src/app/mod.rs` → split into `renderer.rs`, `mux.rs`, `ui.rs`, `input/mod.rs`
- **Implementation:** Decomposed 2000-line `App` into `RenderContext` (GPU), `Mux` (PTY/Tabs/Panes), `UiManager` (AI/Overlays), `InputHandler` (Keyboard/Mouse).
- **Result:** `App` is now a thin event coordinator. Drastically improved maintainability.

### TD-033: Atlas Stability & Eviction
- **File:** `src/renderer/atlas.rs`, `src/app/mod.rs`
- **Implementation:** `GlyphAtlas::upload` returns `AtlasError::Full`. Render catches this, clears both atlases and `RowCache`, and re-renders.
- **Result:** Terminal no longer crashes when atlas fills up.

### TD-032: High-Bandwidth GPU Instance Uploads
- **File:** `src/app/renderer.rs`, `src/renderer/gpu.rs`
- **Implementation:** Dirty-row tracking in `RowCache`. Partial buffer updates via offset — only changed rows uploaded.
- **Result:** ~95% reduction in GPU memory bandwidth per frame.

### TD-029: O(N²) Column Calculation during Shaping
- **File:** `src/font/shaper.rs`
- **Implementation:** `TextShaper::shape_line` uses incremental character counts for O(N) column derivation.
- **Result:** Faster shaping for long lines.

### TD-028: Redundant Text Shaping
- **File:** `src/app/renderer.rs`
- **Implementation:** Row-level `RowCache` with hash (text + colors). Cache hit skips re-shaping and re-rasterizing.
- **Result:** ~80% CPU reduction when terminal content is static.

### TD-018: Powerline Separator Fringing
- **File:** `src/renderer/pipeline.rs` (WGSL shaders)
- **Implementation:** Pixel snapping (`floor`) in vertex shader + manual blending for separator glyphs.
- **Result:** Clean Powerline/catppuccin-tmux separator rendering with no fringing.

### TD-005: PTY Thread JoinHandle Type-Erased
- **File:** `src/term/pty.rs`, `src/app/mod.rs`
- **Implementation:** `std::thread::JoinHandle<()>` + `Pty::shutdown()` sends `Msg::Shutdown` and joins. `App::Drop` calls `mux.shutdown()`.
- **Result:** No orphaned PTY threads on exit.

---

## Resolved 2026-03-27

### TD-025: Vertical Spacing Too Tight
- **File:** `src/config/schema.rs`, `src/font/shaper.rs`
- **Implementation:** `font.line_height` multiplier (default 1.2) applied in `TextShaper`.
- **Result:** Readable line spacing configurable via Lua.

### TD-012: Nerd Font Icons Overflow Cell
- **File:** `src/app/renderer.rs`
- **Implementation:** `clamp_glyph_to_cell()` crops `glyph_size` to cell bounds; Y-only clamping preserves JetBrains Mono ligature negative `bearing_x`.
- **Result:** Nerd Font row bleeding eliminated.

---

## Resolved 2026-03-24 and Earlier

### TD-021: Drag-and-Drop File Path Not Inserted
- `WindowEvent::DroppedFile`: panel focused → append to chat input; terminal focused → write path to PTY.

### TD-020: AI Block Response Not Rendered
- `build_chat_panel_instances` rewritten from scratch with `push_shaped_row` helper; panel rendered at `col_offset = term_cols`.

### TD-019: Space Key Not Forwarded in AI Input
- Explicit `Key::Named(NamedKey::Space)` handler in panel input routing.

### TD-017: Reverse-Video (SGR 7 / Flags::INVERSE) Not Applied
- Commit d70c00d: `cell.flags.contains(Flags::INVERSE)` swaps fg/bg in `collect_grid_cells`.

### TD-016: Ctrl Key Modifier Not Forwarded to PTY
- Commit d70c00d: Ctrl+key → `byte - b'a' + 1` mapping in `key_map.rs`.

### TD-013: Arrow Keys Ignore APP_CURSOR Mode (DECCKM)
- `APP_CURSOR` check in `translate_key`: normal → `\x1b[A`, app → `\x1bOA`.

### TD-011: Shell `exit` Does Not Close Terminal Window
- `PtyEvent::Exit` (mapped from `Event::ChildExit`) sets `shell_exited = true` → `event_loop.exit()`.

### TD-010: Nerd Font Icons Render as CJK Fallback Glyphs
- Bundled JetBrains Mono Nerd Font Mono (v3.3.0) as fallback; atlas packing preserves icon codepoints.

### TD-007: No Clipboard Integration
- `Cmd+C`: `terminal.selection_to_string()` → arboard. `Cmd+V`: arboard → PTY (bracketed paste aware).

### TD-006: No Mouse Event Handling
- SGR and X10 mouse report encoding; drag selection; scroll delta forwarding; `MOUSE_REPORT_CLICK/DRAG/MOTION` mode detection.

### TD-003: PTY cell_width/cell_height Hardcoded at 8×16
- Cell dimensions measured from shaped "M" glyph in `TextShaper::measure_cell`; passed to `TIOCSWINSZ`.

### TD-002: PTY Placeholder Event Proxy on Term Construction
- `Arc<OnceLock<Notifier>>` shared between `PtyEventProxy` and `Pty::spawn`. `direct_notifier` set once PTY loop is ready; `PtyWrite` forwarded immediately on background thread.

### TD-031: Insecure API Key Storage
- `LlmConfig::api_key` uses `secrecy::SecretString`; `#[serde(skip_serializing)]` prevents disk/log leakage; `expose_secret()` only at HTTP boundary.

### TD-030: Secret Leakage to LLM Provider
- `sanitize_command` in `ShellContext` redacts `export VAR=secret` and `Authorization:` headers via regex before injecting into system prompt.
