---

description: "Task list for agent-to-agent messaging"
---

# Tasks: Agent-to-Agent Messaging

**Input**: Design documents from `/specs/002-agent-messaging/`

**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/, quickstart.md. Builds on `specs/001-agents-sidebar-tab` (custom names, Agents list, scan, rename editor, context menu).

**Tests**: Unit tests for all pure logic and the socket round trip are included (plan.md Testing). UI behavior and the real Claude/Kiro round trip are validated through `quickstart.md`.

**Organization**: Grouped by user story. Foundational pure logic and protocol come first.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: can run in parallel (different files, no dependency on an incomplete task)
- **[Story]**: US1..US6 from spec.md

## Conventions

- Single crate; pure logic in `src/term`, protocol and CLIs in `src/ipc`, gpui-only routing and UI in `src/gpui_shell`.
- Files already over 400 lines (`gpui_shell/poll.rs`, `gpui_shell/render.rs`, `term/mod.rs`, `config/lua.rs`) get hooks of <= ~10 lines only; real logic goes in the new files named in plan.md.
- Never write message text to disk or to logs; log outcomes and counts only (FR-018). Add `// no message text in logs` where a log line sits next to message handling.
- Before editing a file read it; before changing a function grep its callers (AGENTS.md).
- Items only the gpui build uses need `#[allow(dead_code)]` (or `cfg_attr`) so the wgpu binary compiles warning-free, as in spec 001.
- Verify with `./scripts/ci-local.sh` (`RUSTFLAGS=-D warnings`); it needs the Metal toolchain.

---

## Phase 1: Setup

- [X] T001 Create `src/term/messaging.rs` (module doc, empty `Outcome` enum) and the empty submodule files `src/term/messaging/frame.rs`, `src/term/messaging/limits.rs`, `src/term/messaging/decide.rs`; add `pub mod messaging;` to `src/term/mod.rs` (single line)
- [X] T002 [P] Create `src/ipc/msg_bus.rs`, `src/ipc/peer.rs`, `src/ipc/msg_cli.rs` as empty modules with doc comments and add `pub mod msg_bus; pub mod peer; pub mod msg_cli;` to `src/ipc/mod.rs`
- [X] T003 [P] Create `src/gpui_shell/agent_msg.rs` (router skeleton) and `src/gpui_shell/agent_accept.rs`, add `mod agent_msg; mod agent_accept;` to `src/gpui_shell/mod.rs`

---

## Phase 2: Foundational (blocks every user story)

**Purpose**: pure logic, the request/response socket path and the CLI, all unit-testable without the UI.

