# Technical Debt Registry

**Last Updated:** 2026-05-05
**Open Items:** 0
**Critical (P0):** 0 | **P1:** 0 | **P2:** 0 | **P3:** 0 | **Deferred:** 2 | **Resueltos (Wave 1):** 4 | **Resueltos (Wave 2):** 5 | **Resueltos (Wave 3):** 2 | **Resueltos (Wave 4):** 3 | **Watch:** 1

> Resolved items are in [TECHNICAL_DEBT_archive.md](./TECHNICAL_DEBT_archive.md).

---

## Priority Definitions

| Priority | Definition | SLA |
|----------|------------|-----|
| P0 | Blocking development or causing incidents | Immediate |
| P1 | Significant impact on velocity or correctness | This sprint |
| P2 | Moderate impact, workaround exists | This quarter |
| P3 | Minor, address when convenient | Backlog |

---

## Grafo de dependencias

```
Wave 1 — Quick wins, independientes (hacer primero)
  AUDIT-PERF-01  ──────────────────────────────────────┐
  AUDIT-PERF-04  ──────────────────────────────────────┤
  AUDIT-REFAC-04 ──────────────────────────────────────┤─► Wave 2
  AUDIT-CLEAN-01 ──────────────────────────────────────┘

Wave 2 — Independientes, esfuerzo medio
  AUDIT-PERF-05  ──────────────────────────────────────┐
  AUDIT-MEM-01   ──────────────────────────────────────┤
  AUDIT-MEM-02   ──────────────────────────────────────┤─► Wave 3
  AUDIT-MEM-03   ──────────────────────────────────────┤
  AUDIT-PERF-03  ──────────────────────────────────────┘

Wave 3 — RESUELTA
  AUDIT-PERF-02  ──────────────────────────────────────┐
  AUDIT-ENERGY-01 ─────────────────────────────────────┤─► Wave 4

Wave 4 — RESUELTA
  AUDIT-REFAC-02  ──────────────────────────────────────┐
  AUDIT-REFAC-03  ──────────────────────────────────────┤
  AUDIT-REFAC-01  ──────────────────────────────────────┘

Watch
  AUDIT-CLEAN-02 (sin cambio; reevaluar si ContextAction crece)
```

**Conflictos a evitar:**
- `AUDIT-ENERGY-01` y `AUDIT-REFAC-03` tocan los mismos campos de `App` → commits separados.
- `AUDIT-PERF-02` y `AUDIT-REFAC-02` tocan `build_chat_panel_instances` → hacer PERF-02 primero.
- `AUDIT-REFAC-01` y cualquier cambio en `window_event()` → hacer REFAC-01 al final.

---

## P0 — Crítico

_Ninguno abierto._

---

## P1 — Alta prioridad

**AUDIT-PERF-01** — RESUELTO (2026-05-05). `FxHashSet` reemplaza `Vec::contains` en `push_md_line`. O(n) → O(1) por inserción. `src/app/renderer.rs:738`.

**AUDIT-PERF-02** — RESUELTO (2026-05-05). `build_chat_panel_instances` y `build_chat_panel_input_rows` reutilizan `fmt_buf` para header, picker, previews, zero-state, pills e input/hints, eliminando los `format!()` del hot path del panel.

**AUDIT-PERF-03** — RESUELTO (2026-05-05). `mcp_tools_cache: Vec<(String, Vec<String>)>` en `App`. Rebuilt lazily before `render_ctx` borrow; invalidated after `reload_mcp`. Zero-cost on sidebar frames (no BTreeMap, no alloc).

**AUDIT-ENERGY-01** — RESUELTO (2026-05-05). `App` ahora usa `needs_redraw: bool`; los handlers llaman `self.request_redraw()` y `about_to_wait()` hace un único `window.request_redraw()` por iteración via `flush_redraw_request()`.

---

## P2 — Prioridad media

**AUDIT-PERF-04** — RESUELTO (2026-05-05). `const HEADER_ACTIONS_COLS: usize = 12` en `chat_panel.rs`. Eliminado el `.map().sum()` por frame.

**AUDIT-PERF-05** — RESUELTO (2026-05-05). `parse_markdown` signature changed to `&mut ParseState` → `Vec<AnnotatedLine>`, eliminating the `streaming_fence_state.clone()`. `panel.input.clone()` deferred to cursor-on path only via `cursor_storage: String` + `&str` borrow.

**AUDIT-MEM-01** — RESUELTO (2026-05-05). Cap de 256 entradas en `terminal_shell_ctxs`: antes de insertar, evicts la entrada con `mtime` más antigua si `len() >= 256`.

**AUDIT-MEM-02** — RESUELTO (2026-05-05). `begin_frame()` runs shrink every 300 frames: `instances`, `lcd_instances`, `panel_instances_cache`, `rect_instances` → `shrink_to(len*2)` when `capacity > len*3`.

**AUDIT-MEM-03** — RESUELTO (2026-05-05). Same 300-frame pass in `begin_frame()`: `scratch_chars`, `scratch_colors`, `colors_scratch` use len*3 threshold; `scratch_str`, `fmt_buf` cap at 880 bytes (TYPICAL_COLS*4).

---

## P3 — Prioridad baja / Backlog

**AUDIT-REFAC-01** — RESUELTO (2026-05-05). `window_event()` delega en `handle_redraw()`, `handle_keyboard()`, `handle_mouse_motion()`, `handle_mouse_button()` y `handle_scroll()`, preservando el flujo del loop.

**AUDIT-REFAC-02** — RESUELTO (2026-05-05). `build_chat_panel_instances` quedó dividido en `build_panel_header()`, `build_panel_file_section()` y `build_panel_messages()`, manteniendo `build_chat_panel_input_rows()` como fase separada.

**AUDIT-REFAC-03** — RESUELTO (2026-05-05). Nuevo `SidebarState` en `src/ui/sidebar.rs`; `App` agrupa bajo `self.sidebar` el estado visual, navegación y scroll del sidebar.

**AUDIT-REFAC-04** — RESUELTO (2026-05-05). `#![allow(dead_code)]` global eliminado de `gpu.rs`. 5 métodos muertos eliminados: `has_lcd`, `take_lcd_atlas`, `queue`, `surface_format`, `is_lcd_ready`.

**AUDIT-CLEAN-01** — RESUELTO (2026-05-05). Función `idx_or_default<T>` añadida en `renderer.rs`. 7 ocurrencias de `.cloned().unwrap_or_default()` reemplazadas.

**AUDIT-CLEAN-02** — WATCH (2026-05-05). `ContextAction` sigue por debajo del umbral para justificar un dispatch table; reevaluar solo si el enum/match crece de forma material.

---

## Deferred — Requieren hardware/profiling específico

**TD-PERF-03** — DIFERIDO a Phase 2+. Dirty-rect GPU tracking solo aplica con GPUs discretas. En Apple Silicon unified memory, `write_buffer` es memcpy — no medible ni relevante.

**TD-PERF-05** — DIFERIDO a Phase 2+ (cross-platform). Atlas de glifos 64 MB de VRAM. Textura dinámica requiere soporte multi-plataforma GPU que no es objetivo actual.

---

## Guía activa

### REC-PERF-04: Medir antes de optimizar
Ningún fix P2/P3 debe implementarse sin profiling previo. El HUD F12 + benches criterion son las herramientas. Ver `term_specs.md §15` para frame budget targets.

### Orden de ejecución recomendado (estado actual)
``` 
Wave 1: COMPLETA
Wave 2: COMPLETA
Wave 3: COMPLETA
Wave 4: COMPLETA
Watch: AUDIT-CLEAN-02
```
