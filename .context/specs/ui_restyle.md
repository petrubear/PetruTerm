# PetruTerm — UI Restyle: Floating Surfaces & Vibrancy

> Aprobado (diseño) 2026-07-02. Ejecución en dos fases.
> **Objetivo:** llevar la chrome de la app (sidebars, command palette, chat panel,
> tabs, status bar, ventana) al estilo visual de la imagen de referencia
> (`~/Downloads/Gemini_Generated_Image_*.png`): superficies redondeadas que "flotan"
> con tonos propios, buen espaciado, botones/headers rellenos y (Fase 2) ventana
> translúcida con blur sobre el wallpaper.
>
> **Lo que NO cambia:** funcionalidad. Las herramientas actuales se mantienen
> (Workspace/MCP/Skills/Steering en la sidebar izquierda, chat/ACP en la derecha,
> tabs, panes). No hay explorador de archivos. El contenido del terminal (centro)
> sigue siendo texto plano — las "tarjetas" alrededor del output en la imagen son
> alucinación del render de IA, no comportamiento real de un terminal.

## Contexto de código (verificado 2026-07-02)

- El pipeline gráfico ya soporta todo lo necesario: `RoundedRectInstance`
  (rects redondeados, bordes, fondos) ya se usa en sidebar, palette, chat panel,
  tabs y status bar. **No hace falta trabajo de motor gráfico.**
- El chat panel (`src/app/renderer/chat.rs`) ya dibuja su borde exterior con radius
  10 (~línea 72/82), bloques de código (~878) y pills (~974). El problema es de
  *estilo aplicado*, no de capacidad: tono de superficie casi igual al fondo, sin
  márgenes/insets, botones como rectángulos de línea fina.
- Tokens de color semánticos ya existen en `ColorScheme`: `ui_accent`, `ui_surface`,
  `ui_surface_active`, `ui_surface_hover`, `ui_muted`, `ui_success`, `ui_overlay`
  (+ `ui_border` referenciado en overlay). Faltan tokens de *espaciado/radio* y un
  posible tono `ui_surface_raised` para las cards flotantes.
- `window.opacity` ya está en el schema (`src/config/schema.rs:217`, `config/default/ui.lua:75`).
  Falta cablearlo a la ventana winit + clear color alpha, y el blur nativo.

## Principios de diseño

1. **Tokens únicos, cero literales.** Toda superficie usa el mismo set de radios,
   espaciado y bordes. Nada de constantes sueltas por módulo.
2. **Jerarquía de tonos:** `background` (terminal) < `ui_surface` (panel) <
   `ui_surface_hover` < `ui_surface_active`. La superficie del panel debe ser
   visiblemente distinta del fondo del terminal.
3. **Paneles que flotan:** inset exterior consistente para que cada panel tenga
   un gap alrededor y lea como card.
4. **Radios consistentes:** panel ~12px, contenedor interno ~8px, pill/botón ~6px
   (× scale_factor).
5. **Bordes sutiles:** 1px `ui_border`, mismo en todos los paneles.
6. **Todo del tema.** Ningún color hardcodeado; adaptativo por tema activo.

---

## Fase 1 — Theming & spacing (sin código de plataforma)

> ~85% del look de la referencia. Todo sobre primitivas existentes. Reversible, bajo riesgo.

### R-1: Tokens de estilo UI (espaciado + radios)
Centralizar en un solo lugar (p. ej. `src/renderer/ui_style.rs` o campos en
`RenderContext`) las constantes de espaciado y radio que hoy están dispersas
(`chat.rs:72` radius, `pill_margin`, insets de `overlay.rs`, etc.).
- [ ] Definir escala de espaciado (`SP_1=4, SP_2=8, SP_3=12, SP_4=16` × scale) y radios (`R_PANEL=12, R_INNER=8, R_PILL=6` × scale) en un módulo/const set
- [ ] Definir `BORDER=1.0 × scale` y helper de color de borde (`colors.ui_border`)
- [ ] Reemplazar los literales de radio/inset dispersos por estos tokens

### R-2: Tonos de superficie en temas
Asegurar contraste real superficie-vs-fondo y añadir tono elevado si hace falta.
- [ ] Verificar que `ui_surface` sea perceptiblemente más claro que `background` en los 5 temas bundled; ajustar `derive_ui_colors` si el delta es < ~8%
- [ ] (Opcional) Añadir `ui_surface_raised` para cards elevadas (palette/overlays) — derivado de `ui_surface` +~4%
- [ ] Actualizar `dracula-pro.lua` (default) con los valores finales

### R-3: Sidebar izquierda — restyle
`src/app/renderer/overlay.rs` → `build_workspace_sidebar_instances`. Mantener las 4
secciones (Workspace/MCP/Skills/Steering) y toda su navegación.
- [ ] Inset exterior para que la sidebar flote (gap contra el borde de la ventana)
- [ ] Fondo de superficie `ui_surface` + borde 1px `ui_border`, radius `R_PANEL`
- [ ] Headers de sección como banda de superficie con acento (`build_syntax_fg`/`ui_accent`)
- [ ] Filas de item con fill redondeado en hover (`ui_surface_hover`) / activo (`ui_surface_active`), radius `R_PILL`
- [ ] Padding interno consistente (usar tokens R-1); respetar el cache key existente

