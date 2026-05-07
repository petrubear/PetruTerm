# Session State

**Last Updated:** 2026-05-07
**Session Focus:** Phase 7 — A-1..A-3 + I-1..I-4 COMPLETAS. Phase 7 cerrada.

## Branch: `master`

## Estado actual

**Phases 1–7 COMPLETAS. master limpio. 10 commits ahead of origin.**
**Sin deuda técnica abierta. Diferidos: TD-PERF-03/05 (solo GPUs discretas).**

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
