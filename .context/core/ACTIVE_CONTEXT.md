# Active Context

**Current Focus:** Phase 3.5 P1 cerrado. Siguiente: P2 quick wins o Phase 4.
**Last Active:** 2026-04-16

## Estado actual del proyecto

**Phase 1–3 COMPLETE. Phase 3.5 PERF + P1 renders COMPLETO.**
**Todos los P1 cerrados y verificados por el usuario.**

---

## P1 resueltos esta sesión (2026-04-16)

### TD-RENDER-01 (real fix): Franjas horizontales bg en filas con texto
- **Archivos:** `src/app/renderer.rs:239` (`build_instances` pre-pase bg-only)
- **Root cause real:** `try_word_cached_shape` (`src/font/shaper.rs:573`) hace
  `text.split(' ')` y nunca emite glyphs para espacios. `try_ascii_fast_path` sí los
  emite, pero cualquier línea con `= < > - | + * / ~ ! : .` cae en el word-cached
  path. Celdas-espacio con `bg ≠ default_bg` (widgets nvim, status/command line,
  selección, search highlight) quedaban sin vértice → clear color del GPU → franjas.
- **Fix:** pre-pase en `build_instances` que emite un vértice bg-only por cada
  celda con `!colors_approx_eq(bg, default_bg)`, antes del bucle de glyphs. El
  shader-level discard introducido antes (`if uv ≈ [0,0] { discard; }`) era
  defensivo y NO solucionaba esto — los vértices simplemente no se emitían.

### TD-RENDER-03 (nuevo): Celda blanca persistente en posición del mouse
- **Archivos:** `src/app/input/mod.rs:32-37`, `src/app/mod.rs:991-1025`
- **Root cause:** `start_selection` en LMB press crea `Selection::new` con
  start==end (1 celda). `Selection::to_range()` retorna `Some(range)` no vacío; el
  renderer invierte fg/bg → bg blanco. Sin drag, nunca se limpiaba.
- **Fix:** flag `mouse_dragged: bool` en `InputHandler`; activado en `CursorMoved`
  cuando `update_selection` corre, reset en press. En `Released` sin drag llama
  `terminal.clear_selection()`. Skip si `mouse_mode_flags().0` (app maneja mouse).

---

## Phase 3.5 — Sprint PERF completo

### Memory leaks — todos los P1 resueltos

| ID | Fix | Estado |
|----|-----|--------|
| TD-MEM-01 | GlyphAtlas `cursor_fill_ratio()` + preemptive clear | RESUELTO |
| TD-MEM-02 | LcdGlyphAtlas epoch/evict + `clear_lcd_rasterizer_cache()` | RESUELTO |
| TD-MEM-03 | `GpuRenderer::rebuild_atlas_bind_groups()` tras atlas.clear() | RESUELTO |
| TD-MEM-04 | SwashCache — **falso positivo**, usa `get_image_uncached` | ARCHIVADO |
| TD-MEM-05 | `word_cache` HashMap → `lru::LruCache(1024)` | RESUELTO |
| TD-MEM-06 | `byte_to_col_buf` shrink condicional | RESUELTO |
| TD-MEM-07 | `ChatPanel.messages` cap 200 + drain `wrapped_cache` | RESUELTO |
| TD-MEM-08 | `Mux.closed_ids` drain → limpieza de `terminal_shell_ctxs` | RESUELTO |

### Performance — sprint resueltos

| ID | Fix | Estado |
|----|-----|--------|
| TD-PERF-06 | Skip `rasterize_to_atlas` cuando LCD atlas tiene hit | RESUELTO |
| TD-PERF-07 | `clear_all_row_caches()` solo en branch `clear()` | RESUELTO |
| TD-PERF-08 | `PresentMode::Mailbox` + `desired_maximum_frame_latency=1` | RESUELTO |
| TD-PERF-09 | mtime guard en `terminal_shell_ctxs` | RESUELTO |
| TD-PERF-10 | Split panel render: content cache + input rows vivos | RESUELTO ⚠ |
| TD-PERF-11 | Búsqueda incremental: `filter_matches()` extiende query | RESUELTO |
| TD-PERF-12 | Scratch buffers en `push_shaped_row` | RESUELTO |
| TD-PERF-13 | `scratch_lines` reuse + `frame_counter` spinner O(1) | RESUELTO |

