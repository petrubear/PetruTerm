# Session State

**Last Updated:** 2026-06-12
**Session Focus:** Phase 8 — ACP Integration (Agent Client Protocol)

## Branch: `acp`

## Estado actual

**Phases 1–7 COMPLETAS. Deuda técnica Wave 7 cerrada.**
**Deuda técnica: 0 items abiertos. Watch: AUDIT-CLEAN-02, AUDIT-PERF-10. Diferidos: TD-PERF-03, TD-PERF-05, AUDIT-MEM-04.**
**ci-local.sh: PASA en master. Branch `acp`: cargo check + fmt + test --lib limpios.**

## Esta sesión (2026-06-12) — Phase 8: ACP-3 implementado

### ACP-3 — `UiManager` wiring + dispatch

**`src/term/pty.rs`:**
- `PtyEvent::Exit` → `PtyEvent::Exit(i32)`. El child monitor thread pasa el código de `waitpid`. `EventListener` emite `Exit(0)` para `Event::Exit`, `Exit(code)` para `Event::ChildExit(code)`.

**`src/app/mux/mod.rs`:**
- `terminal_exit_codes: HashMap<usize, i32>` — se llena en `poll_pty_events` al recibir `Exit(code)`.
- `terminal_output_text(pane_id)` — texto visible del terminal para `terminal/output`.
- `terminal_exit_code(pane_id)` — `None` si aún corre, `Some(code)` si ya salió.
- `kill_terminal(pane_id)` — llama `pty.shutdown()` (SIGHUP).
- `open_terminal_for_acp(...)` — split horizontal + escribe el comando al shell del pane nuevo.

**`src/llm/acp/mod.rs`:**
- `try_send_prompt(content, ai_tx, terminal_tx)` — variante sync via `try_send`. Callable desde el main thread sin `.await`.

**`src/app/ui/mod.rs`:**
- Campos: `acp_session: Option<AcpSession>`, `acp_terminal_tx/rx: crossbeam Sender/Receiver<AcpTerminalRequest>`, `pending_acp_wait_for_exit: Vec<(usize, oneshot::Sender<i32>)>`.
- `new()`: detecta `LlmBackend::Agent` → `block_on(AcpSession::connect(...))` al arrancar.
- `close_panel()`: `self.acp_session = None` — dropea el proceso hijo.
- `submit_ai_query()`: branch ACP antes del path Provider — crea canales tokio mpsc, llama `try_send_prompt`, bridge tokio→crossbeam en task spawneada.

**`src/app/ui/providers.rs`:**
- `rewire_backend(config)`: rama `Provider` → `rewire_llm_provider`; rama `Agent` → `block_on(AcpSession::connect(...))`.
- Mensaje de comando desconocido incluye `/model` y `/agent`.

**`src/app/app_state.rs`:** `rewire_llm_provider` → `rewire_backend`.

**`src/app/frame.rs`:**
- `handle_acp_terminal_requests()`: drena `acp_terminal_rx`, maneja Create/GetOutput/WaitForExit/Kill. WaitForExit pendientes se resuelven en cada llamada al comprobar `terminal_exit_code`.

**`src/app/mod.rs`:** llama `handle_acp_terminal_requests()` en `about_to_wait`.

### Decisiones de implementación (ACP-3)

