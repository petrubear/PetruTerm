# Session State

**Last Updated:** 2026-04-06
**Session Focus:** Bug fixes + UX polish (mouse selection, config, palette keybinds, context menu)

## Branch: `master`

## Session Notes (2026-04-06)

### Bug: Mouse selection broken (fixed)
- **Root cause:** `setMovableByWindowBackground: YES` in `apply_macos_custom_titlebar` made the entire window background draggable, overriding text selection drag.
- **Fix:** Changed to `Bool::NO`. The explicit `drag_window()` call at `y < padding.top` already handles title bar area dragging.
- File: `src/app/mod.rs`

### Default config — all fields now included
- `config/default/ui.lua` — added `font_line_height`, `font_fallbacks`, `lcd_antialiasing`, commented initial_width/height
- `config/default/llm.lua` — added full `ui` section (width_cols, background, user_fg, assistant_fg, input_fg)
- `config/default/perf.lua` — added `shell`, `animation_fps`, `gpu_preference`
- `src/config/mod.rs` — replaced `copy_default_configs` (ran only on first launch) with `ensure_default_configs` which writes any missing file on every startup without overwriting existing ones

### Command palette keybinds
- `PaletteAction` gained `keybind: Option<String>` field
- `built_in_actions(&Config)` now builds a leader shortcut lookup from `config.keys` (format: `^B c`, `Cmd+Q`, etc.)
- Keybinds rendered right-aligned in a dimmed color (`[0.5, 0.5, 0.7, 1.0]`) in `build_palette_instances`
- Hot-reload calls `palette.rebuild_keybinds(&config)` to reflect keybind changes
- Files: `src/ui/palette/actions.rs`, `src/ui/palette/mod.rs`, `src/app/renderer.rs`

### Right-click context menu (new feature)
- New `src/ui/context_menu.rs` — `ContextMenu`, `ContextAction`, `ContextMenuItem`
- Items: **Copy** `Cmd+C`, **Paste** `Cmd+V`, **Clear` — same layout as palette (name left, keybind right)
- Popup rendered at click cell position, clamped to terminal bounds
- Hover highlight tracks `CursorMoved`; closes on click-outside, any key press, or item selection
- In mouse-reporting mode (e.g. vim) right-click still passes through to the terminal app
- Files: `src/ui/context_menu.rs`, `src/ui/mod.rs`, `src/app/ui.rs`, `src/app/mod.rs`, `src/app/renderer.rs`

## Build Status
- **cargo build:** PASS (0 errors — 2026-04-06)
- **branch:** master (stable)

## Previous Sessions

### Phase 2.5 P2 — LLM Tool Use (2026-04-05) — COMPLETE
Tool use loop: `AgentTool` (ReadFile, ListDir), `agent_step()` in providers, max-10 round loop, `ToolStatus` events.

### Phase 2.5 P1 — AI Agent Mode (2026-04-05) — COMPLETE
File picker, `AGENTS.md` auto-load, CWD from `proc_pidinfo`, Ctrl+S submit, /q/quit.

### Phase 3 P1 — Tab bar + Scroll bar (2026-04-04) — COMPLETE
Rounded pill tabs (SDF WGSL), proportional scroll bar, TD-013/TD-014 resolved.

## Key Technical Decisions (standing)

### Mouse drag vs window drag
- `y < padding.top` (60px physical) → `drag_window()` (title bar area only)
- `setMovableByWindowBackground: NO` — rest of window is NOT draggable

### Context menu architecture
- `ContextMenu` stored in `UiManager.context_menu` (pub field)
- Right-click in terminal area (not panel, not mouse-reporting mode) → opens menu
- Mouse-reporting mode → right-click passes through as button 2 SGR/X10 report

### Palette keybind resolution
- `built_in_actions(&Config)` builds leader map from `config.keys` at palette construction
- `rebuild_keybinds(&Config)` called after hot-reload to keep labels in sync
