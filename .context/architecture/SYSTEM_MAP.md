# System Map

> This document describes `petruterm` — the wgpu/winit binary shipped from `master`. A second
> binary, `gpui-petruterm`, exists only on the permanent (never-merged) `worktree-gpui-migration`
> branch and shares `src/term`/`src/config`/`src/llm`/`src/platform` with this one, replacing only
> the chrome/rendering layer (`src/app`, `src/ui`, `src/renderer` → `src/gpui_shell`). See
> "gpui Chrome Binary" near the end of this file for its own architecture summary.

## High-Level Architecture

```
┌──────────────────────────────────────────────────────────────────┐
│                        macOS Process                             │
│                                                                  │
│  ┌─────────────────────────────────────────────────────────────┐ │
│  │                    winit Event Loop                         │ │
│  │                                                             │ │
│  │   WindowEvent::RedrawRequested ──► App::render()           │ │
│  │   WindowEvent::KeyboardInput   ──► App::handle_input()     │ │
│  │   WindowEvent::Resized         ──► App::resize()           │ │
│  └───────────────────────────┬─────────────────────────────────┘ │
│                               │                                  │
│                        ┌──────▼──────┐                           │
│                         │  App State  │                          │
│                         │  (src/app)  │                          │
│                        └──────┬───────┘                          │
│           ┌────────────┬──────┴──────┬─────────────┐            │
│           ▼            ▼             ▼              ▼            │
│     ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌───────────┐       │
│     │   Tab    │ │  Config  │ │ Renderer │ │  Command  │       │
│     │ Manager  │ │  (Lua)   │ │  (wgpu)  │ │  Palette  │       │
│     │(src/ui/  │ │(src/     │ │(src/     │ │ (src/ui/  │       │
│     │ tabs)    │ │ config)  │ │ renderer)│ │ palette)  │       │
│     └────┬─────┘ └────┬─────┘ └────┬─────┘ └───────────┘       │
│          │             │            │                            │
│          ▼             │            │                            │
│     ┌──────────┐       │            │                            │
│     │  Pane    │       │            │                            │
│     │ Manager  │       │            │                            │
│     │(src/ui/  │       │            │                            │
│     │ panes)   │       │            │                            │
│     └────┬─────┘       │            │                            │
│          │             │            │                            │
│          ▼             ▼            ▼                            │
│     ┌──────────────────────────────────────────┐                │
│     │          Per-Pane Terminal Instance       │                │
│     │                                          │                │
│     │  ┌────────────────────────┐              │                │
│     │  │  alacritty_terminal    │              │                │
│     │  │  ├── Term<EventProxy>  │◄─── PTY I/O  │                │
│     │  │  ├── Grid (cells)      │              │                │
│     │  │  ├── Scrollback        │              │                │
│     │  │  └── VTE Parser        │              │                │
│     │  └────────────┬───────────┘              │                │
│     │               │ cells                    │                │
│     │               ▼                          │                │
│     │  ┌────────────────────────┐              │                │
│     │  │    Font Shaper         │              │                │
│     │  │  cosmic-text + swash   │              │                │
│     │  │  (ligatures, emoji)    │              │                │
│     │  └────────────┬───────────┘              │                │
│     │               │ shaped glyphs            │                │
│     └───────────────┼──────────────────────────┘                │
│                     ▼                                            │
│     ┌────────────────────────────────┐                          │
│     │          wgpu Renderer         │                          │
│     │  ┌─────────────────────────┐   │                          │
│     │  │  Glyph Atlas (texture)  │   │  ──► Metal GPU           │
│     │  │  Cell Vertex Buffer     │   │                          │
│     │  │  WGSL Render Pipeline   │   │                          │
│     │  └─────────────────────────┘   │                          │
│     └────────────────────────────────┘                          │
│                                                                  │
│  ┌─────────────────────────────────────────────────────────────┐ │
│  │                  tokio Async Runtime (Phase 2)              │ │
│  │                                                             │ │
│  │  ┌──────────────────────────────────────────────────────┐  │ │
│  │  │                   LLM Engine                         │  │ │
│  │  │  ┌────────────┐ ┌──────────┐ ┌──────────────────┐   │  │ │
│  │  │  │ OpenRouter │ │  Ollama  │ │    LMStudio      │   │  │ │
│  │  │  │ provider   │ │ provider │ │    provider      │   │  │ │
│  │  │  └────────────┘ └──────────┘ └──────────────────┘   │  │ │
│  │  └──────────────────────────────────────────────────────┘  │ │
│  └─────────────────────────────────────────────────────────────┘ │
└──────────────────────────────────────────────────────────────────┘
```

