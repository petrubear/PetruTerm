# Session State

**Last Updated:** 2026-04-04
**Session Focus:** Phase 2 complete — context sync

## Branch: `master`

## Session Notes (2026-04-04)

### Phase 2 completion (commit b815320)

**Per-pane chat history**
- Replaced `chat_panel: ChatPanel` in `UiManager` with `chat_panels: HashMap<usize, ChatPanel>` keyed by `terminal_id`.
- `panel()` / `panel_mut()` accessors; `set_active_terminal(id)` called in `RedrawRequested` and `KeyboardInput`.
- Switching tabs/panes automatically loads the correct conversation.
- Files: `src/app/ui.rs`, `src/app/mod.rs`

**Ctrl+Space — inline AI block**
- Toggles a new 4-row horizontal bar overlay at the bottom of the terminal.
- State machine: Typing → Loading → Streaming → Done / Error.
- Enter submits; Enter again (Done) executes command in PTY; Esc closes.
- Separate `block_tx/rx` channel in `UiManager`; streams via same tokio runtime as chat panel.
- `AiBlock.dirty` flag: renderer skips reshaping when unchanged.
- Files: `src/app/input/mod.rs`, `src/app/ui.rs`, `src/llm/ai_block.rs`

**Inline AI block rendering (`build_ai_block_instances`)**
- Added to `RenderContext` in `src/app/renderer.rs`.
- Renders separator + query input + response + hint at the bottom rows.
- `→` prefix for response, spinner for streaming, green for command.

### Previous session (commit d2502cb)

**Performance — AI panel reshaping every frame**
- `ChatPanel.dirty` flag + `RenderContext.panel_instances_cache` stops HarfBuzz reshaping on every frame.

**Leader key captured by AI panel input**
- Leader activation (Ctrl+B) moved BEFORE panel input handler.

## Build Status
- **cargo check:** PASS (0 errors, warnings only — 2026-04-04)
- **branch:** master (stable)

## Key Technical Decisions (standing)

### Per-Pane Chat Architecture
- `UiManager.chat_panels: HashMap<usize, ChatPanel>` — keyed by `terminal_id`
- `set_active_terminal(id)` syncs active panel on tab/pane switch
- Each pane holds independent chat history; panel visibility is global (one panel shown at a time)

### Inline AI Block Architecture
- 4-row overlay — does NOT resize the PTY (overlays last terminal rows)
- State machine: `Typing` → `Loading` → `Streaming` → `Done` / `Error`
- System prompt: "generate ONLY the shell command" (no explanation)
- `AiBlock.dirty` flag prevents unnecessary reshaping
- Channel: `block_tx/rx` in `UiManager`, same tokio runtime as chat panel
- Keybind: `Ctrl+Space` (hardcoded system keybind)

### Leader Key Architecture
- Leader activation (Ctrl+B) checked BEFORE all overlay handlers
- Adding new keybind = 1 line in `keybinds.lua`

### AI Panel Architecture
- Panel instances appended after terminal instances — full GPU upload required when panel visible
- `ChatPanel.dirty` flag gates reshape
- `RenderContext.panel_instances_cache` holds last-built panel `Vec<CellVertex>`
- Keybind: `<leader>a` (open → focus → close). Esc closes when focused.

### Render Loop Architecture
- Per-frame: `collect_grid_cells()` → `build_instances()` → `upload_instances()` → `render()`
- Terminal rows: `RowCache` (hash-based)
- Panel instances: `panel_instances_cache` (dirty-flag-based)
- AI block instances: `AiBlock.dirty` flag
- Cursor appended last with `FLAG_CURSOR = 0x08`

## Phase 1 Polish Backlog (non-blocking)

| Item | Gap | File |
|------|-----|------|
| Title bar window dragging | `setMovableByWindowBackground:NO` | `app/mod.rs:143` |
| Scroll bar render | No GPU draw code | `config/schema.rs:11` |
| Double/triple-click selection | `SelectionType::Word/Line` not wired | `app/mod.rs:290` |
| OSC 52 clipboard read | `ClipboardLoad` not wired in `mux.rs` | `app/mux.rs:107` |
