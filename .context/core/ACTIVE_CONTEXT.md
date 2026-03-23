# Active Context

**Current Focus:** Phase 1 — Mouse, clipboard, cursor rendering
**Last Active:** 2026-03-22
**Target Completion:** Phase 1 MVP
**Priority:** P0

## Current State

**Working terminal as of end of session:**
- Dracula Pro background `#22212c` ✓
- JetBrains Mono Nerd Font Mono 15pt at correct Retina size (18×36px physical at 2×) ✓
- zsh prompt with Starship (icons render correctly with NF) ✓
- Keyboard input → PTY ✓
- `ls` output with file-type icons ✓
- Config hot-reload wired ✓
- Lua config loading (`require('petruterm')` works) ✓

**Remaining Phase 1 items (not yet implemented):**
Mouse handling, clipboard, cursor rendering, title bar, bundle script.

## Scope

### Completed
- `Cargo.toml` — all Phase 1 deps pinned
- `src/main.rs` — winit EventLoop + App
- `src/app.rs` — render loop, key input, tab/pane commands, scale factor
- `src/renderer/gpu.rs` — full GPU renderer
- `src/renderer/pipeline.rs` — WGSL bg + glyph pipelines
- `src/renderer/atlas.rs` — glyph atlas, Rgba8Unorm
- `src/renderer/cell.rs` — CellVertex + CellUniforms
- `src/term/mod.rs` — Terminal wrapper
- `src/term/pty.rs` — PTY spawn + I/O
- `src/term/color.rs` — AnsiColor → RGBA
- `src/font/loader.rs` — JetBrains Mono NF Mono bundled
- `src/font/shaper.rs` — cosmic-text shaping + swash rasterization
- `src/config/` — Lua DSL, schema, watcher, hot-reload
- `src/ui/` — tabs, panes, command palette
- `assets/fonts/JetBrainsMonoNerdFontMono-*.ttf` — bundled (v3.3.0)
- `config/default/` — all 5 Lua config files

### Not Yet Implemented (Phase 1)
| Feature | Debt ID | Notes |
|---------|---------|-------|
| Mouse handling | TD-006 | CursorMoved, MouseInput, MouseWheel, SGR/X10 |
| Clipboard | TD-007 | Cmd+C/V, OSC 52, `arboard` crate |
| Cursor rendering | — | Block/underline/beam, blinking |
| PTY cell px from shaper | TD-003 | cell_width/height hardcoded at 8×16 |
| Custom title bar | — | Borderless + objc2 traffic lights |
| `.app` bundle script | — | `scripts/bundle.sh` |
| 100k scrollback verify | TD-004 | Not tested |
| Ligatures verify | — | `->` `=>` etc. not confirmed |

### Out of Scope (Phase 2+)
- `src/llm/` — Phase 2
- `src/plugins/` — Phase 3
- `src/snippets/` — Phase 3
- `src/ui/statusbar/` — Phase 3

## Acceptance Criteria Status
- [x] `cargo build` — zero errors
- [x] Window opens, Dracula Pro background
- [x] PTY spawns zsh, keyboard input works
- [x] wgpu renders terminal cells (colors + font correct)
- [x] HiDPI Retina correct sizing
- [x] JetBrains Mono Nerd Font Mono bundled
- [x] Lua config loads and hot-reloads
- [x] Nerd Font icons render (Starship)
- [ ] Font ligatures verified
- [ ] Mouse: click-focus, selection, scroll, SGR/X10
- [ ] Clipboard: Cmd+C/V, OSC 52
- [ ] Cursor rendering
- [ ] Custom title bar / borderless
- [ ] `nvim` renders correctly
- [ ] `tmux` works
- [ ] 100k scrollback

## Next Actions
1. Add `arboard` to `Cargo.toml`; implement clipboard in `app.rs`
2. Handle `WindowEvent::CursorMoved | MouseInput | MouseWheel` in `window_event()`
3. Add cursor cell to `build_instances()` — extra CellVertex with `flags = CURSOR`
4. Wire `cursor_bg`/`cursor_fg` from ColorScheme into cursor instance

## Technical Reference
- Surface: non-sRGB (`Bgra8Unorm` on Metal)
- Atlas: `Rgba8Unorm`, mask as `[a,a,a,255]`, shader samples `.r`
- Scale: `window.scale_factor()` = 2.0 on M4 Max, stored in `App.scale_factor`
- Font: `"JetBrainsMono Nerd Font Mono"`, family registered by fontdb from TTF metadata
- Grid walk: `use alacritty_terminal::grid::Dimensions` required for `screen_lines()`/`columns()`
- `build_instances(cell_data, shaper, renderer, config, font)` — `font` is scaled variant

## Files to Reference
- `.context/specs/term_specs.md` — authoritative spec
- `.context/specs/build_phases.md` — Phase 1 deliverables checklist
- `.context/quality/TECHNICAL_DEBT.md` — open debt items
