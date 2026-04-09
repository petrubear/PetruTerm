# Session State

**Last Updated:** 2026-04-08
**Session Focus:** Phase 3 P2 — Status Bar + UX fixes + leader key change

## Branch: `master`

## Session Notes (2026-04-08 — Phase 3 P2 + fixes)

### Phase 3 P2 — Status Bar (COMPLETA)

#### Archivos nuevos
- `src/ui/status_bar.rs` — StatusBar, StatusBarSegment, build(), format_time(), truncate_path()

#### Cambios clave
- `src/config/schema.rs`: `StatusBarConfig { enabled, position }`, `StatusBarPosition { Top, Bottom }`
- `config/default/ui.lua`: `config.status_bar = { enabled=true, position="bottom" }`
- `src/config/lua.rs`: parsing de `status_bar` table; `ToggleStatusBar` expuesto en `petruterm.action`
- `src/app/mod.rs`: `status_bar_height_px()`, `default_grid_size` y `viewport_rect` restan `sb_h` del bottom
- `src/app/ui.rs`: `poll_git_branch()` con cache TTL 5s; `fetch_git_branch()` async (tokio); `ToggleStatusBar` handler
- `src/app/renderer.rs`: `build_status_bar_instances()` — left segments con `›`, right segments con `│`
- `src/ui/palette/actions.rs`: `Action::ToggleStatusBar`, entry en palette

#### Layout de la status bar
```
[ ^F ] [ ~/…/PetruTerm ] [ master* ]        [ ✘ 1 ] [ 2026-04-08 10:36 ]
  izquierda (›)                               derecha (│)
```

### Leader key: Ctrl+B → Ctrl+F
- `config/default/keybinds.lua`: `key = "f"`
- `~/.config/petruterm/keybinds.lua`: `key = "f"`
- `src/config/schema.rs`: `LeaderConfig::default()` → `key: "f"`

### Bug fixes y UX (commits anteriores)
- `/quit` en chat solo cierra el panel (no el tab)
- System prompt del chat ampliado (responde preguntas generales)
- Context menu: separador + "Ask AI" (envía selección al chat)
- `Ctrl+F a` / `Ctrl+F A` — toggle vs focus del AI panel (dos keybinds separados)

## Build & Tests
- **cargo build:** PASS (2026-04-08)
- **cargo test:** 16/16 PASS
- **cargo clippy:** pendiente verificar

## Session anterior (2026-04-08 — UX fixes)
Ver commit b92cdeb.
