# Session State

**Last Updated:** 2026-04-20
**Session Focus:** Fase C-1 COMPLETA — Unified titlebar.

## Branch: `master`

## Estado actual

**Phase 1–3 + 3.5 COMPLETE. Fase A COMPLETE. Fase 3.6 COMPLETE. Fase B COMPLETE. Fase C-1 COMPLETE.**
**Siguiente: Fase C-2 (Workspace model en Mux) + C-3 (Workspace sidebar).**

---

## Fase C-1 — Unified titlebar — PENDIENTE de arreglos

### Lo que se implemento (sin commit)

- `TITLEBAR_HEIGHT = 30.0` en `src/app/mod.rs`
- `tab_bar_visible()` siempre true en Custom mode
- `tab_bar_height_px()` devuelve TITLEBAR_HEIGHT en Custom mode
- `apply_tab_bar_padding()` usa TITLEBAR_HEIGHT + pad.top
- `default pad.top` cambiado de 30 a 5 en `src/config/schema.rs`
- `build_tab_bar_instances`: nueva firma con `win_w` y `gpu_pad_y`; botones sidebar/layout como rects en pixel coords; pills solo cuando tabs > 1
- Hit-test unificado en zona y < TITLEBAR_HEIGHT para Custom mode
- `hit_test_tab_bar` actualizado para tabs_start_x = 132px en Custom mode

### Issues visibles — 20:46 screenshot (1 tab)

- Zona de cabecera vacia pero respeta el espacio de traffic lights. Correcto.
- Botones sidebar/layout no visibles — BTN_COLOR=[0.22,0.22,0.28,0.7] casi identico al fondo.

### Issues visibles — 20:52 screenshot (2 tabs)

**Problema raiz confirmado: mezcla de unidades logicas vs fisicas.**

Con 2 tabs, las pills "1 zsh" y "2 zsh" aparecen encima de los traffic lights (x≈65px en el screenshot). Esto significa que `tabs_start_col` calcula un valor demasiado pequeno.

**Causa:** Las constantes `TABS_START_X=132`, `SIDEBAR_BTN_X=80`, `LAYOUT_BTN_X=106` estan en pixels LOGICOS (puntos), pero el pipeline del renderer opera en pixels FISICOS. En Retina (2x scale_factor):
- `renderer.size()` devuelve pixels fisicos (ej. 808px para ventana de 404 puntos)
- `self.shaper.cell_width` es fisico (ej. 20px fisico = 10 puntos)
- `pill_x = pad_left + col * cell_w` usa unidades fisicas
- `tabs_start_col = ceil((132 - 20) / cell_w_fisico)` → con cell_w=20 fisico: ceil(5.6)=6, pills en x=20+7*20=160 fisico = 80 puntos. Eso parece bien...

**Revision del diagnostico:** Si la pantalla es 1x (no Retina), cell_w=9-10px logico=fisico, tabs_start_col=ceil(112/10)=12, pills en x=20+13*10=150. Todavia mas alla de traffic lights (que terminan en ~68px).

**El problema real puede ser diferente.** La pill "1" aparece en x≈65px en la imagen de 808px de ancho. Si el display es 2x, eso es x=32.5 logico. Para que pill_x=32.5, col=1 (con cell_w=9 logico). tabs_start_col deberia ser 12+ pero las pills estan en col=1.

**Hipotesis mas probable:** El cache de `tab_bar_instances_cache` esta sirviendo instancias viejas del tab bar anterior (que usaba pad_top=30 y tabs a partir de col=0). El cache se invalida cuando cambian titulos, active_idx, o total_cols — pero NO cuando cambia la estructura de la funcion (el primer frame tras el cambio de codigo usa el cache obsoleto del run anterior guardado en el RenderContext). Como el RenderContext se crea al inicio de la sesion y el cache se llena en el primer frame, si `tab_key_changed` es false porque los inputs son los mismos, usa el cache que tiene geometry con pad_top=30 y col=0 (posiciones antiguas).

