# Implementation Plan: Agent Integrations (Kiro, Codex, Antigravity)

**Branch**: `004-agent-integrations` | **Date**: 2026-11-07 | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `/specs/004-agent-integrations/spec.md`

## Summary

Turn the Claude-only hook path into an `AgentIntegration` trait with a registry (`src/ipc/integrations/`), keep Claude Code as the first implementation without changing its behavior, and add implementations for Kiro, Codex and Antigravity. Each implementation maps its agent's hook payloads to the existing normalized `Message` (state, activity, message, session) and states the exact stdout its agent requires. `petruterm hook <agent> <event>` resolves the agent through the registry; a new `petruterm hooks setup|remove|status` installs the hook entries into each agent's global hook file (merge, idempotent, marker-identified). For agents with no hook-reported state, a small PTY output observer (two atomic timestamps stamped by the existing reader thread and `Pty::write`) supplies working/idle, with hook state taking precedence and a timeout fallback.

Nothing in the card, `AgentRow`, tracker, socket schema or spec-003 contracts changes: every agent already renders through that path. A renamed agent binary needs no new config key, because reports are keyed by pane, not by agent name.

## Technical Context

**Language/Version**: Rust (edition 2021)

**Primary Dependencies**: existing only (serde_json, mlua, gpui, libc). No TOML crate is needed: Kiro, Codex and Antigravity are all configured through JSON hook files.

**Storage**: the user's own hook files (only entries carrying the `PETRUTERM_BIN` marker are ours; Kiro's file `petruterm.json` is wholly ours). Backups as `<file>.petruterm-bak` before the first modification. No payloads or prompts are stored or logged.

**Testing**: `cargo test`: per-agent mapping tests over fixture payloads, a shared contract test over every registered integration, file merge/remove tests on sample files (user entries preserved byte for byte), pure observer-state tests, and `tests/hook_cli.rs` round trips with a fake socket for each agent. Manual quickstart with real Kiro, Codex and `agy` sessions.

**Target Platform**: macOS; the gpui binary renders; the wgpu binary compiles the shared code unchanged.

**Project Type**: desktop app (two binaries, one shared core)

**Performance Goals**: a hook outside PetruTerm or with the app closed finishes in under 100 ms with exit 0; the observer adds two atomic stores per PTY read and per input write (no allocation, no locks).

**Constraints**: every hook exits 0 and prints exactly what its agent expects; no payload text in logs; files under 400 lines (`ipc/cli.rs` gets only a dispatch change, the new CLI lives in new files); observer does nothing when the Agents list is not on screen except stamping atomics.

**Scale/Scope**: 4 integrations, about 12 new files, small edits in 6 existing files.

## Constitution Check

*GATE: passes before research; re-checked after design (still passes).*

| Principle | Result |
|-----------|--------|
| I. Two binaries, one core | Pass. Integrations, hook files and the PTY activity stamps live in `src/ipc` and `src/term` (shared). Only the observer's use in row building is in `src/gpui_shell`. The wgpu binary compiles the shared code; items it never calls get `#[allow(dead_code)]` as in specs 001 to 003. |
| II. Terminal engine | Pass. The observer reads byte timing in the existing reader thread; no grid, VTE or PTY logic is reimplemented. |
| III. Lua config | Pass. One new key `agents.output_state` parsed in `src/config/lua.rs`, default documented in `config/default/notifications.lua`, no wrapper type. |
| IV. Threading | Pass. No blocking calls on the UI thread; the reader thread gains two relaxed atomic stores; hooks are separate short-lived processes. The reader-loop invariants (ordering, tee, wakeup gate) are untouched. |
| V. LLM optional | N/A. |
| VI. Stable deps | Pass. No dependency changes. |
| VII. Simplicity, module size | Pass. One trait with defaulted methods, no speculative config. `src/ipc/cli.rs` (about 270 lines with tests) only changes its dispatch; new code is in new files each well under 400 lines. |

## Project Structure

### Documentation (this feature)

```text
specs/004-agent-integrations/
├── plan.md
├── research.md
├── data-model.md
├── quickstart.md
├── contracts/
│   ├── integration-trait.md
│   ├── hook-cli.md
│   ├── hooks-setup-cli.md
│   ├── agent-hook-files.md
│   └── output-observer.md
└── tasks.md             # /speckit-tasks (not created here)
```

### Source Code (repository root)

