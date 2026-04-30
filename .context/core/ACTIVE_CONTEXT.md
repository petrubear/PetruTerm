# Active Context

**Current Focus:** Phase 6 — Warp UI (W-5 completa, próximo W-6)
**Last Active:** 2026-04-30

## Estado actual del proyecto

**W-1 → W-5 COMPLETAS en `feat/phase-6-warp-ui`.**
**Build limpio. Sin deuda abierta activa; diferidos: TD-PERF-03, TD-PERF-05, TD-PERF-29.**

## Roadmap Phase 6

- [x] W-1: Full-width message background tinting
- [x] W-2: Input box as a bordered card
- [x] W-3: Code block background + left accent bar
- [x] W-4: Sidebar active/inactive color contrast
- [x] W-5: Zero state / empty panel
- [ ] W-6: Header — icon anchor + right-aligned action buttons
- [ ] W-7: Prepared response pill buttons (post-response)
- [ ] W-8: Resizable panel width via mouse drag

## Archivos en scope (Phase 6)

- `src/app/renderer.rs` — `build_chat_panel_instances`, `build_chat_panel_input_rows`, `build_workspace_sidebar_instances`
- `src/app/mod.rs` — mouse handlers, `zero_state_hover_for_row`
- `src/llm/chat_panel.rs` — `ChatPanel` struct + state fields
- `.context/specs/warp_ui_improvements.md` — spec completo W-1..W-8

## Cambios W-5 a preservar

**Zero state layout** (renderer.rs ~línea 1094):
- `center = (history_start_row + sep_row) / 2`
- icon en `center-3` (`✦` ui_accent), subtitle en `center-1` ("Ask a question below" ui_muted)
- pills en `center+2` y `center+3` con patrón dos-rect (border + fill)
- `pill1_row = center+2`, `pill2_row = center+3` — debe mantenerse sincronizado con `zero_state_hover_for_row` en mod.rs

**Hover + click** (mod.rs):
- `zero_state_hover_for_row`: misma fórmula que renderer (center+2, center+3)
- Click en pill → pre-fill input + `submit_ai_query`

**Input card polish** (renderer.rs `build_chat_panel_input_rows`):
- `card_bg = panel_bg + 6%` (NO usar `ui_surface_active`)
- `sep_row` renderiza vacío — NO usar `separator_cache` (elimina la `│────...` ASCII art)

## Invariantes arquitectonicos clave (no romper)

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

**Copilot OAuth:**
Token almacenado en Keychain: `PetruTerm` / `GITHUB_COPILOT_OAUTH_TOKEN`.
