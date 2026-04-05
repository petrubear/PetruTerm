# Active Context

**Current Focus:** Phase 3 — Polish & UI Chrome
**Last Active:** 2026-04-05
**Priority:** P2 (Status Bar)

## Current State

**Phase 1 COMPLETE. Phase 2 COMPLETE. Phase 3 P1 COMPLETE. All TD items resolved. (2026-04-05)**

### Phase 3 P1 Verified ✓ (2026-04-05)

| Feature | Status | Notes |
|---------|--------|-------|
| Tab bar | ✅ | Rounded pill tabs via `RoundedRectPipeline` + SDF WGSL shader |
| Scroll bar | ✅ | 6px right-edge overlay, proportional thumb |
| Tab bar rounded pills | ✅ | TD-013 resolved — `src/renderer/rounded_rect.rs` |
| Tab bar bg transparency | ✅ | TD-014 resolved — inherits `config.colors.background` (clear color) |
| Title bar drag | ✅ | `setMovableByWindowBackground:YES` |
| Double/triple-click selection | ✅ | `Semantic`/`Lines` via `InputHandler::register_click()` |
| Tab bar mouse click | ✅ | `hit_test_tab_bar()` in `app/mod.rs` |
| Shell exit closes tab | ✅ | `close_terminal()` in `app/mux.rs` |
| Font fallback chain | ✅ | `petruterm.font("A, B, C")` resolved at config load time |

### Technical Debt
Clean — 0 open items.

### Keybinds (tmux-aligned)

| Key | Action |
|-----|--------|
| `leader+c` | New tab |
| `leader+&` | Close tab |
| `leader+n` | Next tab |
| `leader+p` | Prev tab |
| `leader+%` | Split horizontal |
| `leader+"` | Split vertical |
| `leader+x` | Close pane |
| `leader+a` | AI panel |
| `leader+p` | Command palette |
| `Ctrl+Space` | Inline AI block |

## Phase 3 Next Steps (ordered by priority)

1. **Status bar engine (P2)** — enable/disable from Lua + command palette
2. **Built-in status bar widgets** — `mode`, `cwd`, `git_branch`, `time`, `exit_code`
3. **Status bar widget Lua API** — `petruterm.statusbar.register_widget({ name, render })`
4. **Status bar position** — `top` or `bottom` (Lua config)
5. **Snippets (P3)** — `config.snippets` table, expand via palette, optional `trigger`
6. **Starship compatibility** — detect `STARSHIP_SHELL`, defer left prompt

## Files to Reference
- `src/renderer/rounded_rect.rs` — `RoundedRectInstance`, `RoundedRectPipeline`, SDF shader
- `src/app/renderer.rs` — `build_tab_bar_instances`, `build_scroll_bar_instances`
- `src/app/mod.rs` — `tab_bar_visible()`, `tab_bar_height_px()`, `hit_test_tab_bar()`
- `src/app/ui.rs` — `handle_palette_action`, AI feature handlers
- `src/app/input/mod.rs` — leader dispatch, `register_click()` for multi-click selection
- `config/default/keybinds.lua` — embedded keybind defaults
