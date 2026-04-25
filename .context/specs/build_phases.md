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
- [ ] Agregar los 7 campos a `ColorScheme` con `#[serde(default)]` + función `derive_ui_colors(&self)` que los calcula desde los colores base cuando no se especifican
- [ ] Actualizar `dracula-pro.lua` con valores explícitos para los 7 tokens
- [ ] Actualizar los otros 4 temas bundled con valores coherentes (catppuccin, tokyo-night, gruvbox, one-dark)
- [ ] Reemplazar los ~17 literales en `renderer.rs` por referencias a `colors.ui_*`
- [ ] Exponer los tokens en la API Lua de temas (documentar en `config/default/ui.lua`)

### G-1: Maximizar / minimizar panes
- [ ] `Leader z` — zoom del pane activo (ocupa todo el área de terminal, oculta separadores)
- [ ] Segundo `Leader z` — restaura layout anterior
- [ ] Estado `zoomed_pane: Option<usize>` en `Mux`; renderer omite otros panes cuando activo
- [ ] Indicador visual en status bar cuando hay zoom activo

### G-2: Sidebar — MCP, Steering, Skills
- [ ] Panel lateral colapsable (drawer estilo VSCode) en el lado izquierdo
- [ ] Tab "MCP": lista de servidores conectados + tools disponibles por servidor
- [ ] Tab "Steering": lista de archivos cargados desde `~/.config/petruterm/steering/` y `<cwd>/.petruterm/steering/`; permite abrir en editor
- [ ] Tab "Skills": lista de skills cargados con nombre + descripción; filtro fuzzy
- [ ] Keybind `Leader s` para abrir/cerrar; navegación con `j/k`, `Enter` para acción

### G-3: Markdown en chat
- [ ] Parser Markdown inline: `**bold**`, `*italic*`, `` `code` ``, `# headings`
- [ ] Bloques de código con resaltado de sintaxis (al menos: rs, py, js, ts, sh, json)
- [ ] Listas (`-`, `1.`) con indentación correcta
- [ ] Render en GPU: mapear spans a colores del tema activo
- [ ] Ancho de wrap respeta el ancho del panel (`PANEL_COLS`)
