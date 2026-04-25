# PetruTerm — Build Phases

> Fases 0.5–3.6 + A–E + D-1/D-2/D-3/D-4/D-5 + Phase 4 archivadas en [`build_phases_archive.md`](./build_phases_archive.md).

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

**G-2-overlay (futuro):** Enter en item seleccionado abre overlay tipo paleta mostrando:
- MCP → contenido del `mcp.json` de ese servidor
- Skill → contenido del `SKILL.md` (sin assets)
- Steering → contenido del archivo steering seleccionado

### G-3: Markdown en chat
- [ ] Parser Markdown inline: `**bold**`, `*italic*`, `` `code` ``, `# headings`
- [ ] Bloques de código con resaltado de sintaxis (al menos: rs, py, js, ts, sh, json)
- [ ] Listas (`-`, `1.`) con indentación correcta
- [ ] Render en GPU: mapear spans a colores del tema activo
- [ ] Ancho de wrap respeta el ancho del panel (`PANEL_COLS`)
