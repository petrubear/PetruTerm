# Feature Specification: Agent Activity Line and State-Colored Cards

**Feature Branch**: `003-agent-activity-cards`

**Created**: 2026-11-07

**Status**: Draft

**Input**: User description: "Make the Agents sidebar tab show what each agent is doing right now (an activity line such as 'Running: cargo test') and make each agent card visibly reflect its state through color, like cmux does. Analyze only; no hook installation by the user."

## Background (verified 2026-11-07)

- Claude Code is already integrated without any user setup: the `claude` shim (`scripts/shims/claude`) starts the real binary with `--settings` carrying 7 hooks that call `petruterm hook claude <event>`. Nothing is written to disk. The hooks map to a state (`working`, `needs-input`, `done`, `idle`) in `src/ipc/claude_hooks.rs`.
- The mapping throws away what the hook payload carries. `PreToolUse` becomes a bare `working` with no text; `tool_name`, `tool_input` and the user prompt are discarded. The sidebar row therefore shows only "working".
- `AttentionTracker::apply_state` (`src/term/attention.rs`) overwrites `last_message` only when a new message arrives and never clears it. A `working` activity stored in `last_message` would still be shown after `Stop` ("done: Bash: cargo test"). The activity must be its own field with its own lifetime.
- The Agents row (`src/gpui_shell/sidebar/agents.rs`) renders a plain transparent row with a 6px dot that is orange only when unread or needs-input. State is conveyed by text and that dot.
- cmux (analyzed 2026-11-07) fills its cards by state and shows an activity line, a status icon and an optional progress bar. Its data comes from hook-pushed status text, not from the terminal. Its spinner is a Core Animation rotation; its progress bar is static. Animation and progress are out of scope here.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - See what an agent is doing right now (Priority: P1)

As a user running several agents, I want each Agents row to show a short line describing the agent's current action, so I can tell a busy agent from a stuck one without switching to its pane.

**Why this priority**: This is the core value: today "working" says nothing.

**Independent Test**: Run Claude Code in a pane, ask it to run a command and edit a file, and watch its row; the line changes to the current tool and target, then clears when the agent finishes.

**Acceptance Scenarios**:

1. **Given** an agent that just received a prompt, **When** the prompt is submitted, **Then** the row shows the start of the prompt (first line, truncated) as its activity.
2. **Given** a working agent, **When** it starts a tool call, **Then** the row shows the tool and its target, for example `Bash: cargo test`, `Edit: src/term/agents.rs`, `Read: Cargo.toml`, `Grep: AgentState`.
3. **Given** an agent that uses a tool with no recognized target, **When** the call starts, **Then** the row shows the tool name alone.
4. **Given** a working agent, **When** it finishes (`Stop`), **Then** the activity line is gone and the row shows the final state ("done") with its usual age.
5. **Given** an agent that needs input, **When** it raises a notification, **Then** the row shows the notification text, not the last tool activity.
6. **Given** a very long command or path, **When** it is displayed, **Then** it is cut to one line with an ellipsis; the row never grows taller.
7. **Given** text that contains control characters, escape sequences or newlines, **When** it is displayed, **Then** it is shown as safe plain text on one line.
8. **Given** an agent that is not running Claude Code (no activity reported), **When** its row renders, **Then** it looks exactly as it does today.

---

### User Story 2 - Tell agent states apart at a glance (Priority: P1)

As a user, I want each agent card to be tinted by its state, so I can scan the list and see which agents are working, waiting for me, done or idle without reading.

**Why this priority**: The list is only useful if the urgent card stands out; the row is currently almost uniform.

**Independent Test**: With four agents in four different states, confirm the four cards have four clearly different looks in the dark theme and in at least one light theme, and that text stays readable.

**Acceptance Scenarios**:

