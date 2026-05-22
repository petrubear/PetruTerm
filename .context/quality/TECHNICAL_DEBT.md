# Technical Debt Registry

**Last Updated:** 2026-05-22
**Open Items:** 0
**Critical (P0):** 0 | **P1:** 0 | **P2:** 0 | **P3:** 0 | **Deferred:** 2 | **Resueltos (Wave 1):** 8 | **Resueltos (Wave 2):** 5+5=10 | **Resueltos (Wave 3):** 4 | **Resueltos (Wave 4+5+6):** 8 | **Resueltos (Wave 7):** 1 | **Watch:** 1

> Resolved items are in [TECHNICAL_DEBT_archive.md](./TECHNICAL_DEBT_archive.md).

---

## Priority Definitions

| Priority | Definition | SLA |
|----------|------------|-----|
| P0 | Blocking development or causing incidents | Immediate |
| P1 | Significant impact on velocity or correctness | This sprint |
| P2 | Moderate impact, workaround exists | This quarter |
| P3 | Minor, address when convenient | Backlog |

---

## Grafo de dependencias

```
Wave 1 — Riesgo y desperdicio inmediato
  AUDIT-SEC-01   ──────────────────────────────────────┐
  AUDIT-SEC-02   ──────────────────────────────────────┤
  AUDIT-ENERGY-02──────────────────────────────────────┤─► Wave 2
  AUDIT-PERF-06  ──────────────────────────────────────┘

Wave 2 — Best in class (speed / battery / safety)
  AUDIT-ENERGY-03──────────────────────────────────────┐
  AUDIT-SEC-03   ──────────────────────────────────────┤
  AUDIT-ENERGY-04──────────────────────────────────────┤
  AUDIT-THEME-01 ──────────────────────────────────────┤─► Wave 3
  AUDIT-PERF-07  ──────────────────────────────────────┘

Wave 3 — Consistencia visual y mantenibilidad
  AUDIT-THEME-02 ──────────────────────────────────────┐
  AUDIT-REFAC-05 ──────────────────────────────────────┘

Wave 4 — Hot paths (CPU/GPU/CPI)
  AUDIT-PERF-08  ──────────────────────────────────────┐
  AUDIT-PERF-09  ──────────────────────────────────────┤
  AUDIT-RESP-01  ──────────────────────────────────────┘─► Wave 5

Wave 5 — Scheduling, memoria y energía
  AUDIT-ENERGY-05──────────────────────────────────────┐
  AUDIT-MEM-04   ──────────────────────────────────────┤
  AUDIT-MEM-05   ──────────────────────────────────────┤─► Wave 6
  AUDIT-REFAC-06 ──────────────────────────────────────┘

Wave 6 — Limpieza estructural
  AUDIT-REFAC-07 ──────────────────────────────────────┐
  AUDIT-CLEAN-03 ──────────────────────────────────────┘

Watch
  AUDIT-CLEAN-02 (sin cambio; reevaluar si ContextAction crece)
```

**Conflictos a evitar:**
- `AUDIT-SEC-02` y `AUDIT-ENERGY-03` tocan el boot path de `UiManager`/MCP → separar trust gate de lazy-init.
- `AUDIT-ENERGY-02` y `AUDIT-ENERGY-04` tocan `about_to_wait()` → resolver el bug de polling infinito antes de retocar más timers.
- `AUDIT-THEME-01` y `AUDIT-THEME-02` deben compartir un único diseño de tokens semánticos para evitar re-hardcodear colores.
- `AUDIT-PERF-08` y `AUDIT-MEM-05` tocan el renderer/atlas path → reducir rebinding sin mezclarlo con cambios de packing/eviction en la misma PR.
- `AUDIT-RESP-01`, `AUDIT-ENERGY-05` y `AUDIT-MEM-04` tocan scheduling/background work → primero capar drenados y unificar wakeups, luego mover trabajo a un manager/threadpool.
- `AUDIT-REFAC-06` no debe abrirse antes de cerrar `AUDIT-PERF-08/09` y `AUDIT-RESP-01`; si no, se mezcla refactor estructural con hot paths.

---

## P0 — Crítico

**AUDIT-SEC-01** — RESUELTO (2026-05-11). Path traversal en `write_file` cerrado: se canonicaliza el ancestro más cercano (para soportar ficheros nuevos en directorios nuevos) y se verifica `starts_with(cwd)` antes de mostrar el diálogo de confirmación. `src/app/ui.rs`.

**AUDIT-SEC-02** — RESUELTO (2026-05-11). Trust gate para MCP local: `load_global` y `load_local` separados en `config.rs`; `src/llm/mcp/trust.rs` persiste cwds confiables en `~/.config/petruterm/mcp_trust.json`; `UiManager::new()` y `reload_mcp()` sólo cargan `.petruterm/mcp.json` si el cwd está en la lista; acción "Trust local MCP config" en la palette para activarlo explícitamente.

