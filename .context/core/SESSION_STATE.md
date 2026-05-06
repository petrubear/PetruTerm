# Session State

**Last Updated:** 2026-05-05
**Session Focus:** Phase 7 — B-4 completa

## Branch: `master` (siguiente: `feat/phase-7`)

## Estado actual

**Phases 1–6 + Auditoría COMPLETAS. master limpio.**
**Sin deuda técnica abierta. Diferidos: TD-PERF-03/05 (solo GPUs discretas).**

## Esta sesión (2026-05-05) — Phase 7: H-1 + B-1 + B-2 + B-3 + B-4 COMPLETAS

### B-4: Operaciones sobre bloques — COMPLETA
**Archivos modificados:**
- `src/term/blocks.rs`: `block_at_absolute_row`, `find_block_by_id`, `remove_block` añadidos a `BlockManager`.
- `src/app/mux.rs`: `block_output_text(terminal_id, block_id)` — extrae texto del grid entre `output_start` y `output_end` del bloque.
- `src/ui/context_menu.rs`: `ContextAction::CopyBlockOutput(tid, bid)`, `ReRunCommand(String)`, `ClearBlock(tid, bid)` + método `open_with_block(...)`.
- `src/app/mod.rs`:
  - Campo `hover_block: Option<(usize, usize)>`.
  - `block_at_cursor(x, y)` — detecta si el mouse está en la zona gutter (primer `cell_w` px) de un pane y hay un bloque en esa fila.
  - `copy_hover_block_output()` / `rerun_hover_block_command()` — helpers para leader y/r.
  - `handle_mouse_motion`: detección de hover de bloque tras detección de hover link.
  - `handle_keyboard`: intercepción de `Leader+y` y `Leader+r` antes de `input.handle_key_input`.
  - Right-click: si `hover_block` está activo → `open_with_block` en lugar del menú estándar.
  - Context menu dispatch: arms para `CopyBlockOutput`, `ReRunCommand`, `ClearBlock`.
- `src/app/renderer.rs`: `build_block_instances` acepta `hover_block: Option<(usize, usize)>`. El bloque hovered usa `ui_surface_hover` al 14% alpha en vez del 6% estándar.

**Decisiones:**
- Gutter hover usa el primer `cell_w` píxeles del pane (más grande que los 2px de la barra, mejor UX).
- `block_output_text` usa `absolute_row - history_size` como índice de grid (`Line(idx)`), estable bajo scrolling.
- `Leader y` / `Leader r` interceptados en `App::handle_keyboard` antes de delegar a `input::handle_key_input` porque necesitan acceso a `self.hover_block` (estado del App, no del input handler).

### B-3: Render visual de bloques — COMPLETA
**Archivos modificados:**
- `src/app/renderer.rs`: nuevo método `build_block_instances(pane_infos, mux, colors)`.
  - Itera bloques visibles via `blocks_in_viewport` de cada terminal.
  - Background rect: `ui_surface` al 6% alpha, ancho completo del pane.
  - Left gutter: 2px × altura del bloque, `ui_muted`, radius 1px.
  - Exit indicator: pill 1.2×0.6 celdas, `ui_success` verde (exit=0) / rojo (exit≠0),
    posicionado 2 celdas del borde derecho, centrado verticalmente en la última fila.
- `src/app/mod.rs`: llamada a `build_block_instances` después del focus border y antes del link underline.

**Coordenadas:**
- `block_y = pane_rect.y + vp_start * cell_h` (pane_rect ya en pixel coords absolutos).
- Conversión: `vp_start = (absolute_row - history_size + display_offset).clamp(0, rows-1)`.

## Esta sesión (2026-05-05) — Phase 7: H-1 + B-1 COMPLETAS

### B-2: Block manager por pane — COMPLETA
**Archivos nuevos/modificados:**
- `src/term/blocks.rs` (nuevo): `Block` + `BlockManager`.
  - Rows almacenados como `absolute_row = history_size + cursor_vp - display_offset` — estable bajo scrolling.
  - `on_marker(marker, absolute_row, command_text)` — maneja A/B/C/D.
  - `blocks_in_viewport(history_size, display_offset, rows) -> Vec<&Block>` — solo bloques completos.
  - `evict_old(history_size)` — limpieza de bloques fuera del scrollback.
  - 5 unit tests.
