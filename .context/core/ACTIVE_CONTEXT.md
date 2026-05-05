# Active Context

**Current Focus:** Auditoría de código — refactoring, rendimiento, memoria, energía
**Last Active:** 2026-05-05

## Estado actual del proyecto

**Phases 1–6 COMPLETAS en `feat/phase-6-warp-ui`** → mergeado a master.
**Sin deuda técnica abierta. Diferidos: TD-PERF-03, TD-PERF-05 (solo GPUs discretas).**
**Todos los benches criterion funcionan. mimalloc activo como global allocator.**

## Completado en Phase 6

- [x] W-1: Full-width message background tinting
- [x] W-2: Input box as a bordered card
- [x] W-3: Code block background + left accent bar
- [x] W-4: Sidebar active/inactive color contrast
- [x] W-5: Zero state / empty panel
- [x] W-6: Header — icon anchor + right-aligned action buttons
- [x] W-7: Prepared response pill buttons (post-response)
- [x] W-8: Resizable panel width via mouse drag

## Rama activa: `audit/code-review`

Objetivo: auditoría sistemática del codebase completo buscando:
- Código repetido o candidato a refactor
- Optimizaciones de rendimiento (hot paths, asignaciones innecesarias)
- Optimizaciones de memoria (retención innecesaria, buffers sobredimensionados)
- Consumo de energía (trabajo innecesario en idle, polling)
- Aplicación de patrones de diseño donde corresponda
- Simplificación de código complejo

## Progreso de auditoría

**Wave 1 COMPLETA (2026-05-05):** AUDIT-PERF-01, AUDIT-PERF-04, AUDIT-REFAC-04, AUDIT-CLEAN-01
**Wave 2 COMPLETA (2026-05-05):** AUDIT-PERF-05, AUDIT-PERF-03, AUDIT-MEM-01, AUDIT-MEM-02, AUDIT-MEM-03

**Pendiente:**
- Wave 3: AUDIT-PERF-02 (25+ format! allocs en build_chat_panel_instances), AUDIT-ENERGY-01 (34 request_redraw() sin dedup)
- Wave 4: AUDIT-REFAC-02, AUDIT-REFAC-03, AUDIT-REFAC-01, AUDIT-CLEAN-02 (bloqueados por Wave 3)

## Invariantes arquitectónicos clave (no romper)

**Shaper drops space cells (TD-RENDER-01):**
Pre-pass bg-only en `build_instances` OBLIGATORIO. Sin él, celdas-espacio con bg != default_bg
no generan vértices → GPU clear color → franjas horizontales.

**damage-skip scratch buffer:**
`cell_data_scratch` es per-terminal. Siempre limpiar cuando cambia `terminal_id`.

**Blink fast path:**
`last_instance_count` + `last_overlay_start` en `RenderContext` OBLIGATORIOS.
Vértice cursor transparente (bg.a=0) para blink-off — no reducir cell_count.

**alacritty_terminal grid scrollback:**
`grid()[Line(row)]` NO cuenta `display_offset`. Usar `Line(row as i32 - display_offset)`.

**alacritty_terminal exit event:** `Event::ChildExit(i32)`, NO `Event::Exit`.

**PTY env vars obligatorias:** `TERM=xterm-256color`, `COLORTERM=truecolor`, `TERM_PROGRAM=PetruTerm`.

**SwashCache:** usar `get_image_uncached()`, NO `get_image()`.

**macOS trackpad:** `MouseScrollDelta::PixelDelta(pos).y` es LOGICAL POINTS.
Divisor: `cell_height / scale_factor`.

**JetBrains Mono ligatures:** bearing_x puede ser NEGATIVO — no clampar a 0.

**alacritty_terminal 1-cell selection:** limpiar con `clear_selection()` en click sin drag.
