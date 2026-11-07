# Feature Specification: Agents Sidebar Tab

**Feature Branch**: `001-agents-sidebar-tab`

**Created**: 2026-11-06

**Status**: Draft

**Input**: User description: "run the specification for the Suggested v1" - a new "Agents" section in the workspace sidebar listing every AI coding agent session running in any pane, across all workspaces and tabs, with state, and jump-to-pane.

## Clarifications

### Session 2026-11-06

- Q: Should the list include the built-in AI panel / ACP chat sessions? -> A: No, CLI agents running in terminal panes only.
- Q: What happens to a row when the agent process is gone but its last reported state was not done/idle? -> A: Remove the row as soon as the process is gone, regardless of last reported state.
- Q: What proves a pane hosts an agent when detection and reports disagree? -> A: A pane is listed only if its running process is a recognized agent; reports only update the state of listed panes. The recognized list must cover many agents (Claude Code, Kiro, Codex, Antigravity, Copilot, and more) and be easy to extend.
- Q: Are agents running inside tmux or other nested sessions detected? -> A: Not in v1; only the pane's foreground process (and its direct children) counts.
- Q: Any always-visible indicator when the sidebar is closed? -> A: No; the list is visible only when the sidebar is open.

### Session 2026-11-06 (amendment: session id and custom names)

- Q: What characters may a custom agent name contain? -> A: Address-safe: 1-32 characters from letters, digits, `-`, `_`, `.`; no spaces. Names are unique among listed agents (case-insensitive), because they will later be the address for inter-agent communication.
- Q: How long does a custom name last? -> A: While the pane lives; it is lost when the pane or the app closes. It is not saved to disk.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - See all running agents in one place (Priority: P1)

As a user running several coding agents (for example Claude Code and Kiro) in different panes, tabs and workspaces, I open the sidebar's "Agents" section and see one row per agent session, so I no longer have to hunt through tabs to find out what is running and what is waiting for me.

**Why this priority**: This is the core value: a single overview of agent activity across the whole terminal.

**Independent Test**: Start two different agents in two panes in different workspaces, open the Agents section, and confirm both appear with the correct agent name, workspace and tab.

**Acceptance Scenarios**:

1. **Given** two agents running in panes of different workspaces, **When** the user opens the Agents section, **Then** two rows are shown, each with agent name, workspace name, tab name and current state.
2. **Given** a pane running only a plain shell, **When** the user opens the Agents section, **Then** that pane does not appear.
3. **Given** an agent that exits, **When** the user looks at the Agents section, **Then** its row disappears without requiring any user action.
4. **Given** no agents are running, **When** the user opens the Agents section, **Then** an empty-state message is shown.

---

### User Story 2 - Jump to an agent's pane (Priority: P1)

As a user, I select a row in the Agents list (by click or by keyboard) and the terminal switches to that agent's workspace, tab and pane and focuses it.

**Why this priority**: An overview is only useful if it takes me to the agent that needs me.

**Independent Test**: With agents in two different workspaces, select the row of the one not currently visible and confirm focus lands in that pane.

**Acceptance Scenarios**:

1. **Given** the Agents list with a row for an agent in another workspace, **When** the user activates that row, **Then** that workspace, tab and pane become active and focused.
2. **Given** the sidebar has keyboard focus, **When** the user moves the cursor with the same keys used in other sidebar sections and presses Enter, **Then** the highlighted agent's pane is focused.
3. **Given** the target pane was closed after the list was drawn, **When** the user activates its row, **Then** nothing breaks and the stale row is removed.

---

### User Story 3 - Know which agent needs me (Priority: P2)

As a user, I see each agent's state (working, needs input, done, idle), unread indication, last message and time since the last update, and rows needing input are listed first, so I can act on the most urgent one.

**Why this priority**: It turns the list from an inventory into a triage view, but the list is useful without it.

**Independent Test**: Have one agent waiting for input and one working; confirm the waiting one is listed first with a distinct indicator and its last message.

**Acceptance Scenarios**:

