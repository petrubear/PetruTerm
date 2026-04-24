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
**Status: COMPLETA — 2026-04-21**

- [x] `Workspace { id: usize, name: String }` en `src/app/mux.rs` (swap trick: tabs/panes como campos directos)
- [x] `Mux`: `workspaces: Vec<Workspace>` + `active_workspace_id` + `inactive_workspaces`
- [x] Workspace create / rename / close / switch / next / prev
- [x] Leader keybinds: `W n` (nuevo), `W &` (cerrar), `W ,` (renombrar), `W j/k` (navegar)
- [x] Workspace rename prompt (mismo flujo que tab rename)
- [x] Palette entries con keybind hints

### C-3: Sidebar de Workspaces
**Status: COMPLETA — 2026-04-21**

- [x] Panel lateral izquierdo tipo drawer (slide-in/out)
- [x] Lista workspaces con dot indicador del activo
- [x] Navegación: `j/k` mover, `Enter` activar, `c` crear, `&` cerrar, `r` renombrar inline, `Esc` cerrar
- [x] Subtítulo: `N tabs · M panes`; colores Dracula Pro

### C-3.5: AI panel como right sidebar + iconos en titlebar
**Status: COMPLETA — 2026-04-22**

- [x] Tercer botón en titlebar para toggle del AI panel (logical [106..128])
- [x] Layout button desplazado a [132..154]; tabs start en 158; hit_test_tab_bar actualizado
- [x] Iconos en los 2 botones: `≡` workspaces, `✦` AI (⊞ layout eliminado — sin handler)
- [x] Botones tintan purple cuando su panel está abierto; iconos lit/dim según estado
- [x] Header del AI panel restyled con SIDEBAR_BG + accent, formato ` ✦ AI  provider:model`
- [x] Click handler para el botón AI (toggle open/close)

---

## Fase D: AI Chat — MCP + Skills
**Status: D-1/D-2/D-3/D-4 COMPLETAS — D-5 pendiente**

### D-1: MCP config loader — COMPLETA 2026-04-24
- [x] `~/.config/petruterm/mcp/mcp.json` (formato: `{ "mcpServers": { "name": { "command", "args", "env" } } }`)
- [x] Merge con `.petruterm/mcp.json` del proyecto (proyecto tiene prioridad)
- [x] XDG fallback: también verifica `~/.config/petruterm/mcp/mcp.json` en macOS (donde `dirs::config_dir()` → `~/Library/Application Support`)

### D-2: MCP client (stdio transport) — COMPLETA 2026-04-24
- [x] Spawn proceso por server, JSON-RPC 2.0 sobre stdin/stdout
- [x] `initialize`, `tools/list`, `tools/call`
- [x] Lifecycle: spawn al arrancar app (`start_all()`), `kill_on_drop(true)` al cerrar
- [x] PATH augmentado con `/opt/homebrew/bin:/usr/local/bin` al spawnar (fix nvm lazy-load)
- [x] stderr forwarded al proceso padre (`Stdio::inherit()`) para debugging

### D-3: MCP tool integration en chat — COMPLETA 2026-04-24
- [x] LLM engine recibe tool list de MCP servers activos (`all_tools_openai()`)
- [x] Rutear tool calls al server correcto (`tool_routes` HashMap)
- [x] Dispatch en `submit_ai_query`: MCP tools PRIMERO, built-ins filtrados (no duplicar cobertura)
- [x] `AgentTool::specs_excluding(mcp_names)` — excluye built-ins cubiertos por MCP
- [x] Status lines: `✓ filesystem.list_directory(/tmp)` (server.tool() format)
- [x] Header badge `[mcp:N skills:M]` en AI panel

### D-4: Skills loader (formato agentskills.io)
**Status: COMPLETA — 2026-04-22**

#### Decisiones de diseño (2026-04-22)
- **Activación automática por relevancia** (NO por `/skill-name` explícito — no es el estándar).
  El usuario escribe su query normalmente; el sistema hace fuzzy match contra descriptions.
- **Progressive disclosure** fiel al estándar agentskills.io:
  1. Discovery: cargar solo `name`+`description` al startup
  2. Activation: si score fuzzy > 50 → leer body completo
  3. Execution: inyectar body al system prompt
- **Sin nuevas dependencias**: frontmatter parseado manualmente; fuzzy via `SkimMatcherV2` (ya en codebase)
- **Thin slash dispatcher** incluido en D4: refactorizar `/q`/`/quit` hardcodeados en un dispatcher
  extensible; añadir `/skill [filtro]` como primer comando real (lista skills disponibles)