- **`PtyEvent::Exit(i32)` doble dispatch**: tanto `EventListener` (alacritty) como el child monitor thread pueden emitir `Exit`. `poll_pty_events` usa `!exited.contains(&id)` para evitar duplicados. El child monitor tiene el código real de `waitpid`; el listener emite 0.
- **`try_send_prompt` en lugar de `prompt` async**: el main thread (winit) no puede `.await`. `mpsc::Sender::try_send` es sync y no bloquea cuando hay espacio en el canal (cap=4).
- **Bridge tokio→crossbeam en task spawneada**: `submit_ai_query` crea canales tokio mpsc para `AiEvent` y `AcpTerminalRequest`, pasa los senders a `try_send_prompt`, y spawna una task que hace `tokio::select!` bridgeando ambos al crossbeam. La `streaming_handle` cancela la task si hay nueva query.
- **`open_terminal_for_acp` usa shell**: en lugar de modificar `Pty::spawn` para soporte de comando custom, se abre un pane normal y se escribe el comando al PTY (`cmd args\r`). El agente ve un terminal real con la shell corriendo el comando.
- **`terminal_exit_code` vs `terminals[id]`**: si el slot es `Some(...)` el proceso aún corre → devuelve `None`. Si es `None` Y `terminal_exit_codes` tiene entrada → devuelve `Some(code)`. Cubre el gap entre exit y cleanup.

### Pendiente
- ACP-4: Header UI `◈`/`✦` en `src/app/renderer/chat.rs::build_panel_header`.
- ACP-5: Slash commands `/model` y `/agent` en `src/app/ui/providers.rs::handle_slash_command`.

## Esta sesión (2026-06-11) — Phase 8: ACP planning

- Analizadas fuentes de Warp (`app/src/ai/agent_sdk/`, `crates/warp_cli/src/agent.rs`, `app/src/ai/harness_display.rs`).
- Protocolo ACP documentado: JSON-RPC stdio/WebSocket, SDK Rust oficial en crates.io.
- Branch `acp` creado desde master.
- Plan detallado escrito en `build_phases.md` como Phase 8 (ACP-0 a ACP-5).

## Esta sesión (2026-05-22) — Wave 7 deuda técnica — continuación

### AUDIT-ENERGY-05 — RESUELTO
- `poll_low_freq_tasks()` en `impl App` extrae battery poll + git poll (74 líneas) de `about_to_wait()`.
- `about_to_wait` reducido de 291 a 217 líneas. Enfocado en scheduling/wakeup.
- 102 tests pasan. clippy limpio.

### AUDIT-REFAC-07 — RESUELTO
- `RenamePrompt` struct privado en `src/app/ui/mod.rs` elimina 8 métodos duplicados (`tab_rename_*`/`workspace_rename_*`).
- Campos `Option<String>` reemplazados por `tab_rename: RenamePrompt` / `workspace_rename: RenamePrompt`.
- `handle_rename_key` reducido a 10 líneas con `RenamePrompt::handle_key`.
- `tab_rename_text()` añadido; `frame.rs:512` actualizado.

### AUDIT-REFAC-08 — RESUELTO
- `PanelMsgParams<'a>` struct en `chat.rs` reemplaza los 22 parámetros posicionales de `build_panel_messages`.
- Zero state extraído a `draw_panel_zero_state` (89 líneas).
- Suggestion pills extraídas a `draw_suggestion_pills` (64 líneas).
- `build_panel_messages`: 505 → 357 líneas. Supresión `too_many_arguments` eliminada.
- 102 tests pasan. clippy limpio.

### AUDIT-REFAC-06 — RESUELTO
- `SidebarDrawParams<'a>` en `src/app/renderer/mod.rs` elimina los 18 parámetros de `build_workspace_sidebar_instances`.
- Call site en `frame.rs` construye el struct; `overlay.rs` destructura al inicio. Supresión `too_many_arguments` eliminada.

### ci-local.sh — corregidos fallos bloqueantes
- 3 errores clippy `unneeded_struct_pattern` en `BlockKind::CodeBlock { .. }` → `BlockKind::CodeBlock` (`chat.rs` ×2, `renderer/mod.rs` ×1).
- `cargo fmt` aplicado a violaciones preexistentes en 7 archivos.
- Script pasa completo: clippy -D warnings + fmt --check + test --lib + audit.

