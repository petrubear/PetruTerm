# Active Context

**Current Focus:** Phase 2.5 P3 — LLM Tool Use: Write & Run
**Last Active:** 2026-04-07

## Estado actual del proyecto

**Phase 1 COMPLETE. Phase 2 COMPLETE. Phase 2.5 P1+P2 COMPLETE. Phase 3 P1 implementada.**
**Deuda técnica: 0 ítems abiertos.**
**Tests: 16/16 passing. `cargo clippy --all-targets --all-features -- -D warnings` PASA limpio.**
**Multi-pane rendering COMPLETO (splits horizontal/vertical, separadores, resize automático).**

### Features verificados (2026-04-07)

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
| Multi-pane splits (^B % / ^B ") + separadores + resize | ✅ |
| Leader+Shift keys (%, ", &) — fix modifier key consuming leader | ✅ |
| Pane exit closes solo el pane (no el tab completo) | ✅ |
| **Leader+h/j/k/l — vim-style pane focus navigation (TD-024)** | ✅ |
| **1-cell padding entre contenido y separadores de pane** | ✅ |

### Deuda técnica — CERO ítems abiertos

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
| `^B h/j/k/l` | Focus pane left/down/up/right |
| `^B a` | AI panel |
| `^B o` | Command palette |
| `Ctrl+Space` | Inline AI block |
| Right-click | Context menu (Copy/Paste/Clear) |

## Archivos clave

| Archivo | Propósito |
|---------|-----------|
| `src/ui/panes.rs` | `PaneManager`, `PanePad`, `focus_dir()`, `pane_infos()` con inset |
| `src/ui/palette/actions.rs` | `Action::FocusPane(FocusDir)`, `built_in_actions()` |
| `src/llm/tools.rs` | `AgentTool`, `execute_tool()` (CWD sandbox) |
| `src/llm/chat_panel.rs` | `ChatPanel`, historial, `last_assistant_command()` |
| `src/app/ui.rs` | `UiManager` (palette, context_menu, panels, ai_block) |
| `src/app/mux.rs` | `cmd_focus_pane_dir()` |
| `src/app/mod.rs` | Event loop, mouse handling, context menu dispatch |
| `src/app/renderer.rs` | `build_palette_instances`, `build_context_menu_instances` |
| `src/ui/context_menu.rs` | `ContextMenu`, `ContextAction` |
| `src/renderer/atlas.rs` | `GlyphAtlas` (4096px, epoch LRU) |
| `src/font/shaper.rs` | `TextShaper`, `is_pua()` (consolidada) |
| `src/config/mod.rs` | `ensure_default_configs()` (idempotente, cada arranque) |
