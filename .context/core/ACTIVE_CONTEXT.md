# Active Context

**Current Focus:** Fase C — Titlebar custom + Workspaces
**Last Active:** 2026-04-20

## Estado actual del proyecto

**Phase 1–3 COMPLETE. Phase 3.5 COMPLETE. Fase A COMPLETE. Fase 3.6 COMPLETE.**
**v0.1.1 publicado. Build limpio: check + test + clippy + fmt PASS. CI verde.**

## Roadmap acordado (en orden)

1. ~~**Sprint cierre 3.5**~~ COMPLETO
2. ~~**Fase A**~~ COMPLETO — Versionado semántico + i18n (v0.1.0)
3. ~~**Fase 3.6**~~ COMPLETO — GitHub Copilot provider (v0.1.1)
4. ~~**Copilot OAuth doc+validation**~~ COMPLETO — README actualizado (device-flow), osascript dialog + /usr/bin/open para .app bundle
5. ~~**Fase B**~~ COMPLETO — Menu bar nativo macOS (crate `muda`)
5. **Fase C** — Titlebar custom (NSWindow híbrido) + Workspaces
6. **Fase D** — AI Chat MCP + Skills (agentskills.io format)
7. **Fase 4** — Plugin ecosystem (Lua, lazy.nvim-style)

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

**Copilot OAuth:**
El endpoint `/copilot_internal/v2/token` solo acepta tokens de OAuth apps registradas para Copilot.
PAT classic y `gh auth token` dan 404. Requiere device flow con `client_id = Iv1.b507a08c87ecfe98`.
Token almacenado en Keychain: `PetruTerm` / `GITHUB_COPILOT_OAUTH_TOKEN`.
