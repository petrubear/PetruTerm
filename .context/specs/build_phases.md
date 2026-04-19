# PetruTerm — Build Phases

> Phases 0.5–3 (complete) archived in [`build_phases_archive.md`](./build_phases_archive.md).

---

## Phase 3.5: Performance Sprint ⚡
**Status: Mayoría completa — Sprint cierre pendiente (P2/P3 tech debt + bench CI)**

| KPI | Target |
|-----|--------|
| Input-to-pixel latency p99 | < 8 ms |
| Input-to-pixel latency p50 | < 4 ms |
| Steady-state frame time | < 1 ms |
| Idle allocations | 0 |
| Cache-miss storm | < 16 ms |
| Startup (exec → first pixel) | < 80 ms |

---

### Sub-phase A: Measurement Infrastructure ✅ (parcial)

**Baselines (2026-04-14, M4 Max, release):**
| Benchmark | Antes | Despues (Sub-C) |
|-----------|-------|-----------------|
| `shape_line_ascii` | 5 643 ns | 317 ns (-94%) |
| `shape_line_ligatures` | 8 766 ns | 659 ns (-92%) |
| `shape_line_unicode` | 5 586 ns | ~5 700 ns (sin cambio) |

- [x] `benches/shaping.rs` con criterion
- [x] `benches/search.rs` — proxy sintético de `Mux::search_active_terminal` + `filter_matches` (2026-04-16)
- [x] Tracing + feature flag `profiling`: spans en `build_instances`, `shape_line`, `RedrawRequested`
- [x] Debug HUD (F12): frame time p50/p95, shape cache hit%, atlas fill%, instance count, GPU upload KB/frame
- [x] `.context/quality/PROFILING.md`
- [ ] Frame budget en `term_specs.md`
- [ ] Bench `build_instances` — **bloqueado**: `RenderContext`/`Mux` acoplados a `winit::EventLoopProxy`; requiere extraer CPU path a función pura.
- [ ] Bench `rasterize_to_atlas` — **bloqueado**: requiere `&wgpu::Queue`. Path forward: (a) bench sólo `swash_cache.get_image_uncached`+conversión RGBA, o (b) wgpu headless adapter en el bench.
- [ ] CI gating: regresion > 5% falla build
- [ ] Tracy integration, GPU timestamps, os_signpost, latency probe completo

---

### Sub-phase B: Idle Zero-Cost ✅

- [x] `ControlFlow::Wait` cuando idle (sin PTY, sin overlay, sin drag)
- [x] Cursor blink pausado en idle
- [x] GPU upload bytes counter en HUD
- [x] `poll_git_branch` → timer 1 Hz independiente (TD-PERF-19)
- [x] In-flight guard en git branch fetch
- [x] Cursor como overlay independiente — `build_cursor_instance`, fast path en RedrawRequested (2026-04-18)
- [x] Damage tracking con `Term::damage()` / `reset_damage()` en `collect_grid_cells_for` (2026-04-18)

---

### Sub-phase C: Hot Path Fast Paths ✅

- [x] Ligature scan bit: `bytes().any()` antes de HarfBuzz
- [x] ASCII fast path: skip HarfBuzz para ASCII sin ligature chars (317 ns vs 5 643 ns baseline)
- [x] Per-word shape cache: `HashMap<(u64,u32), ShapedRun>`, cap 512 entries
- [x] Space cell fast path: `' '` + default bg salta glyph pipeline
- [x] Row hash fix: hashea los 4 canales RGBA (bug: solo hasheaba canal rojo)
- [ ] Pre-shape warmup ASCII 32-126 al arranque
- [ ] Subpixel position quantization

---

### Sub-phase D: Memory & Allocator ✅ (parcial)

- [x] `mimalloc` como `#[global_allocator]`
- [x] Scratch buffers en `RenderContext`: `scratch_chars`, `scratch_str`, `scratch_colors`, `fmt_buf`
- [x] `ChatPanel` separator cached — `'─'.repeat(n)` solo en resize
- [ ] Bumpalo arena per-frame
- [ ] `smallvec` en hot paths
- [ ] `compact_str` para strings cortos

---

### Sub-phase E: Parallel Rendering

- [ ] Rayon per-pane parallel build
- [ ] Parallel row shaping en cache-miss storm
- [ ] Lock-free PTY ring buffer (`rtrb` SPSC)
- [x] PTY reader thread steered to efficiency cores via `QOS_CLASS_UTILITY` (OnceLock, once per thread)

---

### Sub-phase F: Latency Minimization ✅ (parcial)

