# Active Context

**Current Focus:** Phase 4 — Plugin system (Lua, lazy.nvim-style)
**Last Active:** 2026-04-18

## Estado actual del proyecto

**Phase 1–3 COMPLETE. Phase 3.5 COMPLETE. Phase 4 (plugins) desbloqueada.**
**Build limpio: check + test + clippy + fmt PASS. CI verde.**

## Proximo trabajo — Phase 4

Implementar el sistema de plugins lazy.nvim-style en Lua:

1. `src/plugins/mod.rs` — PluginLoader: escanea `~/.config/petruterm/plugins/*.lua`,
   carga bajo demanda, gestiona ciclo de vida
2. `src/plugins/api.rs` — Lua API publica expuesta a plugins (toda funcion documentada aqui)
3. Integrar con el Lua VM existente (`mlua`) y config system

Ver `.context/specs/build_phases.md` Phase 4 para deliverables y exit criteria.

## Bugs resueltos en sesion 2026-04-18 (noche)

### Tab en blanco al cambiar tabs
- **Root cause:** damage-skip en `collect_grid_cells_for` saltaba filas del buffer recien limpiado.
- `src/app/mux.rs`: param `force_full: bool` en `collect_grid_cells_for`
- `src/app/mod.rs`: `force_full = terminal_changed` en `build_all_pane_instances`

### LLM error message + Apple Keychain
- `src/app/ui.rs`: `llm_init_error: Option<String>` — muestra error real de `build_provider`
- `src/llm/openrouter.rs`: `keychain_api_key()` — fallback 3 via `security` CLI de macOS
  - Almacenar: `security add-generic-password -s PetruTerm -a OPENROUTER_API_KEY -w <key>`

### AI panel focus keybind (leader+A con Shift no funcionaba)
- `leader+a` (minuscula) → `FocusAiPanel`: alterna focus terminal↔chat, abre si cerrado
- `Escape` en panel → quita focus sin cerrar (antes cerraba)
- `/q` en input → cierra el panel
- `config/default/keybinds.lua`: actualizado

## Bugs resueltos en sesion 2026-04-18 (tarde) — commit a5d691e

### KKP — Shift+Enter en apps modernas (Claude Code CLI, etc.)
- `src/term/mod.rs:97`: `kitty_keyboard: true`
- `src/app/input/key_map.rs:109`: Shift+Enter → `\x1b[13;2u` cuando DISAMBIGUATE_ESC_CODES

### Tab bleed — TUI app visible en todos los tabs
- `src/app/renderer.rs:53`: `scratch_terminal_id: Option<usize>` en `RenderContext`
- `src/app/mod.rs` (`build_all_pane_instances`): `cell_data_scratch.clear()` cuando terminal_id cambia

### .app bundle env vars — OPENROUTER_API_KEY invisible en Finder
- `src/main.rs`: `inherit_login_shell_env()` spawn `$SHELL -l -c 'env -0'` antes de threads

### CI clippy — manual_checked_ops
- `src/app/renderer.rs`: `.checked_div().unwrap_or(0)`

## Invariantes arquitectonicos clave (no romper)

**Shaper drops space cells (TD-RENDER-01):**
Pre-pass bg-only en `build_instances` OBLIGATORIO. Sin el, celdas-espacio con bg != default_bg
no generan vertices → GPU clear color → franjas horizontales.

**damage-skip scratch buffer:**
`cell_data_scratch` es per-terminal. Siempre limpiar cuando cambia `terminal_id`
(`build_all_pane_instances` en `src/app/mod.rs`). Si se quita, TUI app vuelve a sangrar.
`force_full=true` al limpiar para evitar tab en blanco.

**alacritty_terminal grid scrollback:**
`grid()[Line(row)]` NO cuenta `display_offset`. Usar `Line(row as i32 - display_offset)`.

**alacritty_terminal exit event:** `Event::ChildExit(i32)`, NO `Event::Exit`.

**PTY env vars obligatorias:** `TERM=xterm-256color`, `COLORTERM=truecolor`, `TERM_PROGRAM=PetruTerm`.

**SwashCache:** usar `get_image_uncached()`, NO `get_image()`.

**macOS trackpad:** `MouseScrollDelta::PixelDelta(pos).y` es LOGICAL POINTS.
Divisor: `cell_height / scale_factor`.

**JetBrains Mono ligatures:** bearing_x puede ser NEGATIVO — no clampar a 0.

**alacritty_terminal 1-cell selection:** limpiar con `clear_selection()` en click sin drag.
Ver `mouse_dragged` flag en `InputHandler`.

## Keybinds actuales

| Tecla | Accion |
|-------|--------|
| `Cmd+C / Cmd+V` | Copy / paste |
| `Cmd+Q` | Quit |
| `Cmd+K` | Clear screen + scrollback |
| `Cmd+F` | Abrir/cerrar busqueda |
| `Cmd+1-9` | Cambiar a tab N |
| `^F c` | New tab |
| `^F &` | Close tab |
| `^F n/b` | Next/prev tab |
| `^F ,` | Rename active tab |
| `^F %` | Split horizontal |
| `^F "` | Split vertical |
| `^F x` | Close pane |
| `^F h/j/k/l` | Focus pane (vim-style) |
| `^F Option+←→↑↓` | Resize pane |
| `^F a` | Abrir panel / alternar focus terminal↔chat |
| `Escape` (en panel) | Volver a terminal sin cerrar el panel |
| `/q` (en input panel) | Cerrar el panel |
| `^F e` | Explain last output |
| `^F f` | Fix last error |
| `^F z` | Undo last write |
| `^F o` | Command palette |
| `Ctrl+Space` | Inline AI block |
| Right-click | Context menu |

> Leader = `Ctrl+F`, timeout 1000ms
