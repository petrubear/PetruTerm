# Technical Debt Registry

**Last Updated:** 2026-04-09
**Open Items:** 1
**Critical (P0):** 0 | **P1:** 1 | **P2:** 0 | **P3:** 0

> Resolved items are in [TECHNICAL_DEBT_archive.md](./TECHNICAL_DEBT_archive.md).

## Priority Definitions

| Priority | Definition | SLA |
|----------|------------|-----|
| P0 | Blocking development or causing incidents | Immediate |
| P1 | Significant impact on velocity or correctness | This sprint |
| P2 | Moderate impact, workaround exists | This quarter |
| P3 | Minor, address when convenient | Backlog |

---

## P1 - High Priority

- **TD-042** (P1): **Pane resize no implementado** (`src/ui/panes.rs`, `src/app/input/mod.rs`, `src/app/mod.rs`). Los panes creados con `%` / `"` no se pueden redimensionar. `PaneNode::Split` ya tiene `ratio: f32` — solo falta exponerlo. Dos sub-tareas:
  - **Teclado:** `PaneManager::adjust_ratio(focused_id, dir, delta)` recorre el árbol buscando el Split padre cuya `SplitDir` coincide con la flecha y ajusta `ratio ±0.05`. Keybind: `<leader>+Option+←→↑↓` en `input/mod.rs`. Tras ajustar llamar `mux.resize_all()`.
  - **Mouse drag:** extender `PaneSeparator` con `node_id: usize` para poder localizar el Split dueño. Agregar `dragging_separator: Option<SeparatorDragState>` en `InputHandler`. En `MouseMoved` detectar proximidad ±3px al separador, iniciar drag, calcular delta en celdas y actualizar ratio en vivo. Finalizar en `MouseButtonLeft::Released`.

---

## Recently Resolved (2026-04-09)

- **TD-041** (P0): Input del chat panel duplicado — `n.saturating_sub(2)` y `n.saturating_sub(1)` eran ambos `0` cuando `n==1`. Fix: `vis1 = if n >= 2 { input_lines[n-2] } else { String::new() }`.
- **TD-030** (P0): Archivos adjuntos al LLM capeados a 512 KB/archivo y 1 MB total antes de inyectarlos en el system message. Nota de truncado añadida al contexto.
- **TD-029** (P1): `cwd.canonicalize()` llamado una vez en `submit_ai_query` antes del spawn — fix para macOS donde `/var` es symlink a `/private/var`, rompiendo `starts_with` en `execute_tool`.
- **TD-031** (P1): `EXPORT_REGEX` y `AUTH_REGEX` en `shell_context.rs` ahora son `LazyLock` estáticos — compilados una vez por proceso.
- **TD-033** (P1): Fallback stream tras agotar tool rounds ahora filtra mensajes `role:"tool"` y mensajes assistant con content vacío (solo `tool_calls`), que antes se enviaban con rol incorrecto al LLM.
- **TD-035** (P2): Cache miss en render loop: los dos `get_mut` separados (dirty marking + store) fusionados en uno solo.
- **TD-036** (P2): `update_managed_configs` lee solo los primeros 256 bytes del archivo instalado para extraer el tag de versión, en lugar del archivo completo.
- **TD-037** (P2): `undo_stack` limitado a 10 entradas con política FIFO.
- **TD-038** (P2): Errores LLM clasificados en mensajes accionables: 401 → API key, 429 → rate limit, red, 404 → modelo, 500 → server error, context length.
- **TD-034** (P3): Confirmación `run_command` muestra indicador ⚠ ámbar para patrones destructivos conocidos (`rm -rf`, `dd`, `curl|sh`, etc.).

## Recently Resolved (2026-04-08)

- **TD-026** (P2): Status bar — segmented bar rendered by GPU (`src/ui/status_bar.rs`). Segments: leader-mode indicator, CWD, git branch (left); exit code, date/time (right). Git branch polled async with 5s TTL cache. Toggle via `ToggleStatusBar` action + command palette. Phase 3 P2 complete.
- **TD-027** (P3): Tab rename via `<leader>,` — inline rename prompt in the active tab pill. Typing replaces the title display with `input▌` cursor; Enter confirms, Esc cancels. `TabManager::rename_active()` applies the new label. Works with 1 or 2+ tabs (tab bar forced visible during rename).

## Recently Resolved (2026-04-07)

- **TD-025** (P0): Mouse tab-bar click called `switch_to_index()` without `resize_terminals_for_panel()`, so the newly-active tab's PTY kept the pre-tab-bar row count and content overflowed below the visible area. Fix: added `resize_terminals_for_panel()` after `switch_to_index()` in the `MouseButton::Left` tab-bar hit handler (`app/mod.rs`). Keyboard tab switching already triggered the resize via the `tab_idx != tab_idx_before` guard.
- **TD-028** (P1): `MouseScrollDelta::PixelDelta.y` is in logical points on macOS but was divided by `cell_h` in physical pixels — giving ~0.5 lines/event on 2× Retina → very slow scroll. Fix: divide by `cell_h / scale_factor` (logical cell height). Auto-scroll to bottom on keypress: `send_key_to_active_terminal` now calls `terminal.scroll_to_bottom()` (`Scroll::Bottom`) before `write_input` so any keystroke while scrolled up jumps back to the prompt.

## Recently Resolved (2026-04-06)

- **TD-022** (P2): `cargo clippy --all-targets --all-features -- -D warnings` was failing with 36 lint violations. Fixed all 36. `cargo clippy -D warnings` now passes clean.
- **TD-021** (P2): `title_bar_style` is now parsed from Lua config. `llm.ui.width_cols` propagated into all new `ChatPanel` instances.
- **TD-020** (P2): `check_config_reload()` and `ReloadConfig` palette action both call `rewire_llm_provider()`.
- **TD-019** (P1): `submit_ai_query()` captures `panel_id` before spawn; all AI events tagged; `poll_ai_events()` routes correctly.
- **TD-018** (P1): `cmd_split()` creates `Terminal::new()` first; pane tree only mutated on success.
- **TD-017** (P1): `cmd_close_tab()` sets every owned terminal slot to `None` before removing the pane entry.
- **TD-OP-02** (P1): `is_pua()` redundant subranges removed; BMP PUA block covers all Nerd Font icons.
- **TD-OP-03** (P2): GlyphAtlas upgraded to 4096×4096 with epoch-based LRU eviction.
- **TD-OP-01** (P2): `unsafe impl Sync` removed from TextShaper; `Send` kept with SAFETY comment.
- **TD-016** (P3): `last_assistant_command()` filters tool-status lines (`⟳`/`✓`) before returning command.

> **TD-015** (resolved 2026-04-05): Shift+Enter → `\x1b[13;2u`, Shift+Tab → `\x1b[Z`.
> **TD-013** (resolved 2026-04-05): Rounded tab pills via `RoundedRectPipeline` + SDF WGSL shader.
> **TD-014** (resolved 2026-04-05): Tab bar background inherits `config.colors.background`.

## Closed / Invalid

- **TD-039** (P3): FALSO POSITIVO — `init_default_files` ya llama a `attach_file`, que tiene guard `if self.attached_files.contains(&path) { return; }`. AGENTS.md no puede duplicarse.
- **TD-040** (P3): Consolidado en TD-029.
