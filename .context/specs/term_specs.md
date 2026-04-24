# PetruTerm — Technical Specification

**Version:** 0.1.0
**Language:** Rust (edition 2021)
**Platform:** macOS (initial), cross-platform post-Phase 1
**Config:** Lua 5.4 DSL at `~/.config/petruterm/config.lua`

---

## 1. Design Principles

1. **Speed first** — GPU rendering, alacritty_terminal core, zero-copy where possible
2. **Don't reinvent the wheel** — use proven crates (alacritty_terminal, cosmic-text, wgpu)
3. **Lua everything** — font, colors, keybinds, window, LLM, plugins, status bar all configurable in Lua
4. **Familiar patterns** — WezTerm DSL for config, lazy.nvim DSL for plugins, lualine DSL for status bar
5. **Developer first** — zsh, tmux, nvim, claude must work perfectly out of the box

---

## 2. Tech Stack

| Layer | Technology | Notes |
|-------|------------|-------|
| Language | Rust 2021 | `cargo build --release` for distribution |
| GPU rendering | wgpu (Metal on macOS) | Same as WezTerm |
| Windowing | winit | macOS NSWindow via winit |
| Terminal core | alacritty_terminal | VTE, grid, PTY, scrollback, mouse |
| Font layout | cosmic-text + swash | Ligatures, BiDi, emoji, fallback chains |
| Font discovery | fontdb | System font scanning |
| Config DSL | mlua (Lua 5.4) | `features = ["lua54", "vendored"]` |
| Async / LLM | tokio (full) | LLM streaming, file I/O |
| HTTP / LLM | reqwest | `features = ["json", "stream"]` |
| Hot reload | notify | Config + plugin file watching |
| Fuzzy search | fuzzy-matcher | Command palette |
| Errors | anyhow + thiserror | App errors + typed library errors |
| Serialization | serde + serde_json | Config structs, LLM payloads |
| Thread comms | crossbeam-channel | PTY reader thread → main thread |
| GPU buffers | bytemuck | `Pod`/`Zeroable` for vertex types |
| Config paths | dirs | `dirs::config_dir()` → `~/.config` |

---

## 3. Crate Dependencies (Cargo.toml)

```toml
[dependencies]
wgpu            = "latest"
winit           = "latest"
alacritty_terminal = "latest"
mlua            = { version = "latest", features = ["lua54", "vendored"] }
cosmic-text     = "latest"
swash           = "latest"
fontdb          = "latest"
tokio           = { version = "latest", features = ["full"] }
reqwest         = { version = "latest", features = ["json", "stream"] }
notify          = "latest"
fuzzy-matcher   = "latest"
bytemuck        = { version = "latest", features = ["derive"] }
dirs            = "latest"
anyhow          = "latest"
thiserror       = "latest"
serde           = { version = "latest", features = ["derive"] }
serde_json      = "latest"
crossbeam-channel = "latest"
```

---

## 4. Module Structure

```
src/
├── main.rs                   # Entry: init winit, load config, run event loop
├── app.rs                    # App struct, top-level event dispatch
│
├── renderer/
│   ├── mod.rs                # Renderer trait
│   ├── gpu.rs                # wgpu device, surface, queue, swap chain
│   ├── atlas.rs              # Glyph texture atlas (GPU texture)
│   ├── pipeline.rs           # WGSL shaders + wgpu render pipeline
│   └── cell.rs               # Grid cell → instanced GPU quad vertex
│
├── term/
│   ├── mod.rs                # Terminal struct (wraps alacritty_terminal::Term)
│   └── pty.rs                # PTY spawn, read thread, write, resize
│
├── ui/
│   ├── mod.rs
│   ├── tabs.rs               # Tab bar: create, close, switch, rename
│   ├── panes.rs              # Pane binary tree: split H/V, resize, focus
│   ├── statusbar/
│   │   ├── mod.rs            # Status bar engine (Phase 3)
│   │   └── widgets.rs        # Built-in widgets (Phase 3)
│   └── palette/
│       ├── mod.rs            # Cmd+Shift+P overlay, fuzzy search
│       └── actions.rs        # Built-in action registry
│
├── font/
│   ├── mod.rs
│   ├── loader.rs             # fontdb font discovery + loading
│   └── shaper.rs             # cosmic-text shaping (ligatures via HarfBuzz features)
│
├── config/
│   ├── mod.rs                # Config struct (resolved Rust types)
│   ├── lua.rs                # mlua Lua VM, petruterm global, config.lua eval
│   ├── schema.rs             # Typed config structs (serde + mlua FromLua)
│   └── watcher.rs            # notify watcher → hot-reload trigger
│
├── llm/                      # Phase 2
│   ├── mod.rs                # LlmEngine, AI mode state machine
│   ├── provider.rs           # LlmProvider trait (complete + stream)
│   ├── openrouter.rs         # OpenRouter API implementation
│   ├── ollama.rs             # Ollama (http://localhost:11434)
│   ├── lmstudio.rs           # LMStudio (http://localhost:1234/v1)
│   └── inline.rs             # Inline AI block UI state + render data
│
├── plugins/                  # Phase 3
│   ├── mod.rs                # Plugin manager: scan, load, lifecycle
│   ├── api.rs                # Lua API surface (petruterm.* globals)
│   └── loader.rs             # Scan ~/.config/petruterm/plugins/*.lua
│
└── snippets/                 # Phase 3
    ├── mod.rs
    └── manager.rs            # Load from config, register with palette
```

