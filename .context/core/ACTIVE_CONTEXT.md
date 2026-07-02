# Active Context

**Current Focus:** Phase 9 — UI Restyle (Floating Surfaces & Vibrancy)
**Last Active:** 2026-07-02
**Branch:** `master`
**Estado:** Diseño aprobado. Spec en [`.context/specs/ui_restyle.md`](../specs/ui_restyle.md). Implementación pendiente.
**Próxima tarea:** Ejecutar Fase 1 (R-1 tokens → R-8 float layout). Fase 2 (blur macOS) después.

## Phase 9 — Estado de tareas

| ID | Tarea | Fase | Estado |
|----|-------|------|--------|
| R-1 | Tokens de estilo UI (espaciado + radios) | 1 | **COMPLETA** |
| R-2 | Tonos de superficie con contraste (Dracula Pro) | 1 | **COMPLETA** (otros 4 temas .lua pendientes) |
| R-3 | Sidebar izquierda — restyle | 1 | **COMPLETA** (tokenizado; float en R-8) |
| R-4 | Command palette — restyle | 1 | **COMPLETA** (search-box inset = follow-up) |
| R-5 | Chat panel — restyle | 1 | Casi (tono propio + pills hechos; float inset en R-8) |
| R-6 | Tab bar — restyle | 1 | Ya coherente (radios btn_h/4≈r_pill, tabs planos por diseño) |
| R-7 | Status bar — restyle | 1 | Ya coherente (tonos derivados del tema) |
| R-8 | Float layout de la ventana (inset global) | 1 | Pendiente |
| V-1 | Ventana transparente + `window.opacity` | 2 | Pendiente |
| V-2 | Blur/vibrancy nativo (NSVisualEffectView) | 2 | Pendiente |
| V-3 | Esquinas redondeadas de ventana | 2 | Pendiente |
| V-4 | Tokens de superficie translúcidos con blur | 2 | Pendiente |

---

## Phase 8 — ACP Integration (COMPLETA, previa)

**Branch:** `acp` — **Commits:** `beee958` (feat: backend ACP) + `bb63df0` (chore: docs/llm.lua/fmt)
**Pendiente:** decidir push de `acp` a remoto y/o PR hacia `master` (requiere confirmación explícita)

## Estado actual del proyecto

**Phases 1–7 COMPLETAS. Deuda técnica cerrada. Branch `acp` creado (2026-06-11).**
**Deuda técnica: 0 items abiertos. Watch: AUDIT-CLEAN-02.**

## Phase 8: ACP — Estado de tareas

| ID | Tarea | Estado |
|----|-------|--------|
| ACP-0 | Dependencias (`cargo add`) | **COMPLETA** |
| ACP-1 | Config schema (`LlmBackend`, `AcpAgentConfig`) | **COMPLETA** |
| ACP-2a | `src/llm/acp/mod.rs` + `session.rs` — ciclo de vida sesión | **COMPLETA** |
| ACP-2b | `src/llm/acp/terminal.rs` — integración PTY | **COMPLETA** |
| ACP-2c | `src/llm/acp/fs.rs` — operaciones archivo | **COMPLETA** |
| ACP-3 | `UiManager` wiring + dispatch | **COMPLETA** |
| ACP-4 | Header UI (`◈` agente, `✦` provider) | **COMPLETA** |
| ACP-5 | Slash commands `/model` + `/agent` | **COMPLETA** |
| ACP-6 | Code review fixes + conexión async + prueba manual con Claude | **COMPLETA** (2026-07-01) |

Spec completo con pasos detallados en [`.context/specs/build_phases.md`](../specs/build_phases.md) — Phase 8.

## ACP-6 — Fixes de code review + verificación manual (2026-07-01)

Probado end-to-end con `agent-client-protocol` real: `backend = "agent"`,
`command = "npx"`, `args = { "-y", "@agentclientprotocol/claude-agent-acp" }`.
Funciona: streaming de tokens, `terminal/create` (split real), confirm de
escritura + undo.

Bugs encontrados por code-review y corregidos antes de dar la feature por
completa (ver [`session_state`](SESSION_STATE.md) para detalle por archivo):

