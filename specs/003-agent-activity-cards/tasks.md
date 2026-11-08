---

description: "Task list for the agent activity line and state-colored cards"
---

# Tasks: Agent Activity Line and State-Colored Cards

**Input**: Design documents from `/specs/003-agent-activity-cards/`

**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/, quickstart.md. Builds on `specs/001-agents-sidebar-tab` (Agents list, scan, rows) and the Claude Code shim and hooks (`scripts/shims/claude`, `src/ipc/claude_hooks.rs`).

**Tests**: Unit tests for all pure logic, the hook-to-socket round trip and config parsing are included (FR-012, plan.md Testing). Visual behavior and the real Claude Code run are validated through `quickstart.md`.

**Organization**: Grouped by user story. The shared data path (cleaning, config, tracker, wire field) comes first because every story needs it.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: can run in parallel (different files, no dependency on an incomplete task)
- **[Story]**: US1..US3 from spec.md

## Conventions

- Single crate; pure logic in `src/term` and `src/ipc` (shared by both binaries), rendering only in `src/gpui_shell`.
- `src/term/attention.rs` (485 lines with tests), `src/config/lua.rs` and `src/gpui_shell/poll.rs` are over 400 lines already: add only the small edits named here; real logic goes in the new files (`activity.rs`, `agent_card.rs`).
- Text is counted in characters, never bytes, and never cut inside a character.
- Never forward activity to `remote::notify_attention` or native notifications (FR-009); the event's `message` and the new `activity` stay separate fields.
- Before editing a file read it; before changing a function or struct, grep its callers (AGENTS.md). The `AttentionEvent::State` change touches every construction site: `grep -rn "AttentionEvent::State" src tests`.
- Items only the gpui build uses need `#[allow(dead_code)]` (or `cfg_attr`) so the wgpu binary compiles warning-free, as in specs 001 and 002.
- Verify with `./scripts/ci-local.sh` (`RUSTFLAGS=-D warnings`); it needs the Metal toolchain (`xcodebuild -downloadComponent MetalToolchain`). `cargo test --lib <filter>` works on a cached build without it.

---

## Phase 1: Setup

- [X] T001 Create `src/term/agents/activity.rs` (module doc, `pub const MAX_ACTIVITY_CHARS: usize = 80;`) and add `mod activity;` and `pub use activity::*;` next to the existing `mod names;` / `pub use names::*;` lines in `src/term/agents.rs`
- [X] T002 [P] Create `src/gpui_shell/sidebar/agent_card.rs` (module doc only) and add `pub mod agent_card;` to `src/gpui_shell/sidebar/mod.rs`

---

## Phase 2: Foundational (blocks every user story)

**Purpose**: cleaning, config, tracker state and the wire field, all unit-testable without the UI.

- [X] T003 Implement in `src/term/agents/activity.rs`: `clean_activity(&str) -> Option<String>` that turns every control character and whitespace run into one space, trims, caps at `MAX_ACTIVITY_CHARS` characters replacing the tail with `...` (so the result is at most 80 characters including the dots), and returns `None` when the result is empty; unit tests for newline and tab collapse, ESC sequences removed, exactly 80 and 81 characters, multi-byte characters (accents, CJK, emoji) never split, empty and whitespace-only input
- [X] T004 In `src/term/agents/activity.rs` add `#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)] #[serde(rename_all = "lowercase")] pub enum ActivityDetail { #[default] Full, Tool, Off }` and `apply_detail(ActivityDetail, &str) -> Option<String>` (`Full` returns the text, `Tool` returns the part before the first `": "` or the whole text when there is none, `Off` returns `None`); unit tests for all three levels and for text without a colon
- [X] T005 Add `pub activity: crate::term::agents::ActivityDetail` to `AgentsConfig` in `src/config/schema.rs` with `#[serde(default)]` and `activity: ActivityDetail::Full` in its `Default` impl; parse `agents.activity` in the `agents` table of `src/config/lua.rs` next to `bell_attention` (string `"full"|"tool"|"off"`, unknown or missing value keeps `Full`); document the key (values, default, and the note that commands and prompts can contain secrets) in `config/default/notifications.lua`; extend the config test near `bell_attention` in `src/config/mod.rs` for default, each value, and an unknown value
- [X] T006 [P] In `src/term/attention.rs`: add `activity: Option<String>` to `AttentionEvent::State` and to `PaneAttention` (init `None`); in `apply_state` set it when `state == Working && activity.is_some()`, keep it on a `Working` event without activity, and clear it for every other state; clear it in the `AttentionEvent::Notify`/`Bell` branch of `apply` (state becomes needs-input); fix every construction site of `AttentionEvent::State` found by grep (ipc/mod.rs and attention.rs tests; patterns using `..` need no change); add `pub fn activity(&self, terminal_id: usize) -> Option<&str>` for the row builder; tests: set, replace, kept by a working event without activity, cleared by each of needs-input/done/idle and by a notification, dropped by `remove`, and `message_for`/`last_message` never contain it
- [X] T007 In `src/ipc/mod.rs`: add `activity: Option<String>` to `Message` with `#[serde(default, skip_serializing_if = "Option::is_none")]`; in `into_event` pass `self.activity.as_deref().and_then(clean_activity)` into the `State` event (ignored for state-less messages); tests: a line without the field parses and serializes unchanged, a line with it maps to the event, a `needs-input` message still carries its own `message`, control characters and 5000-character input are cleaned and capped, an unknown state still returns `None`

