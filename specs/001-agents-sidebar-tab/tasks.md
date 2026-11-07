---

description: "Task list for the Agents sidebar tab"
---

# Tasks: Agents Sidebar Tab

**Input**: Design documents from `/specs/001-agents-sidebar-tab/`

**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/, quickstart.md

**Tests**: Unit tests are included for the pure logic in `src/term/agents.rs` and the sidebar section cycling, as stated in plan.md (Testing). UI behavior is validated through `quickstart.md`.

**Organization**: Grouped by user story. Foundational detection comes first because every story depends on it.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies on incomplete tasks)
- **[Story]**: US1..US4 from spec.md

## Conventions

- Single crate; shared logic in `src/term`, gpui-only glue in `src/gpui_shell` (plan.md Structure Decision).
- Keep each file under 400 lines (constitution VII). Files already over 400 lines (`gpui_shell/poll.rs`, `term/attention.rs`, `term/mod.rs`, `config/lua.rs`, `config/mod.rs`, `gpui_shell/workspace_info.rs`) get single-call hooks of at most ~10 lines only; real logic goes in the new files named in plan.md. Do NOT add code to `src/gpui_shell/workspace_info.rs` or `src/gpui_shell/sidebar/sections.rs`.
- Never log, store or display the `args` column of `ps` (data-model.md).
- Prerequisite: `src/gpui_shell/poll.rs` and `workspace_info.rs` carry uncommitted user edits at planning time. Commit or stash them, and create branch `001-agents-sidebar-tab`, before starting T013.
- Before editing any file, read it and grep for callers of anything you change (AGENTS.md).
- Verify with `./scripts/ci-local.sh` (runs under `RUSTFLAGS=-D warnings`).

---

## Phase 1: Setup

**Purpose**: Create module skeletons so later tasks compile incrementally.

- [X] T001 Create `src/term/agents.rs` with module doc comment and empty public items listed in data-model.md (`RecognizedAgent`, `ProcessRow`, `PaneProbe`, `AgentDetection`); add the single line `pub mod agents;` to `src/term/mod.rs`
- [X] T002 [P] Create `src/gpui_shell/agent_scan.rs` skeleton (static result slot, `AgentScanState` struct, empty `drain`/`due`/`start`) and add `mod agent_scan;` in `src/gpui_shell/mod.rs`
- [X] T003 [P] Create `src/gpui_shell/sidebar/agents.rs` with a stub `render_agents_section` and `pub mod agents;` in `src/gpui_shell/sidebar/mod.rs`

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Process scan, agent matching and the new section variant. No user story can work without these.

- [X] T004 Implement `parse_ps_rows(text) -> Vec<ProcessRow>` in `src/term/agents.rs` for `ps -axo pid=,ppid=,pgid=,tpgid=,comm=,args=` output, skipping malformed lines (R1)
- [X] T005 Implement `match_agent(row, &[RecognizedAgent]) -> Option<&RecognizedAgent>` in `src/term/agents.rs`: basename of `comm`, and for `node`/`bun`/`deno`/`python*` the basename (extension stripped) of the first non-flag script arg (R2)
- [X] T006 Implement `default_agents() -> Vec<RecognizedAgent>` in `src/term/agents.rs` with Claude Code (`claude`), Kiro (`kiro-cli`, `kiro`), Codex (`codex`), Copilot (`copilot`), Antigravity (`agy`, `antigravity`) (R3); verify each name against the installed binary with `ps` where available and note the result in research.md R3
- [X] T007 Implement `detect(rows, probes, agents) -> Vec<AgentDetection>` in `src/term/agents.rs`: per pane, candidates are rows with `pgid == tpgid` of the shell row plus direct children of the shell; first match wins; at most one detection per terminal (R1, clarification Q4)
- [X] T008 [P] Unit tests in `src/term/agents.rs` (`#[cfg(test)]`): ps parsing incl. malformed lines and a check that `AgentDetection` carries no `args`; node-script matching (`node /x/claude.js`); direct match; no match for plain `zsh`; backgrounded process (pgid != tpgid) ignored; process deeper than direct child ignored; two panes yield two detections
- [X] T009 Implement background scan in `src/gpui_shell/agent_scan.rs`: `start(probes, agents)` spawns a named thread running one `ps` and storing `Vec<AgentDetection>` in the static slot; `drain()` returns new results; `due(active, now)` true when `active` and 2 s elapsed and none in flight (stuck guard like `workspace_info`) (R5)
- [X] T010 Add `SidebarSection::Agents` to `src/gpui_shell/sidebar/mod.rs`, extend `next()`/`prev()` to the order Workspaces, Mcp, Skills, Steering, Agents, add `agents_cursor` with getter/setter, and update the existing section-cycling tests
- [X] T011 Add `AgentScanState` field to `GpuiShellRoot` in `src/gpui_shell/mod.rs` and initialize it in `src/gpui_shell/construct.rs`