### Validación de reaperturas de Copilot
- AUDIT-CLEAN-03: rechazada — grep confirmó 0 coincidencias de `#![allow(dead_code)]` en los 4 archivos citados.
- AUDIT-ENERGY-05: aceptada como PARCIAL — WaitUntil deduplication resuelta, pero about_to_wait (290 líneas) sigue mezclando battery/git/blink/PTY.
- AUDIT-REFAC-07: aceptada — 8 métodos tab_rename_*/workspace_rename_* duplicados confirmados.
- AUDIT-REFAC-08: aceptada con corrección — build_panel_messages 505 líneas real (548-1052); rango 5-1455 del claim era el archivo completo.
- AUDIT-PERF-10: aceptada como watch — micro-regresiones 1-2% en shaping/rasterize.

## Esta sesión (2026-05-12) — Release prep v0.1.9

### Context menu regression fix — COMPLETO

- Bug corregido: tras abrir el color picker de tabs con click derecho, el siguiente click derecho en la terminal seguía mostrando ese menú en lugar del menú estándar.
- `src/ui/context_menu.rs`: nuevo `default_items()` y `open_default()` para restaurar explícitamente el menú base (`Copy`, `Paste`, `Clear`, `Ask AI`).
- `src/app/mod.rs`: el menú contextual normal de la terminal ahora usa `open_default(...)`.
- Cobertura: prueba unitaria `open_default_resets_tab_color_picker_items`.

### CI local — estado actual observado

- `scripts/ci-local.sh` se ejecutó completo, pero expuso dos fallos de clippy ya existentes:
  - `src/app/mux/snapshot.rs`: `clippy::unnecessary_sort_by`
  - `src/app/mux/workspace.rs`: `clippy::too_many_arguments`
- `cargo fmt --check`, `cargo test --lib` y `cargo audit` siguieron ejecutándose; los tests de librería pasaron.

## Esta sesión (2026-05-12) — Atlas split + PGO

### Atlas split — COMPLETO

**Cambio:** `GlyphAtlas` (4096×4096 `Rgba8Unorm`, 64 MiB) → dos atlases separados:
- `GlyphAtlas` (mask): 4096×4096 `R8Unorm` = **16 MiB** (texto, grayscale)
- `ColorAtlas`: 1024×1024 `Rgba8Unorm` = **4 MiB** (emoji/color glyphs)
- Total: **20 MiB** vs 64 MiB anterior = **68% reducción VRAM**

**Archivos modificados:**
- `src/renderer/atlas.rs`: `GlyphAtlas` → R8Unorm + helper `pad_r8_rows()`; nuevo `ColorAtlas`
- `src/renderer/pipeline.rs`: `CELL_SHADER` atlas bind group → 4 bindings (mask tex/sampler + color tex/sampler); shader usa `t_mask`/`t_color` según `FLAG_COLOR_GLYPH`
- `src/renderer/gpu.rs`: campo `pub color_atlas: ColorAtlas`; `atlas_and_queue()` → `atlases_and_queue()` retorna ambos; `make_main_atlas_bind_group()` 4 entries
- `src/font/shaper.rs`: `rasterize_to_atlas(key, atlas, color_atlas, queue)` — `Mask/SubpixelMask` → GlyphAtlas (raw 1-byte/pixel), `Color` → ColorAtlas (RGBA); `warmup_atlas` actualizado
- `src/app/renderer/mod.rs`, `terminal.rs`: call sites actualizados
- `src/app/frame.rs`: `color_atlas.next_epoch()`, eviction y clear en todos los paths
- `benches/build_instances.rs`: actualizado para `color_atlas`

**No obvio:** `pad_r8_rows()` en atlas.rs — wgpu exige `bytes_per_row` múltiplo de 256 para R8Unorm. La mayoría de glyphs son más estrechos que 256px, así que se rellena con zeros antes del upload. Solo ocurre en cache miss, no hot path.

### PGO — COMPLETO