```text
src/
├── ipc/
│   ├── integrations/
│   │   ├── mod.rs         # NEW: AgentIntegration trait, registry, shared helpers, contract test hook
│   │   ├── claude.rs      # NEW: thin impl over claude_hooks::message_for (behavior unchanged)
│   │   ├── kiro.rs        # NEW: Kiro V3 mapping + hook file (wholly ours: ~/.kiro/hooks/petruterm.json)
│   │   ├── codex.rs       # NEW: Codex mapping + merge into ~/.codex/hooks.json
│   │   └── agy.rs         # NEW: Antigravity mapping + required JSON output + merge into ~/.gemini/config/hooks.json
│   ├── hook_files.rs      # NEW: marker-based JSON merge/remove helpers, backup, dry run
│   ├── hooks_cli.rs       # NEW: `hooks setup|remove|status` (the old `hooks setup claude` keeps working)
│   ├── claude_hooks.rs    # unchanged (still the Claude mapping)
│   ├── cli.rs             # `hook` resolves through the registry and prints the agent's stdout; `hooks` -> hooks_cli
│   └── mod.rs             # `pub mod integrations; pub mod hook_files; pub mod hooks_cli;`
├── term/
│   ├── pty.rs             # PtyActivity (two atomics) stamped by reader_loop and Pty::write
│   └── agents/
│       └── observer.rs    # NEW: pure observed_state() and merge_state() (hook vs observer)
├── config/
│   ├── schema.rs          # AgentsConfig.output_state: bool (default true)
│   └── lua.rs             # parse agents.output_state
└── gpui_shell/
    ├── agent_observer.rs  # NEW: per-terminal last observed state; reports "changed" to the poll tick
    ├── agent_rows.rs      # state = merge_state(hook state, observer state)
    └── agent_scan.rs      # tick() also polls the observer (when the list is on screen)

config/default/notifications.lua   # document output_state
tests/
├── hook_cli.rs                    # extend: kiro/codex/agy round trips, agy stdout JSON
├── hooks_setup.rs                 # NEW: setup/remove/status on sample files for the 3 agents
└── fixtures/agents/{claude,kiro,codex,agy}/*.json   # recorded sample payloads per event
```

**Structure Decision**: single crate, two binaries (existing). The integration interface and all agent knowledge are in `src/ipc/integrations/` (one file per agent); the observer's pure logic is in `src/term/agents/observer.rs`; the gpui layer only combines the two sources when building rows.

## Design Decisions (detail in research.md)

1. **The normalized report is today's `Message`.** No new type and no socket change. Each integration returns `Option<Message>`.
2. **The trait has defaulted methods** (`serves`, `hook_stdout`, install hooks) so a new agent implements only `id` and `on_hook` to get a state-only integration. Required per-agent output (Antigravity) is a method, not special-cased in the CLI.
3. **The hook command always exits 0 and prints the agent's required stdout even outside PetruTerm.** The CLI prints it first, then does the best-effort send. Installed hook commands guard with `PETRUTERM_BIN` the same way the Claude hooks do; Antigravity's command falls back to `echo '{"decision":"allow"}'` (and `{}` for the other events) so the agent never sees an empty reply.
4. **Kiro's file is wholly ours** (`~/.kiro/hooks/petruterm.json`, hook names prefixed `petruterm-`); Codex's and Antigravity's files are merged into and only entries carrying the `PETRUTERM_BIN` marker are ever changed or removed.
5. **No version gating for Kiro.** V3 is a mode inside Kiro CLI 2.13+, not a distinct version number, and 2.x ignores `~/.kiro/hooks/`. Setup writes the file when `kiro-cli` is found and prints that V3 mode is required.
6. **No `integration` config key** (spec FR-012 dropped). Hook reports are keyed by pane, so a renamed binary works once its process name is in `agents.recognized`. The spec is amended.
7. **Observer stamps are taken in the reader thread**: `last_agent_output` is updated only when the read happened more than 300 ms after the last input write, so typing echo never counts. Rows are built from `merge_state`: hook state wins; a hook `Working` that has been silent for 120 s and whose terminal is quiet falls back to the observer.
8. **Fixtures first.** Mapping code is written against fixture payloads taken from each vendor's documentation; real-session verification is in the quickstart and any field that differs is a fixture and mapping fix.

## Complexity Tracking

| Item | Why | Simpler alternative rejected |
|------|-----|------------------------------|
| A trait plus registry for four implementations | The stated goal is adding an agent with one file; per-agent output (Antigravity JSON) and install differ. | A `match agent` in `cli.rs` and `hooks_cli.rs` would put agent knowledge in two places and grow with each agent. |
| `PtyActivity` stamps in `pty.rs` (a hot path) | The reader thread already sees every byte; this is the only place that can tell agent output from echo without extra threads. | Polling the terminal grid from the UI thread allocates and cannot separate echo; a second reader would break the tee/ordering invariants. |
| New `hooks_cli.rs` | `cli.rs` would grow past 400 lines with three more agents. | Extending `cli.rs` in place. |

## Risks

- **Payload shapes differ from the docs.** Kiro's tool-argument field names and prompt field, Codex's `hooks.json` wrapper key, and Antigravity's interrupt reason values are not fully documented. Mitigation: mappings degrade to "no activity" or "no report", the observer covers the state, and every unverified field is listed in research.md for the quickstart.
- **Antigravity `Stop`/`PostToolUse` may not fire** (a user report exists for the IDE on Windows). Mitigation: done is also inferred by the observer going quiet.
- **Hook env inheritance** (`PETRUTERM_PANE_ID`) is assumed. Contingency, not built unless needed: resolve the pane from the hook process's ancestor chain on the server (the machinery exists in `src/ipc/peer.rs` for `petruterm msg`).
- **Codex trust.** Installed hooks do nothing until the user trusts them with `/hooks`; setup output and `hooks status` say so, and the observer covers the gap.
