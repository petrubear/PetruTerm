# Research: Agent-to-Agent Messaging

## R1. Delivery mechanism (verified before planning)
- **Decision**: write the framed text to the target pane's PTY through `Terminal::write_input`: wrapped in bracketed paste (`ESC[200~ … ESC[201~`) when `bracketed_paste_mode()` is on, plain text otherwise, then `\r` to submit. One write for the paste and one for Enter is enough; a single combined write also worked.
- **Evidence (2026-11-06, real sessions in a pty harness)**: Kiro (`kiro-cli chat`) and Claude Code both enable bracketed paste, accept a multi-line paste as one block, submit it as a user turn and answer it; both queue a message that arrives while busy ("1 message queued" / "Press up to edit queued messages") and answer it after the current turn. Paste plus Enter in a single write also worked.
- **Consequence**: no per-agent adapter and no busy detection in v1.
- **Fallback when paste mode is off** (plain shell, or an agent that did not enable it): newlines are collapsed to spaces so a multi-line message cannot submit early.

## R2. Round trip needs an imperative reply instruction (verified)
- **Evidence**: with "reply with: petruterm msg …" Kiro answered in text and only printed the command; with "Answer it by RUNNING this shell command yourself (do not just print it): …" it ran it and the answer reached Claude, which reported it. Kiro accepted `--trust-tools=execute_bash,shell` and ran the stand-in command without a prompt.
- **Decision**: the frame always carries the imperative instruction (FR-021) and `petruterm agents setup` installs standing instructions as a second layer (FR-019). Claude also judges whether a reply is expected (it declined to answer a message it read as the user's request to report to the user), so the frame says explicitly when no reply is expected.

## R3. Request/response over the control socket
- **Problem**: the socket is one-way today: `handle_connection` queues parsed `Message`s for the UI thread and never writes back. The CLI needs an outcome (SC-001, FR-002).
- **Decision**: add a request path. A line carrying a `msg` payload is parsed on the connection thread into `MsgRequest { peer_pid, pane_hint, to, text, no_reply, reply: SyncSender<MsgReply> }`, pushed on a separate bounded channel (cap 64), and the thread waits up to 2 s on the one-shot for the UI thread's reply, then writes one JSON line back. The UI thread drains the channel each poll tick (33 ms) and replies after delivering.
- **Build without a handler (wgpu)**: a `OnceLock` flag set by the gpui build marks "messaging available"; otherwise the server answers `unsupported` immediately, so the CLI never waits.
- **Alternatives**: a second socket (more state and cleanup); fire-and-forget with a result file (races, polling); making the existing queue bidirectional (changes hook semantics).

## R4. Sender identity (not spoofable from the command line)
- **Decision**: take the peer pid of the connection (macOS `getsockopt(SOL_LOCAL, LOCAL_PEERPID)`), walk its parent chain with the same `ps`-style table the scan already builds (or `proc_pidinfo`), and accept it as coming from pane P only if the chain reaches P's shell pid. `PETRUTERM_PANE_ID` from the environment is a hint and must agree; a mismatch or an unresolvable chain is refused as "sender unknown".
- **Rationale**: FR-003. Any process of the same user can open the 0600 socket and write any `pane` number; the peer pid cannot be forged by the caller.
- **Limit**: a process that double-forks away from its ancestors (daemonized tools) is refused; acceptable for agents running shell commands as children.
- **Alternative rejected**: trusting `PETRUTERM_PANE_ID` alone (any process could impersonate another agent).

## R5. Sender display and replies
- A named agent: framed with its custom name and agent type, with the reply command naming it. An unnamed agent or a plain shell: framed as "an unnamed agent / shell in `<workspace / tab>`", marked as not answerable (no usable name), and the reply instruction is omitted. The user: framed as from the user.
- An agent can only be answered if it has a custom name (spec 001), which is why the setup instructions tell agents to ask the user to name them.

## R6. Acceptance (opt-in)
- **Decision**: `AgentAccept` in the chrome, keyed by `(terminal_id, agent pid)`, default off, in memory, dropped by `retain_live` when the process goes or changes (same rules as custom names). Toggle from the agent menu. User messages from the menu bypass it (FR-014).
- Stored apart from `AgentNames` so naming and receiving stay independent.

## R7. Liveness and scanning when the sidebar is closed
- **Problem**: spec 001's scan only runs while the Agents section is visible and the window focused, but messages arrive any time and agents run while the window is unfocused.
- **Decision**: keep scanning (same 2 s interval, same single `ps`) while messaging is in use, defined as any custom name or acceptance being set, regardless of sidebar or focus. At delivery also check the target process with a non-blocking `kill(pid, 0)`; staleness is bounded by 2 s and by that check. Users who never name or accept an agent pay nothing.

## R8. Sanitizing message text
- **Decision**: reject nothing silently except by length; remove all control characters (C0 including ESC and DEL, C1 range, and CR) from the text, keeping `\n` only when bracketed paste is on (otherwise `\n` becomes a space). This makes `ESC[201~` (early paste end) and every escape sequence unrepresentable (FR-008, SC-004). Applies to the sender label too (custom names are already address-safe).
- **Limit**: 4000 characters counted after sanitizing; longer is refused (FR-009).

## R9. Limits and conversations
- **Rate**: sliding window per (sender terminal, target terminal): at most 10 accepted messages in 60 s; the refusal carries `retry_after` seconds (FR-015).
- **Conversation** (revised from "chain of replies"): agents run the command themselves and cannot be relied on to mark replies, so a conversation is the messages between the same unordered pair with no gap over 2 minutes; the 11th is refused ("conversation limit reached"). A message from the user to either agent resets the pair's conversation (FR-014, FR-016). State is in memory and pruned when idle.
- **Alternative rejected**: carrying a reply token in the command (`--re ID`): agents would have to cooperate, and a missing token would disable the guard.

## R10. Message text is never persisted
- No file writes and no log lines containing message text (FR-018). Logging records outcomes and counts only (e.g. "msg delivered cterm->kterm 112 chars"). Names are logged only if already address-safe.

## R11. Instruction files
- **Claude**: personal skills live at `~/.claude/skills/<name>/SKILL.md` with YAML frontmatter (`name`, `description`) and a markdown body; the description tells Claude when to load it. Source: Claude Code skills docs (code.claude.com/docs/en/skills).
- **Kiro**: global steering files live in `~/.kiro/steering/*.md` and apply to the CLI and IDE; default inclusion is "always". Source: kiro.dev/docs/steering. **Caveat**: GitHub issues report global steering not always loading in some versions, which is why the frame carries the instruction too (FR-021).
- **Decision**: `petruterm agents setup` writes `~/.claude/skills/petruterm-messaging/SKILL.md` and `~/.kiro/steering/petruterm-messaging.md` from templates in `config/default/agent-messaging/`, each containing a marker line. It shows what it will do, backs up an older file that carries the marker as `<file>.petruterm-bak` before updating it and leaves any file without the marker alone (reported, exit 1), does nothing when the content already matches, and `remove` deletes only files that carry the marker. Nothing is installed automatically (spec Assumptions).

## R12. Wording of the delivered message
- Verified phrasing from the round trip (R2) with additions: states it was sent by another agent and not the user, names the sender, gives the reply command, says when no reply is expected, and ends with `---` before the body. Exact text in `contracts/delivered-frame.md`.

## Open items to verify during implementation
- `LOCAL_PEERPID` availability through the `libc` crate on the pinned version (fallback: a tiny `extern "C"` declaration of `getsockopt` with the constants).
- Behavior of Kiro and Claude when two messages arrive back to back (expected: both queue); covered by the quickstart.
