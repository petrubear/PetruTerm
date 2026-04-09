# Technical Debt Registry

**Last Updated:** 2026-04-09
**Open Items:** 4
**Critical (P0):** 0 | **P1:** 3 | **P2:** 1 | **P3:** 0

> Resolved items are in [TECHNICAL_DEBT_archive.md](./TECHNICAL_DEBT_archive.md).

## Priority Definitions

| Priority | Definition | SLA |
|----------|------------|-----|
| P0 | Blocking development or causing incidents | Immediate |
| P1 | Significant impact on velocity or correctness | This sprint |
| P2 | Moderate impact, workaround exists | This quarter |
| P3 | Minor, address when convenient | Backlog |

---

## P1 — High Priority

- **TD-043** (P1): **AI panel input — texto en fila incorrecta** (`src/app/renderer.rs` ~l.709). El fix de TD-041 puso `vis1 = ""` cuando `n==1`, dejando la fila con `►` vacía y el texto en la fila sin marcador. Comportamiento correcto: cuando `n==1`, `vis1 = input_lines[0]` (texto va en la fila con `►`) y `vis2 = ""`. Solo duplicar visualmente cuando `n >= 2`. Fix de una línea: `let (vis1, vis2) = if n >= 2 { (lines[n-2].clone(), lines[n-1].clone()) } else { (lines.first().cloned().unwrap_or_default(), String::new()) };`.

- **TD-044** (P1): **Mouse separator drag no detecta el hit** (`src/app/mod.rs` `separator_at_pixel`). La zona de detección es ±3 px físicos (~1.5 px lógicos en Retina), demasiado pequeña; siempre retorna `None` y se inicia una selección normal. Fix: aumentar threshold a ±8 px físicos.

- **TD-045** (P1): **Keyboard pane resize (`<leader>+Option+→`) no funciona** (`src/app/input/mod.rs` leader dispatch). La combinación `<leader>` → soltar → `Option+Arrow` no ajusta el ratio. Investigar si `self.modifiers.state().alt_key()` devuelve `true` en macOS con winit 0.30 cuando Option está presionado al recibir el `ArrowLeft` event, o si la tecla Arrow llega como `Key::Character` en lugar de `Key::Named`.

## P2 — Medium Priority

- **TD-046** (P2): **Status bar no indica modo resize** (`src/app/mod.rs`, `src/ui/status_bar.rs`). Cuando leader está activo y se pulsa Option (preparando un resize), el status bar debería cambiar de color/texto igual que lo hace al activar el leader (`leader_active`). Agregar un campo `leader_alt_active: bool` a `InputHandler` y propagarlo a `StatusBar::build`.

---

> Todos los ítems resueltos están en [TECHNICAL_DEBT_archive.md](./TECHNICAL_DEBT_archive.md).
