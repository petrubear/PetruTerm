# PetruTerm — Build Phases

> Fases 0.5–3.6 + A–E + D-1/D-2/D-3/D-4/D-5 + Phase 4 archivadas en [`build_phases_archive.md`](./build_phases_archive.md).
> **Phase 4 (Plugin Ecosystem) — CANCELADA** (2026-04-28). Decisión: los plugins no son necesarios de momento.

---

## Phase 5: UX Polish

### G-0: Sistema de temas — UI tokens
Actualmente `ColorScheme` solo cubre colores de terminal (fg, bg, cursor, ANSI 0-15).
Toda la chrome de la app (panel de chat, toast, sidebar, palette, separadores, overlays)
tiene ~17 colores hardcodeados en `renderer.rs` que ignoran el tema activo.

**Tokens semánticos a agregar en `ColorScheme`:**
| Token | Uso | Derivado de (default) |
|---|---|---|
| `ui_accent` | Borde foco pane, borde toast, sidebar accent, header accent | `cursor_bg` |
| `ui_surface` | Bg panels, sidebar, palette, chat header | `background` + 15% más claro |
| `ui_surface_active` | Item seleccionado en palette/sidebar | `selection_bg` |
| `ui_surface_hover` | Item hover en palette/sidebar/context menu | `background` + 8% más claro |
| `ui_muted` | Separadores, texto secundario | `foreground` al 35% alpha |
| `ui_success` | Confirm "yes", indicadores positivos | `ansi[2]` (green) |
| `ui_overlay` | Bg toast, modales semitransparentes | `background` + alpha 0.95 |

**Pasos:**
- [x] Agregar los 7 campos a `ColorScheme` con `#[serde(default)]` + función `derive_ui_colors(&self)` que los calcula desde los colores base cuando no se especifican
- [x] Actualizar `dracula-pro.lua` con valores explícitos para los 7 tokens
- [x] Actualizar los otros 4 temas bundled con valores coherentes (catppuccin, tokyo-night, gruvbox, one-dark)
- [x] Reemplazar los ~17 literales en `renderer.rs` por referencias a `colors.ui_*`
- [x] Exponer los tokens en la API Lua de temas (documentar en `config/default/ui.lua`)

### G-1: Maximizar / minimizar panes
- [x] `Leader z` — zoom del pane activo (ocupa todo el área de terminal, oculta separadores)
- [x] Segundo `Leader z` — restaura layout anterior
- [x] Estado `zoomed_pane: Option<usize>` en `Mux`; renderer omite otros panes cuando activo
- [x] Indicador visual en status bar cuando hay zoom activo

### G-2: Sidebar — MCP, Steering, Skills
Extiende la sidebar izquierda existente (`Leader e e`) con tres secciones fijas debajo del árbol
de workspaces. Proporciones fijas: 40% workspace, 20% MCP, 20% Skills, 20% Steering.
Scroll independiente por sección cuando los items no caben. Sección vacía muestra placeholder dimmed.

**Foco y navegación:**
- `Leader e e` abre el drawer con foco en Workspaces (comportamiento actual intacto)
- `j/k` navega solo dentro de la sección activa
- `Tab` / `Shift+Tab` cambia la sección activa (cíclico)
- `Enter` en Workspaces ya funciona (abre el workspace seleccionado)

**Contenido de cada sección:**
- `WORKSPACES` — árbol actual (sin cambios)
- `MCP SERVERS` — lista de servidores conectados con sus tools indentadas; placeholder "no servers connected" si vacío
- `SKILLS` — nombre + descripción (dos líneas por item); placeholder "no skills loaded"
- `STEERING` — archivos desde `~/.config/petruterm/steering/` y `<cwd>/.petruterm/steering/`; placeholder "no steering files"

**Enter en secciones MCP / Skills / Steering — PENDIENTE (G-2-overlay):**
- Emite `InfoSidebarSelection { kind: Mcp/Skill/Steering, name: String }` pero no abre overlay aún
- El match arm queda wired con `log::debug!` como placeholder

- [x] Ampliar `build_workspace_sidebar_instances` con las 3 secciones nuevas (proporciones 40/20/20/20)
- [x] Scroll offset por sección (`mcp_scroll`, `skills_scroll`, `steering_scroll`) en `App`
- [x] `info_sidebar_section: u8` para sección activa; Tab/Shift+Tab la cambia
- [x] Steering files via `SteeringManager.files()` (ya cargado en `UiManager`)
- [x] Enter en secciones 1-3 → `log::debug!` placeholder (G-2-overlay pendiente)
- [x] `Leader s` como alias alternativo para abrir/cerrar (además de `Leader e e`)

**G-2-overlay (COMPLETO 2026-04-28):** Enter en item seleccionado abre overlay centrado mostrando:
- MCP → tools con descripcion + JSON schema (markdown con syntax highlight)
- Skill → body del SKILL.md (markdown con syntax highlight)
- Steering → contenido del archivo (markdown con syntax highlight)
- j/k scroll, Esc cierra. Cursor highlight en item seleccionado del sidebar.

