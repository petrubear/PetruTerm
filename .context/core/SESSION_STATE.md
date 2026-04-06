# Session State

**Last Updated:** 2026-04-05
**Session Focus:** Phase 2.5 P2 — LLM Tool Use (ReadFile, ListDir)

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

## Session Notes (2026-04-05)

### TD-015 — Shift+Enter (resolved)
- `key_map.rs`: `Shift+Enter` → `\x1b[13;2u` (xterm modified); `Shift+Tab` → `\x1b[Z`
- `input/mod.rs`: chat panel `Shift+Enter` inserts `\n` without submitting

### Phase 2.5 P1 — AI Agent Mode: File Context Attachment (complete)
- `ChatPanel.attached_files: Vec<PathBuf>` + `attached_file_chars` for token estimation
- `AGENTS.md` auto-loaded from terminal's real CWD on every panel open (idempotent)
- `Tab` opens file picker overlay; fuzzy search via `fuzzy-matcher`; `Enter` attaches/detaches; `Tab`/`Esc` closes
- File list section rendered at top of panel: `Selected (N files)` header + names
- Attached file contents injected as `--- File: path ---\ncontent` in system message
- Token counter in panel footer (`estimated chars/4`)
- `Ctrl+S` as alternative submit keybind

### CWD resolution (correct terminal directory)
- `Pty::spawn` captures `pty.child().id()` before EventLoop consumes the pty
- `Terminal.child_pid: u32` and `Mux::active_cwd()` expose it
- macOS: `proc_pidinfo(PROC_PIDVNODEPATHINFO)` via `libc` — no shell integration needed
- Linux: `/proc/{pid}/cwd` symlink
- Files: `src/term/pty.rs`, `src/term/mod.rs`, `src/app/mux.rs`

### /q and /quit commands
- Typing `/q` or `/quit` in panel input + Enter → closes panel + `mux.cmd_close_tab()`

## Session Notes (2026-04-05 — Phase 2.5 P2: Tool Use)

### Tool use loop
- `src/llm/tools.rs` — `AgentTool` (ReadFile, ListDir), `execute_tool()` with CWD sandbox
- `src/llm/mod.rs` — `ChatRole::Tool(String)`, `ChatMessage::to_api_value()`, `agent_step()` in trait
- `src/llm/openrouter.rs` + `openai_compat.rs` — `agent_step()` impl; both parse `tool_calls`
- `src/llm/chat_panel.rs` — `AiEvent::ToolStatus { tool, path, done }`; `set_tool_status()`
- `src/app/ui.rs` — `submit_ai_query(wakeup_proxy, cwd)` now runs tool loop (max 10 rounds)
- `src/app/input/mod.rs` — pass CWD from `mux.active_cwd()` to submit

## Session Notes (2026-04-05 — Emoji + TD audit)

### Emoji rendering fix
- Root cause: `fs_main` shader read only `.r` channel — broken for color (RGBA) emoji glyphs
- Fix: `FLAG_COLOR_GLYPH = 0x20` flag; `AtlasEntry.is_color: bool`; shader branches on flag
- Files: `src/renderer/cell.rs`, `src/renderer/atlas.rs`, `src/font/shaper.rs`, `src/app/renderer.rs`, `src/renderer/pipeline.rs`
- Verified working ✅

### TD audit (opencode items)
- TD-OP-01 (P0 use-after-free) → **false positive** — Drop is correct, reclassified P2
- TD-OP-02 (Nerd Font overrides) → **real** — confirmed in `shaper.rs`, kept P1
- TD-OP-03 (atlas no eviction) → **real** — confirmed in `atlas.rs`, kept P2

## Build Status
- **cargo check:** PASS (0 errors — 2026-04-05)
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

## Phase 1 Polish — COMPLETE (2026-04-05)

All polish items resolved:
- Title bar drag: `setMovableByWindowBackground:YES` (commit e9b5af9)
- Double/triple-click: `Semantic`/`Lines` via `InputHandler::register_click()` (commit e9b5af9)
- OSC 52 read: was already wired (context note was stale)
- Scroll bar: was already wired in Phase 3 P1 (context note was stale)
