# Active Context

**Current Focus:** TD-043 / TD-044 / TD-045 — bugs en pane resize (TD-042) + regresión AI panel
**Last Active:** 2026-04-09

## Estado actual del proyecto

**Phase 1 COMPLETE. Phase 2 COMPLETE. Phase 2.5 COMPLETE. Phase 3 P1 COMPLETE. Phase 3 P2 COMPLETE.**
**Phase 3 P3 parcial (snippets/Starship pendientes). Phase 4 (plugins) no iniciada.**
**Deuda técnica: 4 ítems abiertos (P0:0, P1:3, P2:1). `cargo build` PASA. `cargo clippy` PASA (6 lints pre-existentes ajenos).**

### Features verificados

| Feature | Estado |
|---------|--------|
| Render, PTY, teclado, ratón, clipboard, cursor, resize | ✅ |
| Custom title bar, .app bundle, icono | ✅ |
| Scrollback + scroll bar | ✅ |
| Ligatures, nvim/tmux verificados | ✅ |
| Emoji / color glyph rendering | ✅ |
| AI panel + inline AI block (Ctrl+Space) | ✅ |
| Leader key system | ✅ |
| LLM providers (OpenRouter/Ollama/LMStudio) | ✅ |
| Historial de chat por pane | ✅ |
| Tab bar (pill shape, SDF shader) | ✅ |
| Tab rename (`<leader>,`) | ✅ |
| Shell exit cierra tab | ✅ |
| Selección doble/triple click | ✅ |
| Context menu (right-click) | ✅ |
| Command palette | ✅ |
| Phase 2.5 P1 — file context + AGENTS.md + file picker | ✅ |
| Phase 2.5 P2 — LLM tool use (ReadFile, ListDir) | ✅ |
| Phase 2.5 P3 — WriteFile + RunCommand + undo | ✅ |
| Multi-pane splits + separadores | ✅ |
| Leader+h/j/k/l — vim-style pane focus | ✅ |
| Status bar — leader, CWD, git branch, exit code, time | ✅ |
| Pane resize (teclado + mouse drag) | ⚠️ implementado, bugs TD-043/044/045 |

## Deuda técnica abierta

| ID | Prioridad | Archivo | Descripción breve |
|----|-----------|---------|-------------------|
| TD-043 | **P1** | `src/app/renderer.rs` ~l.709 | AI panel input en `vis2` (fila sin ►); debería ir en `vis1` cuando `n==1` |
| TD-044 | **P1** | `src/app/mod.rs` `separator_at_pixel` | Hit area ±3px físicos demasiado pequeña en Retina — aumentar a ±8px |
| TD-045 | **P1** | `src/app/input/mod.rs` leader dispatch | `<leader>+Option+Arrow` no activa resize — investigar `alt_key()` en winit 0.30 macOS |
| TD-046 | P2 | `src/app/mod.rs`, `src/ui/status_bar.rs` | Status bar no cambia color al presionar Option en modo leader |

### Fix exacto para TD-043 (renderer.rs ~l.709)
```rust
// Reemplazar las dos líneas:
let vis1 = if n >= 2 { input_lines[n - 2].clone() } else { String::new() };
let vis2 = input_lines.last().cloned().unwrap_or_default();
// Con:
let (vis1, vis2) = if n >= 2 {
    (input_lines[n - 2].clone(), input_lines[n - 1].clone())
} else {
    (input_lines.first().cloned().unwrap_or_default(), String::new())
};
```

### Fix exacto para TD-044 (mod.rs `separator_at_pixel`)
```rust
// Cambiar ambas comparaciones de 3.0 a 8.0:
if (px - sep_x).abs() <= 8.0 && ...
if (py - sep_y).abs() <= 8.0 && ...
```

### Investigación para TD-045
- Agregar `log::debug!("leader key: {:?} alt={}", event.logical_key, self.modifiers.state().alt_key())` en el leader dispatch
- Verificar si `Key::Named(NamedKey::ArrowLeft)` llega con `alt=true` o si Option+Arrow es mapeado a `Key::Character`
- Si macOS envía `Key::Character` para Option+Arrow, añadir match arm correspondiente

## Phase 3 P3 — Pendiente

| Tarea | Estado |
|-------|--------|
| Tab rename `<leader>,` | ✅ (2026-04-08) |
| Snippets: `config.snippets` tabla Lua, expandir via palette | 🔲 |
| Starship compatibility: detectar `STARSHIP_SHELL` | 🔲 |
| Powerline / Nerd Font glyphs en widgets | 🔲 |
| Built-in themes en `assets/themes/` | 🔲 |

## Keybinds actuales

| Tecla | Acción |
|-------|--------|
| `^B c` | New tab |
| `^B &` | Close tab |
| `^B n/b` | Next/prev tab |
| `^B ,` | Rename active tab |
| `^B %` | Split horizontal |
| `^B "` | Split vertical |
| `^B x` | Close pane |
| `^B h/j/k/l` | Focus pane (vim-style) |
| `^B Option+←→↑↓` | Resize pane (**parcial** — TD-045) |
| `^B a` | Abrir / cerrar AI panel |
| `^B A` | Mover focus terminal ↔ chat |
| `^B e` | Explain last output |
| `^B f` | Fix last error |
| `^B z` | Undo last write |
| `^B o` | Command palette |
| `Ctrl+Space` | Inline AI block |
| Right-click | Context menu |

## Próximos pasos recomendados

1. **Sesión rápida (~1h):** Resolver TD-043 + TD-044 (fixes de 2–5 líneas cada uno)
2. **Sesión media (~1h):** Investigar + resolver TD-045; implementar TD-046
3. **Phase 3 P3:** Snippets y Starship
4. **Phase 4:** Plugin ecosystem
