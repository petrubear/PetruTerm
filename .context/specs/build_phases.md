# PetruTerm — Build Phases

## Phase 0.5: Integration Spike (Risk Reduction)
**Goal:** Validate the three highest-risk integration points before committing to full Phase 1 scope. No polish, no config, no UI — just proof that the core stack works together.

### Deliverables
- [x] **Version pinning:** wgpu=29, winit=0.30.13, alacritty_terminal=0.25.1, cosmic-text=0.18.2, mlua=0.11.6, bytemuck=1.25.0 — `Cargo.toml` written, `cargo check` passes
- [ ] **Spike 1 — Terminal grid + ligatures:** prove that `alacritty_terminal` grid cells can be grouped into text runs, shaped by `cosmic-text` with HarfBuzz ligature features (`calt`, `liga`, `dlig`), rasterized by `swash`, uploaded to a wgpu glyph atlas, and rendered as instanced quads. Must correctly render `->`, `=>`, `!=`, `>=`, `|>` as ligatures
- [ ] **Spike 2 — Custom title bar on macOS:** prove that `winit` + `raw-window-handle` + `objc2` can create a borderless window with a custom-drawn title bar area, correct traffic light (close/minimize/maximize) button positioning, and window dragging from the custom area
- [x] **Spike 3 — PTY ↔ winit event loop wakeup:** PTY I/O thread bridges to main thread via `crossbeam-channel`; `about_to_wait()` polls events and calls `window.request_redraw()` — pattern implemented in `src/app.rs` + `src/term/pty.rs`
- [ ] **Run extractor prototype:** implement a function that walks `alacritty_terminal`'s grid, groups consecutive cells with matching attributes into text runs suitable for `cosmic-text` shaping, and maps shaped glyph positions back to cell coordinates
- [x] **Document `alacritty_terminal` API surface:** recorded in `.context/quality/TECHNICAL_DEBT.md` and `memory/MEMORY.md` — key types: `Term<T>`, `Dimensions` trait, `event::EventListener`, `tty::Options`, `event_loop::EventLoop::new()`, `event_loop::Notifier`

### Exit Criteria
All three spikes produce working proof-of-concept code. A single window opens, spawns a PTY, and renders shaped terminal output with ligatures at 60fps using the full pipeline: `alacritty_terminal` → run extractor → `cosmic-text` → `swash` → glyph atlas → `wgpu`. Custom title bar is draggable with correctly positioned traffic lights. Version compatibility of all crates is confirmed and pinned.

### Risk Notes
- `alacritty_terminal` is an internal crate with no semver guarantees — pin the exact version and wrap it behind a thin abstraction layer so it can be forked or replaced if upstream breaks
- `cosmic-text` expects text runs, not individual cells — the run extractor is the critical bridge and must handle: attribute boundaries, wide characters, tab stops, and cells with `EMPTY` flags
- Custom title bar requires platform-specific `NSWindow` manipulation via `objc2`; `winit` alone cannot do this

---

## Phase 1: Core Terminal (MVP)
**Goal:** A working, fast terminal you can use daily — zsh, tmux, nvim, claude all run correctly.

