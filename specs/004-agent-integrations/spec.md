# Feature Specification: Agent Integrations (Kiro, Codex, Antigravity)

**Feature Branch**: `004-agent-integrations`

**Created**: 2026-11-07

**Status**: Draft

**Input**: User description: "The Agents card must work the same for any agent. Define an interface with multiple implementations so a new agent is easy to add, and cover Kiro, Codex and Agy (Antigravity), as Claude Code already works."

## Background (researched 2026-11-07)

- **The card is already agent-neutral.** The Agents row renders an `AgentRow` (state, activity, message, session, color). Claude Code fills it through hooks: the `claude` shim (`scripts/shims/claude`) starts the real binary with `--settings`, the hooks call `petruterm hook claude <event>`, and `src/ipc/claude_hooks.rs` maps the payload to a `Message` (state, activity, session) for the control socket. Kiro, Codex and Antigravity show only "running" because nothing reports their state.
- **The Claude pieces are hard-coded.** `petruterm hook <agent> <event>` ignores any agent but `claude`, and the payload mapping lives in one Claude-only file.
- **All three agents have hook systems close to Claude's** (sources are the vendors' own docs; items marked *to verify* are settled in the research step of the plan):

| | Kiro CLI (V3) | Codex CLI | Antigravity CLI (`agy`) |
|---|---|---|---|
| Docs | [kiro.dev/docs/hooks](https://kiro.dev/docs/hooks/) | [Codex hooks](https://learn.chatgpt.com/docs/hooks) | [antigravity.google/docs/hooks](https://antigravity.google/docs/hooks) |
| Global config | `~/.kiro/hooks/*.json` (`{"version":"v1","hooks":[{name,trigger,action:{type:"command",command},timeout}]}`) | `~/.codex/hooks.json` or `[hooks]` in `~/.codex/config.toml` | `~/.gemini/config/hooks.json` (hook name -> event -> handlers) |
| Workspace config | `.kiro/hooks/` | `<repo>/.codex/` (only when trusted) | `.agents/hooks.json` |
| Events we need | `UserPromptSubmit`, `PreToolUse`, `Stop`, `SessionStart`, `SessionEnd` | `UserPromptSubmit`, `PreToolUse`, `PermissionRequest`, `Stop`, `Interrupt`, `SessionStart`, `SessionEnd` | `PreToolUse`, `PostToolUse`, `PreInvocation`, `Stop`, `SessionStart` |
| stdin JSON | `hook_event_name`, `cwd`, `session_id`; tool events add `tool_name`, `tool_input` (prompt text: *to verify*, docs mention a `USER_PROMPT` variable) | `session_id`, `cwd`, `hook_event_name`, `turn_id`; `prompt`; `tool_name`, `tool_input`; `last_assistant_message` | camelCase: `conversationId`, `workspacePaths`, `lastUserInput`, `toolCall.name`, `toolCall.args`; `Stop` has `fullyIdle`, `terminationReason` |
| What the hook must output | nothing: exit 0 adds stdout to the model's context; a failing `PreToolUse` hook blocks the tool | nothing: stdout of `UserPromptSubmit`/`SessionStart` becomes context; exit 2 blocks | JSON: `PreToolUse` requires `{"decision": ...}`, `PostToolUse` `{}`, `Stop` any decision other than `"continue"` |
| Needs-input signal | none documented | `PermissionRequest` | none documented |
| Interrupt signal | none documented | `Interrupt` | `Stop` with a `terminationReason` |
| Extra | non-interactive and nested sessions exist | non-managed hooks must be trusted once with `/hooks`; hooks can be disabled by feature flag | PreToolUse/PostToolUse matchers are regexes |

- **Reference**: cmux handles Kiro with an installed global hook file (`cmux hooks setup kiro`), session state running/idle/needsInput, and silent hooks (lifecycle output discarded) so nothing leaks into the model's context.
- **Safety facts that shape the design**: a hook that fails or prints the wrong thing can block a Kiro tool call, stall an Antigravity decision, or inject text into a model's context. Every integration hook must therefore exit 0 and print exactly what that agent expects (nothing, or the required JSON) in every case, including outside PetruTerm and when the app is not running.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - One interface, Claude Code unchanged (Priority: P1)

As a maintainer, I want agent support expressed through one interface with a registry, with Claude Code as its first implementation, so adding an agent is writing one implementation and nothing in the UI.

**Why this priority**: Everything else builds on it, and it must not change what Claude Code users see today.

**Independent Test**: Run the existing Claude Code flow (quickstart of spec 003) before and after: the activity line, states and cards are identical, and `petruterm hook claude <event>` produces the same socket messages byte for byte.

**Acceptance Scenarios**:

1. **Given** `petruterm hook <agent> <event>` for an agent that has an implementation, **When** it runs in a pane, **Then** the registry's implementation maps the payload to a normalized report and sends it; for an unknown agent it exits 0 silently.
2. **Given** the existing Claude hook tests, **When** the suite runs after the refactor, **Then** they pass unchanged.
3. **Given** two implementations reporting the equivalent event (for example "a prompt was submitted"), **When** their reports are applied, **Then** the resulting `AgentRow` state, activity and card appearance are identical apart from the agent name and color.
4. **Given** a new agent implementation added in code, **When** it is registered, **Then** no change is needed in the sidebar, the tracker, the socket protocol or the card code.

---

### User Story 2 - Kiro reports state and activity (Priority: P1)

As a Kiro user, I want my Kiro cards to show working, needs-input, done and idle and a live activity line, like Claude Code's.

**Why this priority**: Kiro is the agent the user runs next to Claude Code and the one cmux already supports.

**Independent Test**: Run `petruterm hooks setup kiro`, start `kiro-cli` in a pane, ask it to run a command: the card goes working with `Bash`-style activity, then done.

**Acceptance Scenarios**:

1. **Given** Kiro CLI V3 and the setup command run once, **When** the user submits a prompt, **Then** the card shows working and the start of the prompt.
2. **Given** a working Kiro, **When** it starts a tool, **Then** the activity line shows the tool and its target (Kiro's tool names and fields are mapped by the implementation).
3. **Given** Kiro finishes its turn, **When** the stop event arrives, **Then** the card shows done, and when the session ends it shows idle.
4. **Given** Kiro outside PetruTerm, **When** a hook runs, **Then** it does nothing and prints nothing.
5. **Given** a hook that fails or times out, **When** Kiro runs a tool, **Then** the tool call is not blocked and nothing is injected into the model's context.
6. **Given** Kiro is not in V3 mode, **When** the user runs the setup command, **Then** the file is still written (V3 is a mode of Kiro CLI 2.13+, not a detectable version) and the command prints that V3 mode is required for the hooks to run.

---

### User Story 3 - Codex reports state, activity and needs-input (Priority: P1)

As a Codex user, I want the same card behavior, including "needs input" when Codex asks for approval and "idle" when I interrupt it.

**Why this priority**: Codex's hooks cover more events than Claude's, so its cards can be the most accurate.

**Independent Test**: With hooks set up, run `codex`, ask it to run a command that needs approval: the card shows needs input, then working after approval, then done; pressing Esc to interrupt returns the card to idle.

**Acceptance Scenarios**:

1. **Given** Codex with the hooks installed and trusted, **When** a prompt is submitted, **Then** the card shows working with the prompt start; tool starts update the activity line.
2. **Given** a permission request, **When** the event arrives, **Then** the card shows needs input with the request's description when present.
3. **Given** the user interrupts a running turn, **When** the interrupt event arrives, **Then** the card shows idle and the activity clears.
4. **Given** hooks are installed but not yet trusted, **When** the user opens Codex, **Then** the card keeps working through the output fallback (Story 5) and the setup command's output tells the user to trust the hooks once with `/hooks`.
5. **Given** a Codex hook, **When** it runs, **Then** it prints nothing on stdout for events whose stdout becomes context and exits 0.

---

### User Story 4 - Antigravity reports state and activity (Priority: P2)

As an Antigravity user, I want its card to show state and activity too.

**Why this priority**: The same value as the other two, ranked below them only because its payload shape and stdout contract differ the most.

**Independent Test**: With hooks set up, run `agy`, ask it to run a command: the card goes working with the tool target, then done when it is fully idle.

**Acceptance Scenarios**:

1. **Given** Antigravity with the hooks installed, **When** a prompt starts an invocation, **Then** the card shows working; tool starts show the tool and target (names like `run_command`, `view_file` are mapped).
2. **Given** a `PreToolUse` hook, **When** it runs, **Then** it prints the JSON decision the agent requires (never blocks or changes a permission) and exits 0.
3. **Given** the `Stop` event, **When** `fullyIdle` is true, **Then** the card shows done; when it is false the card stays working.
4. **Given** `PostToolUse` and `Stop` hooks, **When** they run, **Then** they print the minimal JSON those events expect and never ask the agent to continue.

---

### User Story 5 - Any agent gets a state without hooks (Priority: P2)

As a user of an agent with no integration (a custom agent from `agents.recognized`, or one whose hooks are not installed), I want a basic working/idle state from terminal activity.

**Why this priority**: It makes every card useful on day one and for agents nobody has written an integration for.

**Independent Test**: Run an unintegrated agent (or one with hooks not installed): the card turns working while the agent streams output and idle after it goes quiet; typing into the agent does not flip it to working.

**Acceptance Scenarios**:

1. **Given** an agent with no hook-reported state, **When** its pane produces output continuously, **Then** the card shows working; after a quiet period (default 3 s) it shows idle.
2. **Given** the user types into the pane, **When** the terminal echoes the keystrokes, **Then** that echo does not count as agent activity.
3. **Given** an agent whose hooks have reported a state, **When** the observer disagrees, **Then** the hook-reported state wins and the observer is ignored for that pane.
4. **Given** the observer is the only source, **When** the card renders, **Then** it has a state and colors but no activity text.
5. **Given** `agents.output_state = false`, **When** an agent without hooks runs, **Then** its card stays "running" as today.

---

### User Story 6 - Install, inspect and remove the hooks safely (Priority: P2)

As a user, I want one command to install the hooks for the agents I use, see what is installed, and remove it all, without my own hook files being damaged.

**Why this priority**: Kiro, Codex and Antigravity have no shim; the hooks live in files the user also edits.

**Independent Test**: Run `petruterm hooks setup`, check the three files contain our entries and the user's own entries are intact, run it again (no change), then `petruterm hooks remove` (only our entries are gone).

**Acceptance Scenarios**:

1. **Given** the user has their own hooks in any of the files, **When** setup runs, **Then** those entries are untouched and ours are added once (idempotent, identified by the `PETRUTERM_BIN` marker).
2. **Given** `petruterm hooks setup <kiro|codex|agy|all>`, **When** run, **Then** it installs only for agents whose binary is found, prints what it wrote and where, and supports `--dry-run`.
3. **Given** `petruterm hooks remove [agent]`, **When** run, **Then** only entries carrying our marker are removed and a file that becomes empty by that is deleted only if we created it.
4. **Given** `petruterm hooks status`, **When** run, **Then** it lists each agent as not installed, installed, or installed with a note (for example "trust required in Codex").
5. **Given** an unparsable or unexpected hooks file, **When** setup runs, **Then** it refuses with a clear message and does not modify the file.
6. **Given** the hooks installed, **When** the agent runs outside PetruTerm (no pane variables), **Then** the hook is a silent no-op.

---

### User Story 7 - Add a new agent with a small implementation (Priority: P3)

As a maintainer, I want a documented, tested recipe so a new agent needs one file and a registry line.

**Why this priority**: It is the stated goal of the interface; it validates the design but ships no user-visible behavior.

**Independent Test**: A fake agent implementation in the test suite, written from the documented recipe, shows a correct card through the whole path.

**Acceptance Scenarios**:

1. **Given** the recipe in the module docs, **When** a developer adds an implementation, **Then** the shared contract tests (normalized report shape, silent output, exit 0) run against it with one extra line.
2. **Given** a user-defined agent in `agents.recognized` whose binary is a renamed Kiro, **When** it runs in a pane where Kiro hooks are installed, **Then** its card is hook-driven like Kiro's, with no extra configuration.

---

### Edge Cases

- **Hooks and non-interactive runs.** `kiro-cli chat --no-interactive`, `codex exec` and nested agent sessions fire the same hooks; their reports go to the pane that launched them and must not leave a card stuck on working (a session end or stop always clears it).
- **Two sources, one pane.** The hook state and the output observer can disagree; the hook state wins. If a hook source goes silent while `working` (crash, kill), the pane falls back to the observer after a timeout.
- **Hooks installed twice** (for example Kiro global plus workspace hooks): a duplicate report for the same event is harmless (same state, same activity).
- **App not running.** The hook finds no socket and exits 0 without delay and without output.
- **Tool names we do not know.** The activity line shows the tool name alone.
- **Agent upgrades change a format.** A payload that does not match is treated as "no report" for that event; the card keeps its last state and the observer covers the gap.
- **Prompts in logs.** Prompt text and tool arguments never go to logs or to disk (as in spec 003).
- **Large payloads.** Hook stdin is read with the existing 64 KB and 500 ms limits.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The system MUST define one agent-integration interface with these responsibilities: identify which recognized agents it serves; map one hook invocation (event name plus stdin payload plus pane) to a normalized report (state, activity, message, session) or to no report; state the exact stdout it must print for each event; describe how its hooks are installed and removed.
- **FR-002**: The system MUST keep a registry of implementations; `petruterm hook <agent> <event>` MUST resolve the agent through it, exit 0 silently for an unknown agent, and for a known one send the normalized report over the existing control socket without changing the socket schema.
- **FR-003**: The Claude Code behavior (shim, hooks, mapping, tests) MUST be preserved exactly; it becomes the first implementation.
- **FR-004**: The system MUST provide implementations for Kiro CLI V3, Codex CLI and Antigravity CLI that map their documented events to the states of the existing `AgentState` as follows:

| Normalized | Kiro | Codex | Antigravity |
|------------|------|-------|-------------|
| working (+ prompt activity) | prompt submit | `UserPromptSubmit` | `PreInvocation` (prompt from `lastUserInput`) |
| working (+ tool activity) | `PreToolUse` | `PreToolUse` | `PreToolUse` |
| needs-input (+ message) | none | `PermissionRequest` | none |
| done | `Stop` | `Stop` | `Stop` when `fullyIdle` |
| idle (clears activity) | `SessionEnd` | `Interrupt`, `SessionEnd` | `Stop` with an interrupt reason |
| session id announced | `session_id` | `session_id` | `conversationId` |

- **FR-005**: Every integration hook MUST exit 0 in all cases and print exactly what the agent expects: nothing for Kiro and Codex; the required minimal JSON for Antigravity (`PreToolUse` an allow decision that never changes permissions, `PostToolUse` and `Stop` a value that never continues the agent). Outside a PetruTerm pane the hook MUST print that same output (so the agent behaves as if the hook were absent) and send nothing.
- **FR-006**: Tool activity MUST be derived per agent from its own tool names and argument fields, cleaned and capped by the shared rules of spec 003 (`clean_activity`, 80 characters); unknown tools show the tool name alone.
- **FR-007**: `petruterm hooks setup [kiro|codex|agy|all] [--dry-run]`, `hooks remove [agent]` and `hooks status` MUST install, remove and report hooks in each agent's global configuration (Kiro `~/.kiro/hooks/`, Codex `~/.codex/hooks.json`, Antigravity `~/.gemini/config/hooks.json`), merging into existing files, idempotent, identified by the `PETRUTERM_BIN` marker, never altering entries it did not create, refusing on unparsable files, and skipping agents whose binary is not found.
- **FR-008**: The installed hook command MUST be a no-op outside PetruTerm (`PETRUTERM_BIN` unset), as the Claude hooks are.
- **FR-009**: The system MUST provide an output-based state source for panes whose agent has no hook-reported state: working while the pane's terminal output is flowing, idle after a quiet period (default 3 s), ignoring output that is the echo of the user's own typing. It MUST report state only (no activity text).
- **FR-010**: A hook-reported state MUST take precedence over the output-based state for that pane; the output-based source MUST resume for a pane whose hook-reported `working` has been silent beyond a timeout (default 120 s) or whose agent process was replaced.
- **FR-011**: A new config key `agents.output_state` (boolean, default true) MUST enable or disable the output-based source and apply on hot reload; the quiet period is a constant in code, not a setting.
- **FR-012**: *(removed)* Hook reports are keyed by pane, so a renamed or custom agent binary needs no new config key: it gets hook-driven cards once its process name is in `agents.recognized` and its hook entries exist.
- **FR-013**: The Agents card, `AgentRow`, the tracker, the socket protocol and the activity/cards contracts of spec 003 MUST NOT change; all agents render through the same path.
- **FR-014**: Hook payloads, prompt text and tool arguments MUST NOT be written to logs or disk.
- **FR-015**: New logic MUST live in shared modules (`src/term`, `src/ipc`) with files under 400 lines; pure mapping functions MUST have unit tests per agent with recorded sample payloads, and a shared contract test MUST run against every registered implementation (exit 0, expected stdout, normalized report shape).
- **FR-016**: The wgpu binary MUST keep building and behaving as before.

### Key Entities

- **Integration**: an implementation of the interface for one agent family (Claude, Kiro, Codex, Antigravity); stateless.
- **Report**: the normalized result of one hook event: optional state, optional activity, optional message, optional session id (today's `Message` fields).
- **Output observer**: per-pane tracker of terminal output timing used when no hook state exists.
- **Hook entry**: one command registered in an agent's configuration file, carrying the `PETRUTERM_BIN` marker.

## Success Criteria *(mandatory)*

- **SC-001**: With hooks installed, a Kiro, a Codex and an Antigravity session each show working, a live activity line, and done within one event of the agent finishing; Codex also shows needs input and idle on interrupt.
- **SC-002**: Claude Code behavior is unchanged: the existing hook, tracker and row tests pass without edits, and the socket messages for the same payloads are identical.
- **SC-003**: Equivalent events from different agents yield identical card appearance (verified by a shared test over the normalized reports).
- **SC-004**: A user-owned entry in any of the three hook files survives setup, repeated setup and removal byte for byte (verified by tests on sample files).
- **SC-005**: A hook outside PetruTerm, or with the app not running, completes in under 100 ms with exit 0 and the agent-correct output, for all four integrations.
- **SC-006**: An agent with no hooks shows working within 1 second of streaming output and idle within 4 seconds of silence, and typing alone never flips it to working.
- **SC-007**: Adding a fictional fifth agent in the test suite needs one implementation file and one registry line, with no other source file changed.

## Assumptions

- Kiro CLI V3 mode (global hooks, available from Kiro CLI 2.13) is the supported Kiro mode; 2.x agent-scoped hooks are out of scope.
- Hooks inherit the pane's environment (`PETRUTERM_PANE_ID`, `PETRUTERM_SOCKET`, `PETRUTERM_BIN`) from the agent process; this is verified per agent in the plan's research step.
- The exact stdin shape of Kiro's prompt-submit event (stdin field versus a `USER_PROMPT` variable), Codex tool names and argument fields, and Antigravity's `Stop` interrupt reason values are confirmed against real sessions in research; an unconfirmed field makes that part of the mapping produce no activity, not an error.
- The existing Claude shim remains the Claude installation path; Kiro, Codex and Antigravity use files because no equivalent launch-time injection is documented.
- Activity detail and privacy rules (`agents.activity`, local sidebar only) from spec 003 apply to all agents.

## Out of Scope

- Kiro CLI 2.x agent-scoped hooks and the IDE.
- Copilot (no hook system researched) and agents other than the four; they get the output-based state only.
- Blocking, approving or modifying anything an agent does through a hook (observation only).
- Progress bars, spinners and animation.
- Workspace-level hook files (`.kiro/hooks/`, `.codex/`, `.agents/`); only global configuration is managed.
- Sending anything from PetruTerm into an agent through a hook (agent messaging stays as in spec 002).

## Open Questions

*(Resolved in `research.md` R9; kept for history.)*

1. **Codex trust prompt.** Non-managed hooks need a one-time `/hooks` trust. Is there a launch-time way to inject hooks (the way cmux's Codex wrapper does) that avoids the prompt? Recommended: ship file-based install with a clear message first, and add a shim only if research finds a documented flag.
2. **Interrupt reliability on Kiro and Antigravity.** Neither documents a clean interrupt event. Recommended: rely on the output observer's idle timeout for those two.
3. **Copilot.** Add an integration later if its CLI gains hooks; until then it uses the output-based state.
