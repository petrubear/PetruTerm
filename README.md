# PetruTerm

A developer-first GPU-accelerated terminal emulator written in Rust. Built for speed and extensibility, with first-class AI integration, a Lua configuration DSL, font ligatures, and a tmux-style tab/pane system.

> **Platform:** macOS (primary). Linux planned for Phase 2+.

---

## Features

- **GPU rendering** via wgpu (Metal on macOS) — 60 fps, low latency
- **Full terminal emulation** — xterm-256color, truecolor, bracketed paste, SGR mouse, OSC 52 clipboard
- **Font ligatures** — HarfBuzz shaping with `calt`, `liga`, `dlig` OpenType features
- **Tabs & split panes** — tmux-style keybinds, binary-tree layout
- **AI agent panel** — context-aware chat with file attachment, NL→command, explain output, fix errors
- **Inline AI block** — `Ctrl+Space` for quick NL→shell command without leaving the terminal
- **Command palette** — fuzzy-search for all actions (`Leader+p`)
- **Lua configuration** — hot-reload on save, no restart required
- **Scrollback** — configurable depth with GPU scroll bar

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

---

### `perf.lua` — Performance

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `config.scrollback_lines` | number | `10000` | Maximum scrollback buffer depth per pane. |
| `config.enable_scroll_bar` | bool | `true` | Show the 6 px scroll bar on the right edge when scrollback is active. |
| `config.max_fps` | number | `60` | Target render frame rate. |
| `config.animation_fps` | number | `1` | Animation tick rate. Set to `1` to disable smooth animations. |
| `config.gpu_preference` | string | `"high_performance"` | GPU selection hint: `"high_performance"` or `"low_power"`. |
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
config.leader = { key = "b", mods = "CTRL", timeout_ms = 1000 }
```

Press `Ctrl+B`, release, then press the bound key within `timeout_ms` milliseconds.

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
| `Leader+p` | Open command palette |
| `Leader+a` | Toggle AI panel (open → focus → close) |
| `Leader+e` | Explain last terminal output |
| `Leader+f` | Fix last error |
| `Leader+c` | New tab |
| `Leader+&` | Close tab |
| `Leader+n` | Next tab |
| `Leader+p` | Previous tab |
| `Leader+1–9` | Switch to tab N |
| `Leader+%` | Split pane horizontally |
| `Leader+"` | Split pane vertically |
| `Leader+x` | Close active pane |
| `Leader+h/j/k/l` | Move focus between panes |

#### Custom bindings

```lua
config.keys = {
    { mods = "LEADER", key = "a",  action = petruterm.action.ToggleAiPanel },
    { mods = "LEADER", key = "c",  action = petruterm.action.NewTab },
    { mods = "LEADER", key = "n",  action = petruterm.action.NextTab },
    { mods = "LEADER", key = "p",  action = petruterm.action.PrevTab },
    { mods = "LEADER", key = "%",  action = petruterm.action.SplitHorizontal },
    { mods = "LEADER", key = '"',  action = petruterm.action.SplitVertical },
    { mods = "LEADER", key = "x",  action = petruterm.action.ClosePane },
}
```

#### Available actions

| Action | Description |
|--------|-------------|
| `petruterm.action.CommandPalette` | Open command palette |
| `petruterm.action.ToggleAiPanel` | Toggle AI agent panel |
| `petruterm.action.ExplainLastOutput` | Send last terminal output to AI for explanation |
| `petruterm.action.FixLastError` | Send last failed command to AI for a fix |
| `petruterm.action.NewTab` | Open a new tab |
| `petruterm.action.CloseTab` | Close the current tab |
| `petruterm.action.NextTab` | Switch to the next tab |
| `petruterm.action.PrevTab` | Switch to the previous tab |
| `petruterm.action.SplitHorizontal` | Split active pane horizontally |
| `petruterm.action.SplitVertical` | Split active pane vertically |
| `petruterm.action.ClosePane` | Close the active pane |
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
| `provider` | string | `"openrouter"` | LLM provider: `"openrouter"`, `"ollama"`, or `"lmstudio"`. |
| `model` | string | `"meta-llama/llama-3.1-8b-instruct:free"` | Model identifier. Format depends on the provider. |
| `api_key` | string\|nil | `nil` | API key. Use `os.getenv("VAR")` to avoid hardcoding secrets. Only required for `openrouter`. |
| `base_url` | string\|nil | `nil` | Override the provider's base URL. `nil` uses the default. |
| `context_lines` | number | `50` | Lines of terminal output included as context in AI requests. |

#### Provider defaults

| Provider | Default `base_url` | Requires `api_key` |
|----------|-------------------|---------------------|
| `openrouter` | `https://openrouter.ai/api/v1` | Yes |
| `ollama` | `http://localhost:11434/v1` | No |
| `lmstudio` | `http://localhost:1234/v1` | No |

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
```

---

## AI Agent Panel

Open with `Leader+a`. The panel cycles through three states on repeated presses: **open** → **focused** → **closed**.

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
| `/q` or `/quit` | Close panel and close current tab |

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
| Font shaping | `cosmic-text` 0.18 + HarfBuzz |
| Config DSL | `mlua` 0.11 (Lua 5.4) |
| Async / LLM | `tokio` + `reqwest` |
| Fuzzy search | `fuzzy-matcher` |

---

## Project Layout

```
~/.config/petruterm/
├── config.lua            # Entry point — require and compose modules
├── ui.lua                # Font, colors, window
├── perf.lua              # Scrollback, FPS, GPU
├── keybinds.lua          # Leader key and all bindings
├── llm.lua               # AI provider and features
└── shell-integration.zsh # Optional: source in ~/.zshrc
```

---

## License

MIT