- [X] T004 Implement in `src/term/messaging.rs`: `Outcome` (all variants from data-model.md, with `RateLimited { retry_after_s }`), `Outcome::result_name()` (the wire `result` strings from contracts/socket-msg-protocol.md), `Outcome::cli_line()` and `Outcome::exit_code()` (exactly the table in contracts/cli-msg.md), `MAX_TEXT_CHARS = 4000`, and `sanitize_text(&str, keep_newlines: bool) -> String` removing C0 (including ESC, DEL, CR), C1 and, unless `keep_newlines`, turning `\n` into a space; unit tests for each outcome mapping and for sanitizing (an `ESC[201~` input yields no ESC, multi-line kept/collapsed, tabs and unicode letters kept)
- [X] T005 [P] Implement in `src/term/messaging/frame.rs`: `Sender` (NamedAgent, UnnamedAgent, Shell, User per data-model.md) and `build_frame(sender, text, reply_expected, paste_on) -> String` producing exactly the variants in contracts/delivered-frame.md (reply command only for a named sender and when `reply_expected`; whole frame collapsed to one line when `!paste_on`); unit tests for every variant and for the collapse
- [X] T006 [P] Implement in `src/term/messaging/limits.rs`: `RateLimiter` (10 accepted per (sender,target) per 60 s, `check(now) -> Result<(), retry_after_s>` and `record`) and `Conversations` (unordered pair, 2-minute gap, 10 cap, `reset_pair`, pruning); all time passed in as `Instant`; unit tests for the window edge, the 11th refusal, retry_after, gap reset and user reset
- [X] T007 Implement in `src/term/messaging/decide.rs`: `decide(&DecideInput) -> Result<Accepted, Outcome>` where the input holds sender kind (agent, shell or user), whether the sender and target resolved, same-terminal flag, target alive flag, target accepting flag, sanitized text, and the two limiter states; checks in this order: empty, too long, sender unknown, self, unknown target, gone, not accepting (skipped for the user), rate limit, conversation limit; unit tests for every outcome and the order, and that the user bypasses acceptance and resets the conversation
- [X] T008 [P] Add the optional `msg: Option<MsgPayload>` field (`to`, `text`, `no_reply`) to `Message` in `src/ipc/mod.rs` with serde default and skip-if-none; tests that a hook message without it parses and serializes unchanged and that a `msg` line parses
- [X] T009 [P] Implement `src/ipc/peer.rs`: `peer_pid(&UnixStream) -> Option<u32>` via `getsockopt(SOL_LOCAL, LOCAL_PEERPID)` (declare the constants with a small `extern "C"` if the `libc` crate lacks them) and `ancestors(pid) -> Vec<u32>` walking parent pids (via `proc_pidinfo`/`ps` table as `workspace_info` already does); tests with a real socket pair (own pid) and with a spawned child (its ancestors include this process)
- [X] T010 Implement `src/ipc/msg_bus.rs`: `MsgRequest { peer_pid, pane_hint, to, text, no_reply, reply: SyncSender<Outcome> }`, a bounded request channel (64), `enable()` (set by the gpui build), `submit(req) -> Outcome` for the server thread (answers `Unsupported` immediately when not enabled, `Timeout` after 2 s), `drain_requests()` for the UI thread; tests with a fake responder thread, the unsupported case and the timeout case
- [X] T011 Route `msg` lines in `handle_connection` in `src/ipc/server.rs`: when a parsed `Message` has `msg`, fill `peer_pid` from `peer.rs`, call `msg_bus::submit`, write one JSON reply line (`{"result": ..., "detail"/"retry_after_s": ...}`) and stop reading that connection; hook messages follow the old path; add tests with the existing `serve` helper (request gets a reply, unsupported reply, garbage ignored, hook lines still queued)
- [X] T012 Implement `src/ipc/msg_cli.rs`: `msg(args) -> i32` for `petruterm msg [--no-reply] <name> <text...|->` (text from args joined by spaces, or stdin for `-`), sends the request to `PETRUTERM_SOCKET`, prints the outcome line and returns the exit code (contracts/cli-msg.md), prints the exit-9 line outside PetruTerm without contacting anything, maps no socket/timeout to exit 13; add `"msg"` to the dispatch in `src/ipc/cli.rs`
- [X] T013 [P] Create `tests/msg_cli.rs`: run the built CLI against a fake socket server for each outcome and exit code, for `-` stdin input, for `--no-reply`, and for the outside-PetruTerm case

**Checkpoint**: `cargo test messaging`, `cargo test ipc` and `cargo test --test msg_cli` pass.

---

## Phase 3: User Story 1 - An agent sends a message to another by name (Priority: P1) MVP

**Goal**: `petruterm msg kterm "..."` run in one agent's pane types the framed message into the other agent and reports the result.

**Independent Test**: quickstart steps 2-3 and 5-6 (needs the US3 toggle, see Dependencies).

