# Session State

**Last Updated:** 2026-04-08
**Session Focus:** Bug fixes + UX improvements (chat panel, context menu, keybinds)

## Branch: `master`

## Session Notes (2026-04-08)

### /quit solo cierra el panel (RESUELTO)
- Bug: `/quit` en el chat panel llamaba `cmd_close_tab()` → cerraba todo el app si había un solo tab.
- Fix: `app/input/mod.rs` — eliminar `mux.cmd_close_tab()` del handler de `/quit`. Ahora solo cierra el panel.

### System prompt del chat panel demasiado restrictivo (RESUELTO)
- Bug: el modelo rechazaba preguntas generales con "My purpose is to help with file system tasks".
- Fix: `app/ui.rs` — system prompt ampliado: "helpful AI assistant embedded in PetruTerm... can answer any question".

### Context menu: separador + "Ask AI" (NUEVO)
- `src/ui/context_menu.rs`: `ContextAction::Separator`, `is_separator()`, ancho 22→24.
- Items: Copy / Paste / Clear / ─── / Ask AI.
- `src/app/renderer.rs`: separadores renderizan como `────` en gris, sin hover.
- `src/llm/chat_panel.rs`: nuevo `set_input(text)` — carga texto programáticamente.
- `src/app/mod.rs`: `ContextAction::SendToChat` → obtiene selección, abre panel si cerrado, carga texto en input.

### Ctrl+B A — focus terminal ↔ chat sin cerrar (NUEVO)
- Problema: un solo keybind no puede manejar abrir/cerrar Y mover focus.
- Solución: dos keybinds separados.
  - `Ctrl+B a` → `ToggleAiPanel`: abre / cierra el panel.
  - `Ctrl+B A` → `FocusAiPanel`: mueve focus terminal ↔ chat (panel queda abierto).
- `src/ui/palette/actions.rs`: nuevo `Action::FocusAiPanel` + `from_str`.
- `src/app/ui.rs`: handler `FocusAiPanel`; `ToggleAiPanel` revertido a open/close puro.
- `src/config/lua.rs`: `FocusAiPanel` expuesto en `petruterm.action`.
- `config/default/keybinds.lua`: keybind `A` registrado.

## Build & Tests
- **cargo build:** PASS (2026-04-08)
- **cargo test:** 16/16 PASS (no se modificaron tests)
- **cargo clippy:** pendiente verificar

## Session anterior (2026-04-07 — bug fixes TD-025/TD-028)
Ver historial git para detalles.
