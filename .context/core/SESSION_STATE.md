# Session State

**Last Updated:** 2026-04-24
**Session Focus:** MCP D1/D2/D3 + battery/GPU optimizations + MCP path fix + UI polish

## Branch: `master`

## Estado actual

**Phase 1–3 + 3.5 COMPLETE. Fase A COMPLETE. Fase 3.6 COMPLETE. Fase B COMPLETE. Fase C-1 COMPLETE. Fase C-2 COMPLETE. Fase C-3 COMPLETE. Fase C-3.5 COMPLETE. Fase D-1/D-2/D-3/D-4 COMPLETE. v0.1.3 publicado.**

**MCP end-to-end operativo y verificado.** Battery saver con GPU preference wired.

---

## Esta sesión (2026-04-24) — MCP debugging + UI polish

### MCP path fix (macOS)
- **Bug:** `dirs::config_dir()` en macOS → `~/Library/Application Support`, no `~/.config`. El config loader solo miraba el path de la plataforma.
- **Fix:** `src/llm/mcp/config.rs` — `load()` ahora verifica también `~/.config/petruterm/mcp/mcp.json` como XDG fallback (solo si es distinto al platform path). Commit `641fed3`.
- **Log:** Añadido `log::info!` en `manager.rs` al conectar cada servidor con lista de tools.

### MCP spawn: PATH y stderr
- **Bug:** `npx` no se encontraba en entornos con nvm lazy-loaded en zsh (PATH mínimo heredado).
- **Fix:** `src/llm/mcp/client.rs` — PATH augmentado con `/opt/homebrew/bin:/usr/local/bin` al spawnar. stderr cambiado de `Stdio::null()` a `Stdio::inherit()` para debugging. Commit `0eb5f97`.

### Header badge `[mcp:N skills:M]`
- **Cambio:** `src/llm/chat_panel.rs` — campos `mcp_connected: usize` + `skill_count: usize` en `ChatPanel`.
- **Wire:** `src/app/ui.rs` — setea los campos tras `start_all()` y `skill_manager.load()`. Nuevos métodos: `McpManager::connected_count()`, usa `skill_manager.skills().len()`.
- **Render:** `src/app/renderer.rs` — header muestra badge dinámico `[mcp:N skills:M]` (oculto si ambos 0). Commit `0eb5f97`.

### MCP tool priority over built-ins
- **Bug:** LLM elegía `list_dir` (built-in, restringido al CWD) en lugar de `list_directory` (MCP) porque los built-ins iban primero en la lista.
- **Fix:** `src/llm/tools.rs` — nuevo método `AgentTool::specs_excluding(mcp_tool_names)`: excluye built-ins cubiertos por MCP (exact name match o `list_dir` cuando MCP tiene tool con "list"/"director"). `src/app/ui.rs` — MCP tools van PRIMERO, built-ins filtrados después. Commit `a08f357`.

### MCP tool status display: `server.tool()`
- **Cambio:** `src/app/ui.rs` — en el branch MCP del dispatch, `display_name = format!("{}.{}", server, call.name)` usando `McpManager::server_for_tool()`.
- **Nuevo método:** `McpManager::server_for_tool(name) -> Option<&str>` en `manager.rs`.
- Resultado: `✓ filesystem.list_directory(/tmp)` en lugar de `✓ list_directory()`. Commits `d7fa302`, `62c0771`.

### MCP /tmp symlink fix (user config)
- `/tmp` en macOS es symlink → `/private/tmp`. El MCP filesystem server resuelve el path al arrancar y almacena `/private/tmp`, pero las requests con `/tmp` fallaban el check de acceso.
- Config del usuario actualizada: `~/.config/petruterm/mcp/mcp.json` ahora usa `/private/tmp` y `/Users/edison`.

---

## Esta sesión (2026-04-24) — GPU/Battery

### MCP D-1 / D-2 / D-3 — COMPLETAS

- **D-1** (`src/llm/mcp/config.rs`): `McpConfig` loader. Commit `062f7c1`.
- **D-2** (`src/llm/mcp/client.rs`, `manager.rs`): `McpClient` JSON-RPC 2.0 stdio. Commit `e6f911f`.
- **D-3** (`src/app/ui.rs`, `src/llm/tools.rs`): integración en chat. Commit `1277677`.

### Battery/GPU fix

- `gpu_preference` wired desde config → `GpuRenderer::new()` (`schema.rs`, `lua.rs`, `gpu.rs`).
- Present mode automático en batería: Fifo en batería, Mailbox/FifoRelaxed en AC.
- Fix crash: `FifoRelaxed` verificado contra `available_present_modes` antes de usarlo.
- Default `"low_power"` en `perf.lua`. Commits `d27d272`, `8c3d211`.

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
