# Session State

**Last Updated:** 2026-03-24
**Session Focus:** Phase 1 — scroll + padding bug fixes

## Session Summary
Scroll and padding bugs found and fixed. All Phase 1 runtime items now verified.
Build: 0 errors. Runtime verified on M4 Max.

## Commits This Arc (chronological)
- `a480c12` fix: arrow keys respect APP_CURSOR mode (DECCKM) — TD-013
- `3cc8d1f` fix: space key + TERM env in PTY
- `bc8df52` fix: PtyWrite responses on background thread
- `fa86d2e` fix: share direct_notifier Arc (TD-002) — atuin cursor query works
- `56261bf` chore: remove debug file logging
- `a7ec4b0` fix: trackpad scroll accumulation + padding.top 30→44
- `92f77bf` fix: scroll PixelDelta unit mismatch (logical pts vs physical px)
- `4883895` fix: scroll rendering ignores display_offset + padding top 44→60

## Build Status
- **cargo build:** PASS — 0 errors, ~19 warnings (dead code stubs only)
- **bundle:** PASS — dist/PetruTerm.app, 18 MB, ad-hoc signed, icon embedded

## Root Cause Summary (scroll)
- `grid()[Line(row)]` in alacritty_terminal does NOT account for `display_offset`.
  Fixed by using `Line(row as i32 - display_offset)` in `collect_grid_cells`.
- `PixelDelta.y` from macOS trackpad is in logical points; dividing by physical
  cell_height (36px) made all deltas round to 0. Fixed by dividing by
  `cell_height / scale_factor` (18pt on 2× Retina).

## In Progress
- [ ] None — clean handoff

## Next Session Priorities (in order)
1. Verify Ctrl key works (Ctrl+U, Ctrl+A, Ctrl+C in shell; Ctrl+B prefix in tmux)
2. `nvim` smoke test — colors (reverse-video now fixed), cursor, input, scroll
3. `tmux` smoke test — catppuccin separators (INVERSE fixed), split, scroll
4. Ligatures verify — `->` `=>` `!=` `>=` `|>` in nvim

## Key Technical Decisions (stable)
- Surface: non-sRGB `Bgra8Unorm` on Metal — hex colors stored as sRGB, no double-gamma
- Atlas: `Rgba8Unorm`, glyph mask as `[a,a,a,255]`, shader samples `.r` for coverage
- Scale: `window.scale_factor()` = 2.0 on M4 Max; font 15pt → 30pt physical → 18×36px cell
- Cursor: `FLAG_CURSOR = 0x08`; `vs_bg` uses `glyph_offset`/`glyph_size` as cursor rect
- Blink: 530ms toggle in `about_to_wait` via `ControlFlow::WaitUntil`, reset on keypress
- Shell exit: `Event::ChildExit(i32)` — alacritty_terminal 0.25.1 variant (not `Event::Exit`)
- EventLoopProxy: `wakeup.send_event(())` wakes NSApp immediately on any PTY event
- Custom title bar: `HasWindowHandle → ns_view → [view window]` + FullSizeContentView
- Working directory: `dirs::home_dir()` passed to PtyOptions on spawn

## Files Modified (this arc)
- `src/app.rs`
- `src/main.rs`
- `src/term/mod.rs`
- `src/term/pty.rs`
- `config/default/ui.lua`
- `scripts/bundle.sh` (new)
- `scripts/gen_icon.swift` (new)
- `assets/AppIcon.png` (new)