---

## P1 — Alta prioridad

**AUDIT-PERF-08** — RESUELTO (2026-05-22). `GpuRenderer::render()` re-bindea `uniform_bind_group`, `atlas_bind_group` y `instance_buffer` para `bg_pipeline` y `cell_pipeline` tanto en main como en overlay aunque los recursos no cambian dentro del mismo render pass. `src/renderer/gpu.rs:448-457, 477-486`. Esto aumenta validación del driver, tráfico de comandos GPU y CPI del render loop; conviene encapsular el draw en un helper que haga bind una sola vez por bloque.

**AUDIT-PERF-09** — RESUELTO (2026-05-22). `try_word_cached_shape()` duplica tres veces la reconstrucción de `ShapedGlyph`, repite `colors.get(abs_col).copied().unwrap_or(...)` por glifo y aloca `dummy_colors` + `String` por palabra con cache miss. `src/font/shaper.rs:656-748`. Es hot path puro de shaping: eleva allocs, empeora locality de CPU/cache y sube el CPI en frames con texto nuevo.

**AUDIT-RESP-01** — RESUELTO (2026-05-22). `poll_ai_events()` / `poll_ai_block_events()` drenan canales sin límite con `while let Ok(...)` y se invocan en más de una fase del loop (`src/app/mod.rs:1451-1452, 1689-1690`; `src/app/frame.rs:173-174`). Bajo streaming intenso, una sola iteración puede quedar dominada por eventos AI y retrasar PTY/input/redraw. Hace falta batch cap por iteración y una fase única de polling.

**AUDIT-ENERGY-02** — RESUELTO (2026-05-11). `battery_polled: bool` reemplaza `battery_status.is_none()` como guarda de primera ejecución; en desktop (sin batería) el poll ocurre una vez al arranque y luego cada 30 s. `src/app/mod.rs`.

**AUDIT-ENERGY-03** — RESUELTO (2026-05-11). Cuando `llm.enabled = false`: runtime Tokio cambiado a `current_thread` (ahorra 2 threads OS) y bloque MCP omitido completamente. `src/app/ui.rs`.

**AUDIT-PERF-06** — RESUELTO (2026-05-11). `max_fps` conectado al render loop: `flush_redraw_request` respeta el intervalo `1/max_fps` y deja `needs_redraw=true` cuando el frame llega demasiado pronto; `about_to_wait` inyecta `frame_deadline` en el `WaitUntil` para despertar exactamente cuando el siguiente frame es válido. `animation_fps` eliminado de `perf.lua` (nunca estuvo en el schema). `src/app/mod.rs`.

**AUDIT-SEC-03** — RESUELTO (2026-05-11). `SkillManager::load()` y `SteeringManager::load()` aceptan `include_local: bool`; en `UiManager::new()` y `rewire_llm_provider()` se pasa `trust::is_trusted(&cwd)`. Local skills/steering solo se cargan si el cwd está en la lista de confianza (misma lista que SEC-02). `src/llm/skills.rs`, `src/llm/steering.rs`, `src/app/ui.rs`.

**AUDIT-PERF-01** — RESUELTO (2026-05-05). `FxHashSet` reemplaza `Vec::contains` en `push_md_line`. O(n) → O(1) por inserción. `src/app/renderer.rs:738`.

**AUDIT-PERF-02** — RESUELTO (2026-05-05). `build_chat_panel_instances` y `build_chat_panel_input_rows` reutilizan `fmt_buf` para header, picker, previews, zero-state, pills e input/hints, eliminando los `format!()` del hot path del panel.

**AUDIT-PERF-03** — RESUELTO (2026-05-05). `mcp_tools_cache: Vec<(String, Vec<String>)>` en `App`. Rebuilt lazily before `render_ctx` borrow; invalidated after `reload_mcp`. Zero-cost on sidebar frames (no BTreeMap, no alloc).

**AUDIT-ENERGY-01** — RESUELTO (2026-05-05). `App` ahora usa `needs_redraw: bool`; los handlers llaman `self.request_redraw()` y `about_to_wait()` hace un único `window.request_redraw()` por iteración via `flush_redraw_request()`.

---

## P2 — Prioridad media

**AUDIT-ENERGY-05** — RESUELTO (2026-05-22). `about_to_wait()` mezcla battery poll, git poll, blink, PTY coalescing y clock wake en el thread UI, además de duplicar el cálculo de `ControlFlow::WaitUntil` en ramas idle/battery-saver/normal. `src/app/mod.rs:1705-1933`. El coste no es sólo mantenibilidad: cada iteración recalcula deadlines de distinta prioridad y mantiene trabajo de baja frecuencia acoplado al camino crítico de responsividad/energía.