---

## 5. Lua Configuration DSL

### 5.1 Module Pattern (WezTerm-style)

The entry point composes modules using `apply_to_config`:

```lua
-- ~/.config/petruterm/config.lua
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

### 5.2 UI Module

```lua
-- ~/.config/petruterm/ui.lua
local petruterm = require("petruterm")
local colors    = require("colors")
local module    = {}

function module.apply_to_config(config)
  config.colors            = colors["Dracula Pro"]
  config.font              = petruterm.font("Monolisa Nerd Font")
  config.font_size         = 16
  config.font_features     = { "calt=1", "liga=1", "dlig=1" }
  config.window = {
    borderless       = true,
    start_maximized  = true,
    padding          = { left = 20, right = 20, top = 30, bottom = 10 },
    opacity          = 1.0,
  }
  config.enable_tab_bar       = true
  config.hide_tab_bar_if_one  = true
  config.window_close_confirm = false

  petruterm.on("gui-startup", function()
    petruterm.window.maximize()
  end)
end

return module
```

### 5.3 Performance Module

```lua
-- ~/.config/petruterm/perf.lua
local module = {}

function module.apply_to_config(config)
  config.scrollback_lines  = 100000
  config.enable_scroll_bar = true
  config.animation_fps     = 1           -- effectively disabled for snappiness
  config.gpu_preference    = "high_performance"
end

return module
```

### 5.4 Keybinds Module

```lua
-- ~/.config/petruterm/keybinds.lua
local petruterm = require("petruterm")
local module    = {}

function module.apply_to_config(config)
  config.leader = { key = "b", mods = "CTRL", timeout_ms = 1000 }
  config.keys = {
    -- Command palette
    { mods = "CMD|SHIFT", key = "P",      action = petruterm.action.CommandPalette },
    -- AI mode toggle
    { mods = "CTRL",      key = "Space",  action = petruterm.action.ToggleAiMode },
    -- Explain last output
    { mods = "CTRL|SHIFT", key = "E",     action = petruterm.action.ExplainOutput },
    -- Fix last error
    { mods = "CTRL|SHIFT", key = "F",     action = petruterm.action.FixLastError },
    -- Pane splits (tmux-style leader)
    { mods = "LEADER", key = "%",         action = petruterm.action.SplitHorizontal },
    { mods = "LEADER", key = '"',         action = petruterm.action.SplitVertical },
    -- Pane navigation (vim-style)
    { mods = "LEADER", key = "h",         action = petruterm.action.ActivatePane("Left") },
    { mods = "LEADER", key = "l",         action = petruterm.action.ActivatePane("Right") },
    { mods = "LEADER", key = "k",         action = petruterm.action.ActivatePane("Up") },
    { mods = "LEADER", key = "j",         action = petruterm.action.ActivatePane("Down") },
    -- Close pane
    { mods = "LEADER", key = "x",         action = petruterm.action.ClosePane },
    -- Tabs
    { mods = "CMD",    key = "T",         action = petruterm.action.NewTab },
    { mods = "CMD",    key = "W",         action = petruterm.action.CloseTab },
  }