### G-3: Markdown en chat
- [x] Parser Markdown inline: `**bold**`, `*italic*`, `` `code` ``, `# headings`
- [x] Bloques de código con resaltado de sintaxis (al menos: rs, py, js, ts, sh, json)
- [x] Listas (`-`, `1.`) con indentación correcta
- [x] Render en GPU: mapear spans a colores del tema activo
- [x] Ancho de wrap respeta el ancho del panel (`PANEL_COLS`)

---

## Phase 6: Warp-inspired Chat & Sidebar UI

> Spec completo en [`.context/specs/warp_ui_improvements.md`](./warp_ui_improvements.md).
> Fuente de inspiración: código fuente de Warp (`app/src/ai_assistant/`, `workspace/view/left_panel.rs`).

### W-1: Full-width message background tinting
- [x] User message rows → warm tint rect (flat `RoundedRectInstance`, radius 0, alpha ~8% blend toward `user_fg`)
- [x] Assistant message rows → base `ui_surface` bg (or cool 5% tint toward `ui_accent`)
- [x] Rects pushed before glyph vertices (painter's order)
- [x] Text prefixes `"   You  "` / `"    AI  "` simplificados o eliminados (bg distingue el rol)

### W-2: Input box as a bordered card
- [x] `RoundedRectInstance` detrás de filas de input (`sep_row+1` a `screen_rows-2`), radius 4px, bg = `ui_surface` + 10% más claro, border 1px `ui_muted`
- [x] Separador `sep_row` se vuelve más fino/tenue (1px, `ui_muted` 50% alpha)
- [x] Fila de hint (`screen_rows-1`) permanece fuera del card

### W-3: Code block background + left accent bar
- [x] Detectar spans de código en wrap cache (`ParseState::CodeBlock`)
- [x] Rect de fondo `ui_surface_active` por bloque de código, radius 3px
- [x] Barra vertical de 2px en borde izquierdo del bloque, color `ui_accent` 80% alpha
- [x] Hint `[c]` al final del último renglón del bloque (color `ui_muted`)

### W-4: Sidebar active/inactive color contrast
- [x] Headers de sección activa → color `foreground` (full brightness)
- [x] Headers de sección inactiva → `ui_muted` (foreground 35% alpha)
- [x] Items de sección activa → `foreground`; sección inactiva → `ui_muted`
- [x] Punto de acento (`sidebar_dot_active`) solo en el item activo, no en headers

### W-5: Zero state — empty panel
- [x] Cuando `messages.is_empty() && state == Idle`: renderizar estado vacío centrado
- [x] Fila centro-2: `"  ✦  "` en `ui_accent` (carácter icono grande, centrado en ancho de panel)
- [x] Fila centro-1: `"  Ask a question below  "` en `ui_muted`, centrado
- [x] Fila centro+1/+2: pills `"[ fix last error ]"` y `"[ explain command ]"` clicables, bg `ui_surface_hover`, radius 4px
- [x] Estado `zero_state_hover: Option<u8>` en `ChatPanel` para hover visual
- [x] Click en pill → pre-fill input (+ submit)

### W-6: Header — icon anchor + right-aligned action buttons
- [ ] Zona izquierda (~10 cols): glyph `✦` + model short-name en `ui_accent`
- [ ] Zona central: `provider:model` en `ui_muted`
- [ ] Zona derecha (~15 cols, solo cuando hay mensajes): `[↺]` restart, `[⎘]` copy, `[✕]` close — cada uno clicable
- [ ] Mapear clicks de zona derecha a acciones existentes en `UiManager`

### W-7: Prepared response pill buttons
- [ ] Campo `show_suggestions: bool` en `ChatPanel`, activado en transición `Streaming → Idle`
- [ ] Cuando `show_suggestions`: 2 filas pill después del último mensaje asistente: `"[ Fix last error ]"` y `"[ Explain more ]"`
- [ ] Click en pill → fill input + submit; cualquier otro input → `show_suggestions = false`
- [ ] Descontar 2 filas del área de mensajes cuando `show_suggestions == true`

### W-8: Resizable panel width via mouse drag
- [ ] `panel_cols: u16` en `ChatPanel` (reemplaza constante `PANEL_COLS = 55`, default 55, clamp 30–90)
- [ ] `panel_resize_drag: bool` en `UiManager`
- [ ] Detectar mouse en borde izquierdo del panel (±1 celda): render borde en `ui_accent`
- [ ] `MouseButton::Left` press en borde → `panel_resize_drag = true`
- [ ] `CursorMoved` con drag activo → `panel.panel_cols = (screen_cols - cursor_col).clamp(30, 90)`, mark dirty
- [ ] `MouseButton::Left` release → `panel_resize_drag = false`
- [ ] Reemplazar todas las referencias a `PANEL_COLS` con `panel.panel_cols`
