# Research: Agents Sidebar Tab

## R1. How to find which panes run an agent
- **Decision**: One background `ps -axo pid=,ppid=,pgid=,tpgid=,comm=,args=` per scan. For each pane (shell pid known as `Terminal.child_pid`), take the pane's foreground process group (rows whose `pgid` equals the shell row's `tpgid`) plus direct children of the shell, and test each against the recognized-agent list.
- **Rationale**: Matches the clarified rule (foreground process and direct children only); `tpgid` gives the tty's foreground group without touching PTY fds; one process spawn per scan; same technique family as the existing `workspace_info` probe (`ps` + `lsof`).
- **Alternatives**: `tcgetpgrp` on the PTY master fd (needs fd plumbing through `Pty`, no gain); walking all descendants (would detect backgrounded jobs and tmux children, contradicts clarification Q4); libproc `proc_pidinfo` per pid (more syscalls, more unsafe code).

## R2. Matching a process to a recognized agent
- **Decision**: Match the basename of `comm`; when `comm` is a generic interpreter (`node`, `bun`, `deno`, `python*`), match the basename (and stripped extension) of the first non-flag script argument in `args`. Entries are `{ name, process = {names...} }`.
- **Rationale**: Claude Code and several CLIs run as `node` scripts, so `comm` alone misses them; this also keeps a renamed hostile binary out of scope only as far as process identity is trusted (FR-012/FR-017 trust process identity, not output text).
- **Alternatives**: full-argv regex (fragile, user-hostile config); matching executable path (breaks across install methods).

## R3. Default recognized list
- **Decision**: Defaults `claude` (Claude Code), `kiro-cli`/`kiro` (Kiro), `codex` (Codex), `copilot` (GitHub Copilot CLI), `agy`/`antigravity` (Antigravity). Verified 2026-11-06 on the dev machine: `claude`, `kiro-cli` (real exe `/Applications/Kiro CLI.app/Contents/MacOS/kiro-cli`), `codex`, `copilot` and `agy` are installed native binaries, so their argv[0] basename is the command name. `kiro` and `antigravity` are kept as aliases. Observed: a running Claude Code shows as `claude` with pgid == tpgid. Argv[0] paths containing spaces split on whitespace, which is harmless for matching because only the basename of the first token is compared.
- **Rationale**: Satisfies FR-015 without per-agent setup; unverified names are isolated in one table.

## R4. Extending the list without code changes
- **Decision**: `config.agents.recognized = { { name = "Gemini", process = { "gemini" } } }` in `notifications.lua`; entries are appended to the defaults (a same-`name` entry replaces the default). Whole-`Config` swap on reload applies it (constitution III).
- **Alternatives**: separate JSON file (extra loader, no hot reload story); replacing defaults entirely (users lose agents on upgrade).

## R5. Scan cadence and cost
- **Decision**: Scan every 2 s only while the sidebar is visible, the Agents section is active and the window is focused; also scan immediately when the section is selected or the sidebar opens (so the list is current on open, per edge case). No scan otherwise.
- **Rationale**: Meets the 5 s appear/disappear bound with margin; satisfies SC-005 (no cost when closed). The existing 15 s `workspace_info` cadence is too slow for FR-011.
- **Alternatives**: always-on 2 s timer (violates SC-005 spirit, wastes battery); kqueue process events (large, unsafe, overkill).

## R6. Where agent identity and state live
- **Decision**: Identity = scan result `terminal_id -> agent`; state/unread/message = `AttentionTracker` (already keyed by terminal id and fed by hooks). No IPC `Message` change.
- **Rationale**: FR-017 says reports never create rows, so the agent type need not travel over the socket; keeps the control protocol unchanged and backward compatible.
- **Gap**: `PaneAttention` keeps its timestamp private (`last_event_at`, used for rate limiting). Add a public `updated_at()` accessor (set on every applied event) for "time since last update".

## R7. Row removal
- **Decision**: Rows derive entirely from the latest scan intersected with live terminals; a gone process or closed pane drops out on the next scan (<=2 s) and stale tracker state is ignored for unlisted panes (clarification Q2). Pane close already calls `attention.remove`.

## R8. Layout: five section tabs
- **Risk**: `MIN_SIDEBAR_WIDTH_PX` (180) was sized so four labels fit; a fifth label ("Agents") will clip.
- **Decision**: Raise the minimum to fit five labels at 10.5 px (target 220, verify visually) and keep default width at 220. Alternative: shorten "Workspaces" to "Spaces" (rejected, changes existing UI).

## R9. Jump to pane
- **Decision**: Reuse `GpuiShellRoot::focus_terminal` (switches workspace/tab, focuses pane, unzooms); change its visibility to `pub(super)`. If it returns false (pane gone), drop the row on next render.


---
## Amendment 2026-11-06: session id and custom names

