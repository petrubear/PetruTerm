# Active Context

**Current Focus:** **Phase 3.5 — Performance Sprint** (en curso)
**Last Active:** 2026-04-15

## Estado actual del proyecto

**Phase 1–3 COMPLETE. Phase 3.5 (memory + perf) EN PROGRESO.**

> Todas las features de Phase 1–3 verificadas. Phase 4 (plugins) bloqueada hasta Phase 3.5 exit criteria.

---

## Phase 3.5 — Resumen de progreso

### Memory leaks — todos los P1 resueltos

| ID | Fix | Estado |
|----|-----|--------|
| TD-MEM-01 | GlyphAtlas `cursor_fill_ratio()` + preemptive clear | RESUELTO |
| TD-MEM-02 | LcdGlyphAtlas epoch/evict + `clear_lcd_rasterizer_cache()` | RESUELTO |
| TD-MEM-03 | `GpuRenderer::rebuild_atlas_bind_groups()` tras atlas.clear() | RESUELTO |
| TD-MEM-04 | SwashCache — falso positivo, usa `get_image_uncached` | NO ES LEAK |
| TD-MEM-05 | `word_cache` HashMap → `lru::LruCache(1024)` | RESUELTO |
| TD-MEM-06 | `byte_to_col_buf` shrink condicional | RESUELTO |
| TD-MEM-07 | `ChatPanel.messages` cap 200 + drain `wrapped_cache` | RESUELTO |
| TD-MEM-08 | `Mux.closed_ids` drain → limpieza de `terminal_shell_ctxs` | RESUELTO |

### Performance — resueltos

| ID | Fix | Estado |
|----|-----|--------|
| TD-PERF-06 | Skip `rasterize_to_atlas` cuando LCD atlas tiene hit | RESUELTO |
| TD-PERF-07 | `clear_all_row_caches()` solo en branch `clear()`, no en `evict_cold()` | RESUELTO |
| TD-PERF-08 | `PresentMode::Mailbox` + `desired_maximum_frame_latency=1` | RESUELTO |
| TD-PERF-09 | mtime guard en `terminal_shell_ctxs` (evita disk read por frame) | RESUELTO |
| TD-PERF-10 | Split panel render: content cache + input rows vivos | RESUELTO |
| TD-PERF-11 | Búsqueda incremental: `filter_matches()` extiende query anterior | RESUELTO |
| TD-PERF-12 | Scratch buffers en `push_shaped_row` (`scratch_chars/str/colors`) | RESUELTO |
| TD-PERF-13 | `scratch_lines` reuse + `frame_counter` spinner O(1) | RESUELTO |

### Performance — próximos candidatos (P1 primero)

- **TD-PERF-20** (P2): Truncación con `chars().count()` en varios lugares — fix trivial con `char_indices().nth(N)`
- **TD-PERF-15** (P2): Clipboard (`arboard`) bloquea event loop en copy/paste grande
- **TD-PERF-14** (P2): Scroll bar como N `CellVertex` → 2 `RoundedRectInstance`
- **TD-PERF-16** (P2): Hash key de tab bar / status bar recalculado por frame
- **TD-PERF-17** (P2): Config hot-reload sin debounce
- **TD-PERF-18** (P2): Tokio pool `num_cpus` → 2 workers
- **TD-PERF-19** (P2): `poll_git_branch` sin guard de vuelo
- **TD-PERF-21** (P2): Palette fuzzy matcher no incremental
- **TD-PERF-22** (P2): Search highlight O(matches) por celda en render

### Memory P2 abiertos (menor urgencia)

- TD-MEM-09: Scrollback sin límite global (40-200 MB con muchos tabs)
- TD-MEM-10/11: `file_picker_items` no se limpia + `SkimMatcherV2` por frame
- TD-MEM-12: Tokio tasks de streaming LLM colgados al cerrar panel
- TD-MEM-19: Cursor blink + reloj + git poll corren con ventana sin foco

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
- `cmd_close_tab/pane()` pusean IDs a `Mux.closed_ids`
- App drena `closed_ids` en dos puntos: tras `handle_key_input` y en `close_exited_terminals`
- Permite limpiar estado externo sin pasar `App` a `Mux`

**`RenderContext` scratch fields (TD-PERF-12/13):**
- `scratch_chars/str/colors`: usados por `push_shaped_row` via `mem::take`
- `scratch_lines: Vec<(String,[f32;4])>`: reutilizado por `build_chat_panel_instances`; strings sobreescritas in-place con `push_str` para reusar capacidad
- `frame_counter: u64`: incrementado en `RedrawRequested`; spinners = `(frame_counter/4)%8`
- `fmt_buf: String`: scratch de una sola línea para callers de `push_shaped_row`

**Panel content vs input rows (TD-PERF-10):**
- Content section (`build_chat_panel_instances`): solo reconstruye cuando `ChatPanel::dirty`
- Input rows (`build_chat_panel_input_rows`): reconstruidas cada frame (cursor blink, hint text)
- Blink solo toca input rows, no invalida cache de mensajes

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
