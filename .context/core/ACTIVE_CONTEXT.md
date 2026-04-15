# Active Context

**Current Focus:** **Phase 3.5 — Memory Leak Sprint** (continuación)
**Last Active:** 2026-04-15

## Estado actual del proyecto

**Phase 1–3 COMPLETE. Phase 3.5 EN PROGRESO — sprint de memory leaks activo.**

> Todas las features de Phase 1–3 verificadas. Ver `build_phases_archive.md` para el checklist completo.

## Memory Leaks — Estado post-sesión 2026-04-15

### Resueltos (5 de 8 P1)

| ID | Fix | Archivos |
|----|-----|----------|
| TD-MEM-01 | `cursor_fill_ratio()` + preemptive clear en evicción | `atlas.rs`, `app/mod.rs` |
| TD-MEM-02 | LcdGlyphAtlas epoch/evict + `clear_lcd_rasterizer_cache()` | `lcd_atlas.rs`, `freetype_lcd.rs`, `shaper.rs`, `app/mod.rs` |
| TD-MEM-03 | `GpuRenderer::rebuild_atlas_bind_groups()` tras atlas.clear() | `gpu.rs`, `app/mod.rs` |
| TD-MEM-05 | `word_cache` HashMap → `lru::LruCache(1024)` | `shaper.rs`, `Cargo.toml` |
| TD-MEM-08 | `Mux.closed_ids` drain → `terminal_shell_ctxs.remove()` | `mux.rs`, `app/mod.rs` |

### Pendientes P1

| ID | Descripción | Esfuerzo |
|----|-------------|----------|
| **TD-MEM-06** | `byte_to_col_buf` en `shaper.rs` no hace shrink tras líneas largas | Bajo — `shrink_to()` condicional |
| **TD-MEM-07** | `ChatPanel.messages` sin límite — historial crece sin fin | Medio — truncar a 200 msgs + limpiar `wrapped_cache` |

### Falso positivo (documentado)
- **TD-MEM-04**: SwashCache NO crece — el código usa `get_image_uncached`, no `get_image`. El atlas (64 MiB) es el cache acotado.

## Próximos pasos

1. **TD-MEM-06** — fix trivial, 15 min
2. **TD-MEM-07** — truncar historial de chat a N mensajes + shrink wrapped_cache
3. **P2 pendientes** de TD-MEM-09..19 (scrollback, file_picker, tokio tasks, ventana sin foco)
4. **Phase 3.5 performance** pendiente: TD-PERF-06 (doble rasterización LCD), TD-PERF-07 (reshape storm), TD-PERF-09 (shell context disk read)
5. **Phase 4 (plugins)** — bloqueado hasta Phase 3.5 exit criteria

## Cambios clave de esta sesión

### Nuevas dependencias
- `lru = "0.12"` en Cargo.toml

### Patrones arquitectónicos aprendidos

**LcdGlyphAtlas + FreeTypeLcdRasterizer dualidad de cache:**
- `LcdGlyphAtlas.cache: HashMap<u64, LcdCacheEntry>` — cache principal con epoch tracking
- `FreeTypeLcdRasterizer.cache: Mutex<HashMap<u64, LcdAtlasEntry>>` — cache LOCAL del rasterizador
- Cuando se llama `lcd_atlas.clear()`, el cache LOCAL también debe limpiarse via `clear_lcd_rasterizer_cache()`
- De lo contrario, el rasterizador devuelve UVs apuntando a la textura vacía/destruida

**Atlas eviction física vs lógica:**
- `evict_cold()` es LÓGICA — elimina del HashMap pero el cursor no retrocede
- Para reclamar espacio físico, la única opción es `clear()` (nueva textura)
- Detección: `cursor_fill_ratio() > 0.75` después de evicción → preemptive `clear()`

**Mux.closed_ids patrón:**
- `cmd_close_tab()` y `cmd_close_pane()` pusean a `Mux.closed_ids`
- App drena `closed_ids` después de `handle_key_input` y en `close_exited_terminals`
- Permite limpiar estado externo (terminal_shell_ctxs, chat_panels, etc.) sin pasar App a Mux

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