**Checkpoint**: `cargo test agents` and `cargo check` pass.

---

## Phase 3: User Story 1 - See all running agents in one place (Priority: P1) MVP

**Goal**: An Agents section lists one row per detected agent across all workspaces/tabs with agent name, `workspace / tab`, and state (initially "running").

**Independent Test**: quickstart steps 1, 4, 5 (rows appear, disappear, plain shell excluded) plus empty state.

- [X] T012 [US1] Add `agent_probes()` in `src/gpui_shell/attention.rs` returning `Vec<PaneProbe>` (terminal id + `Terminal.child_pid`) for every live terminal in every workspace
- [X] T013 [US1] Implement `tick(&mut GpuiShellRoot) -> bool` in `src/gpui_shell/agent_scan.rs` (drain results into `AgentScanState`, start a scan when sidebar visible + `SidebarSection::Agents` active + window focused + `due`, return true when results changed) and add a single call to it in `src/gpui_shell/poll.rs`
- [X] T014 [US1] Trigger an immediate scan when the Agents section is selected or the sidebar opens: mark `AgentScanState` due in `on_select_section` in `src/gpui_shell/render_sidebar.rs` and in the `LeaderAction::ToggleWorkspaceSidebar` arm of `src/gpui_shell/leader_dispatch.rs` (research R5)
- [X] T015 [US1] Add `agent_rows()` in `src/gpui_shell/attention.rs` building `AgentRow` (data-model.md, including `pane_number`) from detections, workspace/tab names and tracker state (`Running` when no state reported); drop detections whose terminal no longer exists (clarification Q2)
- [X] T016 [US1] Implement `render_agents_section` in `src/gpui_shell/sidebar/agents.rs`: scrollable list, line 1 = state dot + agent name + `workspace / tab` truncated (+ ` #N` pane number when the tab has 2+ panes), empty state "No agents running" (contracts/sidebar-agents-ui.md)
- [X] T017 [US1] Add the "Agents" label to `render_section_tabs` and dispatch to `render_agents_section` in `src/gpui_shell/sidebar/render.rs`; extend `SidebarRenderCx` with `agent_rows`, `agents_cursor`, `on_open_agent`; pass them from `src/gpui_shell/render_sidebar.rs`
- [X] T018 [US1] Raise `MIN_SIDEBAR_WIDTH_PX` in `src/gpui_shell/sidebar/render.rs` so all five labels fit (target 220; verify visually at minimum width, research R8)
- [ ] T019 [US1] Validate quickstart steps 1, 4, 5, 9; fix issues found

**Checkpoint**: MVP. The list shows and updates; jumping is not yet wired.

---

## Phase 4: User Story 2 - Jump to an agent's pane (Priority: P1)

**Goal**: Click or Enter on a row focuses that agent's workspace, tab and pane.

**Independent Test**: quickstart step 2; closed-pane row removed without error.

