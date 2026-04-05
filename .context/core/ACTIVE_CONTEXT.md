# Active Context

**Current Focus:** Phase 3 — Ecosystem
**Last Active:** 2026-04-04
**Priority:** P1

## Current State

**Phase 1 COMPLETE. Phase 2 COMPLETE (2026-04-04).** Phase 3 not started.

### Phase 2 Verified ✓ (2026-04-04)
All Phase 2 deliverables met. Commit b815320 completed the final three items.

| Feature | Status | Notes |
|---------|--------|-------|
| LLM providers (OpenRouter / Ollama / LMStudio) | ✅ | |
| `llm.lua` config + `config.llm.enabled` toggle | ✅ | |
| Command palette AI toggle | ✅ | |
| Shell context injection (CWD / exit code / last command) | ✅ | `llm/shell_context.rs` |
| NL→Shell (Feature 1): streaming query + Run bar | ✅ | Enter executes command |
| Explain last output (`<leader>e`) | ✅ | wired in `app/ui.rs` |
| Fix last error (`<leader>f`) | ✅ | wired in `app/ui.rs` |
| Per-pane chat history | ✅ | `HashMap<usize, ChatPanel>` in `UiManager` |
| `Ctrl+Space` inline AI block | ✅ | 4-row overlay, state machine |
| Inline AI block rendering | ✅ | `build_ai_block_instances` in `renderer.rs` |

### Phase 1 Polish Backlog (non-blocking)

| Item | Gap | File |
|------|-----|------|
| Title bar drag | `setMovableByWindowBackground:NO` | `app/mod.rs:143` |
| Scroll bar render | Config field exists, no GPU draw code | `config/schema.rs:11` |
| Double/triple-click selection | `SelectionType::Word/Line` not wired | `app/mod.rs:290` |
| OSC 52 clipboard read | `ClipboardLoad` not wired | `app/mux.rs:107` |

## Phase 3 Next Steps (ordered by priority)

1. **Plugin loader** — auto-scan `~/.config/petruterm/plugins/*.lua`
2. **Plugin Lua API** — `petruterm.palette.register()`, `petruterm.on()`, `petruterm.notify()`
3. **Plugin event system** — `tab_created`, `tab_closed`, `pane_split`, `ai_response`, `command_run`
4. **Status bar engine** — enable/disable from Lua + palette; built-in widgets: `mode`, `cwd`, `git_branch`, `time`, `exit_code`
5. **Snippets** — `config.snippets` Lua table, expand via palette, optional `trigger` field
6. **Starship compatibility** — detect `STARSHIP_SHELL`, defer left prompt

## Files to Reference
- `src/plugins/` — plugin loader + Lua API (scaffolded)
- `src/snippets/` — snippet manager (scaffolded)
- `src/app/ui.rs` — `handle_palette_action`, AI feature handlers
- `src/app/input/mod.rs` — system keybinds, leader dispatch
- `src/llm/ai_block.rs` — inline AI block state machine + rendering data
- `src/llm/chat_panel.rs` — chat history, `last_assistant_command()`
- `config/default/keybinds.lua` — single source of truth for custom keybinds