**Checkpoint**: `cargo test --lib activity`, `cargo test --lib attention` and `cargo test --lib ipc` pass.

---

## Phase 3: User Story 1 - See what an agent is doing right now (Priority: P1) MVP

**Goal**: a working Claude Code row shows its current action instead of the bare word "working".

**Independent Test**: Run Claude Code in a pane, ask it to run a command and read a file; the row detail line changes through the prompt start, the tool and its target, and clears when Claude finishes (quickstart step 3).

- [X] T008 [US1] In `src/term/agents/activity.rs` implement `tool_activity(tool_name: &str, tool_input: &serde_json::Value, cwd: Option<&str>) -> String` following the table in research.md R1 (Bash `command`; Edit/Write/Read/MultiEdit/NotebookEdit `file_path`; Grep/Glob `pattern`; WebFetch `url`; WebSearch `query`; Agent/Task `description`; anything else or a missing/non-string field gives the tool name alone), output `"{tool}: {target}"` passed through `clean_activity`; strip the `cwd` prefix (and a following `/`) from `file_path` values; also `prompt_activity(prompt: &str) -> Option<String>` returning `"Prompt: {first line}"` cleaned; table-driven unit tests for every tool, a path under and outside `cwd`, a missing field, a non-string field, an MCP tool name (`mcp__x__y`), an empty prompt, and a multi-line prompt
- [X] T009 [US1] In `src/ipc/claude_hooks.rs` extend `message_for`: for `PreToolUse` read `tool_name`, `tool_input` and `cwd` from the payload and set `activity` via `tool_activity`; for `UserPromptSubmit` read `prompt` and set `activity` via `prompt_activity`; every other event keeps `activity: None`; a missing `tool_name` gives no activity; tests: Bash, Edit and unknown-tool payloads, a prompt payload, `Stop` and `Notification` carry no activity, and the existing `event_mapping` / `session_id` tests still pass
- [X] T010 [P] [US1] Add `pub activity: Option<String>` to `AgentRow` in `src/term/agents.rs` (doc: already filtered by the detail level; `None` when off, not working, or never reported) and add `activity: None` to the `agent_row` literal in `src/term/agents/tests.rs`
- [X] T011 [US1] In `src/gpui_shell/agent_rows.rs` fill `AgentRow.activity`: only when the tracked state is `Working`, take `self.attention.activity(det.terminal_id)` and pass it through `apply_detail(self.config.agents.activity, text)`; no other field changes
- [X] T012 [US1] In `src/gpui_shell/sidebar/agents.rs` extract the detail text into `fn detail_text(row: &AgentRow) -> String` (working with activity returns the activity text alone; every other case returns exactly what the inline code produces today) and use it for `detail`; add a `#[cfg(test)] mod tests` covering working with and without activity, needs-input with a message, done, idle and unreported; keep the line single-line with the existing `truncate()` so the card height never changes (FR-006)
- [X] T013 [P] [US1] In `tests/hook_cli.rs` add a test that runs `hook claude PreToolUse` with a Bash payload (`tool_name`, `tool_input.command`, `cwd`) and asserts the socket line has `state: "working"` and `activity: "Bash: <command>"`, a second test for `UserPromptSubmit` (`activity` starts with `Prompt: `), and a third that `Stop` carries no `activity` key

**Checkpoint**: Claude Code rows show the live activity; `cargo test --lib agents`, `cargo test --lib claude_hooks` and `cargo test --test hook_cli` pass; quickstart steps 3, 4 and 6 work.

---

## Phase 4: User Story 2 - Tell agent states apart at a glance (Priority: P1)

**Goal**: each card is tinted and edged by its state.

**Independent Test**: With agents in four different states, the four cards look different in the dark theme and in `petru-theme-light`, the keyboard cursor stays visible, and no card changes height (quickstart step 5).

