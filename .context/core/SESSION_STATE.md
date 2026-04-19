# Session State

**Last Updated:** 2026-04-18 (sesión noche)
**Session Focus:** Bug fixes — tab en blanco al cambiar tabs, LLM error message, Keychain macOS, keybind AI panel

## Branch: `master`

## Estado actual

**Phase 1–3 COMPLETE. Phase 3.5: Tiers 1–4 CERRADOS. Tier 0 CERRADO (accionable). Tier 3 CERRADO.**
**TD-OP-02 CERRADO. Sin P1 abiertos. Tier 5 (arquitectura pesada) pendiente.**
**Phase 3.5 exit criteria ALCANZADOS. Phase 4 (plugins) desbloqueada.**

## Build

- **cargo check:** PASS
- **cargo test --lib:** PASS (9 tests)
- **cargo clippy --all-features -- -D warnings:** PASS
- **cargo fmt --check:** PASS
- **CI (GitHub):** verde (verificado 2026-04-18)

---

## Commits sesión 2026-04-18 (noche)

| Commit | Descripción |
|--------|-------------|
| (pendiente) | fix: tab blank on switch, LLM keychain, AI panel focus keybind |

## Commits sesión 2026-04-18 (tarde)

| Commit | Descripción |
|--------|-------------|
| `a5d691e` | fix: Shift+Enter KKP, tab bleed, clippy, .app env vars |

---

## Bugs resueltos esta sesión

### 1. Tab en blanco al cambiar de tab (hasta presionar una tecla)
- **Root cause:** `cell_data_scratch.clear()` al cambiar terminal_id, pero
  `collect_grid_cells_for` usaba damage parcial (sin cambios) del nuevo terminal →
  damage-skip saltaba todas las filas → buffer queda con strings vacias → pantalla en blanco.
- **Fix:**
  - `src/app/mux.rs`: `collect_grid_cells_for` recibe `force_full: bool`
  - `src/app/mod.rs` (`build_all_pane_instances`): pasa `force_full = terminal_changed`
  - Cuando `force_full=true`, `can_skip=false` → todas las filas se leen del grid

### 2. Error LLM "LLM not configured" no informativo
- **Root cause:** El mensaje no indicaba por que fallo (API key faltante, provider incorrecto, etc.)
- **Fix:**
  - `src/app/ui.rs`: `llm_init_error: Option<String>` en `UiManager`; captura el error real de `build_provider`
  - Muestra el error real al usuario en lugar de mensaje generico

### 3. API key de OpenRouter desde Apple Keychain
- **Fix:** `src/llm/openrouter.rs`: funcion `keychain_api_key()` — resolucion en orden:
  1. `config.api_key` (Lua)
  2. `OPENROUTER_API_KEY` env var
  3. macOS Keychain via `security find-generic-password -s PetruTerm -a OPENROUTER_API_KEY -w`
  - Para almacenar: `security add-generic-password -s PetruTerm -a OPENROUTER_API_KEY -w <key>`

### 4. Leader+A (Shift+A) para focus AI panel no funcionaba
- **Root cause:** Presionar Shift despues del leader es fragil en macOS (timing, logical_key inconsistente).
- **Fix:** Rediseno del flujo de focus:
  - `leader+a` (minuscula, sin Shift) → `FocusAiPanel`: alterna focus terminal↔chat, abre si cerrado
  - `Escape` en panel → **quita focus sin cerrar** (antes cerraba el panel)
  - `/q` en input del panel → cierra el panel
  - `config/default/keybinds.lua`: `ToggleAiPanel` removido; solo `FocusAiPanel` con `a`

---

## Roadmap priorizado

### Phase 4 — Plugins (DESBLOQUEADA, proximo trabajo)
- lazy.nvim-style plugin loader en Lua
- `src/plugins/` — plugin loader + Lua API (doc en `src/plugins/api.rs`)
- Ver `.context/specs/build_phases.md` para deliverables y exit criteria

### Tier 0 pendiente (bloqueados)
- Bench `build_instances` — bloqueado: acoplado a winit
- Bench `rasterize_to_atlas` — bloqueado: requiere `wgpu::Queue` headless
- Tracy integration, GPU timestamps

### Tier 5 — Arquitectura pesada (requiere baseline Tier 0 primero)
- Sub-E: rayon per-pane + `rtrb` PTY
- Sub-G: atlas split, ring buffer, unificar bg+glyph pass
- Sub-H: PGO con workload real

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
