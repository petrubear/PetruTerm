# Active Context

**Current Focus:** Phase 4 — Plugin system (Lua, lazy.nvim-style)
**Last Active:** 2026-04-18

## Estado actual del proyecto

**Phase 1–3 COMPLETE. Phase 3.5 COMPLETE. Phase 4 (plugins) desbloqueada.**
**Build limpio: check + test + clippy + fmt PASS. CI verde.**

## Próximo trabajo — Phase 4

Implementar el sistema de plugins lazy.nvim-style en Lua:

1. `src/plugins/mod.rs` — PluginLoader: escanea `~/.config/petruterm/plugins/*.lua`,
   carga bajo demanda, gestiona ciclo de vida
2. `src/plugins/api.rs` — Lua API pública expuesta a plugins (toda función documentada aquí)
3. Integrar con el Lua VM existente (`mlua`) y config system

Ver `.context/specs/build_phases.md` Phase 4 para deliverables y exit criteria.

## Bugs resueltos en sesión 2026-04-18 (tarde) — commit a5d691e

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

## Invariantes arquitectónicos clave (no romper)

**Shaper drops space cells (TD-RENDER-01):**
Pre-pass bg-only en `build_instances` OBLIGATORIO. Sin él, celdas-espacio con bg != default_bg
no generan vértices → GPU clear color → franjas horizontales.

**damage-skip scratch buffer:**
`cell_data_scratch` es per-terminal. Siempre limpiar cuando cambia `terminal_id`
(`build_all_pane_instances` en `src/app/mod.rs`). Si se quita, TUI app vuelve a sangrar.

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

| Tecla | Acción |
|-------|--------|
| `Cmd+C / Cmd+V` | Copy / paste |
| `Cmd+Q` | Quit |
| `Cmd+K` | Clear screen + scrollback |
| `Cmd+F` | Abrir/cerrar busqueda |
| `Cmd+1-9` | Cambiar a tab N |
| `^B c` | New tab |
| `^B &` | Close tab |
| `^B n/b` | Next/prev tab |
| `^B ,` | Rename active tab |
| `^B %` | Split horizontal |
| `^B "` | Split vertical |
| `^B x` | Close pane |
| `^B h/j/k/l` | Focus pane (vim-style) |
| `^B Option+←→↑↓` | Resize pane |
| `^B a` | Abrir / cerrar AI panel |
| `^B A` | Mover focus terminal <-> chat |
| `^B e` | Explain last output |
| `^B f` | Fix last error |
| `^B z` | Undo last write |
| `^B o` | Command palette |
| `Ctrl+Space` | Inline AI block |
| Right-click | Context menu |
