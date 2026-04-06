# Active Context

**Current Focus:** Phase 2.5 P3 — LLM Tool Use: Write & Run
**Last Active:** 2026-04-06

## Estado actual del proyecto

**Phase 1 COMPLETE. Phase 2 COMPLETE. Phase 3 P1 implementada, pero con deuda abierta.**
**Deuda técnica: 0 ítems abiertos. Todos resueltos (incluye auditoría Codex 2026-04-06).**
**Tests: 16/16 passing. `cargo clippy --all-targets --all-features -- -D warnings` PASA limpio.**

### Features verificados (2026-04-06)

| Feature | Estado |
|---------|--------|
| Render, PTY, teclado, ratón, clipboard, cursor, resize | ✅ |
| Custom title bar, .app bundle, icono | ✅ |
| Scrollback + scroll bar | ✅ |
| Ligatures, nvim/tmux verificados | ✅ |
| AI panel + inline AI block (Ctrl+Space) | ✅ |
| Leader key system | ✅ |
| LLM providers (OpenRouter/Ollama/LMStudio) | ✅ |
| Historial de chat por pane | ✅ |
| Tab bar (pill shape, SDF shader) | ✅ |
| Shell exit cierra tab | ✅ |
| Selección doble/triple click | ✅ |
| Selección con ratón (fix: `setMovableByWindowBackground: NO`) | ✅ |
| Context menu (right-click: Copy/Paste/Clear) | ✅ |
| Keybinds en command palette (alineados derecha) | ✅ |
| Default configs con todos los campos del schema | ✅ |
| Emoji / color glyph rendering | ✅ |
| Phase 2.5 P1 — file context attachment (AGENTS.md, file picker) | ✅ |
| Phase 2.5 P2 — LLM tool use (ReadFile, ListDir) | ✅ |

### Deuda técnica — CERO ítems abiertos

Todos los ítems de la auditoría Codex (TD-017..TD-022) resueltos el 2026-04-06.

### Deuda técnica resuelta recientemente

| TD | Solución |
|----|---------|
| TD-OP-02 | `is_pua()` consolidada: BMP PUA cubre todos los subrangos |
| TD-OP-03 | Atlas 4096 px + eviction LRU por epoch |
| TD-OP-01 | Eliminado `unsafe impl Sync`; `Send` con SAFETY comment |
| TD-016 | `last_assistant_command()` filtra líneas `⟳`/`✓` |

## Siguiente: Phase 2.5 P3 — Tool Use: Write & Run

### Deliverables pendientes
- [ ] `WriteFile { path, content }` — LLM propone reemplazo completo de archivo
- [ ] `ApplyDiff { path, diff }` — LLM propone patch unificado
- [ ] Preview del diff inline en el panel (`+`/`-` con colores)
- [ ] Confirmación `[y] Apply  [n] Reject` antes de escribir al disco
- [ ] `RunCommand { cmd }` — ejecuta en PTY activo tras confirmación
- [ ] Undo de un paso (`<leader>z` restaura el archivo original en memoria)

## Keybinds (tmux-aligned)

| Tecla | Acción |
|-------|--------|
| `^B c` | New tab |
| `^B &` | Close tab |
| `^B n` | Next tab |
| `^B b` | Prev tab |
| `^B %` | Split horizontal |
| `^B "` | Split vertical |
| `^B x` | Close pane |
| `^B a` | AI panel |
| `^B o` | Command palette |
| `Ctrl+Space` | Inline AI block |
| Right-click | Context menu (Copy/Paste/Clear) |

## Archivos clave

| Archivo | Propósito |
|---------|-----------|
| `src/llm/tools.rs` | `AgentTool`, `execute_tool()` (CWD sandbox) |
| `src/llm/chat_panel.rs` | `ChatPanel`, historial, `last_assistant_command()` |
| `src/app/ui.rs` | `UiManager` (palette, context_menu, panels, ai_block) |
| `src/app/mod.rs` | Event loop, mouse handling, context menu dispatch |
| `src/app/renderer.rs` | `build_palette_instances`, `build_context_menu_instances` |
| `src/ui/context_menu.rs` | `ContextMenu`, `ContextAction` |
| `src/ui/palette/actions.rs` | `PaletteAction` (+ `keybind`), `built_in_actions(&Config)` |
| `src/renderer/atlas.rs` | `GlyphAtlas` (4096px, epoch LRU) |
| `src/font/shaper.rs` | `TextShaper`, `is_pua()` (consolidada) |
| `src/config/mod.rs` | `ensure_default_configs()` (idempotente, cada arranque) |
