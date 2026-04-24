# PetruTerm — Build Phases Archive

Completed phases. Active phases in [`build_phases.md`](./build_phases.md).

---

## Fase E: Design Refactor — Visual Overhaul — COMPLETA 2026-04-23

Objetivo: estilo visual IDE moderno. Solo cambios visuales, sin nueva funcionalidad.

| Token | Hex | Uso |
|---|---|---|
| `BG_DEEP` | `#0e0e10` | Terminal area |
| `BG_PANEL` | `#131316` | Sidebar, AI panel |
| `BG_STATUS` | `#0a0a0c` | Status bar |
| `BORDER` | `#2a2a2f` | Divisores, bordes overlays |
| `ACCENT_TEAL` | `#4ec9b0` | Path en status bar |
| `ACCENT_AMBER` | `#d4a44c` | Branch git, elementos activos |

- [x] T1 — Paleta de colores base (constantes en `renderer.rs`)
- [x] T2 — Tab bar flat `zsh: N` (pills SDF → tabs flat con nombre de proceso)
- [x] T3 — Command palette overlay (corners ~8px, borde `#2a2a2f`, fondo `#131316`)
- [x] T4 — Sidebar + AI panel (`BG_PANEL`, header con borde separador)
- [x] T5 — Divisores de pane (1px lógico, `#2a2a2f`)
- [x] T6 — Status bar (`ACCENT_TEAL`/`ACCENT_AMBER`/`BG_STATUS`)
- [x] T7 — `md_style_line()` en AI panel (headers coloreados, bullets `•`, code verde)

---

## Fase D: AI Chat — MCP + Skills — COMPLETA 2026-04-24

### D-1: MCP config loader — COMPLETA 2026-04-24
- [x] `~/.config/petruterm/mcp/mcp.json` (`{ "mcpServers": { "name": { "command", "args", "env" } } }`)
- [x] Merge con `.petruterm/mcp.json` del proyecto (proyecto tiene prioridad)
- [x] XDG fallback para macOS (`dirs::config_dir()` → `~/Library/Application Support`)

### D-2: MCP client (stdio transport) — COMPLETA 2026-04-24
- [x] Spawn proceso por server, JSON-RPC 2.0 sobre stdin/stdout
- [x] `initialize`, `tools/list`, `tools/call`
- [x] `kill_on_drop(true)` al cerrar; PATH augmentado con `/opt/homebrew/bin:/usr/local/bin`
- [x] stderr → `Stdio::inherit()` para debugging

### D-3: MCP tool integration en chat — COMPLETA 2026-04-24
- [x] MCP tools PRIMERO, built-ins filtrados via `AgentTool::specs_excluding(mcp_names)`
- [x] Status lines: `✓ filesystem.list_directory(/tmp)` (server.tool() format)
- [x] Header badge `[mcp:N skills:M]` en AI panel

### D-4: Skills loader (agentskills.io format) — COMPLETA 2026-04-22
- [x] `SkillManager`: `load(cwd)`, fuzzy match (SkimMatcherV2, threshold 50), `read_body` lazy
- [x] `~/.config/petruterm/skills/<name>/SKILL.md` (global) + `.petruterm/skills/` (project-local)
- [x] Slash commands: `/skills` (color via `md_style_line`), `/mcp`, `/q`
- [x] `⚡ skill-name` en header AI panel cuando skill activo

### D-5: Project-level config + MCP hot-reload — COMPLETA 2026-04-24
- [x] `.petruterm/mcp.json` y `.petruterm/skills/` (project-local, implementado en D-1/D-4)
- [x] `config/watcher.rs`: filtro extendido a `.json`
- [x] `app/mod.rs`: `mcp_watcher` (notify sobre `.petruterm/`) + `mcp_reload_at` debounce 300ms
- [x] `app/ui.rs`: `reload_mcp(cwd)` — crea nuevo McpManager, start_all(), reemplaza Arc

---

## Fase C: Titlebar Custom + Workspaces — COMPLETA 2026-04-22

### C-1: Titlebar custom (NSWindow híbrido) — 2026-04-21
- [x] `TITLEBAR_HEIGHT = 30.0`; tab pills SDF; botones sidebar/AI/layout en titlebar
- [x] BTN_COLOR: Dracula Current Line [0.267, 0.278, 0.353, 1.0]; `padding.top = 5`

### C-2: Modelo Workspace en Mux — 2026-04-21
- [x] `Workspace { id, name }` en Mux; create/rename/close/switch/next/prev
- [x] Leader keybinds: `w` (nuevo, single key), `W &/,/j/k`; rename prompt

