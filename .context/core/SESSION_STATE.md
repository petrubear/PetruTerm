# Session State

**Last Updated:** 2026-05-11
**Session Focus:** Auditoría técnica — Waves 1–3 completas.

## Branch: `master`

## Estado actual

**Phases 1–7 COMPLETAS. master limpio.**
**Deuda técnica: Waves 1–3 resueltas. Watch: AUDIT-CLEAN-02. Diferidos: TD-PERF-03, TD-PERF-05.**

## Esta sesión (2026-05-11) — Auditoría Wave 1 + Wave 2

### Wave 1 — Riesgo y desperdicio inmediato (4 items)

**AUDIT-SEC-01** — Path traversal en `write_file` (`src/app/ui.rs`):
Canonicaliza el ancestro mas cercano existente y verifica `starts_with(cwd)` antes del dialogo de confirmacion. Soporta ficheros en directorios nuevos (no existentes aun).

**AUDIT-SEC-02** — MCP local sin trust gate:
- Nuevo `src/llm/mcp/trust.rs`: lista de cwds confiables en `~/.config/petruterm/mcp_trust.json`.
- `mcp/config.rs`: `load_global()` + `load_local()` separados; `load()` legacy eliminado.
- `UiManager::new()` y `reload_mcp()` solo inician `.petruterm/mcp.json` si `is_trusted(cwd)`.
- Palette: nueva accion "Trust local MCP config".

**AUDIT-ENERGY-02** — Battery poll infinito en desktop:
`battery_polled: bool` reemplaza `battery_status.is_none()` como guarda; poll cada 30 s, no cada iteracion.

**AUDIT-PERF-06** — `max_fps` muerto + `animation_fps` fantasma:
`flush_redraw_request` respeta `1_000_000_000/max_fps ns`; `about_to_wait` inyecta `frame_deadline` en `WaitUntil`. `animation_fps` eliminado de `perf.lua`.

### Wave 2 — Best in class (5 items)

**AUDIT-ENERGY-03** — Tokio + MCP eager cuando `llm.enabled = false`:
Runtime cambia a `current_thread`; bloque MCP omitido completamente.

**AUDIT-SEC-03** — Skills/steering locales sin trust gate:
`load(cwd, include_local: bool)` en ambos managers; se pasa `trust::is_trusted(&cwd)`.

**AUDIT-ENERGY-04** — Wakeups periodicos no gobernados por battery saver:
Git poll guard: 1 s normal / 60 s battery saver. `next_minute_wake` solo cuando `status_bar.enabled`. Battery poll condicionado a `window_focused`.

**AUDIT-THEME-01** — Status bar hardcoded Dracula:
`StatusBarColors` derivado de `ColorScheme::status_bar_colors()`. `StatusBar::build()` recibe `&StatusBarColors`. Constantes eliminadas de `status_bar.rs`.

**AUDIT-PERF-07** — Tab bar aloca `Vec<String>` por frame:
`tab_bar_titles: Vec<String>` → `tab_bar_titles_hash: u64` (FxHasher). Zero aloc en hot path.

## Esta sesión (2026-05-11) — Pane border padding + scrollbar gap

### Bugs corregidos

**Pane focus border overlaps text (top/right/bottom):**
`build_focus_border` solo empujaba el borde izquierdo fuera del contenido cuando `col_offset == 0`.
Los bordes right, bottom y top usaban solo `inset = border/2`, haciendo que el stroke se superpusiera
al texto de la primera/última fila/columna. Fix: agregar `pad_right`, `pad_bottom`, `pad_top: bool`
a `PaneInfo` (propagados desde `PanePad` en `collect_leaf_infos_impl`). Cuando la bandera es `false`
(borde en el edge del viewport, sin separator), el rect del borde se empuja un `cell_w`/`cell_h`
hacia afuera — los pixels fuera del viewport quedan GPU-clipped. Mismo patrón que ya existía para left.

**Scrollbar gap from pane border (split panes):**
Cuando `pad_right = true`, el pane tiene una "pad cell" entre el contenido y el separator. El scrollbar
usaba columna `term_cols - 1` (última columna de contenido), dejando esa pad cell vacía entre el scroll
track y el borde coloreado del pane (~`cell_w` de gap visible). Fix: cuando `pad_right = true`, el
scrollbar usa columna `term_cols` (la pad cell), quedando flush contra el separator/border.
`scroll_bar_state` actualizado para incluir `pad_right` en la cache key.

## Esta sesión (2026-05-07) — Paleta, menú nativo, tab color picker

### Paleta de comandos — auditoria y actualizacion
- Agregadas entradas faltantes: `OpenConfigFolder` y `ClearAiContext`.
- Locales actualizados: `palette.open_config_folder`, `palette.clear_ai_context` (en/es).

### Menú nativo (menu.rs)
- Agregado `Clear AI Context` en el submenu AI (entre "Undo Last Write" y el separador enable/disable).
- Locale `menu.clear_ai_context` agregado a `en.toml`.

### Tab color picker — COMPLETO
- `Tab.accent_color: Option<[f32; 4]>` en `src/ui/tabs.rs`.
- `TabManager::set_tab_color(idx, color)` y `active_accent(default)`.
- `ContextAction::SetTabColor(usize, Option<[f32; 4]>)` en `context_menu.rs`.
- `ContextMenuItem.swatch_color: Option<[f32; 4]>` — renderer dibuja `● ` en el color del swatch.
- `ContextMenu::open_tab_color_picker(tab_idx, brights, ...)` — 7 swatches de `colors.brights[1..7]` + Reset.
- Clic derecho en zona del tab bar → abre el color picker (no el menú normal).
- Tab activo: fondo + underline en `accent_color`. Tab inactivo con color: solo underline. Sin color: sin cambio.
- `build_focus_border` usa `tab_accent: Option<[f32; 4]>` — borde del pane sigue el color del tab activo.
- Cache del tab bar invalidado (`tab_bar_instances_cache.clear()`) al cambiar color.

