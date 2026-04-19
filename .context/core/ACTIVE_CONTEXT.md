# Active Context

**Current Focus:** Sprint cierre Phase 3.5 — deuda técnica P2/P3 antes de nuevas features
**Last Active:** 2026-04-18

## Estado actual del proyecto

**Phase 1–3 COMPLETE. Phase 3.5: mayoría completa (ver `build_phases.md` para estado real).**
**Build limpio: check + test + clippy + fmt PASS. CI verde.**

## Roadmap acordado (en orden)

1. **Sprint cierre 3.5** — P2/P3 tech debt + bench CI (ver SESSION_STATE.md)
2. **Fase A** — Versionado semántico + infraestructura i18n (en/es)
3. **Fase B** — Menu bar nativo macOS (crate `muda`)
4. **Fase C** — Titlebar custom (NSWindow híbrido) + Workspaces (Workspace > Tab > Pane)
5. **Fase D** — AI Chat MCP + Skills (agentskills.io format)
6. **Fase 4** — Plugin ecosystem (Lua, lazy.nvim-style)

## Próximo trabajo — Sprint cierre Phase 3.5

### Archivos clave

| Archivo | Tarea |
|---------|-------|
| `src/llm/tools.rs` | TD-MEM-13: limitar ReadFile 50k chars, max 5 rounds |
| `src/app/ui.rs` | TD-MEM-23: `agent_step(&[Value])` en vez de clone; TD-PERF-18: tokio pool 2 workers; TD-MEM-24: VecDeque undo_stack; TD-PERF-23: leader_deadline |
| `src/llm/chat_panel.rs` | TD-PERF-04: scan_files spawn_blocking; TD-MEM-17: streaming_buf.clear() en close() |
| `src/app/mod.rs` | TD-PERF-15: clipboard spawn_blocking |
| `src/ui/palette/mod.rs` | TD-PERF-21: fuzzy matcher incremental |
| `benches/` | Desbloquear build_instances + rasterize_to_atlas |
| `.github/workflows/ci.yml` | CI gating criterion regresión >5% |

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