end

return module
```

### 5.5 LLM Module

```lua
-- ~/.config/petruterm/llm.lua
local module = {}

function module.apply_to_config(config)
  config.llm = {
    provider = "openrouter",                       -- "openrouter" | "ollama" | "lmstudio"
    model    = "anthropic/claude-sonnet-4-6",
    api_key  = os.getenv("OPENROUTER_API_KEY"),
    base_url = nil,                                -- override for local providers
    -- Ollama example: provider="ollama", base_url="http://localhost:11434", model="llama3"
    -- LMStudio:       provider="lmstudio", base_url="http://localhost:1234/v1", model="..."
    features = {
      nl_to_command  = true,   -- NL → shell command
      explain_output = true,   -- explain selected output
      fix_last_error = true,   -- fix on non-zero exit
      context_chat   = true,   -- multi-turn chat
    },
    context_lines = 50,        -- lines of terminal output sent as context
    enabled = true,            -- master toggle
  }
end

return module
```

### 5.6 Color Scheme Format

```lua
-- ~/.config/petruterm/colors.lua  (same format as WezTerm)
local M = {}

M["Dracula Pro"] = {
  foreground    = "#f8f8f2",
  background    = "#22212c",
  cursor_bg     = "#9580ff",
  cursor_border = "#9580ff",
  cursor_fg     = "#f8f8f2",
  selection_bg  = "#454158",
  selection_fg  = "#c6c6c2",
  ansi    = { "#22212c", "#ff9580", "#8aff80", "#ffff80", "#9580ff", "#ff80bf", "#80ffea", "#f8f8f2" },
  brights = { "#504c67", "#ffaa99", "#a2ff99", "#ffff99", "#aa99ff", "#ff99cc", "#99ffee", "#ffffff" },
}

return M
```

---

## 6. Plugin System (Phase 3)

### 6.1 Plugin File Format (lazy.nvim-style)

Each plugin is a single Lua file in `~/.config/petruterm/plugins/`:

```lua
-- ~/.config/petruterm/plugins/my-plugin.lua
return {
  "author/my-plugin",     -- identifier (built-in id or GitHub slug)
  enabled = true,
  config = function()
    local plugin = require("my-plugin")
    plugin.setup({
      option_a = true,
      key      = "f",
    })
  end,
}
```

### 6.2 Plugin Lua API

```lua
-- Register a command palette action
petruterm.palette.register({
  name   = "My Plugin: Do Thing",
  action = function() petruterm.notify("Hello!") end,
})

-- Register a status bar widget
petruterm.statusbar.register_widget({
  name   = "my_widget",
  render = function() return "text" end,
})

-- Subscribe to events
petruterm.on("tab_created",  function(tab_id) end)
petruterm.on("tab_closed",   function(tab_id) end)
petruterm.on("pane_split",   function(pane_id, direction) end)
petruterm.on("command_run",  function(cmd, exit_code) end)
petruterm.on("ai_response",  function(text) end)
petruterm.on("gui-startup",  function() end)

-- Utilities
petruterm.notify("message")
petruterm.open_url("https://example.com")
petruterm.execute("shell command")

