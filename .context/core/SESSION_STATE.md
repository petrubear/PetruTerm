# Session State

**Last Updated:** 2026-04-03
**Session Focus:** Bug fixes (mouse selection, typing delay, font memory) + TD-040 Leader Key (COMPLETE)

## Branch: `master`

## Session Close Notes (2026-04-03)

### Bug Fixes (commit 096f41f)

**Mouse selection** (`src/term/mod.rs`, `src/app/mod.rs`)
- `start_selection`/`update_selection` now lock the term once and subtract
  `display_offset` from the viewport row, anchoring selections in buffer space.
  This fixes incorrect selection highlighting when scrolled.
- `MouseWheel` handler calls `update_selection` when `mouse_left_pressed`,
  so dragging into scrollback history works.

**Typing delay** (`src/app/mod.rs`)
- `user_event` now checks `has_data` from `poll_pty_events()` and calls
  `request_redraw()` immediately when PTY output arrives. Previously characters
  only appeared on the next independent event (mouse move, 530ms blink tick).

**Font memory** (`src/app/renderer.rs`, `src/font/locator.rs`)
- Removed `locate_font_for_lcd` call from per-frame `scaled_font_config()`.
  This was allocating ~200 KB (`JBM_REGULAR.to_vec()`) every frame for JBM
  Nerd Font users — ~12 MB/s of unnecessary heap churn.
- `locate_via_font_kit` now uses `source.select_best_match()` instead of
  loading every font variant to find the Regular weight — much lower memory.

### TD-040: Leader Key Action Dispatch (commit 8e55d0f)

All custom terminal keybinds now route through the leader key (default: Ctrl+B).

- `schema.rs`: `KeyBind` struct + `keys: Vec<KeyBind>` field on `Config`
- `lua.rs`: parses `config.leader` and `config.keys` tables from Lua;
  `petruterm.action` table now includes all action names
- `actions.rs`: added `CommandPalette` + `ToggleAiPanel` variants; `FromStr`
  impl maps action name strings → `Action` values
- `ui.rs`: `CommandPalette` opens the palette; `ToggleAiPanel`/`ToggleAiMode`
  do the full open → focus → close cycle
- `input/mod.rs`: `InputHandler::new(&Config)` builds `leader_map` from config;
  removed hardcoded `Cmd+Shift+P`, `Cmd+Shift+A`, `Ctrl+Shift+E/F`, `Cmd+T/W`
- `keybinds.lua`: single source of truth — all custom binds via `LEADER`

**New default bindings** (all after Ctrl+B):
| Key | Action |
|-----|--------|
| p   | Command Palette |
| a   | Toggle AI Panel (open → focus → close) |
| e   | Explain Last Output |
| f   | Fix Last Error |
| t   | New Tab |
| w   | Close Tab |
| %   | Split Horizontal |
| "   | Split Vertical |
| x   | Close Pane |

**System keybinds kept hardcoded (not leader):**
- `Cmd+C/V` — clipboard copy/paste
- `Cmd+Q` — quit
- `Cmd+1-9` — switch to tab N

## Build Status
- **cargo check:** PASS — 0 errors.
- **branch:** master (stable).

## Key Technical Decisions

### Modular Architecture
- **Managers:** `renderer`, `mux`, `ui`, and `input` — drastically improved
  compile times and testability.
- **Shader Synchronization:** `vs_bg` and `vs_main` share `floor` + `epsilon`
  rounding to avoid pixel-seams.
- **Standard Input:** xterm-style modifier encoding for CLI tool compatibility.

### Leader Key Architecture
- `leader_map: HashMap<String, Action>` built once at startup from `config.keys`.
- Leader trigger: `ctrl && !shift && !cmd && s == config.leader.key`.
- Dispatch: looks up pressed char in `leader_map`, falls back to lowercase.
- `Action::Quit` handled before `handle_palette_action` (needs `event_loop`).
- Adding a new binding requires only a Lua change — no Rust recompile.

### AI Panel Architecture
- Panel instances appended after terminal instances — full GPU upload required.
- `resize_terminals_for_panel()` called whenever panel visibility changes.
- Keybind: `<leader>a` (open → focus → close cycle). Esc closes when focused.
