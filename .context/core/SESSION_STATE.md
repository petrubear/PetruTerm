# Session State

**Last Updated:** 2026-04-18 (sesión tarde)
**Session Focus:** Bug fixes prioritarios — KKP, tab bleed, CI clippy, .app env vars

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

## Commits sesión 2026-04-18 (tarde)

| Commit | Descripción |
|--------|-------------|
| `a5d691e` | fix: Shift+Enter KKP, tab bleed, clippy, .app env vars |

---

## Bugs resueltos esta sesión

### 1. Shift+Enter en apps con KKP (Claude Code CLI, etc.)
- **Root cause:** `key_map.rs` enviaba `\r` para Enter sin importar Shift. Apps modernas
  usan Kitty Keyboard Protocol (KKP): `\x1b[13;2u` para Shift+Enter. El terminal nunca
  procesaba las solicitudes de activación KKP.
- **Fix:**
  - `src/term/mod.rs:97`: `kitty_keyboard: true` en `TermConfig`
  - `src/app/input/key_map.rs:109`: cuando `TermMode::DISAMBIGUATE_ESC_CODES` activo y
    Shift presionado, enviar `\x1b[13;2u`; de lo contrario `\r`

### 2. TUI app (codeburn, etc.) aparece sobre todos los tabs
- **Root cause:** `cell_data_scratch` se reutiliza entre frames. El damage-skip de
  `collect_grid_cells_for` retiene filas "no dañadas" del frame anterior. Al cambiar de
  tab, el nuevo terminal heredaba datos del anterior en el scratch.
- **Fix:**
  - `src/app/renderer.rs:53`: `scratch_terminal_id: Option<usize>` en `RenderContext`
  - `src/app/mod.rs` (`build_all_pane_instances`): `cell_data_scratch.clear()` cuando
    `terminal_id` cambia

### 3. CI clippy fallando (manual_checked_ops)
- **Root cause:** if/else manual para guardia división-por-cero en `renderer.rs:1634`.
- **Fix:** `.checked_div().unwrap_or(0)`

### 4. Variables de entorno no disponibles en .app bundle
- **Root cause:** macOS no pasa por login shell al lanzar `.app` desde Finder/Dock.
  `~/.zshrc` nunca se carga — `OPENROUTER_API_KEY` invisible.
- **Fix:** `src/main.rs`: `inherit_login_shell_env()` antes de cualquier thread.
  Spawn `$SHELL -l -c 'env -0'`, parsea null-terminated pairs, `set_var` solo vars ausentes.

---

## Roadmap priorizado

### Phase 4 — Plugins (DESBLOQUEADA, próximo trabajo)
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

### 2026-04-18 (mañana) — Tier 3 + Tier 0
- TD-MEM-19, cursor overlay fast path, damage tracking, latency HUD, CI setup, TD-OP-02

### 2026-04-17 — Tier 1 + Tier 2 + Tier 4
- TD-MEM-20/21/12/10/11, TD-PERF-37/22/34/31/32/33/20/17

### 2026-04-16 — TD-RENDER-01 real fix + TD-RENDER-03
- Pre-pass bg-only vertices, mouse_dragged + clear_selection

### 2026-04-15 — Phase 3.5 Memory + Performance sprint
- TD-MEM-01..08, TD-PERF-06..13
