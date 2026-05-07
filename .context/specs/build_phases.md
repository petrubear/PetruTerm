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
- [x] Zona izquierda (~10 cols): glyph `✦` + model short-name en `ui_accent`
- [x] Zona central: `provider:model` en `ui_muted`
- [x] Zona derecha (~15 cols, solo cuando hay mensajes): `[↺]` restart, `[⎘]` copy, `[✕]` close — cada uno clicable
- [x] Mapear clicks de zona derecha a acciones existentes en `UiManager`

### W-7: Prepared response pill buttons
- [x] Campo `show_suggestions: bool` en `ChatPanel`, activado en transición `Streaming → Idle`
- [x] Cuando `show_suggestions`: 2 filas pill antes de sep_row: `"[ Fix last error ]"` y `"[ Explain more ]"`
- [x] Click en pill → fill input + submit; cualquier otro input → `show_suggestions = false`
- [x] Descontar 2 filas del área de mensajes cuando `show_suggestions == true`

### W-8: Resizable panel width via mouse drag
- [x] `panel_resize_drag: bool` y `panel_resize_hover: bool` en `App`
- [x] Detectar mouse en borde izquierdo del panel (±1 celda): render línea 2px en `ui_accent`
- [x] `MouseButton::Left` press en borde → `panel_resize_drag = true`
- [x] `CursorMoved` con drag activo → `panel.width_cols = clamp(30..90)`, mark dirty, resize terminals
- [x] `MouseButton::Left` release → `panel_resize_drag = false`

---

## Phase 7: Warp-inspired Terminal Intelligence

> Inspirado en el código fuente de Warp. Priorizado de más simple a más complejo.
> Fuentes: `warp_terminal/src/model/ansi/`, `warpui_core/src/elements/hoverable.rs`, `crates/ai/src/agent/`.

### H-1: Hover links — URLs, paths, stack traces clicables
**Complejidad: Baja.** PetruTerm ya tiene mouse tracking y context menu. Solo se necesita
un scanner de celdas en hover y un highlight rect.

**Alcance:**
- Detectar en `CursorMoved`: escanear la fila lógica completa bajo el cursor buscando:
  - URLs (`https?://[^\s]+`)
  - Paths absolutos (`/[^\s]+`) y relativos con extensión (`[^\s]+\.[a-z]{1,5}`)
  - Stack traces estilo Rust (`src/foo/bar.rs:123:45`)
- Guardar `hover_link: Option<HoverLink>` en `App` con rango de columnas + tipo + texto
- Renderer: subrayado de 1px en `ui_accent` sobre las celdas del rango
- Click izquierdo sobre link: `open` (macOS) para URLs; abrir en `$EDITOR` para paths
- Context menu: agregar ítem "Open link" / "Copy path" cuando hay `hover_link` activo

**Archivos afectados:** `src/app/mod.rs`, `src/app/renderer.rs`, `src/ui/context_menu.rs`

**Pasos:**
- [x] `HoverLink { start_col, end_col, row, kind: HoverLinkKind, text: String }` en `src/app/hover_link.rs`
- [x] `scan_link_at(row_text, cursor_col) -> Option<(col_start, col_end, kind, text)>` — parser manual sin regex
- [x] `CursorMoved`: llama al scanner via `mux.viewport_row_text(row)`, actualiza `hover_link`, redraw si cambió
- [x] Renderer: underline 1.5px en `ui_accent` (rect pre-computado antes del borrow mutable de rc)
- [x] `MouseButton::Left Pressed`: si hover_link activo → `open <path_or_url>`, return (no selección)
- [x] Context menu: `open_with_link` muestra "Open Link" + "Copy Link" al hacer right-click sobre link

---

### B-1 a B-4: OSC 133 — Command Blocks — COMPLETAS (2026-05-05, bugs corregidos 2026-05-06)
**Complejidad: Media.** Implementadas vía scanner de bytes raw en el PTY reader thread
antes de `vte::ansi::Processor::advance()`, más metadatos de bloque por pane.

