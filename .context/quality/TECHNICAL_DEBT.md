# Technical Debt Registry

**Last Updated:** 2026-05-05
**Open Items:** 14
**Critical (P0):** 0 | **P1:** 4 | **P2:** 4 | **P3:** 6 | **Deferred:** 2

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
Wave 1 — Quick wins, independientes (hacer primero)
  AUDIT-PERF-01  ──────────────────────────────────────┐
  AUDIT-PERF-04  ──────────────────────────────────────┤
  AUDIT-REFAC-04 ──────────────────────────────────────┤─► Wave 2
  AUDIT-CLEAN-01 ──────────────────────────────────────┘

Wave 2 — Independientes, esfuerzo medio
  AUDIT-PERF-05  ──────────────────────────────────────┐
  AUDIT-MEM-01   ──────────────────────────────────────┤
  AUDIT-MEM-02   ──────────────────────────────────────┤─► Wave 3
  AUDIT-MEM-03   ──────────────────────────────────────┤
  AUDIT-PERF-03  ──────────────────────────────────────┘

Wave 3 — Requieren contexto de Waves anteriores
  AUDIT-PERF-02  (tocar renderer.rs antes del split) ──┐
  AUDIT-ENERGY-01 (tocar mod.rs antes del split) ──────┤─► Wave 4

Wave 4 — Refactors estructurales grandes (hacer al final)
  AUDIT-REFAC-02  bloqueado por: AUDIT-PERF-02
  AUDIT-REFAC-03  bloqueado por: AUDIT-ENERGY-01
  AUDIT-REFAC-01  bloqueado por: AUDIT-ENERGY-01 + AUDIT-REFAC-03