## Esta sesión (2026-05-06) — Phase 7: A-1..A-3 + I-1..I-4

### A-1/A-2/A-3: AI Agent Actions — COMPLETAS
- `src/llm/agent_action.rs`: `AgentAction` enum (RunCommand, OpenFile, ExplainOutput) + parser de tags `<action>...</action>` en respuestas LLM.
- `PanelState::ConfirmAction(AgentAction)`: confirm UI inline con pills `[Run] [Cancel]` sobre sep_row. y/Enter confirma, n/Esc cancela.
- Handlers: RunCommand → write PTY, OpenFile → `open -e <path>`, ExplainOutput → captura N líneas del grid y hace nueva query LLM.
- "Always allow" checkbox: `always_allow_actions: bool` en ChatPanel; si activo las acciones se ejecutan sin confirmar.
- Post-ejecución: append mensaje de sistema al transcript.

### I-1: Input shadow buffer — COMPLETA
- `src/term/input_shadow.rs`: `InputShadow` mirrors keystrokes entre OSC 133-A y B.
- Tracks buf + cursor (byte offset). Resets en Ctrl+C/U/K/W/A/E, deactivate en CommandStart/End.
- Deactivate también en Up/Down (history navigation invalida el buffer).

### I-2: Syntax coloring — COMPLETA
- `src/term/tokenizer.rs`: `tokenize_command` + `build_syntax_fg`. `TokenKind`: Command, Arg, Flag, Pipe, Redirect, String.
- `CommandResolver`: lookup no-bloqueante en PATH con cache. Verde = válido, rojo = no encontrado, cian = flag, amarillo = string, naranja = pipe.
- `SyntaxOverlay` aplicado en `collect_grid_cells_for`. Configurable via `input_syntax_highlight`.
- Shadow se desactiva si ArrowRight/Tab en buf-end sin ghost aceptado (previene drift con zsh-autosuggestions).

### I-3: Ghost text — COMPLETA
- `HistoryIndex`: carga `~/.zsh_history` (formato extendido) o `~/.bash_history`, most-recent-first.
- `InputShadow.ghost: Option<String>` actualizado en cada keypress cuando cursor al final.
- `GhostOverlay` reemplaza chars + fg con `ui_muted` en el row del cursor.
- `accept_ghost()` en Tab/ArrowRight escribe el sufijo al PTY. Gateado tras `input_ghost_text` config para no interferir con zsh-autosuggestions.

### I-4: Flag hints — COMPLETA
- `src/term/flag_db.rs`: `lookup_flag(cmd, flag)` — 10 comandos, ~130 flags.
- `FlagHintOverlay` en cursor.row+1, alineado con la posición del flag, color `ui_muted`.
- Desaparece cuando el último token deja de ser Flag.

### Bugfixes input decoration
- **Bug 1**: `accept_ghost()` disparaba aunque `input_ghost_text=false` → escribía bytes extra al PTY rompiendo zsh-autosuggestions.
- **Bug 2**: Shadow cursor se desincronizaba de terminal cursor cuando zsh-autosuggestions aceptaba sugerencia (cmd_start_col incorrecto → colores en columnas erróneas).
- Fix: gate `accept_ghost` tras flag; deactivate shadow en Up/Down y en ArrowRight sin ghost aceptado.
- `input_syntax_highlight: bool` y `input_ghost_text: bool` ahora configurables en `ui.lua`.

## Esta sesión (2026-05-06) — B-4 bug fixes

### Bugs corregidos en B-4

**shell-integration.zsh — OSC 133 no se emitía:**
Añadidos: `D;$?` + `A` en `precmd`; `B;$1` + `C` en `preexec`.

**`Osc133Marker::CommandStart` — texto incluía el prompt:**
Comando viaja embebido en la secuencia: `ESC]133;B;<cmd>ST`.

**`block_at_absolute_row` — devolvía siempre el primer bloque:**
Cambiado a `iter().rev().find()`.

**Context menu — mezclado con menú original:**
Solo se abre al hacer clic derecho sobre el exit-code pill.

**block_at_cursor — hover solo en columna 0:**
Expandido para cubrir todo el ancho del pane.

**ClearBlock eliminado:** `ContextAction::ClearBlock`, `BlockManager::remove_block` removidos.

**Gutter bar eliminada:** 2px stripe izquierdo removido del renderer.

## Sesiones anteriores (resumen)

- 2026-05-05: Phase 7 H-1 + B-1 + B-2 + B-3 + B-4 + Auditoría Waves 1-4
- 2026-04-30: Phase 6 W-5..W-8
- 2026-04-29: Phase 6 W-1..W-4
- 2026-04-28: G-2-overlay
- 2026-04-26: Chat input UX (TD-UI-01/02/03)
- 2026-04-25: Phase 5 G-1 + G-2 + UX polish + G-0
- 2026-04-24: D-5 + /skills + /mcp + Leader+w + MCP fixes
- 2026-04-23: Focus border + sidebar pills + E + D-4
- 2026-04-22: C-3.5 + D-4
- 2026-04-21: C-1 bugs + C-2 + C-3
- 2026-04-20: C-1 inicial + B
- 2026-04-19: A + 3.6