---

## Data Flow: Keystroke → Screen

```
User types 'l' s
     │
     ▼
winit KeyboardInput event
     │
     ▼
App::handle_input()
     │
     ├─[AI mode off]──► alacritty_terminal::input::Processor::key_input()
     │                        │
     │                        ▼
     │                  PTY write (bytes → shell)
     │                        │
     │                        ▼
     │                  Shell executes → PTY read
     │                        │
     │                        ▼
     │                  alacritty_terminal VTE parser
     │                        │
     │                        ▼
     │                  Grid mutation (cells updated)
     │                        │
     │                        ▼
     │                  winit::request_redraw()
     │
     └─[AI mode on]───► LLM inline block handles input

RedrawRequested
     │
     ▼
App::render()
     │
     ▼
Font Shaper: shape dirty cells → glyph IDs + positions
     │
     ▼
Glyph Atlas: rasterize new glyphs → upload to GPU texture
     │
     ▼
Cell Vertex Buffer: build instanced quads from grid
     │
     ▼
wgpu render pass → Metal → display
```

---

## Module Dependency Graph

```
main.rs
  └── app.rs
        ├── config/          (loads first, everything reads config)
        │     ├── lua.rs     (mlua Lua VM)
        │     ├── schema.rs  (typed structs)
        │     └── watcher.rs (notify)
        │
        ├── renderer/        (GPU, no deps on term/ or ui/)
        │     ├── gpu.rs
        │     ├── atlas.rs
        │     ├── pipeline.rs
        │     └── cell.rs
        │
        ├── font/            (used by renderer/)
        │     ├── loader.rs
        │     └── shaper.rs
        │
        ├── term/            (wraps alacritty_terminal)
        │     └── pty.rs
        │
        ├── ui/
        │     ├── tabs.rs
        │     ├── panes.rs    (tree of term/ instances)
        │     └── palette/
        │           ├── mod.rs
        │           └── actions.rs
        │
        ├── llm/             (Phase 2 — async, tokio)
        │     ├── provider.rs
        │     ├── openrouter.rs
        │     ├── ollama.rs
        │     ├── lmstudio.rs
        │     └── inline.rs
        │
        └── plugins/         (Phase 3 — Lua API)
              ├── api.rs
              └── loader.rs
```

---

## Pane Layout Tree

Split panes are stored as a binary tree:

```
Node::Leaf(terminal_id)
Node::Split {
    direction: Horizontal | Vertical,
    ratio: f32,          // 0.0..1.0
    left: Box<Node>,
    right: Box<Node>,
}
```

Each `Leaf` maps to one `alacritty_terminal::Term` instance and one PTY.

---

## Config Loading Sequence

```
1. Locate config: ~/.config/petruterm/config.lua
2. mlua: create Lua VM, inject `petruterm` global table
3. mlua: dofile(config.lua)
   └── config.lua calls module.apply_to_config(config) for each module
4. mlua: extract config table → deserialize into Rust Config struct
5. notify: watch ~/.config/petruterm/ for changes
6. On change: re-run steps 2-4, diff against current config, apply deltas
```

---

## LLM Request Flow (Phase 2)

```
User presses Ctrl+Space → AI mode ON
User types NL query → presses Enter
     │
     ▼
LlmEngine::query(prompt, context)
     │
     ▼
Build messages: [system_prompt, context_lines, user_query]
     │
     ▼
LlmProvider::stream(messages) → tokio task
     │
     ▼
Stream chunks → InlineBlock::append_token()
     │
     ▼
winit::request_redraw() on each chunk
     │
     ▼
Stream complete → show [Run] [Edit] [Explain] actions
```

---

## gpui Chrome Binary (`gpui-petruterm`, `worktree-gpui-migration` only)