```

**Conflictos a evitar:**
- `AUDIT-ENERGY-01` y `AUDIT-REFAC-03` tocan los mismos campos de `App` → commits separados.
- `AUDIT-PERF-02` y `AUDIT-REFAC-02` tocan `build_chat_panel_instances` → hacer PERF-02 primero.
- `AUDIT-REFAC-01` y cualquier cambio en `window_event()` → hacer REFAC-01 al final.

---

## P0 — Crítico

_Ninguno abierto._

---

## P1 — Alta prioridad

**AUDIT-PERF-01** — O(n²) en `push_md_line` al construir `boundaries`.
- **Archivo:** `src/app/renderer.rs:738`
- **Problema:** `boundaries.contains(&s)` dentro de un loop sobre spans — O(n) por iteración. Con líneas de markdown largas con muchos spans esto degrada cuadráticamente.
- **Fix:** Reemplazar `Vec` con `HashSet` para construcción de boundaries; convertir a `Vec` solo para el sort final.
- **Wave:** 1 — independiente, ~5 líneas.

**AUDIT-PERF-02** — 25+ `format!()` allocations por frame en `build_chat_panel_instances`.
- **Archivo:** `src/app/renderer.rs:871–1593`
- **Problema:** Cada frame visible del panel aloca docenas de `String` vía `format!()`. Con 60 fps = miles de allocations/seg en el GC-less runtime de Rust, fragmentando el heap.
- **Fix:** Reusar `self.fmt_buf: String` (ya existe en `RenderContext`) via `write!(&mut self.fmt_buf, ...)` + pasar `&self.fmt_buf` donde posible.
- **Wave:** 3 — hacer ANTES de `AUDIT-REFAC-02` (split de función).
- **Bloquea:** `AUDIT-REFAC-02`.

**AUDIT-PERF-03** — `BTreeMap` allocado y destruido cada frame para lista de MCP servers.
- **Archivo:** `src/app/mod.rs:1743–1750`
- **Problema:** `BTreeMap<String, Vec<String>>` se construye en `window_event(RedrawRequested)` cada frame si el sidebar es visible. Allocation + tree rebalancing + conversión a Vec = trabajo innecesario.
- **Fix:** Cachear resultado en `App` o `UiManager` como `mcp_tools_cache: Vec<(String, Vec<String>)>` e invalidar solo cuando MCP tools cambian.
- **Wave:** 2 — independiente, añadir campo de cache.

**AUDIT-ENERGY-01** — 34 llamadas a `request_redraw()` sin deduplicación.
- **Archivo:** `src/app/mod.rs` — líneas 214, 1047, 1065, 1916, 1932, 1989, 1998, 2012, 2035, 2060, 2071, 2082, 2099, 2157, 2187, 2247, 2254, 2283, 2304 y más.
- **Problema:** Múltiples eventos en el mismo ciclo pueden llamar `request_redraw()` independientemente. Aunque winit coalesce frames, el patrón hace difícil razonar sobre cuándo se renderiza.
- **Fix:** Flag `needs_redraw: bool` en `App`. Reemplazar todas las llamadas a `request_redraw()` por `self.needs_redraw = true`. En `about_to_wait()`, emitir `window.request_redraw()` exactamente una vez si `needs_redraw` y resetear.
- **Wave:** 3 — toca mod.rs ampliamente; hacer ANTES de `AUDIT-REFAC-01`.
- **Bloquea:** `AUDIT-REFAC-01`, `AUDIT-REFAC-03`.

---

## P2 — Prioridad media

**AUDIT-PERF-04** — `header_actions_start_col()` recalcula ancho constante en cada frame.
- **Archivo:** `src/llm/chat_panel.rs:116–131`
- **Problema:** Los labels de las 3 acciones del header son constantes (`[↺]`, `[⎘]`, `[✕]`), su ancho total también. Se recalcula con `.map().sum()` en cada llamada (cada frame del panel).
- **Fix:** Extraer `const HEADER_ACTIONS_TOTAL_WIDTH: usize` calculado en compile time.
- **Wave:** 1 — independiente, trivial, 3 líneas.

**AUDIT-PERF-05** — Clone de `ParseState` y `panel.input` en el render loop.
- **Archivo:** `src/app/renderer.rs:1339, 1708`
- **Problema:** `self.streaming_fence_state.clone()` y `panel.input.clone()` (String) se ejecutan cada frame cuando el panel es visible.
- **Fix:** Pasar `&self.streaming_fence_state` por referencia. Cambiar `input_display` a borrow `&panel.input` en las llamadas que no modifican.
- **Wave:** 2 — independiente, refactor de firmas.

**AUDIT-MEM-01** — `terminal_shell_ctxs` HashMap crece sin límite.
- **Archivo:** `src/app/mod.rs:106`
- **Problema:** Cada terminal abierto agrega una entrada. El cleanup en `close_terminal` funciona, pero si el proceso del shell muere sin pasar por `close_terminal`, la entrada queda viva. Sin upper bound en sesiones largas.
- **Fix:** Agregar cap de 256 entradas: antes de insertar, si `len() >= 256` eliminar la entrada con `mtime` más antigua.
- **Wave:** 2 — independiente, ~8 líneas.

**AUDIT-MEM-02** — Vecs de instancias GPU nunca hacen `shrink_to_fit()`.
- **Archivo:** `src/app/renderer.rs:52–56, 100–103`
- **Problema:** `instances`, `lcd_instances`, `panel_instances_cache`, `rect_instances` y otros Vecs se `.clear()` cada frame pero nunca reducen su capacity. Ventanas grandes o picos de contenido dejan Mb de capacity sin usar.
- **Fix:** En `RenderContext`, agregar `shrink_counter: u64`. Cada 300 frames, si `capacity > len * 3`, hacer `shrink_to(len * 2)` en cada Vec de instancias.
- **Wave:** 2 — independiente.

**AUDIT-MEM-03** — Scratch buffers sin límite de capacity.
- **Archivo:** `src/app/renderer.rs:71–98`
- **Problema:** `scratch_chars`, `scratch_str`, `scratch_colors`, `fmt_buf` se `.clear()` pero nunca encogen. Una línea de 10k chars deja esa capacity permanentemente.
- **Fix:** Mismo patrón que AUDIT-MEM-02 — shrink periódico si `capacity > TYPICAL_COLS * 4` (TYPICAL_COLS = 220).
- **Wave:** 2 — puede hacerse en el mismo commit que AUDIT-MEM-02.

---

## P3 — Prioridad baja / Backlog

**AUDIT-REFAC-01** — `window_event()` monolítico de 2000+ líneas.
- **Archivo:** `src/app/mod.rs:1130–3125`
- **Problema:** Un solo método maneja RedrawRequested (1500+ líneas), KeyboardInput, MouseMotion, MouseButton, ModifiersChanged, Focused, etc. Dificulta lectura, testing y localización de bugs.
- **Fix:** Extraer métodos privados: `handle_redraw()`, `handle_keyboard()`, `handle_mouse_motion()`, `handle_mouse_button()`, `handle_scroll()`.
- **Wave:** 4 — hacer DESPUÉS de `AUDIT-ENERGY-01` y `AUDIT-REFAC-03`.
- **Bloqueado por:** `AUDIT-ENERGY-01`, `AUDIT-REFAC-03`.

**AUDIT-REFAC-02** — `build_chat_panel_instances` de 810 líneas.
- **Archivo:** `src/app/renderer.rs:783–1593`
- **Problema:** Una función hace: colores, background, header, file section, message loop (400+ líneas), suggestions, input rows. Supera el límite de 400 líneas del proyecto.
- **Fix:** Extraer: `build_panel_header()`, `build_panel_file_section()`, `build_panel_messages()`. `build_chat_panel_input_rows()` ya existe — continuar ese patrón.
- **Wave:** 4 — hacer DESPUÉS de `AUDIT-PERF-02`.
- **Bloqueado por:** `AUDIT-PERF-02`.

**AUDIT-REFAC-03** — Estado del sidebar disperso en 10 campos de `App`.
- **Archivo:** `src/app/mod.rs:74–121`
- **Problema:** `sidebar_visible`, `sidebar_nav_cursor`, `panel_resize_drag`, `panel_resize_hover`, `sidebar_rename_input`, `sidebar_kbd_active`, `info_sidebar_section`, `mcp_scroll`, `skills_scroll`, `steering_scroll` podrían agruparse en `SidebarState`.
- **Fix:** Crear `struct SidebarState` en `src/ui/sidebar.rs` y mover los 10 campos. Acceder como `self.sidebar.visible`, etc.
- **Wave:** 4 — hacer DESPUÉS de `AUDIT-ENERGY-01` (toca mismos campos de App).
- **Bloqueado por:** `AUDIT-ENERGY-01`.

**AUDIT-REFAC-04** — `#![allow(dead_code)]` global en `gpu.rs`.
- **Archivo:** `src/renderer/gpu.rs:1`
- **Problema:** Suprime todos los warnings de dead code del módulo. Enmascara métodos realmente muertos.
- **Fix:** Quitar el allow global. Añadir `#[allow(dead_code)]` selectivo solo en los métodos de API pública que sí se necesitan pero el compilador no ve usados.
- **Wave:** 1 — independiente, trivial.

