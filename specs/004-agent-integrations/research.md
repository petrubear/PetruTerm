# Research: Agent Integrations

Verified 2026-11-07 from the vendors' documentation and the repository. Items marked **UNVERIFIED** are settled against real sessions in `quickstart.md`; the design tolerates them being wrong.

## R1 - Kiro CLI hooks

**Decision**: V3 global hook file `~/.kiro/hooks/petruterm.json`, wholly owned by us.

**Findings** ([docs](https://kiro.dev/docs/hooks/), [types](https://kiro.dev/docs/hooks/types/), [actions](https://kiro.dev/docs/hooks/actions/), [migration](https://kiro.dev/docs/cli/v3/hooks-migration/)):
- File schema: `{"version":"v1","hooks":[{"name","trigger","matcher"?,"action":{"type":"command","command"},"timeout"?,"enabled"?}]}`; `timeout` in seconds, default 60.
- V3 triggers (PascalCase): `SessionStart`, `UserPromptSubmit`, `PreToolUse`, `PostToolUse`, `Stop`, `SessionEnd`; `agentSpawn` is the 2.x alias of `SessionStart`.
- stdin JSON: `hook_event_name`, `cwd`, `session_id`; tool events add `tool_name` and `tool_input`. Canonical tool names `fs_read`, `fs_write`, `execute_bash`, `use_aws` (aliases `read`, `write`, `shell`, `aws`); MCP tools appear as `@server/tool`.
- Command actions: exit 0 adds stdout to the agent's context; any other exit blocks `PreToolUse` and `UserPromptSubmit`. So the hook must exit 0 and print nothing.
- V3 is a mode of Kiro CLI 2.13+ (and the default from V3), so the version number cannot gate setup.
- cmux (reference) also uses a global hook file and discards lifecycle output.
- **UNVERIFIED**: the `prompt` stdin field for `UserPromptSubmit` (2.x docs have it, V3 docs do not document it; the docs mention a `USER_PROMPT` variable for shell actions); the `tool_input` field names for `execute_bash` and `fs_*` (a third-party IDE capture shows empty args for `execute_bash`).

**Mapping**: `UserPromptSubmit` -> working (+ `Prompt: ...` if `prompt` present); `PreToolUse` -> working (+ tool activity); `Stop` -> done; `SessionEnd` -> idle; `SessionStart` -> session id only. Tool labels: `execute_bash`/`shell` -> `Shell`, `fs_read`/`read` -> `Read`, `fs_write`/`write` -> `Write`, `use_aws`/`aws` -> `AWS`, others as is. Targets tried in order: `command`, `path`, `file_path`, `paths[0]`, `file`; none found -> label only.

**Alternatives**: per-agent config (`~/.kiro/agents/<name>.json`, `--agent`): only sessions started with that agent report; rejected. Workspace `.kiro/hooks/`: per-project; out of scope.

## R2 - Codex CLI hooks

**Decision**: merge into `~/.codex/hooks.json`.

**Findings** ([docs](https://learn.chatgpt.com/docs/hooks)):
- Hooks come from `hooks.json` or `[hooks]` tables in `config.toml` (user and project level); all sources run. Hooks are on by default (`[features] hooks`), older `codex_hooks` is a deprecated alias. Structure: event -> matcher group -> `hooks` handlers (`type: "command"`, `command`, `timeout` seconds, default 600).
- Events: `SessionStart`, `SessionEnd`, `UserPromptSubmit`, `PreToolUse`, `PostToolUse`, `PermissionRequest`, `Stop`, `SubagentStart`, `SubagentStop`, `Interrupt`, and compact/model-request events.
- stdin: `session_id`, `cwd`, `hook_event_name`, `turn_id`, `model`; `prompt` (UserPromptSubmit); `tool_name`, `tool_input`, `tool_use_id` (tool events); `last_assistant_message` (Stop); `tool_input.description` on `PermissionRequest`.
- Exit 0 with plain stdout becomes developer context for `SessionStart` and `UserPromptSubmit`; so the hook prints nothing. Exit 2 blocks.
- Non-managed hooks must be reviewed and trusted with `/hooks` before they run.
- Third-party sources: `tool_name` is `Bash` (`tool_input.command`) and `apply_patch` (`tool_input.patch`); apply_patch and MCP coverage was historically incomplete; hooks became stable around v0.124.
- No TOML crate in the project, so `hooks.json` (JSON) is the file we merge into.
- **UNVERIFIED**: whether `hooks.json` wraps events in a top-level `"hooks"` key (assumed yes, as Claude's `settings.json`), exact tool names beyond `Bash` and `apply_patch`, and whether a launch-time flag can inject hooks without the trust step (cmux's Codex wrapper suggests one exists).

**Mapping**: `UserPromptSubmit` -> working (+ prompt); `PreToolUse` -> working (+ activity: `Bash: <command>`, `apply_patch: <first file path in the patch>`, others name only); `PermissionRequest` -> needs-input with `tool_input.description` or `Permission: <tool>`; `Stop` -> done; `Interrupt` and `SessionEnd` -> idle; `SessionStart` -> session id only.

## R3 - Antigravity CLI hooks

**Decision**: merge into `~/.gemini/config/hooks.json` under a hook named `petruterm`.

**Findings** ([docs](https://antigravity.google/docs/hooks?tab=cli)):
- Locations: workspace `.agents/hooks.json`, global `~/.gemini/config/hooks.json`; hooks in `~/.gemini/antigravity-cli/settings.json` are not loaded.
- Shape: `{ "<hook name>": { "<Event>": [handlers or {matcher, hooks:[handlers]}] } }`, handler `{type:"command", command, timeout}` (default 30 s); `"enabled": false` per hook name. `PreToolUse`/`PostToolUse` use matcher groups; matchers are regexes on the tool name (`""`/`*` all).
- Events: `SessionStart`, `PreToolUse`, `PostToolUse`, `PreInvocation`, `PostInvocation`, `Stop`.
- stdin is camelCase JSON with no event-name field (the event is passed on the command line, which our installed command does): `conversationId`, `workspacePaths[]`, `transcriptPath`, `modelName`, `lastUserInput`; `PreToolUse`/`PostToolUse` add `toolCall.name`, `toolCall.args`, `stepIdx`; `Stop` adds `executionNum`, `terminationReason` (examples `model_stop`, `max_steps_exceeded`, `error`), `fullyIdle` (bool).
- Output is JSON on stdout: `PreToolUse` requires `{"decision": ...}` (`allow` lets the call proceed to the next hook or default policy); `PostToolUse` returns `{}`; `Stop` continues only for `decision: "continue"`.
- Tool args: `run_command.CommandLine`, `view_file.AbsolutePath`, `write_to_file.TargetFile`, `replace_file_content.TargetFile`, `multi_replace_file_content.TargetFile`, `list_dir.DirectoryPath`, `find_by_name.Pattern`, `grep_search.Query`, `search_web.query`, `read_url_content.Url`.
- A forum report says `Stop`/`PostToolUse` hooks did not fire in the Antigravity IDE (Windows); the CLI is untested.
- **UNVERIFIED**: env inheritance into the hook process; the full `terminationReason` list (no interrupt value known); a documented minimal `Stop` output (assumed `{}`).

**Mapping**: `PreInvocation` -> working (+ `Prompt: <lastUserInput>`); `PreToolUse` -> working (+ activity from the tool table above, labels: `run_command` -> `Shell`, `view_file` -> `Read`, writes/replaces -> `Edit`, `grep_search` -> `Grep`, `find_by_name` -> `Find`, `list_dir` -> `List`, others as is) and prints `{"decision":"allow"}`; `PostToolUse`, `PostInvocation` print `{}`; `Stop` with `fullyIdle: true` -> done (prints `{}`); `Stop` with `fullyIdle: false` -> no report; `SessionStart` -> session id only. Workspace for path relativization: first `workspacePaths` entry.

## R4 - Why no `integration` config key

Hook reports travel `pane -> socket -> tracker` and never mention the agent name. A custom or renamed binary therefore gets the same hook-driven cards as soon as (a) its hook file entries exist (installed for the real agent) and (b) its process name is in `agents.recognized` so the pane is detected. A key would add config surface without behavior. Spec FR-012 and the matching scenario are removed.

## R5 - Observer design

**Decision**: two atomics per terminal in `Pty` (`last_output_ms`, `last_agent_output_ms`, milliseconds since a process epoch) plus `last_input_ms`, all `AtomicU64` with relaxed ordering.

**Findings**: `reader_loop` (`src/term/pty.rs`) already runs per terminal on its own thread and sees every read; `Pty::write` is the single choke point for user input (and message delivery). Stamping there needs no new threads, locks or allocation. A read that happens within 300 ms after the last input write is treated as echo and does not update `last_agent_output_ms`; anything later does (a streaming agent continues past the window).

**Rules** (pure, in `observer.rs`): `observed_state(now, last_agent_output)` -> `Working` if the last agent output is under 3 s old (so the card holds working across short pauses), `Idle` if it is older but the terminal has ever produced agent output, `None` if never. The 1 s / 4 s numbers in the spec's SC-006 follow from a 1 s working window refresh and the 3 s quiet period; the constant is a code constant, not config.

**Merge** (`merge_state(hook, observed, now)`): a hook-reported state wins; a hook `Working` older than 120 s with `observed == Idle` becomes `Idle` (covers a missing `Stop` after a crash, kill or an interrupt that the agent never reports); no hook state -> the observed state.

**Alternatives**: diffing the grid from the UI thread (allocates, cannot separate echo, UI-thread cost); parsing agent spinners (agent-specific, fragile). Both rejected.

**Echo caveat**: a program that redraws its input box on every keystroke after 300 ms of user pause could look like output; typing continuously stays within the window because each keystroke re-stamps input.

## R6 - Hook file ownership and merge rules

**Decision**: marker-based, like the existing Claude merge (`claude_hooks::merge_into`).
- Claude: unchanged (shim, plus the existing `hooks setup claude`).
- Kiro: our own file; identified by its name and the marker in every command; `remove` deletes it only if it still contains the marker.
- Codex and Antigravity: merge into the user's file; an entry is ours iff its command contains `PETRUTERM_BIN`; `remove` strips only those handlers, pruning matcher groups, events and (Antigravity) the hook name when they become empty; a file that becomes empty is deleted only if `setup` created it (recorded by the backup absence).
- Unparsable file -> refuse and do not write. Backup `<file>.petruterm-bak` before the first change. `--dry-run` writes nothing.

## R7 - Hook command forms

- Kiro/Codex/Claude style (no output required): `[ -z "$PETRUTERM_BIN" ] || "$PETRUTERM_BIN" hook <agent> <Event>` (exit 0 either way; the binary always exits 0).
- Antigravity (output required): `if [ -n "$PETRUTERM_BIN" ]; then "$PETRUTERM_BIN" hook agy <Event>; else echo '<default JSON for Event>'; fi`. The binary prints the same default JSON itself, even outside a pane, so both paths give the agent what it needs.
- Timeouts set to 5 s in every installed entry.

## R8 - Pane resolution contingency

If an agent does not pass `PETRUTERM_PANE_ID` to hook processes, reports would carry pane 0 and be dropped. The server can resolve the pane from the connecting process's ancestor chain (`peer_pid` + `ancestors` already exist for `petruterm msg`). Not implemented; only if the quickstart shows missing inheritance.

## R9 - Open questions from the spec

1. Codex trust prompt: ship file-based install with a clear message; revisit a launch-time flag only if one is documented. Setup and `hooks status` print "trust required: run /hooks in Codex".
2. Kiro and Antigravity interrupts: rely on the observer's quiet period and the 120 s fallback.
3. Copilot: observer only.
