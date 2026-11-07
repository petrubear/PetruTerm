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
