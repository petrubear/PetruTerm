# Data Model: Agent Integrations

In-memory except the hook files the setup command writes. Nothing here stores prompts, commands or payloads.

## AgentIntegration (trait object, stateless)

| Member | Required | Meaning |
|--------|----------|---------|
| `id() -> &str` | yes | CLI name (`claude`, `kiro`, `codex`, `agy`); the second argument of `petruterm hook` |
| `on_hook(event, payload, pane) -> Option<Message>` | yes | map one hook call to a report, or none |
| `events() -> &[&str]` | yes | event names it registers (used by setup, status and the contract test) |
| `hook_stdout(event) -> Option<&str>` | default none | exact text the hook must print for that event (Antigravity JSON) |
| `binaries() -> &[&str]` | default empty | executables whose presence makes `setup` install |
| `hook_file(home) -> Option<PathBuf>` | default none | the agent's global hook file |
| `install(doc) / remove(doc)` | default no-op | merge or strip our entries in the parsed JSON document, returning what changed |
| `setup_note() -> Option<&str>` | default none | extra line printed after setup (for example "trust required: run /hooks") |

Registry: a static slice of `&dyn AgentIntegration`; `find(id)` returns one. Claude's `install`/`remove` stay in the existing shim path and `claude_hooks`.

## Report

The existing `ipc::Message`: `pane`, `state` (`working`/`needs-input`/`done`/`idle`), `title`, `body`, `activity`, `session`. No new fields.

### Normalized event mapping (FR-004)

| Normalized | Claude | Kiro | Codex | Antigravity |
|------------|--------|------|-------|-------------|
| working + prompt | `UserPromptSubmit` | `UserPromptSubmit` | `UserPromptSubmit` | `PreInvocation` |
| working + tool | `PreToolUse` | `PreToolUse` | `PreToolUse` | `PreToolUse` |
| needs-input | `Notification` | none | `PermissionRequest` | none |
| done | `Stop`, `SubagentStop` | `Stop` | `Stop` | `Stop` with `fullyIdle` |
| idle | `SessionEnd` | `SessionEnd` | `Interrupt`, `SessionEnd` | none (observer) |
| session id | `SessionStart` / any `session_id` | `session_id` | `session_id` | `conversationId` |

## HookFileEntry (what we write into agent files)

| Field | Rule |
|-------|------|
| command | contains the marker `PETRUTERM_BIN`; forms in research R7 |
| timeout | 5 seconds |
| name (Kiro) | `petruterm-<event>` |
| owner | an entry is ours iff its command contains the marker |

## PtyActivity (per terminal, shared Arc between `Pty`, its reader thread and the UI)

| Field | Type | Written by | Meaning |
|-------|------|-----------|---------|
| `last_input_ms` | `AtomicU64` | `Pty::write` | when bytes were last written to the PTY |
| `last_agent_output_ms` | `AtomicU64` | `reader_loop` | when output last arrived more than 300 ms after `last_input_ms` (0 = never) |

Times are milliseconds since a process-wide epoch; relaxed ordering; no locks.

## ObservedState (pure, `term/agents/observer.rs`)

```
observed_state(now, last_agent_output_ms) ->
    None            never any agent output
    Some(Working)   last agent output < QUIET (3 s) ago
    Some(Idle)      older
merge_state(hook: Option<(AgentState, updated_at)>, observed, now) ->
    hook state, except Working older than STALE (120 s) while observed == Idle  -> Idle
    no hook state                                                               -> observed
```

Constants: `ECHO_WINDOW = 300 ms`, `QUIET = 3 s`, `STALE = 120 s` (code constants).

## AgentsConfig (one new field)

`output_state: bool`, default `true` (Lua `agents.output_state`).

## AgentRow

Unchanged. `state` becomes `merge_state(tracker state, observer state)`; `activity`, `message`, `session` as before.
