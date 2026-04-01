# Session State

**Last Updated:** 2026-03-31
**Session Focus:** AI Panel — Bug Fixes & UI Redesign (COMPLETE)

## Branch: `master`

## Session Close Notes (2026-03-31)

### AI Panel — Fixes & Redesign
- **Root cause fixed:** `resize_terminals_for_panel()` was never called on panel open/close — panel rendered off-screen (past terminal right edge). Fixed by detecting panel visibility change in `KeyboardInput` handler.
- **GPU upload fixed:** Dirty-row optimization produced wrong buffer offsets when panel instances were appended after terminal rows. Now uses full `upload_instances` when panel is visible.
- **Keybind changed:** Ctrl+C (broke SIGINT) → `Cmd+Shift+A` cycle (open+focus → focus → close).
- **UI redesign:** `│` border on every row, blinking `▋` input cursor, braille spinner `⠋⠙⠹…` during loading/streaming, `▸` prompt, contextual hints, improved Dracula Pro colors.
- **keybinds.lua updated:** Added `{ mods = "CMD|SHIFT", key = "A", action = petruterm.action.ToggleAiPanel }`.

## Build Status
- **cargo check:** PASS — 0 errors.
- **branch:** master (stable).

## Key Technical Decisions

### Modular Architecture
- **Managers:** Decomposed logic into `renderer`, `mux`, `ui`, and `input`. This drastically improved compile times for individual components and simplified testing.
- **Shader Synchronization:** `vs_bg` and `vs_main` must share the exact same rounding logic (`floor` + `epsilon`) to avoid pixel-seams.
- **Standard Input:** Adopted xterm-style modifier encoding to ensure maximum compatibility with CLI tools.

### AI Panel Architecture
- Panel instances appended after terminal instances in CPU array — must use full GPU upload (not dirty-row) when visible.
- `resize_terminals_for_panel()` must be called whenever panel visibility changes to keep terminal grid width correct.
- Keybind: `Cmd+Shift+A` (toggle cycle). Esc closes when focused.