1. `terminal_output_text` devolvía `""` tras exit — el pane se auto-cerraba
   antes de que el agente pudiera leer `terminal/output`. Fix: cache
   `terminal_final_output: HashMap<usize, String>` poblado en `close_terminal`.
2. `fs/write_text_file` leía el archivo **después** de escribirlo para el
   `AiEvent::UndoState` → undo restauraba el contenido nuevo (no-op). Fix:
   leer antes de escribir, igual que el path Provider.
3. `validate_path` (`src/llm/acp/fs.rs`) no canonicalizaba antes del check
   `starts_with($HOME)` → bypass de `..`. Fix: mismo patrón que
   `src/llm/tools.rs` (camina al ancestro existente más cercano y canonicaliza).
4. `open_terminal_for_acp` unía `command`+`args` con espacios crudos antes de
   escribir al PTY → argumentos con espacios/metacaracteres se rompían. Fix:
   `shell_quote()` (comillas simples POSIX) por token.
5. `AcpSession::connect` bloqueaba el hilo de UI vía `block_on` en `new()`,
   `rewire_backend()` y `/agent`. Fix: `spawn_acp_connect()` lanza la conexión
   en background; `UiManager::poll_acp_connect()` la recoge sin bloquear
   (pollea cada frame vía `wakeup_proxy`).
6. `handle_slash_command` limpiaba `input` sin resetear `input_cursor` →
   panic (`String::remove` fuera de rango) en el siguiente backspace tras usar
   cualquier slash command (`/model`, `/agent`, etc.). Fix: resetear
   `input_cursor = 0` junto al `clear()`, como en el resto del archivo.

`src/llm/acp/mod.rs` (456 líneas) dividido en `mod.rs` (132, ciclo de vida) +
`session.rs` (340, cadena de handlers del protocolo) para cumplir el límite
de 400 líneas/módulo de `AGENTS.md`.

**Cerrado y commiteado:** `scripts/ci-local.sh` completo en verde (clippy -D
warnings + fmt --check + test --lib + audit) tras aplicar `cargo fmt` a dos
líneas que excedían el ancho. `cargo test --bin petruterm` (93 tests) también
limpio. Commits: `beee958` (feature) + `bb63df0` (docs + llm.lua + fmt).
`cargo audit` reporta 3 advisories no bloqueantes (`ttf-parser`, `anyhow`,
`memmap2`) en dependencias transitivas de terceros — no accionables desde
este repo, `cargo audit` sale con código 0 igual.

## Decisiones de implementación no obvias (ACP-0..ACP-2)

- **Versión ACP**: `agent-client-protocol = "0.11"` (NO 0.14). El wrapper tokio (`agent-client-protocol-tokio = "0.11.1"`) solo implementa `ConnectTo` para la v0.11 del core. Usar 0.14 del core da error de trait en `connect_with`.
- **`AcpSession::connect()`** (async, dentro de la tarea tokio) bloquea hasta que `initialize` + `new_session` completan — oneshot channel `ready_tx` señaliza cuando el agente está listo. Primera llamada a `prompt()` no espera handshake. Los *callers* (`UiManager::new`, `rewire_backend`) NO llaman `block_on` sobre esto — ver ACP-6: se lanza en background vía `spawn_acp_connect()` y se recoge con `poll_acp_connect()` para no congelar la UI mientras el proceso agente arranca.
- **Routing de contexto**: `QueryCtx = Arc<Mutex<Option<Sender<AiEvent>>>>` y `TermCtx = Arc<Mutex<Option<Sender<AcpTerminalRequest>>>>` compartidos entre los handlers (`on_receive_notification`, `on_receive_request`) y el loop de prompts en `connect_with`. Se setean antes de cada `send_request(PromptRequest)` y se borran después.
- **Guard drop antes de await**: Los handlers de `on_receive_notification` extraen el `Sender<AiEvent>` del Mutex y sueltan el guard antes de hacer `.send(...).await` — necesario para que el Future sea `Send`.
- **`terminal/output`**: No es "escribir al terminal" — es polling del output del proceso. `AcpTerminalRequest::GetOutput` pregunta al main thread por el scrollback del pane + exit code.
- **`terminal_id` ↔ `pane_id`**: Se usa `TerminalId::new(pane_id.to_string())`. Requests posteriores parsean `terminal_id.0.parse::<usize>()`. No se necesita mapa extra.
- **`ToolCallUpdate.fields`** es `ToolCallUpdateFields` (no `Option`); `.title` dentro sí es `Option<String>`.