- `src/term/mod.rs`: `pub mod blocks`, re-export `BlockManager`, campo `block_manager: BlockManager` en `Terminal`, inicializado en `Terminal::new`.
- `src/app/mux.rs`: `Mux::apply_osc133_events()` — drena `osc133_events`, captura `absolute_row` via `renderable_content()`, captura `command_text` al `CommandStart`, llama `block_manager.on_marker`.
- `src/app/mod.rs`: `apply_osc133_events()` llamado en los 3 call sites de `poll_pty_events()`.

**Decisión arquitectónica:**
Rows guardados como `history_size + viewport_cursor_row - display_offset`. Permite que los bloques sobrevivan scrolling: al avanzar el historial, `history_size` crece en la misma cantidad que el cursor se desplaza, manteniendo el `absolute_row` estable. Conversión inversa: `viewport_row = absolute_row - history_size + display_offset`.

## Esta sesión (2026-05-05) — Phase 7: H-1 + B-1 COMPLETAS

### B-1: OSC 133 parser — COMPLETA
**Archivos nuevos/modificados:**
- `src/term/osc133.rs` (nuevo): `Osc133Marker` enum + `Osc133Scanner` state machine.
  Reconoce `ESC]133;A/B/C/D` con BEL o ST terminator. 5 unit tests.
- `src/term/pty.rs` (reescrito): PTY custom con `libc::openpty`. Reader thread escanea
  bytes raw para OSC 133 antes de `vte::ansi::Processor::advance()`. `PtyEventProxy`
  usa raw fd para PtyWrite (no más `OnceLock<Notifier>`). Nuevo `PtyEvent::Osc133`.
- `src/term/mod.rs`: `Terminal::new()` delega creación de Term a `Pty::spawn()`.
- `src/app/mux.rs`: `Mux::osc133_events: Vec<(usize, Osc133Marker)>` acumula
  markers por ciclo de poll. Listo para B-2 (BlockManager).

**Decisión arquitectónica clave:**
`alacritty_terminal` 0.25.1 no parsea OSC 133 (cae en "unhandled osc_dispatch").
La única forma de interceptar es a nivel de bytes crudos antes del VTE. Esto requirió
reemplazar `PtyEventLoop` con un loop propio usando `libc::openpty` + reader thread.

## Esta sesión (2026-05-05) — Phase 7: H-1 COMPLETA

### Chat panel background (fix menor)
- `renderer.rs:833` y `renderer.rs:1818`: `config.llm.ui.background` → `config.colors.background`

### H-1: Hover links — COMPLETA
**Archivos nuevos/modificados:**
- `src/app/hover_link.rs` (nuevo): `HoverLink`, `HoverLinkKind`, `scan_link_at`, `path_for_open`
- `src/app/mux.rs`: `viewport_row_text(row)` — lee texto de fila visible respetando display_offset
- `src/app/mod.rs`: campo `hover_link: Option<HoverLink>`, detección en `handle_mouse_motion`,
  apertura en `handle_mouse_button` Left, context menu en Right, underline rect en render
- `src/ui/context_menu.rs`: `ContextAction::OpenLink(String)`, `ContextAction::CopyLink(String)`,
  método `open_with_link`

**Comportamiento:**
- Hover sobre URL/path/stack-trace → underline 1.5px en `ui_accent`
- Click izquierdo → `open <url_or_path>` (macOS), prioridad sobre selección
- Click derecho → context menu con "Open Link" + "Copy Link" en la parte superior
- Detección solo en área de terminal (no panel, no sidebar, no context menu visible)
- `path_for_open` stripea `:line:col` antes de llamar a `open`
- 5 unit tests en `src/app/hover_link.rs`

## Esta sesión (2026-05-05) — Wave 4 de auditoría

