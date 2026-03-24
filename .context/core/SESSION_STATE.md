# Session State

**Last Updated:** 2026-03-23
**Session Focus:** Phase 1 — verification (scrollback, ligatures, nvim/tmux)

## Session Summary
Phase 1 nearly complete. Custom title bar, shell exit, bundle, home dir, icon — all done.
Build: 0 errors, 19 warnings (dead code stubs only). Runtime verified on M4 Max.

## Commits This Arc (chronological)
- `a666bb0` feat: Phase 1 MVP — core terminal foundation
- `67de8b6` feat: Phase 1 — mouse, clipboard, and cursor rendering
- `8a63522` fix: PTY cell dimensions from font shaper (TD-003)
- `49f17d9` fix: shell exit now closes the window (TD-011)
- `cf11e3a` fix: clamp Nerd Font / oversized glyphs to cell bounds (TD-012)
- `6325719` feat: custom title bar via objc2 (TitleBarStyle::Custom/None)
- `2a92400` fix: shell exit via Event::ChildExit + EventLoopProxy wakeup (TD-011)
- `2ffb628` feat: .app bundle script (scripts/bundle.sh)
- `fabccfb` feat: launch in home dir + app icon

## Build Status
- **cargo build:** PASS — 0 errors, 19 warnings (dead code stubs only)
- **bundle:** PASS — dist/PetruTerm.app, 18 MB, ad-hoc signed, icon embedded

## In Progress
- [ ] None — clean handoff

## Next Session Priorities (in order)
1. TD-013 — Fix arrow keys in APP_CURSOR mode (breaks atuin, nvim, tmux)
2. TD-004 — 100k scrollback (`printf '%s\n' {1..110000}`)
3. Ligatures verify — `->` `=>` `!=` `>=` `|>` in nvim or shell
4. `nvim` smoke test — colors, cursor, input, scroll
5. `tmux` smoke test — attach, split, scroll
6. Top padding minor fix — `padding.top` to ~44 so row 0 clears traffic lights

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
