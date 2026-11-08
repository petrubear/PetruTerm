---

description: "Task list for agent integrations (Kiro, Codex, Antigravity)"
---

# Tasks: Agent Integrations (Kiro, Codex, Antigravity)

**Input**: Design documents from `/specs/004-agent-integrations/`

**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/, quickstart.md. Builds on `specs/003-agent-activity-cards` (activity text, `clean_activity`, `tool_activity`, cards) and `specs/001`/`002` (Agents list, scan, socket, `petruterm msg`).

**Tests**: Included (spec FR-015): per-agent mapping tests over fixture payloads, a shared contract test over every registered integration, hook-file merge/remove tests on sample files, pure observer tests, and CLI round trips against a fake socket. Real Kiro, Codex and `agy` sessions are validated through `quickstart.md`.

**Organization**: Grouped by user story. The trait, registry, contract-test harness, file helpers and the generic `hooks setup|remove|status` command come first because every agent story uses them.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: can run in parallel (different files, no dependency on an incomplete task)
- **[Story]**: US1..US7 from spec.md

## Conventions

- Single crate; agent knowledge only in `src/ipc/integrations/<agent>.rs`; shared pure logic in `src/term`; gpui code only combines sources when building rows.
- Hooks always exit 0 and print exactly what the agent expects (contracts/hook-cli.md). Never log or store payloads, prompts, commands or paths.
- A report is the existing `ipc::Message` (state, activity, body, session). Activity text always goes through `clean_activity` (spec 003). `relative_to` in `src/term/agents/activity.rs` becomes `pub` for reuse.
- Fixture payloads live in `tests/fixtures/agents/<agent>/<Event>.json`, taken from the vendors' documentation (research.md). Anything marked UNVERIFIED there must degrade to "no activity" or "no report", never to an error.
- Before editing a file read it; before changing a function grep its callers (AGENTS.md). Files stay under 400 lines; `src/ipc/cli.rs` only changes its dispatch.
- Items only the gpui build uses need `#[allow(dead_code)]` (or `cfg_attr`) so the wgpu binary compiles warning-free.
- Verify with `./scripts/ci-local.sh` (`RUSTFLAGS=-D warnings`); it needs the Metal toolchain. `cargo test --lib <filter>` and `cargo test --test <name>` work on a cached build without it.

---

## Phase 1: Setup

- [X] T001 Create `src/ipc/integrations/mod.rs` (module doc only), `src/ipc/hook_files.rs` and `src/ipc/hooks_cli.rs` (module docs only) and add `pub mod integrations; pub mod hook_files; pub mod hooks_cli;` next to the existing module declarations in `src/ipc/mod.rs`
- [X] T002 [P] Create `src/term/agents/observer.rs` (module doc only, `// Used by the gpui binary only` dead-code allowance like its siblings) and add `mod observer;` plus `#[allow(unused_imports)] pub use observer::*;` next to the existing `mod activity;` lines in `src/term/agents.rs`

---

## Phase 2: Foundational (blocks every user story)

**Purpose**: the interface, its test harness, the hook-file helpers and the generic setup command.

