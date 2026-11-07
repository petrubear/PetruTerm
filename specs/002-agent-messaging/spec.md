# Feature Specification: Agent-to-Agent Messaging

**Feature Branch**: `002-agent-messaging`

**Created**: 2026-11-06

**Status**: Draft

**Input**: User description: "Agent-to-agent messaging in PetruTerm. From one agent running in a pane (for example the agent named cterm) send a message to another agent by its custom name (for example kterm), so the message arrives as a normal user turn in the target agent and the target can answer back the same way."

## Background (feasibility verified 2026-11-06)

Tests with real Claude Code and Kiro sessions showed that text typed into an agent's pane as a paste followed by Enter arrives as a normal user turn in both; both agents queue a message that arrives while they are busy and handle it afterwards; and a full question/answer round trip between a Claude agent and a Kiro agent worked once the delivered message told the receiver to *run* the reply command rather than only describe it. This feature builds on the custom agent names from `specs/001-agents-sidebar-tab` (address-safe, unique, lasting while the agent runs in its pane).

## User Scenarios & Testing *(mandatory)*

### User Story 1 - An agent sends a message to another agent by name (Priority: P1)

As a user running several agents, I want one agent (for example `cterm`) to be able to send a message to another agent by its custom name (for example `kterm`) using a plain command, so agents can ask each other things without me copying text between panes.

**Why this priority**: This is the whole point of the feature; everything else supports it.

**Independent Test**: With agents named `cterm` and `kterm` running, ask the agent in `cterm` to run the send command addressed to `kterm`; confirm the message appears in `kterm` as a submitted user turn and the command printed a delivery confirmation.

**Acceptance Scenarios**:

1. **Given** agents named `cterm` and `kterm`, where `kterm` is accepting messages, **When** a command in `cterm`'s pane sends "What is 7*6?" to `kterm`, **Then** the text arrives in `kterm`'s pane as a submitted user turn and the command reports that it was delivered.
2. **Given** the send command is run in a pane, **When** the message arrives, **Then** the receiver is told which agent sent it, determined by the pane the command ran in and not by anything typed in the command.
3. **Given** a name that no running agent has, **When** the command runs, **Then** it reports that the name is unknown and nothing is sent anywhere.
4. **Given** the target agent is busy answering something else, **When** the message is sent, **Then** it is still delivered and the command reports delivered (the agent handles it after its current work).
5. **Given** the command is run outside PetruTerm (no pane context), **When** it runs, **Then** it reports that it cannot send from here and exits without error output to other panes.
6. **Given** a command in `cterm` addressed to `cterm` itself, **When** it runs, **Then** it is refused with a clear reason.

---

### User Story 2 - The receiver can answer back the same way (Priority: P1)

As a user, I want the receiving agent to know the message came from another agent, know who sent it, and know how to answer, so a two-agent conversation works without my help.

**Why this priority**: A one-way message is much less useful; the round trip is what makes agents collaborate.

**Independent Test**: Send a question from `cterm` to `kterm`; confirm `kterm` answers by sending a message back to `cterm`, which arrives in `cterm` the same way, and `cterm` can use it.

**Acceptance Scenarios**:

1. **Given** a delivered message, **When** the receiver reads it, **Then** it states plainly that it came from another agent (named, with its agent type) and not from the user, and it tells the receiver to answer by running the send command addressed to the sender.
2. **Given** `kterm` receives a question from `cterm`, **When** `kterm` answers by running the send command addressed to `cterm`, **Then** the answer arrives in `cterm` as a submitted user turn with the same framing, naming `kterm` as the sender.
3. **Given** a message that needs no answer (for example a status note), **When** it is sent with the "no reply needed" option, **Then** the delivered text says no reply is expected.
4. **Given** the sender's name changes or is removed after sending, **When** the receiver answers, **Then** the answer is refused as unknown name with a clear reason (no stale routing).

---

### User Story 3 - An agent only receives messages if it opted in (Priority: P2)

As a user, I want to decide per agent whether it accepts messages from other agents, because a message is text typed into that agent's input and could be abused.

