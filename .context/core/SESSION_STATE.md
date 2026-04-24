# Session State

**Last Updated:** 2026-04-24
**Session Focus:** MCP D1/D2/D3 + battery/GPU optimizations

## Branch: `master`

## Estado actual

**Phase 1–3 + 3.5 COMPLETE. Fase A COMPLETE. Fase 3.6 COMPLETE. Fase B COMPLETE. Fase C-1 COMPLETE. Fase C-2 COMPLETE. Fase C-3 COMPLETE. Fase C-3.5 COMPLETE. Fase D-1/D-2/D-3/D-4 COMPLETE. v0.1.3 publicado.**

---

## Esta sesión (2026-04-24)

### MCP D-1 / D-2 / D-3 — COMPLETAS

- **D-1** (`src/llm/mcp/config.rs`): `McpConfig` loader — parsea `~/.config/petruterm/mcp/mcp.json` + merge con `.petruterm/mcp.json` local. Commit `062f7c1`.
- **D-2** (`src/llm/mcp/client.rs`, `manager.rs`): `McpClient` con JSON-RPC 2.0 sobre stdio, `initialize` handshake, `tools/list`, `tools/call`. `McpManager` con `start_all()` concurrente, `all_tools_openai()`, `call_tool()` por routing. Commit `e6f911f`.
- **D-3** (`src/app/ui.rs`, `src/llm/tools.rs`): `mcp_manager: Arc<McpManager>` en `UiManager`; tools MCP mergeados en tool_specs; dispatch loop: builtin → MCP fallback. Commit `1277677`.
- **40 tests pasando.**

### Análisis y fix de consumo de batería

- **Diagnóstico:** cursor blink 530ms (mayor impacto), `gpu_preference` hardcodeado como `HighPerformance` ignorando config, doc comment incorrecto (30s vs 60s TTL).
- **`gpu_preference` wired** (`src/config/schema.rs`, `src/config/lua.rs`, `src/renderer/gpu.rs`): enum `GpuPreference { LowPower, HighPerformance, None }` añadido al schema y parser Lua. `GpuRenderer::new()` usa la preferencia configurada al seleccionar el adaptador.
- **Present mode automático en batería** (`src/app/mod.rs`): cuando `battery_saver_active` cambia, se llama `renderer.set_present_mode(Fifo)` en batería y `Mailbox`/`FifoRelaxed` en AC. Efecto inmediato, sin reinicio.
- **`available_present_modes`** almacenado en `GpuRenderer` para consulta en runtime.
- **Default cambiado** a `"low_power"` en `config/default/perf.lua`.
- **Doc fix**: comment de `battery_saver` en `perf.lua` corregido (30s → 60s, añadido "switches present mode to Fifo").
- Commit `d27d272`.

---

## Esta sesión (2026-04-24 — battery saver previo)

### Battery saver mode

- `ControlFlow::Poll` → `ControlFlow::Wait` en `main.rs` (eliminado busy-loop inicial).
- `src/platform/battery.rs`: IOKit FFI via `IOPSCopyPowerSourcesInfo` — sin dependencias nuevas. Consulta cada 30s en `about_to_wait`.
- `config.battery_saver`: enum `Auto|Always|Never` en `schema.rs`, parseado desde Lua.
- Restricciones en modo batería (`Auto` + desconectado):
  - `git_dirty_check` forzado a `false` (elimina `git status --porcelain`)
  - Git poll TTL: 5s → 60s
  - Cursor blink: desactivado (solid cursor)
  - Present mode: Mailbox → Fifo
- Status bar: segmento `BAT XX%` (verde / rojo < 20%) visible solo en batería.

### Focus border — left-edge pane overlap fix (v0.1.3)

- **Fix:** cuando `col_offset == 0`, borde izquierdo desplazado un `cell_w` hacia la izquierda.
- **Archivo:** `src/app/renderer.rs` → `build_focus_border`

---

## Sesiones anteriores (resumen)

### 2026-04-23 tarde — Focus border + sidebar pills
- `pane_rect` snapping; shader ring mode; focus border como `RoundedRectInstance`.
- Sidebar items activos con pill `RoundedRectInstance`.
- Auditoría técnica: TD-MEM-26 resuelto, falsos positivos cerrados.

### 2026-04-23 mañana — Fase E + D-4 bugs
- Design refactor branch: paleta oscura, tabs flat, palette corners, AI panel.
- Skills D-4 bugs: /skills plural, YAML block scalar, explicit name match, assets inlineados, skill persiste, chat panel workspace-level, copy/paste.

### 2026-04-22 mañana — Fase C-3.5 + D-4 planificación
- Botones sidebar + AI en titlebar; header AI panel restyled.

### 2026-04-21 — Fase C-1 bugs + C-2 + C-3
- BTN_COLOR fix, padding.top fix, Workspace model en Mux, Sidebar izquierdo drawer.

### 2026-04-20 — Fase C-1 inicial + Fase B cerrada
- Unified titlebar (traffic lights + buttons + tab pills), AppMenu con muda.

### 2026-04-19 — Fase A + Fase 3.6
- v0.1.0 publicado, GitHub Copilot provider.
