# PetruTerm

[![CI](https://github.com/petrubear/PetruTerm/actions/workflows/ci.yml/badge.svg)](https://github.com/petrubear/PetruTerm/actions/workflows/ci.yml)

A developer-first GPU-accelerated terminal emulator written in Rust. Built for speed and extensibility, with first-class AI integration, a Lua configuration DSL, font ligatures, and a tmux-style tab/pane system.

> **Platform:** macOS (primary). Linux planned for Phase 2+.

---

## Features

- **GPU rendering** via wgpu (Metal on macOS) — 60/120 fps, sub-8 ms input-to-pixel latency
- **Full terminal emulation** — xterm-256color, truecolor, bracketed paste, SGR mouse, OSC 52 clipboard
- **Font ligatures** — HarfBuzz shaping with `calt`, `liga`, `dlig` OpenType features; per-word shape cache
- **Bold/italic rendering** — SGR bold/italic mapped automatically to the installed font family's own bold/italic/bold-italic faces via fontdb; no separate font path to configure
- **Emoji & color glyphs** — full RGBA emoji rendering via Apple Color Emoji (and any color font)
- **Floating UI + macOS blur** — translucent vibrancy behind panels and the sidebar (`config.window.blur`)
- **Tabs & split panes** — tmux-style keybinds, binary-tree layout; each pane has an independent PTY; exiting a shell closes only that pane
- **Workspaces** — named workspaces with independent tab sets; save and restore layouts with `Leader+W+s` / `Leader+W+L`; auto-save on exit and/or on switch, both configurable
- **Sidebar** — collapsible VSCode-style drawer (`Leader+e+e`) with four `Tab`-cycled sections: Workspaces, MCP, Skills, Steering
- **Status bar** — configurable plain or powerline bottom bar with leader mode, CWD, git branch, exit code, and time
- **Input decoration** — syntax highlighting for shell commands (valid command = green, flags = cyan, strings = yellow), ghost text from shell history, inline flag hints on the row below the cursor
- **Kitty keyboard protocol** — `Shift+Enter`, `Ctrl+Enter`, and other disambiguated key sequences work correctly in Neovim, Claude Code CLI, and other KKP-aware apps
- **AI agent panel** — context-aware chat with file attachment, NL→command, explain output, fix errors, write files; agent can propose and run commands with confirmation
- **ACP agent backend** — point the AI panel at an external Agent Client Protocol process (e.g. Claude Code) instead of a direct LLM API; switch on the fly with `/agent`
- **LLM tool use** — AI agent can read files, list directories, write files, and run commands (sandboxed to CWD, with confirmation)
- **Skills** — reusable `SKILL.md` prompts loaded from global and per-project directories, browsable from the sidebar or via `/skills`
- **Steering** — always-on custom instructions injected into every AI request, global and per-project (project wins on name clash)
- **Inline AI block** — `Ctrl+Space` for quick NL→shell command without leaving the terminal
- **Multiple LLM providers** — OpenRouter, Ollama, LM Studio, GitHub Copilot; per-pane independent chat history; switch model or agent at runtime with `/model` / `/agent`
- **MCP (Model Context Protocol)** — connect the AI agent to external tools (databases, APIs, filesystems) via JSON-configured MCP servers; use `/mcp` in the panel to list active servers and tools
- **Contextual right-click menus** — selection (Copy, Paste, Clear, **Ask AI**), hovered links (Open Link, Copy Link), command output blocks (Copy Output, Re-run Command), failed-command exit-code info, and a per-tab color picker
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

On first launch, PetruTerm creates `~/.config/petruterm/` and seeds:

- `config.lua`, `ui.lua`, `perf.lua`, `keybinds.lua`, `llm.lua`
- `snippets.lua`, `notifications.lua`
- `system/system_prompt.md`
- bundled themes in `themes/`
- `shell-integration.zsh`

Existing user files are preserved; only managed assets such as `keybinds.lua` and `shell-integration.zsh` may be updated when their bundled version changes.

The config is organized into six Lua modules plus system assets.

---

### `config.lua` — Entry point

Composes the six Lua modules. You can `require` and override any of them.

```lua
local ui            = require("ui")
local perf          = require("perf")
local keybinds      = require("keybinds")
local llm           = require("llm")
local snippets      = require("snippets")
local notifications = require("notifications")

local config = {}

ui.apply_to_config(config)
perf.apply_to_config(config)
keybinds.apply_to_config(config)
llm.apply_to_config(config)
snippets.apply_to_config(config)
notifications.apply_to_config(config)

return config
```

---

### `ui.lua` — Appearance

#### Font

| Key                       | Type     | Default                                                                | Description                                                                                                          |
| ------------------------- | -------- | ---------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------- |
| `config.font`             | string   | `"JetBrainsMono Nerd Font Mono, Monolisa Nerd Font, Fira Code, Menlo"` | Font family name. Use `petruterm.font("A, B, C")` to resolve the first installed family from a comma-separated list. |
| `config.font_size`        | number   | `16`                                                                   | Font size in points.                                                                                                 |
| `config.font_line_height` | number   | `1.2`                                                                  | Line-height multiplier.                                                                                              |
| `config.font_features`    | string[] | `{"calt=1","liga=1","dlig=1"}`                                         | HarfBuzz OpenType feature tags.                                                                                      |
| `config.font_fallbacks`   | string[] | `{"Apple Color Emoji","Noto Color Emoji"}`                             | Fallback fonts for missing glyphs and emoji.                                                                         |
| `config.lcd_antialiasing` | bool     | `false`                                                                | Enable LCD subpixel antialiasing where supported.                                                                    |

```lua
config.font         = petruterm.font("Monolisa Nerd Font, JetBrainsMono Nerd Font Mono")
config.font_size    = 14
config.font_features = { "calt=1", "liga=1", "dlig=0" }
```

Bold and italic text (SGR bold/italic from the shell) render using the same family's own bold/italic/bold-italic font files, resolved automatically via fontdb — there is no separate `bold_font`/`italic_font` key to set. If the installed family has no dedicated bold or italic face, PetruTerm falls back to its regular face.

#### Colors

`config.colors` accepts a table with the following hex string keys:

| Key                 | Default     | Description                                      |
| ------------------- | ----------- | ------------------------------------------------ |
| `foreground`        | `"#e0e0e8"` | Default text color                               |
| `background`        | `"#0e0e10"` | Terminal background                              |
| `cursor_bg`         | `"#9580ff"` | Cursor fill color                                |
| `cursor_fg`         | `"#e0e0e8"` | Text under cursor                                |
| `cursor_border`     | `"#9580ff"` | Cursor outline                                   |
| `selection_bg`      | `"#2a2a3a"` | Selection background                             |
| `selection_fg`      | `"#e0e0e8"` | Selected text color                              |
| `ansi`              | Dracula Pro | Array of 8 normal ANSI colors (indices 0–7)      |
| `brights`           | Dracula Pro | Array of 8 bright ANSI colors (indices 8–15)     |
| `ui_accent`         | derived     | Optional semantic accent color for UI highlights |
| `ui_surface`        | derived     | Optional semantic panel / sidebar background     |
| `ui_surface_active` | derived     | Optional semantic selected-item background       |
| `ui_surface_hover`  | derived     | Optional semantic hover background               |
| `ui_muted`          | derived     | Optional semantic muted text / separator color   |
| `ui_success`        | derived     | Optional semantic success color                  |
| `ui_overlay`        | derived     | Optional semantic overlay background             |

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

| Key               | Type        | Default                                  | Description                                                                                                                          |
| ----------------- | ----------- | ---------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------ |
| `title_bar_style` | string      | `"custom"`                               | `"custom"` — transparent title bar, draggable content area (macOS). `"native"` — standard OS title bar. `"none"` — fully borderless. |
| `padding`         | table       | `{left=20, right=20, top=60, bottom=10}` | Inner padding in physical pixels. `top` should be ≥ 60 with `"custom"` to clear traffic lights.                                      |
| `start_maximized` | bool        | `true`                                   | Launch maximized.                                                                                                                    |
| `initial_width`   | number\|nil | `nil`                                    | Initial window width in pixels (overrides `start_maximized`).                                                                        |
| `initial_height`  | number\|nil | `nil`                                    | Initial window height in pixels.                                                                                                     |
| `opacity`         | number      | `1.0`                                    | Window opacity (0.0–1.0).                                                                                                            |
| `borderless`      | bool        | `false`                                  | Remove all window chrome.                                                                                                            |
| `blur`            | string\|bool | `false`                                  | macOS vibrancy behind the window: `"dark"`, `"light"`, or `false`/omitted to disable. Softens `ui_surface*` panel colors automatically. |

```lua
config.window = {
    title_bar_style = "custom",
    padding = { left = 12, right = 12, top = 60, bottom = 8 },
    start_maximized = false,
    initial_width   = 1400,
    initial_height  = 900,
    opacity = 0.96,
    blur    = "dark",
}
```

When `blur` is set (or `opacity < 1.0`), panel and sidebar backgrounds (`ui_surface`, `ui_surface_hover`) automatically render with reduced alpha so the vibrancy/translucency shows through.

#### Tab bar

| Key                          | Type | Default | Description                                  |
| ---------------------------- | ---- | ------- | -------------------------------------------- |
| `config.enable_tab_bar`      | bool | `true`  | Show tab bar when more than one tab is open. |
| `config.hide_tab_bar_if_one` | bool | `true`  | Hide tab bar when only one tab exists.       |

#### Status bar

| Key                          | Type   | Default    | Description                                                  |
| ---------------------------- | ------ | ---------- | ------------------------------------------------------------ |
| `config.status_bar.enabled`  | bool   | `true`     | Show the status bar. Also togglable via command palette.     |
| `config.status_bar.position` | string | `"bottom"` | `"bottom"` or `"top"`.                                       |
| `config.status_bar.style`    | string | `"plain"`  | `"plain"` text separators or `"powerline"` Nerd Font arrows. |

The status bar shows (left to right): **leader mode indicator** (turns purple when active), **current directory**, **git branch** (with `*` if dirty), and on the right: **last exit code** (only when non-zero, in red) and **date/time**.

```lua
config.status_bar = {
    enabled  = true,
    position = "bottom",
}
```

---

### `perf.lua` — Performance

| Key                                 | Type   | Default       | Description                                                                                                                  |
| ----------------------------------- | ------ | ------------- | ---------------------------------------------------------------------------------------------------------------------------- |
| `config.scrollback_lines`           | number | `5000`        | Maximum scrollback buffer depth per pane.                                                                                    |
| `config.enable_scroll_bar`          | bool   | `true`        | Show the 6 px scroll bar on the right edge when scrollback is active.                                                        |
| `config.max_fps`                    | number | `60`          | Target render frame rate.                                                                                                    |
| `config.gpu_preference`             | string | `"low_power"` | GPU selection preference: `"high_performance"`, `"low_power"`, or `"none"`.                                                  |
| `config.status_bar.git_dirty_check` | bool   | `false`       | Poll `git status --porcelain` for a dirty marker in the status bar.                                                          |
| `config.battery_saver`              | string | `"auto"`      | Battery saver policy: `"auto"`, `"always"`, or `"never"`.                                                                    |
| `config.shell_integration`          | bool   | `true`        | Enable shell integration hooks (writes CWD/exit-code context for the AI panel). See [Shell Integration](#shell-integration). |

```lua
config.scrollback_lines  = 50000
config.enable_scroll_bar = true
config.max_fps           = 120
config.gpu_preference    = "high_performance"
config.battery_saver     = "never"
config.shell_integration = true
```

---

### Workspaces

| Key                                 | Type | Default | Description                                              |
| ------------------------------------ | ---- | ------- | ---------------------------------------------------------- |
| `config.workspaces.auto_save_on_exit`   | bool | `true`  | Save the layout of every workspace automatically on quit. |
| `config.workspaces.auto_save_on_switch` | bool | `false` | Save a workspace's layout automatically when you switch away from it (`Leader+W+j/k`), in addition to on exit. |

```lua
config.workspaces = {
    auto_save_on_exit   = true,
    auto_save_on_switch = true,
}
```

See [Sidebar](#sidebar) for browsing and restoring saved workspaces.

---

### `keybinds.lua` — Key bindings

#### Leader key

```lua
config.leader = { key = "f", mods = "CTRL", timeout_ms = 1000 }
```

Press `Ctrl+F`, release, then press the bound key within `timeout_ms` milliseconds.

#### Keyboard

| Key                          | Type | Default | Description                                                          |
| ----------------------------- | ---- | ------- | ---------------------------------------------------------------------- |
| `config.keyboard.option_as_meta` | bool | `false` | Send `Option+key` as a Meta-prefixed escape sequence (Emacs/readline-style) instead of the macOS accented-character input. |

#### Hardcoded system bindings (not configurable)

| Key          | Action                      |
| ------------ | --------------------------- |
| `Cmd+C`      | Copy selection to clipboard |
| `Cmd+V`      | Paste from clipboard        |
| `Cmd+Q`      | Quit                        |
| `Cmd+K`      | Clear screen and scrollback |
| `Cmd+F`      | Toggle text search          |
| `Cmd+1–9`    | Switch to tab N             |
| `Ctrl+Space` | Toggle inline AI block      |
| `F12`        | Toggle debug HUD            |

#### Default leader bindings

| Binding               | Action                                                     |
| --------------------- | ---------------------------------------------------------- |
| `Leader+o`            | Open command palette                                       |
| `Leader+a+a`          | Open / close AI panel                                      |
| `Leader+A`            | Move focus between terminal and AI panel (without closing) |
| `Leader+a+e`          | Explain last terminal output                               |
| `Leader+a+f`          | Fix last error                                             |
| `Leader+a+z`          | Undo last AI file write                                    |
| `Leader+e+e`          | Toggle sidebar (Workspaces / MCP / Skills / Steering — see [Sidebar](#sidebar)) |
| `Leader+w`            | New workspace                                              |
| `Leader+W+n`          | New workspace                                              |
| `Leader+W+&`          | Close workspace                                            |
| `Leader+W+,`          | Rename workspace                                           |
| `Leader+W+j`          | Next workspace                                             |
| `Leader+W+k`          | Previous workspace                                         |
| `Leader+W+s`          | Save current workspace layout                              |
| `Leader+W+L`          | Open saved workspaces palette                              |
| `Leader+c`            | New tab                                                    |
| `Leader+&`            | Close tab                                                  |
| `Leader+n`            | Next tab                                                   |
| `Leader+b`            | Previous tab                                               |
| `Leader+%`            | Split pane horizontally (left \| right)                    |
| `Leader+"`            | Split pane vertically (top / bottom)                       |
| `Leader+x`            | Close active pane                                          |
| `Leader+h/j/k/l`      | Focus pane left / down / up / right (vim-style)            |
| `Leader+Option+Arrow` | Resize active pane                                         |

#### Custom bindings

```lua
config.keys = {
    { mods = "LEADER", key = "A",  action = petruterm.action.FocusAiPanel },
    { mods = "LEADER", key = "o",  action = petruterm.action.CommandPalette },
    { mods = "LEADER", key = "c",  action = petruterm.action.NewTab },
    { mods = "LEADER", key = "n",  action = petruterm.action.NextTab },
    { mods = "LEADER", key = "%",  action = petruterm.action.SplitHorizontal },
    { mods = "LEADER", key = '"',  action = petruterm.action.SplitVertical },
    { mods = "LEADER", key = "x",  action = petruterm.action.ClosePane },
}
```

`Leader+a+*`, `Leader+e+e`, `Leader+w`, `Leader+W+*`, `Ctrl+Space`, `Cmd+F`, `Cmd+K`, `Cmd+1-9`, and `F12` are handled by the built-in input layer rather than `config.keys`. The single-key leader prefixes `a`, `e`, and `W` are reserved for those built-in sequences.

#### Available actions

| Action                               | Description                                     |
| ------------------------------------ | ----------------------------------------------- |
| `petruterm.action.CommandPalette`    | Open command palette                            |
| `petruterm.action.ToggleAiPanel`     | Open / close AI agent panel                     |
| `petruterm.action.FocusAiPanel`      | Move focus between terminal and AI panel        |
| `petruterm.action.ExplainLastOutput` | Send last terminal output to AI for explanation |
| `petruterm.action.FixLastError`      | Send last failed command to AI for a fix        |
| `petruterm.action.UndoLastWrite`     | Undo last AI-proposed file write                |
| `petruterm.action.ToggleStatusBar`   | Show / hide the status bar                      |
| `petruterm.action.NewTab`            | Open a new tab                                  |
| `petruterm.action.CloseTab`          | Close the current tab                           |
| `petruterm.action.NextTab`           | Switch to the next tab                          |
| `petruterm.action.PrevTab`           | Switch to the previous tab                      |
| `petruterm.action.SplitHorizontal`   | Split active pane horizontally                  |
| `petruterm.action.SplitVertical`     | Split active pane vertically                    |
| `petruterm.action.ClosePane`         | Close the active pane                           |
| `petruterm.action.FocusPaneLeft`     | Focus pane to the left                          |
| `petruterm.action.FocusPaneRight`    | Focus pane to the right                         |
| `petruterm.action.FocusPaneUp`       | Focus pane above                                |
| `petruterm.action.FocusPaneDown`     | Focus pane below                                |
| `petruterm.action.ToggleFullscreen`  | Toggle fullscreen mode                          |
| `petruterm.action.ReloadConfig`      | Hot-reload configuration                        |
| `petruterm.action.OpenConfigFile`    | Open config file in default editor              |
| `petruterm.action.Quit`              | Quit PetruTerm                                  |

---

### `llm.lua` — AI features

```lua
config.llm = {
    enabled  = false,

    provider = "openrouter",
    model    = "meta-llama/llama-3.1-8b-instruct:free",
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

| Key             | Type        | Default                                   | Description                                                                               |
| --------------- | ----------- | ----------------------------------------- | ----------------------------------------------------------------------------------------- |
| `enabled`       | bool        | `false`                                   | Master switch. Set to `true` to enable all AI features.                                   |
| `backend`       | string      | `"provider"`                              | `"provider"` — talk to `provider`/`model` directly. `"agent"` — drive the panel via an external ACP process (see [Agent backend (ACP)](#agent-backend-acp) below). |
| `provider`      | string      | `"openrouter"`                            | LLM provider: `"openrouter"`, `"ollama"`, `"lmstudio"`, or `"copilot"`.                   |
| `model`         | string      | `"meta-llama/llama-3.1-8b-instruct:free"` | Model identifier. Format depends on the provider.                                         |
| `api_key`       | string\|nil | `nil`                                     | API key. Use `os.getenv("VAR")` to avoid hardcoding secrets. See provider defaults below. |
| `base_url`      | string\|nil | `nil`                                     | Override the provider's base URL. `nil` uses the default.                                 |
| `context_lines` | number      | `50`                                      | Lines of terminal output included as context in AI requests.                              |

#### Provider defaults

| Provider     | Default `base_url`              | Auth                     |
| ------------ | ------------------------------- | ------------------------ |
| `openrouter` | `https://openrouter.ai/api/v1`  | API key required         |
| `ollama`     | `http://localhost:11434/v1`     | None                     |
| `lmstudio`   | `http://localhost:1234/v1`      | None                     |
| `copilot`    | `https://api.githubcopilot.com` | GitHub token (see below) |

#### `features` table

| Key              | Type | Default | Description                                                          |
| ---------------- | ---- | ------- | -------------------------------------------------------------------- |
| `nl_to_command`  | bool | `true`  | Natural language → shell command via inline AI block (`Ctrl+Space`). |
| `explain_output` | bool | `true`  | Explain last terminal output (`Leader+a+e`).                         |
| `fix_last_error` | bool | `true`  | Suggest a fix for the last failed command (`Leader+a+f`).            |
| `context_chat`   | bool | `true`  | Multi-turn chat panel with CWD, exit code, and last command context. |

#### `ui` table

| Key            | Type   | Default | Description                             |
| -------------- | ------ | ------- | ---------------------------------------- |
| `width_cols`   | number | `55`    | Chat panel width, in terminal columns.   |

```lua
config.llm.ui = { width_cols = 70 }
```

#### Agent backend (ACP)

Set `backend = "agent"` to have the AI panel talk to an external [Agent Client Protocol](https://agentclientprotocol.com) process instead of a direct LLM provider. `provider`/`model`/`api_key` are ignored in this mode.

| Key                 | Type        | Default | Description                                                             |
| -------------------- | ----------- | ------- | -------------------------------------------------------------------------- |
| `agent.command`      | string      | —       | Executable to launch as the ACP agent.                                    |
| `agent.args`         | string[]    | `{}`    | Arguments passed to `agent.command`.                                      |
| `agent.env`          | table       | `{}`    | Extra environment variables for the agent process, as `{KEY = "value"}`.  |
| `agent.display_name` | string\|nil | `nil`   | Override the name shown in the chat header (`◈ <name>`); defaults to the command basename. |

```lua
config.llm.backend = "agent"
config.llm.agent = {
    command = "npx",
    args    = { "-y", "@agentclientprotocol/claude-agent-acp" },
    env     = {},
}
```

The chat header shows `◈ <agent>` in agent mode vs `✦ <model>` in provider mode. Use `/agent` in the panel input to view or switch the agent command at runtime, and `/model` to view or switch model in provider mode — each is disabled in the other backend.

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
model    = "gpt-4o-mini"   -- also: gpt-4o, claude-3.5-sonnet, claude-3.7-sonnet, o3-mini, o1-mini
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
2. Open the AI panel (`Leader+a+a`). On first use, PetruTerm starts the authorization flow automatically:
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

### `notifications.lua` — Notifications

| Key                        | Type   | Default   | Description                                            |
| --------------------------- | ------ | --------- | ---------------------------------------------------------- |
| `config.notifications.style` | string | `"toast"` | `"toast"` — in-app toast notifications. `"native"` — macOS Notification Center. |

```lua
config.notifications = { style = "native" }
```

---

## AI Agent Panel

Open with `Leader+a+a`. Press again to close. Use `Leader+A` to move focus between the terminal and the panel without closing it.

### File context

When the panel opens it automatically attaches `AGENTS.md` from the current terminal's working directory as project context. Press `Tab` to open the file picker and attach additional files:

| Key       | Action                        |
| --------- | ----------------------------- |
| `Tab`     | Open / close file picker      |
| `↑` / `↓` | Navigate file list            |
| `Enter`   | Attach / detach selected file |
| `Esc`     | Close file picker             |

Attached files are injected into the LLM system message before every query. The footer shows an estimated token count.

### Chat input

| Key             | Action                      |
| --------------- | --------------------------- |
| `Enter`         | Submit query                |
| `Shift+Enter`   | Insert newline in input     |
| `Ctrl+S`        | Submit query (alternative)  |
| `Esc`           | Close panel / dismiss error |
| `/q` or `/quit` | Close panel                 |

### Slash commands

Typed as the entire chat input:

| Command             | Description                                                              |
| --------------------- | --------------------------------------------------------------------------- |
| `/model [name]`      | Show or switch the LLM model (provider backend only).                      |
| `/agent [command]`   | Show or switch the ACP agent command (agent backend only).                 |
| `/mcp`                | List connected MCP servers and their tools.                                |
| `/skills [filter]`   | List loaded skills, optionally filtered by name.                           |
| `/clear` or `/reset` | Clear the current chat history.                                            |
| `/q` or `/quit`      | Close the panel.                                                            |

### Ask AI from context menu

Right-click any selected text and choose **Ask AI** to send it directly to the chat panel as input. The panel opens automatically if it was closed.

### LLM tool use

When the LLM needs additional context it can autonomously call built-in tools (up to 10 rounds per query). Filesystem tools are sandboxed to the terminal's current working directory.

| Tool         | Confirmation | Description                                         |
| ------------ | ------------ | --------------------------------------------------- |
| `ReadFile`   | No           | Read the contents of a file                         |
| `ListDir`    | No           | List files in a directory                           |
| `WriteFile`  | **Yes**      | Overwrite a file with a diff preview before writing |
| `RunCommand` | **Yes**      | Execute a shell command in the active PTY           |

Write and run tools show a `[y] Apply  [n] Reject` prompt. Use `Leader+a+z` to undo the last file write.

While a tool is running the panel shows `⟳ tool(path)`; after completion it shows `✓ tool(path)`.

### Inline AI block (`Ctrl+Space`)

A 4-row overlay at the bottom of the terminal for quick NL→command generation:

| Key                    | Action                           |
| ---------------------- | -------------------------------- |
| `Enter` (typing)       | Submit query                     |
| `Enter` (result ready) | Execute suggested command in PTY |
| `Esc`                  | Close                            |

---

## Sidebar

A collapsible VSCode-style drawer, toggled with `Leader+e+e` (closed with `Escape` while it has focus). It has four sections, cycled with `Tab` / `Shift+Tab`:

| Section        | Contents                                                                 |
| --------------- | ------------------------------------------------------------------------- |
| **Workspaces** | Saved workspace layouts — browse, restore, rename, delete.               |
| **MCP**        | Connected MCP servers and their tools (same data as `/mcp`).             |
| **Skills**     | Loaded `SKILL.md` files — browse and inspect (see [Skills](#skills)).    |
| **Steering**   | Active steering files — browse and inspect (see [Steering](#steering)). |

### Skills

Reusable prompt snippets, one per directory, with a `SKILL.md` file describing when and how the AI should use them:

```
~/.config/petruterm/skills/<name>/SKILL.md   # global, available in every project
<project>/.petruterm/skills/<name>/SKILL.md  # project-local
```

New skill directories require explicit trust approval before their contents are loaded. List what's currently loaded with `/skills` in the chat panel, or browse them in the sidebar's Skills section.

### Steering

Always-on custom instructions injected into every AI request, without needing to attach a file manually:

```
~/.config/petruterm/steering/*.md   # global
<project>/.petruterm/steering/*.md  # project-local — wins on a filename clash with a global one
```

Like skills, a new steering directory requires trust approval before it's read. Browse active steering files in the sidebar's Steering section.

---

## Context Menu

Right-click surfaces a different menu depending on what's under the cursor:

| Context                        | Actions                                                          |
| -------------------------------- | ---------------------------------------------------------------- |
| Selected text                   | Copy, Paste, Clear, **Ask AI** (sends the selection to the chat panel, opening it if closed) |
| A hovered URL                   | Open Link, Copy Link                                             |
| A command's output block        | Copy Output (`Leader+y`), Re-run Command (`Leader+r`)             |
| A failed command (non-zero exit) | Shows the failed command text with a Copy Command action         |
| A tab                            | Set a per-tab accent color, picked from the active theme's bright palette |

---

## Shell Integration

For richer AI context (last command, exit code, CWD written via shell hooks), source the integration script in your `~/.zshrc`:

```zsh
source ~/.config/petruterm/shell-integration.zsh
```

> **Note:** Shell integration is optional. PetruTerm reads the terminal process's real CWD directly via OS APIs (`proc_pidinfo` on macOS) and does not require the integration script for the file picker or `AGENTS.md` auto-attach to work.

The script writes one JSON file per shell process under `~/.cache/petruterm/` after each command, for example `shell-context-12345.json`. This is read by the AI panel to include CWD, last command, and exit code in every query without panes overwriting each other's context.

---

## MCP (Model Context Protocol)

PetruTerm's AI agent supports MCP servers, letting it call tools exposed by external processes (databases, file systems, custom APIs).

### Configuration

Servers are declared in `~/.config/petruterm/mcp/mcp.json`:

```json
{
  "mcpServers": {
    "postgres": {
      "command": "npx",
      "args": [
        "-y",
        "@modelcontextprotocol/server-postgres",
        "postgresql://user:pass@localhost/mydb"
      ],
      "env": {}
    },
    "filesystem": {
      "command": "npx",
      "args": [
        "-y",
        "@modelcontextprotocol/server-filesystem",
        "/private/tmp",
        "/Users/you"
      ],
      "env": {}
    }
  }
}
```

Each key under `mcpServers` is the display name shown in the panel. PetruTerm launches the server process on startup and keeps it alive for the session.

### Using MCP tools in the panel

- Type `/mcp` in the AI panel input to list all connected servers and their available tools.
- The agent selects and calls MCP tools automatically when they are relevant to your query.
- Tool invocations appear in the panel as `⟳ mcp:server/tool(…)` while running and `✓ mcp:server/tool(…)` when complete.

### Built-in servers

| Server       | Package                                   | Description                       |
| ------------ | ----------------------------------------- | --------------------------------- |
| `filesystem` | `@modelcontextprotocol/server-filesystem` | Read/write files in allowed paths |
| `postgres`   | `@modelcontextprotocol/server-postgres`   | Query a PostgreSQL database       |

> **Requirement:** `npx` must be available in your `PATH` (comes with Node.js).

---

## AGENTS.md

Place an `AGENTS.md` file in your project root to give the AI panel automatic context about your project. It is attached as the first file every time the panel opens in that directory.

---

## Tech Stack

| Component          | Crate                                        |
| ------------------ | -------------------------------------------- |
| GPU rendering      | `wgpu` 29 (Metal on macOS)                   |
| Windowing          | `winit` 0.30                                 |
| Terminal emulation | `alacritty_terminal` 0.25                    |
| Font shaping       | `cosmic-text` 0.18 + HarfBuzz + FreeType LCD |
| Config DSL         | `mlua` 0.11 (Lua 5.4)                        |
| Async / LLM        | `tokio` + `reqwest`                          |
| Fuzzy search       | `skim` + `fuzzy-matcher`                     |
| Hashing            | `rustc-hash` (FxHasher)                      |

---

## Project Layout

```
~/.config/petruterm/
├── config.lua             # Entry point — require and compose modules
├── ui.lua                 # Font, colors, window, status bar
├── perf.lua               # Scrollback, FPS, GPU
├── keybinds.lua           # Leader key and all bindings
├── llm.lua                # AI provider/agent and features
├── snippets.lua           # Tab-expandable snippets
├── notifications.lua      # Toast vs native notification style
├── plugins/               # Auto-scanned Lua plugins
├── skills/                # SKILL.md prompts, one directory per skill (see Skills)
│   └── <name>/SKILL.md
├── steering/              # Always-on AI instructions, one .md per file (see Steering)
├── mcp/
│   └── mcp.json           # MCP server definitions (filesystem, postgres, …)
└── shell-integration.zsh  # Optional: source in ~/.zshrc
```

A project directory can add its own `.petruterm/skills/` and `.petruterm/steering/` alongside the global ones above.

### Performance notes

- Persistent row storage: shaped cell instances live in stable per-row GPU slots; a frame re-uploads only the rows that actually changed instead of rebuilding the whole pane's vertex buffer.
- Row cache: unchanged terminal rows are served from a per-pane shape cache — HarfBuzz runs only on dirty rows.
- Damage tracking: alacritty's `TermDamage` API skips grid reads for undamaged rows when no selection or search is active.
- Cursor overlay: cursor blink updates a single GPU vertex without rebuilding the cell buffer.
- Split glyph atlas: grayscale glyphs in a 16 MiB R8 texture; color/emoji in a separate 4 MiB RGBA texture (68% VRAM reduction vs. the previous single 64 MiB atlas).
- Search parallelism: scrollback search uses rayon to fan out across rows — 8–9× faster on large buffers.
- Idle: event loop parks when the window loses focus — no timer wakeups, no git polling, no redraws.
- Battery saver: present mode switches to vsync-locked Fifo on battery; git poll interval extends to 60 s; cursor blink disabled.

---

## License

[MIT](LICENSE)