## Decisiones de diseño clave (Phase 8)

- **Backend, no mode**: campo `llm.backend = "provider" | "agent"` (alineado con Harness de Warp)
- **Sesión persistente**: `AcpSession` vive mientras el panel está abierto; idle timeout 300s; reconexión automática
- **Terminal dedicado**: `terminal/create` → nuevo pane real via `Mux` (killer feature — somos un terminal nativo)
- **Mismos `AiEvent`**: ambos backends (Provider y Agent) producen los mismos eventos; `ChatPanel` no distingue
- **Comandos separados**: `/model` para LLM provider, `/agent` para ACP agent (alineado con Warp)
- **Header**: `◈` + display_name para Agent, `✦` + short_model para Provider

## Archivos centrales de Phase 8

| Archivo | Rol |
|---------|-----|
| `src/config/schema.rs` | `LlmBackend`, `AcpAgentConfig`, cambios en `LlmConfig` |
| `src/config/lua.rs` | Parsing Lua del nuevo backend y agent config |
| `src/llm/acp/mod.rs` | `AcpSession` — spawn, init, session, prompt, idle timeout |
| `src/llm/acp/session.rs` | Cadena de handlers ACP (`on_receive_request`/`notification`) + loop de prompts |
| `src/llm/acp/terminal.rs` | `AcpTerminalRequest` — puente ACP↔Mux PTY |
| `src/llm/acp/fs.rs` | `fs/read_text_file`, `fs/write_text_file` con confirm, `validate_path` canonicalizado |
| `src/app/ui/providers.rs` | `rewire_backend()`, `/model`, `/agent` slash commands |
| `src/app/ui/mod.rs` | `acp_session`, `acp_terminal_tx`, dispatch en `send_query()` |
| `src/app/renderer/chat.rs` | `build_panel_header` — diferenciación visual ◈/✦ |

## Release prep v0.1.9 — EN CURSO (2026-05-12)

- Fix incluido: el menú contextual normal de la terminal vuelve a restaurar `Copy/Paste/Clear/Ask AI` después de usar el color picker de tabs.
- `scripts/ci-local.sh` ejecutado: detectó dos fallos de clippy preexistentes en `src/app/mux/snapshot.rs` y `src/app/mux/workspace.rs`.
- Versionado: siguiente release preparada como `v0.1.9`.

## Workspace Persistence — COMPLETA (2026-05-12)

**Estado:** IMPLEMENTADA. Validación previa limpia; el run actual de `ci-local.sh` expuso fallos de clippy no relacionados.

### Almacenamiento
- Un archivo JSON por workspace: `~/.config/petruterm/workspaces/<name>.json`
- Versión: `"version": 1`
- Contenido: nombre, `saved_at`, array de tabs (nombre + árbol de panes `PaneNode` + CWD por pane)
- No se guardan procesos corriendo ni scrollback; al cargar se abre shell nuevo en cada CWD

### Formato `pane_tree` (recurso serde)
```json
{ "type": "split", "dir": "horizontal", "ratio": 0.6,
  "left":  { "type": "leaf", "cwd": "/path/to/a" },
  "right": { "type": "leaf", "cwd": "/path/to/b" } }
```

### Keybinds nuevos
| Keybind | Acción |
|---------|--------|
| `Leader W s` | Guardar workspace activo a disco |
| `Leader W L` | Abrir paleta filtrada en workspaces guardados |

### Paleta de comandos
- Nueva sección dinámica **"Saved Workspaces"**: escanea `~/.config/petruterm/workspaces/*.json` en tiempo real
- Muestra: nombre, nro de tabs, fecha de guardado
- Seleccionar → crea workspace NUEVO basado en ese layout (no destruye el actual)
- Si ya existe un workspace con ese nombre en memoria: aviso + opción de cancelar o renombrar