**Why this priority**: Safety. Without it, any process in any pane could type into every agent. It is P2 because the feature works for a single trusted pair without it, but it must ship in the same release.

**Independent Test**: Send to an agent that has not opted in and confirm it is refused with a "not accepting" result; turn acceptance on from the agent's right-click menu and confirm the same message is now delivered.

**Acceptance Scenarios**:

1. **Given** a new agent, **When** it first appears, **Then** it is not accepting messages.
2. **Given** a non-accepting agent, **When** a message is sent to it, **Then** nothing is typed into it and the sender's command reports "not accepting" with the name.
3. **Given** an agent's right-click menu, **When** the user chooses "Accept messages", **Then** the agent accepts messages and its row shows an indicator that it is accepting; choosing it again turns acceptance off.
4. **Given** an accepting agent whose process is replaced by a different agent in the same pane, **When** the new agent starts, **Then** acceptance is off again.
5. **Given** acceptance is on, **When** the app restarts, **Then** acceptance is off (it is never remembered across restarts).

---

### User Story 4 - The user sends a message from the Agents list (Priority: P2)

As a user, I want to send a message to an agent from its right-click menu, so I can test routing, or nudge an agent, without an agent running the command.

**Why this priority**: It gives the user the same capability as agents and makes the feature testable by hand.

**Independent Test**: Right-click an accepting agent row, choose "Send message…", type text, press Enter, and confirm it arrives in that agent as a user turn framed as coming from the user.

**Acceptance Scenarios**:

1. **Given** an accepting agent row, **When** the user right-clicks it and chooses "Send message…", **Then** an inline editor opens on the row; Enter sends, Escape or clicking elsewhere cancels.
2. **Given** a message sent this way, **When** it arrives, **Then** it is framed as coming from the user (not from another agent), and it is delivered even if the agent has not turned on acceptance, because acceptance applies to agent-to-agent messages only.
3. **Given** an empty message or a message over the length limit, **When** the user presses Enter, **Then** it is refused with a visible one-line reason and the editor stays open.
4. **Given** the target agent exits while the editor is open, **When** the user presses Enter, **Then** the editor closes and nothing is sent.

---

### User Story 5 - Loops and floods are stopped (Priority: P2)

As a user, I want two agents that keep answering each other, or one agent that spams another, to be stopped automatically, so a mistake costs seconds and not a runaway bill.

**Why this priority**: Without limits, two eager agents can ping-pong forever.

**Independent Test**: Make two accepting agents answer each other repeatedly; confirm the exchange is cut off after a fixed number of messages with a clear refusal, and that a burst of messages from one sender is rate limited.

**Acceptance Scenarios**:

1. **Given** two agents that keep messaging each other with no pause longer than 2 minutes (one conversation), **When** the conversation reaches 10 messages, **Then** the next one is refused with "conversation limit reached" and the target does not receive it.
2. **Given** a sender that sends more than 10 messages to the same target within one minute, **When** it sends another, **Then** it is refused with "rate limited" and told when it can try again.
3. **Given** a conversation stopped by the limit, **When** the user sends a new message from the Agents list, **Then** a fresh conversation starts and messages flow again.
4. **Given** refused messages, **When** the sender looks at the command output, **Then** the reason is stated so the agent can stop retrying.

---

### User Story 6 - Agents learn how to use messaging (Priority: P2)

As a user, I want each supported agent to have standing instructions on how to answer inbound messages, so it does the right thing even if the delivered text is phrased differently or is skimmed.

**Why this priority**: The test showed the wording of the delivered message decides whether the receiver answers or only describes an answer. Standing instructions make it reliable.

**Independent Test**: Run the setup command for Claude Code and for Kiro; confirm an instruction is installed for each, and that an agent with it answers an inbound message by sending a reply message.

**Acceptance Scenarios**:

