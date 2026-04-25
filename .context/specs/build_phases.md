# PetruTerm — Build Phases

> Fases 0.5–3.6 + A–E + D-1/D-2/D-3/D-4/D-5 + Phase 4 archivadas en [`build_phases_archive.md`](./build_phases_archive.md).

---

## Phase 5: UX Polish

### G-1: Maximizar / minimizar panes
- [ ] `Leader z` — zoom del pane activo (ocupa todo el área de terminal, oculta separadores)
- [ ] Segundo `Leader z` — restaura layout anterior
- [ ] Estado `zoomed_pane: Option<usize>` en `Mux`; renderer omite otros panes cuando activo
- [ ] Indicador visual en status bar cuando hay zoom activo

### G-2: Sidebar — MCP, Steering, Skills
- [ ] Panel lateral colapsable (drawer estilo VSCode) en el lado izquierdo
- [ ] Tab "MCP": lista de servidores conectados + tools disponibles por servidor
- [ ] Tab "Steering": lista de archivos cargados desde `~/.config/petruterm/steering/` y `<cwd>/.petruterm/steering/`; permite abrir en editor
- [ ] Tab "Skills": lista de skills cargados con nombre + descripción; filtro fuzzy
- [ ] Keybind `Leader s` para abrir/cerrar; navegación con `j/k`, `Enter` para acción

### G-3: Markdown en chat
- [ ] Parser Markdown inline: `**bold**`, `*italic*`, `` `code` ``, `# headings`
- [ ] Bloques de código con resaltado de sintaxis (al menos: rs, py, js, ts, sh, json)
- [ ] Listas (`-`, `1.`) con indentación correcta
- [ ] Render en GPU: mapear spans a colores del tema activo
- [ ] Ancho de wrap respeta el ancho del panel (`PANEL_COLS`)