- [X] T014 [P] [US2] In `src/gpui_shell/sidebar/agent_card.rs` implement `CardStyle { tint: Option<[f32; 4]>, edge: Option<[f32; 4]> }` and `card_style(state: Option<AgentState>, colors: &ColorScheme) -> CardStyle` per data-model.md (Working: `ui_accent`, NeedsInput: `ATTENTION_COLOR`, Done: `ui_success`, Idle/`None`: no tint and no edge; tint alpha `0.12`, edge alpha `1.0`); unit tests: each state's colors, idle and unreported have none, the tint alpha is below `0.2`, the edge is opaque, and for the dark and light default themes (`petru-theme-dark`, `petru-theme-light` values) the tint lowers the contrast of `ui_muted` text against the row background by no more than 10% compared with the untinted row (research R7)
- [X] T015 [US2] In `src/gpui_shell/sidebar/agents.rs` apply `card_style` to each row: always reserve a 3 px left edge (`border_l`, transparent when `edge` is `None`) so text never shifts between states, draw the tint as the row background when present, keep `ui_surface_active` as the background for the cursor row (the edge stays drawn), and leave the 6 px dot, unread logic, badges and click handlers untouched; read an existing `border_`/`bg(` use in `src/gpui_shell` for the exact gpui calls first

**Checkpoint**: quickstart step 5 passes by eye in both themes.

---

## Phase 5: User Story 3 - Other agents can feed the same line (Priority: P3)

**Goal**: scripts and future integrations can set the activity through `petruterm notify`.

**Independent Test**: In a pane with a detected agent run `petruterm notify --state working --activity "Indexing repo"`; the row shows the text, and `--state idle` clears it (quickstart step 7).

- [X] T016 [US3] In `src/ipc/cli.rs` add `"--activity" => &mut msg.activity` to the flag match in `notify`, update the usage line and the module doc comment to `notify [--state S] [--title T] [--body B] [--activity A]`; keep exit codes (2 for unknown argument or missing value); extend `notify_validates_arguments` (a missing `--activity` value exits 2, `--activity` alone without state or body still prints usage and exits 2)
- [X] T017 [US3] In `tests/hook_cli.rs` add a test that `notify --state working --activity "Indexing repo"` puts `activity` on the socket line, one that `notify --state done` carries none, and one that a 5000-character `--activity` is still delivered with its state (the app truncates it)

**Checkpoint**: `cargo test --test hook_cli` passes; quickstart step 7 works.

---

## Phase 6: Polish and Cross-Cutting

- [X] T018 Confirm FR-009 by review: `remote::notify_attention` callers in `src/gpui_shell/poll.rs` and `src/app/attention.rs` still pass only `state.label()` and `message`; the native-notification `Alert` path in `src/term/attention.rs` never uses `activity`; record the result in research.md
- [X] T019 [P] Document the activity line, the state colors and `agents.activity` in the README Agents section and in `.context/architecture/SYSTEM_MAP.md` (data path: hook payload, `Message.activity`, `AttentionEvent::State`, `PaneAttention.activity`, `AgentRow.activity`)
- [X] T020 Run `./scripts/ci-local.sh` (check, test, clippy, fmt, audit under `RUSTFLAGS=-D warnings`) and fix everything it reports, including dead-code warnings in the wgpu binary
- [ ] T021 Run the quickstart end to end with a real Claude Code session, including the Esc interrupt check (step 7); record in research.md R1 whether `Stop` fires on interrupt
- [X] T022 [P] Run `graphify update .` and add the session handoff note to `.context/core/SESSION_STATE.md`

---

## Dependencies and Order

- Phase 1 -> Phase 2 -> Phases 3 to 5 -> Phase 6.
- Phase 2: T003 -> T004 -> T005; T006 is independent of T003 to T005 (T006 [P]); T007 needs T003 and T006.
- US1 (Phase 3): T008 needs T004; T009 needs T008 and T007; T010 is independent after T001; T011 needs T005, T006, T010; T012 needs T010; T013 needs T009.
- US2 (Phase 4): T014 needs T002 only and can run alongside US1; T015 needs T012 and T014 (same file as T012).
- US3 (Phase 5): T016 needs T007; T017 needs T016 and follows T013 (same file).
- US2 and US3 do not depend on US1's hook mapping; US2 needs only T012's `detail_text` refactor to avoid conflicting edits in `agents.rs`.

## Parallel Examples

- After T001/T002: T003 and T006 and T014 touch different files and can run at the same time.
- After T007: T010 (`term/agents.rs`), T013 (`tests/hook_cli.rs`) and T016 (`ipc/cli.rs`) are independent.
- Polish: T019 and T022 are independent of each other.

## Implementation Strategy

- **MVP**: Phases 1, 2 and 3 (User Story 1). It delivers the activity line for Claude Code with the config switch, and every later phase is additive.
- **Increment 2**: Phase 4 (cards). Pure style function first (T014), then the one edit to `agents.rs` (T015).
- **Increment 3**: Phase 5 (notify flag) for other integrations.
- Stop and run the matching quickstart step after each phase; the gpui-dependent checks need the Metal toolchain.