`scripts/build_pgo.sh` — proceso 3 fases:
1. Build instrumentado: `RUSTFLAGS="-Cprofile-generate=..."`
2. Workload: benches de shaping + search + rasterize + build_instances (--profile-time 3)
3. Build optimizado: `RUSTFLAGS="-Cprofile-use=..."` → `target/pgo/release/petruterm`

`llvm-profdata` disponible via `xcrun` (Xcode). Beneficio esperado: 5-10% en hot paths.

## Esta sesión (2026-05-12) — Tier 5: rayon

### Tier 0: criterion baseline
18 benchmarks ejecutados en release profile (Apple M-series). Baseline en `.criterion-baselines/`.

### Tier 5: rayon — search parallelization

**Hallazgo clave — vertex offset NO paralelizable con rayon:**
`build_instances` vertex offset (200 rows): serial 9.7 µs → rayon 138 µs (14x PEOR).
Fork-join overhead (~130 µs) > tiempo de computo (~10 µs). Rayon solo ayuda con tareas > ~50 µs.

**search_active_terminal — collect-then-parallel:**
`src/app/mux/mod.rs`: dos fases:
1. Serial (lock held): leer grid en flat `Vec<char>` (liberación rápida del lock).
2. Parallel (lock released): `par_chunks(cols)` + `flat_map_iter` → collect.
Fallback serial para < 400 filas (overhead rayon > ganancia).
Resultado: 2.3 ms → 278 µs = **8-9x speedup** en scrollback de 10k filas.
Lock hold time: de 2 ms → ~200 µs (beneficio adicional para PTY output concurrente).

**build_instances two-phase refactor (sin rayon):**
`src/app/renderer/terminal.rs`: fase 1 = shape+cache, fase 2 = emit serial.
Código más limpio; rayon documentado como no aplicable (benchmark `build_frame_hit_large_{serial,par}`).

**Bench baseline actualizado:** 3 nuevos entries en `.criterion-baselines/search_cold_par/`.

| Benchmark | Tiempo |
|---|---|
| build_frame_hit (cache hit) | 754 ns |
| build_frame_miss (cache miss) | 31.5 µs |
| build_row_hit | 124 ns |
| build_row_miss | 806 ns |
| rasterize_glyph_ascii | 1.24 µs |
| rasterize_line_ascii | 30.5 µs |
| rasterize_line_ligatures | 44.4 µs |
| rasterize_line_unicode | 39.7 µs |
| search_cold (common word) | ~2.1 ms |
| search_cold (medium) | ~1.94 ms |
| search_incremental | 18.8 µs |
| shape_line_ascii | 245 ns |
| shape_line_ascii_cached | 247 ns |
| shape_line_ligatures | 515 ns |
| shape_line_ligatures_cached | 517 ns |
| shape_line_unicode | 5.3 µs |

`critcmp` instalado. Gate CI `bench-regression` activo: falla si regresion > 5% vs baseline.

## Esta sesión (2026-05-11) — Auditoría Wave 1 + Wave 2

### Wave 1 — Riesgo y desperdicio inmediato (4 items)

**AUDIT-SEC-01** — Path traversal en `write_file` (`src/app/ui.rs`):
Canonicaliza el ancestro mas cercano existente y verifica `starts_with(cwd)` antes del dialogo de confirmacion. Soporta ficheros en directorios nuevos (no existentes aun).

**AUDIT-SEC-02** — MCP local sin trust gate:
- Nuevo `src/llm/mcp/trust.rs`: lista de cwds confiables en `~/.config/petruterm/mcp_trust.json`.
- `mcp/config.rs`: `load_global()` + `load_local()` separados; `load()` legacy eliminado.
- `UiManager::new()` y `reload_mcp()` solo inician `.petruterm/mcp.json` si `is_trusted(cwd)`.
- Palette: nueva accion "Trust local MCP config".

**AUDIT-ENERGY-02** — Battery poll infinito en desktop:
`battery_polled: bool` reemplaza `battery_status.is_none()` como guarda; poll cada 30 s, no cada iteracion.

