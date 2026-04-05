# PetruTerm — Build Phases

## Phase 0.5: Integration Spike (Risk Reduction)
**Goal:** Validate the three highest-risk integration points before committing to full Phase 1 scope. No polish, no config, no UI — just proof that the core stack works together.

### Deliverables
- [x] **Version pinning:** wgpu=29, winit=0.30.13, alacritty_terminal=0.25.1, cosmic-text=0.18.2, mlua=0.11.6, bytemuck=1.25.0 — `Cargo.toml` written, `cargo check` passes
- [x] **Spike 1 — Terminal grid + ligatures:** `alacritty_terminal` grid cells grouped into runs, shaped by `cosmic-text` with HarfBuzz (`calt`, `liga`, `dlig`), rasterized by `swash`, uploaded to wgpu glyph atlas, rendered as instanced quads. `->`, `=>`, `!=`, `>=`, `|>` all work. nvim/tmux verified.
- [~] **Spike 2 — Custom title bar on macOS:** NSWindow style mask + transparency applied via `objc2` (`apply_macos_custom_titlebar()`). Traffic lights show correctly. **Remaining:** window dragging from custom area (`setMovableByWindowBackground` currently NO).
- [x] **Spike 3 — PTY ↔ winit event loop wakeup:** PTY I/O thread bridges to main thread via `crossbeam-channel`; `about_to_wait()` polls events and calls `window.request_redraw()` — implemented in `src/app/mod.rs` + `src/term/pty.rs`
- [x] **Run extractor prototype:** `TextShaper::shape_line()` in `src/font/shaper.rs` — walks grid, groups cells into runs, shapes with HarfBuzz, maps glyph positions back to cell coordinates. Handles wide chars, PUA icons, ligatures.
- [x] **Document `alacritty_terminal` API surface:** recorded in `memory/MEMORY.md` — key types: `Term<T>`, `Dimensions` trait, `event::EventListener`, `tty::Options`, `event_loop::EventLoop::new()`, `event_loop::Notifier`

### Exit Criteria
All three spikes produce working proof-of-concept code. A single window opens, spawns a PTY, and renders shaped terminal output with ligatures at 60fps using the full pipeline: `alacritty_terminal` → run extractor → `cosmic-text` → `swash` → glyph atlas → `wgpu`. Custom title bar is draggable with correctly positioned traffic lights. Version compatibility of all crates is confirmed and pinned.

> **Status:** 5/6 done. Spike 2 partial (dragging from title bar area not wired).

---

## Phase 1: Core Terminal (MVP)
**Goal:** A working, fast terminal you can use daily — zsh, tmux, nvim, claude all run correctly.