-- Install external plugin (git clone)
petruterm.plugins.install("user/repo")
```

### 6.3 Built-in Plugin: Status Bar

```lua
-- ~/.config/petruterm/plugins/status-bar.lua
return {
  "petruterm/status-bar",
  enabled = true,
  config = function()
    require("petruterm.statusbar").setup({
      position   = "bottom",     -- "top" | "bottom"
      theme      = "dracula",
      separators = { left = "", right = "" },
      sections = {
        left  = { "mode", "cwd" },
        right = { "git_branch", "time", "exit_code" },
      },
    })
  end,
}
```

---

## 7. LLM Integration (Phase 2)

### 7.1 Mode Toggle

- **Normal mode:** all input goes to PTY shell
- **AI-Assisted mode:** `Ctrl+Space` activates inline AI block

```
⚡ AI > [cursor — type natural language here]
```

### 7.2 Feature 1: NL → Shell Command

1. User types NL query, presses Enter
2. LLM receives: `system_prompt` + last `context_lines` of terminal output + user query
3. Response streams token-by-token into the block
4. On completion: `[⏎ Run]` `[Edit]` `[Explain]` action buttons appear
   - **Run:** send command string directly to PTY
   - **Edit:** place command in shell prompt line for editing
   - **Explain:** re-query LLM with "explain this command in detail"

### 7.3 Feature 2: Explain Last Output

- Trigger: `Ctrl+Shift+E` or command palette "Explain Last Output"
- If text is selected: explain the selection
- Otherwise: explain the last command + its output block
- Response appears in a new inline block below

### 7.4 Feature 3: Fix Last Error

- Trigger: `Ctrl+Shift+F` or command palette "Fix Last Error"
- Also: subtle indicator (⚠) appears in the prompt area after any non-zero exit
- LLM receives: failed command + full output + exit code
- Response: explanation of error + corrected command with `[Run]` `[Edit]` actions

### 7.5 Feature 4: Context-Aware Chat

- Trigger: `Ctrl+Space` then type (same as NL→command, but multi-turn)
- LLM receives: full chat history + CWD + last `context_lines` of output + shell history
- Chat thread persists for the lifetime of the pane (cleared on pane close)
- Not persisted across sessions (Phase 1 scope)

### 7.6 Provider Trait

```rust
#[async_trait]
pub trait LlmProvider: Send + Sync {
    async fn complete(&self, messages: Vec<Message>) -> anyhow::Result<String>;
    async fn stream(
        &self,
        messages: Vec<Message>,
    ) -> anyhow::Result<Pin<Box<dyn Stream<Item = anyhow::Result<String>> + Send>>>;
}
```

### 7.7 Provider Endpoints

| Provider | Base URL | Auth |
|----------|----------|------|
| OpenRouter | `https://openrouter.ai/api/v1` | `Authorization: Bearer <api_key>` |
| Ollama | `http://localhost:11434` | None (local) |
| LMStudio | `http://localhost:1234/v1` | None (local, OpenAI-compat) |

All providers use the OpenAI chat completions format (`/v1/chat/completions`).

---

## 8. Command Palette

Opened with `Cmd+Shift+P`. Fuzzy-searchable overlay. Actions sourced from:
1. Built-in registry (hardcoded in `src/ui/palette/actions.rs`)
2. Plugin-registered actions (Phase 3, via `petruterm.palette.register()`)
3. Snippet names (Phase 3, auto-registered by snippet manager)

### Built-in Actions (Phase 1)

| Action | Description |
|--------|-------------|
| Open Config File | Opens `~/.config/petruterm/config.lua` |
| Reload Config | Hot-reloads Lua config |
| New Tab | Creates a new tab |
| Close Tab | Closes the current tab |
| Split Pane Horizontal | Splits current pane horizontally |
| Split Pane Vertical | Splits current pane vertically |
| Close Pane | Closes the current pane |
| Toggle Fullscreen | Toggles macOS fullscreen |

### Additional Actions (Phase 2)

| Action | Description |
|--------|-------------|
| Toggle AI Mode | Switch between Normal and AI-Assisted |
| Enable AI Features | Master enable (sets `config.llm.enabled = true`) |
| Disable AI Features | Master disable (sets `config.llm.enabled = false`) |
| Explain Last Output | Trigger AI explanation of last output |
| Fix Last Error | Trigger AI fix suggestion for last error |

### Additional Actions (Phase 3)

| Action | Description |
|--------|-------------|
| Enable Status Bar | Show status bar |
| Disable Status Bar | Hide status bar |
| Run Snippet: `<name>` | Execute a named snippet |
| Install Plugin | Prompt for GitHub slug, git clone |

---

## 9. Font System

- **Discovery:** `fontdb` scans system fonts + `~/.config/petruterm/fonts/`
- **Shaping:** `cosmic-text` with HarfBuzz features (`calt`, `liga`, `dlig`)
- **Rasterization:** `swash` rasterizes glyphs to bitmaps
- **Atlas:** GPU texture atlas stores rasterized glyph bitmaps; new glyphs uploaded on demand
- **Emoji:** color emoji supported via swash color layer rendering
- **Fallback chain:** configurable in Lua as `config.font_fallback = { "Font A", "Font B", "Noto Color Emoji" }`

