# Session State

**Last Updated:** 2026-04-25
**Session Focus:** Phase 5 UX Polish — G-1 + G-2

## Branch: `master`

## Estado actual

**Phase 1–3 + 3.5 + A + 3.6 + B + C + D (todas las fases) COMPLETE. Phase 5 G-0 + G-1 + G-2 COMPLETE.**
**v0.1.3 publicado. Phase 5 (UX Polish) en curso. Sin deuda técnica abierta activa; quedan diferidos TD-PERF-03 / TD-PERF-05 / TD-PERF-29.**

**Pendiente en Phase 5:** G-3 (Markdown en chat). G-2-overlay (Enter en sidebar abre contenido).

---

## Esta sesión (2026-04-25) — Phase 5 G-1 + G-2

### G-1: Zoom de pane
- `src/app/mux.rs` — `zoomed_pane: Option<usize>` en `Mux`; `cmd_toggle_zoom_pane()`; se limpia en `cmd_split` y `cmd_close_pane`.
- `src/ui/palette/actions.rs` — variante `ZoomPane`; `FromStr`; entrada en paleta (`^F z`).
- `src/app/ui.rs` — `Action::ZoomPane => mux.cmd_toggle_zoom_pane()`.
- `src/app/input/mod.rs` — `Leader+z` hardcodeado en single-key dispatch.
- `src/app/mod.rs` — tras `fill_active_pane_infos`: filtra a 1 pane y expande a viewport; salta separadores cuando zoomed; agrega bit de zoom a `sb_key`; pasa `pane_zoomed` a `StatusBar::build`.
- `src/ui/status_bar.rs` — param `pane_zoomed: bool`; segmento ` ZOOM ` en teal cuando activo.
- `locales/en.toml` + `locales/es.toml` — clave `zoom_pane`.

### G-2: Sidebar — Workspaces + MCP + Skills + Steering
Diseño: sección única izquierda con proporciones fijas 40/20/20/20. Scroll independiente por sección. Tab/Shift+Tab para ciclar sección activa. j/k y flechas dentro del bloque activo.

- `src/llm/steering.rs` — `pub fn files() -> &[(String, String)]`.
- `src/app/renderer.rs` — `build_workspace_sidebar_instances` extendido con 7 nuevos params; cache key `Option<u64>` (FxHasher); workspace clipeado al 40%; secciones MCP/Skills/Steering con separadores 1px, headers coloreados según sección activa, scroll, placeholders dimmed.
- `src/app/mod.rs` — 4 nuevos campos: `info_sidebar_section`, `mcp_scroll`, `skills_scroll`, `steering_scroll`; `handle_sidebar_key` extendido con Tab/Shift+Tab + j/k routing por sección + Enter placeholder (`log::debug!`) para G-2-overlay; call site actualizado con `mcp_servers` precalculado via `BTreeMap`.
- `src/app/input/mod.rs` — `Leader+s` como alias de `Leader+e+e`.

### Keybinds nuevos
| Keybind | Acción |
|---|---|
| `Leader z` | Zoom/unzoom pane activo |
| `Leader s` | Abrir/cerrar sidebar (alias de `Leader e e`) |
| `Tab` (en sidebar) | Sección siguiente |
| `Shift+Tab` (en sidebar) | Sección anterior |
| `j/k` o `↑↓` (en sidebar) | Navegar dentro de la sección activa |

### G-2-overlay (pendiente)
Enter en items de MCP/Skills/Steering emite `log::debug!` por ahora. Cuando se implemente:
- MCP → overlay con contenido del `mcp.json` del servidor seleccionado
- Skill → overlay con `SKILL.md` (sin assets)
- Steering → overlay con contenido del archivo seleccionado

---

## Sesiones anteriores (resumen)

### 2026-04-25 mañana — UX polish + deuda técnica
- Auditoría runtime: idle detection, búsqueda capped, force_rebuild removido del chat.
- Chat UX: `Leader+a+a` toggle real, `Leader+A` reenfocar, hints actualizados.
- `src/llm/mcp/config.rs` — test aislado del entorno real.
- `src/app/ui.rs` — `AiEvent::Done` dispara notificación Lua inmediatamente.
- Búsqueda capped: `MAX_SEARCH_MATCHES = 10_000`, UI muestra `N+`, incremental desactivado sobre resultados truncados.

### 2026-04-25 tarde — Phase 5 G-0: UI tokens
- 7 tokens semánticos en `ColorScheme` (`ui_accent`, `ui_surface`, etc.).
- `derive_ui_colors()` calcula desde colores base cuando no se especifican.
- ~20+ literales hardcodeados en `renderer.rs` reemplazados por `colors.ui_*`.
- 5 temas bundled actualizados con tokens explícitos.

### 2026-04-24 — D-5 + REC-PERF-01/02/05 + /skills + /mcp + Leader+w + MCP fixes
Ver sesiones anteriores en historial de git.

### 2026-04-23 — Focus border + sidebar pills + Fase E + D-4 bugs
### 2026-04-22 — Fase C-3.5 + D-4 planificación
### 2026-04-21 — Fase C-1 bugs + C-2 + C-3
### 2026-04-20 — Fase C-1 inicial + Fase B
### 2026-04-19 — Fase A + Fase 3.6
