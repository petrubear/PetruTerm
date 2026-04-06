# Session State

**Last Updated:** 2026-04-06
**Session Focus:** Resolución completa de los 6 TDs de auditoría Codex + clippy limpio

## Branch: `master`

## Session Notes (2026-04-06 — auditoría Codex)

- Se añadieron 6 ítems nuevos al registro de deuda técnica (`TD-017`..`TD-022`).
- Origen explícito de esos ítems: auditoría de Codex del 2026-04-06.
- Hallazgos principales:
  - `CloseTab` no limpia pane tree ni PTY asociados.
  - `cmd_split()` deja estado inválido si falla la creación del terminal.
  - El streaming del AI panel no está ligado al pane origen.
  - `ReloadConfig` / hot reload no reconstruyen estado derivado de forma consistente.
  - Parte del config documentado no se parsea o no se aplica.
  - El contexto del proyecto afirmaba cero deuda abierta, pero `clippy -D warnings` falla.

## Session Notes (2026-04-06 — TD cleanup)

### TD-016 (P3) — run bar con líneas de tool-status (RESUELTO)
- `last_assistant_command()` en `src/llm/chat_panel.rs` filtra con `.filter()` las líneas que empiezan con `⟳` o `✓` antes de devolver el comando.
- 7 tests unitarios añadidos en `chat_panel.rs::tests`.

### TD-OP-02 (P1) — is_pua() con subrangos redundantes (RESUELTO)
- Se eliminaron 5 subrangos de `is_pua()` (Devicons 0xE700, Font Awesome 0xF000, Seti-UI 0xE5FA, Font Logotypes 0xE200, Weather 0xE300) — todos subconjuntos del BMP PUA `0xE000..=0xF8FF`.
- Elimina los warnings `unreachable_patterns`. Doc-comment ampliado.
- Test `test_is_pua()` extendido con 20+ assertions cubriendo todas las ranges eliminadas (siguen funcionando vía BMP PUA principal).

### TD-OP-03 (P2) — GlyphAtlas sin eviction ni tamaño suficiente (RESUELTO)
- Atlas aumentado de 2048→4096 px (4× capacidad, 64 MiB en Metal).
- Añadida eviction LRU basada en epoch: `next_epoch()` por frame; `evict_cold(60)` al 90% de ocupación (`is_near_full()`); `clear()` como último recurso.
- `AtlasEntry` lleva `last_used: u64`.
- 5 tests en `atlas.rs::tests` que validan la lógica de epoch, eviction y umbral sin necesidad de GPU.

### TD-OP-01 (P2) — unsafe impl Sync for TextShaper incorrecto (RESUELTO)
- Eliminado `unsafe impl Sync for TextShaper` — FreeType no es thread-safe; permitir `&TextShaper` compartida entre hilos sería UB.
- Se mantuvo `unsafe impl Send` con bloque `// SAFETY:` que documenta el invariante: TextShaper vive exclusivamente en el main thread, nunca se aliasa concurrentemente.
- Validación: `Arc::new(shaper)` debe rechazarse por el compilador (no-Send-Sync check manual).

## Session Notes (2026-04-06 — multi-pane rendering)

### Multi-pane splits COMPLETO
- `PaneInfo` + `PaneSeparator` en `src/ui/panes.rs` — info de layout por pane.
- `Mux::active_pane_infos()`, `active_pane_separators()`, `collect_grid_cells_for()`, `resize_all()`, `active_pane_count()`.
- `RenderContext.row_caches: HashMap<usize, RowCache>` — cache por terminal.
- `build_all_pane_instances()` en `app/mod.rs` — itera todos los panes, aplica col/row offset.
- `build_pane_separators()` en `app/renderer.rs` — dibuja separadores con `FLAG_CURSOR`.
- Detección de cambio de pane count en `KeyboardInput` → llama `resize_terminals_for_panel()`.
- Keybinds: `^B %` (split horizontal), `^B "` (split vertical), `^B x` (close pane).

## Session Notes (2026-04-07 — pane bug fixes)

### Leader+Shift keys (%, ", &) not working (RESUELTO)
- Raíz: el Shift keydown genera un `KeyboardInput` con `Key::Named(NamedKey::Shift)`.
  Este evento entraba al bloque leader, ponía `leader_active = false` y retornaba sin
  despachar acción — el leader quedaba consumido antes de que llegara el `%` o `"`.
- Fix: al inicio del bloque leader, si `event.logical_key` es una tecla modificadora
  (Shift/Ctrl/Alt/Super/Meta/Hyper) → `return` sin tocar `leader_active`.
- Archivos: `src/app/input/mod.rs`

### Exit cerraba el tab completo (RESUELTO)
- Raíz 1: `close_terminal` buscaba el tab por `focused_terminal == terminal_id` — fallaba
  si el terminal que salió no era el enfocado.
- Raíz 2: siempre cerraba el tab entero aunque hubiera más panes.
- Fix: buscar el tab por `leaf_ids().contains(&terminal_id)`. Si el tab tiene múltiples
  panes → llamar `pane_mgr.close_specific(terminal_id)` (solo elimina ese pane del árbol).
  Solo cerrar el tab si era el último pane.
- Nuevo método: `PaneManager::close_specific(terminal_id)` en `src/ui/panes.rs`.
- Archivos: `src/app/mux.rs`, `src/ui/panes.rs`

## Build & Tests
- **cargo build:** PASS (0 errors — 2026-04-07)
- **cargo test:** 16/16 PASS
- **cargo clippy --all-targets --all-features -- -D warnings:** PASS (0 errors, 0 warnings — 2026-04-07)
- **branch:** master

## Session anterior (2026-04-06 — UX polish)

### Mouse selection (fixed)
`setMovableByWindowBackground: NO` — el whole-window drag estaba rompiendo la selección de texto.

### Default configs completas
`ensure_default_configs()` — escribe archivos faltantes en cada arranque sin sobrescribir los existentes. `ui.lua`, `llm.lua`, `perf.lua` actualizados con todos los campos del schema.

### Keybinds en la palette
`PaletteAction.keybind: Option<String>` — `built_in_actions(&Config)` resuelve los atajos desde `config.keys`. Renderizados alineados a la derecha en color tenue.

### Context menu (right-click)
`src/ui/context_menu.rs` — Copy/Paste/Clear con keybinds. Hover highlight. Se cierra con click afuera o cualquier tecla.