1. **Given** the user runs the setup command, **When** it finishes, **Then** an instruction exists for Claude Code (as a skill) and for Kiro (as a steering file) describing: how to recognize a message from another agent, that it is not from the user, how to answer by running the send command, and when not to answer.
2. **Given** existing agent configuration files, **When** setup runs, **Then** it shows what it will change, makes a backup of anything it modifies, and never overwrites the user's own content.
3. **Given** setup was already run, **When** it runs again, **Then** it changes nothing.
4. **Given** the user has not run setup, **When** messages are delivered, **Then** the delivered text itself still contains the reply instruction (standing instructions improve reliability, they are not required).
5. **Given** the user wants to remove the instructions, **When** they run the removal option, **Then** exactly what was installed is removed.

---

### Edge Cases

- **Target exits between lookup and delivery**: the command reports that the agent is gone; nothing is typed anywhere else.
- **Text with control or escape characters** (including terminal sequences meant to change how the text is received): they are removed before delivery; a message can never inject keystrokes or alter how it is received.
- **Message too long**: refused above 4000 characters with a clear reason; never truncated silently.
- **Multi-line message**: delivered as one message, not as several submitted turns.
- **Several messages in a burst to one agent**: each arrives in order; the agent queues them.
- **Message to an agent whose pane is zoomed, hidden, or in another workspace/tab**: delivered the same; the user does not need to see the pane.
- **Sender is a plain shell (no agent) in a pane**: allowed; the receiver is told it came from a shell in `<workspace / tab>` (or that pane's custom name if it has one).
- **Names compared case-insensitively**: `Kterm` and `kterm` address the same agent.
- **Two windows of PetruTerm**: only agents in the same window are reachable; other windows report unknown name.
- **Agent not running at all, or process detection lagging**: addressing uses the custom name that exists only while the agent is detected, so a stale name never routes.
- **Message content is not saved to disk or written to logs**; only counts and outcomes may be logged.
- **Hostile process in a pane tries to message every agent**: refused by acceptance (default off), rate limits and the per-sender cap.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The system MUST provide a command, usable as an ordinary shell command by any agent or by the user, that sends a text message to a running agent addressed by its custom name.
- **FR-002**: The command MUST report one of these results to its caller and exit accordingly: delivered, unknown name, not accepting, rate limited, conversation limit reached, message too long, agent gone, or not running inside PetruTerm.
- **FR-003**: The system MUST identify the sender from the pane the command runs in, and MUST ignore any sender identity supplied in the command itself.
- **FR-004**: Addressing MUST use custom names only, compared case-insensitively; an agent without a custom name cannot be addressed.
- **FR-005**: A delivered message MUST arrive in the target agent's pane as a single submitted user turn, including when the message has several lines.
- **FR-006**: A message sent while the target is busy MUST still be delivered; the system MUST NOT wait for the target to be idle.
- **FR-007**: The delivered text MUST state that it comes from another agent and not from the user, name the sender and its agent type, and tell the receiver how to answer by running the send command addressed to the sender; messages marked "no reply needed" MUST say so instead.
- **FR-008**: The system MUST remove control and escape characters from message text before delivery so the text cannot inject keystrokes or alter how the message is received.
- **FR-009**: Messages MUST be limited to 4000 characters; longer messages MUST be refused, not truncated.
- **FR-010**: A new agent MUST NOT accept messages from other agents until acceptance is turned on for it.
- **FR-011**: The user MUST be able to turn acceptance on and off per agent from the agent's right-click menu, and the row MUST show whether the agent is accepting.
- **FR-012**: Acceptance MUST end when the agent exits or a different agent takes the pane, and MUST NOT survive an app restart.
- **FR-013**: The user MUST be able to send a message to an agent from its right-click menu ("Send message…") using an inline editor on the row; Enter sends, Escape or a click elsewhere cancels, and invalid input shows a one-line reason without closing the editor.
- **FR-014**: Messages sent by the user from the menu MUST be framed as coming from the user, MUST be delivered even if the agent has not turned on acceptance for agent-to-agent messages, and MUST start a fresh conversation for the limits in FR-016.
- **FR-015**: The system MUST refuse more than 10 messages from one sender to one target within 60 seconds ("rate limited"), and MUST report when sending is possible again.
- **FR-016**: The system MUST treat messages between the same two agents, with no pause longer than 2 minutes between consecutive messages, as one conversation, and MUST refuse the 11th message of a conversation ("conversation limit reached"); a message from the user to either agent starts a new conversation.
- **FR-017**: A message to the sender's own agent MUST be refused.
- **FR-018**: Message text MUST NOT be written to disk or logs; outcomes and counts only.
- **FR-019**: The system MUST provide a setup command that installs standing instructions for Claude Code (a skill) and for Kiro (a steering file) explaining how to recognize an inbound agent message, that it is not from the user, how to answer by running the send command, and when no answer is needed.
- **FR-020**: Setup MUST show what it will change, back up any file it modifies, never overwrite the user's own content, do nothing when already applied, and offer an option to remove exactly what it installed.
- **FR-021**: Delivered messages MUST contain the reply instruction even when setup has not been run.
- **FR-022**: Existing Agents list behavior (rows, sorting, rename, session id, jump) MUST continue to work unchanged.
- **FR-023**: The ability to route a message from a sender to a target by custom name MUST be available through the same lookup the custom names already provide, so no second name registry exists.

### Key Entities

- **Message**: text (up to 4000 characters), sender, target name, optional "no reply needed" flag, the conversation it belongs to, outcome.
- **Sender identity**: the agent (custom name and agent type) or plain shell or user that sent a message, derived from where the command ran or from the menu.
- **Acceptance**: a per-agent on/off setting, default off, lasting only while that agent runs in its pane.
- **Delivery result**: one of the outcomes in FR-002, returned to the sender.
- **Conversation**: the messages between the same two agents with no pause longer than 2 minutes, with a count capped at 10 (it does not depend on agents marking their replies).
- **Standing instruction**: a Claude skill and a Kiro steering file describing how to handle inbound agent messages.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: A message sent between two accepting agents is typed into the target within 1 second of the send command being run, and the command returns its result within 1 second.
- **SC-002**: A full question and answer between a Claude Code agent and a Kiro agent completes without any manual step by the user in at least 9 of 10 attempts with the standing instructions installed, and in at least 7 of 10 with only the delivered text.
- **SC-003**: 100% of attempts to message an unknown name, a non-accepting agent, or oneself are refused with a distinct reason, and nothing is typed into any pane.
- **SC-004**: 0 control or escape sequences contained in a message reach the target's input unchanged, tested with a message that contains terminal control sequences, including one that would end the delivery early.
- **SC-005**: Two agents set to answer each other stop after at most 10 messages, and a burst from one sender is cut off after 10 messages per minute.
- **SC-006**: A user can send a message to an agent from the Agents list in under 10 seconds (right-click, type, Enter).
- **SC-007**: 100% of new agents start with acceptance off, and 0 settings or message contents are present after the app restarts.
- **SC-008**: Running setup twice produces no second change, and removal restores the files to their previous state.
- **SC-009**: With the feature idle (no messages), there is no measurable change in terminal responsiveness or in the Agents list's refresh behavior.

## Assumptions

- Scope is a single PetruTerm window; agents in other windows are not reachable.
- Claude Code and Kiro are the supported agents; others can be added later by the same mechanism.
- Messages are text only (no files or images), up to 4000 characters.
- Delivery confirmation means "typed into the target and submitted"; it does not mean the target understood or answered.
- Receiving agents queue messages that arrive while busy (verified for Claude Code and Kiro); no busy detection is needed.
- Acceptance applies to messages from agents and shells; messages the user sends from the Agents list are always delivered, because the user is the one acting.
- Custom names, their uniqueness and their pane-lifetime come from `specs/001-agents-sidebar-tab`; this feature adds no second naming scheme and does not persist anything.
- The user decides whether to run setup; nothing is installed into agent configuration automatically.
- No conversation history or message archive is kept; a message exists only as the text typed into the target.
- The conversation limit counts messages in a conversation (10), and the rate limit counts messages per sender and target per minute (10); both numbers can be tuned later without changing the behavior described here.
