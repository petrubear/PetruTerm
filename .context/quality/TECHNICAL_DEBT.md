# Technical Debt Registry

**Last Updated:** 2026-07-03
**Open Items:** 3
**Critical (P0):** 0 | **P1:** 0 | **P2:** 0 | **P3:** 3 | **Deferred:** 2 | **Resueltos (Wave 1):** 8 | **Resueltos (Wave 2):** 5+5=10 | **Resueltos (Wave 3):** 4 | **Resueltos (Wave 4+5+6):** 8 | **Resueltos (Wave 7):** 4 | **Watch:** 3

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

Wave 7 — Deuda estructural remanente
  AUDIT-REFAC-08 ──────────────────────────────────────┘

Phase 9 — UI Restyle (abierto; branch ui-restyle)
  TD-P9-01 (verificación visual V-3/V-4; R-8 ya OK) ─┐
  TD-P9-05 (inset con titlebar no-Custom)            ├─► verificar/mergear a master
  TD-P9-06 (tuning visual V-4 bajo blur)             ┘
  RESUELTOS: TD-P9-02 (tab hit-test), TD-P9-03 (padding status bar), TD-P9-04 (borde panel), TD-P9-08 (deadlock cierre)

Watch
  AUDIT-CLEAN-02 (sin cambio; reevaluar si ContextAction crece)
  AUDIT-PERF-10 (micro-regresiones de benchmark; reevaluar tras el próximo pase de perf)
  TD-P9-07 (ignore de cargo audit quick-xml; quitar cuando winit bumpee Wayland)
```

**Conflictos a evitar:**
- `AUDIT-SEC-02` y `AUDIT-ENERGY-03` tocan el boot path de `UiManager`/MCP → separar trust gate de lazy-init.
- `AUDIT-ENERGY-02` y `AUDIT-ENERGY-04` tocan `about_to_wait()` → resolver el bug de polling infinito antes de retocar más timers.
- `AUDIT-THEME-01` y `AUDIT-THEME-02` deben compartir un único diseño de tokens semánticos para evitar re-hardcodear colores.
- `AUDIT-PERF-08` y `AUDIT-MEM-05` tocan el renderer/atlas path → reducir rebinding sin mezclarlo con cambios de packing/eviction en la misma PR.
- `AUDIT-RESP-01`, `AUDIT-ENERGY-05` y `AUDIT-MEM-04` tocan scheduling/background work → primero capar drenados y unificar wakeups, luego mover trabajo a un manager/threadpool.
- `AUDIT-REFAC-06` no debe abrirse antes de cerrar `AUDIT-PERF-08/09` y `AUDIT-RESP-01`; si no, se mezcla refactor estructural con hot paths.
- `AUDIT-REFAC-07`, `AUDIT-CLEAN-03` y `AUDIT-REFAC-08` tocan la capa de render/markdown/ui → separar reaperturas pequeñas (deduplicación/suppressions) de cualquier split mayor de módulos.

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

**AUDIT-ENERGY-05** — RESUELTO (2026-05-22). `poll_low_freq_tasks()` en `impl App` extrae battery poll + git poll (74 líneas) de `about_to_wait()`. Este último queda en 217 líneas, enfocado en scheduling/wakeup. `about_to_wait` sigue siendo llamado por winit pero ya no mezcla lógica de baja frecuencia con la de scheduling.

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

**AUDIT-REFAC-07** — RESUELTO (2026-05-22). `RenamePrompt` struct privado en `src/app/ui/mod.rs` unifica la lógica de los 8 métodos duplicados (`tab_rename_*` / `workspace_rename_*`). Los campos `tab_rename_input`/`workspace_rename_input` reemplazados por `tab_rename: RenamePrompt` / `workspace_rename: RenamePrompt`. `handle_rename_key` reducido a 10 líneas usando `RenamePrompt::handle_key`. Método `tab_rename_text()` añadido; `frame.rs` actualizado. 4 métodos públicos quedan (start/is_renaming/text + handle_key wrapper).

**AUDIT-CLEAN-03** — RESUELTO (2026-05-22). Se eliminó `#![allow(dead_code)]` global de `markdown.rs` y la supresión duplicada de `overlay.rs`. Los archivos `freetype_lcd.rs`, `pipeline.rs`, `tabs.rs` y `panes.rs` verificados: ninguno tiene `#![allow(dead_code)]` global. Reapertura de Copilot rechazada por falsa.