### AUDIT-REFAC-03 — estado del sidebar agrupado
- Nuevo `src/ui/sidebar.rs` con `SidebarState`.
- `App` ahora concentra `visible`, `nav_cursor`, `panel_resize_drag`, `panel_resize_hover`, `rename_input`, `keyboard_active`, `active_section`, `mcp_scroll`, `skills_scroll`, `steering_scroll` bajo `self.sidebar`.

### AUDIT-REFAC-02 — split de `build_chat_panel_instances`
- `src/app/renderer.rs`: extraídos `build_panel_header()`, `build_panel_file_section()` y `build_panel_messages()`.
- `build_chat_panel_input_rows()` quedó separado como antes; se preservó `fmt_buf` reutilizable del Wave 3.

### AUDIT-REFAC-01 — split de `window_event()`
- `src/app/mod.rs`: extraídos `handle_redraw()`, `handle_keyboard()`, `handle_mouse_motion()`, `handle_mouse_button()` y `handle_scroll()`.
- `window_event()` ahora delega sin cambiar orden de eventos ni comportamiento.

### AUDIT-CLEAN-02 — evaluado, sin cambio
- `ContextAction` sigue con un número bajo de variantes; no se justificó introducir un dispatch table.

## Esta sesión (2026-05-05) — Wave 3 de auditoría

### AUDIT-PERF-02 — `build_chat_panel_instances` sin `format!()` hot-path
- `src/app/renderer.rs`: composición de strings calientes movida a `fmt_buf` reutilizable.
- Header, file picker, previews, zero-state, suggestion pills y filas de input/hints dejaron de crear `String` temporales por frame mediante `format!()`.

### AUDIT-ENERGY-01 — redraw deduplicado por ciclo
- `src/app/mod.rs`: nuevo flag `needs_redraw: bool` en `App`.
- `request_redraw()` ahora solo marca el flag; `flush_redraw_request()` emite `window.request_redraw()` una sola vez en `about_to_wait()`.
- Se preserva el coalescing de PTY y el redraw continuo de toast/blink, pero con un solo request por iteración del loop.

## Esta sesión (2026-05-05) — Clippy fixes + deuda cerrada

### Clippy (3 errores)
- `renderer.rs:2514-2515`: `(x.min(a)).max(b)` → `x.clamp(b, a)` (dos instancias)
- `notifications.rs:37`: `&**content` → `&content` (explicit_auto_deref)

### bundle.sh — NSUserNotificationUsageDescription
- Agregada clave `NSUserNotificationUsageDescription` al Info.plist generado por `scripts/bundle.sh`.
  Sin esta clave el OS ignora la solicitud de permiso de `UNUserNotificationCenter` → las notificaciones nativas nunca aparecen. Cierra definitivamente TD-UX-01.

### TECHNICAL_DEBT.md
- TD-PERF-29 marcado RESUELTO: mimalloc ya estaba implementado en `main.rs:13-15` + `Cargo.toml:108`. La nota "diferido" era incorrecta.
- TD-UX-01 actualizado con el fix de bundle.sh.
- Nota "benches bloqueados" corregida: todos los benches corren. `build_frame_miss` 32µs, `rasterize_line_ascii` 31µs, `shape_line_unicode` 5.2µs.

## Esta sesión (2026-05-04) — Notificaciones nativas + bug fix

### Notificaciones nativas (TD-UX-01)

**`Cargo.toml`:** dependencias `objc2-user-notifications 0.2` + `block2 0.5`.

**`src/platform/notifications.rs`** (nuevo):
- `send(body)` — entrega notificacion via `UNUserNotificationCenter` en macOS; no-op en otras plataformas.
- Solicita permiso (alert + sound) en el primer envio; sin-op si ya fue decidido.
- Identificador fijo `"petruterm.toast"` — notificaciones rapidas se reemplazan entre si.

**`src/platform/mod.rs`:** expone `pub mod notifications`.

**`src/config/schema.rs`:**
- `NotificationStyle` enum: `Toast` (default) | `Native`.
- `NotificationsConfig { style }` + `Config.notifications`.