## R10. Session id source for Claude Code
- **Decision**: Use the `session_id` field that Claude Code puts in the JSON it sends to every hook (documented common field, with `transcript_path`, `cwd`, `hook_event_name`). `petruterm hook claude` already reads that payload; copy `session_id` into the socket message as a new optional `session` field. Also register the hook for `SessionStart` (in the injected `--settings` JSON and in `hooks setup claude`) so the id is known at launch and updates on `/clear` or resume, instead of waiting for the first prompt.
- **Fallback**: parse `--resume <id>`, `-r <id>` or `--session-id <id>` from the process command line during the scan.
- **Rationale**: no file reads in `~/.claude`, no transcript access (FR-019), and it uses the integration already shipped. Limitation: only panes with the hooks active (the bundled shim does this automatically).
- **Alternatives**: scanning `~/.claude/projects/*/*.jsonl` by mtime (guesses wrong with two sessions in one directory, touches conversation files); reading `~/.claude/sessions` (not confirmed to exist, and outside the project without permission).
- Source: Claude Code hooks reference (docs.claude.com/en/docs/claude-code/hooks).

## R11. Session id source for Kiro (OPEN: needs a verification spike)
- **Known**: Kiro's docs say sessions have UUID ids, are stored per directory in a local SQLite database under `~/.kiro/`, `kiro-cli chat --resume-id <ID>` resumes one, the id is printed on exit and by `/session-id`. Kiro's own draft (`agent-comm-draft.md`, from another machine, KAS 0.66.x) instead describes `~/.kiro/sessions/<hash>/sess_<uuid>/` directories with a `.lock` file holding `{"pid": N}`. The two descriptions differ.
- **Decision**: ship a chain, in this order: (1) hook payload if Kiro's hooks expose a session id (verify), (2) `--resume-id <id>` in the process arguments, (3) pid-to-session lookup in the on-disk store only if the layout is confirmed on this machine. Whatever is not confirmed stays out; Kiro then shows no id (FR-019, edge case "store unreadable").
- **Spike (task T041)**: with the user's permission, list file names (not contents of conversations) under `~/.kiro` and read only small lock/metadata files, while a Kiro session runs, to confirm which layout is real. Record the result here. SC-007 for Kiro depends on this outcome.
- **Risk**: if only the SQLite store exists, a lookup would need a SQLite dependency or the `kiro-cli chat --list-sessions` command (per-directory, no pid), which cannot tell two sessions in one directory apart. In that case Kiro shows an id only when it can be determined unambiguously (args or hook).

## R12. Which source wins, and when an id is stale
- **Decision**: store ids per `(terminal_id, agent pid)`. Priority: hook-reported > store lookup > command-line args. When the detection's pid changes or the row disappears, the stored id is dropped. A newer hook-reported id replaces the old one immediately (covers `/clear`).
- **Rationale**: ties the id to one running agent so a reused pane never shows the previous agent's id; implements Acceptance Scenario 4 of US5.

## R13. Custom name storage, validation, uniqueness
- **Decision**: `AgentNames` in the gpui chrome: `terminal_id -> { pid, name }`. Pure `validate_name(&str) -> Result<String, NameError>` in shared `term/agents.rs` (1-32 chars, `[A-Za-z0-9._-]`, trimmed). Uniqueness is checked case-insensitively against the other live entries. Entries are dropped when the detection disappears or its pid changes (FR-025). Not persisted (clarification).
- **Lookup for the future bus (FR-027)**: `terminal_for(name) -> Option<terminal_id>` and `name_of(terminal_id)`. No other consumer is built now.
- **Alternatives**: persisting by session id (rejected by the user: pane lifetime only); storing the name in `PaneAttention` (mixes concerns, and the tracker outlives the agent process).

## R14. Right-click menu and inline rename
- **Decision**: reuse the existing gpui context menu (`gpui_shell/context_menu.rs`: pixel position + `ContextMenuItem` list + `ContextAction` dispatch). Add `AgentRename(terminal_id)`, `AgentCopySessionId(terminal_id)`, `AgentResetName(terminal_id)` to the shared `ContextAction`; build the item list in a new `agent_menu.rs` so `context_menu.rs` (262 lines) barely grows. Rename editor reuses `text_input::TextInput` exactly like `rename.rs` does for workspaces, with one difference: a refused name keeps the editor open and shows the message under the row instead of closing.
- **Click elsewhere cancels** (FR-021): wrap the editor row in `on_mouse_down_out`, as the context menu already does.
- **Clipboard**: `cx.write_to_clipboard(ClipboardItem::new_string(id))` as the existing Copy action does.

## R15. Process identity needs the agent pid
- **Decision**: `AgentDetection` gains `pid` (the matched process) and an optional `session` parsed from the command line during `detect` (only the id token is kept; `args` is still never stored, logged or shown). All new keys use `(terminal_id, pid)`.