### Deliverables
- [x] Cargo workspace + all Phase 1 dependencies compile (`cargo check` — zero errors)
- [ ] wgpu + winit: blank window opens on macOS (Metal backend)
- [ ] `alacritty_terminal` integration: PTY spawns `/bin/zsh -l`, I/O works
- [ ] wgpu renderer: terminal cells render at 60fps
- [x] Font loading via `fontdb`, text shaping via `cosmic-text` + `swash` — `TextShaper` + `GlyphAtlas` implemented
- [x] Font ligatures enabled via HarfBuzz features (`calt=1`, `liga=1`, `dlig=1`) — configured in `FontConfig::default()` and Lua DSL
- [x] Glyph atlas on GPU (rasterize once, cache as texture) — `GlyphAtlas` with shelf packing + `TexelCopyTextureInfo` upload
- [x] Lua config DSL: `petruterm` global, `apply_to_config` module pattern — `src/config/lua.rs` fully implemented
- [x] Config hot-reload via `notify` watcher — `ConfigWatcher` implemented, polled in `about_to_wait()`
- [ ] Custom title bar with `borderless` option
- [x] Window config: `initial_width`, `initial_height`, `start_maximized` — all in `WindowConfig` schema + applied in `resumed()`
- [x] Tab bar: create (Cmd+T), close (Cmd+W), switch (Cmd+1–9), rename — `TabManager` implemented, keybinds wired
- [x] Split panes: horizontal (Leader+%), vertical (Leader+"), binary tree layout — `PaneManager` + binary tree implemented
- [x] Pane navigation: vim-style Leader+hjkl — keybind dispatch in `app.rs` (focus nav pending render wiring)
- [x] Command palette (Cmd+Shift+P): fuzzy search, built-in actions only — `CommandPalette` + `SkimMatcherV2` implemented
- [x] Dracula Pro default color theme — `ColorScheme::dracula_pro()` in schema
- [x] Monolisa Nerd Font 16px default (JetBrains Mono as fallback) — `FontConfig::default()`
- [ ] 100k scrollback lines, scroll bar enabled — configured but not verified (see TD-004)
- [x] Leader key: Ctrl+B, 1000ms timeout — `LeaderConfig::default()` + dispatch in `app.rs`
- [x] Default config files shipped in `config/default/` and embedded in binary — `include_str!` in `config/mod.rs`
- [ ] `.app` bundle script in `scripts/bundle.sh`
- [ ] Mouse handling: click-to-focus pane, click-to-place cursor, drag text selection, scroll wheel for scrollback, mouse reporting modes (SGR, X10) for tmux/nvim
- [ ] Clipboard: Cmd+C copy selected text, Cmd+V paste, OSC 52 clipboard support
- [ ] Text selection: click-drag, double-click word select, triple-click line select
- [ ] Cursor rendering: block/underline/beam shapes, blinking, cursor colors from theme
- [x] Resize handling: window resize → pane layout recalc → `Term::resize()` → content reflow — wired in `WindowEvent::Resized`
- [ ] Run extractor: grid cell → text run grouping for `cosmic-text` ligature shaping — `TextShaper::shape_line()` written, needs integration
- [x] PTY ↔ winit wakeup via channel — `crossbeam-channel` + `about_to_wait()` polling pattern implemented
- [x] Error display for Lua config parse failures — logged via `log::error!`, stderr fallback
- [x] Font-not-found graceful fallback with user-visible warning — `font_available()` check + `log::warn!`
- [x] `log` crate integration for debug/diagnostic output — `env_logger` initialized in `main.rs`
- [x] Cmd+Q quit, Cmd+1–9 tab switching keybinds — implemented in `handle_key_input()`

### Exit Criteria
`cargo build --release` produces a working binary. The app opens on macOS,
spawns zsh, renders correctly with ligatures. `nvim`, `tmux`, and `claude`
all work inside it. Command palette opens. Tabs and pane splits work.
Config hot-reloads without restart.

---

## Phase 2: AI Layer
**Goal:** Full Warp-style LLM integration, all 4 AI features, provider flexibility.

### Deliverables
- [ ] `LlmProvider` trait: `complete()` + `stream()` async methods
- [ ] OpenRouter provider (`https://openrouter.ai/api/v1`, any model)
- [ ] Ollama provider (`http://localhost:11434`, OpenAI-compat)
- [ ] LMStudio provider (`http://localhost:1234/v1`, OpenAI-compat)
- [ ] AI mode toggle keybind: `Ctrl+Space` (configurable)
- [ ] Inline AI block UI: `⚡ AI >` prompt, streaming response renders token-by-token
- [ ] **Feature 1 — NL → Shell Command:** natural language input → command suggestion with `[⏎ Run]` `[Edit]` `[Explain]` actions
- [ ] **Feature 2 — Explain Last Output:** `Ctrl+Shift+E` or command palette → explain selected/last output
- [ ] **Feature 3 — Fix Last Error:** subtle indicator on non-zero exit, `Ctrl+Shift+F` → suggests corrected command
- [ ] **Feature 4 — Context-Aware Chat:** multi-turn chat with CWD + recent output + shell history context; persists per pane
- [ ] Command palette: "Enable AI Features" / "Disable AI Features" master toggle
- [ ] `llm.lua` config module: `provider`, `model`, `api_key`, `base_url`, `features`, `enabled`
- [ ] Shell integration script (`shell-integration.zsh`): tracks CWD, exit codes, command boundaries
- [ ] `config.llm.enabled = false` disables all AI features cleanly

### Exit Criteria
Can type natural language and get a shell command. Can ask "why did that
fail?" after a non-zero exit. Can toggle AI off entirely from command
palette. Works with OpenRouter, Ollama, and LMStudio.

---

## Phase 3: Ecosystem
**Goal:** Extensible plugin platform, status bar, snippets, Starship support.

### Deliverables
- [ ] Plugin loader: auto-scan `~/.config/petruterm/plugins/*.lua`
- [ ] lazy.nvim-style plugin spec: `{ "id", enabled=bool, config = function() ... end }`
- [ ] Plugin Lua API: `petruterm.palette.register()`, `petruterm.on()`, `petruterm.notify()`
- [ ] Plugin event system: `tab_created`, `tab_closed`, `pane_split`, `ai_response`, `command_run`
- [ ] Status bar engine (lua-line style): enable/disable from Lua + command palette
- [ ] Built-in status bar widgets: `mode`, `cwd`, `git_branch`, `time`, `exit_code`
- [ ] Status bar widget Lua API: `petruterm.statusbar.register_widget({ name, render })`
- [ ] Status bar position: `top` or `bottom` (Lua config)
- [ ] Snippets: `config.snippets` table in Lua, expand via command palette
- [ ] Snippet keybind: optional `trigger` field per snippet
- [ ] Starship compatibility: detect `STARSHIP_SHELL`, defer left prompt
- [ ] Powerline support: Nerd Font glyphs in custom widget strings
- [ ] `petruterm.plugins.install("user/repo")` — git clone helper
- [ ] Plugin hot-reload (re-source plugin file on change)
- [ ] Built-in themes as Lua files in `assets/themes/`
- [ ] Example plugin + documentation

### Exit Criteria
Status bar renders with at least 3 widgets. A third-party Lua plugin can
register a command palette action and a status bar widget. Snippets expand
via command palette. Starship prompt works when enabled.
