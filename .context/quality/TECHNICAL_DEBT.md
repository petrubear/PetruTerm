# Technical Debt Registry

**Last Updated:** 2026-04-07
**Open Items:** 2
**Critical (P0):** 0 | **P1:** 1 | **P2:** 0 | **P3:** 1

> Resolved items are in [TECHNICAL_DEBT_archive.md](./TECHNICAL_DEBT_archive.md).

## Priority Definitions

| Priority | Definition | SLA |
|----------|------------|-----|
| P0 | Blocking development or causing incidents | Immediate |
| P1 | Significant impact on velocity or correctness | This sprint |
| P2 | Moderate impact, workaround exists | This quarter |
| P3 | Minor, address when convenient | Backlog |

---

## P0 - Critical

- _None_

---

## P1 - High Priority

- **TD-023** (P1): Window drag interferes with text selection. **Design decision: the terminal area NEVER moves the window — mouse is exclusively for text selection.** Window movement is only possible via the native title bar (the top strip where the traffic-light buttons live). Fix: set `setMovableByWindowBackground: NO` permanently. The native title bar strip remains draggable by default via macOS standard window chrome, so no additional drag-region implementation is needed.

---

## P2 - Medium Priority

- _None_

---

## P3 - Low Priority

- **TD-024** (P3): Implement `Leader+h/j/k/l` vim-style pane focus navigation. Multi-pane rendering is now complete; the missing piece is focus movement by direction. The `FocusDir` enum (`Left/Right/Up/Down`) already exists in `src/ui/panes.rs`. Needs: (a) a `PaneManager::focus_dir(dir: FocusDir)` method that picks the nearest pane in the given direction using rect center-point geometry; (b) four keybinds (`h`, `j`, `k`, `l`) wired to a new `Action::FocusPane(FocusDir)` variant; (c) add to `petruterm.action` table in `lua.rs`; (d) default bindings in `config/default/keybinds.lua` with version bump.

---

## Recently Resolved (2026-04-06)

- **TD-022** (P2): `cargo clippy --all-targets --all-features -- -D warnings` was failing with 36 lint violations. Fixed all 36 (never_loop, too_many_arguments, needless_borrow, manual_clamp, unnecessary_cast, ptr_offset_with_cast, manual_flatten, is_some_and, collapsible_if, needless_splitn, manual_range_contains, manual_is_ascii_check, manual_repeat_n, redundant_closure, map_identity, items_after_test_module, unused_variable). `cargo clippy -D warnings` now passes clean.
- **TD-021** (P2): `title_bar_style` is now parsed from Lua config (`config/lua.rs`). `llm.ui.width_cols` is propagated into all new `ChatPanel` instances via `UiManager.panel_width_cols` and kept in sync via `rewire_llm_provider()`.
- **TD-020** (P2): `check_config_reload()` now calls `rewire_llm_provider()` for hot-reload; `ReloadConfig` palette action also calls it. Both paths rebuild the LLM provider and panel width from the fresh config.
- **TD-019** (P1): `submit_ai_query()` captures `panel_id` before the async spawn; all AI events tagged with `panel_id`; `poll_ai_events()` routes each `(panel_id, event)` to the correct `ChatPanel` entry — tab-switching during streaming no longer corrupts history.
- **TD-018** (P1): `cmd_split()` creates `Terminal::new()` first; pane tree is only mutated on success.
- **TD-017** (P1): `cmd_close_tab()` iterates `panes[active].root.leaf_ids()` and sets every owned terminal slot to `None` before removing the pane entry.
- **TD-OP-02** (P1): `is_pua()` redundant subranges removed; BMP PUA block covers all Nerd Font icons.
- **TD-OP-03** (P2): GlyphAtlas upgraded to 4096×4096 with epoch-based LRU eviction.
- **TD-OP-01** (P2): `unsafe impl Sync` removed from TextShaper; `Send` kept with SAFETY comment.
- **TD-016** (P3): `last_assistant_command()` filters tool-status lines (`⟳`/`✓`) before returning command.

> **TD-015** (resolved 2026-04-05): Shift+Enter → `\x1b[13;2u`, Shift+Tab → `\x1b[Z`.
> **TD-013** (resolved 2026-04-05): Rounded tab pills via `RoundedRectPipeline` + SDF WGSL shader.
> **TD-014** (resolved 2026-04-05): Tab bar background inherits `config.colors.background`.