**Contexto del protocolo OSC 133 (semantic prompts):**
```
OSC 133 ; A ST   — prompt start (inicio del prompt)
OSC 133 ; B ST   — command start (usuario terminó de tipear, start de output)
OSC 133 ; C ST   — output start (igual que B en la mayoría de shells)
OSC 133 ; D ; N ST — command end (N = exit code)
```
El shell emite estos via `precmd` / `preexec` hooks (zsh, bash, fish lo soportan nativamente
o con un snippet de 3 líneas en `.zshrc`).

**Estructura de datos:**
```rust
struct Block {
    id: usize,
    prompt_row: i32,     // fila grid donde empieza el prompt
    output_start: i32,   // fila donde empieza el output
    output_end: Option<i32>, // None si aún streamea
    exit_code: Option<i32>,
    command_text: String,    // capturado entre A y B
}
```

#### B-1: Parser OSC 133 en el VTE handler — COMPLETA
- [x] En `src/term/mod.rs`, interceptar `TermEvent` o hook en el `EventListener` de alacritty
- [x] Detectar secuencias `OSC 133 ; A/B/C/D` en el stream de eventos del terminal
- [x] Emitir `TermEvent::Osc133(marker: Osc133Marker)` hacia `App`
- [x] Capturar texto del comando entre marcador A y B leyendo el grid

#### B-2: Block manager por pane — COMPLETA
- [x] `BlockManager` en `src/term/blocks.rs`: `Vec<Block>`, `current_block: Option<Block>`
- [x] `on_marker(marker, current_row)` — actualiza estado del bloque activo
- [x] Cada pane (`Terminal`) tiene su `BlockManager`
- [x] `blocks_in_viewport(history_size, display_offset, rows) -> Vec<&Block>` para el renderer

#### B-3: Render visual de bloques — COMPLETA
- [x] Renderer: rect sutil de fondo por bloque en viewport (alpha 6%, `ui_surface`)
- [x] Indicador de exit code: pill verde (`ui_success`) / rojo en la última fila del bloque (2 cols del borde derecho)
- [x] No renderizar bloque activo (sin output_end) — solo bloques completos
- ~~Gutter izquierdo 2px~~ eliminado — no aportaba valor visual

#### B-4: Operaciones sobre bloques — COMPLETA
- [x] Hover sobre cualquier fila del bloque → highlight (`ui_surface_hover`)
- [x] Right-click sobre exit-code pill → context menu de bloque: "Copy Output", "Re-run Command", "Clear", "Copy", "Paste", "Ask AI"
- [x] `Leader y` — copiar output del bloque bajo el cursor al clipboard
- [x] `Leader r` — re-ejecutar el comando del bloque bajo el cursor

**Notas de implementación (no obvias):**
- `shell-integration.zsh` emite `B;<cmd>` (comando embebido en el OSC) para evitar capturar PS1 del grid.
- `block_at_absolute_row` usa `iter().rev()` — cuando `D` y `A` comparten fila, el bloque más nuevo gana.
- `block_indicator_at_pixel` (hit-test del pill) es independiente de `hover_block` (highlight visual).
- `clear block` eliminado — `BlockManager::remove_block` también eliminado.

---

### A-1 a A-3: AI Agent Actions
**Complejidad: Media-alta.** El panel de chat ya existe y los providers LLM están conectados.
Se necesita un schema de acciones estructuradas y handlers en el terminal.

**Concepto:** El AI responde con acciones tipadas además de texto. PetruTerm las detecta,
muestra una confirmación inline, y las ejecuta si el usuario acepta.

**Tipos de acción:**
```rust
enum AgentAction {
    RunCommand { cmd: String, explanation: String },
    OpenFile    { path: String },
    ExplainOutput { last_n_lines: usize },
}
```

#### A-1: Schema de acciones + parser en respuestas LLM — COMPLETA (2026-05-06)
- [x] `src/llm/agent_action.rs`: enum `AgentAction` + parser de tags `<action>...</action>` en respuestas LLM
- [x] System prompt instruye al LLM a emitir acciones en JSON entre `<action>` tags
- [x] `parse_action_from_response(&str) -> Option<AgentAction>`

