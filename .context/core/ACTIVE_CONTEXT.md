# Active Context

**Current Focus:** Fase A — Versionado semántico + infraestructura i18n
**Last Active:** 2026-04-19

## Estado actual del proyecto

**Phase 1–3 COMPLETE. Phase 3.5 COMPLETE (sprint cierre incluido 2026-04-19).**
**Build limpio: check + test + clippy + fmt PASS. CI verde.**

## Roadmap acordado (en orden)

1. ~~**Sprint cierre 3.5**~~ COMPLETO
2. **Fase A** — Versionado semántico + infraestructura i18n (en/es)
3. **Fase B** — Menu bar nativo macOS (crate `muda`)
4. **Fase C** — Titlebar custom (NSWindow híbrido) + Workspaces (Workspace > Tab > Pane)
5. **Fase D** — AI Chat MCP + Skills (agentskills.io format)
6. **Fase 4** — Plugin ecosystem (Lua, lazy.nvim-style)

## Próximo trabajo — Fase A

| Tarea | Archivo/Crate |
|-------|---------------|
| Bump versión a 0.1.0 | `Cargo.toml` |
| Crear CHANGELOG.md | raíz del proyecto |
| Añadir crate `rust-i18n` | `Cargo.toml` |
| Detección locale macOS (`NSLocale`) | nuevo módulo `src/i18n/` |
| Archivos `locales/en.toml` + `locales/es.toml` | nuevo dir `locales/` |
| Scope: menu labels, errores LLM, panel AI, status bar | varios |

## Invariantes arquitectonicos clave (no romper)

**Shaper drops space cells (TD-RENDER-01):**
Pre-pass bg-only en `build_instances` OBLIGATORIO. Sin el, celdas-espacio con bg != default_bg
no generan vertices → GPU clear color → franjas horizontales.

**damage-skip scratch buffer:**
`cell_data_scratch` es per-terminal. Siempre limpiar cuando cambia `terminal_id`
(`build_all_pane_instances` en `src/app/mod.rs`). Si se quita, TUI app vuelve a sangrar.

**Blink fast path:**
`last_instance_count` + `last_overlay_start` en `RenderContext` OBLIGATORIOS.
Vertex cursor transparente (bg.a=0) para blink-off — no reducir cell_count.
Si se revierte, status bar desaparece en cada blink.

**alacritty_terminal grid scrollback:**
`grid()[Line(row)]` NO cuenta `display_offset`. Usar `Line(row as i32 - display_offset)`.

**alacritty_terminal exit event:** `Event::ChildExit(i32)`, NO `Event::Exit`.

**PTY env vars obligatorias:** `TERM=xterm-256color`, `COLORTERM=truecolor`, `TERM_PROGRAM=PetruTerm`.

**SwashCache:** usar `get_image_uncached()`, NO `get_image()`.

**macOS trackpad:** `MouseScrollDelta::PixelDelta(pos).y` es LOGICAL POINTS.
Divisor: `cell_height / scale_factor`.

**JetBrains Mono ligatures:** bearing_x puede ser NEGATIVO — no clampar a 0.

**alacritty_terminal 1-cell selection:** limpiar con `clear_selection()` en click sin drag.