**AUDIT-PERF-06** — `max_fps` muerto + `animation_fps` fantasma:
`flush_redraw_request` respeta `1_000_000_000/max_fps ns`; `about_to_wait` inyecta `frame_deadline` en `WaitUntil`. `animation_fps` eliminado de `perf.lua`.

### Wave 2 — Best in class (5 items)

**AUDIT-ENERGY-03** — Tokio + MCP eager cuando `llm.enabled = false`:
Runtime cambia a `current_thread`; bloque MCP omitido completamente.

**AUDIT-SEC-03** — Skills/steering locales sin trust gate:
`load(cwd, include_local: bool)` en ambos managers; se pasa `trust::is_trusted(&cwd)`.

**AUDIT-ENERGY-04** — Wakeups periodicos no gobernados por battery saver:
Git poll guard: 1 s normal / 60 s battery saver. `next_minute_wake` solo cuando `status_bar.enabled`. Battery poll condicionado a `window_focused`.

**AUDIT-THEME-01** — Status bar hardcoded Dracula:
`StatusBarColors` derivado de `ColorScheme::status_bar_colors()`. `StatusBar::build()` recibe `&StatusBarColors`. Constantes eliminadas de `status_bar.rs`.

**AUDIT-PERF-07** — Tab bar aloca `Vec<String>` por frame:
`tab_bar_titles: Vec<String>` → `tab_bar_titles_hash: u64` (FxHasher). Zero aloc en hot path.

## Esta sesión (2026-05-11) — Pane border padding + scrollbar gap

### Bugs corregidos

**Pane focus border overlaps text (top/right/bottom):**
`build_focus_border` solo empujaba el borde izquierdo fuera del contenido cuando `col_offset == 0`.
Los bordes right, bottom y top usaban solo `inset = border/2`, haciendo que el stroke se superpusiera
al texto de la primera/última fila/columna. Fix: agregar `pad_right`, `pad_bottom`, `pad_top: bool`
a `PaneInfo` (propagados desde `PanePad` en `collect_leaf_infos_impl`). Cuando la bandera es `false`
(borde en el edge del viewport, sin separator), el rect del borde se empuja un `cell_w`/`cell_h`
hacia afuera — los pixels fuera del viewport quedan GPU-clipped. Mismo patrón que ya existía para left.

**Scrollbar gap from pane border (split panes):**
Cuando `pad_right = true`, el pane tiene una "pad cell" entre el contenido y el separator. El scrollbar
usaba columna `term_cols - 1` (última columna de contenido), dejando esa pad cell vacía entre el scroll
track y el borde coloreado del pane (~`cell_w` de gap visible). Fix: cuando `pad_right = true`, el
scrollbar usa columna `term_cols` (la pad cell), quedando flush contra el separator/border.
`scroll_bar_state` actualizado para incluir `pad_right` en la cache key.

## Esta sesión (2026-05-07) — Paleta, menú nativo, tab color picker

### Paleta de comandos — auditoria y actualizacion
- Agregadas entradas faltantes: `OpenConfigFolder` y `ClearAiContext`.
- Locales actualizados: `palette.open_config_folder`, `palette.clear_ai_context` (en/es).

### Menú nativo (menu.rs)
- Agregado `Clear AI Context` en el submenu AI (entre "Undo Last Write" y el separador enable/disable).
- Locale `menu.clear_ai_context` agregado a `en.toml`.

