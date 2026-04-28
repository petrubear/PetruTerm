# Session State

**Last Updated:** 2026-04-28
**Session Focus:** G-2-overlay — info overlay para sidebar MCP/Skills/Steering

## Branch: `master`

## Estado actual

**Phase 1–3 + 3.5 + A + 3.6 + B + C + D + Phase 5 G-0/G-1/G-2/G-3 + G-2-overlay COMPLETE.**
**Sin deuda técnica abierta. Diferidos: TD-PERF-03/05/29.**

**Phase 5 COMPLETA. Next: Phase 4 — Plugin Ecosystem.**

## Esta sesión (2026-04-28) — G-2-overlay

**Archivos:**
- `src/ui/info_overlay.rs` (nuevo) — `InfoOverlay` struct
- `src/ui/mod.rs` — exporta `InfoOverlay`
- `src/llm/mcp/manager.rs` — `tools_for_server()`
- `src/app/ui.rs` — `mcp_overlay_content()`
- `src/app/mod.rs` — campo `info_overlay`, `open_sidebar_info_overlay()`, `close_sidebar()`, handlers Esc/j/k, render call
- `src/app/renderer.rs` — `build_info_overlay_instances()`, cursor highlight en sidebar MCP/Skills/Steering

**Bug fix posterior:** Esc en overlay dejaba `sidebar_kbd_active = true` → Enter interceptado. Fix: Esc en overlay resetea `sidebar_kbd_active = false` (no cierra sidebar). Esc en sidebar sin overlay → `close_sidebar()`.

---

## Esta sesión (2026-04-26) — Chat input UX

### TD-UI-01/02/03 + vertical scroll + 4-line input

**`src/llm/chat_panel.rs`:**
- `input_cursor: usize` — posición del cursor (char index)
- `prompt_history: Vec<String>`, `history_idx: Option<usize>`, `history_draft: String`
- `type_char`: inserta en cursor y avanza; `backspace`: borra antes del cursor
- `move_cursor_left/right/home/end`, `clear_input` (Cmd+A)
- `history_up/down`: navegación bash-style; guarda draft al entrar, restaura al salir
- `cursor_visual_pos(wrap_width)`: mapea char index → (vline, vcol) con soft-wrap y \n hard-breaks
- `visual_pos_to_cursor(vline, vcol, wrap_width)`: inverso
- `cursor_up/down`: mueven cursor una visual-line; retornan false en boundary → history fallback

**`src/app/input/mod.rs`:**
- ArrowLeft/Right/Home/End → cursor movement
- ArrowUp: `cursor_up()` primero; si false → `history_up()`
- ArrowDown: `cursor_down()` primero; si false → `history_down()`
- Cmd+A → `clear_input()`

**`src/app/renderer.rs`:**
- `sep_row = screen_rows - 6` (era - 4) — libera 2 filas para input de 4 líneas
- `build_chat_panel_input_rows`: 4 filas de input (input_row1..4); viewport de 4 líneas sigue al cursor (`vis_start = cursor_line.saturating_sub(3)`)
- Blink-off cursor_line ahora usa `panel.cursor_visual_pos()` (era heurística buggy con \n)
- Guard mínimo: `screen_rows < 8` (era < 6)

**Fixes clippy/fmt pre-existentes (G-3):**
- `div_ceil`, `strip_prefix`, `type_complexity`, `collapsible_match`, `rustfmt`

---

### G-3: Markdown en chat

**Nuevo módulo `src/llm/markdown.rs`:**
- `AnnotatedLine { display: String, kind: BlockKind, spans: Vec<(usize, usize, SpanKind)> }`
- `parse_markdown(content, width, state) -> (Vec<AnnotatedLine>, ParseState)` — block-level (headings, fences, listas) + inline (`**bold**`, `*italic*`, `` `code` ``)
- `highlight_code(lang, line)` — tokenizador manual para rs/py/js/ts/sh/json (keywords, strings, comentarios, números, operadores)
- `ParseState { in_fence, fence_lang }` — carry-over de estado de fence para streaming
- Normalización de aliases: `rust`→`rs`, `python`→`py`, `javascript`→`js`, etc.
- 6 unit tests cubriendo headings, inline, code fence, listas, streaming state, wrap

**`src/llm/chat_panel.rs`:**
- `wrapped_cache: Vec<Vec<AnnotatedLine>>` (era `Vec<Vec<String>>`)
- `ensure_wrap_cache` llama `parse_markdown` por mensaje
- `wrapped_message(i) -> &[AnnotatedLine]`

**`src/app/renderer.rs`:**
- `scratch_lines: Vec<(String, [f32;4], Option<[f32;4]>, Vec<(usize,usize,[f32;4])>)>` — 4-tupla con spans resueltos
- `streaming_stable_lines: Vec<AnnotatedLine>` + `streaming_fence_state: ParseState`
- `push_md_line()` — render multi-span por fila visual (N calls a `push_shaped_row` con col_offset acumulado)
- `resolve_line_fg()` — BlockKind → ColorScheme (H1=`ui_accent`, H2=`ansi[6]`, H3=`ansi[3]`, CodeBlock=`ansi[2]`)
- `resolve_span_fg()` — SpanKind → color (keyword=`ansi[5]`, string=`ansi[3]`, comment=`ui_muted`, number=`ansi[6]`)
- Bug fix: spans offseteados por `prefix_len` (8 chars) al construir `resolved_spans` — sin esto los tokens de syntax highlight coloreaban el prefix en lugar del código
- Eliminado: `md_style_line()`, `RUN_FG`, hint bar de último comando (raw markdown visible, poco valor)

### Colores de salida (Dracula Pro)
| Elemento | Color |
|---|---|
| H1 | Purple `ui_accent` (#9580ff) |
| H2 | Cyan `ansi[6]` |
| H3 | Amber `ansi[3]` |
| Code block / inline code | Green `ansi[2]` |
| keyword | Magenta `ansi[5]` |
| string literal | Yellow `ansi[3]` |
| comment | Muted `ui_muted` |
| number | Cyan `ansi[6]` |
| bold | `brighten(fg, 0.2)` |
| italic | `dim(fg, 0.15)` |

---

## Sesiones anteriores (resumen)

### 2026-04-25 tarde — Phase 5 G-1 + G-2
- G-1: `Leader z` zoom pane, `zoomed_pane` en Mux, indicador en status bar.
- G-2: Sidebar extendida con MCP/Skills/Steering (proporciones 40/20/20/20), Tab/Shift+Tab, scroll independiente por sección, `Leader s` alias.

### 2026-04-25 mañana — UX polish + G-0
- G-0: 7 tokens semánticos en `ColorScheme`, ~20 literales hardcodeados reemplazados.
- Idle detection, búsqueda capped, chat UX fixes.

### 2026-04-24 — D-5 + REC-PERF-01/02/05 + /skills + /mcp + Leader+w + MCP fixes
### 2026-04-23 — Focus border + sidebar pills + Fase E + D-4 bugs
### 2026-04-22 — Fase C-3.5 + D-4 planificación
### 2026-04-21 — Fase C-1 bugs + C-2 + C-3
### 2026-04-20 — Fase C-1 inicial + Fase B
### 2026-04-19 — Fase A + Fase 3.6
