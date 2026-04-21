# Session State

**Last Updated:** 2026-04-20
**Session Focus:** Fase B COMPLETA — Menu Bar nativo macOS.

## Branch: `master`

## Estado actual

**Phase 1–3 + 3.5 COMPLETE. Fase A COMPLETE. Fase 3.6 COMPLETE. Fase B COMPLETE.**
**Build limpio: check + test + clippy + fmt PASS. CI verde.**
**Siguiente: Fase C — Titlebar custom (NSWindow híbrido) + Workspaces**

## Commits recientes relevantes

| Commit | Descripción |
|--------|-------------|
| fix    | Fix menu events never firing — use muda static receiver |
| fix    | Apply tab bar padding and terminal resize after menu actions |
| fix    | Restructure menu bar — correct macOS conventions |
| feat   | Add native macOS menu bar via muda |
| fix    | Fix Copilot OAuth docs and .app bundle auth visibility |

## Fase B — Menu Bar nativo macOS — CERRADA 2026-04-20

**Implementado:**
- `src/app/menu.rs`: `AppMenu` struct con muda. Menus: PetruTerm (app), File, View, AI, Window
- File: Settings (abre `~/.config/petruterm/` en Finder), Reload Config
- View: Toggle Status Bar, Switch Theme, Toggle Fullscreen
- AI: Toggle Panel, Explain, Fix Error, Undo Write, Enable/Disable
- Window: predefined macOS + Tab submenu (New/Close/Rename/Next/Prev) + Pane submenu (Split H/V, Close, Focus dirs)
- Sin aceleradores — keybinds son leader-based y no se pueden registrar como menu shortcuts
- `OpenConfigFolder` action agregada (abre carpeta config en Finder)

**Key non-obvious finding:**
- `muda::MenuEvent::set_event_handler` y `receiver()` son mutuamente exclusivos.
  Con `set_event_handler` activo, `receiver()` siempre vacío. Solución: no usar handler, solo `receiver()`.
- El drain de menu events debe hacerse en `about_to_wait()`, no en `user_event()`.
- Después de dispatch de acción de menu, hay que replicar el bloque post-accion del handler
  `KeyboardInput`: capturar `tab_count_before`/`pane_count_before` y llamar
  `apply_tab_bar_padding()` + `resize_terminals_for_panel()` si cambian. Sin esto,
  nuevos tabs/panes desde el menu se renderizan con viewport de altura cero.

---

## Fase 3.6 — GitHub Copilot Provider — CERRADA 2026-04-19

**Implementado:**
- `src/llm/copilot.rs`: device-flow OAuth con `client_id = Iv1.b507a08c87ecfe98`
- Token almacenado en macOS Keychain (`PetruTerm` / `GITHUB_COPILOT_OAUTH_TOKEN`)
- Copilot JWT auto-refresh cada ~30 min
- Fallback de modelo: si el modelo configurado tiene `/` o `:` (OpenRouter/Ollama), usa `gpt-4o`
- Header del chat panel muestra `provider:model` activo
- SSE helpers extraidos a `mod.rs` — eliminada duplicacion entre openrouter/openai_compat
- README: sección "Storing API keys securely (macOS Keychain)" con comandos para ambos providers

**Key non-obvious finding:**
- El endpoint `/copilot_internal/v2/token` solo acepta tokens de OAuth apps registradas para Copilot.
  Ni `gh auth token` ni PAT classic sirven — requiere device flow con `client_id = Iv1.b507a08c87ecfe98`.

---

## ⚠️ TAREA PRIORITARIA antes de Fase B — Copilot OAuth fix

### Problema
El README documenta el auth de Copilot como **PAT clásico**, pero la implementación real usa **device-flow OAuth** (`client_id = Iv1.b507a08c87ecfe98`). La documentación está desactualizada y engañosa.

Además, el device-flow OAuth funciona correctamente al ejecutar con `cargo run`, pero **no se ha validado desde el `.app` bundle**. Cuando la app se lanza como bundle, el flujo de device auth (que abre el browser y espera que el usuario ingrese el código) puede fallar silenciosamente si hay restricciones de entorno (sandbox, env vars, permisos de Keychain, apertura de browser con `open`).

### Tareas

1. **Actualizar README** — reemplazar la sección "GitHub Copilot" (líneas ~384-411) para describir el device-flow:
   - Al usar `provider = "copilot"` por primera vez, PetruTerm inicia el device-flow automáticamente.
   - Imprime en consola (o muestra en el panel de chat) la URL y el código de activación.
   - El token OAuth resultante se guarda en Keychain (`PetruTerm` / `GITHUB_COPILOT_OAUTH_TOKEN`).
   - No se necesita crear ni copiar ningún token manualmente.
   - Quitar toda mención a PAT clásico.

2. **Validar device-flow desde el `.app` bundle**:
   - Ejecutar `PetruTerm.app` (o bundle de `scripts/bundle.sh`), ir al panel de AI con `Leader+a`, seleccionar provider `copilot`.
   - Verificar que el device-flow se dispara: la URL/código aparece en el panel o en los logs.
   - Verificar que `open` abre el browser correctamente desde el contexto del bundle.
   - Verificar que el token se guarda y se reutiliza en sesiones posteriores.
   - Si el browser no abre, fallback: mostrar la URL copiable en el panel de chat.

3. **Fix si el `.app` falla** — posibles causas conocidas:
   - `std::process::Command::new("open")` puede necesitar ruta absoluta en bundles: `/usr/bin/open`.
   - El Keychain prompt puede no mostrarse si la app no tiene foco; verificar `SecKeychainItemCopyContent` vs `security` CLI.
   - La salida de consola (donde se imprime el código) no es visible desde `.app`; el código/URL debe aparecer en la UI (panel de chat o una ventana modal).

---

## Próximo: Fase B — Menu Bar nativo macOS

- Crate `muda`; inicializar `MenuBar` en `main.rs` antes del event loop
- Menus: File, Edit, AI Chat, Window, Help
- Labels via i18n

---

## Sesiones anteriores (resumen)

### 2026-04-19 — Fase A + Fase 3.6
- v0.1.0 publicado (Fase A: versionado + i18n)
- Fase 3.6: GitHub Copilot provider

### 2026-04-19 — Sprint cierre Phase 3.5
- Deuda P2/P3 cerrada, benches desbloqueados, CI verde, status bar flicker fix

### 2026-04-18 (tarde) — Bug fixes prioritarios
- KKP Shift+Enter, tab bleed, CI clippy, .app env vars

### 2026-04-18 (mañana) — Tier 3 + Tier 0
- TD-MEM-19, cursor overlay fast path, damage tracking, latency HUD, CI setup, TD-OP-02

### 2026-04-17 — Tier 1 + Tier 2 + Tier 4
- TD-MEM-20/21/12/10/11, TD-PERF-37/22/34/31/32/33/20/17

### 2026-04-16 — TD-RENDER-01 real fix + TD-RENDER-03
- Pre-pass bg-only vertices, mouse_dragged + clear_selection

### 2026-04-15 — Phase 3.5 Memory + Performance sprint
- TD-MEM-01..08, TD-PERF-06..13