### R-4: Command palette — restyle
`src/app/renderer/overlay.rs` → `build_palette_instances`.
- [ ] Contenedor redondeado (`R_PANEL`) con `ui_surface_raised` + borde `ui_border`
- [ ] Input de búsqueda como campo inset (superficie interna, radius `R_INNER`)
- [ ] Filas de resultado con fill de hover/selección redondeado; padding consistente
- [ ] Hints de keybind alineados a la derecha en `ui_muted`
- [ ] Mantener fuzzy match, scroll y orden alfabético actuales

### R-5: Chat panel — restyle
`src/app/renderer/chat.rs`. Mantener markdown, ACP/provider, botones de sugerencia, resize.
- [ ] Superficie del panel con tono propio distinto del terminal + inset para flotar
- [ ] Header como banda de superficie (`◈`/`✦` + labels ya existentes) con separación clara
- [ ] Botones de zero-state / sugerencia (`[ Fix last error ]`, `[ Explain command ]`) como filas rellenas redondeadas (`ui_surface_hover`, radius `R_PILL`) en vez de brackets de línea
- [ ] Campo de input como card inset (ya parcialmente hecho en W-2 — alinear a tokens R-1)
- [ ] Bloques de código y footer alineados a los radios/espaciado nuevos

### R-6: Tab bar — restyle
`src/ui/tabs.rs` + builder de tab bar en el renderer.
- [ ] Pills de tab activo/inactivo con radios y tonos de los tokens nuevos
- [ ] Espaciado entre tabs y contra el borde consistente con R-1
- [ ] Botón `+` y close `×` alineados al nuevo estilo

### R-7: Status bar — restyle
`src/ui/status_bar.rs` + `build_status_bar_instances`.
- [ ] Alinear tonos de los segmentos a la jerarquía de superficies nueva
- [ ] Mantener segmentos powerline (git branch, cwd, exit code, hora, leader)
- [ ] Espaciado/altura consistentes con tokens

### R-8: Float layout de la ventana
Dar a la zona de terminal + paneles un margen exterior para que todo "flote" sobre el fondo.
- [ ] Inset global del contenido (probablemente `src/app/layout.rs`) — gap uniforme
- [ ] Verificar que tabs/panes/splits siguen calculando filas/columnas correctamente con el inset
- [ ] Verificar hit-testing de mouse (resize de panes/panel, clicks) con el nuevo offset

### Fase 1 — Verificación
- [ ] `cargo build` + `cargo clippy -D warnings` + `cargo fmt --check` limpios
- [ ] `grep` sin literales de color nuevos hardcodeados en los builders tocados
- [ ] Revisión visual con `/run` de cada superficie (sidebar, palette, chat, tabs, status, float)
- [ ] Probar los 5 temas bundled (contraste de superficie correcto en cada uno)

---

## Fase 2 — Ventana translúcida & blur (plataforma macOS)

> ~15% restante. Único bloque con código específico de macOS. Opcional/aislado:
> si resulta inestable, la Fase 1 ya entrega un resultado completo.

### V-1: Ventana transparente + opacity
- [ ] Crear la ventana winit con `with_transparent(true)`
- [ ] Respetar `config.window.opacity` en el clear color de wgpu (alpha del surface)
- [ ] Formato de surface con alpha (`CompositeAlphaMode` apropiado en macOS/Metal)

### V-2: Blur / vibrancy nativo (NSVisualEffectView)
- [ ] Hook de plataforma (objc/cocoa) que inserta un `NSVisualEffectView` detrás de la capa Metal
- [ ] Config `window.blur = false | "dark" | "light"` (material vibrancy)
- [ ] Solo macOS; no-op en otras plataformas (feature-gate)

### V-3: Esquinas redondeadas de ventana
- [ ] Asegurar esquinas redondeadas coherentes con el material (si borderless, aplicar corner radius; con titlebar nativo ya vienen)
- [ ] El contenido respeta las esquinas (no dibuja fuera del radio)

### V-4: Tokens de superficie translúcidos
- [ ] Cuando blur activo, tonos de superficie con alpha < 1 para que el wallpaper se filtre sutilmente
- [ ] Alternar entre tokens opacos (sin blur) y semitransparentes (con blur) según config

### Fase 2 — Verificación
- [ ] Build + clippy + fmt limpios
- [ ] Verificación manual: blur on/off, opacity, cambio de tema con blur activo
- [ ] Sin regresiones de rendimiento (F12 HUD p50/p95/p99)

---

## Fuera de alcance (explícito)
- Explorador / árbol de archivos en la sidebar (el usuario no lo quiere).
- Encajonar la salida de comandos del terminal (no es real).
- Cambios de comportamiento en tabs/panes (funcionan bien, se conservan).

## Archivos centrales
| Archivo | Rol |
|---------|-----|
| `src/app/renderer/overlay.rs` | Sidebar izquierda + command palette |
| `src/app/renderer/chat.rs` | Chat panel |
| `src/app/renderer/mod.rs` | Tab bar / status bar builders, `RenderContext` (tokens) |
| `src/ui/tabs.rs`, `src/ui/status_bar.rs` | Modelos de tab/status |
| `src/app/layout.rs` | Float layout / inset global |
| `src/config/schema.rs`, `config/default/ui.lua` | `window.opacity`, `window.blur` |
| `assets/themes/*.lua` | Tonos de superficie por tema |
| `src/renderer/ui_style.rs` (nuevo) | Tokens de espaciado/radio (R-1) |
| plataforma macOS (nuevo) | NSVisualEffectView hook (V-2) |
