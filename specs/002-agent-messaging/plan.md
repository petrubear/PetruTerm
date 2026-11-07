# Implementation Plan: Agent-to-Agent Messaging

**Branch**: `002-agent-messaging` | **Date**: 2026-11-06 | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `/specs/002-agent-messaging/spec.md`

## Summary

Add `petruterm msg <name> <text>`: a shell command any agent (or the user) can run to send a message to another running agent by its custom name. The command talks to the running app over the existing control socket, but unlike hooks it gets an answer back: the connection thread hands the request to the UI thread, which resolves sender and target, applies acceptance, rate and conversation limits, sanitizes the text, and types a framed message into the target's pane (bracketed paste when the pane enabled it, then Enter). Sender identity is established from the connecting process (peer pid walked up to a pane shell), not from what the caller claims. A per-agent "Accept messages" toggle (default off) and a "Send message…" item on the Agents list menu complete the UI; `petruterm agents setup` installs a Claude skill and a Kiro steering file with standing reply instructions.

Pure logic (sanitizing, framing, limits, conversation tracking, result codes) goes in shared `src/term/messaging*`; protocol and CLI in `src/ipc`; routing, acceptance and the compose editor in the gpui chrome.

## Technical Context

**Language/Version**: Rust (edition 2021)

**Primary Dependencies**: existing only (serde_json, crossbeam-channel, libc, gpui). No new crates.

**Storage**: none for messages (never written to disk or logs); acceptance and conversation state are in memory; the setup command writes two instruction files the user asked for.

**Testing**: `cargo test` (pure logic and the socket round trip with a fake handler); `tests/` integration for the CLI exit codes and setup idempotency; manual quickstart with a real Claude Code and Kiro pair.

**Target Platform**: macOS (peer-pid lookup uses `LOCAL_PEERPID`)

**Project Type**: desktop app (two binaries, one shared core); delivery is implemented in the gpui binary, the wgpu binary answers "not supported in this build"

**Performance Goals**: command result within 1 s; delivery typed within 1 s; zero cost when idle

**Constraints**: socket worker threads wait at most 2 s for the UI thread; the UI thread does bounded work per tick; no message text in logs; modules under 400 lines; `poll.rs`, `context_menu.rs`, `cli.rs`, `ui/context_menu.rs` get hooks of <= ~10 lines

**Scale/Scope**: a handful of agents, tens of messages per minute at most

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principle | Assessment |
|---|---|
| I. Two binaries, one core | PASS. Pure logic in shared `term/messaging`; protocol/CLI in `ipc` (compiled into both binaries); only routing/UI in `gpui_shell`. The wgpu binary replies "not supported in this build" instead of leaving the CLI waiting. |
| II. No reimplementing the terminal engine | PASS. Uses `Terminal::write_input` and `bracketed_paste_mode()`; no grid or PTY logic. |
| III. Lua config, atomic swap | PASS. No new config keys (limits are constants, documented as tunable later). |
| IV. Threading and async discipline | PASS with care. Socket threads block on a per-request one-shot with a 2 s timeout; the UI thread drains requests each tick and never blocks. The scan thread (spec 001) is reused; liveness check at delivery is a non-blocking `kill(pid, 0)`. |
| V. LLM optional | PASS. Unrelated to LLM features; agents are external processes. |
| VI. Stable dependencies | PASS. No new crates. |
| VII. Simplicity and module size | PASS with the same rule as 001: hooks of <= ~10 lines in files already over 400 lines (`poll.rs` 451, `render.rs` 432); real logic in new files. |
| Protocol constraint (backward compatible control socket) | PASS. New request type is a new optional field; old hooks and CLIs are unaffected. |

Post-design re-check: unchanged, all PASS. No Complexity Tracking entries.

## Project Structure

### Documentation (this feature)

```text
specs/002-agent-messaging/
├── plan.md
├── research.md
├── data-model.md
├── quickstart.md
├── contracts/
│   ├── cli-msg.md
│   ├── socket-msg-protocol.md
│   ├── delivered-frame.md
│   ├── agents-menu-messaging.md
│   └── agents-setup-cli.md
└── tasks.md             # /speckit-tasks (not created here)
```

### Source Code (repository root)

```text
src/
├── term/
│   ├── messaging.rs             # NEW shared: Outcome + exit codes + text, sanitize_text, length check
│   └── messaging/
│       ├── frame.rs             # NEW: framed text for agent/shell/user senders, reply and no-reply variants
│       ├── limits.rs            # NEW: RateLimiter (sliding window) + Conversations (pair, 2 min gap, cap 10)
│       └── decide.rs            # NEW: pure `decide(...) -> Result<(), Outcome>` combining every check, so routing is unit-testable
├── ipc/
│   ├── mod.rs                   # EDIT: optional `msg` payload on Message (hook messages unaffected)
│   ├── msg_bus.rs               # NEW: request/reply plumbing: MsgRequest, one-shot reply, drain for the UI
│   ├── peer.rs                  # NEW: LOCAL_PEERPID of a connection + ancestor walk helper
│   ├── msg_cli.rs               # NEW: `petruterm msg ...` (text from args or stdin) and exit codes
│   ├── agent_setup.rs           # NEW: `petruterm agents setup|remove [--dry-run]`
│   ├── server.rs                # EDIT: route `msg` lines to msg_bus and write the reply (<= ~25 lines)
│   └── cli.rs                   # EDIT: dispatch `msg` and `agents` (2 lines)
├── gpui_shell/
│   ├── agent_msg.rs             # NEW: router: drain requests, resolve sender/target, checks, deliver, reply
│   ├── agent_accept.rs          # NEW: per-agent acceptance (terminal + pid), retain_live
│   ├── agent_compose.rs         # NEW: "Send message…" inline editor (pattern of agent_rename.rs)
│   ├── agent_menu.rs            # EDIT: two new items (Accept messages / Send message…)
│   ├── agent_scan.rs            # EDIT: scan also while messaging is in use, regardless of sidebar/focus
│   ├── context_menu.rs          # EDIT: delegate two new actions
│   ├── poll.rs                  # EDIT: one call to agent_msg::tick
│   ├── mod.rs / construct.rs    # EDIT: AgentAccept, AgentMsgState, compose editor state
│   └── sidebar/agents.rs        # EDIT: accepting badge, compose editor row
├── ui/context_menu.rs           # EDIT: ContextAction::{AgentToggleAccept, AgentSendMessage}
└── app/mod.rs                   # EDIT: no-op arms for the new actions (wgpu build)
config/default/agent-messaging/
├── claude-skill.md              # NEW: SKILL.md content installed by setup
└── kiro-steering.md             # NEW: steering file content installed by setup
tests/
└── msg_cli.rs                   # NEW: CLI exit codes against a fake socket, setup idempotency
```

**Structure Decision**: same layering as spec 001: shared pure logic in `src/term`, protocol and CLIs in `src/ipc`, gpui-only routing and UI in `src/gpui_shell`.

## Complexity Tracking

No constitution violations to justify.
