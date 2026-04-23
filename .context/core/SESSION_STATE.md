# Session State

**Last Updated:** 2026-04-23
**Session Focus:** D-4 bug fixes post-launch.

## Branch: `master`

## Estado actual

**Phase 1–3 + 3.5 COMPLETE. Fase A COMPLETE. Fase 3.6 COMPLETE. Fase B COMPLETE. Fase C-1 COMPLETE. Fase C-2 COMPLETE. Fase C-3 COMPLETE. Fase C-3.5 COMPLETE. Fase D-4 COMPLETE + bugs fixed.**
**Siguiente: Fase D-1/D-2/D-3 (MCP) o Fase D-5 (project-level config).**

---

## Esta sesión (2026-04-23)

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
