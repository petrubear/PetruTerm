# Active Context

**Current Focus:** Phase 3 — Polish & UI Chrome
**Last Active:** 2026-04-04
**Priority:** P2 (Status Bar)

## Current State

**Phase 1 COMPLETE. Phase 2 COMPLETE. Phase 3 P1 COMPLETE (2026-04-04).**

### Phase 3 P1 Verified ✓ (2026-04-04)

| Feature | Status | Notes |
|---------|--------|-------|
| Tab bar | ✅ | Rectangular pill: badge + title segments; active=purple, inactive=gray |
| Scroll bar | ✅ | 6px right-edge overlay, proportional thumb, `build_scroll_bar_instances` |
| Tab bar rounded pills | ⏳ | TD-013 — needs GPU rounded-rect render pass |
| Tab bar bg transparency | ⏳ | TD-014 — BAR_BG should inherit `config.colors.background` |

### Keybinds (tmux-aligned, both embedded + user config updated)

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

### Phase 1 Polish Backlog (non-blocking)

| Item | Gap | File |
|------|-----|------|
| Title bar drag | `setMovableByWindowBackground:NO` | `app/mod.rs:143` |
| Scroll bar render | Config field exists, no GPU draw code | `config/schema.rs:11` |
| Double/triple-click selection | `SelectionType::Word/Line` not wired | `app/mod.rs:290` |
| OSC 52 clipboard read | `ClipboardLoad` not wired | `app/mux.rs:107` |

## Phase 3 Next Steps (ordered by priority)

1. **Status bar engine (P2)** — enable/disable from Lua + command palette
2. **Built-in status bar widgets** — `mode`, `cwd`, `git_branch`, `time`, `exit_code`
3. **Status bar widget Lua API** — `petruterm.statusbar.register_widget({ name, render })`
4. **Status bar position** — `top` or `bottom` (Lua config)
5. **Snippets (P3)** — `config.snippets` table, expand via palette, optional `trigger`
6. **Starship compatibility** — detect `STARSHIP_SHELL`, defer left prompt

## Files to Reference
- `src/app/renderer.rs` — `build_tab_bar_instances`, `build_scroll_bar_instances`
- `src/app/mod.rs` — `tab_bar_visible()`, `tab_bar_height_px()`, `apply_tab_bar_padding()`
- `src/app/ui.rs` — `handle_palette_action`, AI feature handlers
- `src/app/input/mod.rs` — leader dispatch, system keybinds
- `config/default/keybinds.lua` — embedded keybind defaults
- `~/.config/petruterm/keybinds.lua` — user keybind overrides
- `.context/quality/TECHNICAL_DEBT.md` — TD-013, TD-014 (tab bar polish)