### Tab color picker — COMPLETO
- `Tab.accent_color: Option<[f32; 4]>` en `src/ui/tabs.rs`.
- `TabManager::set_tab_color(idx, color)` y `active_accent(default)`.
- `ContextAction::SetTabColor(usize, Option<[f32; 4]>)` en `context_menu.rs`.
- `ContextMenuItem.swatch_color: Option<[f32; 4]>` — renderer dibuja `● ` en el color del swatch.
- `ContextMenu::open_tab_color_picker(tab_idx, brights, ...)` — 7 swatches de `colors.brights[1..7]` + Reset.
- Clic derecho en zona del tab bar → abre el color picker (no el menú normal).
- Tab activo: fondo + underline en `accent_color`. Tab inactivo con color: solo underline. Sin color: sin cambio.
- `build_focus_border` usa `tab_accent: Option<[f32; 4]>` — borde del pane sigue el color del tab activo.
- Cache del tab bar invalidado (`tab_bar_instances_cache.clear()`) al cambiar color.

## Esta sesión (2026-05-06) — Phase 7: A-1..A-3 + I-1..I-4

### A-1/A-2/A-3: AI Agent Actions — COMPLETAS
- `src/llm/agent_action.rs`: `AgentAction` enum (RunCommand, OpenFile, ExplainOutput) + parser de tags `<action>...</action>` en respuestas LLM.
- `PanelState::ConfirmAction(AgentAction)`: confirm UI inline con pills `[Run] [Cancel]` sobre sep_row. y/Enter confirma, n/Esc cancela.
- Handlers: RunCommand → write PTY, OpenFile → `open -e <path>`, ExplainOutput → captura N líneas del grid y hace nueva query LLM.
- "Always allow" checkbox: `always_allow_actions: bool` en ChatPanel; si activo las acciones se ejecutan sin confirmar.
- Post-ejecución: append mensaje de sistema al transcript.

### I-1: Input shadow buffer — COMPLETA
- `src/term/input_shadow.rs`: `InputShadow` mirrors keystrokes entre OSC 133-A y B.
- Tracks buf + cursor (byte offset). Resets en Ctrl+C/U/K/W/A/E, deactivate en CommandStart/End.
- Deactivate también en Up/Down (history navigation invalida el buffer).

### I-2: Syntax coloring — COMPLETA
- `src/term/tokenizer.rs`: `tokenize_command` + `build_syntax_fg`. `TokenKind`: Command, Arg, Flag, Pipe, Redirect, String.
- `CommandResolver`: lookup no-bloqueante en PATH con cache. Verde = válido, rojo = no encontrado, cian = flag, amarillo = string, naranja = pipe.
- `SyntaxOverlay` aplicado en `collect_grid_cells_for`. Configurable via `input_syntax_highlight`.
- Shadow se desactiva si ArrowRight/Tab en buf-end sin ghost aceptado (previene drift con zsh-autosuggestions).

### I-3: Ghost text — COMPLETA
- `HistoryIndex`: carga `~/.zsh_history` (formato extendido) o `~/.bash_history`, most-recent-first.
- `InputShadow.ghost: Option<String>` actualizado en cada keypress cuando cursor al final.
- `GhostOverlay` reemplaza chars + fg con `ui_muted` en el row del cursor.
- `accept_ghost()` en Tab/ArrowRight escribe el sufijo al PTY. Gateado tras `input_ghost_text` config para no interferir con zsh-autosuggestions.

### I-4: Flag hints — COMPLETA
- `src/term/flag_db.rs`: `lookup_flag(cmd, flag)` — 10 comandos, ~130 flags.
- `FlagHintOverlay` en cursor.row+1, alineado con la posición del flag, color `ui_muted`.
- Desaparece cuando el último token deja de ser Flag.

### Bugfixes input decoration
- **Bug 1**: `accept_ghost()` disparaba aunque `input_ghost_text=false` → escribía bytes extra al PTY rompiendo zsh-autosuggestions.
- **Bug 2**: Shadow cursor se desincronizaba de terminal cursor cuando zsh-autosuggestions aceptaba sugerencia (cmd_start_col incorrecto → colores en columnas erróneas).
- Fix: gate `accept_ghost` tras flag; deactivate shadow en Up/Down y en ArrowRight sin ghost aceptado.
- `input_syntax_highlight: bool` y `input_ghost_text: bool` ahora configurables en `ui.lua`.

