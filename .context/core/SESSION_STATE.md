# Session State

**Last Updated:** 2026-05-04
**Session Focus:** Notificaciones nativas + bug fix config loader

## Branch: `feat/phase-6-warp-ui`

## Estado actual

**Phase 1–3 + 3.5 + A + 3.6 + B + C + D + Phase 5 G-0/G-1/G-2/G-3 + G-2-overlay COMPLETE.**
**Phase 6 Warp UI: W-1 W-2 W-3 W-4 W-5 W-6 W-7 W-8 COMPLETAS. Phase 6 COMPLETA.**
**Sin deuda técnica abierta. Diferidos: TD-PERF-03/05/29.**

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