**`config/default/notifications.lua`** (nuevo): modulo default con `style = "toast"`.

**`config/default/config.lua`:** agrega `require("notifications")` + `notifications.apply_to_config(config)`.

**`src/app/mod.rs`:** `dispatch_notification(msg, ms)` — despacha a `platform::notifications::send` si `style == Native`, o al toast GPU si `style == Toast`.

### Bug fix: config loader crasheaba por notifications.lua faltante

**Causa raiz:** `notifications.lua` no estaba en `EMBEDDED_MODULES` ni en `ensure_default_configs`. El loader Lua fallaba al hacer `require("notifications")` → `load()` retornaba `Err` → fallback a `Config::default()` con font `"JetBrainsMono Nerd Font Mono"` → crash porque esa fuente no esta instalada.

**`src/config/mod.rs`:**
- `DEFAULT_NOTIFICATIONS` embedded via `include_str!`.
- Agregado a `EMBEDDED_MODULES` para el fallback sin filesystem.
- Agregado a `ensure_default_configs` para que se escriba en `~/.config/petruterm/` en instalaciones nuevas.

## Esta sesión (2026-04-30) — Phase 6 W-7 + W-8

### W-7: Prepared response pill buttons

**`src/llm/chat_panel.rs`:**
- Campos `show_suggestions: bool` y `suggestion_hover: Option<u8>` en `ChatPanel`
- `mark_done()` activa `show_suggestions = true` al completar streaming
- `submit_input()`, `type_char()`, `backspace()`, `close()`, `clear_messages()` resetean `show_suggestions`

**`src/app/renderer.rs` — `build_chat_panel_instances`:**
- `suggestion_rows = 2` cuando `show_suggestions && !messages.is_empty() && Idle`
- `effective_history_rows = history_rows - suggestion_rows` — reduce el área de mensajes
- Pills fijas en `sep_row-2` y `sep_row-1`: "[ Fix last error ]" y "[ Explain more ]"
- Mismo patrón visual que W-5 (border outer + fill inner, hover en `ui_accent`/`ui_surface_active`)

**`src/app/mod.rs`:**
- `suggestion_hover_for_row(panel_row)` — hits `sep_row-2` y `sep_row-1`
- `CursorMoved`: tracking hover de suggestion pills, marca dirty si cambia
- `MouseInput Left Pressed`: click en pill → pre-fill input + submit, `show_suggestions = false`

### W-8: Resizable panel width via mouse drag

**`src/app/mod.rs`:**
- Campos `panel_resize_drag: bool` y `panel_resize_hover: bool` en `App`
- `near_panel_left_edge(x)` — true cuando x está dentro de 1 celda del borde izquierdo del panel
- `CursorMoved`: si `panel_resize_drag` → `panel.width_cols = ((right_edge - x) / cell_w).clamp(30, 90)` + `resize_terminals_for_panel()`; si no, actualiza hover
- `MouseInput Left Pressed`: si near edge → `panel_resize_drag = true` y return (antes del separator hit-test)
- `MouseInput Left Released`: si `panel_resize_drag` → reset + `resize_terminals_for_panel()`
- Render: línea 2px en `ui_accent` (50% alpha hover, 100% drag) en borde del panel, fuera del cache

## Esta sesión (2026-04-30) — Phase 6 W-6

### W-6: Header — icon anchor + right-aligned action buttons

**`src/app/renderer.rs` — `build_chat_panel_instances`:**
- Header row reworked into 3 zones: left `✦ + short model` in `ui_accent`, centered `provider:model` in `ui_muted`, right-aligned `[↺] [⎘] [✕]`
- Right-side actions only render when the transcript is non-empty
- Header text now uses span colors instead of a single concatenated label

**`src/app/mod.rs`:**
- `panel_hit_cell(x, y) -> Option<(col, row)>` centraliza hit-testing real del panel usando `viewport_rect()`
- Click en row 0 del panel mapea `[↺]` → restart, `[⎘]` → copy transcript, `[✕]` → close panel
- `mouse_in_panel()` ahora reutiliza `panel_hit_cell`, evitando drift entre render y mouse hit-testing