- [x] `PresentMode::Mailbox → FifoRelaxed → Fifo` (auto por caps)
- [x] `desired_maximum_frame_latency: 2 → 1`
- [x] Adaptive PTY coalescing: <=2 eventos = redraw inmediato; >2 = 4ms window
- [x] Skip render cuando window ocluida (`WindowEvent::Occluded`)
- [x] Input-to-pixel latency probe (`RUST_LOG=petruterm=debug`)
- [ ] Input event priority sobre PTY en tick
- [ ] `CVDisplayLink` en macOS (experimental)
- [ ] `CAMetalLayer::setDisplaySyncEnabled(false)` (experimental)

---

### Sub-phase G: GPU Architecture

- [ ] Atlas split por tamano: 1024 ASCII + 4096 emoji/wide
- [ ] Persistent mapped ring buffer (3x frame in flight)
- [ ] Indirect draw para multi-pane
- [ ] Unificar bg + glyph en un solo pass
- [ ] GPU-resident grid (Phase 5+ candidate)

---

### Sub-phase H: Build & Release

- [ ] PGO con workload representativo
- [x] `target-cpu=apple-m1` en `bundle.sh`
- [x] `release-native` profile en `Cargo.toml` (`[profile.release-native]`)
- [x] Lua bytecode cache (`~/.cache/petruterm/lua-bc/*.luac`, mtime-validated)
- [ ] Config eager-load en paralelo con window creation

---

### Sprint cierre Phase 3.5 (PRÓXIMO — antes de Fases A–D)

**P2 prioritarios:**
- [ ] TD-MEM-23: `agent_step(&[Value])` — elimina `api_msgs.clone()` por round
- [ ] TD-MEM-13: Limitar `ReadFile` a 50k chars + max 5 rounds en agent loop
- [ ] TD-PERF-04: `scan_files()` → `spawn_blocking` en file picker
- [ ] TD-PERF-15: Clipboard copy/paste → `spawn_blocking`
- [ ] TD-PERF-21: Palette fuzzy matcher incremental (filtrar `last_results` si query extiende el anterior)

**P3 triviales:**
- [ ] TD-MEM-17: `streaming_buf.clear()` en `ChatPanel::close()`
- [ ] TD-MEM-24: `VecDeque` para `undo_stack` (`pop_front`/`push_back`)
- [ ] TD-PERF-18: Tokio pool → `.worker_threads(2)`
- [ ] TD-PERF-23: `leader_deadline: Instant` (evitar `elapsed()` syscall por keystroke)

**Benchmarks:**
- [ ] Desbloquear `build_instances` bench: extraer CPU path a función pura sin `winit`
- [ ] Desbloquear `rasterize_to_atlas` bench: variant swash-only sin `wgpu::Queue`
- [ ] CI gating: `critcmp`, falla si regresión >5% en `shape_line` / `build_instances` / `search`

**Descartado de Phase 3.5 (→ backlog Phase 2/futuro):**
- Sub-E: rayon per-pane, `rtrb` lock-free PTY ring buffer
- Sub-G: atlas split, persistent ring buffer, unificar bg+glyph pass
- Sub-H: PGO (requiere workloads reales de fases futuras)
- CVDisplayLink / CAMetalLayer: experimental, incierto
- "Zero allocs con dhat" y comparativa vs Alacritty: diferir

### Exit Criteria (Phase 3.5 — revisados)

- [x] Debug HUD (F12) operativo
- [x] `PROFILING.md` documentado
- [x] Damage tracking con alacritty_terminal
- [x] Cursor overlay fast path
- [x] Idle zero-cost (ControlFlow::Wait + focus guard)
- [ ] Sprint cierre: P2/P3 tech debt + bench CI gating
- _Diferidos: latency measurement formal, comparativa, dhat_

---

---

## Fase A: Fundación — Versionado + i18n
**Status: Not started**

- [ ] Bump `Cargo.toml` a `0.1.0`; crear `CHANGELOG.md` con historial resumido desde Phase 1
- [ ] Crate `rust-i18n`; detección de locale del sistema (macOS `NSLocale`)
- [ ] `locales/en.toml` + `locales/es.toml` con todos los strings de UI
- [ ] Scope inicial: menu labels, mensajes error LLM, panel AI, status bar labels

---

## Fase B: Menu Bar nativo macOS
**Status: Not started**