1. **Given** an agent in state working, needs-input, done or idle (or not yet reported), **When** its card renders, **Then** each state has its own distinct color treatment.
2. **Given** the needs-input state, **When** the card renders, **Then** it is the most prominent treatment in the list (it is the one that wants the user).
3. **Given** the idle or not-yet-reported state, **When** the card renders, **Then** it is neutral and quiet, as today.
4. **Given** the keyboard cursor or selection is on a card, **When** it renders, **Then** the selection remains visible on top of the state color (the user can tell which card is selected and what its state is).
5. **Given** any bundled theme, **When** a card renders in any state, **Then** the name, activity line and age keep readable contrast against the card background.
6. **Given** a state change, **When** the next event arrives, **Then** the card color follows it immediately, with no stale color from the previous state.
7. **Given** the user has unread attention on a card, **When** it renders, **Then** the existing unread dot still appears.

---

### User Story 3 - Other agents can feed the same line (Priority: P3)

As a user (and as a future integration author), I want the control socket and `petruterm notify` to accept an optional activity text, so Kiro, Codex and others can show the same line once they report it.

**Why this priority**: The first integration is Claude Code only; this keeps the contract ready for the rest without another wire change. Per-agent integrations are separate features.

**Independent Test**: From a pane, send a `working` state with an activity text through the CLI and confirm the pane's agent row shows it; send `idle` and confirm it clears.

**Acceptance Scenarios**:

1. **Given** a pane running a detected agent, **When** a `working` state with activity text is sent for that pane, **Then** the row shows that text.
2. **Given** a message without an activity field, **When** it is received, **Then** behavior is unchanged from today (old hooks and old scripts keep working).
3. **Given** an oversized or malformed activity value, **When** it is received, **Then** it is truncated or ignored without dropping the state update.

---

### Edge Cases

- **Stale "working" after an interrupt.** The user presses Esc in Claude Code; Claude Code may not fire `Stop`, so the card stays working with an old activity. The age shown on the row already exposes this; detecting the interrupt is out of scope (see Open Questions).
- **Burst of events.** Tool calls can fire many times per second. The row must update without flicker and without making the app sluggish; only the latest activity is kept.
- **State changes without activity.** `needs-input`, `done` and `idle` clear the activity. A later `working` event without activity keeps the previous activity text until a new one arrives or the state leaves `working`.
- **Pane closed or agent gone.** The activity is dropped with the pane's attention state, as the state is today.
- **Sensitive content.** A command line or prompt can contain secrets. It is shown only in the local sidebar and is not forwarded elsewhere by this feature (see FR-009).
- **Row already showing a custom name, session id, "msg" badge.** Adding the activity line must not displace or hide any of them.
- **Narrow sidebar.** On the minimum sidebar width the line truncates; nothing overflows or wraps.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The system MUST keep, per pane, an activity text that is separate from the notification message. It MUST be set by a `working` report that carries activity, replaced by newer activity, and cleared when the pane's state becomes `needs-input`, `done` or `idle`, or when the pane's attention state is removed.
- **FR-002**: The Claude Code hook mapping MUST derive activity from the hook payload: from the user prompt on `UserPromptSubmit`, and from the tool name and a short, tool-specific target on `PreToolUse` (command for shell tools, file path for file tools, pattern for search tools, tool name alone otherwise). Fields that are absent or of an unexpected type MUST be ignored without error.
- **FR-003**: Activity text MUST be sanitized and truncated before storage: control and escape characters removed, whitespace collapsed to a single line, and a hard length cap applied (default 80 characters, with an ellipsis).
- **FR-004**: The wire message MUST accept an optional activity field in addition to the existing fields; messages without it MUST behave exactly as before, and messages with it MUST NOT break older receivers' parsing of the other fields.
- **FR-005**: The Agents row MUST show the activity text on its detail line while the agent is `working`, in place of the bare state word, and MUST continue to show the state word, the notification message and the age in the other states as it does today.
- **FR-006**: The row MUST NOT change height when the activity text changes or is cleared.
- **FR-007**: Each Agents card MUST apply a state-dependent color treatment for working, needs-input, done and idle/unreported, drawn from the active theme's colors (plus the existing attention color for needs-input) so it adapts to every bundled theme, light and dark.
- **FR-008**: The state color MUST NOT make card text noticeably harder to read than the untinted row in any bundled theme (the tint is low-opacity; the activity line uses the same muted style as the existing detail text), and the selection/cursor highlight MUST remain distinguishable from the state color. (Amended 2026-11-07: the existing muted text color is the foreground at 35% alpha, so an absolute 4.5:1 target is already unmet today; see research R7.)
- **FR-009**: Activity text MUST NOT be sent to remote (phone) clients or any other consumer outside the local sidebar by this feature. Existing remote payloads are unchanged.
- **FR-010**: A config option under `agents` MUST control the activity detail: `"full"` (tool and target, default), `"tool"` (tool name only, no commands or paths) or `"off"` (no activity line). It MUST apply on config hot reload without a restart.
- **FR-011**: The wgpu binary MUST keep building and behaving as before; the new data path is shared state, and only the gpui Agents tab renders it.
- **FR-012**: New code MUST follow the project conventions: files under 400 lines, `anyhow`/`thiserror` for errors, and unit tests for the hook mapping, sanitization and the clearing rules.

