# PetruTerm — Build Phases

> Phases 0.5–3 + Phase 3.5 sub-phases A–H archived in [`build_phases_archive.md`](./build_phases_archive.md).

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
**Status: Not started**

- [ ] Bump `Cargo.toml` a `0.1.0`; crear `CHANGELOG.md` con historial resumido
- [ ] Crate `rust-i18n`; detección de locale del sistema (macOS `NSLocale`)
- [ ] `locales/en.toml` + `locales/es.toml`
- [ ] Scope: menu labels, mensajes error LLM, panel AI, status bar labels

---

## Fase B: Menu Bar nativo macOS
**Status: Not started**

- [ ] Crate `muda`; inicializar `MenuBar` en `main.rs` antes del event loop
- [ ] **File**: New Tab, New Pane (H/V), Close Tab, Close Pane, Quit
- [ ] **Edit**: Copy, Paste, Clear Scrollback, Find
- [ ] **AI Chat**: Toggle Panel, Send to AI, Explain Last Output, Fix Last Error, Clear Chat
- [ ] **Window**: New Workspace, Next/Prev Workspace, Next/Prev Tab, Minimize, Zoom
- [ ] **Help**: About PetruTerm (`env!("CARGO_PKG_VERSION")`), Open Config Folder
- [ ] Wiring de acciones vía `MenuEvent`
- [ ] Labels via i18n (Fase A)

---

## Fase C: Titlebar Custom + Workspaces
**Status: Not started**

### C-1: Titlebar custom (NSWindow híbrido)
- [ ] `objc2`: `setTitlebarAppearsTransparent(true)`, `setTitleVisibility(.hidden)` — traffic lights nativos conservados
- [ ] Expandir área render wgpu para cubrir zona título
- [ ] Drag region via `NSWindow.setIsMovableByWindowBackground`
- [ ] Botón toggle sidebar en titlebar (junto a traffic lights)

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