- [X] T020 [US2] Make `focus_terminal` in `src/gpui_shell/attention.rs` `pub(super)` (research R9); grep callers to confirm no behavior change
- [X] T021 [US2] Add `sidebar_open_agent_at(idx)` in `src/gpui_shell/sidebar_nav.rs` that resolves the row at `idx` from `agent_rows()` and calls `focus_terminal`; if it returns false, mark the scan due so the stale row disappears
- [X] T022 [US2] Extend `sidebar_move_cursor` (wrap over `agent_rows().len()`) and `sidebar_activate_selection` in `src/gpui_shell/sidebar_nav.rs` for `SidebarSection::Agents`
- [X] T023 [US2] Build the `on_open_agent` click callback in `src/gpui_shell/render_sidebar.rs` (set cursor, jump, `cx.notify()`) and attach it to rows in `src/gpui_shell/sidebar/agents.rs`; highlight the cursor row
- [ ] T024 [US2] Validate quickstart step 2 and the closed-pane edge case

---

## Phase 5: User Story 3 - Know which agent needs me (Priority: P2)

**Goal**: Rows show reported state, unread indicator, last message and elapsed time, ordered by urgency.

**Independent Test**: quickstart step 3.

- [X] T025 [US3] Add a minimal `updated_at()` accessor (field set where `last_event_at` is set; at most ~6 lines) to `PaneAttention` in `src/term/attention.rs`, with a test in the existing attention tests (research R6)
- [X] T026 [US3] Implement `sort_rows` ordering in `src/term/agents.rs` (NeedsInput, Working, Done, Idle, Running; ties by most recent `updated_at`, then stable workspace/tab/pane order) with unit tests; call it from `agent_rows()` in `src/gpui_shell/attention.rs`
- [X] T027 [US3] Extend `AgentRow` population in `agent_rows()` with unread (respecting `config.agents.ring`), single-line sanitized message, and elapsed time
- [X] T028 [US3] Render line 2 (message) and relative time ("12s", "3m", "1h"), accent dot for unread/needs-input, muted dot for running, in `src/gpui_shell/sidebar/agents.rs`; add a unit test for the relative-time formatter
- [ ] T029 [US3] Validate quickstart step 3 and that focusing a pane clears its unread indication

---

## Phase 6: User Story 4 - Agents are found without setup (Priority: P2)

**Goal**: The recognized list is user-extendable via Lua; agents that never report show "running"; trust rules hold.

**Independent Test**: quickstart steps 6 and 7.

- [X] T030 [US4] Add the `recognized: Vec<RecognizedAgent>` field (default empty, serde-compatible) to `AgentsConfig` in `src/config/schema.rs`; derive Deserialize/Clone on `RecognizedAgent` in `src/term/agents.rs`
- [X] T031 [US4] Create `src/config/agents_cfg.rs` that parses and validates `agents.recognized` from the Lua table (skip invalid entries with a logged warning) with its own `#[cfg(test)]` tests for valid, invalid and absent key; register it in `src/config/mod.rs` (`mod agents_cfg;`) and add one call from `src/config/lua.rs`
- [X] T032 [US4] Implement `merged_agents(defaults, user)` in `src/term/agents.rs` (user entries appended; same `name` replaces) with unit tests; use it when `tick` in `src/gpui_shell/agent_scan.rs` starts a scan
- [X] T033 [P] [US4] Document `agents.recognized` with an example in `config/default/notifications.lua` (contracts/config-agents-recognized.md)
- [X] T034 [US4] For any agent whose binary was not installed during T006 (likely Copilot and Antigravity), confirm its real process name with `ps` while running it, correct `default_agents()` in `src/term/agents.rs`, and record the result in research.md R3
- [ ] T035 [US4] Validate quickstart steps 6 and 7 (fake `notify` creates no row; custom `sleep` agent appears after reload)

---

## Phase 7: Polish & Cross-Cutting