- [X] T003 In `src/ipc/integrations/mod.rs` define `pub trait AgentIntegration: Sync` exactly as in data-model.md (required `id`, `events`, `on_hook`; defaulted `hook_stdout`, `binaries`, `hook_file`, `install`, `remove`, `setup_note`), `pub fn all() -> &'static [&'static dyn AgentIntegration]` (initially empty) and `pub fn find(id: &str) -> Option<&'static dyn AgentIntegration>`; add a `report(pane, state, activity, session, body) -> Message` builder and make `relative_to` in `src/term/agents/activity.rs` `pub`; put the recipe from contracts/integration-trait.md in the module doc; tests: `find` of an unknown id is `None`, registry ids are unique and lowercase
- [X] T004 In `src/ipc/integrations/mod.rs` add a `#[cfg(test)] pub(crate) mod contract` helper `check_integration(i: &dyn AgentIntegration, fixtures_dir: &str)` implementing the shared checks of contracts/integration-trait.md: loads each `<fixtures_dir>/<Event>.json`, `on_hook` returns a report with the pane set, activity only with a `working` state and free of control characters and at most 80 characters, `hook_stdout` is `Some` only where valid JSON is expected; garbage payloads (null, numbers, arrays, 10 MB strings, deeply nested objects) never panic and unknown events return `None`; plus a `contract_case!(integration, "Event", expected_state, expected_activity_prefix)` macro for the per-agent expectations
- [X] T005 [P] In `src/ipc/hook_files.rs` implement the shared helpers: `pub const MARKER: &str = "PETRUTERM_BIN"`, `contains_marker(&Value) -> bool` (any string value in the subtree contains the marker), `load_json(path) -> Result<Option<(String, Value)>, String>` (missing file is `Ok(None)`, empty file is `{}`, invalid JSON is an error that says the file is not touched), `write_json(path, &Value, existing: Option<&str>, dry_run) -> Result<String, String>` (creates parent directories, writes `<path>.petruterm-bak` first when the file existed, returns the human-readable report, writes nothing on dry run) and `prune_empty(&mut Value)` (drops empty matcher groups and events left by a removal); tests on sample files: backup content, dry run writes nothing, invalid JSON refused unchanged, pruning
- [X] T006 In `src/ipc/hooks_cli.rs` implement `pub fn run(args: &[String]) -> i32` for `setup`, `remove` and `status` per contracts/hooks-setup-cli.md: parse the agent selector (`kiro|codex|agy|all`, default all, `claude` is not part of all) and `--dry-run`; for each selected integration find its binary with a `find_binary(names, path_dirs)` helper (PATH search, injectable for tests), skip with a note when absent, load the file through `hook_files`, call `install`/`remove`, write through `write_json`, print the per-agent line and `setup_note`; exit codes 0/1/2; make every function take the home directory and PATH dirs as parameters so tests use temporary directories; route `hooks setup|remove|status` in `hooks()` in `src/ipc/cli.rs` to it while `hooks setup claude [--dry-run] [--settings PATH]` keeps using the existing code; tests with a fake integration for the flow, skipping and exit codes

**Checkpoint**: `cargo test --lib integrations`, `cargo test --lib hook_files` and `cargo test --lib hooks_cli` pass.

---

## Phase 3: User Story 1 - One interface, Claude Code unchanged (Priority: P1) MVP

**Goal**: `petruterm hook <agent> <event>` resolves through the registry with Claude Code as the first implementation, and nothing changes for Claude users.

**Independent Test**: Run the existing Claude hook and attention tests unchanged, and run spec 003's quickstart steps 3 and 4: the activity line, states and cards are identical.

- [X] T007 [US1] Create `src/ipc/integrations/claude.rs`: a unit struct `Claude` implementing `AgentIntegration` with `id` `"claude"`, `events` taken from `claude_hooks::EVENTS`, `on_hook` calling `claude_hooks::message_for(event, payload, pane)` unchanged, no `hook_stdout`, no install (the shim and `hooks setup claude` stay as they are); register it in `all()` in `src/ipc/integrations/mod.rs`
- [X] T008 [US1] Add `tests/fixtures/agents/claude/{SessionStart,UserPromptSubmit,PreToolUse,Notification,Stop,SessionEnd}.json` (payloads as in `src/ipc/claude_hooks.rs` tests and the Claude hooks reference) and a `#[cfg(test)]` module in `src/ipc/integrations/claude.rs` running `check_integration` plus `contract_case!` lines for each event, and a test asserting `Claude.on_hook` returns exactly what `claude_hooks::message_for` returns for every fixture
- [X] T009 [US1] In `hook()` in `src/ipc/cli.rs` replace the `agent != "claude"` check: resolve the agent with `integrations::find` (unknown agent: return 0 silently), print `hook_stdout(event)` first when the integration has one (always, flush before anything else, even without a pane or socket), then `pane_target()`, read stdin with the existing limits, call `on_hook`, `deliver` the report, return 0; extend `tests/hook_cli.rs`: unknown agent exits 0 with empty stdout, a Claude event still produces the same socket line as before (reuse the existing tests unchanged), outside a pane a Claude hook prints nothing and exits 0

