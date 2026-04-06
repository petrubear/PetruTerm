# Session State

**Last Updated:** 2026-04-06
**Session Focus:** Deuda técnica — resolución y tests de los 4 ítems abiertos

## Branch: `master`

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

## Build & Tests
- **cargo build:** PASS (0 errors — 2026-04-06)
- **cargo test:** 16/16 PASS — 3 passes anteriores + 13 nuevos
- **branch:** master (stable, 2 commits adelante de origin)

## Session anterior (2026-04-06 — UX polish)

### Mouse selection (fixed)
`setMovableByWindowBackground: NO` — el whole-window drag estaba rompiendo la selección de texto.

### Default configs completas
`ensure_default_configs()` — escribe archivos faltantes en cada arranque sin sobrescribir los existentes. `ui.lua`, `llm.lua`, `perf.lua` actualizados con todos los campos del schema.

### Keybinds en la palette
`PaletteAction.keybind: Option<String>` — `built_in_actions(&Config)` resuelve los atajos desde `config.keys`. Renderizados alineados a la derecha en color tenue.

### Context menu (right-click)
`src/ui/context_menu.rs` — Copy/Paste/Clear con keybinds. Hover highlight. Se cierra con click afuera o cualquier tecla.
