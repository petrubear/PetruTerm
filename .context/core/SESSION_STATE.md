# Session State

**Last Updated:** 2026-04-15
**Session Focus:** Phase 3.5 — Performance Sprint (en curso)

## Branch: `master`

## Estado actual

**Phase 1–3 COMPLETE. Phase 3.5 (performance + memory) EN PROGRESO.**

Commits de esta sesión:
- `334dee0` perf: TD-PERF-06/07/09 — skip double rasterization + lazy row cache + mtime guard
- `9a2fcb8` fix: TD-MEM-06/07 — byte_to_col_buf shrink + ChatPanel message cap
- `86683c7` perf: Phase 3.5-E/H — PTY QoS + Lua cache + native build profile
- `6730465` fix: TD-MEM-05 + TD-MEM-08 — word_cache LRU + terminal_shell_ctxs cleanup
- `d0c3b1b` fix: TD-MEM-03 — rebuild atlas bind groups after atlas.clear()
- `188c1e8` fix: TD-MEM-01 + TD-MEM-02 — atlas eviction reclaims physical space + LCD eviction

## Build
- **cargo check:** PASS — 0 errores, 0 warnings (verificado 2026-04-15)

## Deuda técnica: estado post-sesión

### Resueltos hoy (P1)
- TD-MEM-01: GlyphAtlas cursor_fill_ratio + preemptive clear cuando evicción no reclaim espacio
- TD-MEM-02: LcdGlyphAtlas epoch tracking + evict_cold + clear_lcd_rasterizer_cache
- TD-MEM-03: Bind groups stale tras atlas.clear() → GpuRenderer::rebuild_atlas_bind_groups()
- TD-MEM-05: word_cache HashMap::clear() → lru::LruCache(1024)
- TD-MEM-08: terminal_shell_ctxs leak por terminal cerrado → Mux.closed_ids drain

### Falso positivo documentado
- TD-MEM-04: SwashCache NO es el leak — el código usa `get_image_uncached`, no `get_image`

### P1 pendientes
- ~~TD-MEM-06~~: `byte_to_col_buf` shrink condicional — RESUELTO
- ~~TD-MEM-07~~: `ChatPanel.messages` truncado a 200 + drain wrapped_cache — RESUELTO

### P2 notables abiertos (ver TECHNICAL_DEBT.md para lista completa)
- TD-MEM-09: scrollback alto (40-200 MB con muchos tabs)
- TD-MEM-10/11: file_picker items + SkimMatcherV2 por frame
- TD-MEM-12: Tokio tasks de streaming LLM no cancelados al cerrar panel
- TD-MEM-19: cursor blink + reloj + git polling cuando ventana sin foco

## Deuda técnica abierta (performance)

- ~~TD-PERF-06~~: skip Swash cuando LCD tiene hit — RESUELTO
- ~~TD-PERF-07~~: reshape storm movido a branch de clear() — RESUELTO
- ~~TD-PERF-09~~: mtime guard en shell context — RESUELTO
- ~~TD-PERF-10~~: cursor blink invalida panel de chat — RESUELTO
- ~~TD-PERF-11~~: text search incremental — RESUELTO
- ~~TD-PERF-12~~: scratch buffers en push_shaped_row — RESUELTO
- ~~TD-PERF-13~~: format! spam + O(n) spinner → scratch_lines + frame_counter — RESUELTO
- TD-PERF-08 (P1): PresentMode::Fifo → Mailbox para menor latencia input-to-pixel
- ... (ver TECHNICAL_DEBT.md para lista completa)

---

## Sesiones anteriores

### 2026-04-15 — Phase 3.5-E/H + Memory leak audit
- PTY thread → QOS_CLASS_UTILITY (efficiency cores) via OnceLock
- Lua bytecode cache (~/.cache/petruterm/lua-bc/*.luac)
- release-native profile en Cargo.toml; target-cpu=apple-m1 en bundle.sh
- Auditoría de memory leaks: 12 items (TD-MEM-01..12 + TD-MEM-19)
- Resueltos: TD-MEM-01, 02, 03, 05, 08 (5 P1 en una sesión)

### 2026-04-10 — Exit code per-pane + click para detalles
- `poll_pty_events()` → `(Vec<usize>, Vec<usize>)` (IDs con datos + IDs que salieron)
- `terminal_shell_ctxs: HashMap<usize, ShellContext>` — contexto por terminal_id
- Shell integration: `shell-context-$$.json` per-PID con fallback al global
- Click en badge rojo → context menu con exit code + comando + "Copy command"
- `ContextAction::Label` — fila no-interactiva (dim, sin hover)
