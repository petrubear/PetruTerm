# Implementation Plan: Agents Sidebar Tab

**Branch**: `001-agents-sidebar-tab` | **Date**: 2026-11-06 | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `/specs/001-agents-sidebar-tab/spec.md`

## Summary

Add a fifth "Agents" section to the gpui sidebar listing every recognized CLI agent running in
any pane of any workspace/tab. Agent identity comes from a background process scan of each pane's
foreground process group (no IPC change, no per-agent setup); state, unread flag and last message
come from the existing `AttentionTracker`, which hooks already feed. Pure logic (process-table
parsing, agent matching, foreground filtering, ordering) lives in a new shared module
`src/term/agents.rs`; the gpui chrome adds a scan bridge, a section renderer and keyboard nav.
The recognized-agent list is a Lua config option with built-in defaults.

## Technical Context

**Language/Version**: Rust (edition 2021)

**Primary Dependencies**: existing only (gpui, serde, mlua, libc). No new crates.

**Storage**: N/A (in-memory; recomputed from the process table and attention tracker)

**Testing**: `cargo test` (unit tests for pure logic in `term/agents.rs`; sidebar cursor/nav tests alongside existing ones); manual quickstart for UI

**Target Platform**: macOS (Darwin `ps`)

**Project Type**: desktop app (two binaries, one shared core); this feature targets `gpui-petruterm`

**Performance Goals**: row appears/disappears within 5 s (scan every 2 s while the Agents section is on screen); zero scan cost when the sidebar is closed or another section is active; UI usable with 20+ rows

**Constraints**: scan runs on a background thread (never on the render/UI thread); one `ps` invocation per scan; modules under 400 lines; no new IPC message fields

**Scale/Scope**: tens of panes; one window; 5 default recognized agents, user-extendable

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principle | Assessment |
|---|---|
| I. Two binaries, one core | PASS. Pure logic goes in shared `src/term/agents.rs` (no UI deps); only rendering/nav/bridge are gpui-specific, so the wgpu binary can adopt it later. No duplication. |
| II. Do not reimplement terminal engine | PASS. Only reads child pids already held by `Terminal`; no grid/PTY logic. |
| III. Lua config, full re-eval, atomic swap | PASS. New `agents.recognized` key read through the existing `config.agents` table; `config/default/notifications.lua` updated in the same change. |
| IV. Threading/async discipline | PASS. Process scan on a named background thread with a static result slot drained by the existing poll loop (same bridge as `workspace_info`). No blocking on the render thread; no per-frame allocation (rows built only while the section renders). |
| V. LLM optional | PASS. Unrelated to LLM features; ACP/chat sessions explicitly excluded. |
| VI. Stable dependencies | PASS. No new dependencies. |
| VII. Simplicity and module size | PASS with constraints. Several files to be touched are already over 400 lines: `gpui_shell/poll.rs` (445), `term/attention.rs` (480), `term/mod.rs` (501), `config/lua.rs` (786), `config/mod.rs` (408), `workspace_info.rs` (410). Rule for this feature: edits to those files MUST be single-call hooks of at most ~10 lines (a call, a field, a `pub mod`); all real logic goes in new files (`term/agents.rs`, `gpui_shell/agent_scan.rs`, `gpui_shell/sidebar/agents.rs`, `config/agents_cfg.rs`). `sections.rs` (352) must not grow. Splitting the pre-existing oversized files is out of scope and tracked separately. |

Post-design re-check (after Phase 1): unchanged, all PASS. Complexity Tracking not needed.

## Project Structure

### Documentation (this feature)

```text
specs/001-agents-sidebar-tab/
├── plan.md
├── research.md
├── data-model.md
├── quickstart.md
├── contracts/
│   ├── config-agents-recognized.md
│   └── sidebar-agents-ui.md
└── tasks.md             # /speckit-tasks (not created here)
```

### Source Code (repository root)

```text
src/
├── term/
│   ├── agents.rs            # NEW shared: ps parsing, matching, fg filter, row ordering (+ tests)
│   ├── attention.rs         # EDIT: pub `updated_at` accessor on PaneAttention
│   └── mod.rs               # EDIT: pub mod agents
├── config/
│   ├── agents_cfg.rs        # NEW: parse + validate agents.recognized from the Lua table
│   ├── schema.rs            # EDIT: AgentsConfig.recognized field only
│   └── lua.rs               # EDIT: one call into agents_cfg.rs
├── gpui_shell/
│   ├── agent_scan.rs        # NEW: background scan bridge (static slot, drain, due-check)
│   ├── attention.rs         # EDIT: focus_terminal -> pub(super); agent_rows() builder
│   ├── poll.rs              # EDIT: one call into agent_scan::tick(); logic lives in agent_scan.rs
│   ├── mod.rs               # EDIT: mod agent_scan; AgentScanState field
│   ├── construct.rs         # EDIT: init AgentScanState
│   ├── leader_dispatch.rs   # EDIT: mark scan due in the ToggleWorkspaceSidebar arm
│   ├── render_sidebar.rs    # EDIT: pass rows + on_open_agent callback
│   ├── sidebar_nav.rs       # EDIT: Agents cursor move + Enter activate
│   └── sidebar/
│       ├── mod.rs           # EDIT: SidebarSection::Agents, cursor, next/prev
│       ├── render.rs        # EDIT: fifth tab label, dispatch to agents section, width min
│       └── agents.rs        # NEW: render_agents_section
config/default/notifications.lua   # EDIT: document agents.recognized
```

**Structure Decision**: single crate, shared pure logic in `src/term`, gpui-only glue in `src/gpui_shell`, matching the existing attention-layer layout. No wgpu changes.

## Complexity Tracking

No constitution violations to justify.
