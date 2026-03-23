# Session State

**Last Updated:** 2026-03-23
**Session Focus:** Phase 1 — bundle script (next session start)

## Session Summary
Phase 1 feature-complete. Custom title bar implemented, shell exit fixed.
Build: 0 errors, 19 warnings (dead code stubs only). Runtime verified on M4 Max.

## Commits This Arc (chronological)
- `a666bb0` feat: Phase 1 MVP — core terminal foundation
- `67de8b6` feat: Phase 1 — mouse, clipboard, and cursor rendering
- `8a63522` fix: PTY cell dimensions from font shaper (TD-003)
- `49f17d9` fix: shell exit now closes the window (TD-011)
- `cf11e3a` fix: clamp Nerd Font / oversized glyphs to cell bounds (TD-012)
- `6325719` feat: custom title bar via objc2 (TitleBarStyle::Custom/None)
- (pending) fix: shell exit via Event::ChildExit + EventLoopProxy wakeup

## Build Status
- **cargo build:** PASS — 0 errors, 19 warnings (dead code stubs only)
- **Runtime:** PASS — Metal GPU, custom title bar, traffic lights, PTY, exit closes window

## In Progress
- [ ] None — clean handoff

## Next Session Priorities (in order)
1. `.app` bundle script — `scripts/bundle.sh`
2. TD-004 — 100k scrollback verification (`printf '%s\n' {1..110000}`)
3. Ligatures verify — `->` `=>` `!=` `>=` `|>` in the terminal
4. `nvim` / `tmux` smoke test
5. Top padding fix — increase `padding.top` so terminal row 0 clears the traffic lights

## Key Technical Decisions (stable)
- Surface: non-sRGB `Bgra8Unorm` on Metal — hex colors stored as sRGB, no double-gamma
- Atlas: `Rgba8Unorm`, glyph mask as `[a,a,a,255]`, shader samples `.r` for coverage
- Scale: `window.scale_factor()` = 2.0 on M4 Max; font 15pt → 30pt physical → 18×36px cell
- Cursor: `FLAG_CURSOR = 0x08`; `vs_bg` uses `glyph_offset`/`glyph_size` as cursor rect
- Blink: 530ms toggle in `about_to_wait` via `ControlFlow::WaitUntil`, reset on keypress
- Mouse scroll: `Scroll::Delta(-lines)` — positive LineDelta y = wheel up = show history
- Cell dims: `TextShaper::cell_width/height` (physical px) passed through to TIOCSWINSZ
- Glyph clamping: `clamp_glyph_to_cell()` crops bitmap + UV to cell bounds
- Shell exit: `Event::ChildExit(i32)` is the correct alacritty_terminal 0.25.1 variant
  (not `Event::Exit`). EventLoopProxy wakes NSApp immediately on any PTY event.

## Files Modified (this arc)
- `src/app.rs`
- `src/main.rs`
- `src/term/mod.rs`
- `src/term/pty.rs`
- `config/default/ui.lua`
