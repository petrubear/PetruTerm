# Session State

**Last Updated:** 2026-04-23
**Session Focus:** Fase E — Design Refactor (branch `design-refactor`)

## Branch: `design-refactor` (base: `master`)

## Estado actual

**Phase 1–3 + 3.5 COMPLETE. Fase A COMPLETE. Fase 3.6 COMPLETE. Fase B COMPLETE. Fase C-1 COMPLETE. Fase C-2 COMPLETE. Fase C-3 COMPLETE. Fase C-3.5 COMPLETE. Fase D-4 COMPLETE + bugs fixed.**
**Fase E: Design Refactor — IN PROGRESS (branch design-refactor).**
**Pendiente en master: Fase D-1/D-2/D-3 (MCP), Fase D-5 (project-level config).**

---

## Esta sesión (2026-04-23)

### Fase E — Design Refactor — COMPLETA
Branch `design-refactor`. Todos los cambios son visuales, compilacion limpia.
T1: paleta oscura (#0e0e10, #131316, #2a2a2f, teal, amber). T2: tabs flat "title: N" con underline amber. T3: palette con rounded corners (8px) + border rect. T4: AI panel bg #131316, sidebar sep color. T5: divisores 1px #2a2a2f. T6: status bar teal/amber. T7: md_style_line() en respuestas AI (headers/bullets/code).

### D-4 Skills — bug fixes post-launch

1. **`/skill` → `/skills`**: comando renombrado (plural). Removed alias singular.
2. **YAML block scalar**: `parse_frontmatter` ahora parsea `description: >` multilinea; antes quedaba como `">"` y fuzzy match nunca activaba.
3. **Explicit name match**: `match_query` primero busca el nombre exacto del skill en el mensaje antes de fuzzy-scoring.
4. **Assets inlineados**: `read_body` llama `collect_skill_files(skill_dir)` — recorre recursivamente `references/`, `assets/`, `scripts/` y cualquier otro directorio, appending contenido verbatim al body.
5. **Skill persiste en conversación**: `matched_skill` ya NO se limpia en `mark_done`/`mark_error`. `submit_ai_query` reutiliza el skill activo del panel si el query nuevo no matchea ninguno.
6. **Chat panel workspace-level**: `HashMap<usize, ChatPanel>` → un solo `chat_panel: ChatPanel`. `set_active_terminal()` es no-op. El panel es visible en todos los panes del workspace.
7. **Copy/paste en chat**: Cmd+V pega clipboard en el input; Cmd+C copia el input actual. Handler agregado antes del bloque `!cmd`.
8. **Skill prompt**: system prompt indica explícitamente que los archivos referenciados ya están inlineados — no usar file tools para leerlos.

### Skill format (agentskills.io standard)

```
~/.config/petruterm/skills/<name>/SKILL.md   ← required (frontmatter + instructions)
~/.config/petruterm/skills/<name>/references/ ← guides loaded inline
~/.config/petruterm/skills/<name>/assets/     ← templates loaded inline
~/.config/petruterm/skills/<name>/scripts/    ← scripts loaded inline
.petruterm/skills/<name>/SKILL.md            ← project-local (prioridad sobre global)
```

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