**AUDIT-MEM-04** — DIFERIDO (2026-05-22). Los 16 spawns ad-hoc son de baja frecuencia (clipboard, file scan, git). El coste de migrar a tokio::spawn_blocking o un BackgroundTaskManager es alto y el impacto en RSS medible es bajo en sesiones normales. Reevaluar si se observan picos de RSS bajo carga. Hay al menos 16 `std::thread::spawn` ad-hoc para clipboard, file scan, branch scan, PATH checks y `open`, sin pool ni backpressure. `src/app/mod.rs:823-889, 1221-1223`; `src/app/ui/mod.rs:496-533, 650-652`; `src/app/ui/git.rs:77-80`; `src/app/input/mod.rs:542-545, 790-793`; `src/term/tokenizer.rs:279-287`. Esto aumenta RSS, wakeups y jitter; conviene centralizar en `tokio::spawn_blocking` o un `BackgroundTaskManager`.

**AUDIT-MEM-05** — RESUELTO (2026-05-22). El atlas usa shelf packing y `evict_cold()` sólo limpia el mapa lógico; no recupera espacio físico y el cursor sigue avanzando hasta `AtlasError::Full`. `src/renderer/atlas.rs:153-158, 195-202`. Resultado: más clears completos del atlas, re-rasterización/upload extra y desperdicio de memoria GPU/CPU cuando la fragmentación crece.

**AUDIT-ENERGY-04** — RESUELTO (2026-05-11). (a) Git poll guard extendido a 60 s en battery saver mode (coincide con TTL). (b) `next_minute_wake` solo se computa cuando `status_bar.enabled`. (c) Battery poll condicionado a `window_focused`. `src/app/mod.rs`.

**AUDIT-THEME-01** — RESUELTO (2026-05-11). `ColorScheme::status_bar_colors() -> StatusBarColors` deriva todos los colores de la status bar del tema activo (accent, surface, ANSI cyan/yellow/red). `StatusBar::build()` recibe `&StatusBarColors`; constantes hardcoded eliminadas. `src/config/schema.rs`, `src/ui/status_bar.rs`, `src/app/mod.rs`, `src/app/renderer.rs`.

**AUDIT-THEME-02** — RESUELTO (2026-05-11). `ColorScheme.ui_border` derivado del fondo (pane separators). Palette `keybind_fg` → `colors.ui_muted`. `build_syntax_fg` recibe `&ColorScheme`; colores de sintaxis del input mapean a `ansi[1/2/3/6]` y `brights[3]`. `ChatUiConfig` reducido a `width_cols`; colores del panel de chat derivados del tema activo (`ansi[6]`, `foreground`). `config/default/llm.lua` limpiado.

**AUDIT-PERF-07** — RESUELTO (2026-05-11). `tab_bar_titles: Vec<String>` reemplazado por `tab_bar_titles_hash: u64` (FxHasher) en `RenderContext`. Comparacion y actualizacion del cache sin aloc por frame. `src/app/renderer.rs`, `src/app/mod.rs`.

**AUDIT-PERF-04** — RESUELTO (2026-05-05). `const HEADER_ACTIONS_COLS: usize = 12` en `chat_panel.rs`. Eliminado el `.map().sum()` por frame.

**AUDIT-PERF-05** — RESUELTO (2026-05-05). `parse_markdown` signature changed to `&mut ParseState` → `Vec<AnnotatedLine>`, eliminating the `streaming_fence_state.clone()`. `panel.input.clone()` deferred to cursor-on path only via `cursor_storage: String` + `&str` borrow.

**AUDIT-MEM-01** — RESUELTO (2026-05-05). Cap de 256 entradas en `terminal_shell_ctxs`: antes de insertar, evicts la entrada con `mtime` más antigua si `len() >= 256`.

**AUDIT-MEM-02** — RESUELTO (2026-05-05). `begin_frame()` runs shrink every 300 frames: `instances`, `lcd_instances`, `panel_instances_cache`, `rect_instances` → `shrink_to(len*2)` when `capacity > len*3`.

**AUDIT-MEM-03** — RESUELTO (2026-05-05). Same 300-frame pass in `begin_frame()`: `scratch_chars`, `scratch_colors`, `colors_scratch` use len*3 threshold; `scratch_str`, `fmt_buf` cap at 880 bytes (TYPICAL_COLS*4).

---

## P3 — Prioridad baja / Backlog (vacío)

**AUDIT-REFAC-06** — RESUELTO (2026-05-22). `build_workspace_sidebar_instances()` tenía 18 parámetros con `#[allow(clippy::too_many_arguments)]`. Resuelto con `SidebarDrawParams<'a>` en `src/app/renderer/mod.rs`; call site en `frame.rs` construye el struct; función en `overlay.rs` destructura al inicio — cuerpo sin cambios, supresión eliminada.

