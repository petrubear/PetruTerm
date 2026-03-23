# Session State

**Last Updated:** 2026-03-23
**Session Focus:** Phase 1 — Mouse handling, clipboard, cursor rendering

## Session Summary
Mouse handling implemented. Full scroll wheel, drag selection, SGR/X10 mouse
reporting for tmux/nvim all wired. Terminal + PTY layer extended with
selection/scroll APIs.

## Completed This Session
- [x] `src/term/mod.rs` — added `start_selection`, `update_selection`,
      `selection_text`, `clear_selection`, `scroll_display`, `mouse_mode_flags`,
      `bracketed_paste_mode`, `cursor_info`, `CursorInfo`, `CursorShape` re-export
- [x] `src/term/pty.rs` — extended `PtyEvent` with `ClipboardStore`, `ClipboardLoad`,
      `PtyWrite`; `send_event` now routes OSC 52 + PtyWrite events
- [x] `src/renderer/cell.rs` — added `FLAG_CURSOR = 0x08` constant
- [x] `src/renderer/pipeline.rs` — `vs_bg` extended: uses `glyph_offset`/`glyph_size`
      for cursor rect when `FLAG_CURSOR` bit is set
- [x] `src/app.rs` — cursor blink fields + 530ms toggle in `about_to_wait`;
      `WaitUntil` control flow for efficient blinking; cursor instance emitted in
      `build_instances` (block/underline/beam shapes); blink reset on keypress;
      mouse handling, clipboard (Cmd+C/V, OSC 52)

## Build Status
- **cargo build:** PASS — 0 errors, 22 warnings (dead code stubs only)
- **Runtime smoke test:** PASS — config loads, Metal GPU, 18×36px cell, PTY spawns

## In Progress
- [ ] None — stopping here for the session

## Next Session Priorities (in order)
1. PTY cell dimensions from shaper — fix cell_width/cell_height in resize (TD-003)
2. Cursor rendering — block/underline/beam shapes, blinking
3. PTY cell dimensions from shaper — fix cell_width/cell_height in resize (TD-003)
4. Custom title bar — borderless + traffic lights via objc2
5. `.app` bundle script — `scripts/bundle.sh`
6. 100k scrollback verification (TD-004)

## Key Technical Decisions
- Mouse scroll: `Scroll::Delta(-lines)` where `lines` = LineDelta y (positive = wheel up = show history)
- SGR mouse report: `\x1b[<{btn};{col1};{row1}M/m` (1-indexed, M=press m=release)
- X10 encoding: `\x1b[M{btn+32}{x+32}{y+32}` clamped to 255, press-only
- Scroll wheel buttons: 64=up, 65=down (standard SGR encoding)
- Motion drag: button 32 = left-button held (SGR drag report)
- `active_terminal()` helper avoids repeated pane/tab lookup boilerplate

## Files Modified This Session
- `src/term/mod.rs`
- `src/app.rs`
