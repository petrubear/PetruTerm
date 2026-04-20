# Session State

**Last Updated:** 2026-04-19
**Session Focus:** Fase 3.6 COMPLETA — GitHub Copilot provider. Tag v0.1.1 publicado.

## Branch: `master`

## Estado actual

**Phase 1–3 + 3.5 COMPLETE. Fase A COMPLETE. Fase 3.6 COMPLETE. Tag v0.1.1 publicado.**
**Build limpio: check + test + clippy + fmt PASS. CI verde.**
**Siguiente: Fase B — Menu Bar nativo macOS (crate muda)**

## Commits recientes relevantes

| Commit | Descripción |
|--------|-------------|
| (feat) | feat(llm): Add GitHub Copilot provider with device-flow OAuth |
| (chore)| chore: Update context — Fase 3.6 complete, v0.1.1 released |

---

## Fase 3.6 — GitHub Copilot Provider — CERRADA 2026-04-19

**Implementado:**
- `src/llm/copilot.rs`: device-flow OAuth con `client_id = Iv1.b507a08c87ecfe98`
- Token almacenado en macOS Keychain (`PetruTerm` / `GITHUB_COPILOT_OAUTH_TOKEN`)
- Copilot JWT auto-refresh cada ~30 min
- Fallback de modelo: si el modelo configurado tiene `/` o `:` (OpenRouter/Ollama), usa `gpt-4o`
- Header del chat panel muestra `provider:model` activo
- SSE helpers extraidos a `mod.rs` — eliminada duplicacion entre openrouter/openai_compat
- README: sección "Storing API keys securely (macOS Keychain)" con comandos para ambos providers

**Key non-obvious finding:**
- El endpoint `/copilot_internal/v2/token` solo acepta tokens de OAuth apps registradas para Copilot.
  Ni `gh auth token` ni PAT classic sirven — requiere device flow con `client_id = Iv1.b507a08c87ecfe98`.

---

## Próximo: Fase B — Menu Bar nativo macOS

- Crate `muda`; inicializar `MenuBar` en `main.rs` antes del event loop
- Menus: File, Edit, AI Chat, Window, Help
- Labels via i18n

---

## Sesiones anteriores (resumen)

### 2026-04-19 — Fase A + Fase 3.6
- v0.1.0 publicado (Fase A: versionado + i18n)
- Fase 3.6: GitHub Copilot provider

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
