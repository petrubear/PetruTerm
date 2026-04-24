# Changelog

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