**`src/app/ui.rs`:**
- Nuevos helpers: `close_panel()`, `restart_chat_panel()`, `copy_chat_panel_transcript()`
- Call sites existentes (`/q`, toggle panel, disable AI, run command) reutilizan `close_panel()`
- `ClearAiContext` y `/clear` reutilizan `restart_chat_panel()`

**`src/llm/chat_panel.rs`:**
- `HeaderAction` + `header_action_for_col()` comparten el layout clicable del header entre renderer/input
- `transcript_text()` genera texto portable para clipboard
- `clear_messages()` ahora limpia `confirm_display`, `matched_skill` y `zero_state_hover`
- Tests nuevos para transcript copy + header hit-testing

## Sesión anterior (2026-04-30) — Phase 6 W-5 + input card polish

### W-5: Zero state — empty panel

**`src/llm/chat_panel.rs`:**
- Campo `zero_state_hover: Option<u8>` en `ChatPanel` — rastrea qué pill está bajo el cursor (0 = Fix last error, 1 = Explain command, None = ninguna)

**`src/app/renderer.rs` — `build_chat_panel_instances`:**
- Cuando `messages.is_empty() && state == Idle`: rama zero-state en lugar de la vista de mensajes
- Layout centrado: `✦` en `ui_accent` (center-3), blank (center-2), "Ask a question below" en `ui_muted` (center-1), blank (center), pills en center+2 y center+3
- Pills: dos rectángulos superpuestos (border outer + fill inner, patrón W-2) — `ui_muted`/`ui_surface` por defecto, `ui_accent`/`ui_surface_active` en hover
- Texto de pills ligeramente dimmed cuando no hovered; foreground completo en hover

**`src/app/mod.rs`:**
- `zero_state_hover_for_row(panel_row) -> Option<u8>` — computa pill rows con la misma fórmula que el renderer
- `CursorMoved`: actualiza `zero_state_hover` cuando el mouse está en el panel; marca dirty y pide redraw si cambió
- `MouseButton::Left Pressed` en panel: si zero state activo y click en pill → pre-fill input + auto-submit query

### Input card polish (W-2 retrofix)

**`src/app/renderer.rs` — `build_chat_panel_instances`:**
- `sep_row` ya no renderiza `│────...` — fila vacía. El borde redondeado de la card ya provee la separación visual

**`src/app/renderer.rs` — `build_chat_panel_input_rows`:**
- `card_bg`: reemplazado `ui_surface_active` (selection purple) por `panel_bg + 6%` — tono sutil coherente con el panel

---

## Sesiones anteriores (resumen)

### 2026-04-29 — Phase 6 W-1 → W-4
- W-1: full-width message background tinting (user warm tint, assistant cool tint)
- W-2: input box como bordered card (`RoundedRectInstance` radius 4px, `ui_muted` border)
- W-3: code block background (`ui_surface_active`) + left accent stripe (`ui_accent` 80%)
- W-4: sidebar section headers activos/inactivos en `foreground`/`ui_muted`

### 2026-04-28 — G-2-overlay
- `InfoOverlay` en `src/ui/info_overlay.rs`
- Enter en sección MCP/Skills/Steering del sidebar → overlay con contenido completo

### 2026-04-26 — Chat input UX
- TD-UI-01/02/03: cursor posicionado, historial de prompts, scroll vertical en input 4-líneas

### 2026-04-25 tarde — Phase 5 G-1 + G-2
- G-1: zoom pane (`Leader z`)
- G-2: Sidebar extendida MCP/Skills/Steering

### Anteriores
- 2026-04-25 mañana: UX polish + G-0 (UI tokens)
- 2026-04-24: D-5 + /skills + /mcp + Leader+w + MCP fixes
- 2026-04-23: Focus border + sidebar pills + E + D-4
- 2026-04-22: C-3.5 + D-4
- 2026-04-21: C-1 bugs + C-2 + C-3
- 2026-04-20: C-1 inicial + B
- 2026-04-19: A + 3.6
