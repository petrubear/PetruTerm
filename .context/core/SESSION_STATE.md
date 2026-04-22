# Session State

**Last Updated:** 2026-04-21
**Session Focus:** Fase C-1 bugs resueltos. Siguiente: C-2 (Workspace model).

## Branch: `master`

## Estado actual

**Phase 1–3 + 3.5 COMPLETE. Fase A COMPLETE. Fase 3.6 COMPLETE. Fase B COMPLETE. Fase C-1 COMPLETE.**
**Siguiente: Fase C-2 (Workspace model en Mux) + C-3 (Workspace sidebar).**

---

## Fase C-1 — Unified titlebar — COMPLETA (bugs resueltos 2026-04-21)

### Bugs resueltos

1. **BTN_COLOR invisible** (`e3e70bb`): `[0.22,0.22,0.28,0.7]` → `[0.267,0.278,0.353,1.0]`
   (Dracula Current Line, full opacity — era invisible contra el background #22212c).

2. **Espacio excesivo debajo de la titlebar**: el config del usuario tenía `padding.top = 60`
   — valor de antes de que la titlebar custom manejara el clearance de traffic lights internamente.
   Cambiado a `top = 5` en `~/.config/petruterm/ui.lua`.
   El terminal ahora empieza en y = TITLEBAR_HEIGHT(30) + top(5) = 35px, sin el gap de 60px.

### Non-obvious: padding.top en modo Custom

`TITLEBAR_HEIGHT = 30.0` en `src/app/mod.rs` maneja el clearance de traffic lights.
`padding.top` es el gap ADICIONAL entre el borde inferior de la titlebar y la primera fila
del terminal. Con Custom titlebar, `top = 5` es suficiente (no usar 60 como antes).

### Lo que funciona

- Botones sidebar/layout visibles en titlebar (Dracula Current Line)
- Pills de tabs posicionadas correctamente a la derecha de traffic lights
- Rename de tabs funcional (pill crece con el texto, limitado a 16 chars)
- Terminal empieza inmediatamente debajo de la titlebar (5px gap)
- Status bar, menu nativo macOS, separadores: sin regresión

---

## Sesiones anteriores (resumen)

### 2026-04-20 — Fase C-1 inicial + Fase B cerrada
- Unified titlebar committeado (59097cd): traffic lights + buttons + tab pills
- Fase B: AppMenu con muda, menus File/View/AI/Window

### 2026-04-19 — Fase A + Fase 3.6
- v0.1.0 publicado, GitHub Copilot provider

### 2026-04-19 — Sprint cierre Phase 3.5
- Deuda P2/P3 cerrada, benches desbloqueados, CI verde
