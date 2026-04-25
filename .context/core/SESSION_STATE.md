# Session State

**Last Updated:** 2026-04-25
**Session Focus:** Phase 5 G-0 — UI tokens en ColorScheme

## Branch: `master`

## Estado actual

**Phase 1–3 + 3.5 + A + 3.6 + B + C + D (todas las fases) COMPLETE. Phase 5 G-0 COMPLETE.**
**v0.1.3 publicado. Fase 4 (plugins) y Phase 5 (UX Polish) en paralelo. Deuda técnica: 2 items P3 abiertos (TD-MEM-30, TD-PERF-40).**

---

## Esta sesión (2026-04-25) — Phase 5 G-0: UI tokens

### G-0: Sistema de temas — UI tokens
- `src/config/schema.rs` — 7 nuevos campos en `ColorScheme`: `ui_accent`, `ui_surface`, `ui_surface_active`, `ui_surface_hover`, `ui_muted`, `ui_success`, `ui_overlay`. Todos con `#[serde(default)]` y sentinel `[0.0;4]`. Método `derive_ui_colors(&mut self)` calcula valores desde colores base cuando no se especifican.
- `src/config/lua.rs` — `table_to_color_scheme` lee los 7 tokens opcionales y llama `derive_ui_colors()`. `parse_hex_linear` extendido para soportar hex 8-char `#rrggbbaa`.
- `src/app/renderer.rs` — ~20+ literales hardcodeados reemplazados por `colors.ui_*` en: `build_focus_border`, `build_palette_instances`, `build_context_menu_instances`, `build_tab_bar_instances`, `build_scroll_bar_instances`, `build_search_bar_instances`, `build_status_bar_instances`, `build_workspace_sidebar_instances`, `build_toast_instances`, `build_chat_panel_instances`. Cada función recibió `colors: &ColorScheme`.
- `src/app/mod.rs` — 9 call sites actualizados para pasar `&self.config.colors`. Separador sidebar: `[0.165, 0.165, 0.184, 1.0]` → `ui_muted`.
- `assets/themes/` — Los 5 temas bundled actualizados con tokens explícitos coherentes con su paleta.
- `config/default/ui.lua` — Tokens documentados como campos comentados con descripción.
- `~/.config/petruterm/themes/` — Sincronizados con los bundled.

---

## Esta sesión (2026-04-24) — D-5 + Recomendaciones estratégicas

### D-5: MCP hot-reload
- `config/watcher.rs` — filtro extendido a `.json` además de `.lua`
- `app/mod.rs` — `mcp_watcher: Option<ConfigWatcher>` (notify sobre CWD `.petruterm/`) + `mcp_reload_at: Option<Instant>` (debounce 300ms separado). `check_config_reload` enruta eventos `.json` → `mcp_reload_at`, `.lua` → `config_reload_at`. Al disparar, llama `ui.reload_mcp(cwd)`.
- `app/ui.rs` — `reload_mcp(cwd)`: crea nuevo `McpManager`, `block_on(start_all())`, reemplaza `Arc`, actualiza `chat_panel.mcp_connected`.
- Cubre: global `~/Library/Application Support/petruterm/mcp/mcp.json` + project-local `.petruterm/mcp.json`.

### REC-PERF-01: ASCII warmup al arranque
- `font/shaper.rs` — `init_ascii_glyph_cache()` llamado eagerly en el constructor (antes era lazy en `try_ascii_fast_path`).
- `font/shaper.rs` — nuevo método `warmup_atlas(atlas, queue)`: pre-rasteriza los 95 glyphs ASCII imprimibles al atlas.
- `app/renderer.rs` — `warmup_atlas` llamado en `RenderContext::new()` tras `set_cell_size`, eliminando cache-misses en el primer frame.

### REC-PERF-02: parking_lot::Mutex
- `Cargo.toml` — `parking_lot = "0.12"` añadido.
- Reemplazado `std::sync::Mutex` en: `font/freetype_lcd.rs` (glyph cache), `font/loader.rs` (FONT_PATH_CACHE estático), `llm/copilot.rs` (JWT cache). Ningún lock se mantiene a través de `.await`.
- `.lock().unwrap()` → `.lock()` (parking_lot infallible).

### REC-PERF-05: Frame budget documentado
- `.context/specs/term_specs.md` §15 — tabla de targets (p99 < 8ms, idle 0 CPU/GPU, cold start < 16ms, atlas storm < 50ms), metodología HUD F12, referencia al CI criterion gate.

---

## Esta sesión (2026-04-24) — Slash commands + keybind fixes

### /skills color formatting
- **Bug:** `/skills` mostraba nombre y descripcion en la misma linea, dificil de leer como bloque.
- **Fix:** `src/app/ui.rs` `handle_slash_command` — formato cambiado a `## name\ndescription`. El renderer ya aplica `md_style_line` a mensajes de asistente: `##` → teal, `#` → purple, descripcion en color normal.
- **Resultado:** nombre del skill en teal, descripcion en fg normal — visualmente distintos.

### /mcp slash command (nuevo)
- **Nuevo:** `/mcp` en el input del panel AI lista todos los servidores MCP conectados agrupados por nombre, con conteo de herramientas y la lista de tool names.
- **Implementacion:** `src/app/ui.rs` — usa `McpManager::all_tools()` + `connected_count()`. Mismo esquema de color: `# MCP` en purple, `## server (N tools)` en teal.

### Leader+w para nuevo workspace
- **Bug:** crear workspace requeria `Leader+W+n` (3 teclas, Shift necesario). Ademas, `handle_sidebar_key` en `src/app/mod.rs` interceptaba `leader+a` cuando el sidebar estaba abierto y creaba un workspace, rompiendo `leader+a+a` para abrir el chat.
- **Fix 1:** `src/app/input/mod.rs` — `Leader+w` (lowercase, single key) dispara `Action::NewWorkspace` directamente antes del bloque de prefijos. `Leader+W+n` conservado como alias.
- **Fix 2:** `src/app/mod.rs` `handle_sidebar_key` — el interceptor `"a"` cambiado a `"w"`. `Leader+a` ya no es secuestrado por el sidebar; siempre entra al prefijo AI.
- **Docs:** `AGENTS.md` keybinds table actualizada con workspace shortcuts completos y AI sub-leader expandido (`a a`, `a e`, `a f`, `a z`). `/skills` y `/mcp` agregados como panel slash commands.

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
