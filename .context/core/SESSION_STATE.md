# Session State

**Last Updated:** 2026-04-09
**Session Focus:** TD-043–TD-047 — bugs en pane resize + P2 polish (status bar, padding)

## Branch: `master`

## Session Notes (2026-04-09)

### Trabajo realizado

#### TD-043 — AI panel input en fila incorrecta (resuelto)
- `src/app/renderer.rs` ~l.709: `(vis1, vis2) = if n >= 2 { (lines[n-2], lines[n-1]) } else { (lines[0], "") }`. Texto en la fila con `►` cuando `n==1`.

#### TD-044 — Mouse separator hit area (resuelto)
- `src/app/mod.rs` `separator_at_pixel`: umbral ±3.0 → ±8.0 px físicos en ambas ramas.

#### TD-045 — Keyboard pane resize Option+Arrow (resuelto)
- `src/app/input/mod.rs`: añadidos imports `PhysicalKey`, `KeyCode`. Fallback a `physical_key` cuando `logical_key` es `Key::Character` (macOS transforma Option+Arrow).

#### TD-046 — Status bar modo resize (resuelto)
- `src/ui/status_bar.rs`: nuevo parámetro `leader_resize_mode: bool`; constante `BG_LEADER_RESIZE = #ffb86c` (naranja Dracula). Muestra " RESIZE " cuando `leader_active && alt_key()`.
- `src/app/mod.rs`: `leader_resize_mode` calculado inline antes del `StatusBar::build`.

#### TD-047 — Padding terminal↔status bar (resuelto)
- `src/app/mod.rs` `status_bar_height_px()`: retorna `cell_h + 4.0` en lugar de `cell_h`. Crea franja de 4px cubierta por `bg_color` entre terminal y status bar. Sin cambios al shader.

### Archivos modificados (esta sesión)
- `src/app/renderer.rs` — fix TD-043 (vis1/vis2 logic)
- `src/app/mod.rs` — fix TD-044 (±8px), TD-046 (leader_resize_mode inline), TD-047 (SB_PAD_PX)
- `src/app/input/mod.rs` — fix TD-045 (PhysicalKey fallback)
- `src/ui/status_bar.rs` — fix TD-046 (leader_resize_mode param, BG_LEADER_RESIZE)
- `.context/quality/TECHNICAL_DEBT.md` — 0 ítems abiertos
- `.context/quality/TECHNICAL_DEBT_archive.md` — TD-043–TD-047 archivados

## Build & Tests
- **cargo build:** PASS (2026-04-09)
- **cargo clippy:** no ejecutado esta sesión
- **cargo test:** no ejecutado esta sesión

## Próxima sesión

Deuda técnica a cero. Opciones:
1. **Phase 3 P3:** Snippets (`config.snippets` Lua + expand via palette) y Starship compat.
2. **Phase 4:** Plugin ecosystem (Lua plugin loader, API surface).