## Esta sesión (2026-05-06) — B-4 bug fixes

### Bugs corregidos en B-4

**shell-integration.zsh — OSC 133 no se emitía:**
Añadidos: `D;$?` + `A` en `precmd`; `B;$1` + `C` en `preexec`.

**`Osc133Marker::CommandStart` — texto incluía el prompt:**
Comando viaja embebido en la secuencia: `ESC]133;B;<cmd>ST`.

**`block_at_absolute_row` — devolvía siempre el primer bloque:**
Cambiado a `iter().rev().find()`.

**Context menu — mezclado con menú original:**
Solo se abre al hacer clic derecho sobre el exit-code pill.

**block_at_cursor — hover solo en columna 0:**
Expandido para cubrir todo el ancho del pane.

**ClearBlock eliminado:** `ContextAction::ClearBlock`, `BlockManager::remove_block` removidos.

**Gutter bar eliminada:** 2px stripe izquierdo removido del renderer.

## Esta sesión (2026-05-12) — Workspace Persistence

### COMPLETA. CI limpio (101 tests, 0 clippy warnings, fmt ok).

**Archivos nuevos:**
- `src/app/mux/snapshot.rs` — tipos `WorkspaceSnapshot/TabSnapshot/PaneNodeSnapshot/SplitDirSnapshot/SavedWorkspaceInfo` + `workspaces_dir/list_saved_workspaces/load_workspace/save_snapshot`

**Archivos modificados:**
- `src/app/mux/workspace.rs` — `save_workspace`, `save_all_workspaces`, `build_workspace_snapshot`, `snapshot_pane_node`, `restore_workspace` + helpers libres `home_str` y `restore_pane_recursive`
- `src/app/mux/mod.rs` — `pub mod snapshot;`
- `src/config/schema.rs` — `WorkspacesConfig { auto_save_on_exit: bool, auto_save_on_switch: bool }` + campo en `Config`
- `src/config/lua.rs` — parsing de `workspaces.*`
- `config/default/config.lua` — defaults `workspaces`
- `src/ui/palette/actions.rs` — `SaveWorkspace`, `OpenSavedWorkspaces`, `RestoreWorkspace(String)` + 2 palette items
- `src/app/input/mod.rs` — `Leader W s` (guardar), `Leader W L` (abrir paleta de guardados)
- `src/app/ui/mod.rs` — handlers para los 3 nuevos actions
- `src/app/mod.rs` — auto-save antes de `event_loop.exit()` cuando `auto_save_on_exit = true`
- `src/ui/panes.rs` — `next_node_id` → `pub(crate)`

**No obvio — `restore_pane_recursive` es free fn:**
No puede ser método de Mux porque toma `&mut Mux` y llama `mux.open_terminal` mientras itera sobre el snapshot. Si fuera método, el borrow checker rechazaría el doble borrow. Diseño: función libre que recibe `&mut Mux` explícitamente.

**Formato JSON guardado:** `~/.config/petruterm/workspaces/<name>.json`

## Sesiones anteriores (resumen)

- 2026-05-05: Phase 7 H-1 + B-1 + B-2 + B-3 + B-4 + Auditoría Waves 1-4
- 2026-04-30: Phase 6 W-5..W-8
- 2026-04-29: Phase 6 W-1..W-4
- 2026-04-28: G-2-overlay
- 2026-04-26: Chat input UX (TD-UI-01/02/03)
- 2026-04-25: Phase 5 G-1 + G-2 + UX polish + G-0
- 2026-04-24: D-5 + /skills + /mcp + Leader+w + MCP fixes
- 2026-04-23: Focus border + sidebar pills + E + D-4
- 2026-04-22: C-3.5 + D-4
- 2026-04-21: C-1 bugs + C-2 + C-3
- 2026-04-20: C-1 inicial + B
- 2026-04-19: A + 3.6
