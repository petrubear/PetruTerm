# Changelog

## [1.0.1] — 2026-09-17

### Fixed
- File drag-and-drop now works in `gpui-petruterm` (was previously wgpu-binary only).

---

## [1.0.0] — 2026-09-17

### Added
- `gpui-petruterm` — a second binary with a chrome built on [gpui](https://www.gpui.rs/) (Zed's retained-mode UI framework), at feature parity with the wgpu/winit binary: grid rendering, tabs/panes, sidebars (Workspaces/MCP/Skills/Steering), AI chat panel + inline block, command palette, search, context menu, toasts, and prompt-context injection (skills/steering/MCP).
- ACP (Agent Client Protocol) is now the default LLM backend for both binaries.
- `./scripts/bundle.sh` builds and packages both binaries into `dist/PetruTerm.app` and `dist/PetruTerm-gpui.app`, each with its own bundle ID and icon.
- Bundled `PetruTheme Dark`/`PetruTheme Light` themes.
- A colored focus border around the active pane, the battery status-bar widget, and a floating-card chrome restyle (sidebar drag-to-resize, aligned header rows, per-tab accent color on inactive tabs) ported to `gpui_shell`.

### Fixed
- `config.colors` set in `config.lua`/`ui.lua` was silently never applied — now read and honored (pre-existing bug, not introduced by the gpui work).

This release folds in the full gpui chrome migration (milestones M0-M5d); see `.context/core/SESSION_STATE.md` and `.context/quality/TECHNICAL_DEBT.md` for the detailed, commit-by-commit history.

---

## [0.4.1] — 2026-08-20

### Changed
- `[PERF-ROI-01]` incremental rendering: merged GPU range uploads, stable per-row GPU slots for terminal instances (only changed rows re-upload), row damage propagation via alacritty's `TermDamage`, and new performance baseline counters.
- `[PERF-ROI-02]` PTY wakeups coalesced instead of firing per read.

---

## [0.4.0] — 2026-08-03

### Added
- Bold, italic, and weight variants now render in the terminal grid, resolved automatically from the installed font family's own faces via fontdb.

### Changed
- `GRAPH-ARCH-01`: consolidated several duplicated `Config`-reading call sites behind shared views/helpers (`LlmRuntimeView`, `LeaderBindingsView`, `RenderContext::locate_scaled_font`, `App::frame_interval`).
- Status bar clock now shows local time instead of UTC.

---

## [0.3.1] — 2026-07-08

### Fixed
- A lost `EventLoopProxy` wakeup on macOS that could leave pasted text or Atuin selections unrendered until a manual scroll; the PTY-echo grace window is now armed only on real PTY writes and scoped correctly.
- Left-margin cell of the tab color-picker menu row not painting.
- Various critical/high-severity code review findings.

### Changed
- Bundled themes now curate an explicit `ui_border` color instead of deriving one.

---

## [0.3.0] — 2026-07-03

### Added
- Phase 9 UI Restyle: floating-surface chrome (sidebar, command palette, chat panel, tab bar, status bar), macOS window blur/vibrancy (`config.window.blur`), rounded window corners, and translucent surface color tokens.

---

## [0.2.1] — 2026-07-01

### Added
- ACP (Agent Client Protocol) backend for the AI chat panel: `llm.backend = "agent"` lets an external ACP agent process (e.g. Claude Code via `@agentclientprotocol/claude-agent-acp`) drive the panel instead of a direct LLM provider. Same `AiEvent` stream as the provider backend, so the chat UI doesn't need to distinguish between them.
- `terminal/create` support: an ACP agent can open a real terminal pane (split) to run commands, and read back its output/exit code via `terminal/output` and `terminal/wait_for_exit`.
- `fs/read_text_file` and `fs/write_text_file` support, with the existing write-confirmation UI and undo stack (`Leader a z`).
- `/model` and `/agent` slash commands in the chat panel to switch LLM provider/model or ACP agent on the fly.
- Chat panel header now shows `◈ <agent>` for the agent backend vs `✦ <model>` for the provider backend.

### Fixed
- `terminal/output` no longer returns an empty string once the underlying command has exited — output is now cached at pane-close time instead of being lost when the pane auto-closes.
- Undo (`Leader a z`) after an agent-driven file write now restores the actual original content instead of a no-op (the original was being read back *after* the write instead of before).
- Path validation for ACP filesystem requests now canonicalizes before checking the `$HOME` boundary, closing a `..` traversal bypass.
- Commands/arguments passed to `terminal/create` are now shell-quoted instead of joined with raw spaces, so arguments containing spaces or shell metacharacters are passed through literally.
- Connecting to an ACP agent (startup, config hot-reload, `/agent`) no longer blocks the UI thread — the connection is established in the background and polled without blocking.
- Fixed a crash (`String::remove` past the end of the input buffer) that could happen on the next backspace after using any slash command in the chat panel.

---

## [0.1.5] — 2026-05-05

### Changed
- Reduced hot-path chat panel allocations by reusing the render formatting buffer instead of rebuilding strings with repeated `format!()` calls.
- Deduplicated redraw scheduling so the app issues at most one `window.request_redraw()` per event-loop cycle.
- Grouped sidebar UI state into `SidebarState` and split large chat-panel and window-event handlers into smaller private helpers without changing behaviour.
- Shell integration now writes per-pane context files (`shell-context-$$.json`) so multiple panes do not overwrite each other's AI context.

### Documentation
- Synced the README with the current first-launch config seeding flow, the six-module config layout, current UI/perf defaults, and the per-pane shell integration behaviour.

---

## [0.1.4] — 2026-04-24

### Added
- MCP (Model Context Protocol) support: the AI agent panel can now connect to external MCP servers defined in `~/.config/petruterm/mcp/mcp.json`.
- Built-in `filesystem` MCP server pre-configured (exposes `/private/tmp` and `~`).
- PostgreSQL MCP server support via `@modelcontextprotocol/server-postgres`; configure with a standard `postgresql://` connection string.
- `/mcp` slash command in the AI panel lists all connected MCP servers and their available tools.

---

## [0.1.3] — 2026-04-23

### Fixed
- Focus border on the left pane in a horizontal split no longer overlaps text in column 0. The border rect is now shifted one cell outward on the left side when the pane is at the viewport left edge (`col_offset == 0`), matching the behaviour of panes that have a separator on their left.

---

## [0.1.2] — 2026-04-23

### Fixed
- Focus border alignment: `pane_rect` edges are now snapped to the cell grid in `collect_leaf_infos_impl`, so the border lines up exactly with separator lines at all DPI settings.

### Changed
- Rounded-rect shader extended with `border_width` field for stroke-ring mode (`border_width > 0` = ring; `0` = filled).
- `build_focus_border` replaced four 1 px filled rects with a single `RoundedRectInstance` stroke ring (`border_width = 1.5 × scale_factor`, `radius = 6 × scale_factor`).

---

## [0.1.1] — 2026-04-19

### Features
- GitHub Copilot provider (`provider = "copilot"`) with device-flow OAuth and automatic Keychain storage
- Chat panel header shows active provider and model (`copilot:gpt-4o-mini`)
- Shared SSE/agent-response parsing extracted to `llm/mod.rs` (removes duplication across providers)

### Supported Copilot models
`gpt-4o`, `gpt-4o-mini`, `claude-3.5-sonnet`, `claude-3.7-sonnet`, `o3-mini`, `o1-mini`

---

## [0.1.0] — 2026-04-19

First tagged release. Phases 1–3 + 3.5 complete.

### Features
- GPU-accelerated terminal rendering (wgpu / Metal on macOS)
- Full PTY support via alacritty_terminal (xterm-256color, truecolor, KKP)
- Tab bar with pill/SDF rendering and rename support
- Pane splits (horizontal/vertical) with keyboard and mouse-drag resize
- Powerline-style status bar (git branch, CWD, exit code, time)
- Scrollback with scroll bar and trackpad scroll
- Font ligatures (JetBrains Mono / Monolisa), emoji, Nerd Font icons
- LCD subpixel antialiasing
- Inline AI block and side chat panel with per-pane context
- LLM providers: OpenRouter, Ollama, LMStudio; streaming responses
- Tool use: read_file, write_file, run_command (with confirmation UI)
- Command palette with fuzzy search
- Context menu (right-click)
- Snippet expansion (Tab)
- Built-in themes (Dracula Pro default)
- Theme picker
- Kitty Keyboard Protocol (Shift+Enter, etc.)
- macOS login-shell env inheritance for .app bundle launches
- Lua config DSL with hot-reload
- i18n: English and Spanish locales; auto-detected from LANG env var
