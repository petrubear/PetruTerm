# PetruTerm — Build Phases Archive

Completed phases. Active phases in [`build_phases.md`](./build_phases.md).

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
