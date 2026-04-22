# PetruTerm — Build Phases

> Phases 0.5–3 + Phase 3.5 sub-phases A–H archived in [`build_phases_archive.md`](./build_phases_archive.md).

---

## Fase 3.6: GitHub Copilot Provider — COMPLETA
**Cerrado:** 2026-04-19

- [x] `src/llm/copilot.rs`: `CopilotProvider` con JWT cache + auto-refresh
- [x] Auth: `GITHUB_TOKEN` env → `gh auth token` CLI → Keychain (`GITHUB_COPILOT_OAUTH_TOKEN`)
- [x] SSE helpers extraidos a `mod.rs` (`parse_sse_chunk`, `parse_agent_response`) — eliminada duplicacion entre openrouter/openai_compat
- [x] `build_provider()` wired: `"copilot"` match arm
- [x] `config/default/llm.lua` documentado con ejemplos de modelos y setup

---

## Phase 3.5: Performance Sprint ⚡ — COMPLETA
**Cerrado:** 2026-04-19 | **Archivado:** `build_phases_archive.md`

### Exit Criteria
- [x] Debug HUD (F12) operativo
- [x] `PROFILING.md` documentado
- [x] Damage tracking con alacritty_terminal
- [x] Cursor overlay fast path
- [x] Idle zero-cost (ControlFlow::Wait + focus guard)
- [x] P2/P3 tech debt resuelto (TD-PERF-19, TD-MEM-09 diferido a P2 backlog)
- [x] Bench CI gating activo (search + shaping; build_instances/rasterize diferidos por winit/wgpu acoplamiento)

### Diferidos al backlog (no bloquean Fase A)
- `build_instances` bench: requiere extraer CPU path sin winit
- `rasterize_to_atlas` bench: requiere variant swash-only sin wgpu::Queue

---

## Fase A: Fundación — Versionado + i18n
**Status: COMPLETA — 2026-04-19**

- [x] `Cargo.toml` ya en `0.1.0`; `CHANGELOG.md` creado
- [x] `rust-i18n` 3.1; detección via `LANG`/`LC_ALL`/`LC_MESSAGES`
- [x] `locales/en.toml` + `locales/es.toml` (35 strings)
- [x] Status bar, errores LLM, panel AI, paleta de comandos
- [x] Release workflow (`release.yml`) — tag `v*` → build → bundle → GitHub Release
- [x] README: nota `xattr` para Gatekeeper
- [x] Tag `v0.1.0` publicado

---

## Fase B: Menu Bar nativo macOS
**Status: COMPLETA — 2026-04-20**

- [x] Crate `muda`; inicializar `MenuBar` en `main.rs` antes del event loop
- [x] **File**: Settings (abre config folder), Reload Config
- [x] **View**: Toggle Status Bar, Switch Theme, Toggle Fullscreen
- [x] **AI**: Toggle Panel, Explain, Fix Error, Undo Write, Enable/Disable
- [x] **Window**: macOS predefined + Tab submenu + Pane submenu
- [x] Wiring de acciones vía `MenuEvent` (drain en `about_to_wait`)

---

## Fase C: Titlebar Custom + Workspaces

### C-1: Titlebar custom (NSWindow híbrido)
**Status: COMPLETA — 2026-04-21**

- [x] `TITLEBAR_HEIGHT = 30.0`; `tab_bar_visible()` always true en Custom mode
- [x] Tab pills (SDF rounded rect) solo cuando tabs > 1; drag region sobre zona vacía
- [x] Botones sidebar/layout en titlebar; hit-test unificado para clicks en zona y < TITLEBAR_HEIGHT * sf
- [x] BTN_COLOR: Dracula Current Line [0.267, 0.278, 0.353, 1.0] (era invisible)
- [x] `padding.top = 5` en config de usuario (era 60, leftover de antes del custom titlebar)

### C-2: Modelo Workspace en Mux
- [ ] `Workspace { id: usize, name: String, tabs: Vec<TabId> }` en `src/app/mux.rs`
- [ ] `Mux`: `workspaces: Vec<Workspace>` + `active_workspace_id` (en lugar de `tabs: Vec<Tab>` directo)
- [ ] Workspace create / rename / close
- [ ] Leader keybinds: `W n` (nuevo), `W &` (cerrar), `W ,` (renombrar), `W j/k` (navegar)

### C-3: Sidebar de Workspaces
- [ ] Panel lateral izquierdo tipo drawer (slide-in/out)
- [ ] Lista workspaces con dot indicador del activo
- [ ] Navegación: `j/k` mover, `Enter` activar, `c` crear, `&` cerrar, `r` renombrar inline, `Esc` cerrar

---

## Fase D: AI Chat — MCP + Skills
**Status: Not started**

### D-1: MCP config loader
- [ ] `~/.config/petruterm/mcp/mcp.json` (formato: `{ "mcpServers": { "name": { "command", "args", "env" } } }`)
- [ ] Merge con `.petruterm/mcp.json` del proyecto (proyecto tiene prioridad)

### D-2: MCP client (stdio transport)
- [ ] Spawn proceso por server, JSON-RPC 2.0 sobre stdin/stdout
- [ ] `initialize`, `tools/list`, `tools/call`, `resources/list`, `resources/read`
- [ ] Lifecycle: spawn al abrir AI panel, kill al cerrar sesión

### D-3: MCP tool integration en chat
- [ ] LLM engine recibe tool list de MCP servers activos
- [ ] Rutear tool calls al server correcto
- [ ] Mostrar tool calls en panel AI (collapsible)

### D-4: Skills loader (formato agentskills.io)
- [ ] Escanear `~/.config/petruterm/skills/*/SKILL.md` al inicio (+ `.petruterm/skills/` local)
- [ ] Parsear frontmatter YAML: `name`, `description`; body cargado solo al activar
- [ ] Activación: `/skill-name` en input, o por relevancia de descripción vs query
- [ ] Inyectar body del skill activo al system prompt

### D-5: Project-level config
- [ ] `.petruterm/mcp.json` — MCP servers del proyecto
- [ ] `.petruterm/skills/` — Skills del proyecto
- [ ] Merge con global: proyecto tiene prioridad en conflictos de nombre

---

## Phase 4: Plugin Ecosystem
**Status: Not started — después de Fases A–D**

- [ ] Plugin loader: auto-scan `~/.config/petruterm/plugins/*.lua`
- [ ] lazy.nvim-style plugin spec
- [ ] Plugin Lua API: `petruterm.palette.register()`, `petruterm.on()`, `petruterm.notify()`
- [ ] Plugin event system: `tab_created`, `tab_closed`, `pane_split`, `ai_response`, `command_run`
- [ ] `petruterm.plugins.install("user/repo")`
- [ ] Plugin hot-reload
- [ ] Example plugin + documentation
