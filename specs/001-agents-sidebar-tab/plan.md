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

## Amendment 2026-11-06: session id and custom names (US5, US6)

Adds (a) the agent's session id on each row and (b) a right-click menu with Rename / Copy
session id / Reset name, where the custom name is the future address for inter-agent
messaging. Approach:
- **Session id sources, in priority order** (research R10-R12): the id Claude Code passes to
  every hook (forwarded over the control socket in a new optional `session` field of
  `ipc::Message`); `--resume`/`--resume-id`/`--session-id` style arguments found in the agent's
  process command line; Kiro's on-disk session store keyed by pid, only if a verification spike
  confirms the layout on this machine. Agents with no reliable source show no id (FR-019).
- **Process identity**: `AgentDetection` gains the matched process `pid`, so session ids and
  custom names are tied to one running agent and clear themselves when a different agent
  process takes over the pane (FR-025).
- **Custom names**: in-memory store `AgentNames` (terminal id -> pid + name) with pure
  validation in shared `term/agents.rs`; reuses the existing inline `TextInput` rename pattern
  and the gpui right-click context menu with three new `ContextAction` variants. Exposes
  `terminal_for(name)` for the later messaging feature (FR-027). Nothing is persisted.

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

Post-design re-check (after Phase 1): unchanged, all PASS. Amendment re-check: I PASS (shared validation in `term`, gpui-only UI); IV PASS (Kiro store lookup runs on the scan thread, never on render); VI PASS (no new crates); VII PASS with the same rule (hooks of <= ~10 lines in `poll.rs`, `term/mod.rs`, `ui/context_menu.rs`; real logic in new files; `agents.rs` is at 269 lines and splits to `agents/names.rs` before 400); protocol constraint PASS (`ipc::Message.session` is optional and ignored by older senders/receivers, so the control-socket format stays backward compatible). Complexity Tracking not needed.

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

Amendment documents: `contracts/ipc-message-session.md`, `contracts/agent-context-menu.md`
(plus updated `contracts/sidebar-agents-ui.md`), research R10-R15, data-model additions.

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

# Amendment (US5, US6)
src/
├── term/agents.rs           # EDIT: AgentDetection.pid/.session, args session parsing, validate_name,
│                            #       AgentRow.custom_name/.session (keep < 400 lines: move name+session
│                            #       helpers to term/agents/names.rs if needed)
├── ipc/
│   ├── mod.rs               # EDIT: Message.session (optional, sanitized)
│   └── claude_hooks.rs      # EDIT: copy payload.session_id into Message; add SessionStart to hook set
├── ui/context_menu.rs       # EDIT: ContextAction::{AgentRename, AgentCopySessionId, AgentResetName}
└── gpui_shell/
    ├── agent_names.rs       # NEW: AgentNames store (set/get/reset/retain_live/terminal_for)
    ├── agent_sessions.rs    # NEW: AgentSessions store (reported id per terminal+pid, priority rules)
    ├── session_lookup.rs    # NEW (only if the Kiro spike confirms a store): pid -> session id
    ├── agent_rename.rs      # NEW: inline rename editor + validation message (pattern of rename.rs)
    ├── agent_menu.rs        # NEW: builds the 3-item agent context menu, handles its actions
    ├── poll.rs              # EDIT: <= 5 lines: hand msg.session to AgentSessions in the drain loop
    ├── agent_rows.rs        # EDIT: fill custom_name/session in rows
    └── sidebar/agents.rs    # EDIT: right-click handler, name/session rendering, editor row
```

**Structure Decision**: single crate, shared pure logic in `src/term`, gpui-only glue in `src/gpui_shell`, matching the existing attention-layer layout. No wgpu changes.

## Complexity Tracking

No constitution violations to justify.
