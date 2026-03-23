# Active Context

**Current Focus:** Phase 1 — .app bundle script
**Last Active:** 2026-03-23
**Target Completion:** Phase 1 MVP
**Priority:** P0

## Current State

**Working terminal as of this session:**
- Dracula Pro background `#22212c` ✓
- JetBrains Mono Nerd Font Mono 15pt, 18×36px at 2× Retina ✓
- zsh + Starship prompt, keyboard input, `ls` output ✓
- Mouse: drag selection, scroll wheel, SGR/X10 reporting ✓
- Clipboard: Cmd+C/V, OSC 52, bracketed paste ✓
- Cursor: block/underline/beam, 530ms blink, resets on keypress ✓
- PTY resize: uses actual cell px from TextShaper ✓
- Shell exit: `exit` / Ctrl+D closes the window ✓ (fixed: ChildExit + EventLoopProxy)
- Nerd Font icons: clamped to cell bounds, no row bleeding ✓
- Config hot-reload ✓
- Custom title bar: transparent, traffic lights native position, draggable ✓

## Scope

### Completed (Phase 1)
- `Cargo.toml` — all deps pinned
- `src/main.rs` — winit EventLoop + App + EventLoopProxy
- `src/app.rs` — full render loop; key/mouse input; clipboard; cursor blink;
  tab/pane commands; scale factor; glyph cell-clamping; terminal resize;
  custom title bar (objc2); ApplicationHandler<()> with user_event wakeup
- `src/renderer/gpu.rs` — wgpu 29 renderer (Metal)
- `src/renderer/pipeline.rs` — WGSL bg + glyph pipelines; FLAG_CURSOR in vs_bg
- `src/renderer/atlas.rs` — glyph atlas, Rgba8Unorm shelf packing
- `src/renderer/cell.rs` — CellVertex + CellUniforms + FLAG_CURSOR = 0x08
- `src/term/mod.rs` — Terminal wrapper; wakeup proxy threaded through
- `src/term/pty.rs` — PTY spawn; ChildExit + EventLoopProxy wakeup
- `src/term/color.rs` — AnsiColor → RGBA
- `src/font/loader.rs` — JetBrains Mono NF Mono bundled via include_bytes!
- `src/font/shaper.rs` — cosmic-text shaping + swash rasterization
- `src/config/` — Lua DSL, schema, watcher, hot-reload
- `src/ui/` — tabs, panes, command palette
- `assets/fonts/JetBrainsMonoNerdFontMono-*.ttf` — bundled (v3.3.0)
- `config/default/` — all 5 Lua config files

### Not Yet Implemented (Phase 1)
| Feature | Debt ID | Notes |
|---------|---------|-------|
| `.app` bundle script | — | `scripts/bundle.sh` |
| 100k scrollback verify | TD-004 | Not tested |
| Ligatures verify | — | `->` `=>` etc. not confirmed |
| `nvim` / `tmux` verify | — | Not smoke-tested |
| Top padding fix | — | `padding.top=30` slightly overlaps traffic lights (~44px needed) |

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
- [x] Nerd Font icons render (Starship, clamped to cell)
- [x] Mouse: drag selection, scroll wheel, SGR/X10
- [x] Clipboard: Cmd+C/V, OSC 52
- [x] Cursor rendering: block/underline/beam, blink
- [x] Resize handling: terminal grid + PTY resize on window resize
- [x] Shell exit closes window
- [x] Custom title bar / borderless
- [ ] Font ligatures verified
- [ ] `nvim` renders correctly
- [ ] `tmux` works
- [ ] 100k scrollback

## Technical Reference
- Shell exit: alacritty_terminal 0.25.1 sends `Event::ChildExit(i32)`, not `Event::Exit`
- EventLoopProxy: `wakeup.send_event(())` wakes NSApp immediately from PTY thread
- Custom title bar: `HasWindowHandle → AppKitWindowHandle.ns_view → [view window]`
  then `setStyleMask | (1<<15)`, `setTitlebarAppearsTransparent`, `setTitleVisibility:1`,
  `setMovableByWindowBackground`
- Surface: non-sRGB `Bgra8Unorm` on Metal
- Atlas: `Rgba8Unorm`, mask `[a,a,a,255]`, shader samples `.r`
- Scale: `window.scale_factor()` = 2.0 on M4 Max
- Cursor flag: `FLAG_CURSOR = 0x08`
- PTY cell dims: `shaper.cell_width as u16`, `shaper.cell_height as u16`

## Files to Reference
- `.context/specs/term_specs.md` — authoritative spec
- `.context/specs/build_phases.md` — Phase 1 deliverables checklist
- `.context/quality/TECHNICAL_DEBT.md` — open debt items