**AUDIT-CLEAN-01** — Patrón `.cloned().unwrap_or_default()` duplicado en 7+ lugares.
- **Archivo:** `src/app/renderer.rs:1742–1745, 1879, 1915, 1930`
- **Problema:** Código idiomático repetido. Menor, pero agrega ruido.
- **Fix:** Añadir extension trait o función libre `get_or_default<T: Clone + Default>(slice: &[T], idx: usize) -> T`.
- **Wave:** 1 — independiente, trivial.

**AUDIT-CLEAN-02** — `ContextAction` match podría escalar mal.
- **Archivo:** `src/app/mod.rs:2197`
- **Problema:** 6 ramas hoy. Si crece a 15+, un dispatch table `HashMap<ContextAction, fn>` sería más limpio.
- **Fix:** No urgente — marcar como watch. Actuar si llega a 10+ variantes.
- **Wave:** 4 — solo si crece.

---

## Deferred — Requieren hardware/profiling específico

**TD-PERF-03** — DIFERIDO a Phase 2+. Dirty-rect GPU tracking solo aplica con GPUs discretas. En Apple Silicon unified memory, `write_buffer` es memcpy — no medible ni relevante.

**TD-PERF-05** — DIFERIDO a Phase 2+ (cross-platform). Atlas de glifos 64 MB de VRAM. Textura dinámica requiere soporte multi-plataforma GPU que no es objetivo actual.

---

## Guía activa

### REC-PERF-04: Medir antes de optimizar
Ningún fix P2/P3 debe implementarse sin profiling previo. El HUD F12 + benches criterion son las herramientas. Ver `term_specs.md §15` para frame budget targets.

### Orden de ejecución recomendado (Wave 1 → 4)
```
Wave 1: AUDIT-PERF-01, AUDIT-PERF-04, AUDIT-REFAC-04, AUDIT-CLEAN-01
Wave 2: AUDIT-PERF-05, AUDIT-MEM-01, AUDIT-MEM-02+03, AUDIT-PERF-03
Wave 3: AUDIT-PERF-02, AUDIT-ENERGY-01
Wave 4: AUDIT-REFAC-02, AUDIT-REFAC-03, AUDIT-REFAC-01, AUDIT-CLEAN-02
```