- [X] T014 [US1] Add `AgentAccept` to `src/gpui_shell/agent_accept.rs` (`is_accepting(terminal_id)`, `set(terminal_id, pid, on)`, `retain_live(&[AgentDetection])` dropping entries whose process is gone or replaced) with unit tests; add the field to `GpuiShellRoot` in `src/gpui_shell/mod.rs` and initialize it in `src/gpui_shell/construct.rs`
- [X] T015 [US1] Add `AgentMsgState` (a `Conversations` and a map of `RateLimiter` per (sender,target)) to `src/gpui_shell/agent_msg.rs` and a field in `src/gpui_shell/mod.rs` / `construct.rs`
- [X] T016 [US1] Implement sender resolution in `src/gpui_shell/agent_msg.rs`: from `peer_pid` ancestors find the terminal whose `child_pid` is in the chain and require it to match `pane_hint` (pane uid) and to be a live terminal; build the `Sender` (named agent via `AgentNames` + its detection, unnamed agent, or shell with its location from the workspace/tab) or return `SenderUnknown`
- [X] T017 [US1] Implement target resolution and liveness in `src/gpui_shell/agent_msg.rs`: `AgentNames::terminal_for(name)`, terminal exists in `self.terminals`, detection pid matches, and the process is alive via non-blocking `libc::kill(pid, 0)`; fill the `DecideInput` and call `decide`
- [X] T018 [US1] Implement delivery in `src/gpui_shell/agent_msg.rs`: sanitize with `keep_newlines = terminal.bracketed_paste_mode()`, build the frame, write `ESC[200~ + frame + ESC[201~` (or the plain collapsed frame) with `Terminal::write_input`, then a separate `\r` write; record the message in the limiters; reply `Delivered`; log only outcome and length (`// no message text in logs`)
- [X] T019 [US1] Add `tick(&mut GpuiShellRoot) -> bool` in `src/gpui_shell/agent_msg.rs` that drains `msg_bus::drain_requests()` and answers each, call `msg_bus::enable()` once at startup, and add a single call to `tick` in `src/gpui_shell/poll.rs`
- [X] T020 [US1] In `src/gpui_shell/agent_scan.rs` make the scan active whenever messaging is in use (any custom name or acceptance set) regardless of sidebar visibility and window focus, and call `AgentAccept::retain_live` where names are reconciled; add a unit test for the `due`/active rule
- [X] T021 [US1] Mark `AgentNames::terminal_for` as used (remove its `allow(dead_code)`) and confirm `src/gpui_shell/agent_names.rs` still has no other changes needed; run `cargo test` and `cargo check --all-targets` clean

**Checkpoint**: messages route end to end once acceptance can be turned on (US3).

---

## Phase 4: User Story 2 - The receiver can answer back the same way (Priority: P1)

**Goal**: the delivered text identifies the sender and tells the receiver to run the reply command, so a two-agent conversation works.

**Independent Test**: quickstart step 4 (10 repetitions with Claude and Kiro).

- [X] T022 [US2] Pass `no_reply` from the request through `agent_msg.rs` into `build_frame`'s `reply_expected` and add a router-level unit test with a fake sender/target pair covering reply/no-reply and unnamed-sender frames
- [X] T023 [US2] Add a test in `src/gpui_shell/agent_names.rs` (or `agent_msg.rs`) showing that after a sender's name is reset or changed, `terminal_for(old_name)` is `None`, so a reply to the old name resolves as `UnknownName` (spec US2 scenario 4)
- [ ] T024 [US2] Verify the delivered wording against the real agents using the quickstart step 4 method, and adjust the text in `src/term/messaging/frame.rs` and `contracts/delivered-frame.md` together if an agent answers in text instead of running the command

---

## Phase 5: User Story 3 - An agent only receives messages if it opted in (Priority: P2)

**Goal**: acceptance is per agent, default off, toggled from the agent's right-click menu, shown on the row.

**Independent Test**: quickstart steps 2-3 and 9.

