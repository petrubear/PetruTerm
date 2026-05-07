# Active Context

**Current Focus:** Phase 7 — AI Agent Actions + Input Decoration
**Last Active:** 2026-05-06 (B-4 bugs corregidos)

## Estado actual del proyecto

**Phases 1–6 + Auditoría Waves 1–4 COMPLETAS.** Branch `audit/code-review` mergeado a master.
**Phase 7 completadas:** H-1 + B-1 + B-2 + B-3 + B-4.
**Sin deuda técnica abierta. Diferidos: TD-PERF-03, TD-PERF-05 (solo GPUs discretas).**

## Phase 7 — Plan de trabajo (simple → complejo)

Ver spec completo en [`.context/specs/build_phases.md`](../specs/build_phases.md).

| ID | Feature | Complejidad | Estado |
|----|---------|-------------|--------|
| H-1 | Hover links — URLs, paths, stack traces clicables | Baja | **COMPLETA** |
| B-1 | OSC 133 parser en VTE handler | Media | **COMPLETA** |
| B-2 | Block manager por pane | Media | **COMPLETA** |
| B-3 | Render visual de bloques | Media | **COMPLETA** |
| B-4 | Operaciones sobre bloques (context menu, keybinds) | Media | **COMPLETA** |
| A-1 | AI agent: schema de acciones + parser | Media-Alta | **COMPLETA** |
| A-2 | AI agent: confirm UI inline | Media-Alta | **COMPLETA** |
| A-3 | AI agent: action handlers | Media-Alta | **COMPLETA** |
| I-1 | Input shadow buffer (depende de B-1) | Alta | **COMPLETA** |
| I-2 | Syntax coloring del comando | Alta | **COMPLETA** |
| I-3 | Ghost text — inline completion hints | Alta | **COMPLETA** |
| I-4 | Flag hints — tooltips de flags | Alta | **COMPLETA** |

## Rama sugerida: `feat/phase-7`

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
