# Technical Debt Registry

**Last Updated:** 2026-04-08
**Open Items:** 10
**Critical (P0):** 1 | **P1:** 3 | **P2:** 5 | **P3:** 1

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

- **TD-030** (P0) [Kiro]: **Archivos adjuntos al LLM sin límite de tamaño** (`src/app/ui.rs:340-344`). `std::fs::read_to_string` se inyecta completo en el system message sin ningún cap. Un archivo de varios MB puede agotar memoria, exceder el context window del LLM silenciosamente, o ser explotado con prompt injection. Fix: limitar a un máximo configurable (ej. 512 KB por archivo, 1 MB total).

---

## P1 - High Priority

- **TD-029** (P1) [Kiro]: **`cwd` no canonicalizado rompe `execute_tool` en macOS** (`src/llm/tools.rs:172`, `src/app/ui.rs`). El check `canon.starts_with(cwd)` compara una ruta canonicalizada contra `cwd` no canonicalizado. En macOS, `/var/folders/...` es symlink a `/private/var/...`, lo que hace que `starts_with` devuelva `false` para rutas legítimas dentro del proyecto — las herramientas `read_file` y `list_dir` fallan silenciosamente. Fix: canonicalizar `cwd` una vez antes del `spawn` en `submit_ai_query` y usar esa versión en `execute_tool`. Consolida TD-040.

- **TD-031** (P1) [Kiro]: **Regex compilada en cada llamada a `sanitize_command`** (`src/llm/shell_context.rs:37-44`). Dos `Regex::new(...).unwrap()` se ejecutan en cada invocación. Deben ser `static` con `once_cell::sync::Lazy`.

- **TD-033** (P1) [Kiro]: **Historial corrupto en el fallback de tool rounds agotados** (`src/app/ui.rs:480-503`). Cuando se agotan los 10 rounds, el fallback mapea mensajes con `role == "tool"` a `ChatRole::System` (brazo `_` del match), enviándolos con rol incorrecto al LLM. Los mensajes `assistant` que sólo contienen `tool_calls` quedan con `content: ""`. El LLM puede responder de forma incoherente sin que el usuario lo sepa. Fix: extender `ChatRole` con `Tool` o reutilizar el path `agent_step` en el fallback.

---

## P2 - Medium Priority

- **TD-032** (P2) [Kiro]: **Clone masivo del historial en cada tool round** (`src/app/ui.rs:373`). `api_msgs.clone()` se llama hasta 10 veces por query, clonando el vector completo incluyendo el system message con archivos adjuntos. Con archivos grandes esto puede ser varios MB por clone. Considerar pasar por referencia o usar `Arc`.

- **TD-035** (P2) [Kiro]: **Doble lookup al hashmap en el render loop** (`src/app/renderer.rs:144,165`). Se llama `entry()` para insertar y luego `get()` para leer en el mismo `row_caches` hashmap, resultando en dos lookups por fila. Refactorizar para usar la referencia retornada por `entry()` directamente.

- **TD-036** (P2) [Kiro]: **`update_managed_configs` lee archivo completo del disco para comparar versión** (`src/config/mod.rs:89-109`). En cada hot-reload se lee todo `keybinds.lua` solo para extraer un número de versión. La versión debería cachearse en memoria tras la primera lectura.

- **TD-037** (P2) [Kiro]: **Undo stack sin límite de tamaño** (`src/app/ui.rs:265`). `self.undo_stack` es un `Vec` que crece indefinidamente. Con muchas escrituras de archivos grandes en una sesión, acumula todo el contenido anterior en memoria. Fix: limitar a N entradas (ej. 10) con política FIFO.

- **TD-038** (P2) [Kiro]: **Errores LLM sin contexto accionable para el usuario** (`src/app/ui.rs:163`). El mensaje crudo de `anyhow` se muestra directamente (ej. `"OpenRouter returned an error status"`). No se diferencia entre API key inválida, rate limit, sin conexión, o modelo no disponible. Cada caso debería tener un mensaje con acción clara.

---

## P3 - Low Priority

- **TD-034** (P3) [Kiro]: **`run_command` sin indicador de riesgo visual** (`src/app/ui.rs:211-222`). El comando propuesto por el LLM ya requiere confirmación explícita (y/n), pero la UI no diferencia ni advierte sobre patrones potencialmente destructivos (ej. `rm -rf`, `curl | sh`). Fix: añadir un indicador de riesgo junto al prompt de confirmación para patrones conocidos peligrosos.

---

## Closed / Invalid (2026-04-08)

- **TD-039** (P3) [Kiro]: FALSO POSITIVO — `init_default_files` ya llama a `attach_file`, que tiene guard `if self.attached_files.contains(&path) { return; }` (`chat_panel.rs:328`). AGENTS.md no puede duplicarse.
- **TD-040** (P3) [Kiro]: Consolidado en TD-029 — misma raíz (`cwd` no canonicalizado). Fix único en `submit_ai_query` antes del `spawn`.

## Recently Resolved (2026-04-08)

- **TD-026** (P2): Status bar — segmented bar rendered by GPU (`src/ui/status_bar.rs`). Segments: leader-mode indicator, CWD, git branch (left); exit code, date/time (right). Git branch polled async with 5s TTL cache. Toggle via `ToggleStatusBar` action + command palette. Phase 3 P2 complete.
- **TD-027** (P3): Tab rename via `<leader>,` — inline rename prompt in the active tab pill. Typing replaces the title display with `input▌` cursor; Enter confirms, Esc cancels. `TabManager::rename_active()` applies the new label. Works with 1 or 2+ tabs (tab bar forced visible during rename).

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