### Key Entities

- **Activity**: a short plain-text description of the agent's current action, tied to one pane, valid only while that pane's state is `working`.
- **Card state**: one of working, needs-input, done, idle/unreported, derived from the existing attention state; it picks the card's color treatment.

## Success Criteria *(mandatory)*

- **SC-001**: While Claude Code works on a task, the user can read what it is doing from the Agents tab alone, with the line updating on each tool call, and the stale text is gone within one event of the agent stopping.
- **SC-002**: With four agents in four different states, a user can name each agent's state from the sidebar without reading text, in both a dark and a light theme.
- **SC-003**: Existing behavior is unchanged for agents that report no activity: the unit tests for attention state, the hook mapping and the agents row layout that pass today still pass.
- **SC-004**: Activity text never contains control characters or exceeds the configured length, for any hook payload, including malformed ones.
- **SC-005**: A burst of 50 tool-call events within one second leaves the app responsive (no dropped frames noticeable in the sidebar) and the row showing the last activity.

## Assumptions

- Claude Code's `PreToolUse` payload carries `tool_name` and `tool_input`, and `UserPromptSubmit` carries `prompt`. The exact field names per tool are confirmed during research before implementation.
- Only the gpui sidebar renders the Agents tab; the wgpu binary shares the attention state but shows no Agents list.
- Kiro, Codex, Copilot and Antigravity integrations are separate follow-up features; this feature only prepares the shared contract (User Story 3).
- Colors for working (blue/accent), done (green) and idle (neutral) come from the theme where the theme provides them, otherwise from fixed fallbacks that meet FR-008. The needs-input color stays the existing attention orange.

## Out of Scope

- Progress bars, spinners and any animation (cmux's spinner is a Core Animation rotation and its bar is static; both are a separate, later feature).
- Hooks for Kiro, Codex and other agents, and any installation command for them.
- Detecting an interrupted agent that never sent `Stop`.
- Quick-reply buttons or any action that types into an agent's pane.
- Sending activity to the remote (phone) protocol.

## Open Questions

1. **Secrets in commands.** Shell commands can contain tokens. Default `"full"` shows them in the local sidebar only. Should the default be `"tool"` instead? Recommended: keep `"full"` (the sidebar is local and the user's own session), documented in the config comments.
2. **Interrupt handling.** Should a `working` card with no event for a long time dim or show a "stalled" hint? Recommended: not in this feature; the age on the row is enough for now.
3. **Card treatment.** A full tinted fill (cmux style) or a tinted background with a colored left edge? Recommended: a low-opacity tint plus a colored left edge, because a full fill is hard to keep readable across themes.