### C-3: Sidebar de Workspaces — 2026-04-21
- [x] Drawer lateral izquierdo; lista con dot indicador; `j/k/Enter/c/&/r/Esc`
- [x] Subtítulo `N tabs · M panes`; colores Dracula Pro

### C-3.5: AI panel right sidebar + iconos titlebar — 2026-04-22
- [x] Tercer botón AI en titlebar; iconos `≡` / `✦`; botones tintan purple cuando abierto
- [x] Header AI panel restyled: `SIDEBAR_BG + accent`, formato ` ✦ AI  provider:model`

---

## Fase B: Menu Bar nativo macOS — COMPLETA 2026-04-20
- [x] Crate `muda`; File/View/AI/Window menus; acciones via `MenuEvent` drain en `about_to_wait`

---

## Fase A: Fundación — Versionado + i18n — COMPLETA 2026-04-19 (v0.1.0)
- [x] `rust-i18n` 3.1; `locales/en.toml` + `locales/es.toml` (35 strings)
- [x] Release workflow `release.yml`; tag `v0.1.0` publicado

---

## Fase 3.6: GitHub Copilot Provider — COMPLETA 2026-04-19
- [x] `CopilotProvider` con JWT cache + auto-refresh; device flow OAuth
- [x] Auth: `GITHUB_TOKEN` → `gh auth token` → Keychain (`GITHUB_COPILOT_OAUTH_TOKEN`)

---

## Phase 3.5: Performance Sprint — Sub-phases completadas
**Archivado:** 2026-04-18 | **Activo:** ver Sprint cierre en `build_phases.md`

### KPIs (baselines 2026-04-14, M4 Max, release)
| Benchmark | Antes | Después |
|-----------|-------|---------|
| `shape_line_ascii` | 5 643 ns | 317 ns (-94%) |
| `shape_line_ligatures` | 8 766 ns | 659 ns (-92%) |
| `shape_line_unicode` | 5 586 ns | ~5 700 ns (sin cambio) |

### Sub-phase A: Measurement Infrastructure ✅ (parcial)
- [x] `benches/shaping.rs` con criterion
- [x] `benches/search.rs` — proxy sintético `Mux::search_active_terminal` + `filter_matches`
- [x] Tracing + feature flag `profiling`: spans en `build_instances`, `shape_line`, `RedrawRequested`
- [x] Debug HUD (F12): frame time p50/p95, shape cache hit%, atlas fill%, instance count, GPU upload KB/frame
- [x] `.context/quality/PROFILING.md`
- Pendiente → Sprint cierre: bench `build_instances`, bench `rasterize_to_atlas`, CI gating
- Descartado: Tracy integration, GPU timestamps, os_signpost

### Sub-phase B: Idle Zero-Cost ✅ COMPLETA
- [x] `ControlFlow::Wait` cuando idle
- [x] Cursor blink pausado en idle
- [x] GPU upload bytes counter en HUD
- [x] `poll_git_branch` → timer 1 Hz + in-flight guard (TD-PERF-19)
- [x] Cursor como overlay independiente — `build_cursor_instance`, fast path RedrawRequested (2026-04-18)
- [x] Damage tracking `Term::damage()` / `reset_damage()` en `collect_grid_cells_for` (2026-04-18)

### Sub-phase C: Hot Path Fast Paths ✅ (parcial)
- [x] Ligature scan bit: `bytes().any()` antes de HarfBuzz
- [x] ASCII fast path: skip HarfBuzz (317 ns vs 5 643 ns baseline)
- [x] Per-word shape cache: `HashMap<(u64,u32), ShapedRun>`, cap 512 entries
- [x] Space cell fast path: `' '` + default bg salta glyph pipeline
- [x] Row hash fix: hashea los 4 canales RGBA
- Descartado: pre-shape warmup ASCII 32-126, subpixel position quantization

### Sub-phase D: Memory & Allocator ✅ (parcial)
- [x] `mimalloc` como `#[global_allocator]`
- [x] Scratch buffers en `RenderContext`: `scratch_chars`, `scratch_str`, `scratch_colors`, `fmt_buf`
- [x] `ChatPanel` separator cached — `'─'.repeat(n)` solo en resize
- Descartado: Bumpalo arena, `smallvec`, `compact_str`

### Sub-phase E: Parallel Rendering — DESCARTADA (→ Phase 2)
- [x] PTY reader thread steered to efficiency cores via `QOS_CLASS_UTILITY`
- Descartado: rayon per-pane, parallel row shaping, `rtrb` lock-free PTY ring buffer

