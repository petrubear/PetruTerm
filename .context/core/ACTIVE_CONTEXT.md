# Active Context

**Current Focus:** Phase 1 — TD-003 PTY cell dimensions from shaper
**Last Active:** 2026-03-23
**Target Completion:** Phase 1 MVP
**Priority:** P0

## Current State

**Working terminal as of commit 67de8b6:**
- Dracula Pro background `#22212c` ✓
- JetBrains Mono Nerd Font Mono 15pt, 18×36px physical at 2× Retina ✓
- zsh + Starship prompt, keyboard input, `ls` output ✓
- Mouse: drag selection, scroll wheel, SGR/X10 reporting ✓
- Clipboard: Cmd+C/V, OSC 52, bracketed paste ✓
- Cursor: block/underline/beam shapes, 530ms blink ✓
- Config hot-reload ✓
- Known visual issue: Nerd Font icons overflow cell bounds (TD-012)
- Known functional issue: `exit` doesn't close window (TD-011)

## Scope

### Completed
- `Cargo.toml` — all Phase 1 deps pinned + arboard
- `src/main.rs` — winit EventLoop + App
- `src/app.rs` — full render loop, key/mouse input, clipboard, cursor blink,
  tab/pane commands, config reload, scale factor
- `src/renderer/gpu.rs` — full wgpu renderer (Metal)
- `src/renderer/pipeline.rs` — WGSL bg + glyph pipelines; FLAG_CURSOR support
- `src/renderer/atlas.rs` — glyph atlas, Rgba8Unorm
- `src/renderer/cell.rs` — CellVertex + CellUniforms + FLAG_CURSOR
- `src/term/mod.rs` — Terminal wrapper, selection, scroll, cursor, mouse mode APIs
- `src/term/pty.rs` — PTY spawn + I/O + OSC 52/PtyWrite events
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
| PTY cell px from shaper | TD-003 | cell_width/height hardcoded 8×16 in PTY resize |
| `exit` closes window | TD-011 | PtyEvent::Exit received but ignored |
| Custom title bar | — | Borderless + objc2 traffic lights |
| `.app` bundle script | — | `scripts/bundle.sh` |
| Nerd Font icon sizing | TD-012 | Icons overflow cell height (cosmetic) |
| 100k scrollback verify | TD-004 | Not tested |
| Ligatures verify | — | `->` `=>` etc. not confirmed |
| `nvim` / `tmux` verify | — | Not tested |

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
- [x] Mouse: drag selection, scroll wheel, SGR/X10
- [x] Clipboard: Cmd+C/V, OSC 52
- [x] Cursor rendering: block/underline/beam, blink
- [ ] Font ligatures verified
- [ ] Custom title bar / borderless
- [ ] `nvim` renders correctly
- [ ] `tmux` works
- [ ] 100k scrollback

## Technical Reference
- Surface: non-sRGB (`Bgra8Unorm` on Metal)
- Atlas: `Rgba8Unorm`, glyph mask as `[a,a,a,255]`, shader samples `.r`
- Scale: `window.scale_factor()` = 2.0 on M4 Max → 30pt font → 18×36px cell
- Cursor flag: `FLAG_CURSOR = 0x08`; `vs_bg` uses `glyph_offset`/`glyph_size` as rect
- `build_instances(cell_data, shaper, renderer, config, font, cursor, blink_on)`
- `active_terminal() -> Option<&Terminal>` — pane/tab lookup helper
- PTY resize uses hardcoded `cell_width: 8, cell_height: 16` (TD-003)

## Files to Reference
- `.context/specs/term_specs.md` — authoritative spec
- `.context/specs/build_phases.md` — Phase 1 deliverables checklist
- `.context/quality/TECHNICAL_DEBT.md` — open debt items