1. **Given** agents in states needs-input, working, done and idle, **When** the list is shown, **Then** rows are ordered needs-input, working, done, idle (ties by most recent update).
2. **Given** an agent reports a message, **When** the list is shown, **Then** the row shows the message truncated to one line and the elapsed time since the last update.
3. **Given** an unread agent row, **When** the user focuses that pane, **Then** the unread indication clears.

---

### User Story 4 - Agents are found without setup (Priority: P2)

As a user, I want agents to appear even when I have not configured any integration for them, so that "anywhere on the terminal" is true for agents such as Kiro. Detection alone identifies the agent; agents that also report their state show richer information (state, message), and agents that do not are shown as Running.

**Why this priority**: Without this, only integrated agents are visible and the list misleads.

**Independent Test**: Launch an agent with no integration configured in a pane and confirm a row appears within a few seconds, labelled running.

**Acceptance Scenarios**:

1. **Given** a supported agent started in a pane with no integration, **When** a few seconds pass, **Then** a row appears showing the agent name and a neutral "running" state.
2. **Given** an agent that reports its state, **When** it reports, **Then** its row shows the reported state instead of "running".

### User Story 5 - Know which session an agent row refers to (Priority: P2)

As a user with several sessions of the same agent open, I see each row's session identifier next to the agent name (when it can be determined), so I can tell which session a row refers to and match it with what the agent itself shows (for example when resuming it later).

**Why this priority**: Two "Claude Code" rows are indistinguishable without it; it complements, but does not block, the list itself.

**Independent Test**: Start two Claude Code sessions and one Kiro session; confirm each row shows a different short session id that matches the id the agent reports for that session.

**Acceptance Scenarios**:

1. **Given** an agent whose session id is known, **When** the list is shown, **Then** the row shows a short form of the id (first 8 characters) after the agent name.
2. **Given** an agent whose session id cannot be determined (not yet reported, unsupported agent), **When** the list is shown, **Then** the row shows no id and no placeholder, and is otherwise unchanged.
3. **Given** a row with a known session id, **When** the user right-clicks it and chooses "Copy session id", **Then** the full id is on the clipboard.
4. **Given** an agent that starts a new session in the same pane (for example after `/clear` or a restart), **When** its id changes, **Then** the row shows the new id within the normal refresh time.

---

### User Story 6 - Give an agent a custom name (Priority: P2)

As a user, I right-click an agent row and choose "Rename" to give it a custom name (for example `reviewer` or `backend-1`), so I can tell agents apart at a glance. The custom name is shown in the row, and is intended to become the name used to address that agent in later inter-agent communication.

**Why this priority**: It makes the list usable with many same-type agents and sets up the later messaging feature; the list works without it.

**Independent Test**: Right-click a row, rename it to `reviewer`, and confirm the row shows `reviewer`; try the same name on a second row and confirm it is refused; reset the name and confirm the row returns to its agent name.

**Acceptance Scenarios**:

1. **Given** an agent row, **When** the user right-clicks it, **Then** a context menu opens with "Rename", "Copy session id" (only if an id is known) and "Reset name" (only if a custom name is set).
2. **Given** the user chooses "Rename", **When** they type a valid name and confirm with Enter, **Then** the row shows the custom name in addition to the agent type, and the name stays until the pane closes.
3. **Given** a name already used by another listed agent (compared case-insensitively), **When** the user confirms it, **Then** the rename is refused with a visible message and the previous name is kept.
4. **Given** a name with spaces, more than 32 characters, or characters outside letters, digits, `-`, `_`, `.`, **When** the user types it, **Then** it is refused with a visible message.
5. **Given** the user presses Escape while renaming, **When** the editor closes, **Then** the previous name is kept.
6. **Given** an agent row with a custom name, **When** the agent exits and a new agent is started in the same pane, **Then** the custom name is cleared (it belongs to the running agent, not the pane slot).
7. **Given** a custom name, **When** the user chooses "Reset name", **Then** the row shows the plain agent name again.

### Edge Cases