Replaces `src/app` + `src/ui` + `src/renderer` (winit event loop, hand-rolled wgpu chrome
rendering) with [gpui](https://www.gpui.rs/) (Zed's retained-mode Rust UI framework) driving a
real `Entity<GpuiShellRoot>`/`Render` tree instead of an imperative per-frame draw call.
`alacritty_terminal` (`src/term`), Lua config (`src/config`), LLM/ACP (`src/llm`), and platform
FFI (`src/platform`) are shared verbatim with the wgpu binary — this migration only ever touches
the chrome/rendering layer.

```
GpuiShellRoot (src/gpui_shell/mod.rs + construct.rs)
├── workspaces: WorkspaceManager      — tabs, panes, per-workspace state (workspace.rs)
├── terminals: HashMap<usize, Rc<Terminal>>  — same Terminal/Pty as the wgpu binary
├── sidebar: WorkspaceSidebar          — Workspaces/MCP/Skills/Steering sections (sidebar/)
├── chat: ChatPanelView                — AI chat drawer, Provider + ACP backends (chat_panel/)
├── ai_block: AiBlockView              — inline Ctrl+Space AI block (ai_block.rs)
├── palette / search_bar / context_menu / info_overlay / toast — same-shaped state+render pairs
└── config: Config                     — hot-reloaded, shared schema with the wgpu binary

render() (render.rs, impl Render for GpuiShellRoot)
  → terminal_card { tab_bar (tabs/) ; pane_area → pane_view::render_pane_tree }
  → sidebar drawer (when visible, via render_sidebar.rs)
  → chat panel drawer (when visible)
  → status bar (status_bar/)
  → overlays: palette, context menu, toast, info overlay (absolute-positioned)

Own render frame is one `div()` tree per frame (gpui/Taffy flexbox layout), NOT the wgpu
binary's `RoundedRectInstance` vertex-buffer pixel math — corners, borders, and pill shapes are
real `rounded_lg()`/`border_1()` style calls, not hand-computed quads.
```

**Repaint model:** gpui has no cross-thread wake bridge from a background PTY-reader thread into
its own entity-update mechanism, so `poll.rs` runs a `cx.spawn` loop on a ~33ms timer (gated on
each terminal's `WakeupGate` so an idle tick is a no-op) that drains PTY events, refreshes the
status bar's CWD/git-branch/exit-code/battery segments, and calls `cx.notify()` only when
something actually changed. This is the same repair for the `gotcha_lost_pty_echo_wakeup` class
of bug the wgpu binary hit on macOS, ported to gpui's own event model.

**400-line module convention** is enforced hard here (more so than the wgpu binary's older
code): every file in `src/gpui_shell/` is expected to stay under 400 lines, splitting into a
`foo/{mod.rs, render.rs, ...}` directory when a single concern (e.g. tabs, sidebar, status bar,
chat panel) outgrows one file.

**Not yet built (deliberately, not gaps):** chat panel drag-to-resize (sidebar has it, via the
shared `resize_handle.rs` custom `Element`; chat panel doesn't yet — no one has asked). InputShadow/
ghost-text/syntax-highlighting/flag-hints (`src/term/input_shadow.rs`, `tokenizer.rs`, `flag_db.rs`)
are real, shared, engine-agnostic code but were never wired into `gpui_shell` — confirmed gap,
explicitly deprioritized by the user (their own shell's own suggestions cover it).

## Key External Crates

| Crate | Role | Notes |
|-------|------|-------|
| `alacritty_terminal` | Terminal emulation core | VTE, grid, PTY, scrollback — do not reimplement |
| `wgpu` | GPU rendering | Metal on macOS via wgpu-hal |
| `winit` | Window + event loop | Use latest; API changed at 0.29 |
| `mlua` | Lua 5.4 VM | `features = ["lua54", "vendored"]` |
| `cosmic-text` | Text layout + shaping | Handles ligatures, BiDi, emoji |
| `swash` | Font rasterization | Used internally by cosmic-text |
| `fontdb` | Font discovery | Scans system fonts |
| `tokio` | Async runtime | Full features for LLM I/O |
| `reqwest` | HTTP client | `features = ["json", "stream"]` |
| `notify` | File watcher | Config hot-reload |
| `fuzzy-matcher` | Fuzzy search | Command palette matching |
| `bytemuck` | GPU buffer casting | `Pod` + `Zeroable` for vertex types |
| `dirs` | Config paths | `dirs::config_dir()` for `~/.config` |
| `anyhow` | Error handling | Application-level errors |
| `thiserror` | Error types | Library-style typed errors |
