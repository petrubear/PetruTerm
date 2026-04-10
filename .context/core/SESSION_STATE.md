# Session State

**Last Updated:** 2026-04-10
**Session Focus:** Technical debt sprint completo — P1, P2, P3, TD-OP-01/02/03

## Branch: `master`

## Session Notes (2026-04-10 — debt sprint)

### Resumen

Sprint de deuda técnica completado. Todos los ítems P1–P3 resueltos (18 en total en la sesión).
Solo queda TD-PERF-03 abierto (GPU upload en PCIe — no aplica en Apple Silicon).

### Ítems resueltos hoy

#### P1
- **TD-PERF-01**: `ShellContext::load()` — 60 file reads/seg → `App.cached_exit_code`
- **TD-PERF-02**: `active_cwd()` — 60 syscalls/seg → `App.cached_cwd`, `refresh_status_cache()`
- **TD-PERF-04**: `dirty_rows` dead code — eliminado
- **TD-PERF-05**: `word_wrap()` múltiples veces/frame → `ChatPanel.wrapped_cache`
- **TD-OP-02**: Nerd Font glyph ID override frágil → `primary_face_ids: HashSet<fontdb::ID>`

#### P2
- **TD-PERF-06**: `panel_instances_cache.to_vec()` → `clear() + extend_from_slice`
- **TD-PERF-07**: `process_cwd()` Vec<u8> 1024 bytes → `from_raw_parts` cast in-place
- **TD-PERF-08/09/10**: Scroll/tab/status bar sin cache → caches con key hash
- **TD-PERF-11**: `char_chunks()` Vec<char> → loop directo sin allocation intermedia
- **TD-OP-03**: Atlas sin eviction → `get_and_touch` actualiza `last_used` en cache hits

#### P3
- **TD-PERF-12**: `collect_grid_cells_for()` N allocs/frame → buffer reutilizable en `RenderContext.cell_data_scratch`
- **TD-PERF-13**: `byte_to_col` Vec por cache miss → `TextShaper.byte_to_col_buf` reutilizable
- **TD-PERF-14**: `colors_scratch` capacidad 256 → `Vec::with_capacity(cols)`
- **TD-PERF-15**: Separadores N CellVertex → 1 `RoundedRectInstance` por separador
- **TD-OP-01**: `unsafe impl Send for TextShaper` → eliminado (el compilador nunca lo requirió)

### Archivos modificados

| Archivo | Cambios |
|---------|---------|
| `src/app/mod.rs` | `sep_pad_x`, `sb_pad_y` pasados a `build_pane_separators`; `mem::take` para `cell_data_scratch` |
| `src/app/renderer.rs` | `cell_data_scratch` en `RenderContext`; `build_pane_separators` usa `RoundedRectInstance`; `colors_scratch` capacidad correcta |
| `src/app/mux.rs` | `collect_grid_cells_for` acepta buffer mutable; reusa allocaciones inner |
| `src/font/shaper.rs` | `primary_face_ids: HashSet`; `primary_font_id`; `byte_to_col_buf`; eliminado `unsafe impl Send` |
| `src/renderer/atlas.rs` | `get` eliminado (dead); `get_and_touch` ahora público sin `#[allow(dead_code)]` |
| `src/llm/chat_panel.rs` | `char_chunks` sin Vec<char> intermedio |
| `src/term/mod.rs` | `process_cwd` sin Vec<u8> heap allocation |
| `.context/quality/TECHNICAL_DEBT.md` | 18 ítems cerrados, 1 abierto |

## Build & Tests
- **cargo check:** PASS (2026-04-10, cero warnings, cero errores)

## Deuda técnica restante

**1 ítem** — TD-PERF-03 (GPU upload completo en PCIe). No aplica en Apple Silicon. Dejar para cross-platform (Phase 2+).

## Próxima sesión

**Phase 4:** Plugin ecosystem (Lua loader, API surface). Ver `build_phases.md`.