- [ ] T036 [P] Verify zero scan cost when closed: confirm no `ps` spawn with the sidebar closed or another section active; validate quickstart step 8 (20 agent panes, no frame-rate change)
- [ ] T037 [P] Confirm existing attention behaviors unchanged (tab dots, pane rings, notifications, jump-to-unread) per FR-016
- [X] T038 Confirm no file touched by this feature grew past 400 lines unless it was already over (`wc -l` on every changed file), then run `./scripts/ci-local.sh` and fix any check/test/clippy/fmt/audit findings
- [X] T039 [P] Update `.context/architecture/SYSTEM_MAP.md` and `.context/core/SESSION_STATE.md`; add a short note to `docs/superpowers/specs/2026-10-15-agent-attention-design.md` linking this feature
- [X] T040 Run `graphify update .` to refresh the knowledge graph

---

## Dependencies & Execution Order

- Phase 1 -> Phase 2 -> user stories -> Polish.
- US1 depends on Phase 2 only. US2 depends on US1 (rows exist to click). US3 depends on US1 (extends rows). US4 depends on Phase 2 (config merge feeds the scan) and is independent of US2/US3.
- Within Phase 2: T004 -> T005 -> T007; T006 any time after T001; T008 after T007; T009 after T001/T002; T010, T011 independent of T004-T009.
- Within US1: T012 -> T013/T014/T015 -> T016 -> T017 -> T018 -> T019.

## Parallel Opportunities

- Phase 1: T002 and T003 together.
- Phase 2: T008 (tests) alongside T009 (scan bridge) and T010 (section variant) once T007 is done.
- US3 and US4 can proceed in parallel after US1 (different files: `attention.rs`/`agents.rs` vs `schema.rs`/`lua.rs`).
- Polish: T036, T037, T039 in parallel.

## Implementation Strategy

1. MVP = Phases 1-3 (US1): detection plus a visible, live list.
2. Add US2 (jump) next; it makes the list actionable.
3. Add US3 (triage) and US4 (config extensibility) in either order.
4. Finish with Polish; run `./scripts/ci-local.sh` before committing. Commits follow `[TASK_ID] type: Message.`

---

# Amendment 2026-11-06: session id (US5) and custom names (US6)

Design: `plan.md` (Amendment), `research.md` R10-R15, `data-model.md`, `contracts/ipc-message-session.md`, `contracts/agent-context-menu.md`. Same conventions as above: files already over 400 lines (`poll.rs`, `term/mod.rs`, `config/lua.rs`) get hooks of <= ~10 lines; `term/agents.rs` (269 lines) must not grow past 400, so new helpers go into submodules; never log or store `args`.

## Phase 8: Foundational for the amendment

