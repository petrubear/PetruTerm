# Session State

**Last Updated:** 2026-04-04
**Session Focus:** Phase 3 P1 complete — tab bar polish + keybind alignment

## Branch: `master`

## Session Notes (2026-04-04)

### Phase 3 P1 — Tab bar + Scroll bar (commits 000f8fb, bcb2d19, 7905178, 1e5b102, 54e8b83)

**Terminal ID ownership refactor**
- `PaneManager` no longer owns `next_terminal_id`; `Mux` allocates IDs and passes them into `PaneManager::new` and `split(dir, new_id)`.
- Fixes `cmd_split` using a stale ID before `Terminal::new` succeeded.
- Files: `src/app/mux.rs`, `src/ui/panes.rs`

**Tab bar redesign**
- Each tab renders as `[gap BAR_BG][" N " BADGE_BG][" title " TAB_BG]`.
- Active: Dracula purple `#bd93f9` body + darker purple badge.
- Inactive: Dracula current-line gray body + darker gray badge.
- Powerline glyphs (E0B4/E0B6) were attempted for rounded caps but produce solid triangle arrows — removed. Rounded pills deferred to TD-013 (GPU render pass).
- Tab bar background hardcoded; should match terminal bg — deferred to TD-014.
- File: `src/app/renderer.rs` → `build_tab_bar_instances`

**Keybinds — tmux alignment**
- `leader+c` new tab (was `t`), `leader+&` close tab (was `w`), `leader+p` prev tab (was `b`), `leader+n` next tab (unchanged).
- Updated both `config/default/keybinds.lua` (embedded) and `~/.config/petruterm/keybinds.lua` (user).

**Technical Debt added**
- TD-013: Tab bar rounded pills — requires wgpu rounded-rect render pass.
- TD-014: Tab bar BAR_BG should inherit `config.colors.background`.

### Previous session (2026-04-04) — Phase 2 + Phase 3 P1 base

See archived notes: per-pane chat history, Ctrl+Space inline AI block, AI block rendering, performance fixes, leader key ordering.

## Build Status
- **cargo check:** PASS (0 errors — 2026-04-04)
- **branch:** master (stable)

## Key Technical Decisions (standing)

### Tab Bar Architecture
- Rendered at `grid_pos[1] = -1.0` (one row above terminal) via `build_tab_bar_instances`
- Visible only when tab count > 1 (`tab_bar_visible()`)
- Height = 1 cell height; padding shifted via `renderer.set_padding()`
- Rounded corners deferred (TD-013); bg color transparency deferred (TD-014)

### Keybind Architecture (tmux-style)
- Leader: `Ctrl+B`, 1000ms timeout
- Tab: `c` new, `&` close, `n` next, `p` prev, `1-9` switch
- Pane: `%` split-H, `"` split-V, `x` close, `hjkl` focus nav
- AI: `a` panel, `e` explain, `f` fix, `Ctrl+Space` inline block
- Command palette: `p`

### Per-Pane Chat Architecture
- `UiManager.chat_panels: HashMap<usize, ChatPanel>` keyed by `terminal_id`
- `set_active_terminal(id)` called in `RedrawRequested` and `KeyboardInput`

### AI Panel Performance
- `ChatPanel.dirty` flag + `RenderContext.panel_instances_cache` gates HarfBuzz reshape

## Phase 1 Polish Backlog (non-blocking)

| Item | Gap | File |
|------|-----|------|
| Title bar window dragging | `setMovableByWindowBackground:NO` | `app/mod.rs:143` |
| Scroll bar render | Config field exists, no GPU draw code | `config/schema.rs:11` |
| Double/triple-click selection | `SelectionType::Word/Line` not wired | `app/mod.rs:290` |
| OSC 52 clipboard read | `ClipboardLoad` not wired | `app/mux.rs:107` |
