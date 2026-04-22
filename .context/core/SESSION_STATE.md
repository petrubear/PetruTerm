# Session State

**Last Updated:** 2026-04-22
**Session Focus:** Bugfix — eliminado botón ⊞ fantasma de titlebar.

## Branch: `master`

## Estado actual

**Phase 1–3 + 3.5 COMPLETE. Fase A COMPLETE. Fase 3.6 COMPLETE. Fase B COMPLETE. Fase C-1 COMPLETE. Fase C-2 COMPLETE. Fase C-3 COMPLETE. Fase C-3.5 COMPLETE.**
**Siguiente: Fase D-1 (MCP config loader).**

---

## Fase C-3.5 — AI panel right sidebar + iconos — COMPLETA (2026-04-22)

### Lo que se hizo

1. **Botones de titlebar** (sidebar + AI panel, 2 total):
   - `≡` sidebar workspaces en [80..102], `✦` AI panel en [106..128]
   - Dimmed cuando panel cerrado, lit cuando abierto; tinta purple cuando activo
   - Técnica: push_shaped_row col=0 row=0, override grid_pos a coords físicas

2. **Header del AI panel restyled** para igualar estética del sidebar izquierdo.

3. **Click handler** para botón AI (toggle open/close).

**Bugfix (2026-04-22):** Eliminado tercer botón `⊞` (layout/pane) que se introdujo
accidentalmente al agregar los iconos. Nunca tuvo handler. Tabs ahora empiezan en 132.

### Archivos modificados
- `src/app/renderer.rs`: buttons, iconos, header chat panel
- `src/app/mod.rs`: hit_test_tab_bar, click handler, call site build_tab_bar_instances

---

## Sesiones anteriores (resumen)

### 2026-04-21 — Fase C-1 bugs + C-2 + C-3
- BTN_COLOR fix, padding.top fix
- Workspace model en Mux
- Sidebar izquierdo drawer (workspaces)

### 2026-04-20 — Fase C-1 inicial + Fase B cerrada
- Unified titlebar committeado (59097cd): traffic lights + buttons + tab pills
- Fase B: AppMenu con muda, menus File/View/AI/Window

### 2026-04-19 — Fase A + Fase 3.6
- v0.1.0 publicado, GitHub Copilot provider