**AUDIT-REFAC-08** — RESUELTO (2026-05-22). `build_panel_messages` reducido de 505 a 357 líneas. `PanelMsgParams<'a>` struct en `chat.rs` reemplaza los 22 parámetros posicionales. Zero state extraído a `draw_panel_zero_state` (89 líneas); suggestion pills a `draw_suggestion_pills` (64 líneas). Supresión `too_many_arguments` eliminada de `build_panel_messages`. `App`, `UiManager` y `RenderContext` siguen concentrando muchos fields; la refactorización estructural mayor queda como deuda futura.

**AUDIT-REFAC-05** — RESUELTO (2026-05-11). Todos los monolitos convertidos a directorios-módulo con subarchivos por responsabilidad. Antes → ahora (mayor archivo del grupo): `renderer.rs` 4024 → `renderer/{mod,terminal,chat,overlay}.rs` max 1483; `mod.rs` 3663 → `mod+frame+app_state+layout.rs` max 1921; `ui.rs` 1986 → `ui/{mod,git,providers}.rs` max 1579; `chat_panel.rs` 1188 → `chat_panel/{mod,picker}.rs` max 919; `mux.rs` 1147 → `mux/{mod,workspace}.rs` max 981. 101/101 tests pasan.

**AUDIT-REFAC-01** — RESUELTO (2026-05-05). `window_event()` delega en `handle_redraw()`, `handle_keyboard()`, `handle_mouse_motion()`, `handle_mouse_button()` y `handle_scroll()`, preservando el flujo del loop.

**AUDIT-REFAC-02** — RESUELTO (2026-05-05). `build_chat_panel_instances` quedó dividido en `build_panel_header()`, `build_panel_file_section()` y `build_panel_messages()`, manteniendo `build_chat_panel_input_rows()` como fase separada.

**AUDIT-REFAC-03** — RESUELTO (2026-05-05). Nuevo `SidebarState` en `src/ui/sidebar.rs`; `App` agrupa bajo `self.sidebar` el estado visual, navegación y scroll del sidebar.

**AUDIT-REFAC-04** — RESUELTO (2026-05-05). `#![allow(dead_code)]` global eliminado de `gpu.rs`. 5 métodos muertos eliminados: `has_lcd`, `take_lcd_atlas`, `queue`, `surface_format`, `is_lcd_ready`.

**AUDIT-CLEAN-01** — RESUELTO (2026-05-05). Función `idx_or_default<T>` añadida en `renderer.rs`. 7 ocurrencias de `.cloned().unwrap_or_default()` reemplazadas.

**AUDIT-CLEAN-02** — WATCH (2026-05-05). `ContextAction` sigue por debajo del umbral para justificar un dispatch table; reevaluar solo si el enum/match crece de forma material.

**AUDIT-PERF-10** — WATCH (2026-05-22). La revalidación de Criterion no mostró fallos graves, pero sí micro-regresiones repetidas de ~1-2% en shaping/rasterize/build instances (`shape_line_ascii` 284.68 ns +1.54%, `shape_line_ascii_cached` 277.03 ns +1.39%, `shape_line_ligatures_cached` 546.21 ns +1.57%, `rasterize_glyph_ascii` 1.3094 µs +1.67%, `build_row_miss` 857.77 ns +1.58%, `build_frame_hit` 792.79 ns +1.13%). No bloquea, pero conviene volver a medir tras el próximo pase de optimización de hot paths.

**TD-P9-07** — WATCH (2026-07-03). `cargo audit` ignora `RUSTSEC-2026-0194` y `RUSTSEC-2026-0195` (quick-xml 0.39.2, DoS/quadratic) en `.cargo/audit.toml`. Entran transitivamente por `winit → smithay-client-toolkit → wayland-scanner` (Wayland, solo Linux; el target macOS nunca las compila). El fix 0.41 no satisface `wayland-scanner 0.31.9` (`quick-xml = "^0.39"`), así que requiere bump de winit upstream. Quitar el ignore cuando winit actualice su cadena Wayland.

---

## Phase 9 — UI Restyle (abierto, 2026-07-03)

> Introducidos por el trabajo de restyle en la branch `ui-restyle` (R-8 float
> layout + V-3/V-4). El denominador común es que **ninguno pudo verificarse
> visualmente** en el entorno de desarrollo (no hay captura de la ventana GPU);
> todo se validó por razonamiento estático + build/clippy/test. Ver
> [[project_phase9_ui_restyle]].

**TD-P9-01 — P3 — Verificación visual pendiente de V-3/V-4.**
PARCIALMENTE VERIFICADO (2026-07-03): capturas del usuario confirman R-8 (float
layout: sidebar/panel/terminal flotan, titlebar/status bar full-bleed), el header
del chat y la card del panel en la config por defecto (Custom titlebar, sin blur).
Falta verificar **V-3** (esquinas borderless — requiere `title_bar_style="none"`)
y **V-4** (superficies translúcidas — requiere `window.blur="dark"`), que son
no-op en la config por defecto. Acción: correr con esas configs y confirmar antes
de mergear. `src/app/mod.rs` (V-3), `src/config/schema.rs` (V-4).