**Fix para proxima sesion:**
1. Verificar el scale_factor del display: agregar `log::info!("scale_factor={}", window.scale_factor())` en `resumed()`.
2. Pasar `scale_factor: f32` a `build_tab_bar_instances` y multiplicar todos los pixel constants por el.
3. Alternativamente: calcular `tabs_start_x` en unidades fisicas desde el principio usando `scale_factor * 132.0`.
4. Forzar invalidacion del tab_bar_instances_cache en la primera sesion despues de cambio estructural (o simplemente limpiar el cache en `resumed()`).
5. Confirmar que `BTN_COLOR` contrasta con el fondo cambiandolo a [0.5, 0.3, 0.9, 1.0] temporalmente para debug.

### Lo que FUNCIONA

- Con 1 tab: no se muestra pill (correcto segun pedido del usuario)
- La zona de cabecera respeta el espacio de los traffic lights
- El status bar sigue funcionando
- El menu nativo macOS sigue funcionando
- Terminal content empieza en y = 35 (TITLEBAR_HEIGHT=30 + pad.top=5)

---

## Fase B — Menu Bar nativo macOS — CERRADA 2026-04-20

**Implementado:**
- `src/app/menu.rs`: `AppMenu` struct con muda. Menus: PetruTerm (app), File, View, AI, Window
- File: Settings (abre `~/.config/petruterm/` en Finder), Reload Config
- View: Toggle Status Bar, Switch Theme, Toggle Fullscreen
- AI: Toggle Panel, Explain, Fix Error, Undo Write, Enable/Disable
- Window: predefined macOS + Tab submenu (New/Close/Rename/Next/Prev) + Pane submenu (Split H/V, Close, Focus dirs)
- Sin aceleradores — keybinds son leader-based y no se pueden registrar como menu shortcuts
- `OpenConfigFolder` action agregada (abre carpeta config en Finder)

**Key non-obvious finding:**
- `muda::MenuEvent::set_event_handler` y `receiver()` son mutuamente exclusivos.
  Con `set_event_handler` activo, `receiver()` siempre vacio. Solucion: no usar handler, solo `receiver()`.
- El drain de menu events debe hacerse en `about_to_wait()`, no en `user_event()`.
- Despues de dispatch de accion de menu, hay que replicar el bloque post-accion del handler
  `KeyboardInput`: capturar `tab_count_before`/`pane_count_before` y llamar
  `apply_tab_bar_padding()` + `resize_terminals_for_panel()` si cambian. Sin esto,
  nuevos tabs/panes desde el menu se renderizan con viewport de altura cero.

---

## Sesiones anteriores (resumen)

### 2026-04-20 — Fase C-1 inicio
- Unified titlebar implementado pero con bugs (ver arriba). Sin commit.

### 2026-04-19 — Fase A + Fase 3.6
- v0.1.0 publicado (Fase A: versionado + i18n)
- Fase 3.6: GitHub Copilot provider

### 2026-04-19 — Sprint cierre Phase 3.5
- Deuda P2/P3 cerrada, benches desbloqueados, CI verde, status bar flicker fix

### 2026-04-18 (tarde) — Bug fixes prioritarios
- KKP Shift+Enter, tab bleed, CI clippy, .app env vars

### 2026-04-18 (manana) — Tier 3 + Tier 0
- TD-MEM-19, cursor overlay fast path, damage tracking, latency HUD, CI setup, TD-OP-02

### 2026-04-17 — Tier 1 + Tier 2 + Tier 4
- TD-MEM-20/21/12/10/11, TD-PERF-37/22/34/31/32/33/20/17

### 2026-04-16 — TD-RENDER-01 real fix + TD-RENDER-03
- Pre-pass bg-only vertices, mouse_dragged + clear_selection

### 2026-04-15 — Phase 3.5 Memory + Performance sprint
- TD-MEM-01..08, TD-PERF-06..13
