# Session State

**Last Updated:** 2026-03-23
**Session Focus:** Phase 1 — TD-003 PTY cell dimensions from shaper

## Session Summary
Mouse handling, clipboard, and cursor rendering all completed and committed
(commit 67de8b6). Build passes 0 errors. Runtime verified on M4 Max / Metal.
Two new debt items logged from visual inspection (TD-011, TD-012).

## Completed This Session
- [x] Mouse handling (TD-006) — CursorMoved/MouseInput/MouseWheel; drag selection
      via alacritty Selection API; SGR + X10 mouse reporting; scrollback scroll wheel
- [x] Clipboard (TD-007) — arboard; Cmd+C/V; OSC 52 via PtyEvent channel;
      bracketed paste wrapping; PtyWrite forwarding
- [x] Cursor rendering — CursorInfo + cursor_info(); FLAG_CURSOR bit in vs_bg shader;
      block/underline/beam shapes; 530ms blink via ControlFlow::WaitUntil; reset on keypress
- [x] TD-011 logged — `exit` command doesn't close window
- [x] TD-012 logged — Nerd Font icons overflow cell bounds (visual, from screenshot)

## Build Status
- **cargo build:** PASS — 0 errors, 22 warnings (dead code stubs only)
- **Runtime:** PASS — Metal GPU, 18×36px cell, PTY spawns, cursor visible, Starship renders

## In Progress
- [ ] TD-003 — PTY cell dimensions from shaper (started this session)

## Next Session Priorities (in order)
1. Custom title bar — borderless + traffic lights via objc2
2. `.app` bundle script — `scripts/bundle.sh`
3. TD-011 — `exit` doesn't close window
4. TD-012 — Nerd Font icons overflow cell bounds
5. 100k scrollback verification (TD-004)
6. Ligatures verify (`->` `=>` etc.)

## Key Technical Decisions
- Cursor: FLAG_CURSOR (0x08) in CellVertex.flags; vs_bg uses glyph_offset/glyph_size
  as partial-cell rect when flag is set — no extra GPU pass needed
- Cursor blink: ControlFlow::WaitUntil(now + 530ms) in about_to_wait; resets on keypress
- Mouse scroll: Scroll::Delta(-lines), positive LineDelta y = wheel up = show history
- SGR mouse: \x1b[<btn;col;rowM/m (1-indexed); X10: \x1b[Mbxy press-only
- Clipboard: arboard::Clipboard created per-call on main thread (not stored in App)
- OSC 52: PtyEvent::ClipboardStore/Load routed via channel to main thread

## Files Modified This Session
- `src/app.rs`
- `src/term/mod.rs`
- `src/term/pty.rs`
- `src/renderer/cell.rs`
- `src/renderer/pipeline.rs`
- `Cargo.toml` (added arboard = "3")