#### A-2: Confirm UI inline en el chat panel — COMPLETA (2026-05-06)
- [x] `PanelState::ConfirmAction(AgentAction)` — nuevo estado en el chat panel
- [x] Renderer: card de confirmación sobre `sep_row` con descripción de la acción + pills `[Run] [Cancel]`
- [x] Teclado: `y` / `Enter` confirma, `n` / `Esc` cancela
- [x] "Always allow" checkbox: `always_allow_actions: bool` en `ChatPanel`

#### A-3: Action handlers — COMPLETA (2026-05-06)
- [x] `RunCommand`: write al PTY del pane activo
- [x] `OpenFile`: `open -e <path>` en macOS
- [x] `ExplainOutput`: captura N líneas del grid, nueva query al LLM
- [x] Post-ejecución: append mensaje de sistema al transcript

---

### I-1 a I-4: Input Decoration Layer
**Complejidad: Alta.** Requiere interceptar el input antes del PTY y mantener un buffer
de edición paralelo al del shell. Riesgo de divergencia con el estado real del shell.

**Nota arquitectónica:** alacritty_terminal envía keystrokes directamente al PTY. Para decorar
el input necesitamos un "shadow buffer" que refleje lo que el usuario está tipeando, sin romper
el flujo PTY. OSC 133 (B-1) provee los límites de prompt/command que hacen esto viable.

#### I-1: Shadow input buffer — COMPLETA
- [x] `InputShadow { buf: String, cursor: usize, active: bool }` por pane
- [x] Activar cuando OSC 133-A recibido (estamos en zona de prompt)
- [x] Desactivar en OSC 133-B (command enviado)
- [x] Interceptar `KeyboardInput` en `handle_keyboard`: actualizar `InputShadow` en espejo
  (no reemplaza el envío al PTY, solo lo replica para decoración)
- [x] Reset en `Ctrl+C`, `Ctrl+U`, y en OSC 133-B/D

#### I-2: Syntax coloring del comando — COMPLETA
- [x] `tokenize_command(input: &str) -> Vec<(Range<usize>, TokenKind)>`
  - `TokenKind`: `Command`, `Arg`, `Flag`, `Pipe`, `Redirect`, `String`, `Arg`
- [x] Colorear sobre las celdas del grid que coincidan con el shadow buffer
  (overlay de color — mismo mecanismo que selection highlight)
- [x] Resolver si el primer token es un comando válido: buscar en `$PATH` (cache, no bloqueante)
- [x] Comando no encontrado → color rojo

#### I-3: Ghost text — inline completion hints — COMPLETA
- [x] `HistoryIndex::load()` en `src/term/tokenizer.rs` — lee `~/.zsh_history` o `~/.bash_history`; most-recent-first
- [x] `InputShadow.ghost: Option<String>` — suffix actualizado en cada keypress cuando cursor al final del buf
- [x] `GhostOverlay` en `mux.rs` — reemplaza chars + aplica `ui_muted` fg en el viewport row del cursor
- [x] Damage-skip nunca omite el ghost row (se redibuja en cada keypress)
- [x] `Tab` o `ArrowRight` al final del buffer: `accept_ghost()` escribe el sufijo al PTY

#### I-4: Flag hints — tooltips de flags — COMPLETA
- [x] `src/term/flag_db.rs`: `lookup_flag(cmd, flag) -> Option<&'static str>` — 10 comandos (ls, git, cargo, grep/rg, docker, kubectl, ssh, curl, tar, find)
- [x] `FlagHintOverlay` en `mux.rs` — similar a `GhostOverlay` pero en `cursor.row + 1`; aligned con la posición del flag en el grid
- [x] Hint format: `"<flag>  <description>"` en color `ui_muted`
- [x] Aparece cuando el último token es `TokenKind::Flag` y `lookup_flag` retorna Some
- [x] Cierra solo (el último token deja de ser Flag cuando el usuario sigue tipeando; Esc limpia el buf)