**Checkpoint**: all existing `claude_hooks` and `hook_cli` tests pass unchanged; `cargo test --lib integrations` passes for Claude.

---

## Phase 4: User Story 2 - Kiro reports state and activity (Priority: P1)

**Goal**: Kiro CLI V3 cards show working, a live activity line, and done.

**Independent Test**: `petruterm hooks setup kiro`, start `kiro-cli` in a pane, ask it to run a command: working with activity, then done (quickstart step 3).

- [X] T010 [US2] Add fixtures `tests/fixtures/agents/kiro/{SessionStart,UserPromptSubmit,PreToolUse_execute_bash,PreToolUse_fs_read,PreToolUse_mcp,PreToolUse_no_input,Stop,SessionEnd}.json` using the documented fields (`hook_event_name`, `cwd`, `session_id`, `tool_name`, `tool_input`, `prompt`; research R1)
- [X] T011 [US2] Create `src/ipc/integrations/kiro.rs` with the mapping of research R1: `UserPromptSubmit` -> working (+ `Prompt: ...` when a `prompt` string is present), `PreToolUse` -> working (+ `<label>: <target>` where label maps `execute_bash`/`shell` -> `Shell`, `fs_read`/`read` -> `Read`, `fs_write`/`write` -> `Write`, `use_aws`/`aws` -> `AWS`, anything else as is, and target is the first string found under `command`, `path`, `file_path`, `paths[0]`, `file` with `relative_to(cwd)` applied to paths; no target gives the label alone; `@server/tool` names give the name alone), `Stop` -> done, `SessionEnd` -> idle, `SessionStart` -> session id only; session id from `session_id` through `sanitize_session`; `hook_stdout` is `None` for every event; unit tests over the fixtures and for missing/odd fields
- [X] T012 [US2] In `src/ipc/integrations/kiro.rs` implement the install side: `binaries()` = `["kiro-cli", "kiro"]`, `hook_file(home)` = `$KIRO_HOME/hooks/petruterm.json` or `<home>/.kiro/hooks/petruterm.json`, `install` writes the document of contracts/agent-hook-files.md (one `petruterm-<Event>` entry for `SessionStart`, `UserPromptSubmit`, `PreToolUse`, `Stop`, `SessionEnd`, command `[ -z "$PETRUTERM_BIN" ] || "$PETRUTERM_BIN" hook kiro <Event>`, `timeout` 5), `remove` deletes our entries (the file when nothing else remains), `setup_note` = "requires Kiro CLI V3 mode"; tests on a temporary home: creates the file, second run changes nothing, remove deletes it, a pre-existing user entry in the same file is preserved
- [X] T013 [US2] Register `Kiro` in `all()` in `src/ipc/integrations/mod.rs`; add the `check_integration` module and `contract_case!` lines for every fixture in `kiro.rs`; add `tests/hook_cli.rs` round trips for `hook kiro UserPromptSubmit`, `hook kiro PreToolUse` (activity `Shell: ...`) and `hook kiro Stop` (state done, no `activity` key), and one asserting the command prints nothing to stdout

**Checkpoint**: Kiro mapping, contract and setup tests pass; `petruterm hooks setup kiro --dry-run` prints the planned file.

---

## Phase 5: User Story 3 - Codex reports state, activity and needs-input (Priority: P1)

**Goal**: Codex cards show working, activity, needs input on approval requests, done, and idle on interrupt.

**Independent Test**: Install the hooks, trust them with `/hooks`, run `codex` and ask for an action that needs approval (quickstart step 3).

