# Active Context

**Current Focus:** Deuda técnica resuelta — TD-043 a TD-047 completados
**Last Active:** 2026-04-09

## Estado actual del proyecto

**Phase 1 COMPLETE. Phase 2 COMPLETE. Phase 2.5 COMPLETE. Phase 3 P1 COMPLETE. Phase 3 P2 COMPLETE.**
**Phase 3 P3 parcial (snippets/Starship pendientes). Phase 4 (plugins) no iniciada.**
**Deuda técnica: 0 ítems abiertos. `cargo build` PASA.**

### Features verificados

| Feature | Estado |
|---------|--------|
| Render, PTY, teclado, ratón, clipboard, cursor, resize | ✅ |
| Custom title bar, .app bundle, icono | ✅ |
| Scrollback + scroll bar | ✅ |
| Ligatures, nvim/tmux verificados | ✅ |
| Emoji / color glyph rendering | ✅ |
| AI panel + inline AI block (Ctrl+Space) | ✅ |
| Leader key system | ✅ |
| LLM providers (OpenRouter/Ollama/LMStudio) | ✅ |
| Historial de chat por pane | ✅ |
| Tab bar (pill shape, SDF shader) | ✅ |
| Tab rename (`<leader>,`) | ✅ |
| Shell exit cierra tab | ✅ |
| Selección doble/triple click | ✅ |
| Context menu (right-click) | ✅ |
| Command palette | ✅ |
| Phase 2.5 P1 — file context + AGENTS.md + file picker | ✅ |
| Phase 2.5 P2 — LLM tool use (ReadFile, ListDir) | ✅ |
| Phase 2.5 P3 — WriteFile + RunCommand + undo | ✅ |
| Multi-pane splits + separadores | ✅ |
| Leader+h/j/k/l — vim-style pane focus | ✅ |
| Status bar — leader, CWD, git branch, exit code, time | ✅ |
| Pane resize (teclado + mouse drag) | ✅ (TD-042–045 resueltos) |
| Status bar modo resize (naranja al presionar Option) | ✅ (TD-046) |
| Padding visual terminal↔status bar (4px) | ✅ (TD-047) |

## Deuda técnica abierta

*Sin ítems abiertos. Ver [TECHNICAL_DEBT_archive.md](../../.context/quality/TECHNICAL_DEBT_archive.md) para historial completo.*

## Keybinds actuales

| Tecla | Acción |
|-------|--------|
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
| `^B A` | Mover focus terminal ↔ chat |
| `^B e` | Explain last output |
| `^B f` | Fix last error |
| `^B z` | Undo last write |
| `^B o` | Command palette |
| `Ctrl+Space` | Inline AI block |
| Right-click | Context menu |

## Phase 3 P3 — Pendiente

| Tarea | Estado |
|-------|--------|
| Tab rename `<leader>,` | ✅ (2026-04-08) |
| Snippets: `config.snippets` tabla Lua, expandir via palette | 🔲 |
| Starship compatibility: detectar `STARSHIP_SHELL` | 🔲 |
| Powerline / Nerd Font glyphs en widgets | 🔲 |
| Built-in themes en `assets/themes/` | 🔲 |

## Próximos pasos recomendados

1. **Phase 3 P3:** Snippets y Starship compatibility
2. **Phase 4:** Plugin ecosystem (Lua loader, API surface)
