# PetruTerm

[![CI](https://github.com/petrubear/PetruTerm/actions/workflows/ci.yml/badge.svg)](https://github.com/petrubear/PetruTerm/actions/workflows/ci.yml)

A developer-first GPU-accelerated terminal emulator written in Rust. Built for speed and extensibility, with first-class AI integration, a Lua configuration DSL, font ligatures, and a tmux-style tab/pane system.

> **Platform:** macOS (primary). Linux planned for Phase 2+.

---

## Features

- **GPU rendering** via wgpu (Metal on macOS) — 60/120 fps, sub-8 ms input-to-pixel latency
- **Full terminal emulation** — xterm-256color, truecolor, bracketed paste, SGR mouse, OSC 52 clipboard
- **Font ligatures** — HarfBuzz shaping with `calt`, `liga`, `dlig` OpenType features; per-word shape cache
- **Emoji & color glyphs** — full RGBA emoji rendering via Apple Color Emoji (and any color font)
- **Tabs & split panes** — tmux-style keybinds, binary-tree layout; each pane has an independent PTY; exiting a shell closes only that pane
- **Status bar** — Powerline-style bottom bar with leader mode, CWD, git branch, exit code, and time
- **AI agent panel** — context-aware chat with file attachment, NL→command, explain output, fix errors, write files
- **LLM tool use** — AI agent can read files, list directories, write files, and run commands (sandboxed to CWD, with confirmation)
- **Inline AI block** — `Ctrl+Space` for quick NL→shell command without leaving the terminal
- **Multiple LLM providers** — OpenRouter, Ollama, LM Studio, GitHub Copilot; per-pane independent chat history
- **Right-click context menu** — Copy, Paste, Clear, and **Ask AI** (sends selection directly to chat panel)
- **Command palette** — fuzzy-search for all actions (`Leader+o`)
- **Snippets** — Tab-expandable text templates, configurable in Lua
- **Lua configuration** — hot-reload on save, no restart required
- **Scrollback** — configurable depth with GPU scroll bar
- **Debug HUD** — `F12` overlay: frame time p50/p95, input latency p50/p95/p99, shape cache hit rate, atlas fill, GPU upload KB/frame

---

## Installation

### Build from source

```bash
cargo build --release
```

### macOS app bundle

```bash
./scripts/bundle.sh
```

This creates `PetruTerm.app` in the project root, ready to drag to `/Applications`.

### Download a release

Grab the latest zip from the [Releases](https://github.com/petrubear/PetruTerm/releases) page, unzip, and move `PetruTerm.app` to `/Applications`.

Because the binary is ad-hoc signed (no Apple Developer certificate), macOS Gatekeeper will block it on first launch. Run this once after copying the app:

```bash
xattr -d com.apple.quarantine /Applications/PetruTerm.app
```

---

## Configuration

PetruTerm looks for its configuration in:

```
~/.config/petruterm/config.lua
```

If this file does not exist, the compiled-in defaults are used. You can create the directory and copy the defaults to start customizing:

```bash
mkdir -p ~/.config/petruterm
```

The config is organized into four modules. Each can be overridden independently.

---

### `config.lua` — Entry point

Composes the four modules. You can `require` and override any of them.

```lua
local ui       = require("ui")
local perf     = require("perf")
local keybinds = require("keybinds")
local llm      = require("llm")

local config = {}

ui.apply_to_config(config)
perf.apply_to_config(config)
keybinds.apply_to_config(config)
llm.apply_to_config(config)

return config
```

---

### `ui.lua` — Appearance

#### Font

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `config.font` | string | `"JetBrainsMono Nerd Font Mono"` | Font family name. Use `petruterm.font("A, B, C")` to resolve the first installed family from a comma-separated list. |
| `config.font_size` | number | `16` | Font size in points. |
| `config.font_features` | string[] | `{"calt=1","liga=1","dlig=1"}` | HarfBuzz OpenType feature tags. |

```lua
config.font         = petruterm.font("Monolisa Nerd Font, JetBrainsMono Nerd Font Mono")
config.font_size    = 14
config.font_features = { "calt=1", "liga=1", "dlig=0" }
```

#### Colors

`config.colors` accepts a table with the following hex string keys:

| Key | Default | Description |
|-----|---------|-------------|
| `foreground` | `"#f8f8f2"` | Default text color |
| `background` | `"#22212c"` | Terminal background |
| `cursor_bg` | `"#9580ff"` | Cursor fill color |
| `cursor_fg` | `"#f8f8f2"` | Text under cursor |
| `cursor_border` | `"#9580ff"` | Cursor outline |
| `selection_bg` | `"#454158"` | Selection background |
| `selection_fg` | `"#c6c6c2"` | Selected text color |
| `ansi` | Dracula Pro | Array of 8 normal ANSI colors (indices 0–7) |
| `brights` | Dracula Pro | Array of 8 bright ANSI colors (indices 8–15) |

```lua
config.colors = {
    foreground   = "#cdd6f4",
    background   = "#1e1e2e",
    cursor_bg    = "#f5e0dc",
    cursor_fg    = "#1e1e2e",
    cursor_border = "#f5e0dc",
    selection_bg = "#585b70",
    selection_fg = "#cdd6f4",
    ansi    = { "#45475a", "#f38ba8", "#a6e3a1", "#f9e2af",
                "#89b4fa", "#f5c2e7", "#94e2d5", "#bac2de" },
    brights = { "#585b70", "#f38ba8", "#a6e3a1", "#f9e2af",
                "#89b4fa", "#f5c2e7", "#94e2d5", "#a6adc8" },
}
```

#### Window

`config.window` accepts:

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `title_bar_style` | string | `"custom"` | `"custom"` — transparent title bar, draggable content area (macOS). `"native"` — standard OS title bar. `"none"` — fully borderless. |
| `padding` | table | `{left=20, right=20, top=60, bottom=10}` | Inner padding in physical pixels. `top` should be ≥ 60 with `"custom"` to clear traffic lights. |
| `start_maximized` | bool | `true` | Launch maximized. |
| `initial_width` | number\|nil | `nil` | Initial window width in pixels (overrides `start_maximized`). |
| `initial_height` | number\|nil | `nil` | Initial window height in pixels. |
| `opacity` | number | `1.0` | Window opacity (0.0–1.0). |
| `borderless` | bool | `false` | Remove all window chrome. |

```lua
config.window = {
    title_bar_style = "custom",
    padding = { left = 12, right = 12, top = 60, bottom = 8 },
    start_maximized = false,
    initial_width   = 1400,
    initial_height  = 900,
    opacity = 0.96,
}
```

#### Tab bar

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `config.enable_tab_bar` | bool | `true` | Show tab bar when more than one tab is open. |
| `config.hide_tab_bar_if_one` | bool | `true` | Hide tab bar when only one tab exists. |

#### Status bar

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `config.status_bar.enabled` | bool | `true` | Show the status bar. Also togglable via command palette. |
| `config.status_bar.position` | string | `"bottom"` | `"bottom"` or `"top"`. |

The status bar shows (left to right): **leader mode indicator** (turns purple when active), **current directory**, **git branch** (with `*` if dirty), and on the right: **last exit code** (only when non-zero, in red) and **date/time**.

```lua
config.status_bar = {
    enabled  = true,
    position = "bottom",
}
```

---

### `perf.lua` — Performance

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `config.scrollback_lines` | number | `10000` | Maximum scrollback buffer depth per pane. |
| `config.enable_scroll_bar` | bool | `true` | Show the 6 px scroll bar on the right edge when scrollback is active. |
| `config.max_fps` | number | `60` | Target render frame rate. |
| `config.shell_integration` | bool | `true` | Enable shell integration hooks (writes CWD/exit-code context for the AI panel). See [Shell Integration](#shell-integration). |

```lua
config.scrollback_lines  = 50000
config.enable_scroll_bar = true
config.max_fps           = 120
```

---

### `keybinds.lua` — Key bindings

#### Leader key

```lua
config.leader = { key = "f", mods = "CTRL", timeout_ms = 1000 }
```

Press `Ctrl+F`, release, then press the bound key within `timeout_ms` milliseconds.

#### Hardcoded system bindings (not configurable)

| Key | Action |
|-----|--------|
| `Cmd+C` | Copy selection to clipboard |
| `Cmd+V` | Paste from clipboard |
| `Cmd+Q` | Quit |
| `Cmd+1–9` | Switch to tab N |
| `Ctrl+Space` | Toggle inline AI block |

#### Default leader bindings

| Binding | Action |
|---------|--------|
| `Leader+o` | Open command palette |
| `Leader+a` | Open / close AI panel |
| `Leader+A` | Move focus between terminal and AI panel (without closing) |
| `Leader+e` | Explain last terminal output |
| `Leader+f` | Fix last error |
| `Leader+z` | Undo last AI file write |
| `Leader+c` | New tab |
| `Leader+&` | Close tab |
| `Leader+n` | Next tab |
| `Leader+b` | Previous tab |
| `Leader+%` | Split pane horizontally (left \| right) |
| `Leader+"` | Split pane vertically (top / bottom) |
| `Leader+x` | Close active pane |
| `Leader+h/j/k/l` | Focus pane left / down / up / right (vim-style) |

#### Custom bindings

```lua
config.keys = {
    { mods = "LEADER", key = "a",  action = petruterm.action.ToggleAiPanel },
    { mods = "LEADER", key = "A",  action = petruterm.action.FocusAiPanel },
    { mods = "LEADER", key = "c",  action = petruterm.action.NewTab },
    { mods = "LEADER", key = "n",  action = petruterm.action.NextTab },
    { mods = "LEADER", key = "%",  action = petruterm.action.SplitHorizontal },
    { mods = "LEADER", key = '"',  action = petruterm.action.SplitVertical },
    { mods = "LEADER", key = "x",  action = petruterm.action.ClosePane },
}
```

#### Available actions

| Action | Description |
|--------|-------------|
| `petruterm.action.CommandPalette` | Open command palette |
| `petruterm.action.ToggleAiPanel` | Open / close AI agent panel |
| `petruterm.action.FocusAiPanel` | Move focus between terminal and AI panel |
| `petruterm.action.ExplainLastOutput` | Send last terminal output to AI for explanation |
| `petruterm.action.FixLastError` | Send last failed command to AI for a fix |
| `petruterm.action.UndoLastWrite` | Undo last AI-proposed file write |
| `petruterm.action.ToggleStatusBar` | Show / hide the status bar |
| `petruterm.action.NewTab` | Open a new tab |
| `petruterm.action.CloseTab` | Close the current tab |
| `petruterm.action.NextTab` | Switch to the next tab |
| `petruterm.action.PrevTab` | Switch to the previous tab |
| `petruterm.action.SplitHorizontal` | Split active pane horizontally |
| `petruterm.action.SplitVertical` | Split active pane vertically |
| `petruterm.action.ClosePane` | Close the active pane |
| `petruterm.action.FocusPaneLeft` | Focus pane to the left |
| `petruterm.action.FocusPaneRight` | Focus pane to the right |
| `petruterm.action.FocusPaneUp` | Focus pane above |
| `petruterm.action.FocusPaneDown` | Focus pane below |
| `petruterm.action.ToggleFullscreen` | Toggle fullscreen mode |
| `petruterm.action.ReloadConfig` | Hot-reload configuration |
| `petruterm.action.OpenConfigFile` | Open config file in default editor |
| `petruterm.action.Quit` | Quit PetruTerm |

---

### `llm.lua` — AI features

```lua
config.llm = {
    enabled  = true,

    provider = "openrouter",
    model    = "anthropic/claude-3.5-haiku",
    api_key  = os.getenv("OPENROUTER_API_KEY"),
    base_url = nil,   -- nil = provider default

    features = {
        nl_to_command  = true,
        explain_output = true,
        fix_last_error = true,
        context_chat   = true,
    },

    context_lines = 50,
}
```

#### Top-level options

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `enabled` | bool | `false` | Master switch. Set to `true` to enable all AI features. |
| `provider` | string | `"openrouter"` | LLM provider: `"openrouter"`, `"ollama"`, `"lmstudio"`, or `"copilot"`. |
| `model` | string | `"meta-llama/llama-3.1-8b-instruct:free"` | Model identifier. Format depends on the provider. |
| `api_key` | string\|nil | `nil` | API key. Use `os.getenv("VAR")` to avoid hardcoding secrets. See provider defaults below. |
| `base_url` | string\|nil | `nil` | Override the provider's base URL. `nil` uses the default. |
| `context_lines` | number | `50` | Lines of terminal output included as context in AI requests. |

#### Provider defaults

| Provider | Default `base_url` | Auth |
|----------|-------------------|------|
| `openrouter` | `https://openrouter.ai/api/v1` | API key required |
| `ollama` | `http://localhost:11434/v1` | None |
| `lmstudio` | `http://localhost:1234/v1` | None |
| `copilot` | `https://api.githubcopilot.com` | GitHub token (see below) |

#### `features` table

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `nl_to_command` | bool | `true` | Natural language → shell command via inline AI block (`Ctrl+Space`). |
| `explain_output` | bool | `true` | Explain last terminal output (`Leader+e`). |
| `fix_last_error` | bool | `true` | Suggest a fix for the last failed command (`Leader+f`). |
| `context_chat` | bool | `true` | Multi-turn chat panel with CWD, exit code, and last command context. |

#### Local provider examples

```lua
-- Ollama (no API key needed)
provider = "ollama"
model    = "llama3.2"

-- LM Studio (no API key needed)
provider = "lmstudio"
model    = "lmstudio-community/Meta-Llama-3-8B-Instruct-GGUF"

-- GitHub Copilot (requires active Copilot subscription)
provider = "copilot"
model    = "gpt-4o"   -- also: gpt-4o-mini, claude-3.5-sonnet, o1-mini
```

---

### Storing API keys securely (macOS Keychain)

Avoid putting secrets in environment variables or config files. PetruTerm reads keys directly from the macOS Keychain.

#### OpenRouter

1. Get your API key from [openrouter.ai/keys](https://openrouter.ai/keys).
2. Store it:

```bash
security add-generic-password \
  -s PetruTerm \
  -a OPENROUTER_API_KEY \
  -w "<your-openrouter-key>"
```

3. In `llm.lua`, omit `api_key` (or set it to `nil`). PetruTerm finds it automatically.

#### GitHub Copilot

The `copilot` provider uses **device-flow OAuth** — no token needs to be created or copied manually. You need an active GitHub Copilot subscription.

1. In `llm.lua`, set `provider = "copilot"` and omit `api_key`.
2. Open the AI panel (`Leader+a`). On first use, PetruTerm starts the authorization flow automatically:
   - A browser window opens at `github.com/login/device`.
   - The activation code is shown in the chat panel.
   - Enter the code in the browser and click **Authorize**.
3. PetruTerm saves the OAuth token to your Keychain automatically. No further action needed on subsequent launches.

To revoke and re-authorize (e.g. after switching GitHub accounts):

```bash
security delete-generic-password -s PetruTerm -a GITHUB_COPILOT_OAUTH_TOKEN
```

Then reopen the AI panel — the device flow runs again.

To inspect the stored token:

```bash
security find-generic-password -s PetruTerm -a GITHUB_COPILOT_OAUTH_TOKEN -w
```

---

## AI Agent Panel

Open with `Leader+a`. Press again to close. Use `Leader+A` to move focus between the terminal and the panel without closing it.

### File context

When the panel opens it automatically attaches `AGENTS.md` from the current terminal's working directory as project context. Press `Tab` to open the file picker and attach additional files:

| Key | Action |
|-----|--------|
| `Tab` | Open / close file picker |
| `↑` / `↓` | Navigate file list |
| `Enter` | Attach / detach selected file |
| `Esc` | Close file picker |

Attached files are injected into the LLM system message before every query. The footer shows an estimated token count.

### Chat input

| Key | Action |
|-----|--------|
| `Enter` | Submit query |
| `Shift+Enter` | Insert newline in input |
| `Ctrl+S` | Submit query (alternative) |
| `Esc` | Close panel / dismiss error |
| `/q` or `/quit` | Close panel |

### Ask AI from context menu

Right-click any selected text and choose **Ask AI** to send it directly to the chat panel as input. The panel opens automatically if it was closed.

### LLM tool use

When the LLM needs additional context it can autonomously call built-in tools (up to 10 rounds per query). Filesystem tools are sandboxed to the terminal's current working directory.

| Tool | Confirmation | Description |
|------|-------------|-------------|
| `ReadFile` | No | Read the contents of a file |
| `ListDir` | No | List files in a directory |
| `WriteFile` | **Yes** | Overwrite a file with a diff preview before writing |
| `RunCommand` | **Yes** | Execute a shell command in the active PTY |

Write and run tools show a `[y] Apply  [n] Reject` prompt. Use `Leader+z` to undo the last file write.

While a tool is running the panel shows `⟳ tool(path)`; after completion it shows `✓ tool(path)`.

### Inline AI block (`Ctrl+Space`)

A 4-row overlay at the bottom of the terminal for quick NL→command generation:

| Key | Action |
|-----|--------|
| `Enter` (typing) | Submit query |
| `Enter` (result ready) | Execute suggested command in PTY |
| `Esc` | Close |

---

## Shell Integration

For richer AI context (last command, exit code, CWD written via shell hooks), source the integration script in your `~/.zshrc`:

```zsh
source ~/.config/petruterm/shell-integration.zsh
```

> **Note:** Shell integration is optional. PetruTerm reads the terminal process's real CWD directly via OS APIs (`proc_pidinfo` on macOS) and does not require the integration script for the file picker or `AGENTS.md` auto-attach to work.

The script writes `~/.cache/petruterm/shell-context.json` after each command. This JSON is read by the AI panel to include CWD, last command, and exit code in every query.

---

## AGENTS.md

Place an `AGENTS.md` file in your project root to give the AI panel automatic context about your project. It is attached as the first file every time the panel opens in that directory.

---

## Tech Stack

| Component | Crate |
|-----------|-------|
| GPU rendering | `wgpu` 29 (Metal on macOS) |
| Windowing | `winit` 0.30 |
| Terminal emulation | `alacritty_terminal` 0.25 |
| Font shaping | `cosmic-text` 0.18 + HarfBuzz + FreeType LCD |
| Config DSL | `mlua` 0.11 (Lua 5.4) |
| Async / LLM | `tokio` + `reqwest` |
| Fuzzy search | `skim` + `fuzzy-matcher` |
| Hashing | `rustc-hash` (FxHasher) |

---

## Project Layout

```
~/.config/petruterm/
├── config.lua            # Entry point — require and compose modules
├── ui.lua                # Font, colors, window, status bar
├── perf.lua              # Scrollback, FPS, GPU
├── keybinds.lua          # Leader key and all bindings
├── llm.lua               # AI provider and features
├── plugins/              # Auto-scanned Lua plugins
└── shell-integration.zsh # Optional: source in ~/.zshrc
```

### Performance notes

- Row cache: unchanged terminal rows are served from a per-pane shape cache — HarfBuzz runs only on dirty rows.
- Damage tracking: alacritty's `TermDamage` API skips grid reads for undamaged rows when no selection or search is active.
- Cursor overlay: cursor blink updates a single GPU vertex without rebuilding the cell buffer.
- Idle: event loop parks when the window loses focus — no timer wakeups, no git polling, no redraws.

---

## License

MIT