- [X] T025 [US3] Add `AgentToggleAccept(usize)` and `AgentSendMessage(usize)` to `ContextAction` in `src/ui/context_menu.rs` (with `#[allow(dead_code)]`) and no-op arms for both in the wgpu match in `src/app/mod.rs`; grep every `match` on `ContextAction` so none is left non-exhaustive
- [X] T026 [US3] Add the menu items "Accept messages" / "Stop accepting messages" and "Send message…" in `src/gpui_shell/agent_menu.rs` in the order given in contracts/agents-menu-messaging.md, and delegate the two new actions from `dispatch_context_action` in `src/gpui_shell/context_menu.rs` to new `dispatch_agent_action` arms (toggle calls `AgentAccept::set` with the row's `agent_pid`; send records a pending compose request)
- [X] T027 [US3] Add `accepting: bool` to `AgentRow` in `src/term/agents.rs` (update the test helper in `src/term/agents/tests.rs`), fill it in `src/gpui_shell/agent_rows.rs`, and render the small `msg` badge after the name in `src/gpui_shell/sidebar/agents.rs`
- [X] T028 [US3] Ensure acceptance ends with the agent: call `AgentAccept::retain_live` from `tick` in `src/gpui_shell/agent_scan.rs` and add a test that a replaced pid clears it; confirm nothing is persisted (no file writes in `agent_accept.rs`)
- [ ] T029 [US3] Validate quickstart steps 2, 3, 9 (with US1 now testable end to end)

---

## Phase 6: User Story 4 - The user sends a message from the Agents list (Priority: P2)

**Goal**: "Send message…" opens an inline editor on the row and sends as the user.

**Independent Test**: quickstart step 8.

- [X] T030 [US4] Create `src/gpui_shell/agent_compose.rs` following `src/gpui_shell/agent_rename.rs`: an editor pinned to `(terminal_id, pid)` with a `TextInput` (placeholder per contracts/agents-menu-messaging.md), Enter validates and sends, refusals keep it open with a one-line reason, Escape or click outside cancels, closes without sending if the agent is gone; add the state field in `src/gpui_shell/mod.rs` / `construct.rs`, a `pending_agent_compose: Option<usize>` consumed in `render()` next to `pending_agent_rename`, and the focus/keyboard guards in `src/gpui_shell/render.rs` and `src/gpui_shell/input.rs` as the rename editor has (hooks only)
- [X] T031 [US4] Add `send_as_user(terminal_id, text) -> Outcome` in `src/gpui_shell/agent_msg.rs` reusing `decide` with `Sender::User` (bypasses acceptance, resets the pair's conversation) and the same delivery path; add a unit test of the decision with a fake input
- [X] T032 [US4] Show the compose editor in the Agents list: pass the compose editor through the existing `AgentEditor` parameter of `render_agents_section` in `src/gpui_shell/sidebar/agents.rs` (only one editor, rename or compose, is open at a time; opening one closes the other) and wire it in `src/gpui_shell/render_sidebar.rs`
- [ ] T033 [US4] Validate quickstart step 8 (user message delivered with acceptance off; empty and over-long input refused inline)

---

## Phase 7: User Story 5 - Loops and floods are stopped (Priority: P2)

**Goal**: rate limit and conversation cap protect against runaway exchanges, with clear refusals.

**Independent Test**: quickstart step 7.

- [X] T034 [US5] Wire `RateLimiter` and `Conversations` from `AgentMsgState` into the `DecideInput` in `src/gpui_shell/agent_msg.rs`, recording only accepted deliveries, returning `retry_after_s` in `Outcome::RateLimited`, and resetting the pair on a user message
- [X] T035 [US5] Prune idle limiter and conversation entries in `tick` in `src/gpui_shell/agent_msg.rs` (entries older than 2 minutes) and add a unit test that state does not grow without bound over many pairs
- [X] T036 [US5] Add an end-to-end style test in `src/gpui_shell/agent_msg.rs` (or `src/term/messaging/decide.rs`) that simulates 12 messages between two terminals: the 11th is `ConversationLimit`, a user message resets it, and 11 messages in a minute from one sender give `RateLimited`
- [ ] T037 [US5] Validate quickstart step 7 with two real agents

---

## Phase 8: User Story 6 - Agents learn how to use messaging (Priority: P2)

**Goal**: `petruterm agents setup` installs standing instructions for Claude Code and Kiro.

**Independent Test**: quickstart steps 4 and 11.

- [X] T038 [P] [US6] Write the two instruction templates `config/default/agent-messaging/claude-skill.md` (SKILL.md with `name: petruterm-messaging` and a `description` saying when it applies) and `config/default/agent-messaging/kiro-steering.md`, both with the content in contracts/agents-setup-cli.md and a `petruterm-managed` marker line
- [X] T039 [US6] Implement `src/ipc/agent_setup.rs`: `setup(opts)` and `remove(opts)` with `--dry-run`, `--claude-skills-dir`, `--kiro-steering-dir`; embeds the templates with `include_str!`, prints what it will do, backs up a different existing file to `<file>.petruterm-bak`, leaves an identical file alone, and removes only files containing the marker; add `"agents"` to the dispatch in `src/ipc/cli.rs`
- [X] T040 [P] [US6] Unit tests in `src/ipc/agent_setup.rs` and integration tests in `tests/msg_cli.rs` using temp directories: fresh install, second run does nothing, existing different file is backed up, `remove` deletes only marked files and restores nothing else, `--dry-run` writes nothing
- [ ] T041 [US6] Validate quickstart steps 4 and 11 with real Claude Code and Kiro, and re-check the Kiro steering caveat (research R11): if global steering does not load in the installed version, note it in research.md and rely on the frame (FR-021)

---

## Phase 9: Polish and cross-cutting

- [X] T042 [P] Security pass: grep the new code for any log, `format!` to a file or `eprintln!` that includes message text (FR-018); confirm the socket still only accepts same-user connections (mode 0600) and that a forged `pane` number is refused (quickstart step 10); record results in research.md
- [X] T043 [P] Confirm no touched file exceeds 400 lines unless it already did (`wc -l` on every changed file), then run `./scripts/ci-local.sh`
- [ ] T044 [P] Validate quickstart steps 1, 5, 6, 9, 10, 12 (idle cost: no `ps` with nothing named or accepting)
- [X] T045 [P] Update `CHANGELOG.md` (Unreleased: messaging, `petruterm msg`, `petruterm agents setup`), `.context/architecture/SYSTEM_MAP.md` and `.context/core/SESSION_STATE.md`, and re-validate `specs/002-agent-messaging/checklists/requirements.md`
- [X] T046 [P] Run `graphify update .`

---

## Dependencies & Execution Order

- Phase 1 -> Phase 2 -> user stories -> Polish.
- Phase 2 internal order: T004 -> T005/T006/T007 (T007 needs T004, T005 not required); T008 -> T011; T009 -> T011; T010 -> T011 -> T012 -> T013. T005, T006, T008, T009 can run in parallel.
- US1 (T014-T021) depends on Phase 2. US1's manual validation needs acceptance to be switchable, which is US3's menu item (T025-T027), so US3 is implemented right after US1 and both are validated together at T029.
- US2 depends on US1 (router) and T005 (frame). US4 depends on US1 and on the editor pattern from spec 001 (`agent_rename.rs`); US3's T026 adds its menu entry. US5 depends on T006 and US1's `decide` wiring. US6 is independent of US1-US5 (only needs `cli.rs` dispatch) and can run in parallel after Phase 2.
- Shared-file ordering: `agent_msg.rs` (T015-T019, T022, T031, T034, T035, T036), `agent_menu.rs`/`context_menu.rs` (T026), `agents.rs` (T027, T032) and `mod.rs`/`construct.rs` are touched by several tasks; serialize those edits.

## Parallel Opportunities

- Phase 1: T002 and T003 together.
- Phase 2: T005, T006, T008, T009 together; T013 alongside T012.
- After Phase 2: US6 (T038-T041) in parallel with US1.
- Polish: T042-T046 together.

## Implementation Strategy

1. MVP = Phases 1-2, US1, US3 and US2 (T001-T029): routing, acceptance, the round trip.
2. Then US4 (user can send from the menu), US5 (limits are already unit-tested in Phase 2; this wires and validates them) and US6 (instructions).
3. Run the quickstart with real Claude Code and Kiro after the MVP before polishing; if the delivered wording needs a change it is a one-file edit (T024).
4. Finish with Polish. Commits follow `[TASK_ID] type: Message.`
