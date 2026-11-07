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