### Sub-phase F: Latency Minimization ✅ (parcial)
- [x] `PresentMode::Mailbox → FifoRelaxed → Fifo` (auto por caps)
- [x] `desired_maximum_frame_latency: 2 → 1`
- [x] Adaptive PTY coalescing: ≤2 eventos = redraw inmediato; >2 = 4 ms window
- [x] Skip render cuando window ocluida (`WindowEvent::Occluded`)
- [x] Input-to-pixel latency probe (`RUST_LOG=petruterm=debug`)
- Descartado: input event priority, CVDisplayLink, CAMetalLayer

### Sub-phase G: GPU Architecture — DESCARTADA (→ Phase 2)
Todos los items diferidos: atlas split, persistent ring buffer, indirect draw, unify passes, GPU-resident grid.

### Sub-phase H: Build & Release ✅ (parcial)
- [x] `target-cpu=apple-m1` en `bundle.sh`
- [x] `release-native` profile en `Cargo.toml`
- [x] Lua bytecode cache (`~/.cache/petruterm/lua-bc/*.luac`, mtime-validated)
- Descartado: PGO (requiere workloads reales), config eager-load

---

## Phase 0.5: Integration Spike (Risk Reduction)
**Status: COMPLETE** (partial — Spike 2 funcional pero drag desde título area no wired)

**Goal:** Validate the three highest-risk integration points before committing to full Phase 1 scope.

### Deliverables
- [x] Version pinning: wgpu=29, winit=0.30.13, alacritty_terminal=0.25.1, cosmic-text=0.18.2, mlua=0.11.6, bytemuck=1.25.0
- [x] Spike 1 — Terminal grid + ligatures: `->`, `=>`, `!=`, `>=`, `|>` all work. nvim/tmux verified.
- [~] Spike 2 — Custom title bar on macOS: NSWindow style mask + transparency applied. `setMovableByWindowBackground:YES` wired.
- [x] Spike 3 — PTY ↔ winit event loop wakeup via crossbeam-channel + `about_to_wait()`
- [x] Run extractor prototype: `TextShaper::shape_line()` handles wide chars, PUA icons, ligatures.
- [x] Document `alacritty_terminal` API surface in `memory/MEMORY.md`

---

## Phase 1: Core Terminal (MVP)
**Status: COMPLETE** (2026-03-27)

**Goal:** A working, fast terminal you can use daily — zsh, tmux, nvim, claude all run correctly.