### Comportamiento de carga
- CWD inexistente → fallback a `$HOME`
- Procesos: shell limpio (no se puede restaurar proceso previo)

### Config opcional (`config.lua`)
```lua
workspaces = {
  auto_save_on_exit   = true,   -- guarda todos los ws al cerrar la app
  auto_save_on_switch = false,  -- guarda ws activo al cambiar a otro
}
```

### Pasos de implementación
1. `serde` derives en `PaneNode`, `SplitDir`, `Tab` → struct `WorkspaceSnapshot`
2. `save_workspace(path)` — serializa activo, escribe JSON
3. `load_workspace(path)` — deserializa, crea tabs/panes, spawna PTYs con CWDs guardados
4. Paleta: sección "Saved Workspaces" dinámica
5. Keybinds `Leader W s` y `Leader W L`
6. Auto-save on exit (respeta config)

---

## Infraestructura de seguridad nueva (Wave 1+2)

- `src/llm/mcp/trust.rs` — lista de cwds confiables (`~/.config/petruterm/mcp_trust.json`)
- Trust gate unificado: MCP local, skills locales y steering local comparten el mismo check `trust::is_trusted(&cwd)`
- Palette action "Trust local MCP config" → `trust::trust(cwd)` + `reload_mcp()`
- Camino por defecto: global siempre, local solo si trusted

## Phase 7 — COMPLETA

| ID | Feature | Estado |
|----|---------|--------|
| H-1 | Hover links — URLs, paths, stack traces clicables | **COMPLETA** |
| B-1 | OSC 133 parser en VTE handler | **COMPLETA** |
| B-2 | Block manager por pane | **COMPLETA** |
| B-3 | Render visual de bloques | **COMPLETA** |
| B-4 | Operaciones sobre bloques (context menu, keybinds) | **COMPLETA** |
| A-1 | AI agent: schema de acciones + parser | **COMPLETA** |
| A-2 | AI agent: confirm UI inline | **COMPLETA** |
| A-3 | AI agent: action handlers | **COMPLETA** |
| I-1 | Input shadow buffer (depende de B-1) | **COMPLETA** |
| I-2 | Syntax coloring del comando | **COMPLETA** |
| I-3 | Ghost text — inline completion hints | **COMPLETA** |
| I-4 | Flag hints — tooltips de flags | **COMPLETA** |

## Notas de input decoration (I-1..I-4)

- `input_syntax_highlight` y `input_ghost_text` configurables en `ui.lua` (default: true).
- Con zsh-autosuggestions: poner ambos a `false` para evitar conflictos.
- Shadow se desactiva en Up/Down (history nav) y en ArrowRight/Tab en buf-end sin ghost aceptado (previene drift de `cmd_start_col`).

## Invariantes arquitectónicos clave (no romper)

**Shaper drops space cells (TD-RENDER-01):**
Pre-pass bg-only en `build_instances` OBLIGATORIO. Sin él, celdas-espacio con bg != default_bg
no generan vértices → GPU clear color → franjas horizontales.

**damage-skip scratch buffer:**
`cell_data_scratch` es per-terminal. Siempre limpiar cuando cambia `terminal_id`.

**Blink fast path:**
`last_instance_count` + `last_overlay_start` en `RenderContext` OBLIGATORIOS.
Vértice cursor transparente (bg.a=0) para blink-off — no reducir cell_count.

**alacritty_terminal grid scrollback:**
`grid()[Line(row)]` NO cuenta `display_offset`. Usar `Line(row as i32 - display_offset)`.

**alacritty_terminal exit event:** `Event::ChildExit(i32)`, NO `Event::Exit`.

**PTY env vars obligatorias:** `TERM=xterm-256color`, `COLORTERM=truecolor`, `TERM_PROGRAM=PetruTerm`.

**SwashCache:** usar `get_image_uncached()`, NO `get_image()`.

**macOS trackpad:** `MouseScrollDelta::PixelDelta(pos).y` es LOGICAL POINTS.
Divisor: `cell_height / scale_factor`.

**JetBrains Mono ligatures:** bearing_x puede ser NEGATIVO — no clampar a 0.

**alacritty_terminal 1-cell selection:** limpiar con `clear_selection()` en click sin drag.