- [ ] Agregar crate `muda`; inicializar `MenuBar` en `main.rs` antes del event loop
- [ ] **File**: New Tab, New Pane (H/V), Close Tab, Close Pane, Quit
- [ ] **Edit**: Copy, Paste, Clear Scrollback, Find
- [ ] **AI Chat**: Toggle Panel, Send to AI, Explain Last Output, Fix Last Error, Clear Chat
- [ ] **Window**: New Workspace, Next/Prev Workspace, Next/Prev Tab, Minimize, Zoom
- [ ] **Help**: About PetruTerm (version via `env!("CARGO_PKG_VERSION")`), Open Config Folder
- [ ] Wiring de acciones vía `MenuEvent` a handlers existentes
- [ ] Labels via sistema i18n (Fase A)

---

## Fase C: Titlebar Custom + Workspaces
**Status: Not started**

### C-1: Titlebar custom (NSWindow híbrido)
- [ ] Via `objc2`: `setTitlebarAppearsTransparent(true)`, `setTitleVisibility(.hidden)`, `setStyleMask` — conservar traffic lights nativos
- [ ] Expandir área render wgpu para cubrir zona título
- [ ] Drag region via `NSWindow.setIsMovableByWindowBackground`
- [ ] Botón toggle sidebar en titlebar (izquierda, junto a traffic lights)

### C-2: Modelo Workspace en Mux
- [ ] Agregar `Workspace { id: usize, name: String, tabs: Vec<TabId> }` a `src/app/mux.rs`
- [ ] `Mux` pasa de `tabs: Vec<Tab>` a `workspaces: Vec<Workspace>` + `active_workspace_id`
- [ ] Operaciones tab/pane operan dentro del workspace activo (sin romper API existente)
- [ ] Workspace create / rename / close
- [ ] Leader keybinds: `W n` (nuevo), `W &` (cerrar), `W ,` (renombrar), `W j/k` (navegar)

### C-3: Sidebar de Workspaces
- [ ] Panel lateral izquierdo tipo drawer (slide-in/out animado)
- [ ] Toggle via botón titlebar
- [ ] Lista workspaces con indicador del activo (dot de color)
- [ ] Navegación: `j/k` mover, `Enter` activar, `c` crear, `&` cerrar, `r` renombrar inline, `Esc` cerrar sidebar

---

## Fase D: AI Chat — MCP + Skills
**Status: Not started**

### D-1: MCP config loader
- [ ] Leer `~/.config/petruterm/mcp/mcp.json` (formato estándar: `{ "mcpServers": { ... } }`)
- [ ] Merge con `.petruterm/mcp.json` en directorio de trabajo actual (proyecto tiene prioridad)

### D-2: MCP client (stdio transport)
- [ ] Spawn proceso por server, JSON-RPC 2.0 sobre stdin/stdout
- [ ] Implementar: `initialize`, `tools/list`, `tools/call`, `resources/list`, `resources/read`
- [ ] Lifecycle: spawn al abrir AI panel, kill al cerrar sesión/cambiar workspace
- [ ] Cada `ChatPanel` conecta al conjunto de MCP servers activos para su `cwd`

### D-3: MCP tool integration en chat
- [ ] LLM engine recibe tool list de MCP servers activos
- [ ] Rutear tool calls al server correcto
- [ ] Mostrar tool calls en panel AI (collapsible)

### D-4: Skills loader (formato agentskills.io)
- [ ] Escanear `~/.config/petruterm/skills/*/SKILL.md` al inicio
- [ ] Parsear frontmatter YAML: `name`, `description` (body cargado solo al activar)
- [ ] Escanear `.petruterm/skills/*/SKILL.md` en directorio actual
- [ ] Activación: por `/skill-name` en input o por relevancia de descripción vs query
- [ ] Inyectar body del skill activo al system prompt

### D-5: Project-level config
- [ ] `.petruterm/mcp.json` — MCP servers del proyecto
- [ ] `.petruterm/skills/` — Skills del proyecto
- [ ] Merge con global: proyecto tiene prioridad en conflictos de nombre

---

## Phase 4: Plugin Ecosystem
**Status: Not started — después de Fases A–D**

- [ ] Plugin loader: auto-scan `~/.config/petruterm/plugins/*.lua`
- [ ] lazy.nvim-style plugin spec
- [ ] Plugin Lua API: `petruterm.palette.register()`, `petruterm.on()`, `petruterm.notify()`
- [ ] Plugin event system: `tab_created`, `tab_closed`, `pane_split`, `ai_response`, `command_run`
- [ ] `petruterm.plugins.install("user/repo")`
- [ ] Plugin hot-reload
- [ ] Example plugin + documentation