### Deliverables
- [x] Cargo workspace + all Phase 1 dependencies compile (`cargo check` — zero errors)
- [x] wgpu + winit: blank window opens on macOS (Metal backend) — `app/mod.rs:158–183`, `resumed()` wires Metal + wgpu surface
- [x] `alacritty_terminal` integration: PTY spawns `/bin/zsh -l`, I/O works — `term/pty.rs:122–127`, env vars set (`xterm-256color`, `COLORTERM=truecolor`)
- [x] wgpu renderer: terminal cells render at 60fps — `renderer/gpu.rs:112` Fifo present mode; `RedrawRequested` build→upload→render loop
- [x] Font loading via `fontdb`, text shaping via `cosmic-text` + `swash` — `TextShaper` + `GlyphAtlas` implemented
- [x] Font ligatures enabled via HarfBuzz features (`calt=1`, `liga=1`, `dlig=1`) — configured in `FontConfig::default()` and Lua DSL
- [x] Glyph atlas on GPU (rasterize once, cache as texture) — `GlyphAtlas` with shelf packing + `TexelCopyTextureInfo` upload
- [x] Lua config DSL: `petruterm` global, `apply_to_config` module pattern — `src/config/lua.rs` fully implemented
- [x] Config hot-reload via `notify` watcher — `ConfigWatcher` implemented, polled in `about_to_wait()`
- [~] Custom title bar with `borderless` option — style mask + transparency done (`app/mod.rs:128–147`); **missing:** `setMovableByWindowBackground:YES` for window dragging
- [x] Window config: `initial_width`, `initial_height`, `start_maximized` — all in `WindowConfig` schema + applied in `resumed()`
- [x] Tab bar: create, close, switch (Cmd+1–9) — `TabManager` implemented, keybinds wired, renders in UI
- [x] Split panes: horizontal (Leader+%), vertical (Leader+"), binary tree layout — `PaneManager` + binary tree implemented
- [x] Pane navigation: vim-style Leader+hjkl — keybind dispatch in `input/mod.rs`
- [x] Command palette (Leader+p): fuzzy search, built-in actions only — `CommandPalette` + `SkimMatcherV2` implemented
- [x] Dracula Pro default color theme — `ColorScheme::dracula_pro()` in schema
- [x] Monolisa Nerd Font 16px default (JetBrains Mono as fallback) — `FontConfig::default()`
- [~] 100k scrollback lines, scroll bar enabled — scrollback wired (`term/mod.rs:91–100`, default 10k lines); **missing:** scroll bar not rendered (config field exists, no GPU draw code)
- [x] Leader key: Ctrl+B, 1000ms timeout — `LeaderConfig::default()` + dispatch in `input/mod.rs`; all custom binds Lua-configurable
- [x] Default config files shipped in `config/default/` and embedded in binary — `include_str!` in `config/mod.rs`
- [x] `.app` bundle script in `scripts/bundle.sh` — complete: builds binary, creates .app structure, writes Info.plist, signs ad-hoc
- [x] Mouse handling: click-to-focus, click-to-place cursor, drag selection, scroll wheel, SGR + X10 mouse reporting — `app/mod.rs:266–325`, `app/input/mod.rs:80–93`
- [~] Clipboard: Cmd+C copy, Cmd+V paste, OSC 52 — Cmd+C/V fully working (`app/input/mod.rs:167–187`); **missing:** OSC 52 read path (store works, load not fully wired in `mux.rs`)
- [~] Text selection: click-drag, double-click word select, triple-click line select — drag selection works; **missing:** double-click (`SelectionType::Word`) and triple-click (`SelectionType::Line`) not implemented
- [x] Cursor rendering: block/underline/beam shapes, blinking, cursor colors from theme — `app/renderer.rs:237–248`, 530ms blink, `cursor_bg`/`cursor_fg` from config
- [x] Resize handling: window resize → pane layout recalc → `Term::resize()` → content reflow — wired in `WindowEvent::Resized`
- [x] Run extractor: grid cell → text run grouping for `cosmic-text` ligature shaping — `font/shaper.rs:350–440` `shape_line()` fully integrated
- [x] PTY ↔ winit wakeup via channel — `crossbeam-channel` + `about_to_wait()` polling + `user_event()` wakeup
- [x] Error display for Lua config parse failures — logged via `log::error!`, stderr fallback
- [x] Font-not-found graceful fallback with user-visible warning — `font_available()` check + `log::warn!`
- [x] `log` crate integration for debug/diagnostic output — `env_logger` initialized in `main.rs`
- [x] Cmd+Q quit, Cmd+1–9 tab switching keybinds — implemented in `input/mod.rs`

### Exit Criteria
`cargo build --release` produces a working binary. The app opens on macOS,
spawns zsh, renders correctly with ligatures. `nvim`, `tmux`, and `claude`
all work inside it. Command palette opens. Tabs and pane splits work.
Config hot-reloads without restart.

> **Status:** Phase 1 COMPLETE (MVP criteria met). 3 polish items remain: title bar drag, scroll bar render, double/triple-click selection. OSC 52 read path minor gap.

---

## Phase 2: AI Layer
**Goal:** Full Warp-style LLM integration, all 4 AI features, provider flexibility.

### Deliverables
- [x] `LlmProvider` trait: `complete()` + `stream()` async methods — `llm/mod.rs:56–64`
- [x] OpenRouter provider (`https://openrouter.ai/api/v1`, any model) — `llm/openrouter.rs`, full HTTP + streaming
- [x] Ollama provider (`http://localhost:11434`, OpenAI-compat) — `llm/openai_compat.rs`, `ollama()` factory
- [x] LMStudio provider (`http://localhost:1234/v1`, OpenAI-compat) — `llm/openai_compat.rs`, `lmstudio()` factory
- [ ] AI mode toggle keybind: `Ctrl+Space` (configurable) — not wired; accessible via command palette only
- [ ] Inline AI block UI: `⚡ AI >` prompt, streaming response renders token-by-token — `llm/ai_block.rs` exists but is dead code; not rendered
- [~] **Feature 1 — NL → Shell Command:** LLM query + streaming response works (`app/ui.rs:70–104`); `last_assistant_command()` extracts command (`llm/chat_panel.rs:132–142`); Run bar rendered in history (`app/renderer.rs` — green `│ ⏎ cmd` line); Enter with empty input executes via PTY. **missing:** `[Edit]` `[Explain]` buttons (secondary UX)
- [~] **Feature 2 — Explain Last Output:** `explain_last_output()` scaffolded (`app/ui.rs:106–115`), wired to palette; **missing:** `<leader>e` keybind not connected
- [~] **Feature 3 — Fix Last Error:** `fix_last_error()` scaffolded (`app/ui.rs:117–130`), wired to palette; **missing:** `<leader>f` keybind not connected
- [~] **Feature 4 — Context-Aware Chat:** CWD + exit code + last command injected as system message (`app/ui.rs:77–82`, `llm/shell_context.rs`); **missing:** per-pane chat history persistence (currently global)
- [x] Command palette: "Enable AI Features" / "Disable AI Features" master toggle — `app/ui.rs:193–197`
- [x] `llm.lua` config module: `provider`, `model`, `api_key`, `base_url`, `features`, `enabled` — `config/lua.rs`
- [~] Shell integration script (`shell-integration.zsh`): exists at `scripts/shell-integration.zsh` but minimal; **missing:** full CWD/exit-code/history tracking writing to `~/.cache/petruterm/shell-context.json`
- [x] `config.llm.enabled = false` disables all AI features cleanly — checked in `app/ui.rs:33–37`

### Exit Criteria
Can type natural language and get a shell command. Can ask "why did that
fail?" after a non-zero exit. Can toggle AI off entirely from command
palette. Works with OpenRouter, Ollama, and LMStudio.

> **Status:** ~60% complete. Providers + config solid. Remaining: Ctrl+Space hotkey, Run/Edit/Explain buttons (Feature 1 UX), keybinds for Features 2 & 3, per-pane history, shell integration script.

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

> **Status:** Not started.
