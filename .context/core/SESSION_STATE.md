# Session State

**Last Updated:** 2026-04-04
**Session Focus:** Context audit — empate de tareas completadas vs build_phases.md

## Branch: `master`

## Session Notes (2026-04-04)

Full codebase audit performed. `build_phases.md` updated to reflect actual implementation state.
`ACTIVE_CONTEXT.md` updated with prioritized Phase 2 backlog.

### Audit Findings

**Phase 1:** All MVP exit criteria met. 3 non-blocking polish items remain (title bar drag, scroll bar render, double/triple-click selection). No TD items open.

**Phase 2:** ~60% complete. Core LLM infrastructure solid. Remaining work is UX-focused.

### Phase 2 Remaining (prioritized)

| # | Item | Effort | Files |
|---|------|--------|-------|
| 1 | Wire `<leader>e` and `<leader>f` keybinds | XS | `keybinds.lua`, `actions.rs` |
| 2 | Shell integration zsh script | S | `scripts/shell-integration.zsh` |
| 3 | `[⏎ Run]` button for AI command suggestions | M | `app/renderer.rs`, `app/ui.rs` |
| 4 | Per-pane chat history | M | `llm/chat_panel.rs`, `ui/mux.rs` |
| 5 | `Ctrl+Space` AI mode keybind | XS | `app/input/mod.rs` |
| 6 | Inline AI block rendering | L | `llm/ai_block.rs`, `app/renderer.rs` |

### Phase 1 Polish Backlog (non-blocking)

| Item | Gap | File |
|------|-----|------|
| Title bar window dragging | `setMovableByWindowBackground:NO` | `app/mod.rs:143` |
| Scroll bar render | No GPU draw code | `config/schema.rs:11` |
| Double/triple-click selection | `SelectionType::Word/Line` not wired | `app/mod.rs:290` |
| OSC 52 clipboard read | `ClipboardLoad` not wired in `mux.rs` | `app/mux.rs:107` |

## Build Status
- **cargo check:** PASS (last verified 2026-04-03)
- **branch:** master (stable)

## Key Technical Decisions (standing)

### Leader Key Architecture
- `leader_map: HashMap<String, Action>` built once at startup from `config.keys`
- Adding a new keybind = 1 line in `keybinds.lua`, no Rust recompile
- System keybinds hardcoded: `Cmd+C/V` (clipboard), `Cmd+Q` (quit), `Cmd+1-9` (tabs)

### AI Panel Architecture
- Panel instances appended after terminal instances — full GPU upload required when panel visible
- `resize_terminals_for_panel()` called whenever panel visibility changes
- Keybind: `<leader>a` (open → focus → close cycle). Esc closes when focused.
- Shell context (CWD, exit code, last command) injected as system message per query

### Render Loop Architecture
- `GpuRenderer` owns: `CellPipeline`, `GlyphAtlas`, uniform_buffer, instance_buffer (32768 max)
- Per-frame: `collect_grid_cells()` → `build_instances()` → `upload_instances()` → `render()`
- Cursor appended last with `FLAG_CURSOR = 0x08`
- Blink: 530ms toggle in `about_to_wait`; reset on keypress
