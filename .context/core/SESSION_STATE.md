# Session State

**Last Updated:** 2026-04-07
**Session Focus:** Bug fixes — tab bar viewport, scroll speed, auto-scroll on keypress

## Branch: `master`

## Session Notes (2026-04-07 — bug fixes)

### TD-025: Tab bar viewport overflow (RESUELTO)
- Root cause: mouse click on tab bar called `switch_to_index()` without `resize_terminals_for_panel()` — PTY kept old row count, last rows rendered off-screen.
- Fix: `app/mod.rs` — added `self.resize_terminals_for_panel()` after `switch_to_index(idx)` in the tab-bar mouse click handler.

### TD-028: Scroll speed + auto-scroll on keypress (RESUELTO)
- Scroll speed: `PixelDelta.y` is logical points; was divided by physical `cell_h` → ~0.5 lines/event on Retina. Fix: divide by `cell_h / scale_factor`.
- Auto-scroll: `send_key_to_active_terminal` now calls `terminal.scroll_to_bottom()` before `write_input`.
- Files: `app/mod.rs`, `app/input/mod.rs`, `term/mod.rs`.

### Deuda técnica registrada (nuevas entradas)
- TD-026 (P2): Status bar segmentado con plugins Lua
- TD-027 (P3): Tab rename via `<leader>,`

## Session Notes (2026-04-07 — Phase 2.5 P3)

### Phase 2.5 P3 COMPLETA

#### Archivos nuevos
- `src/llm/diff.rs` — LCS line diff + `compress_diff(ctx=2)`

#### Cambios clave
- `src/llm/tools.rs`: `AgentTool::WriteFile`, `RunCommand` + specs OpenAI + `content_arg()`, `cmd_arg()`, `requires_confirmation()`
- `src/llm/chat_panel.rs`:
  - `AiEvent::ConfirmWrite { path, new_content, result_tx }` / `ConfirmRun { cmd, result_tx }` / `UndoState { path, content }`
  - `PanelState::AwaitingConfirm`
  - `ConfirmDisplay::Write { path, diff, added, removed }` / `Run { cmd }` + `for_write()`
  - `ChatPanel.confirm_display: Option<ConfirmDisplay>`
  - `mark_awaiting_confirm()` / `resolve_confirm()`
- `src/app/ui.rs`:
  - `UiManager`: `pending_confirm_tx`, `undo_stack`, `pending_pty_run`
  - `confirm_yes()` / `confirm_no()` / `cmd_undo_last_write()`
  - Agent loop: `requires_confirmation()` branch → oneshot → await → write/run
- `src/app/mod.rs`: `flush_pending_pty_run()` — envía cmd confirmado al PTY activo
- `src/app/input/mod.rs`: y/Enter → `confirm_yes`, n/Esc → `confirm_no` en `AwaitingConfirm`
- `src/app/renderer.rs`: confirmation view (diff lines +/-, prompt [y]/[n] rows, hints)
- `src/ui/palette/actions.rs`: `Action::UndoLastWrite`
- `config/default/keybinds.lua`: `<leader>z` → `UndoLastWrite`

## Build & Tests
- **cargo build:** PASS (0 errors — 2026-04-07)
- **cargo test:** 16/16 PASS
- **cargo clippy --all-targets --all-features -- -D warnings:** PASS
- **branch:** master

## Session anterior (2026-04-07 — pane focus + separator padding)

### TD-024 (P3) — Leader+h/j/k/l (RESUELTO)
- `PaneManager::focus_dir(dir)` — centr-point geometry, nearest pane in direction.
- `Action::FocusPane(FocusDir)` + `Mux::cmd_focus_pane_dir()`.
- Keybinds `^B h/j/k/l` en `keybinds.lua` (config version 3).

### Separator padding fix
- `PanePad` struct + `collect_leaf_infos_impl` — 1 celda de respiro en cada lado del separador.
