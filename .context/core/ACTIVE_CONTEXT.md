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
| Context menu (right-click: Copy/Paste/Clear/Ask AI) | ✅ |
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

## Phase 3 P2 — Status Bar (EN PROGRESO)

### Diseño

**Layout visual:**
```
[ leader ] [ cwd ] [ git_branch ]          [ exit_code ] [ time ]
  izquierda, separados por ›               derecha, separados por │
```

**Segmentos izquierda:**
- `leader`: texto "LEADER", bg morado Dracula cuando activo, gris cuando inactivo
- `cwd`: directorio truncado a ~20 chars (…/PetruTerm)
- `git_branch`: rama + dirty flag `*` (vacío si no es repo)

**Segmentos derecha:**
- `exit_code`: solo visible si ≠ 0, bg rojo, texto "✘ N"
- `time`: "2026-04-08 10:36"

**Decisiones técnicas:**
- Posición default: `bottom` (configurable top/bottom)
- Altura: 1 fila del terminal (mismo mecanismo que tab bar)
- Git branch: async tokio, cache TTL 5s, channel igual que AI events
- Fondo entre segmentos: Dracula `current-line`

### Subtareas

| # | Tarea | Estado | Bloqueada por |
|---|-------|--------|--------------|
| 1 | `StatusBarConfig` + schema + ajuste de padding | 🔲 pendiente | — |
| 2 | `StatusBar` struct + segmentos izquierda/derecha | 🔲 pendiente | #1 |
| 3 | Git branch async con cache TTL 5s | 🔲 pendiente | #2 |
| 4 | `build_status_bar_instances()` en renderer GPU | 🔲 pendiente | #2 |
| 5 | Lua API + `ToggleStatusBar` en command palette | 🔲 pendiente | #3, #4 |

### Archivos a crear/modificar

| Archivo | Cambio |
|---------|--------|
| `src/ui/status_bar.rs` | NUEVO — StatusBar, StatusBarSegment, build() |
| `src/config/schema.rs` | StatusBarConfig, StatusBarPosition |
| `config/default/ui.lua` | config.status_bar = { enabled, position } |
| `src/app/mod.rs` | status_bar_height_px(), resize ajustado |
| `src/app/renderer.rs` | build_status_bar_instances() |
| `src/app/ui.rs` | poll_status_bar_events(), git cache, ToggleStatusBar |
| `src/ui/palette/actions.rs` | Action::ToggleStatusBar |
| `src/config/lua.rs` | petruterm.statusbar.register_widget() |

## Keybinds actuales

| Tecla | Acción |
|-------|--------|
| `^F c` | New tab |
| `^F &` | Close tab |
| `^F n/b` | Next/prev tab |
| `^F %` | Split horizontal |
| `^F "` | Split vertical |
| `^F x` | Close pane |
| `^F h/j/k/l` | Focus pane left/down/up/right |
| `^F a` | Abrir / cerrar AI panel |
| `^F A` | Mover focus terminal ↔ chat (sin cerrar) |
| `^F e/f` | Explain/Fix last output |
| `^F z` | Undo last write |
| `^F o` | Command palette |
| `Ctrl+Space` | Inline AI block |
| Right-click | Context menu (Copy/Paste/Clear/Ask AI) |

## Pendiente después de Status Bar

- Phase 3 P3: Snippets, Starship, temas built-in
- Phase 4: Plugin ecosystem (lazy.nvim-style)
- TD-027 (P3): Tab rename con `<leader>,`