### Deliverables
- [x] Cargo workspace + all Phase 1 dependencies compile
- [x] wgpu + winit: blank window opens on macOS (Metal backend)
- [x] `alacritty_terminal` integration: PTY spawns `/bin/zsh -l`
- [x] wgpu renderer: terminal cells render at 60fps
- [x] Font loading via `fontdb`, text shaping via `cosmic-text` + `swash`
- [x] Font ligatures enabled via HarfBuzz features (`calt=1`, `liga=1`, `dlig=1`)
- [x] Glyph atlas on GPU (rasterize once, cache as texture)
- [x] Lua config DSL: `petruterm` global, `apply_to_config` module pattern
- [x] Config hot-reload via `notify` watcher
- [x] Custom title bar with `borderless` option
- [x] Window config: `initial_width`, `initial_height`, `start_maximized`
- [x] Tab bar: create, close, switch (Cmd+1–9)
- [x] Split panes: horizontal (Leader+%), vertical (Leader+"), binary tree layout
- [x] Pane navigation: vim-style Leader+hjkl
- [x] Command palette (Leader+p): fuzzy search, built-in actions only
- [x] Dracula Pro default color theme
- [x] Monolisa Nerd Font 16px default (JetBrains Mono as fallback)
- [x] 100k scrollback lines, scroll bar enabled
- [x] Leader key: Ctrl+B, 1000ms timeout
- [x] Default config files shipped in `config/default/` and embedded in binary
- [x] `.app` bundle script in `scripts/bundle.sh`
- [x] Mouse handling: click-to-focus, drag selection, scroll wheel, SGR + X10 reporting
- [x] Clipboard: Cmd+C copy, Cmd+V paste, OSC 52 read/write
- [x] Text selection: click-drag, double-click word, triple-click line
- [x] Cursor rendering: block/underline/beam, blinking, cursor colors from theme
- [x] Resize handling: window resize → pane layout recalc → `Term::resize()` → reflow
- [x] Run extractor: grid cell → text run grouping for `cosmic-text` ligature shaping
- [x] PTY ↔ winit wakeup via channel
- [x] Error display for Lua config parse failures
- [x] Font-not-found graceful fallback
- [x] `log` crate integration
- [x] Cmd+Q quit, Cmd+1–9 tab switching

---

## Phase 2: AI Layer
**Status: COMPLETE** (2026-04-04)

**Goal:** Full Warp-style LLM integration, all 4 AI features, provider flexibility.

### Deliverables
- [x] `LlmProvider` trait: `complete()` + `stream()` async methods
- [x] OpenRouter provider (any model)
- [x] Ollama provider (OpenAI-compat)
- [x] LMStudio provider (OpenAI-compat)
- [x] AI mode toggle keybind: `Ctrl+Space` — 4-row inline AI block overlay
- [x] Inline AI block UI: state machine (Typing→Loading→Streaming→Done/Error)
- [x] Feature 1 — NL → Shell Command + Run bar + Execute via PTY
- [x] Feature 2 — Explain Last Output (`<leader>e`)
- [x] Feature 3 — Fix Last Error (`<leader>f`)
- [x] Feature 4 — Context-Aware Chat (CWD + exit code + last command; per-pane history)
- [x] Command palette: "Enable AI Features" / "Disable AI Features" toggle
- [x] `llm.lua` config module
- [x] Shell integration script (`shell-integration.zsh`)
- [x] `config.llm.enabled = false` disables all AI cleanly

---

## Phase 2.5: AI Agent Mode
**Status: COMPLETE** (2026-04-07)

**Goal:** Upgrade the chat panel into a context-aware coding agent (avante.nvim-style).

### Deliverables

#### P1 — File Context Attachment ✅
- [x] `ChatPanel.attached_files: Vec<PathBuf>`
- [x] Auto-load `AGENTS.md` from CWD on every panel open
- [x] File list section rendered at top of panel
- [x] `Tab` key toggles focus between file-picker and chat input
- [x] File picker: fuzzy-search files in CWD, `Enter` to attach/detach
- [x] Attached file contents injected as `role: system` messages
- [x] Token counter rendered in panel footer
- [x] `<C-s>` submits (in addition to Enter)
- [x] CWD from real terminal process PID via `proc_pidinfo` (macOS)
- [x] `/q` / `/quit` closes panel + current tab

#### P2 — LLM Tool Use: Read & Explore ✅
- [x] `AgentTool` enum: `ReadFile`, `ListDir` in OpenAI function-calling format
- [x] `agent_step()` added to `LlmProvider` trait; both providers implement it
- [x] Tool execution loop (max 10 rounds)
- [x] Streaming UI: `⟳ tool(path)` / `✓ tool(path)` inline
- [x] Safety: `canonicalize()` + `starts_with(cwd)` check

#### P3 — LLM Tool Use: Write & Run ✅
- [x] `WriteFile { path, content }` tool with diff preview
- [x] Confirmation prompt: `[y] Apply  [n] Reject`
- [x] `RunCommand { cmd }` tool with user confirmation
- [x] Undo: `<leader>z` restores last written file

---

## Phase 3: Polish & UI Chrome
**Status: COMPLETE** (2026-04-09)

**Goal:** Complete visual chrome — tab bar, scroll bar, status bar, snippets, Starship support.

### Deliverables

#### P1 — Visual Chrome ✅
- [x] Tab bar: renders above terminal; active tab highlighted; Dracula Pro colors
- [x] Scroll bar: 6px right-edge overlay; proportional thumb; gated by `config.enable_scroll_bar`
- [x] Right-click context menu: floating popup; Copy/Paste/Clear; hover highlight
- [x] Pane resize — keyboard: `<leader>+Option+←→↑↓` adjusts ratio in 0.05 steps
- [x] Pane resize — mouse drag: ±8px hit-test on separator; live drag

#### P2 — Status Bar ✅ (2026-04-08)
- [x] Status bar engine: enable/disable from Lua + command palette (`ToggleStatusBar`)
- [x] Built-in widgets: `mode`, `cwd`, `git_branch`, `time`, `exit_code`
- [x] Position: `top` or `bottom` (Lua config)
- [x] Git branch polled async with 5s TTL cache
- [x] Per-pane exit code badge + click to view details (2026-04-10)

#### P3 — Snippets & Compatibility ✅
- [x] Tab rename: `<leader>,`
- [x] Snippets: `config.snippets` table in Lua, expand via command palette
- [x] Snippet keybind: optional `trigger` field — Tab-expand with input_echo tracker
- [x] Powerline support: Nerd Font arrows via `config.status_bar.style = "powerline"`
- [x] Built-in themes: Lua files in `~/.config/petruterm/themes/`, theme picker in palette

#### Additional ✅
- [x] Cmd+K — clear screen + scrollback (2026-04-10)
- [x] Cmd+F — text search in terminal + scrollback with highlight (2026-04-10)
