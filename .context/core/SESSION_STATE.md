# Session State

**Last Updated:** 2026-04-09
**Session Focus:** Bug fixes + Phase 3 P3 inicio

## Branch: `master`

## Session Notes (2026-04-09 — batch 4)

### Trabajo realizado

#### Command palette — scroll + orden alfabético
- **Bug scroll:** el renderer siempre mostraba items `[0..14]`; `selected` quedaba fuera de la ventana al navegar hacia abajo. Fix: `scroll_offset = max(0, selected - max_visible + 1)`; items indexados con `scroll_offset + i`.
- **Orden alfabético:** `built_in_actions()` ahora hace `sort_unstable_by(|a,b| a.name.cmp(&b.name))` antes de retornar. Con query activo el fuzzy scorer sigue teniendo precedencia.

#### Status bar — click en git no funcionaba
- **Bug:** hit zone calculada como `win_h - pad_bottom - cell_h` no coincidía con la posición renderizada por `floor()`. Gap de hasta `cell_h - 1` px.
- **Fix:** hit zone ahora usa la misma fórmula que el renderer: `pad_top + tab_h + floor(viewport_h / cell_h) * cell_h`.

### Archivos modificados
- `src/app/renderer.rs` — palette scroll offset
- `src/ui/palette/actions.rs` — sort alfabético
- `src/app/mod.rs` — status bar hit zone row-based

## Build & Tests
- **cargo check:** PASS (2026-04-09)

## Próxima sesión

**Phase 3 P3 — Snippets.** Ver plan en ACTIVE_CONTEXT.md.
