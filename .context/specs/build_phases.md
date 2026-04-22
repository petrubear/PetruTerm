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
**Status: En planificación — listo para implementar**

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

#### Todos (en orden de ejecución)
```
d4-skills-rs ──┬──► d4-mod-rs ──► d4-ui-rs ──────┐
               │                                    ├──► d4-commit
               └──► d4-slash ──────────────────────┤
d4-chat-panel ─┬──► d4-ui-rs                       │
               └──► d4-renderer ───────────────────┘
```
- `d4-skills-rs` — crear `src/llm/skills.rs`
- `d4-chat-panel` — `matched_skill` en `ChatPanel`
- `d4-mod-rs` — registrar módulo (dep: d4-skills-rs)
- `d4-slash` — thin dispatcher en input/mod.rs + handle_slash_command en ui.rs (dep: d4-skills-rs)
- `d4-ui-rs` — integrar SkillManager + injection (dep: d4-chat-panel, d4-mod-rs)
- `d4-renderer` — indicador visual (dep: d4-chat-panel)
- `d4-commit` — commit + actualizar SESSION_STATE/build_phases (dep: todos los anteriores)

### D-5: Project-level config
- [ ] `.petruterm/mcp.json` — MCP servers del proyecto
- [ ] `.petruterm/skills/` — Skills del proyecto (ya contemplado en D-4 como `reload_local`)
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