**AUDIT-REFAC-07** — RESUELTO (2026-05-22). Hay duplicación clara en el renombrado tab/workspace (`src/app/input/mod.rs:234-282`) y en el parser markdown para headings y spans delimitados (`src/llm/markdown.rs:79-106, 224-309`). Extraer helpers/traits reduciría ramas repetidas, bajaría mantenimiento y eliminaría trabajo redundante del parser.

**AUDIT-CLEAN-03** — RESUELTO (2026-05-22). El análisis estático muestra suppressions demasiado amplias: `#![allow(dead_code)]` global en `src/llm/markdown.rs:1` pese a que el módulo está activo, y `#[allow(clippy::too_many_arguments)]` duplicado en `src/app/renderer/overlay.rs:4-5`. Estas excepciones ocultan señal útil de clippy/dead code y conviene reemplazarlas por suppressions locales o por extracción de helpers/context structs.

**AUDIT-REFAC-05** — RESUELTO (2026-05-11). Todos los monolitos convertidos a directorios-módulo con subarchivos por responsabilidad. Antes → ahora (mayor archivo del grupo): `renderer.rs` 4024 → `renderer/{mod,terminal,chat,overlay}.rs` max 1483; `mod.rs` 3663 → `mod+frame+app_state+layout.rs` max 1921; `ui.rs` 1986 → `ui/{mod,git,providers}.rs` max 1579; `chat_panel.rs` 1188 → `chat_panel/{mod,picker}.rs` max 919; `mux.rs` 1147 → `mux/{mod,workspace}.rs` max 981. 101/101 tests pasan.

**AUDIT-REFAC-01** — RESUELTO (2026-05-05). `window_event()` delega en `handle_redraw()`, `handle_keyboard()`, `handle_mouse_motion()`, `handle_mouse_button()` y `handle_scroll()`, preservando el flujo del loop.

**AUDIT-REFAC-02** — RESUELTO (2026-05-05). `build_chat_panel_instances` quedó dividido en `build_panel_header()`, `build_panel_file_section()` y `build_panel_messages()`, manteniendo `build_chat_panel_input_rows()` como fase separada.

**AUDIT-REFAC-03** — RESUELTO (2026-05-05). Nuevo `SidebarState` en `src/ui/sidebar.rs`; `App` agrupa bajo `self.sidebar` el estado visual, navegación y scroll del sidebar.

**AUDIT-REFAC-04** — RESUELTO (2026-05-05). `#![allow(dead_code)]` global eliminado de `gpu.rs`. 5 métodos muertos eliminados: `has_lcd`, `take_lcd_atlas`, `queue`, `surface_format`, `is_lcd_ready`.

**AUDIT-CLEAN-01** — RESUELTO (2026-05-05). Función `idx_or_default<T>` añadida en `renderer.rs`. 7 ocurrencias de `.cloned().unwrap_or_default()` reemplazadas.

**AUDIT-CLEAN-02** — WATCH (2026-05-05). `ContextAction` sigue por debajo del umbral para justificar un dispatch table; reevaluar solo si el enum/match crece de forma material.

---

## Deferred — Requieren hardware/profiling específico

**TD-PERF-03** — DIFERIDO a Phase 2+. Dirty-rect GPU tracking solo aplica con GPUs discretas. En Apple Silicon unified memory, `write_buffer` es memcpy — no medible ni relevante.

**TD-PERF-05** — DIFERIDO a Phase 2+ (cross-platform). Atlas de glifos 64 MB de VRAM. Textura dinámica requiere soporte multi-plataforma GPU que no es objetivo actual.

---

## Guía activa

### REC-PERF-04: Medir antes de optimizar
Ningún fix P2/P3 debe implementarse sin profiling previo. El HUD F12 + benches criterion son las herramientas. Ver `term_specs.md §15` para frame budget targets.

### Orden de ejecución recomendado (estado actual)
``` 
Wave 1: AUDIT-SEC-01, AUDIT-SEC-02, AUDIT-ENERGY-02, AUDIT-PERF-06
Wave 2: AUDIT-ENERGY-03, AUDIT-SEC-03, AUDIT-ENERGY-04, AUDIT-THEME-01, AUDIT-PERF-07
Wave 3: AUDIT-THEME-02, AUDIT-REFAC-05
Wave 4: AUDIT-PERF-08, AUDIT-PERF-09, AUDIT-RESP-01
Wave 5: AUDIT-ENERGY-05, AUDIT-MEM-04, AUDIT-MEM-05, AUDIT-REFAC-06
Wave 6: AUDIT-REFAC-07, AUDIT-CLEAN-03
Watch: AUDIT-CLEAN-02
```