- An agent is started inside tmux, ssh or another multiplexer: it is NOT listed in v1; only the pane's foreground process (and its direct children) is examined.
- Two agents in the same tab (split panes): each gets its own row.
- Same agent type run twice: rows are distinguishable by workspace, tab and pane number within the tab.
- A workspace or tab is renamed or closed while the list is open: the row updates or disappears.
- A very long message or workspace name: truncated to a single line without breaking layout.
- A hostile process prints text claiming to be an agent state: it must not create a row on its own; only recognized agent processes or explicit state reports from the pane's own session do.
- Sidebar hidden: no new indicator is shown (existing tab dots, pane rings and notifications still apply); the list is current the moment it is opened.
- Many agents (for example 20): the list scrolls and remains usable.
- The session id is longer or shorter than 8 characters: show up to the first 8 characters of whatever the agent uses as its identifier.
- Two panes report the same session id (for example one session resumed twice): both rows show it; nothing is merged.
- The agent's session store is missing, unreadable or in an unexpected format: no id is shown, no error is surfaced, and the rest of the row works normally.
- A rename is in progress and the pane closes or the agent exits: the editor closes and nothing is saved.
- A custom name that equals another row's agent type name (for example `kiro`): allowed, because uniqueness applies only among custom names.
- Right-click on empty list space: no menu.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The sidebar (gpui binary) MUST offer an "Agents" section, reachable the same ways as the existing sections (click on its label and keyboard section cycling).
- **FR-002**: The Agents section MUST list one row per agent session found in any pane of any tab of any workspace in the window.
- **FR-003**: Each row MUST show the agent name, workspace name, tab name and current state.
- **FR-004**: Plain shell panes with no agent MUST NOT appear in the list. Built-in AI panel / ACP chat sessions are not agent sessions for this feature and MUST NOT appear.
- **FR-005**: Activating a row (click or Enter) MUST switch to and focus the corresponding workspace, tab and pane.
- **FR-006**: The list MUST support keyboard navigation consistent with the other sidebar sections.
- **FR-007**: Rows MUST be ordered by urgency (needs-input, working, done, idle, Running), then by most recent update.
- **FR-008**: Each row SHOULD show an unread indicator, the last reported message (single line) and elapsed time since last update.
- **FR-009**: The system MUST identify the agent type of each pane by automatic detection of the pane's running process. Explicit reports from the agent only supply its state and message, never its identity.
- **FR-010**: Agents detected automatically with no state reports MUST be shown with a neutral "running" state.
- **FR-011**: Rows MUST appear within 5 seconds of an agent starting and disappear within 5 seconds of it exiting or its pane closing. A row MUST be removed when its agent process is gone, regardless of the last reported state (no stale "working" rows).
- **FR-012**: Agent information coming from terminal output or messages MUST be treated as untrusted: text shown is sanitized and length-limited, and a message cannot make a non-agent pane appear as an agent or affect another pane.
- **FR-013**: The list MUST show an empty-state message when there are no agents.
- **FR-014**: The list MUST remain responsive with at least 20 agent rows and MUST NOT measurably affect terminal rendering performance when the sidebar is closed.
- **FR-015**: Agent detection MUST NOT require any manual per-agent configuration for the agents on the recognized list. The list MUST include at least Claude Code, Kiro, Codex, Antigravity and Copilot, and MUST be extendable (for example by the user adding a process name) without code changes.
- **FR-016**: Existing attention behaviours (tab dots, pane rings, notifications, jump-to-unread) MUST continue to work unchanged.
- **FR-017**: A pane MUST be listed only when its running process is a recognized agent. An explicit state report from a pane that is not running a recognized agent MUST NOT create a row; reports only update the state of rows already listed.
- **FR-018**: When an agent's session identifier can be determined, its row MUST show the first 8 characters of it after the agent name; when it cannot, the row MUST show nothing in its place.
- **FR-019**: Session identifier discovery MUST work for Claude Code and Kiro, MUST NOT require the user to change agent settings, and MUST NOT read conversation content; agents for which no reliable source exists MUST simply show no id.
- **FR-020**: Right-clicking an agent row MUST open a context menu with "Rename", "Copy session id" (shown only when an id is known) and "Reset name" (shown only when a custom name is set).
- **FR-021**: "Rename" MUST open an inline editor on the row; Enter confirms, Escape cancels, and clicking elsewhere cancels.
- **FR-022**: A custom name MUST be 1-32 characters from letters, digits, `-`, `_` and `.`; invalid names MUST be refused with a visible message and the previous name kept.
- **FR-023**: Custom names MUST be unique among currently listed agents, compared case-insensitively; a duplicate MUST be refused with a visible message.
- **FR-024**: A row with a custom name MUST show the custom name prominently and still show the agent type; sorting, state, jump and colors are unchanged.
- **FR-025**: A custom name MUST last only while its agent runs in its pane: it is cleared when the pane closes, the app exits, or a different agent process starts in that pane. It MUST NOT be written to disk.
- **FR-026**: "Copy session id" MUST place the full session identifier on the clipboard.
- **FR-027**: Custom names MUST be readable by other parts of the app through a single lookup (agent row to name, and name to pane) so the later inter-agent communication feature can address agents by name without changing this feature.