Default harfbuzz features (matching user's WezTerm config):
```lua
config.font_features = { "calt=1", "liga=1", "dlig=1" }
```

---

## 10. Window System

### Title Bar Styles
- `"custom"` (default): PetruTerm draws its own title bar with tab strip integrated
- `"native"`: use macOS native NSWindow title bar
- `borderless = true`: no title bar at all (set via Lua)

### Window Config Schema (Lua)
```lua
config.window = {
  borderless       = false,
  initial_width    = nil,         -- nil = use OS default
  initial_height   = nil,
  start_maximized  = true,
  title_bar_style  = "custom",    -- "custom" | "native"
  padding          = { left = 20, right = 20, top = 30, bottom = 10 },
  opacity          = 1.0,
}
```

---

## 11. Shell Integration

Auto-sourced when `config.shell_integration = true` (default: true).

Script: `~/.config/petruterm/shell-integration.zsh`

Tracks via OSC sequences:
- `OSC 7` — current working directory
- `OSC 133` — command start/end + exit code (semantic zones)
- `OSC 1337` — iTerm2-compatible user vars

Enables:
- Accurate CWD in status bar and LLM context
- Exit code tracking for Fix Last Error feature
- Semantic command zones (Warp-style block selection, future)

---

## 12. Scrollback & Performance

| Setting | Default | Lua key |
|---------|---------|---------|
| Scrollback lines | 100,000 | `config.scrollback_lines` |
| Scroll bar | Enabled | `config.enable_scroll_bar` |
| Animation FPS | 1 (disabled) | `config.animation_fps` |
| GPU preference | High performance | `config.gpu_preference` |
| Render FPS target | 60 | `config.max_fps` |

---

## 13. Default Config Files

Shipped with the app in `config/default/`, embedded in binary via `include_str!`:

```
config/default/
├── config.lua       # Entry point (composes modules)
├── ui.lua           # Font, colors, window (Dracula Pro + Monolisa defaults)
├── perf.lua         # Scrollback, GPU, animation
├── keybinds.lua     # All keybinds (Leader+tmux-style)
└── llm.lua          # LLM provider config (disabled by default, user fills key)
```

Copied to `~/.config/petruterm/` on first launch if directory doesn't exist.

---

## 14. Distribution

- **Build:** `cargo build --release` → `target/release/petruterm`
- **Bundle:** `scripts/bundle.sh` wraps binary in `PetruTerm.app` (macOS .app bundle)
- **Bundle structure:**
  ```
  PetruTerm.app/
  └── Contents/
      ├── Info.plist
      ├── MacOS/
      │   └── petruterm          # binary
      └── Resources/
          └── assets/            # themes, icons
  ```
- No code signing for Phase 1 (dev use only); add in post-Phase 3


---

## 15. Frame Budget (REC-PERF-05)

Performance targets for PetruTerm on Apple Silicon (primary platform, Phase 1).

| Scenario | Target | Metric |
|---|---|---|
| Input-to-pixel (keystroke visible on screen) | < 8 ms p99 | One frame at 120 Hz |
| Steady-state idle (no terminal activity) | 0 CPU/GPU work | ControlFlow::Wait + focus guard |
| Cache-miss cold start (first frame after launch) | < 16 ms | One frame at 60 Hz |
| Atlas evict + reshape storm (large scroll) | < 50 ms | Acceptable stutter budget |
| Streaming LLM token render | < 16 ms per token frame | Smooth at 60+ Hz |

### HUD monitoring

Press **F12** in-app to toggle the latency HUD. Displays rolling p50/p95/p99 frame times.
p99 > 8 ms renders in red as a regression signal.

### CI regression gate

`benches/` contains criterion benchmarks for `shape_line`, `search`, and `build_instances`.
CI fails if any benchmark regresses > 5% versus the stored baseline (critcmp).

### Measurement methodology

Latency samples collected via `latency_samples: VecDeque<f32>` (120 entries) on `RenderContext`.
Sampled from `RedrawRequested` entry to `queue.submit()`. The HUD displays p50/p95/p99 live.

### Known headroom

- `build_instances` hot path: damage tracking skips undamaged rows (alacritty_terminal `TermDamage` API).
- Atlas warmup at startup: all 95 printable ASCII glyphs pre-rasterized to eliminate cold-start misses.
- `parking_lot::Mutex` used for all internal locks: ~2x faster than `std::sync::Mutex` on uncontended paths.
