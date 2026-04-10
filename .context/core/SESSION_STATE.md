# Session State

**Last Updated:** 2026-04-10
**Session Focus:** Bug fix status bar exit code (per-pane) + feature click para ver detalles

## Branch: `master`

## Session Notes (2026-04-10 — tarde)

### Resumen

Bug fix + feature en status bar: el exit code ahora es por pane y se puede hacer click para ver el comando que falló.

### Bug resuelto: exit code global → per-pane

**Problema:** `poll_pty_events()` devolvía sólo `bool has_data`, sin decir qué terminal disparó. Al hacer switch de pane, se leía el archivo global de shell context (último exit code de *cualquier* pane). El badge rojo se mostraba en todos los panes aunque sólo uno hubiera tenido el error.

**Fix:**
- `poll_pty_events()` ahora devuelve `(Vec<usize>, Vec<usize>)` — IDs con datos + IDs que salieron.
- `terminal_shell_ctxs: HashMap<usize, ShellContext>` en `App` — contexto por terminal_id.
- `update_terminal_shell_ctx(id)` se llama sólo cuando *ese* terminal dispara PTY. Asocia el contexto al terminal correcto incluso con el archivo global (sin cambiar la shell integration).
- `active_shell_ctx()` devuelve el contexto del pane activo.
- Shell integration reescrita para generar `shell-context-$$.json` (per-PID). `ShellContext::load_for_pid(pid)` con fallback al archivo global para retrocompatibilidad.

### Feature: click en exit code → popup con detalles

- Click en el badge rojo abre un context menu justo encima del status bar.
- Muestra: código de salida + comando que falló (truncado a 20 chars) + acción "Copy command".
- `ContextAction::Label` — fila no-interactiva para display de texto (dim, sin hover).
- `ContextAction::CopyLastCommand` — copia `last_command` al clipboard.
- `ContextMenu::open_exit_info(exit_code, last_command, col, term_rows, term_cols)`.

### Archivos modificados

| Archivo | Cambios |
|---------|---------|
| `scripts/shell-integration.zsh` | Escribe `shell-context-$$.json` (per-PID) en lugar del archivo global |
| `src/llm/shell_context.rs` | `context_file_path_for_pid()`, `load_for_pid()` con fallback global |
| `src/app/mux.rs` | `poll_pty_events()` → `(Vec<usize>, Vec<usize>)` |
| `src/app/mod.rs` | `terminal_shell_ctxs: HashMap`, `update_terminal_shell_ctx()`, `active_shell_ctx()`, click ExitCode |
| `src/ui/context_menu.rs` | `ContextAction::Label`, `ContextAction::CopyLastCommand`, `open_exit_info()`, `is_non_interactive()` |
| `src/app/renderer.rs` | Render de filas `Label` en context menu (dim, sin hover) |

## Build & Tests
- **cargo build:** PASS — 0 errores, 0 warnings (2026-04-10)

## Deuda técnica restante

**4 ítems abiertos** — TD-PERF-03, TD-PERF-04, TD-PERF-05, TD-MAINT-01. Ver `TECHNICAL_DEBT.md`.

## Próxima sesión

**Phase 4:** Plugin ecosystem (Lua loader, API surface). Ver `build_phases.md`.
