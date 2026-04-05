# Session State

**Last Updated:** 2026-04-04
**Session Focus:** Bug fixes — AI panel performance + leader key capture

## Branch: `master`

## Session Notes (2026-04-04)

### Context audit (commit 7e691b9)
Full codebase audit — `build_phases.md` updated to reflect actual implementation state.
- `<leader>e`/`<leader>f` and shell integration script were already complete.
- Added `[⏎ Run]` bar to AI chat panel (green `│ ⏎ cmd` line after AI response).
- Fixed stale `Cmd+Shift+A` hints → `<Leader>a`.

### Bug fixes (commit d2502cb)

**Performance — AI panel reshaping every frame**
- Root cause: `build_chat_panel_instances` called `shape_line` (HarfBuzz) for every
  visible line on every redraw, even when panel content hadn't changed.
- Fix: `ChatPanel.dirty` flag set on every mutation; `RenderContext.panel_instances_cache`
  stores last built `Vec<CellVertex>`. Redraw skips reshape when `dirty == false`,
  only doing a fast `extend_from_slice` from cache.
- Cursor blink and terminal resize still mark `dirty = true` to stay correct.
- Files: `src/llm/chat_panel.rs`, `src/app/renderer.rs`, `src/app/mod.rs`

**Leader key captured by AI panel input**
- Root cause: `Key::Character("b")` from Ctrl+B was intercepted by the panel handler
  before reaching the leader check. The `!cmd` guard only filters macOS Command key.
- Fix: Leader activation (Ctrl+B) and dispatch moved BEFORE the panel input handler.
  Ctrl+B now always activates the leader regardless of which overlay is visible.
- File: `src/app/input/mod.rs`

### Phase 1 Polish Backlog (non-blocking)

| Item | Gap | File |
|------|-----|------|
| Title bar window dragging | `setMovableByWindowBackground:NO` | `app/mod.rs:143` |
| Scroll bar render | No GPU draw code | `config/schema.rs:11` |
| Double/triple-click selection | `SelectionType::Word/Line` not wired | `app/mod.rs:290` |
| OSC 52 clipboard read | `ClipboardLoad` not wired in `mux.rs` | `app/mux.rs:107` |

## Build Status
- **cargo check:** PASS (verified 2026-04-04)
- **branch:** master (stable)

## Key Technical Decisions (standing)

### Leader Key Architecture
- `leader_map: HashMap<String, Action>` built once at startup from `config.keys`
- Leader activation checked BEFORE all overlay handlers (panel, palette) — always intercepts Ctrl+B
- Adding a new keybind = 1 line in `keybinds.lua`, no Rust recompile
- System keybinds hardcoded: `Cmd+C/V` (clipboard), `Cmd+Q` (quit), `Cmd+1-9` (tabs)

### AI Panel Architecture
- Panel instances appended after terminal instances — full GPU upload required when panel visible
- `ChatPanel.dirty` flag gates reshape: only calls `build_chat_panel_instances` when content changed
- `RenderContext.panel_instances_cache` holds last-built panel `Vec<CellVertex>` for reuse
- `resize_terminals_for_panel()` called whenever panel visibility changes; also marks panel dirty
- Cursor blink marks panel dirty only when panel is focused (input cursor needs update)
- Keybind: `<leader>a` (open → focus → close cycle). Esc closes when focused.
- Shell context (CWD, exit code, last command) injected as system message per query

### Render Loop Architecture
- `GpuRenderer` owns: `CellPipeline`, `GlyphAtlas`, uniform_buffer, instance_buffer (32768 max)
- Per-frame: `collect_grid_cells()` → `build_instances()` → `upload_instances()` → `render()`
- Terminal rows use `RowCache` (hash-based, skip unchanged rows)
- Panel instances use `panel_instances_cache` (dirty-flag-based, skip reshape when unchanged)
- Cursor appended last with `FLAG_CURSOR = 0x08`
- Blink: 530ms toggle in `about_to_wait`; reset on keypress