> ⚠ TD-PERF-10 introdujo regresiones TD-RENDER-01/02.

### P1 cerrados

| ID | Descripción | Estado |
|----|-------------|--------|
| TD-RENDER-02 | Flickering spinner panel | RESUELTO |
| TD-RENDER-01 | Franjas bg filas con texto (shaper drops spaces) | RESUELTO |
| TD-RENDER-03 | Celda blanca persistente tras click sin drag | RESUELTO |
| TD-PERF-36 | Overflow silencioso en instance buffers | RESUELTO |

### Quick wins P2 para próxima sesión (post renders)

| ID | Archivo | Fix |
|----|---------|-----|
| TD-PERF-32 | `src/app/renderer.rs:191` | Mover `colors_scratch` a `RenderContext` |
| TD-PERF-20 | `src/app/renderer.rs:662,663,754` | `char_indices().nth(N)` zero-alloc |
| TD-PERF-19 | `src/app/ui.rs:265` | `git_branch_in_flight: bool` guard |
| TD-PERF-36 | `src/renderer/gpu.rs:20` | `MAX_RECT_INSTANCES` → 1 024 + `log::warn!` |

---

## Patrones arquitectónicos clave

**LcdGlyphAtlas + FreeTypeLcdRasterizer dualidad de cache:**
- `LcdGlyphAtlas.cache` — cache principal con epoch tracking
- `FreeTypeLcdRasterizer.cache` — cache LOCAL del rasterizador (Mutex interno)
- Al llamar `lcd_atlas.clear()`, el cache LOCAL también debe limpiarse via `clear_lcd_rasterizer_cache()`

**Atlas eviction física vs lógica:**
- `evict_cold()` es LÓGICA — elimina del HashMap pero el cursor no retrocede; UVs de entradas supervivientes siguen válidas
- Para reclamar espacio físico, la única opción es `clear()` (nueva textura)
- Detección: `cursor_fill_ratio() > 0.75` después de evicción → preemptive `clear()`

**`Mux.closed_ids` patrón:**
- `cmd_close_tab/pane()` pushean IDs a `Mux.closed_ids`
- App drena `closed_ids` tras `handle_key_input` y en `close_exited_terminals`
- Permite limpiar estado externo sin pasar `App` a `Mux`

**`RenderContext` scratch fields (TD-PERF-12/13):**
- `scratch_chars/str/colors`: usados por `push_shaped_row` via `mem::take`
- `scratch_lines: Vec<(String,[f32;4])>`: reutilizado por `build_chat_panel_instances`; strings sobreescritas in-place con `push_str`
- `frame_counter: u64`: incrementado en `RedrawRequested`; spinners = `(frame_counter/4)%8`
- `fmt_buf: String`: scratch de una sola línea para callers de `push_shaped_row`

**Panel content vs input rows (TD-PERF-10):**
- Content section (`build_chat_panel_instances`): solo reconstruye cuando `ChatPanel::dirty`
- Input rows (`build_chat_panel_input_rows`): reconstruidas cada frame (cursor blink, hint text)
- Blink solo toca input rows, no invalida cache de mensajes
- ⚠ Ruta de regresión: si coordenadas del content cache no incluyen `scroll_offset`, los bloques renderizan en posición incorrecta

## Keybinds actuales

| Tecla | Acción |
|-------|--------|
| `Cmd+C / Cmd+V` | Copy / paste |
| `Cmd+Q` | Quit |
| `Cmd+K` | Clear screen + scrollback |
| `Cmd+F` | Abrir/cerrar búsqueda de texto |
| `Cmd+1-9` | Cambiar a tab N |
| `^B c` | New tab |
| `^B &` | Close tab |
| `^B n/b` | Next/prev tab |
| `^B ,` | Rename active tab |
| `^B %` | Split horizontal |
| `^B "` | Split vertical |
| `^B x` | Close pane |
| `^B h/j/k/l` | Focus pane (vim-style) |
| `^B Option+←→↑↓` | Resize pane |
| `^B a` | Abrir / cerrar AI panel |
| `^B A` | Mover focus terminal ↔ chat |
| `^B e` | Explain last output |
| `^B f` | Fix last error |
| `^B z` | Undo last write |
| `^B o` | Command palette |
| `Ctrl+Space` | Inline AI block |
| Right-click | Context menu |
