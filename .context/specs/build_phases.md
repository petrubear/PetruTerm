# PetruTerm — Build Phases

> Fases 0.5–3.6 + A–E + D-1/D-2/D-3/D-4/D-5 archivadas en [`build_phases_archive.md`](./build_phases_archive.md).

---

## Phase 4: Lua Scripting
**Status: Not started — próxima fase**

### F-1: Event hooks — `petruterm.on(event, fn)`
- [ ] `config/lua.rs`: almacenar callbacks en `HashMap<String, Vec<LuaFunction>>` (reemplazar stub no-op)
- [ ] Disparar `tab_created` / `tab_closed` desde `mux.cmd_new_tab` / `cmd_close_tab`
- [ ] Disparar `pane_split` desde `mux.cmd_split`
- [ ] Disparar `terminal_exit` desde `close_exited_terminals`
- [ ] Disparar `ai_response` desde `poll_ai_events`

### F-2: Toast notifications — `petruterm.notify(msg, ms?)`
- [ ] `toast: Option<(String, Instant, u64)>` en `App` (texto, deadline, duración ms)
- [ ] Render: rect semitransparente + texto en overlay, esquina superior derecha
- [ ] Solicitar redraw automático hasta expiración
- [ ] Exponer `petruterm.notify(msg, ms?)` en la Lua API (`ms` opcional, default 3000)
