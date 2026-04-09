# Active Context

**Current Focus:** Phase 3 P2 — Status Bar
**Last Active:** 2026-04-08

## Estado actual del proyecto

**Phase 1 COMPLETE. Phase 2 COMPLETE. Phase 2.5 COMPLETE (P1+P2+P3). Phase 3 P1 implementada.**
**Deuda técnica: 2 ítems abiertos (P2, P3).**
**Tests: 16/16 passing. `cargo build` PASA limpio.**

### Features verificados (2026-04-08)

| Feature | Estado |
|---------|--------|
| Render, PTY, teclado, ratón, clipboard, cursor, resize | ✅ |
| Custom title bar, .app bundle, icono | ✅ |
| Scrollback + scroll bar | ✅ |
| Ligatures, nvim/tmux verificados | ✅ |
| AI panel + inline AI block (Ctrl+Space) | ✅ |
| Leader key system | ✅ |
| LLM providers (OpenRouter/Ollama/LMStudio) | ✅ |
| Historial de chat por pane | ✅ |
| Tab bar (pill shape, SDF shader) | ✅ |
| Shell exit cierra tab (o solo el pane si hay más) | ✅ |
| Selección doble/triple click | ✅ |
| Context menu (right-click: Copy/Paste/Clear/───/Ask AI) | ✅ |
| Keybinds en command palette | ✅ |
| Default configs completas | ✅ |
| Emoji / color glyph rendering | ✅ |
| Phase 2.5 P1 — file context + AGENTS.md + file picker | ✅ |
| Phase 2.5 P2 — LLM tool use (ReadFile, ListDir) | ✅ |
| Phase 2.5 P3 — WriteFile + RunCommand + undo | ✅ |
| Multi-pane splits + separadores + padding | ✅ |
| Leader+h/j/k/l — vim-style pane focus | ✅ |
| /quit solo cierra panel (no tabs) | ✅ |
| System prompt abierto a preguntas generales | ✅ |
| Ctrl+B a — abrir/cerrar panel | ✅ |
| Ctrl+B A — mover focus terminal ↔ chat | ✅ |

## Siguiente: Phase 3 P2 — Status Bar

### Deliverables pendientes
- [ ] Status bar engine (lua-line style): enable/disable desde Lua + command palette
- [ ] Widgets built-in: `mode`, `cwd`, `git_branch`, `time`, `exit_code`
- [ ] Lua API: `petruterm.statusbar.register_widget({ name, render })`
- [ ] Posición configurable (`top` / `bottom`)
- Referencia visual: `~/Documents/ScreenShots/Screenshot 2026-04-07 at 10.36.37.png`

## Keybinds actuales

| Tecla | Acción |
|-------|--------|
| `^B c` | New tab |
| `^B &` | Close tab |
| `^B n/b` | Next/prev tab |
| `^B %` | Split horizontal |
| `^B "` | Split vertical |
| `^B x` | Close pane |
| `^B h/j/k/l` | Focus pane left/down/up/right |
| `^B a` | Abrir / cerrar AI panel |
| `^B A` | Mover focus terminal ↔ chat (sin cerrar) |
| `^B e/f` | Explain/Fix last output |
| `^B z` | Undo last write |
| `^B o` | Command palette |
| `Ctrl+Space` | Inline AI block |
| Right-click | Context menu (Copy/Paste/Clear/Ask AI) |

## Pendiente después de Status Bar

- Phase 3 P3: Snippets, Starship, temas built-in
- Phase 4: Plugin ecosystem (lazy.nvim-style)
