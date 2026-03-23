# PetruTerm

## Overview

PetruTerm is a developer-first terminal emulator written in Rust, built for speed and extensibility. It provides GPU-accelerated rendering, a Lua configuration DSL modeled after WezTerm, a command palette, first-class LLM integration (Warp-style), font ligatures, snippets, and a lazy.nvim-style plugin system. Primary target: macOS.

## Tech Stack

- **Language:** Rust (edition 2021)
- **GPU:** wgpu (WebGPU / Metal on macOS)
- **Windowing:** winit
- **Terminal Core:** alacritty_terminal (VTE/xterm/PTY/grid)
- **Font:** cosmic-text + swash + fontdb (ligatures, emoji, fallback chains)
- **Config DSL:** Lua 5.4 via mlua
- **LLM:** tokio + reqwest (OpenRouter, Ollama, LMStudio)
- **Key Dependencies:** wgpu, winit, alacritty_terminal, mlua, cosmic-text, tokio, reqwest, notify, fuzzy-matcher

## Architecture

PetruTerm uses a winit event loop as its backbone. The App struct owns a Tab/Pane manager, an alacritty_terminal instance per pane, and a wgpu Renderer. A Lua VM (mlua) loads config at startup and watches for hot-reload via notify. The LLM engine runs on a tokio runtime and streams responses into the inline AI block. Plugins are Lua files auto-scanned from `~/.config/petruterm/plugins/`.

## Quick Commands

| Action  | Command                 |
| ------- | ----------------------- |
| Build   | `cargo build`           |
| Release | `cargo build --release` |
| Test    | `cargo test`            |
| Check   | `cargo check`           |
| Lint    | `cargo clippy`          |
| Format  | `cargo fmt`             |
| Bundle  | `./scripts/bundle.sh`   |

## Project Structure

```
PetruTerm/
├── CLAUDE.md                    # This file
├── Cargo.toml                   # Workspace manifest
├── Cargo.lock
├── scripts/
│   └── bundle.sh                # .app bundle script
├── assets/
│   └── themes/                  # Bundled color themes (Lua)
├── config/
│   └── default/                 # Default config files shipped with app
│       ├── config.lua
│       ├── ui.lua
│       ├── perf.lua
│       ├── keybinds.lua
│       └── llm.lua
├── src/
│   ├── main.rs                  # Entry point
│   ├── app.rs                   # App struct, event loop dispatch
│   ├── renderer/                # wgpu GPU renderer
│   ├── term/                    # Terminal engine (wraps alacritty_terminal)
│   ├── ui/                      # Tabs, panes, status bar, command palette
│   ├── font/                    # Font loading + shaping
│   ├── config/                  # Lua DSL + hot reload
│   ├── llm/                     # LLM providers + inline AI mode
│   ├── plugins/                 # Plugin loader + Lua API
│   └── snippets/                # Snippet manager
├── .context/
│   ├── core/
│   │   ├── SESSION_STATE.md
│   │   └── ACTIVE_CONTEXT.md
│   ├── architecture/
│   │   └── SYSTEM_MAP.md
│   ├── specs/
│   │   ├── term_specs.md        # Full technical specification
│   │   └── build_phases.md      # Phased build plan
│   └── quality/
│       └── TECHNICAL_DEBT.md
└── tests/
```

## Conventions

- Use `async`/`await` with tokio for all I/O-bound work (LLM, file watching)
- GPU work stays on the main thread; spawn tokio tasks for LLM requests
- Config types are plain Rust structs derived from `serde::Deserialize`; Lua values are deserialized into them via mlua
- Module files stay under 400 lines; split when exceeded
- Error handling: use `anyhow::Result` for application errors, `thiserror` for library-style errors
- All Lua API functions exposed to plugins must be documented in `src/plugins/api.rs`

## Context Files

| File                                  | Purpose                                       |
| ------------------------------------- | --------------------------------------------- |
| `.context/core/SESSION_STATE.md`      | Current session status and handoff notes      |
| `.context/core/ACTIVE_CONTEXT.md`     | Current focus area and in-scope files         |
| `.context/architecture/SYSTEM_MAP.md` | Detailed architecture + component map         |
| `.context/specs/term_specs.md`        | Full technical specification (authoritative)  |
| `.context/specs/build_phases.md`      | Phased build plan with deliverables checklist |
| `.context/quality/TECHNICAL_DEBT.md`  | Known debt registry                           |

## Current Focus

**Phase 1 — Core Terminal (MVP).** See `.context/specs/build_phases.md` for deliverables checklist and exit criteria.

## Important Notes

- `alacritty_terminal` owns the terminal grid and PTY; do not reimplement grid logic
- The wgpu renderer reads cells from alacritty_terminal's grid and maps them to GPU vertices
- Lua config is loaded once at startup; hot-reload replaces only changed fields (no full restart)
- LLM features are entirely optional and can be disabled via `config.llm.enabled = false`
- Default theme: Dracula Pro. Default font: Monolisa Nerd Font (fallback: JetBrains Mono)
- macOS only for Phase 1; cross-platform considered for Phase 2+

## Git Commit Standard

All commits have to follow this standard

```
[TASK_ID] type: Message.

Body.
```

Where `TASK_ID` and `Body` are optional, but `type` and `Message` are mandatory.

`type` must be one of the following:

| Type       | When to use                                            |
| ---------- | ------------------------------------------------------ |
| `feat`     | New additions                                          |
| `fix`      | Bug corrections                                        |
| `chore`    | Formatting, tooling, or changes that don't affect code |
| `refactor` | Changes that don't affect functionality                |