- [X] T014 [US3] Add fixtures `tests/fixtures/agents/codex/{SessionStart,UserPromptSubmit,PreToolUse_Bash,PreToolUse_apply_patch,PreToolUse_other,PermissionRequest,PermissionRequest_no_description,Stop,Interrupt,SessionEnd}.json` using the documented fields (`session_id`, `cwd`, `hook_event_name`, `turn_id`, `prompt`, `tool_name`, `tool_input`, `last_assistant_message`; research R2)
- [X] T015 [US3] Create `src/ipc/integrations/codex.rs` with the mapping of research R2: `UserPromptSubmit` -> working (+ prompt), `PreToolUse` -> working (+ `Bash: <command>`; `apply_patch: <first path from a `*** Add|Update|Delete File: <path>` header in `tool_input.patch`>`, relative to `cwd`; other tools name only), `PermissionRequest` -> needs-input with body `tool_input.description` or `Permission: <tool>`, `Stop` -> done, `Interrupt` and `SessionEnd` -> idle, `SessionStart` -> session id only; `hook_stdout` `None` everywhere; unit tests over the fixtures including a patch with several files and a missing patch
- [X] T016 [US3] In `src/ipc/integrations/codex.rs` implement the install side: `binaries()` = `["codex"]`, `hook_file(home)` = `<home>/.codex/hooks.json`, `install` merges handlers `{type: "command", timeout: 5, command: "[ -z \"$PETRUTERM_BIN\" ] || \"$PETRUTERM_BIN\" hook codex <Event>"}` for `SessionStart`, `UserPromptSubmit`, `PreToolUse` (matcher `*`), `PermissionRequest` (matcher `*`), `Stop`, `Interrupt`, `SessionEnd` under the wrapper key constant `HOOKS_KEY = "hooks"` (UNVERIFIED, one constant), skipping events that already carry the marker, `remove` strips only marked handlers and prunes with `hook_files::prune_empty`, `setup_note` = "trust required: run /hooks in Codex once"; tests on sample files: user hooks preserved exactly, idempotent, remove restores the user content
- [X] T017 [US3] Register `Codex` in `all()`; add `check_integration` and `contract_case!` lines for every fixture in `codex.rs`; add `tests/hook_cli.rs` round trips for `hook codex UserPromptSubmit`, `PreToolUse` with a `Bash` payload, `PermissionRequest` (state needs-input and body) and `Interrupt` (state idle), each asserting empty stdout

**Checkpoint**: Codex mapping, contract and merge tests pass.

---

## Phase 6: User Story 4 - Antigravity reports state and activity (Priority: P2)

**Goal**: Antigravity cards show working, activity and done, and its hooks never alter the agent's behavior.

**Independent Test**: Install the hooks, run `agy`, ask it to run a command: working with the command, then done when fully idle (quickstart step 3).

