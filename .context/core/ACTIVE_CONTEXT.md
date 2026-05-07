# Active Context

**Current Focus:** Post-Phase 7 — Tab color picker + mantenimiento paleta/menú
**Last Active:** 2026-05-07

## Estado actual del proyecto

**Phases 1–7 COMPLETAS.** master limpio, 10 commits ahead of origin.
**Sin deuda técnica abierta. Diferidos: TD-PERF-03, TD-PERF-05 (solo GPUs discretas).**

## Phase 7 — COMPLETA

| ID | Feature | Estado |
|----|---------|--------|
| H-1 | Hover links — URLs, paths, stack traces clicables | **COMPLETA** |
| B-1 | OSC 133 parser en VTE handler | **COMPLETA** |
| B-2 | Block manager por pane | **COMPLETA** |
| B-3 | Render visual de bloques | **COMPLETA** |
| B-4 | Operaciones sobre bloques (context menu, keybinds) | **COMPLETA** |
| A-1 | AI agent: schema de acciones + parser | **COMPLETA** |
| A-2 | AI agent: confirm UI inline | **COMPLETA** |
| A-3 | AI agent: action handlers | **COMPLETA** |
| I-1 | Input shadow buffer (depende de B-1) | **COMPLETA** |
| I-2 | Syntax coloring del comando | **COMPLETA** |
| I-3 | Ghost text — inline completion hints | **COMPLETA** |
| I-4 | Flag hints — tooltips de flags | **COMPLETA** |

## Notas de input decoration (I-1..I-4)

- `input_syntax_highlight` y `input_ghost_text` configurables en `ui.lua` (default: true).
- Con zsh-autosuggestions: poner ambos a `false` para evitar conflictos.
- Shadow se desactiva en Up/Down (history nav) y en ArrowRight/Tab en buf-end sin ghost aceptado (previene drift de `cmd_start_col`).

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
