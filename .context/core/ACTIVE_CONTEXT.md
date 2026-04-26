# Active Context

**Current Focus:** Phase 4 — Plugin Ecosystem
**Last Active:** 2026-04-26

## Estado actual del proyecto

**Phase 1–3 COMPLETE. Phase 3.5 COMPLETE. Fase A COMPLETE. Fase 3.6 COMPLETE. Fase B COMPLETE. Fase C COMPLETE. Fase D (D-1/D-2/D-3/D-4/D-5) COMPLETE. REC-PERF-01/02/05 COMPLETE. Phase 5 G-0/G-1/G-2/G-3 COMPLETE. Chat input UX COMPLETE.**
**Build limpio. CI verde. Sin deuda abierta activa; diferidos: TD-PERF-03, TD-PERF-05, TD-PERF-29.**

## Roadmap acordado (en orden)

1. ~~**Sprint cierre 3.5**~~ COMPLETO
2. ~~**Fase A**~~ COMPLETO — Versionado semántico + i18n (v0.1.0)
3. ~~**Fase 3.6**~~ COMPLETO — GitHub Copilot provider (v0.1.1)
4. ~~**Fase B**~~ COMPLETO — Menu bar nativo macOS (crate `muda`)
5. ~~**Fase C**~~ COMPLETO — Titlebar custom (NSWindow híbrido) + Workspaces
6. ~~**Fase D-1/D-2/D-3**~~ COMPLETO — MCP integration (config, client, chat wiring)
7. ~~**Fase D-4**~~ COMPLETO — AI Chat Skills (agentskills.io format)
8. ~~**Fase D-5**~~ COMPLETO — MCP hot-reload (notify + debounce + reload_mcp)
9. ~~**Bug fixes**~~ COMPLETO — Focus border (v0.1.2), left-edge overlap (v0.1.3), Leader+a sidebar hijack
10. ~~**UI polish**~~ COMPLETO — /skills color, /mcp command, Leader+w workspace
11. ~~**REC-PERF-01/02/05**~~ COMPLETO — ASCII warmup, parking_lot, frame budget doc
12. ~~**Phase 5 G-0**~~ COMPLETO — UI tokens en ColorScheme
13. ~~**Phase 5 G-1**~~ COMPLETO — Zoom pane (`Leader z`)
14. ~~**Phase 5 G-2**~~ COMPLETO — Sidebar MCP/Steering/Skills tabs
15. ~~**Phase 5 G-3**~~ COMPLETO — Markdown en chat
16. ~~**Chat input UX**~~ COMPLETO — cursor, historial, vertical scroll, 4-line input
17. **Fase 4** — Plugin ecosystem (Lua, lazy.nvim-style)

## Cambios recientes a preservar

**Chat panel toggle/focus split:**
- `Leader+a+a` = `ToggleAiPanel` (abrir/cerrar).
- `Leader+A` = `FocusAiPanel` (mover foco terminal ↔ chat sin cerrar).
- `Esc` dentro del chat NO cierra el panel: devuelve foco a terminal, excepto en `Error` (`dismiss_error`) y `AwaitingConfirm` (`confirm_no`).

**Titlebar cache inputs:**
- `RenderContext.tab_bar_inputs` ahora incluye `(active_index, total_cols, sidebar_visible, panel_visible)`.
- Si se quitan `sidebar_visible` / `panel_visible` de la clave, los botones superiores del sidebar/chat dejan de reflejar el estado toggle activo.

**Search memory guard:**
- `MAX_SEARCH_MATCHES = 10_000` en `src/app/mux.rs`.
- `SearchBar.matches_truncated` evita reutilizar el incremental filter sobre resultados capados.

**Background AI idle policy:**
- `about_to_wait` distingue AI activity visible vs background. Solo panel enfocado / block visible pueden impedir idle.
- Mantener esta separación para no reintroducir wakeups periódicos durante streaming en background.

**AI completion notifications:**
- `UiManager::poll_ai_events()` y `poll_ai_block_events()` devuelven `AiPollResult { changed, completed }`.
- `ai_response` debe dispararse desde cualquier sitio que drene el canal AI cuando `completed == true`; no depender solo de `RedrawRequested`, porque `about_to_wait` puede consumir `AiEvent::Done` antes.

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
