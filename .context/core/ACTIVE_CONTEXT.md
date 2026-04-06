# Active Context

**Current Focus:** Phase 2.5 — AI Agent Mode (P3 next) + polish
**Last Active:** 2026-04-06
**Priority:** P3 (Write & Run tools)

## Current State

**Phase 1 COMPLETE. Phase 2 COMPLETE. Phase 3 P1 COMPLETE. All TD items resolved. (2026-04-06)**

### Phase 3 P1 Verified ✓ (2026-04-06)

| Feature | Status | Notes |
|---------|--------|-------|
| Tab bar | ✅ | Rounded pill tabs via `RoundedRectPipeline` + SDF WGSL shader |
| Scroll bar | ✅ | 6px right-edge overlay, proportional thumb |
| Tab bar rounded pills | ✅ | TD-013 resolved — `src/renderer/rounded_rect.rs` |
| Tab bar bg transparency | ✅ | TD-014 resolved — inherits `config.colors.background` (clear color) |
| Title bar drag | ✅ | `drag_window()` at y < padding.top; `setMovableByWindowBackground: NO` |
| Mouse text selection | ✅ | Fixed (was broken by `setMovableByWindowBackground: YES`) |
| Double/triple-click selection | ✅ | `Semantic`/`Lines` via `InputHandler::register_click()` |
| Tab bar mouse click | ✅ | `hit_test_tab_bar()` in `app/mod.rs` |
| Shell exit closes tab | ✅ | `close_terminal()` in `app/mux.rs` |
| Font fallback chain | ✅ | `petruterm.font("A, B, C")` resolved at config load time |
| Right-click context menu | ✅ | Copy/Paste/Clear with keybind hints — `src/ui/context_menu.rs` |
| Palette keybind hints | ✅ | Right-aligned `^B c` / `Cmd+Q` labels in command palette |
| Default config — all fields | ✅ | All schema fields documented in shipped config files |
| Missing configs auto-created | ✅ | `ensure_default_configs()` writes missing files on every startup |

### Technical Debt
4 open items: TD-OP-02 (P1), TD-OP-03 (P2), TD-OP-01 (P2), TD-016 (P3 run bar shows tool status lines).

### Keybinds (tmux-aligned)

| Key | Action |
|-----|--------|
| `leader+c` | New tab |
| `leader+&` | Close tab |
| `leader+n` | Next tab |
| `leader+b` | Prev tab |
| `leader+%` | Split horizontal |
| `leader+"` | Split vertical |
| `leader+x` | Close pane |
| `leader+a` | AI panel |
| `leader+o` | Command palette |
| `Ctrl+Space` | Inline AI block |

## Phase 2.5 Status

### P2 — Tool Use (read & explore) — COMPLETE (2026-04-05)
Tool use loop verified working: `list_dir(.)` shows ⟳/✓ status inline, LLM receives real listing and responds.

### P1 — File Context — COMPLETE (2026-04-05)
`ChatPanel.attached_files`, AGENTS.md auto-load, file picker, Ctrl+S submit, /q/quit, CWD from proc_pidinfo.

## Phase 2.5 Next Steps

### P3 — Tool Use (write & run)
1. **`WriteFile` / `ApplyDiff`** — diff preview inline, `[y]/[n]` confirm before disk write
2. **`RunCommand`** — execute in PTY after confirm
3. **Undo** — single-step file restore

## Files to Reference
- `src/ui/context_menu.rs` — `ContextMenu`, `ContextAction`, `CONTEXT_MENU_WIDTH`
- `src/ui/palette/actions.rs` — `PaletteAction` (+ `keybind` field), `built_in_actions(&Config)`
- `src/ui/palette/mod.rs` — `CommandPalette::new(&Config)`, `rebuild_keybinds(&Config)`
- `src/llm/chat_panel.rs` — `ChatPanel`, `attached_files`, file picker, `scan_files()`
- `src/app/ui.rs` — `UiManager` (palette, context_menu, chat panels, ai_block)
- `src/app/mod.rs` — right-click → context menu; left-click → menu hit-test
- `src/app/renderer.rs` — `build_palette_instances`, `build_context_menu_instances`
- `src/config/mod.rs` — `ensure_default_configs()` (idempotent, every startup)
