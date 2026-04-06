# Active Context

**Current Focus:** Phase 2.5 — AI Agent Mode
**Last Active:** 2026-04-05
**Priority:** P1 (File context attachment + panel upgrade)

## Current State

**Phase 1 COMPLETE. Phase 2 COMPLETE. Phase 3 P1 COMPLETE. All TD items resolved. (2026-04-05)**

### Phase 3 P1 Verified ✓ (2026-04-05)

| Feature | Status | Notes |
|---------|--------|-------|
| Tab bar | ✅ | Rounded pill tabs via `RoundedRectPipeline` + SDF WGSL shader |
| Scroll bar | ✅ | 6px right-edge overlay, proportional thumb |
| Tab bar rounded pills | ✅ | TD-013 resolved — `src/renderer/rounded_rect.rs` |
| Tab bar bg transparency | ✅ | TD-014 resolved — inherits `config.colors.background` (clear color) |
| Title bar drag | ✅ | `setMovableByWindowBackground:YES` |
| Double/triple-click selection | ✅ | `Semantic`/`Lines` via `InputHandler::register_click()` |
| Tab bar mouse click | ✅ | `hit_test_tab_bar()` in `app/mod.rs` |
| Shell exit closes tab | ✅ | `close_terminal()` in `app/mux.rs` |
| Font fallback chain | ✅ | `petruterm.font("A, B, C")` resolved at config load time |

### Technical Debt
3 open items: TD-OP-02 (P1 Nerd Font override fragility), TD-OP-03 (P2 atlas eviction), TD-OP-01 (P2 unsafe Send on TextShaper).

### Keybinds (tmux-aligned)

| Key | Action |
|-----|--------|
| `leader+c` | New tab |
| `leader+&` | Close tab |
| `leader+n` | Next tab |
| `leader+p` | Prev tab |
| `leader+%` | Split horizontal |
| `leader+"` | Split vertical |
| `leader+x` | Close pane |
| `leader+a` | AI panel |
| `leader+p` | Command palette |
| `Ctrl+Space` | Inline AI block |

## Phase 2.5 P1 — COMPLETE (2026-04-05)

All P1 deliverables shipped:
- `ChatPanel.attached_files` + `AGENTS.md` auto-load ✅
- File picker overlay (`Tab`) with fuzzy search ✅
- File contents injected into LLM system message ✅
- Token counter in footer ✅
- `Ctrl+S` submit ✅
- CWD from real terminal process (`proc_pidinfo` on macOS) ✅
- `/q`/`/quit` closes panel + tab ✅

## Phase 2.5 Next Steps

### P2 — Tool Use (read & explore)
1. **`AgentTool` enum** — `ReadFile`, `ListDir` in OpenAI function-calling format
2. **Provider extension** — serialize tool defs, parse `tool_calls` in response
3. **Tool execution loop** — call → inject result → re-query until done
4. **Streaming UI** — `⟳ reading…` / `✓ done` inline

### P3 — Tool Use (write & run)
5. **`WriteFile` / `ApplyDiff`** — diff preview inline, `[y]/[n]` confirm before disk write
6. **`RunCommand`** — execute in PTY after confirm
7. **Undo** — single-step file restore

## Files to Reference
- `src/llm/chat_panel.rs` — `ChatPanel`, `attached_files`, `file_picker_*`, `scan_files()`
- `src/app/ui.rs` — `open_panel_with_context(id, cwd)`, `submit_ai_query` (file injection)
- `src/app/input/mod.rs` — Tab picker, `/q`/`/quit`, `Ctrl+S`, `Shift+Enter`
- `src/app/renderer.rs` — `build_chat_panel_instances` (file section + picker overlay)
- `src/term/mod.rs` — `Terminal.child_pid`, `process_cwd(pid)`
- `src/app/mux.rs` — `Mux::active_cwd()`
- `src/renderer/rounded_rect.rs` — `RoundedRectInstance`, `RoundedRectPipeline`, SDF shader
