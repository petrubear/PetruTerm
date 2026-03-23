# Session State

**Last Updated:** 2026-03-22
**Session Focus:** Phase 1 — Cell rendering milestone + font fixes

## Session Summary
Full render pipeline wired and verified working on macOS (Apple M4 Max, Retina 2×).
PetruTerm is now a functional terminal: Dracula Pro theme, JetBrains Mono Nerd Font Mono
15pt, Starship prompt with correct icons, keyboard input, zsh output.

## Completed This Session
- [x] `src/term/color.rs` — `resolve_color(AnsiColor, &ColorScheme) -> [f32;4]`
- [x] `src/term/mod.rs` — added `pub mod color`
- [x] `src/renderer/gpu.rs` — full GPU pipeline wired: CellPipeline + GlyphAtlas +
      uniform buffer + instance buffer; bg pass + glyph pass render
- [x] `src/renderer/atlas.rs` — format changed to `Rgba8Unorm` (was sRGB)
- [x] `src/font/loader.rs` — bundled JetBrains Mono Nerd Font Mono v3.3.0 via
      `include_bytes!`; font system init loads them before system scan
- [x] `src/font/shaper.rs` — fixed glyph mask RGBA: `[a,a,a,255]` so `.r` = coverage
- [x] `src/config/lua.rs` — fixed `require('petruterm')` via `package.preload`
- [x] `src/config/schema.rs` — default font: "JetBrainsMono Nerd Font Mono", size 15
- [x] `src/app.rs` — full render loop: collect_grid_cells → build_instances → upload →
      render; `scale_factor` from `window.scale_factor()`; `scaled_font_config()`
- [x] `assets/fonts/JetBrainsMonoNerdFontMono-{Regular,Bold,Italic,BoldItalic}.ttf`
- [x] `config/default/ui.lua` — font family + size updated

## Build Status
- **cargo build:** PASS — 0 errors, 23 warnings (dead code stubs only)
- **Runtime:** PASS
  - Config loads successfully
  - GPU: Apple M4 Max (Metal)
  - Scale factor: 2× (Retina)
  - Font: JetBrainsMono Nerd Font Mono 15pt (30pt physical), cell 18×36px
  - PTY: /bin/zsh spawned
  - Visual: Dracula Pro bg, prompt + ls output render correctly, Nerd Font icons work

## In Progress
- [ ] None — stopping here for the session

## Next Session Priorities (in order)
1. Mouse handling — CursorMoved, MouseInput, MouseWheel → alacritty mouse processor (TD-006)
2. Clipboard — Cmd+C/V + OSC 52 via `arboard` crate (TD-007)
3. Cursor rendering — block/underline/beam shapes, blinking
4. PTY cell dimensions from shaper — fix cell_width/cell_height in resize (TD-003)
5. Custom title bar — borderless + traffic lights via objc2
6. `.app` bundle script — `scripts/bundle.sh`
7. 100k scrollback verification (TD-004)

## Key Technical Decisions
- Non-sRGB surface format — hex colors are sRGB-space, no double gamma encoding
- Atlas `Rgba8Unorm` — mask coverage is linear, not a display color
- Glyph mask: `[a,a,a,255]` storage, shader reads `.r` for coverage
- `scale_factor = window.scale_factor()` (2.0 on M4 Max Retina) × font size
- `scaled_font_config()` returns `config.font` with `size *= scale_factor`
- `build_instances()` free fn takes explicit `font: &FontConfig` to avoid borrow conflicts
- Nerd Font Mono variant chosen (not standard NF): icons forced to single-cell width
- Grid walk: requires `use alacritty_terminal::grid::Dimensions` in scope

## Files Modified This Session
- `src/term/color.rs` (new)
- `src/term/mod.rs`
- `src/renderer/gpu.rs`
- `src/renderer/atlas.rs`
- `src/font/loader.rs`
- `src/font/shaper.rs`
- `src/config/lua.rs`
- `src/config/schema.rs`
- `src/app.rs`
- `assets/fonts/JetBrainsMonoNerdFontMono-*.ttf` (new, replaces plain JetBrains Mono)
- `config/default/ui.lua`