#### Formato de Skill
```
~/.config/petruterm/skills/<name>/SKILL.md   ← global
.petruterm/skills/<name>/SKILL.md            ← project-local (prioridad sobre global)
```
```markdown
---
name: git-helper
description: Git expert for branches, commits, conflicts and repository workflows
---
Body del prompt aquí...
```

#### Cambios por archivo
| Archivo | Qué |
|---|---|
| `src/llm/skills.rs` *(nuevo)* | `SkillMeta{name,description,path}`, `SkillManager`: `load(cwd)`, `reload_local(cwd)`, `match_query→Option<&SkillMeta>` (SkimMatcherV2 threshold 50), `read_body→Result<String>` |
| `src/llm/mod.rs` | `pub mod skills;` |
| `src/llm/chat_panel.rs` | `pub matched_skill: Option<String>` en `ChatPanel`; limpiar en `Done`/`Error` |
| `src/app/ui.rs` | `skill_manager: SkillManager` en `UiManager`; en `submit_ai_query()`: match→inject body en system_text, setear `matched_skill`; método `handle_slash_command(cmd, args)`; limpiar `matched_skill` en `AiEvent::Done/Error` |
| `src/app/input/mod.rs` | Enter handler: si `input.starts_with('/')` → `ui.handle_slash_command(cmd, args)`; migrar `/q`/`/quit` al dispatcher |
| `src/app/renderer.rs` | Header AI panel: si `matched_skill` es `Some(name)` → mostrar `⚡ name` junto a provider:model |

#### Archivos modificados
- `src/llm/skills.rs` *(nuevo)* — `SkillMeta`, `SkillManager`
- `src/llm/mod.rs` — `pub mod skills`
- `src/llm/chat_panel.rs` — `matched_skill: Option<String>`
- `src/app/ui.rs` — `skill_manager`, `handle_slash_command`, skill injection en `submit_ai_query`
- `src/app/input/mod.rs` — slash dispatcher en Enter handler
- `src/app/renderer.rs` — `⚡ skill-name` en header AI panel

### D-5: Project-level config
- [x] `.petruterm/mcp.json` — MCP servers del proyecto (implementado en D-1: `load(cwd)` merge)
- [x] `.petruterm/skills/` — Skills del proyecto (implementado en D-4 como `reload_local`)
- [ ] Hot-reload de `.petruterm/mcp.json` al cambiar (actualmente solo se lee al arrancar)

---

## Fase E: Design Refactor — Visual Overhaul
**Branch:** `design-refactor`
**Status: In progress — 2026-04-23**

Objetivo: adoptar el estilo visual IDE moderno de la imagen de referencia. Solo cambios visuales/esteticos, sin nueva funcionalidad.

### Paleta de colores target
| Token | Hex | Uso |
|---|---|---|
| `BG_DEEP` | `#0e0e10` | Terminal area, fondo principal |
| `BG_PANEL` | `#131316` | Sidebar, AI panel |
| `BG_STATUS` | `#0a0a0c` | Status bar |
| `BORDER` | `#2a2a2f` | Divisores, bordes de overlays |
| `TEXT` | `#e0e0e8` | Texto principal |
| `TEXT_MUTED` | `#6b6b7a` | Labels, subtitulos |
| `ACCENT_TEAL` | `#4ec9b0` | Path en status bar |
| `ACCENT_AMBER` | `#d4a44c` | Branch git, elementos activos |

### Tareas

- [x] **T1** — Paleta de colores base: constantes en `renderer.rs` y temas
- [x] **T2** — Tab bar flat `zsh: N`: reemplazar pills SDF por tabs flat con nombre de proceso
- [x] **T3** — Command palette overlay: esquinas redondeadas (~8px), borde `#2a2a2f`, fondo `#131316`
- [x] **T4** — Sidebar + AI panel: `BG_PANEL` en ambos, header AI panel con borde separador
- [x] **T5** — Divisores de pane: 1px logico, `#2a2a2f`
- [x] **T6** — Status bar: nuevos colores `ACCENT_TEAL`/`ACCENT_AMBER`/`BG_STATUS`
- [x] **T7** — Markdown en AI panel: `md_style_line()` — headers coloreados, bullets con `•`, code en verde

### Notas de implementacion
- T1 desbloquea T2–T6; hacerla primero
- T7 es independiente, puede ir en paralelo con T2–T6
- No se requiere blur gaussiano real — el command palette usa panel solido oscuro con borde
- cosmic-text soporta font size variable por text run (necesario para T7)

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
