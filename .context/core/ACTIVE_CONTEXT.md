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
| ID | Priority | Description |
|----|----------|-------------|
| TD-015 | P1 | Shift+Enter treated as regular Enter (input/PTY)

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

## Phase 2.5 Next Steps (ordered by priority)

### TD-015 (fix first — P1)
1. **Shift+Enter fix** — detect `Enter + SHIFT` in `InputHandler`, send `\n` not `\r`, no PTY exec

### P1 — File Context Attachment
2. **`ChatPanel.attached_files`** — `Vec<PathBuf>`, auto-populate with `AGENTS.md` from CWD on open
3. **File list section** — render `Selected (N files)` header + filenames at top of panel
4. **File picker** — `Tab` toggles focus; fuzzy search CWD files; `Enter` attach/detach
5. **Context injection** — file contents as `role: system` messages before user query
6. **Token counter** — footer: `Tokens: NNNN; <C-s>: submit`

### P2 — Tool Use (read & explore)
7. **`AgentTool` enum** — `ReadFile`, `ListDir` in OpenAI function-calling format
8. **Provider extension** — serialize tool defs, parse `tool_calls` in response
9. **Tool execution loop** — call → inject result → re-query until done
10. **Streaming UI** — `⟳ reading…` / `✓ done` inline

### P3 — Tool Use (write & run)
11. **`WriteFile` / `ApplyDiff`** — diff preview inline, `[y]/[n]` confirm before disk write
12. **`RunCommand`** — execute in PTY after confirm
13. **Undo** — single-step file restore

## Files to Reference
- `src/renderer/rounded_rect.rs` — `RoundedRectInstance`, `RoundedRectPipeline`, SDF shader
- `src/app/renderer.rs` — `build_tab_bar_instances`, `build_scroll_bar_instances`
- `src/app/mod.rs` — `tab_bar_visible()`, `tab_bar_height_px()`, `hit_test_tab_bar()`
- `src/app/ui.rs` — `handle_palette_action`, AI feature handlers
- `src/app/input/mod.rs` — leader dispatch, `register_click()` for multi-click selection
- `config/default/keybinds.lua` — embedded keybind defaults