- [X] T018 [US4] Add fixtures `tests/fixtures/agents/agy/{SessionStart,PreInvocation,PreToolUse_run_command,PreToolUse_view_file,PreToolUse_write_to_file,PreToolUse_unknown,PostToolUse,PostInvocation,Stop_idle,Stop_not_idle}.json` using the documented camelCase fields (`conversationId`, `workspacePaths`, `lastUserInput`, `toolCall.name`, `toolCall.args`, `stepIdx`, `fullyIdle`, `terminationReason`; research R3)
- [X] T019 [US4] Create `src/ipc/integrations/agy.rs` with the mapping of research R3: `PreInvocation` -> working (+ `Prompt: <lastUserInput>`), `PreToolUse` -> working (+ `<label>: <target>` with labels `run_command` -> `Shell` (`CommandLine`), `view_file` -> `Read` (`AbsolutePath`), `write_to_file`/`replace_file_content`/`multi_replace_file_content` -> `Edit` (`TargetFile`), `grep_search` -> `Grep` (`Query`), `find_by_name` -> `Find` (`Pattern`), `list_dir` -> `List` (`DirectoryPath`), `search_web` -> `Search` (`query`), `read_url_content` -> `Fetch` (`Url`), others name only; paths relative to the first `workspacePaths` entry), `Stop` with `fullyIdle == true` -> done and `false` or absent -> no report, `SessionStart` -> session id only (from `conversationId`), `PostToolUse`/`PostInvocation` -> no report; `hook_stdout`: `{"decision":"allow"}` for `PreToolUse`, `{}` for `PostToolUse`, `PreInvocation`, `PostInvocation`, `Stop`, `SessionStart`; unit tests over the fixtures
- [X] T020 [US4] In `src/ipc/integrations/agy.rs` implement the install side: `binaries()` = `["agy", "antigravity"]`, `hook_file(home)` = `<home>/.gemini/config/hooks.json`, `install` merges the hook named `petruterm` of contracts/agent-hook-files.md (events `SessionStart`, `PreInvocation`, `PreToolUse` and `PostToolUse` as matcher groups with matcher `*`, `Stop`) with the command form `if [ -n "$PETRUTERM_BIN" ]; then "$PETRUTERM_BIN" hook agy <Event>; else echo '<default JSON>'; fi`, `timeout` 5, `remove` deletes the `petruterm` hook name only, `setup_note` none; tests on sample files: other hook names preserved exactly, idempotent, the generated commands print the default JSON when run through `sh -c` with `PETRUTERM_BIN` unset
- [X] T021 [US4] Register `Agy` in `all()`; add `check_integration` and `contract_case!` lines for every fixture in `agy.rs`; add `tests/hook_cli.rs` cases: `hook agy PreToolUse` prints exactly `{"decision":"allow"}` and exits 0 outside a pane, with a dead socket, and inside a pane (while still sending the report); `hook agy Stop` prints `{}`; `hook agy PreToolUse` with a `run_command` payload sends activity `Shell: ...`

**Checkpoint**: Antigravity mapping, stdout contract and merge tests pass.

---

## Phase 7: User Story 5 - Any agent gets a state without hooks (Priority: P2)

**Goal**: panes whose agent has no hook-reported state show working/idle from terminal output.

**Independent Test**: with hooks removed, run any agent: working while it streams, idle about 3 s after; typing alone does not flip it (quickstart step 5).