### Key Entities

- **Agent session**: A coding agent running in a pane. Attributes: agent type (for example Claude Code, Kiro), the pane it runs in, state, unread flag, last message, time of last update, how it was identified (reported or detected).
- **Agent state**: working, needs-input, done, idle, or running (unknown, for agents that do not report).
- **Location**: Workspace, tab and pane identity for an agent session, used for display and jump.
- **Session identifier**: The agent's own identifier for a conversation session (may be unknown). Shown shortened; copied in full.
- **Custom name**: A user-assigned, unique, address-safe label for a running agent; lives with the agent's pane and process, never persisted.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: With three agents running in three different workspaces, a user can identify which one needs input and focus it in under 5 seconds from opening the sidebar.
- **SC-002**: 100% of recognized agents started in a pane appear in the list within 5 seconds without any prior configuration.
- **SC-003**: 100% of exited agents or closed panes are removed from the list within 5 seconds.
- **SC-004**: Activating a row focuses the correct pane in 100% of cases across workspaces and tabs.
- **SC-005**: No visible change in terminal frame rate or input latency with the sidebar closed and 20 agents running.
- **SC-006**: Plain shell panes produce zero false-positive rows in a session with 10 panes and no agents.
- **SC-007**: For Claude Code and Kiro sessions started normally, the correct session id is shown on the row within 10 seconds of the session starting in at least 95% of tries, and a wrong id is never shown.
- **SC-008**: A user can rename an agent in under 10 seconds (right-click, type, Enter), and 100% of invalid or duplicate names are refused with a visible message.
- **SC-009**: Zero custom names are present after the app restarts.

## Assumptions

- The feature targets the gpui binary's sidebar; the wgpu binary's info overlay is out of scope (consistent with the attention layer's earlier decision).
- Scope is a single terminal window's workspaces; agents in other terminal windows or other applications are out of scope.
- The initial recognized agent list is Claude Code, Kiro, Codex, Antigravity and Copilot; others (Gemini CLI, OpenCode, etc.) are added by extending the list.
- Starting, stopping and messaging agents from the list are out of scope. The only actions are jumping to a pane, renaming, resetting the name and copying the session id. Inter-agent messaging is a separate, later feature that will use the custom names as addresses.
- Cwd is not shown in this version. Session ids are shown only when they can be determined (Claude Code and Kiro); Codex, Copilot and Antigravity show none until a reliable source exists.
- The session id is shown shortened to 8 characters; the full id is available through "Copy session id".
- No new always-visible indicator (count or badge) is added for when the sidebar is closed.
- Panes with plain shells are never listed, even if idle.
- Existing agent state reports (working, needs-input, done, idle) already reach the terminal and are reused unchanged; the agent type comes from process detection, not from the reports.
