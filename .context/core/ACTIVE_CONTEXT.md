# Active Context

**Current Focus:** Phase 4 (plugins) — búsqueda de texto y deuda técnica completadas
**Last Active:** 2026-04-10

## Estado actual del proyecto

**Phase 1–3 COMPLETE. Phase 4 (plugins) no iniciada.**
**Deuda técnica: 4 ítems abiertos (TD-PERF-03/04/05, TD-MAINT-01). `cargo clippy` PASA, 0 warnings.**

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
| Command palette (scroll + orden alfabético) | ✅ |
| Phase 2.5 P1 — file context + AGENTS.md + file picker | ✅ |
| Phase 2.5 P2 — LLM tool use (ReadFile, ListDir) | ✅ |
| Phase 2.5 P3 — WriteFile + RunCommand + undo | ✅ |
| Multi-pane splits + separadores | ✅ |
| Leader+h/j/k/l — vim-style pane focus | ✅ |
| Status bar — leader, CWD, git branch, exit code, time | ✅ |
| Pane resize (teclado + mouse drag) | ✅ |
| Palette + context menu — fondo sólido sobre contenido LCD | ✅ |
| **Cmd+K** — clear screen + scrollback | ✅ (2026-04-10) |
| **Cmd+F** — búsqueda de texto en terminal + scrollback | ✅ (2026-04-10) |

## Deuda técnica abierta

| ID | Descripción | Prioridad |
|----|-------------|-----------|
| TD-PERF-03 | GPU upload completo (PCIe) — no aplica en Apple Silicon | P1 |
| TD-PERF-04 | `scan_files()` síncrono en hilo principal al abrir file picker | P2 |
| TD-PERF-05 | Atlas de glifos siempre 64 MB desde arranque | P2 |
| TD-MAINT-01 | Sin `cargo-audit` — sin escaneo CVEs | P3 |

Ver [TECHNICAL_DEBT.md](../../.context/quality/TECHNICAL_DEBT.md) para detalle.

## Keybinds actuales (hardcoded + leader)

| Tecla | Acción |
|-------|--------|
| `Cmd+C / Cmd+V` | Copy / paste |
| `Cmd+Q` | Quit |
| `Cmd+K` | Clear screen + scrollback |
| `Cmd+F` | Abrir/cerrar búsqueda de texto |
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
| `^B A` | Mover focus terminal ↔ chat |
| `^B e` | Explain last output |
| `^B f` | Fix last error |
| `^B z` | Undo last write |
| `^B o` | Command palette |
| `Ctrl+Space` | Inline AI block |
| Right-click | Context menu |

## Búsqueda de texto — arquitectura (2026-04-10)

- `src/ui/search_bar.rs` — `SearchBar` + `SearchMatch { grid_line: i32, col, len }`
- `UiManager.search_bar: SearchBar` en `src/app/ui.rs`
- `Mux::search_active_terminal(&query)` — char-indexed (`Vec<char>` por fila) para evitar desplazamiento con chars multi-byte
- `collect_grid_cells_for` acepta `Option<(&[SearchMatch], usize)>` e inyecta `AnsiColor::Spec(Rgb)` para highlights
- `RenderContext::build_search_bar_instances` — overlay top-right con query + contador + hint
- Auto-scroll al match activo usando `scroll_display(delta)` — centra el match en viewport

## Próximos pasos recomendados

1. **Phase 4:** Plugin ecosystem (Lua loader, API surface). Ver `build_phases.md`.
