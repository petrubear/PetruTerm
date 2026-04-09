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
- [x] Custom title bar with `borderless` option — style mask + transparency done, `setMovableByWindowBackground:YES` wired (`app/mod.rs`)
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
- [x] Clipboard: Cmd+C copy, Cmd+V paste, OSC 52 read/write — all complete (`app/input/mod.rs`, `app/mux.rs` ClipboardLoad/ClipboardStore handlers)
- [x] Text selection: click-drag, double-click word select (Semantic), triple-click line select (Lines) — all wired via `InputHandler::register_click()` in `app/mod.rs`
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
- [x] AI mode toggle keybind: `Ctrl+Space` — `app/input/mod.rs`; 4-row inline AI block overlay
- [x] Inline AI block UI: state machine (Typing→Loading→Streaming→Done/Error), streaming token-by-token — `llm/ai_block.rs` + `build_ai_block_instances` in `app/renderer.rs`
- [x] **Feature 1 — NL → Shell Command:** LLM query + streaming works; Run bar (green `│ ⏎ cmd`); Enter executes via PTY — `app/ui.rs`, `app/renderer.rs`
- [x] **Feature 2 — Explain Last Output:** `explain_last_output()` wired to palette + `<leader>e` — `app/ui.rs`
- [x] **Feature 3 — Fix Last Error:** `fix_last_error()` wired to palette + `<leader>f` — `app/ui.rs`
- [x] **Feature 4 — Context-Aware Chat:** CWD + exit code + last command injected as system message; per-pane history via `HashMap<usize, ChatPanel>` — `app/ui.rs`, `llm/shell_context.rs`
- [x] Command palette: "Enable AI Features" / "Disable AI Features" master toggle — `app/ui.rs:193–197`
- [x] `llm.lua` config module: `provider`, `model`, `api_key`, `base_url`, `features`, `enabled` — `config/lua.rs`
- [x] Shell integration script (`shell-integration.zsh`): `preexec`/`precmd` hooks write CWD/exit-code/last-command to `~/.cache/petruterm/shell-context.json` — `scripts/shell-integration.zsh`
- [x] `config.llm.enabled = false` disables all AI features cleanly — checked in `app/ui.rs:33–37`

### Exit Criteria
Can type natural language and get a shell command. Can ask "why did that
fail?" after a non-zero exit. Can toggle AI off entirely from command
palette. Works with OpenRouter, Ollama, and LMStudio.

> **Status:** COMPLETE (2026-04-04). All Phase 2 deliverables shipped. Commit b815320 closed the final three items: per-pane history, Ctrl+Space inline block, and inline rendering.

---

## Phase 2.5: AI Agent Mode
**Goal:** Upgrade the chat panel into a context-aware coding agent — similar to avante.nvim — where the user can attach files from the current working directory, have `AGENTS.md` loaded automatically as project context, and let the LLM read, propose edits, and apply changes to files.

The chat panel and agent panel are **one unified panel** (`leader+a`). When the user asks a general question it responds as a chat assistant; when the user attaches files or the LLM needs file context, it operates as a file-aware agent.

### Deliverables

#### P1 — File Context Attachment ✅ (2026-04-05)
- [x] `ChatPanel` gains `attached_files: Vec<PathBuf>` — list of files injected into system context
- [x] Auto-load `AGENTS.md` from CWD on every panel open (if it exists)
- [x] File list section rendered at top of panel: `Selected (N files)` header + filenames
- [x] `Tab` key in panel toggles focus between file-picker and chat input
- [x] File picker: fuzzy-search files in CWD (reuse `fuzzy-matcher`), `Enter` to attach/detach
- [x] Attached file contents injected as `role: system` messages before user query
- [x] Token counter rendered in panel footer: `Tokens: NNNN`
- [x] Keybind: `<C-s>` submits (in addition to Enter)
- [x] CWD from real terminal process PID via `proc_pidinfo` (macOS) — no shell integration needed
- [x] `/q` / `/quit` in panel input closes panel + current tab

#### P2 — LLM Tool Use: Read & Explore ✅ (2026-04-05)
- [x] `AgentTool` enum: `ReadFile`, `ListDir` in OpenAI function-calling format — `src/llm/tools.rs`
- [x] `agent_step()` added to `LlmProvider` trait; both OpenRouter + OpenAI-compat implement it
- [x] Tool execution loop (max 10 rounds): call → execute → inject result → re-query — `src/app/ui.rs`
- [x] Streaming UI: `⟳ tool(path)` / `✓ tool(path)` inline via `AiEvent::ToolStatus` — `src/llm/chat_panel.rs`
- [x] Safety: `canonicalize()` + `starts_with(cwd)` check before any file access — `src/llm/tools.rs`

