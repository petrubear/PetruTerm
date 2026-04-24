# PetruTerm — Build Phases

> Fases 0.5–3.6 + A–E + D-1/D-2/D-3/D-4/D-5 archivadas en [`build_phases_archive.md`](./build_phases_archive.md).

---

## Phase 4: Lua Scripting — COMPLETA 2026-04-24

### F-1: Event hooks — `petruterm.on(event, fn)`
- [x] `config/lua.rs`: callbacks en `_pt_handlers` registry table (reemplaza stub no-op)
- [x] Disparar `tab_created` / `tab_closed` desde `mux.cmd_new_tab` / `cmd_close_tab`
- [x] Disparar `pane_split` / `pane_closed` desde `mux.cmd_split` / `cmd_close_pane`
- [x] Disparar `terminal_exit` desde `close_exited_terminals`
- [x] Disparar `ai_response` desde `poll_ai_events`

### F-2: Toast notifications — `petruterm.notify(msg, ms?)`
- [x] `toast: Option<(String, Instant)>` en `App`
- [x] Render: rect semitransparente + texto en overlay, esquina superior derecha
- [x] Redraw automático hasta expiración + frame final de limpieza
- [x] `petruterm.notify(msg, ms?)` en Lua API (`ms` opcional, default 3000)
