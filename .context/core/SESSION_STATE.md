# Session State

**Last Updated:** 2026-04-19
**Session Focus:** Sprint cierre Phase 3.5 COMPLETO — bugs CI + status bar flicker fix

## Branch: `master`

## Estado actual

**Phase 1–3 COMPLETE. Phase 3.5 COMPLETE (sprint cierre incluido).**
**Build limpio: check + test + clippy + fmt PASS. CI verde.**

## Commits recientes relevantes

| Commit | Descripción |
|--------|-------------|
| (fmt)  | chore: cargo fmt |
| (fix)  | fix: status bar flickers/disappears on mouse click |
| (ci)   | chore: fix clippy warnings breaking CI |
| (bench)| chore: fix bench compilation + add rasterize/build_instances to CI gating |
| (perf) | [TD-PERF-15] perf: async OSC 52 clipboard in poll_pty_events |

---

## Sprint cierre Phase 3.5 — CERRADO 2026-04-19

Todos los ítems P2/P3 revisados. La mayoría ya estaba implementado en código.
Único código nuevo: TD-PERF-15 (Pty.tx + async OSC 52).

**Deuda cerrada esta sesión:**
- TD-MEM-23, TD-MEM-13, TD-PERF-04, TD-PERF-21 — ya implementados
- TD-MEM-17, TD-MEM-24, TD-PERF-18, TD-PERF-23 — ya implementados
- TD-PERF-15 — resuelto con código nuevo
- Benches build_instances + rasterize — compilaban con firma vieja; migrados a TextShaperConfig
- CI bench gating — añadidos los 2 nuevos benches al workflow

**Bugs adicionales resueltos:**
- CI clippy: 8 warnings (collapsible_match x4, manual_repeat_n, unnecessary_sort_by x3)
- CI fmt: 4 archivos con formato incorrecto
- Rust version mismatch: local era Homebrew 1.94.1, migrado a rustup 1.95.0
- Status bar flicker/desaparece al hacer click: blink fast path usaba
  `cell_count = content_end + 1` cortando el draw antes del status bar.
  Fix: `last_overlay_start` en RenderContext; blink path usa `last_instance_count`
  + vertex transparente (bg.a=0) para cursor off.

---

## Próximo: Fase A — Fundación (versionado + i18n)

- Bump `Cargo.toml` a `0.1.0`, crear `CHANGELOG.md`
- Crate i18n (`rust-i18n`), detección locale macOS, archivos `en.toml` + `es.toml`
- Scope: menu labels, mensajes de error LLM, panel AI, status bar labels

### Fase B — Menu Bar nativo macOS
- Crate `muda`, inicializar antes del event loop
- Menus: File, Edit, AI Chat, Window, Help

### Fase C — Titlebar custom + Workspaces
- Titlebar via `objc2` NSWindow híbrido
- Modelo `Workspace { id, name, tabs }` en Mux
- Sidebar izquierda (drawer)

### Fase D — AI Chat MCP + Skills
- MCP config + client JSON-RPC stdio
- Skills agentskills.io format

### Fase 4 — Plugin Ecosystem (después de A–D)

---

## Sesiones anteriores (resumen)

### 2026-04-19 — Sprint cierre Phase 3.5
- Deuda P2/P3 cerrada, benches desbloqueados, CI verde, status bar flicker fix

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
