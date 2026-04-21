# Session State

**Last Updated:** 2026-04-21
**Session Focus:** Fase C-1 bugs fixed. Siguiente: C-2 (Workspace model).

## Branch: `master`

## Estado actual

**Phase 1–3 + 3.5 COMPLETE. Fase A COMPLETE. Fase 3.6 COMPLETE. Fase B COMPLETE. Fase C-1 COMPLETE.**
**Siguiente: Fase C-2 (Workspace model en Mux) + C-3 (Workspace sidebar).**

---

## Fase C-1 — Unified titlebar — COMPLETA (bugs resueltos 2026-04-21)

### Bugs resueltos en esta sesion

1. **BTN_COLOR invisible**: cambiado de `[0.22, 0.22, 0.28, 0.7]` a `[0.267, 0.278, 0.353, 1.0]`
   (Dracula "Current Line" con full opacity — contrasta contra el background #282A36).

2. **TITLEBAR_HEIGHT usado como pixels fisicos en vez de logicos**: en displays Retina (2x),
   `TITLEBAR_HEIGHT=30.0` (logico) se pasaba como 30px fisicos donde se necesitaban 60px fisicos.
   - `tab_bar_height_px()`: ahora devuelve `TITLEBAR_HEIGHT * scale_factor()` en Custom mode
   - `apply_tab_bar_padding()`: usa `TITLEBAR_HEIGHT * sf` para el offset fisico
   - `build_tab_bar_instances` call: `gpu_pad_y` y `pad_left` ahora en pixels fisicos
   - Nueva helper `scale_factor()` en App
   - `scale_factor()` helper añadido en `src/app/mod.rs`

### Lo que funciona (post-fixes)

- Buttons sidebar/layout visibles en la titlebar
- Pills de tabs posicionadas correctamente a la derecha de los traffic lights
- Contenido terminal empieza despues del titlebar (no overlap) en Retina 2x
- Status bar, menu nativo macOS, separadores de panes: sin regresion

### Non-obvious: unidades en la pipeline de render

`TITLEBAR_HEIGHT = 30.0` es en **logical points** (no pixels fisicos).
- Multiplicar por `scale_factor` antes de pasar a `set_padding()` o al rect pipeline (que opera en pixels fisicos).
- `cell_width/height` del shaper son **pixels fisicos** (el shaper usa `font.size * scale_factor`).
- `pad.top/left/bottom/right` del config son **logical points** — pequena imprecision pre-existente al no multiplicar por sf, pero <5px en la mayoria de configs.

---

## Sesiones anteriores (resumen)

### 2026-04-20 — Fase C-1 inicio + commit inicial
- Unified titlebar implementado y committeado (59097cd).
- Bugs identificados: BTN_COLOR invisible, TITLEBAR_HEIGHT no escalado en varios call sites.

### 2026-04-20 — Fase B cerrada
- `src/app/menu.rs`: AppMenu con muda. File/View/AI/Window menus.
- Key: usar `receiver()` en `about_to_wait()`, no `set_event_handler`.

### 2026-04-19 — Fase A + Fase 3.6
- v0.1.0 publicado (versionado + i18n)
- GitHub Copilot provider

### 2026-04-19 — Sprint cierre Phase 3.5
- Deuda P2/P3 cerrada, benches desbloqueados, CI verde
