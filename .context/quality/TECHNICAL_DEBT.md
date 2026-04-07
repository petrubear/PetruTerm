# Technical Debt Registry

**Last Updated:** 2026-04-07
**Open Items:** 2
**Critical (P0):** 0 | **P1:** 0 | **P2:** 1 | **P3:** 1

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

- _None_

---

## P2 - Medium Priority

- **TD-026** (P2): Status bar (Phase 3 P2) — segmented right-aligned bar rendered by GPU. Segments (left→right): running command, current directory, leader-mode indicator (changes color when leader active), date/time. Each segment sourced from a Lua plugin. Reference screenshot: `~/Documents/ScreenShots/Screenshot 2026-04-07 at 10.36.37.png`.

---

## P3 - Low Priority

- **TD-027** (P3): Tab rename via `<leader>,` — prompt user for a new label and replace the default `# zsh` title displayed in the tab pill. Mirrors tmux `prefix + ,` behavior.

---

## Recently Resolved (2026-04-07)

- **TD-025** (P0): Mouse tab-bar click called `switch_to_index()` without `resize_terminals_for_panel()`, so the newly-active tab's PTY kept the pre-tab-bar row count and content overflowed below the visible area. Fix: added `resize_terminals_for_panel()` after `switch_to_index()` in the `MouseButton::Left` tab-bar hit handler (`app/mod.rs`). Keyboard tab switching already triggered the resize via the `tab_idx != tab_idx_before` guard.
- **TD-028** (P1): `MouseScrollDelta::PixelDelta.y` is in logical points on macOS but was divided by `cell_h` in physical pixels — giving ~0.5 lines/event on 2× Retina → very slow scroll. Fix: divide by `cell_h / scale_factor` (logical cell height). Auto-scroll to bottom on keypress: `send_key_to_active_terminal` now calls `terminal.scroll_to_bottom()` (`Scroll::Bottom`) before `write_input` so any keystroke while scrolled up jumps back to the prompt.

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