#### P3 — LLM Tool Use: Write & Run ✅ (2026-04-07)
- [x] `WriteFile { path, content }` tool: LLM proposes full file replacement
- [x] Diff preview rendered inline in panel (before/after lines with `+`/`-` colors)
- [x] Confirmation prompt: `[y] Apply  [n] Reject` before writing to disk
- [x] `RunCommand { cmd }` tool: executes in active PTY after user confirmation
- [x] Undo: keep original file in memory for single-step undo (`<leader>z` restores)

### Exit Criteria
Panel opens and auto-loads `AGENTS.md`. User can fuzzy-attach additional files. Token count
updates as files are added. LLM receives file contents as context and gives file-aware answers.
LLM can request to read additional files via tool use. LLM can propose file edits with a diff
preview; user confirms before any write happens.

> **Status:** COMPLETE (2026-04-07). P1+P2+P3 shipped. WriteFile+RunCommand con diff preview y confirmación. Undo stack via `<leader>z`.

---

## Phase 3: Polish & UI Chrome
**Goal:** Complete visual chrome — tab bar, scroll bar, status bar, snippets, Starship support.

### Deliverables

#### Priority — Visual Chrome (P1)
- [x] **Tab bar:** renders at grid row -1 (above terminal); active tab highlighted; Dracula Pro colors — `build_tab_bar_instances()` in `app/renderer.rs`; GPU padding shifted via `renderer.set_padding()`
- [x] **Scroll bar:** 6px right-edge overlay using FLAG_CURSOR; thumb proportional to `screen_rows / total_lines`; gated by `config.enable_scroll_bar` — `build_scroll_bar_instances()` in `app/renderer.rs`; `Terminal::scrollback_info()` in `term/mod.rs`
- [x] **Right-click context menu:** floating popup at click position; items: Copy (Cmd+C), Paste (Cmd+V), Clear; keybinds shown right-aligned; hover highlight; closes on click-outside, key press, or action — `src/ui/context_menu.rs`; `build_context_menu_instances()` in `app/renderer.rs`; mouse handling in `app/mod.rs`

#### Status Bar (P2) ✅ (2026-04-08)
- [x] Status bar engine: enable/disable from Lua + command palette (`ToggleStatusBar`)
- [x] Built-in status bar widgets: `mode` (leader indicator), `cwd`, `git_branch`, `time`, `exit_code`
- [x] Status bar position: `top` or `bottom` (Lua config `config.status_bar.position`)
- [x] Git branch polled async with 5s TTL cache (`poll_git_branch` in `app/ui.rs`)

#### Snippets & Compatibility (P3)
- [x] Tab rename: `<leader>,` — inline prompt in tab pill, Enter confirms, Esc cancels (2026-04-08)
- [ ] Snippets: `config.snippets` table in Lua, expand via command palette
- [ ] Snippet keybind: optional `trigger` field per snippet
- [ ] Starship compatibility: detect `STARSHIP_SHELL`, defer left prompt
- [ ] Powerline support: Nerd Font glyphs in custom widget strings
- [ ] Built-in themes as Lua files in `assets/themes/`

### Exit Criteria
Tab bar renders and reflects active tab. Scroll bar visible when scrollback is active.
Status bar renders with at least 3 widgets. Snippets expand via command palette.
Starship prompt works when enabled.

> **Status:** Not started.

---

## Phase 4: Plugin Ecosystem
**Goal:** Extensible plugin platform — third-party Lua plugins can extend palette, status bar, and events.

### Deliverables
- [ ] Plugin loader: auto-scan `~/.config/petruterm/plugins/*.lua`
- [ ] lazy.nvim-style plugin spec: `{ "id", enabled=bool, config = function() ... end }`
- [ ] Plugin Lua API: `petruterm.palette.register()`, `petruterm.on()`, `petruterm.notify()`
- [ ] Plugin event system: `tab_created`, `tab_closed`, `pane_split`, `ai_response`, `command_run`
- [ ] `petruterm.plugins.install("user/repo")` — git clone helper
- [ ] Plugin hot-reload (re-source plugin file on change)
- [ ] Example plugin + documentation

### Exit Criteria
A third-party Lua plugin can register a command palette action and a status bar widget.
Plugin hot-reload works. `install()` clones a repo into the plugins directory.

> **Status:** Not started.
