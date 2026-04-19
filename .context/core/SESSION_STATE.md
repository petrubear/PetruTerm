# Session State

**Last Updated:** 2026-04-18 (sesión planning)
**Session Focus:** Roadmap review + planning — sprint cierre Phase 3.5, luego Fases A–D (menu, titlebar, workspaces, MCP/Skills)

## Branch: `master`

## Estado actual

**Phase 1–3 COMPLETE. Phase 3.5: sub-phases A–H completas (archivadas). Sprint cierre pendiente.**
**Build limpio: check + test + clippy + fmt PASS. CI verde.**

## Commits recientes relevantes

| Commit | Descripción |
|--------|-------------|
| `b2da6ac` | fix: tab blank on switch, LLM keychain, AI panel focus keybind |
| `a5d691e` | fix: Shift+Enter KKP, tab bleed, clippy, .app env vars |

---

## Roadmap acordado (sesión 2026-04-18 planning)

### Sprint cierre Phase 3.5 (PRÓXIMO)
Resolver deuda técnica antes de implementar nuevas features. Ver `build_phases.md` Sprint Cierre.

**P2 prioritarios:**
- ~~TD-MEM-23~~ RESUELTO (ya era `&[Value]` en el código actual)
- TD-MEM-13: Limitar `ReadFile` a 50k chars + max 5 rounds en agent loop
- TD-PERF-04: `scan_files()` sincrónico en file picker → `spawn_blocking`
- TD-PERF-15: Clipboard bloquea event loop → `spawn_blocking` en copy/paste grande
- TD-PERF-21: Palette fuzzy sin caché incremental → filtrado incremental

**P3 triviales (de paso):**
- TD-MEM-17: `streaming_buf.clear()` en `close()`
- TD-MEM-24: `VecDeque` para `undo_stack`
- TD-PERF-18: Tokio pool → `worker_threads(2)`
- TD-PERF-23: `leader_deadline: Instant` en lugar de `elapsed()` por keystroke

**Benchmarks:**
- Desbloquear `build_instances` bench (extraer CPU path a fn pura)
- Desbloquear `rasterize_to_atlas` bench (variant swash-only sin wgpu)
- CI gating: regresión >5% falla build

**Descartado de Phase 3.5:**
- Sub-E (rayon/rtrb), Sub-G (atlas split/ring buffer), Sub-H (PGO) → backlog Phase 2
- CVDisplayLink / CAMetalLayer → skip
- "Zero allocs con dhat" y comparativa vs Alacritty → diferir

---

### Fase A — Fundación (versionado + i18n)
- Bump `Cargo.toml` a `0.1.0`, crear `CHANGELOG.md`
- Crate i18n (`rust-i18n`), detección locale macOS, archivos `en.toml` + `es.toml`
- Scope: menu labels, mensajes de error LLM, panel AI, status bar labels

### Fase B — Menu Bar nativo macOS
- Crate `muda`, inicializar antes del event loop
- Menus: File, Edit, AI Chat, Window, Help (ver `build_phases.md` Fase B)
- "About" muestra `env!("CARGO_PKG_VERSION")`
- Labels via i18n

### Fase C — Titlebar custom + Workspaces
- Titlebar via `objc2` NSWindow híbrido (traffic lights nativos conservados)
- Modelo `Workspace { id, name, tabs }` en Mux
- Sidebar izquierda (drawer) con lista de workspaces, nav j/k
- Leader keybinds: `W n/&/,/j/k`

### Fase D — AI Chat MCP + Skills
- MCP config: `~/.config/petruterm/mcp/mcp.json` + `.petruterm/mcp.json` (proyecto)
- MCP client: JSON-RPC stdio, tools/list + tools/call
- Skills: `~/.config/petruterm/skills/*/SKILL.md` (formato agentskills.io) + `.petruterm/skills/`
- Activación bajo demanda; skills inyectados al system prompt

### Fase 4 — Plugin Ecosystem (después de A–D)
- lazy.nvim-style plugin loader en Lua
- `src/plugins/` — plugin loader + Lua API
- Ver `build_phases.md` Phase 4 para deliverables

---

## Sesiones anteriores (resumen)

### 2026-04-18 (tarde) — Bug fixes prioritarios
- KKP Shift+Enter, tab bleed, CI clippy, .app env vars

### 2026-04-18 (mañana) — Tier 3 + Tier 0
- TD-MEM-19, cursor overlay fast path, damage tracking, latency HUD, CI setup, TD-OP-02

### 2026-04-17 — Tier 1 + Tier 2 + Tier 4
- TD-MEM-20/21/12/10/11, TD-PERF-37/22/34/31/32/33/20/17

### 2026-04-16 — TD-RENDER-01 real fix + TD-RENDER-03
- Pre-pass bg-only vertices, mouse_dragged + clear_selection

### 2026-04-15 — Phase 3.5 Memory + Performance sprint
- TD-MEM-01..08, TD-PERF-06..13