- [X] T022 [US5] In `src/term/pty.rs` add `pub struct PtyActivity { last_input_ms: AtomicU64, last_agent_output_ms: AtomicU64 }` with `pub fn now_ms() -> u64` (milliseconds since a process-wide `OnceLock<Instant>`), `fn on_input()` (stamps input), `fn on_output()` (stamps agent output only when the last input is more than `ECHO_WINDOW_MS = 300` old) and `pub fn last_agent_output_ms()`; create an `Arc<PtyActivity>` in `Pty::spawn`, pass a clone to `reader_loop` (stamp after each successful read, before the VTE advance) and stamp input in `Pty::write`; expose `pub fn activity(&self) -> &Arc<PtyActivity>` on `Pty`; keep ordering, the tee and the wakeup gate untouched; tests: echo within 300 ms of an input does not count, output later does, never-written stays 0; add `#[allow(dead_code)]` where the wgpu binary does not use it
- [X] T023 [P] [US5] In `src/term/agents/observer.rs` implement `QUIET_MS = 3000`, `STALE_HOOK_SECS = 120`, `pub fn observed_state(now_ms: u64, last_agent_output_ms: u64) -> Option<AgentState>` (0 -> `None`; under `QUIET_MS` old -> `Working`; else `Idle`) and `pub fn merge_state(hook: Option<(AgentState, Instant)>, observed: Option<AgentState>, now: Instant) -> Option<AgentState>` per data-model.md (hook wins; a hook `Working` older than `STALE_HOOK_SECS` with `observed == Some(Idle)` becomes `Idle`; no hook state returns `observed`); unit tests for each branch and the boundaries
- [X] T024 [P] [US5] Add `output_state: bool` (default `true`) to `AgentsConfig` in `src/config/schema.rs`, parse `agents.output_state` next to `activity` in `src/config/lua.rs` using `Option<bool>` like its neighbors, document the key in `config/default/notifications.lua`, and extend the config test in `src/config/mod.rs` (default true, `false` honored)
- [X] T025 [US5] Create `src/gpui_shell/agent_observer.rs` with `AgentObserver { last: HashMap<usize, Option<AgentState>> }` and `poll(&mut self, terminals: impl Iterator<Item = (usize, u64)>, now_ms: u64) -> bool` (compute `observed_state` per terminal id, return true when any entry changed, drop ids that are gone) plus `state_of(id)`; add `mod agent_observer;` and a field `agent_observer` to `GpuiShellRoot` in `src/gpui_shell/mod.rs` and `construct.rs`; tests for change detection
- [X] T026 [US5] In `src/gpui_shell/agent_rows.rs` compute each row's `state` as `merge_state(tracked.filter(|p| p.updated_at.is_some()).map(|p| (p.state, p.updated_at.unwrap())), observed, Instant::now())` where `observed` is `observed_state(now_ms(), terminal.pty.activity().last_agent_output_ms())` when `config.agents.output_state` is true and `None` otherwise; leave `unread`, `message`, `activity` as they are; add a test for the row-level combination if `agent_rows.rs` has a pure seam, otherwise cover it through `merge_state` tests
- [X] T027 [US5] In `tick()` in `src/gpui_shell/agent_scan.rs` call `root.agent_observer.poll(...)` over the terminals when the Agents list is on screen and `output_state` is on, and OR its result into the returned `changed`; make sure nothing runs (apart from the atomic stamps) when the list is not visible

**Checkpoint**: `cargo test --lib observer`, `cargo test --lib pty` pass; quickstart step 5 works by hand.

---

## Phase 8: User Story 6 - Install, inspect and remove the hooks safely (Priority: P2)

**Goal**: one command sets up everything, nothing the user owns is damaged, and the result is inspectable.

**Independent Test**: run `petruterm hooks setup`, `status`, `setup` again, `remove` against a temporary home with user hooks present (quickstart step 2).

- [X] T028 [US6] Create `tests/hooks_setup.rs` running the real binary with `HOME` set to a temporary directory and a fake `PATH` containing stub `kiro-cli`, `codex`, `agy` executables: user entries in sample `~/.codex/hooks.json` and `~/.gemini/config/hooks.json` survive `setup` and `remove` byte for byte (apart from documented JSON re-formatting, asserted by parsing), second `setup` reports nothing to do and leaves files identical, backups `*.petruterm-bak` exist for pre-existing files, an invalid JSON file is refused unchanged with exit 1 while the other agents still install, `--dry-run` writes nothing, an agent whose binary is missing is skipped with a note, `setup kiro` touches only the Kiro file, `remove` deletes `petruterm.json` and a file that did not exist before setup
- [X] T029 [US6] Make `status` in `src/ipc/hooks_cli.rs` print one line per agent (binary found or not; `not installed`, `installed (N events)`, or `installed, note: <setup_note>`), and add the cases to `tests/hooks_setup.rs` (nothing installed, all installed, Codex shows the trust note, a hand-edited file with half our entries counts as installed with its event count)

**Checkpoint**: `cargo test --test hooks_setup` passes.

---

## Phase 9: User Story 7 - Add a new agent with a small implementation (Priority: P3)

**Goal**: the recipe is real: a fifth agent needs one file and a registry line, and equivalent events look the same on every card.

**Independent Test**: the fake-agent test passes with no source file other than the new one and the registry changed.

