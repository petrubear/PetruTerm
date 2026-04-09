# Active Context

**Current Focus:** Deuda técnica abierta / Phase 3 P3
**Last Active:** 2026-04-08

## Estado actual del proyecto

**Phase 1 COMPLETE. Phase 2 COMPLETE. Phase 2.5 COMPLETE (P1+P2+P3). Phase 3 P1 COMPLETE. Phase 3 P2 COMPLETE. Phase 3 P3 parcial.**
**Deuda técnica: 10 ítems abiertos (P0:1, P1:3, P2:5, P3:1). Triaje Kiro completado 2026-04-08.**
**Tests: 16/16 passing. `cargo build` PASA limpio. `cargo clippy -D warnings` PASA limpio.**

### Features verificados

| Feature | Estado |
|---------|--------|
| Render, PTY, teclado, ratón, clipboard, cursor, resize | ✅ |
| Custom title bar, .app bundle, icono | ✅ |
| Scrollback + scroll bar | ✅ |
| Ligatures, nvim/tmux verificados | ✅ |
| Emoji / color glyph rendering | ✅ |
| AI panel + inline AI block (Ctrl+Space) | ✅ |
| Leader key system (Ctrl+F) | ✅ |
| LLM providers (OpenRouter/Ollama/LMStudio) | ✅ |
| Historial de chat por pane | ✅ |
| Tab bar (pill shape, SDF shader) | ✅ |
| Tab rename (`<leader>,`) | ✅ |
| Shell exit cierra tab | ✅ |
| Selección doble/triple click | ✅ |
| Context menu (right-click: Copy/Paste/Clear/Ask AI) | ✅ |
| Command palette con keybinds | ✅ |
| Phase 2.5 P1 — file context + AGENTS.md + file picker | ✅ |
| Phase 2.5 P2 — LLM tool use (ReadFile, ListDir) | ✅ |
| Phase 2.5 P3 — WriteFile + RunCommand + undo | ✅ |
| Multi-pane splits + separadores | ✅ |
| Leader+h/j/k/l — vim-style pane focus | ✅ |
| Status bar — leader, CWD, git branch, exit code, time | ✅ |

## Deuda técnica abierta (priorizada)

| ID | Prioridad | Esfuerzo | Descripción |
|----|-----------|----------|-------------|
| TD-030 | **P0** | ~30 min | Archivos adjuntos sin límite de tamaño → OOM |
| TD-029 | P1 | ~15 min | `cwd` no canonicalizado rompe tool use en macOS |
| TD-031 | P1 | ~20 min | Regex compilada en cada `sanitize_command` |
| TD-033 | P1 | 1–2 h | Fallback de tool rounds mapea `tool` msgs a `System` |
| TD-032 | P2 | ~2 h | `api_msgs.clone()` hasta 10x por query |
| TD-035 | P2 | ~45 min | Doble lookup hashmap en render loop |
| TD-036 | P2 | ~30 min | Hot-reload lee keybinds.lua completo para extraer versión |
| TD-037 | P2 | ~10 min | Undo stack sin límite de tamaño |
| TD-038 | P2 | ~1.5 h | Errores LLM sin contexto accionable |
| TD-034 | P3 | ~1 h | `run_command` sin indicador de riesgo visual |

## Phase 3 P3 — Pendiente

| Tarea | Estado |
|-------|--------|
| Tab rename `<leader>,` | ✅ (2026-04-08) |
| Snippets: `config.snippets` tabla Lua, expandir via palette | 🔲 |
| Starship compatibility: detectar `STARSHIP_SHELL` | 🔲 |
| Powerline / Nerd Font glyphs en widgets | 🔲 |
| Built-in themes en `assets/themes/` | 🔲 |

## Keybinds actuales

| Tecla | Acción |
|-------|--------|
| `^F c` | New tab |
| `^F &` | Close tab |
| `^F n/b` | Next/prev tab |
| `^F ,` | Rename active tab |
| `^F %` | Split horizontal |
| `^F "` | Split vertical |
| `^F x` | Close pane |
| `^F h/j/k/l` | Focus pane left/down/up/right |
| `^F a` | Abrir / cerrar AI panel |
| `^F A` | Mover focus terminal ↔ chat |
| `^F e` | Explain last output |
| `^F f` | Fix last error |
| `^F z` | Undo last write |
| `^F o` | Command palette |
| `Ctrl+Space` | Inline AI block |
| Right-click | Context menu |

## Próximos pasos recomendados

1. **Sesión rápida (~1.5 h):** resolver TD-030 + TD-029 + TD-031 + TD-037 (4 fixes triviales)
2. **Sesión media (~2 h):** TD-033 (extender ChatRole con Tool variant)
3. **Phase 3 P3:** Snippets y Starship
4. **Phase 4:** Plugin ecosystem
