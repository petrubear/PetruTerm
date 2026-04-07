# Active Context

**Current Focus:** Phase 3 P2 — Status Bar
**Last Active:** 2026-04-07

## Estado actual del proyecto

**Phase 1 COMPLETE. Phase 2 COMPLETE. Phase 2.5 COMPLETE (P1+P2+P3). Phase 3 P1 implementada.**
**Deuda técnica: 2 ítems abiertos (P2, P3). TD-025/TD-028 resueltos hoy.**
**Tests: 16/16 passing. `cargo clippy --all-targets --all-features -- -D warnings` PASA limpio.**

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
| Shell exit cierra tab (o solo el pane si hay más) | ✅ |
| Selección doble/triple click | ✅ |
| Context menu (right-click: Copy/Paste/Clear) | ✅ |
| Keybinds en command palette | ✅ |
| Default configs completas | ✅ |
| Emoji / color glyph rendering | ✅ |
| Phase 2.5 P1 — file context + AGENTS.md + file picker | ✅ |
| Phase 2.5 P2 — LLM tool use (ReadFile, ListDir) | ✅ |
| Phase 2.5 P3 — WriteFile + RunCommand + undo | ✅ |
| Multi-pane splits + separadores + padding | ✅ |
| Leader+h/j/k/l — vim-style pane focus | ✅ |

## Siguiente: Phase 3 P2 — Status Bar

### Deliverables pendientes
- [ ] Status bar engine (lua-line style): enable/disable desde Lua + command palette
- [ ] Widgets built-in: `mode`, `cwd`, `git_branch`, `time`, `exit_code`
- [ ] Lua API: `petruterm.statusbar.register_widget({ name, render })`
- [ ] Posición configurable (`top` / `bottom`)

## Keybinds actuales

| Tecla | Acción |
|-------|--------|
| `^B c` | New tab |
| `^B &` | Close tab |
| `^B n/b` | Next/prev tab |
| `^B %` | Split horizontal |
| `^B "` | Split vertical |
| `^B x` | Close pane |
| `^B h/j/k/l` | Focus pane left/down/up/right |
| `^B a` | AI panel |
| `^B e/f` | Explain/Fix last output |
| `^B z` | Undo last write |
| `^B o` | Command palette |
| `Ctrl+Space` | Inline AI block |
| Right-click | Context menu |

## Archivos clave (Phase 2.5 P3)

| Archivo | Propósito |
|---------|-----------|
| `src/llm/diff.rs` | LCS line diff + compress_diff |
| `src/llm/tools.rs` | WriteFile, RunCommand + requires_confirmation() |
| `src/llm/chat_panel.rs` | ConfirmWrite/ConfirmRun events, AwaitingConfirm state, ConfirmDisplay |
| `src/app/ui.rs` | confirm_yes/no, undo_stack, pending_pty_run, agent loop |
| `src/app/mod.rs` | flush_pending_pty_run() |
| `src/app/renderer.rs` | Confirmation view con diff +/- coloreado |
| `src/app/input/mod.rs` | y/n/Enter/Esc en AwaitingConfirm |

## Pendiente después de Status Bar

- Phase 3 P3: Snippets, Starship, temas built-in
- Phase 4: Plugin ecosystem (lazy.nvim-style)
