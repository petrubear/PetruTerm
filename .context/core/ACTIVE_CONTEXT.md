# Active Context

**Current Focus:** Phase 2 — AI Layer completion
**Last Active:** 2026-04-04
**Priority:** P1

## Current State

**Phase 1 COMPLETE (MVP criteria met).** Phase 2 ~60% complete. Phase 3 not started.

### Phase 1 Verified ✓ (2026-04-04 audit)
All MVP exit criteria met: window, PTY, 60fps render, ligatures, nvim/tmux/claude verified.
Leader key system, command palette, tabs, panes, hot-reload all functional.

**3 polish items remain (non-blocking for MVP):**
| Item | Gap | File |
|------|-----|------|
| Title bar drag | `setMovableByWindowBackground:NO` | `app/mod.rs:143` |
| Scroll bar render | Config field exists, no GPU draw code | `config/schema.rs:11` |
| Double/triple-click selection | `SelectionType::Word/Line` not wired | `app/mod.rs:290` |

**Minor gap:** OSC 52 clipboard read path not fully wired (`mux.rs:107`).

### Phase 2 Status (2026-04-04 audit)

**Done:** `LlmProvider` trait, OpenRouter + Ollama + LMStudio providers, `llm.lua` config, `config.llm.enabled` toggle, command palette AI toggle, shell context injection (CWD/exit code/last command), streaming response to chat panel.

**Remaining — prioritized:**

| Priority | Item | What's missing |
|----------|------|----------------|
| ✅ P1 DONE | `<leader>e` / `<leader>f` keybinds | Already wired: `keybinds.lua`, `actions.rs`, `handle_palette_action` |
| ✅ P1 DONE | Shell integration script | `scripts/shell-integration.zsh` complete: `preexec`/`precmd` hooks → JSON |
| ✅ P1 DONE | `[⏎ Run]` button (Feature 1) | Run bar in `app/renderer.rs`: green `│ ⏎ cmd` line after AI response; Enter executes via PTY |
| P2 | Per-pane chat history | Currently global; move `ChatPanel` into `Pane` struct |
| P2 | `Ctrl+Space` AI mode toggle | Add to system keybinds in `input/mod.rs` |
| P3 | Inline AI block (`llm/ai_block.rs`) | Dead code; not rendered — lower priority than chat panel UX |

## Next Steps (ordered)

1. **Per-pane history** — move `ChatPanel` state into `Pane` struct in `ui/panes.rs`
2. **`Ctrl+Space` AI mode toggle** — add to system keybinds in `app/input/mod.rs`
3. **Inline AI block rendering** — wire `llm/ai_block.rs` to the renderer

## Files to Reference
- `src/app/input/mod.rs` — Leader key dispatch; system keybinds
- `src/ui/palette/actions.rs` — `Action` enum + `FromStr`
- `config/default/keybinds.lua` — single source of truth for custom keybinds
- `src/app/ui.rs` — `handle_palette_action`, `explain_last_output`, `fix_last_error`, `submit_ai_query`
- `src/llm/chat_panel.rs` — `last_assistant_command()`, chat history
- `src/llm/shell_context.rs` — shell context tracking
- `scripts/shell-integration.zsh` — zsh hooks (needs expansion)
- `src/app/renderer.rs` — `build_chat_panel_instances` (add Run button here)