- [ ] T041 Kiro session-source spike (needs the user's permission first, because it reads under `~/.kiro`): with a Kiro session running, list file names under `~/.kiro` and read only small lock/metadata files to confirm which layout is real (SQLite only, or `sessions/<hash>/sess_<uuid>/` with a `.lock` holding the pid). Also check whether a Kiro hook payload exposes a session id. Record the result in research.md R11 and decide whether T050 is built or marked N/A
- [X] T042 [P] Create `src/term/agents/names.rs` with `validate_name(&str) -> Result<String, NameError>` (trim, 1-32 chars, `[A-Za-z0-9._-]`), `NameError` with user-facing messages, `sanitize_session(&str) -> Option<String>` (`[A-Za-z0-9_-]`, 1-64 chars) and `short_session(&str) -> &str` (first 8 chars), with unit tests; add `mod names; pub use names::*;` to `src/term/agents.rs`
- [X] T043 [P] Create `src/term/agents/session.rs` with `session_from_args(args: &str) -> Option<String>` recognizing `--resume <id>`, `-r <id>`, `--resume-id <id>`, `--session-id <id>` (and `=` forms), passing the result through `sanitize_session`, with unit tests (including a flag with no value, and a path-like value that is rejected); add `mod session;` to `src/term/agents.rs`
- [X] T044 Extend `AgentDetection` in `src/term/agents.rs` with `pid: u32` (the matched process) and `session: Option<String>` (from `session_from_args`, args themselves still never kept); update `detect()` and the existing tests in `src/term/agents/tests.rs`
- [X] T045 Add the optional `session: Option<String>` field to `ipc::Message` in `src/ipc/mod.rs` (serde default and skip-if-none), sanitized with `sanitize_session` on receipt via a `Message::session()` accessor; unit tests for absent, valid, invalid and oversized values, and that a message with only `pane` and `session` yields no attention event
- [X] T046 In `src/ipc/claude_hooks.rs` copy the payload's `session_id` into `Message.session` and add `SessionStart` (session only, no state change); add `SessionStart` to the hook set in `scripts/shims/claude` (the static settings JSON) and to the list used by `petruterm hooks setup claude`; update the tests there

**Checkpoint**: `cargo test agents` and `cargo test ipc` pass.

## Phase 9: User Story 5 - Session id on the row (Priority: P2)

**Goal**: A row shows the first 8 characters of the agent's session id when known; nothing otherwise.

**Independent Test**: quickstart steps 10-12.

- [X] T047 [US5] Create `src/gpui_shell/agent_sessions.rs` with `AgentSessions` (`note(terminal_id, pid, id)`, `get(terminal_id, pid)`, `retain_live(&[AgentDetection])`) implementing research R12 (latest report wins; dropped on pid change or row removal), with unit tests; add `mod agent_sessions;` and a field in `src/gpui_shell/mod.rs`, initialized in `src/gpui_shell/construct.rs`
- [X] T048 [US5] In the control-socket drain loop in `src/gpui_shell/poll.rs`, read `msg.session()` before `into_event`, resolve the pane uid to a terminal id as the loop already does, and call `AgentSessions::note` with the current detection's pid (<= 5 lines; if more is needed, put it in a function in `agent_sessions.rs`)
- [X] T049 [US5] Call `AgentSessions::retain_live` from `tick` in `src/gpui_shell/agent_scan.rs` whenever a scan result is drained
- [X] T050 [US5] **N/A for now** (T041 not run, no permission to inspect `~/.kiro`; Kiro shows an id only from `--resume-id`). Only if T041 confirms a usable Kiro store: create `src/gpui_shell/session_lookup.rs` (`lookup(pid) -> Option<String>`, read-only, tolerant of missing/unreadable/unknown formats, never reads conversation files) and call it from the scan thread in `src/gpui_shell/agent_scan.rs` to fill `AgentDetection.session` for Kiro; otherwise mark this task N/A in tasks.md with the reason
- [X] T051 [US5] Add `session: Option<String>` and `agent_pid: u32` to `AgentRow` in `src/term/agents.rs`; fill them in `src/gpui_shell/agent_rows.rs` (reported id if present, else the detection's `session`); update the row helper in `src/term/agents/tests.rs`
- [X] T052 [US5] Render the muted 8-character id after the agent name in `src/gpui_shell/sidebar/agents.rs` (nothing when unknown); keep the line truncating cleanly at the minimum sidebar width
- [ ] T053 [US5] Validate quickstart steps 10-12 and record the Kiro outcome in research.md R11

## Phase 10: User Story 6 - Rename agents from a right-click menu (Priority: P2)

**Goal**: Right-click a row to Rename / Copy session id / Reset name; custom names are unique, address-safe and live with the pane.

**Independent Test**: quickstart steps 13-17.

- [X] T054 [US6] Add `AgentRename(usize)`, `AgentCopySessionId(usize)` and `AgentResetName(usize)` to `ContextAction` in `src/ui/context_menu.rs`; add no-op arms for them in the wgpu match in `src/app/mod.rs` (that binary has no Agents list); grep every `match` on `ContextAction` so none is left non-exhaustive
- [X] T055 [US6] Create `src/gpui_shell/agent_names.rs` with `AgentNames` (`set(terminal_id, pid, name) -> Result<(), NameError>` checking case-insensitive uniqueness against other live entries, `get`, `reset`, `retain_live(&[AgentDetection])`, `terminal_for(name)`, `name_of(terminal_id)`) with unit tests for duplicates, case-insensitivity, pid change clearing the name, and lookups; add the module and a field in `src/gpui_shell/mod.rs`, initialized in `src/gpui_shell/construct.rs`; call `retain_live` from `tick` in `src/gpui_shell/agent_scan.rs`
- [X] T056 [US6] Create `src/gpui_shell/agent_menu.rs` that builds the 3-item menu for a row (Copy session id only with an id, Reset name only with a name, per contracts/agent-context-menu.md) and opens it at the pointer through the existing `ContextMenu`; add a delegating arm for the three new actions in `dispatch_context_action` in `src/gpui_shell/context_menu.rs` (copy via `cx.write_to_clipboard`, reset via `AgentNames::reset`, rename via T057)
- [X] T057 [US6] Create `src/gpui_shell/agent_rename.rs` following `src/gpui_shell/rename.rs`: a `TextInput` editor pinned to a terminal id, Enter validates with `validate_name` then `AgentNames::set`; on error keep the editor open and store the message for display, Escape and click-outside cancel, the editor closes if the row disappears; add the state fields in `src/gpui_shell/mod.rs` / `construct.rs`
- [X] T058 [US6] In `src/gpui_shell/sidebar/agents.rs` add the right-click handler (`MouseButton::Right`, passes terminal id and pointer position), render the custom name prominently with the agent name muted next to it, render the inline editor and the one-line error under the row; thread `custom_name`, the editor element and the new callback through `SidebarRenderCx` in `src/gpui_shell/sidebar/render.rs` and `src/gpui_shell/render_sidebar.rs`
- [X] T059 [US6] Add `custom_name: Option<String>` to `AgentRow` in `src/term/agents.rs` and fill it in `src/gpui_shell/agent_rows.rs`; make sure sorting and jump still use the terminal id only
- [ ] T060 [US6] Validate quickstart steps 13-17 (menu items, valid/invalid/duplicate names, cancel paths, name cleared when a different agent takes the pane, nothing after restart)

## Phase 11: Polish for the amendment

- [X] T061 [P] Confirm no touched file exceeds 400 lines unless it already did (`wc -l` on every changed file, especially `src/term/agents.rs`), then run `./scripts/ci-local.sh`
- [X] T062 [P] Update `CHANGELOG.md` (Unreleased: session id and rename), `contracts/config-agents-recognized.md` untouched, and `specs/001-agents-sidebar-tab/checklists/requirements.md` re-validated against the amended spec
- [X] T063 [P] Run `graphify update .`

## Amendment dependencies

- T042, T043 (parallel) -> T044 -> T051. T045 -> T046 -> T048. T041 gates only T050.
- US5: T047 -> T048/T049 -> T051 -> T052 -> T053. T050 independent once T041 is answered.
- US6 depends on Phase 8 (T042 for `validate_name`) and on T044 (`pid` on detections) but not on US5; T054 -> T056; T055 -> T056/T057; T057 -> T058; T059 after T055.
- US5 and US6 can proceed in parallel after Phase 8 (they touch different files except `agents/agents.rs`, `agent_rows.rs` and `mod.rs`/`construct.rs`, so serialize those edits).

## Amendment implementation strategy

1. Phase 8 first (pure logic and the socket field are fully unit-testable).
2. Ask for permission and run the Kiro spike (T041) early; it decides whether Kiro shows ids and can run while other work proceeds.
3. Deliver US6 (rename) first if you want the naming for the later bus soonest; it does not depend on session ids.
