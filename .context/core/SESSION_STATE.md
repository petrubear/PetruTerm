# Session State

**Last Updated:** 2026-04-22
**Session Focus:** Fase D-4 Skills loader — COMPLETA.

## Branch: `master`

## Estado actual

**Phase 1–3 + 3.5 COMPLETE. Fase A COMPLETE. Fase 3.6 COMPLETE. Fase B COMPLETE. Fase C-1 COMPLETE. Fase C-2 COMPLETE. Fase C-3 COMPLETE. Fase C-3.5 COMPLETE. Fase D-4 COMPLETE.**
**Siguiente: Fase D-1/D-2/D-3 (MCP) o Fase D-5 (project-level config).**

---

## Esta sesión (2026-04-22)

### D-4 Skills loader — COMPLETA

Todos los todos implementados y commiteados:

- `d4-skills-rs` — `src/llm/skills.rs` creado: `SkillMeta`, `SkillManager` con `load/reload_local/match_query/read_body/skills`. Frontmatter parseado manualmente, fuzzy via `SkimMatcherV2` (threshold 50), sin deps nuevas.
- `d4-chat-panel` — `matched_skill: Option<String>` en `ChatPanel`, limpiado en `mark_done`/`mark_error`.
- `d4-mod-rs` — `pub mod skills` registrado en `llm/mod.rs`.
- `d4-slash` — thin dispatcher en `input/mod.rs`: Enter con `/` prefix → `ui.handle_slash_command`; `/q`/`/quit` migrados al dispatcher.
- `d4-ui-rs` — `skill_manager: SkillManager` en `UiManager`; `submit_ai_query` inyecta skill body en system prompt y setea `matched_skill`; `handle_slash_command` implementado con `/q`, `/skill [filter]`, y fallback de error.
- `d4-renderer` — Header AI panel muestra `⚡ skill-name` cuando skill está activo.

### Skill format

```
~/.config/petruterm/skills/<name>/SKILL.md   ← global
.petruterm/skills/<name>/SKILL.md            ← project-local (prioridad sobre global)
```

### Lo hecho esta sesión (D-4)

1. `src/llm/skills.rs` (nuevo)
2. `src/llm/mod.rs` — pub mod skills
3. `src/llm/chat_panel.rs` — matched_skill field
4. `src/app/ui.rs` — SkillManager + handle_slash_command + skill injection
5. `src/app/input/mod.rs` — slash dispatcher
6. `src/app/renderer.rs` — ⚡ indicator en header

---

## Sesiones anteriores (resumen)

### 2026-04-22 mañana — Fase C-3.5 + D-4 planificación
- Botones sidebar + AI en titlebar; header AI panel restyled
- Diseño D-4 cerrado

### 2026-04-21 — Fase C-1 bugs + C-2 + C-3
- BTN_COLOR fix, padding.top fix
- Workspace model en Mux
- Sidebar izquierdo drawer (workspaces)

### 2026-04-20 — Fase C-1 inicial + Fase B cerrada
- Unified titlebar committeado (59097cd): traffic lights + buttons + tab pills
- Fase B: AppMenu con muda, menus File/View/AI/Window

### 2026-04-19 — Fase A + Fase 3.6
- v0.1.0 publicado, GitHub Copilot provider