- [X] T030 [US7] In `src/ipc/integrations/mod.rs` add a `#[cfg(test)]` fictional `Fake` integration written exactly from the recipe (id `fake`, two events, a tool mapping), run `check_integration` against fixtures in `tests/fixtures/agents/fake/`, and assert it works through `hook()`-level resolution when added to a test registry; verify the recipe text in the module doc matches the steps the test needed (SC-007)
- [X] T031 [US7] In `src/term/attention.rs` tests (or a new `src/ipc/integrations/equivalence.rs` test module) add a test feeding the equivalent events from Claude, Kiro, Codex and Antigravity fixtures (prompt submitted, tool started, finished) through `Message::into_event` and `AttentionTracker::apply` and asserting the resulting `PaneAttention` state, activity shape and `card_style` are identical for all four (SC-003)

**Checkpoint**: `cargo test --lib integrations` passes including the equivalence and fake-agent tests.

---

## Phase 10: Polish and Cross-Cutting

- [X] T032 [P] Update the README Agents section (hooks for Kiro, Codex and Antigravity, `petruterm hooks setup|remove|status`, `agents.output_state`), `.context/architecture/SYSTEM_MAP.md` (the integration registry and the observer data path), and add the session note to `.context/core/SESSION_STATE.md`
- [X] T033 Run `./scripts/ci-local.sh` (check, test, clippy, fmt, audit under `RUSTFLAGS=-D warnings`) and fix everything it reports, including dead-code warnings in the wgpu binary
- [ ] T034 Run the quickstart with real Kiro (V3 mode), Codex and `agy` sessions, fill in every UNVERIFIED row of quickstart step 4 in `research.md` (Kiro prompt field and tool argument names, Codex `hooks.json` wrapper key and tool names, Antigravity interrupt reasons and `Stop` firing, env inheritance), and fix any fixture and mapping that differs; implement research R8 only if env inheritance fails
- [X] T035 [P] Run `graphify update .`

---

## Dependencies and Order

- Phase 1 -> Phase 2 -> Phases 3 to 9 -> Phase 10.
- Phase 2: T003 first; T004 and T005 need T003 (T005 is [P] relative to T004); T006 needs T003 and T005.
- US1 (T007-T009) needs Phase 2 and is the MVP: nothing for another agent works until the CLI resolves through the registry (T009).
- US2, US3, US4 each need US1 and are independent of each other (different files), except that all three edit the registry slice in `src/ipc/integrations/mod.rs` and `tests/hook_cli.rs`: do those edits one after another.
- US5 (T022-T027) is independent of US2 to US4 and of the registry; T023 and T024 are [P] with T022; T025 needs T022 and T023; T026 needs T024 and T025; T027 needs T025.
- US6 needs US2, US3 and US4 (it tests their install side). T029 follows T028 (same file).
- US7 (T030, T031) needs US1 for T030 and US2 to US4 for T031.
- Polish after everything it documents or verifies; T034 needs real agent sessions.

## Parallel Examples

- After T003: T004 and T005 touch different files.
- After US1: US2 (kiro.rs), US3 (codex.rs), US4 (agy.rs) and US5 (pty.rs, observer.rs) can be built in parallel by different workers; only the shared registry line and `tests/hook_cli.rs` edits must be serialized.
- Within US5: T023 (observer.rs) and T024 (config files) are independent of T022 (pty.rs).
- Polish: T032 and T035 are independent.

## Implementation Strategy

- **MVP**: Phases 1, 2 and 3 (User Story 1). It changes no user-visible behavior but makes everything else additive, and its checkpoint proves Claude Code is untouched.
- **Increment 2**: Kiro (US2), then Codex (US3), then Antigravity (US4), validating each with the quickstart before the next. Real-session checks (T034) can start as soon as the first agent lands and will correct fixtures early.
- **Increment 3**: output observer (US5) so every agent, including unintegrated ones, has a state.
- **Increment 4**: setup safety tests (US6), recipe and equivalence tests (US7), polish.
- Stop and run the matching quickstart step after each phase; the gpui-dependent checks need the Metal toolchain.