**TD-P9-02 — RESUELTO (2026-07-03).** `hit_test_tab_bar` (`src/app/layout.rs`)
divergía del render en DOS cosas: (1) origen — usaba `158.0*sf` fijo en vez del
grid `content_pad_x` (que incluye sidebar_px + inset R-8); (2) ancho por tab —
usaba `" N " + " title "` (take 14) mientras el render usa `" title: N "`
(take 18). El drift del formato de label era la causa original; el inset de R-8 lo
empeoró. Fix: (a) `tab_display_label()` en `src/ui/tabs.rs` — formato de label
compartido por el render (`build_tab_bar_instances`) y el hit-test (test unit
`tab_label_tests`); (b) `hit_test_tab_bar` reescrito para replicar exactamente el
loop de columnas del render (mismo `pad_left`, `tabs_start_x=132*sf`,
`right_reserve`, `tabs_start_col`, `max_cols`). Verificación visual del clic con
sidebar abierto pendiente del usuario.

**TD-P9-03 — RESUELTO (2026-07-03).** Era un `round()` en `status_row`
(`frame.rs`) que podía bajar la status bar hasta ½ celda **más allá** de
`win_h - pad.bottom`, comiéndose el padding inferior (regresión visible reportada
por el usuario; se veía o no según la altura de ventana por el redondeo).
Cambiado a `floor()`: la barra nunca baja de `win_h - pad.bottom`, así el padding
se conserva siempre — reproduce el placement pre-R-8. El gap inferior varía entre
`pad.bottom` y `pad.bottom + cell_h` (inherente al snap a fila de grid, igual que
antes de R-8), pero nunca es menor.

**TD-P9-08 — RESUELTO (2026-07-03).** Deadlock al cerrar la ventana (colgaba tras
tener 2+ tabs y que `run_app` retornara, ejecutando `App::drop`). **Bug
preexistente** en el orden de `Pty::shutdown` (`src/term/pty.rs`), no de R-8.
Se cerraba el master fd (`close(master_fd)`) **antes** de mandar SIGHUP al shell.
En macOS/BSD `close()` bloquea hasta que el `read(master_fd)` en curso del hilo
lector termine, pero ese `read()` no termina hasta que el slave cierre — lo que
requería el SIGHUP posterior → deadlock. Encontrado por instrumentación del path
de cierre (el log paraba justo en `close_master`). Fix: reordenar a **SIGHUP →
join(reader) → join(child) → close(master)**. Verificado con reproducción real:
el log llega a `reader joined`/`child joined`/`App::drop end` y el proceso cierra
limpio. Nota: en cierre normal de 1 tab, macOS winit suele salir con
`process::exit` dentro de `run_app` (no corre `App::drop`), por eso el bug solo
aparecía intermitentemente.

**TD-P9-04 — VERIFICADO (2026-07-03).** El borde cerrado del panel de chat
(`build_chat_panel_instances`, `chat.rs`) se confirmó en captura: el panel se lee
como card completa, su borde inferior flota por encima de la status bar sin
recorte ni colisión. `py+ph` (alto = `total_rows*ch`) queda correctamente en el
gap sobre la status bar.

**TD-P9-05 — P3 — Coherencia del inset con titlebar no-Custom.**
El modelo de origen de grid único se validó para `title_bar_style=Custom`. En
modo `Native`/`None` con 2+ tabs, `gpu_pad_y` usa `TITLEBAR_HEIGHT*sf` mientras
`tab_h`/`sb_pad_y` usan `cell_height`; el back-compute de la tab bar podría
descuadrar. No probado. `src/app/frame.rs`, `src/app/layout.rs`.

**TD-P9-06 — P3 — Tuning visual de V-4 bajo blur.**
El alpha fijo `0.85` de `ui_surface`/`ui_surface_hover`
(`ColorScheme::apply_blur_translucency`, `schema.rs`) y el apilado de tints de
fila de mensaje + code-block bg (`ui_surface_active`) + selección sobre el panel
translúcido no se ajustaron con blur real. Puede verse desigual/parcheado.
Reevaluar el factor y qué tokens participan una vez verificado en pantalla.

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
Wave 7: AUDIT-REFAC-08
Phase 9 (abierto): TD-P9-01 (V-3/V-4), TD-P9-05, TD-P9-06 | resueltos: TD-P9-02, TD-P9-03, TD-P9-04, TD-P9-08
Watch: AUDIT-CLEAN-02, AUDIT-PERF-10, TD-P9-07
```
