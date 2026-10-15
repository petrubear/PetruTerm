# Agent Attention Layer - Design

Status: APPROVED (2026-10-15). Slices 1, 2 and 3 IMPLEMENTED (OSC/BEL signals, tab dot, pane
ring, jump to unread, native notifications, `agents` config, control socket, `notify` / `hook` /
`hooks setup` subcommands, `claude` shim). Slices 2 and 3 were re-planned after reading the cmux
sources (see "cmux findings" and "Delivery slices"). Slice 4 IMPLEMENTED for the gpui sidebar
(`src/gpui_shell/workspace_info.rs`); the wgpu sidebar (an info overlay, not a row list) is not
enriched.

Implementation deviations: panes are identified on the socket by a process-unique `pane_uid`
(`PETRUTERM_PANE_ID`), not the terminal id, because the wgpu build allocates terminal ids after
spawning the shell. Hook state messages are not rate limited (OSC notifications still are).
Shim ordering: macOS `path_helper` and user rc files reorder PATH in login shells (verified: on
the dev machine `~/.local/bin` precedes the shim). Solved with shell integration
(`src/ipc/shell.rs`): zsh gets a generated `ZDOTDIR` whose `.zshenv` restores the user's
`ZDOTDIR`, sources their `.zshenv` and re-prepends the shim dir in a one-shot `precmd` hook;
bash gets a `PROMPT_COMMAND` prefix. Other shells (fish) rely on `hooks setup claude`.

## Goal

Make PetruTerm a good host for CLI coding agents (Claude Code, Codex, OpenCode, Gemini CLI)
running as ordinary processes in panes, in the style of cmux. No agent protocol is involved:
the agent runs in a normal PTY. We add an observation layer that tells the user which pane
needs them, and a way for agents to tell the terminal what they are doing.

This is independent of the ACP backend (our own chat panel as an ACP client).

## What the code does today (verified)

| Signal | Path | Result |
|---|---|---|
| Bell (BEL) | alacritty `Event::Bell` -> `PtyEvent::Bell` ([pty.rs:96](../../../src/term/pty.rs)) | Dropped: wgpu `Mux::poll_pty_events` has `PtyEvent::Bell => {}` ([mux/mod.rs:400](../../../src/app/mux/mod.rs)); gpui `poll.rs` drains it with no action |
| Title (OSC 0/2) | `PtyEvent::TitleChanged` | wgpu only `log::debug!`s it; gpui drains it |
| OSC 9 / 99 / 777 | `Osc133Scanner` ([osc133.rs](../../../src/term/osc133.rs)) classifies every non-133 OSC as `OscOther` and discards it; alacritty's parser ignores them too | Not surfaced anywhere |
| OSC 133 | `Osc133Scanner` in `reader_loop` ([pty.rs:482](../../../src/term/pty.rs)) -> `PtyEvent::Osc133` | Used for blocks / shell context |
| Native notification | `platform::notifications::send(body)` | One fixed identifier (`petruterm.toast`) so notifications replace each other; fixed title; no pane association. Called from wgpu `app_state.rs::dispatch_notification` |
| Per-pane state | `Tab { id, title, accent_color }` ([ui/tabs.rs](../../../src/ui/tabs.rs)) | No attention/agent state anywhere |
| Pane env | `TERM`, `COLORTERM`, `TERM_PROGRAM=PetruTerm` only ([pty.rs:431](../../../src/term/pty.rs)) | No pane id / socket path for a child process to call back |

Both binaries drain `Pty::rx` separately (`mux/mod.rs` for wgpu, `gpui_shell/poll.rs` for
gpui), so the event wiring must be done twice or moved into shared code.

## Design

### 1. Attention state (shared core)

New module `src/term/attention.rs` (shared by both binaries, no UI deps).

```rust
pub enum AgentState { Idle, Working, NeedsInput, Done }

pub struct PaneAttention {
    pub state: AgentState,
    pub unread: bool,          // set when an event arrives while pane is not focused / window inactive
    pub last_message: Option<String>,   // sanitized, <= 200 chars
    pub updated_at: Instant,
}
```

Pure state machine: `apply(AttentionEvent, pane_focused: bool)`; `clear_on_focus()`. Unit
testable without a window. Aggregation helpers: `tab_attention(tab)` and
`workspace_attention(ws)` return the highest-priority state (NeedsInput > Done > Working > Idle)
and whether anything is unread.

`AttentionEvent` sources, in priority order:
1. Explicit hook / CLI event (state + message) - section 3.
2. OSC 9 / 99 / 777 notification from the PTY - section 2. Maps to `NeedsInput` with the
   text as message.
3. Bell - maps to `NeedsInput` only when `agents.bell_attention = true` (default false, see
   open questions).

### 2. PTY parsing: OSC notifications

Extend the existing byte scanner (it already runs once per byte in `reader_loop`, so no extra
pass) to capture bodies for OSC 9, 99 and 777 in addition to 133, and emit a new
`PtyEvent::Notify { title: Option<String>, body: String }`.

- OSC 9: `ESC ] 9 ; <text> ST`. Ignore the ConEmu progress form (`9;4;...`).
- OSC 777: `ESC ] 777 ; notify ; <title> ; <body> ST`.
- OSC 99 (kitty): start with the simple single-chunk form; ignore multi-part (`d=0`) payloads
  at first and log them at debug.
- Body capped at 4 KiB while scanning; extra bytes discarded.
- `osc133.rs` is 262 lines; if the extension pushes it past 400, split the scanner into
  `term/osc.rs`.

The text comes from the PTY, so it is untrusted (anything that prints a crafted sequence,
e.g. `cat` on a hostile file, can fire it). Sanitize before display: strip control chars,
cap at 200 chars, rate-limit per pane (e.g. max 1 notification per second, drop the rest).

### 3. Control socket + `petruterm notify`

This is how agent hooks talk to the terminal.

- Each running instance listens on a Unix socket in `~/.local/state/petruterm/` (dir 0700,
  socket 0600), named by pid. tokio `UnixListener`, newline-delimited JSON.
- Each pane's shell gets two new env vars at spawn: `PETRUTERM_PANE_ID` and
  `PETRUTERM_SOCKET`. Both children and agent hooks inherit them.
- Message: `{ "pane": <id>, "state": "working|needs-input|done|idle", "title": "...", "body": "..." }`.
  Unknown pane ids are ignored. The payload is data, never executed.
- CLI: `petruterm notify [--state S] [--title T] [--body B]` as a subcommand dispatched early
  in `main` of both binaries (shared client function in the lib crate). If
  `PETRUTERM_SOCKET` / `PETRUTERM_PANE_ID` are unset it exits 0 silently, so hooks are safe
  to leave configured when running outside PetruTerm.

Slice 2 also adds `petruterm hook <agent> <event>`: the entry point agent hooks call. It reads
the hook's JSON payload from stdin (Claude Code passes one), maps the event to a state, and
sends the same socket message as `notify`. Same silent exit 0 outside PetruTerm. The mapping
lives in Rust (testable), not in the shell wrapper.

Event mapping for Claude Code:

| Hook event | State | Message |
|---|---|---|
| `UserPromptSubmit`, `PreToolUse` | `working` | none |
| `Notification` | `needs-input` | payload `message` |
| `Stop`, `SubagentStop` | `done` | none |
| `SessionEnd` | `idle` | none |

`AgentState::Working` and `Done` (currently `#[allow(dead_code)]`) become live in this slice.
Unread/notification rules: `needs-input` and `done` raise the dot, ring and native
notification when the pane is not observed; `working` and `idle` only update state (and clear
the unread flag of that pane, since the user just acted).

### 4. Zero-setup agent wrapper (slice 3), and explicit installer

Modeled on cmux (see "cmux findings"): the user installs nothing by hand.

**Wrapper.** The app bundle ships `Resources/bin/claude` (a small POSIX shell script). Each
pane's PATH gets that directory prepended at spawn. The script:

1. If `PETRUTERM_PANE_ID` is unset, or the user already passed `--settings`, or the real
   `claude` cannot be found: `exec` the real `claude` unchanged.
2. Otherwise finds the real `claude` by removing its own directory from PATH, and `exec`s it
   with `--settings '<json>'`, where the JSON registers hooks (`UserPromptSubmit`,
   `PreToolUse`, `Notification`, `Stop`, `SubagentStop`, `SessionEnd`) running
   `petruterm hook claude <event>`.
3. Claude Code merges `--settings` with the user's own `settings.json`; nothing on disk is
   modified.

Deliberately minimal compared with cmux's 1,669-line wrapper: the JSON is a static string (no
`node` dependency), and a user-supplied `--settings` disables injection instead of merging.
Merge support is a later improvement only if users hit it.

`preferredNotifChannel` is NOT overridden: with `TERM_PROGRAM=PetruTerm` Claude Code emits no
OSC notification by default, so there are no duplicates. If a user opted in to OSC/bell
manually (slice 1), the attention tracker's 1 event/sec/pane limit absorbs the overlap.

**Explicit installer.** `petruterm hooks setup <agent>` writes the same hooks persistently to
the agent's own config, for users who run the agent outside PetruTerm's PATH shim or whose
shell rc reorders PATH. Shows the diff and writes a backup first; never automatic.

Agents other than Claude Code (Codex `notify` setting, OpenCode plugin/event hooks, Gemini
CLI): NOT verified. Confirm each mechanism from its docs before adding it; they reuse
`petruterm hook <agent> <event>` and the installer.

### 5. UI

Both binaries (wgpu chrome and `gpui_shell`):

- Tab pill: colored dot for `NeedsInput` (accent) and `Done` (muted) when unread.
- Pane: ring (border color override) on a pane that needs input, cleared on focus.
- Workspace sidebar row: dot plus the last message line.
- Jump to next unread: `Leader u` in the tmux style, `Cmd+Shift+U` in the normal style.
- Native notification per pane: title = tab title, body = message, identifier
  `petruterm.pane.<id>` so panes do not overwrite each other. Suppressed when that pane is
  focused and the window is active. Clicking it focuses the pane (phase 2).
- Optional: Dock badge with the unread count.

### 6. Config (`notifications.lua`)

```lua
config.agents = {
    notifications = true,   -- native/toast notification on needs-input / done
    ring          = true,   -- pane ring + tab dot
    bell_attention = false, -- treat BEL as needs-input
}
```

## Delivery slices

1. **Core + PTY signals (no socket):** `attention.rs` state machine, OSC 9/99/777 scanner,
   `PtyEvent::Notify`, Bell wiring in both pollers, tab dot + pane ring, jump-to-unread,
   per-pane native notifications, config keys. Fully unit-testable (scanner cases, state
   machine, aggregation).
2. **Socket + CLI** (re-planned):
   - 2a. Shared client + message types in the lib crate (`src/ipc/`): serialize/parse the
     newline-delimited JSON, `send()` that exits quietly when env vars are missing. Pure and
     unit-tested (malformed lines, oversized lines, unknown fields, unknown pane id).
   - 2b. Listener in each binary (tokio `UnixListener`, 0600 socket in a 0700 dir, one socket
     per pid, removed on exit, stale sockets from dead pids cleaned at startup). Messages are
     forwarded to the main loop through the same `attention_events` channel as slice 1
     (new `AttentionEvent::State { state, message }` variant), so the tracker, UI and
     notification code are reused.
   - 2c. Pane env vars (`PETRUTERM_PANE_ID`, `PETRUTERM_SOCKET`) set in `pty.rs` spawn, plus
     the PATH shim directory prepend (empty until slice 3 ships a wrapper).
   - 2d. `petruterm notify` and `petruterm hook <agent> <event>` subcommands in both binaries,
     with the Claude Code event mapping above. An end-to-end test: spawn a real PTY pane, run
     `petruterm notify` inside it, assert the tracker state changes.
3. **Claude Code wrapper + installer** (re-planned): `Resources/bin/claude` wrapper script
   (bundled by `scripts/bundle.sh` for both `.app` bundles), PATH prepend wired to the bundle
   location (skipped in unbundled dev builds unless `PETRUTERM_BIN_DIR` is set), shell-level
   test of the wrapper (fake `claude` that echoes its argv; assert `--settings` injected, and
   skipped when `--settings` is present or `PETRUTERM_PANE_ID` is unset), and
   `petruterm hooks setup claude` with diff and backup.
4. **Sidebar enrichment:** per-workspace branch and PR status (`gh`), listening ports
   (`lsof`), last notification line.
5. **Later, separate designs:** richer control API (create splits, send keys, read screen) so
   an agent can orchestrate panes and teammates can appear as splits; per-agent session
   resume (record the agent's native session id per pane and relaunch with its resume flag).

## Decisions (2026-10-15)

1. **Bell as attention:** default OFF (`agents.bell_attention = false`). Revisit a
   "foreground process is not the shell" heuristic later if wanted.
2. **`notify` CLI:** a subcommand of both binaries sharing one client function in the lib
   crate; no third bundled binary.
3. **Delivery:** slices ship incrementally (1, then 2, then 3), each mergeable on its own.
   Slice 1 only helps agents that emit OSC 9/99/777; hook-driven state arrives with 2 and 3.
4. **gpui native notifications:** verified that gpui never calls
   `platform::notifications::send` (only the wgpu `app_state.rs` does; gpui has an in-window
   toast in `gpui_shell/toast.rs`). Slice 1 must wire native notifications into gpui too,
   honoring `config.notifications.style`.

## Getting agents to emit it (verified against Claude Code docs, 2026-10-15)

Claude Code sends a desktop notification on its own only in Ghostty, Kitty and iTerm2; in any
other terminal (`TERM_PROGRAM=PetruTerm` included) it emits nothing by default. Until the hook
installer (slice 3) exists, the user must opt in manually, either:

- `"preferredNotifChannel": "terminal_bell"` in `~/.claude/settings.json` plus
  `agents.bell_attention = true`; or
- a `Notification` hook whose command prints an OSC 9 sequence to the pane's tty, e.g.
  `printf '\033]9;Claude Code needs your attention\007' > /dev/tty` (expected to work, not yet
  tested end to end with a real Claude Code session).

Lead for later: Claude Code v2.1.295+ also reports working / waiting / done status to terminals
that implement the OSC 7501 "Program Status Protocol", with no hook needed. Not read or
implemented yet.

## cmux findings (read from the cmux sources, 2026-10-15)

cmux does not make the user install hooks for Claude Code. It does three things:

1. **Identity spoof.** Sets `TERM_PROGRAM=ghostty` so Claude Code uses its own notification
   path (cmux is built on libghostty). Rejected for PetruTerm: other tools key off that value
   (graphics, shell integration) and PetruTerm is not Ghostty.
2. **Per-pane environment.** `CMUX_SURFACE_ID`, `CMUX_WORKSPACE_ID`, `CMUX_PANEL_ID`,
   `CMUX_TAB_ID`, `CMUX_SOCKET_PATH`, ... Equivalent to our `PETRUTERM_PANE_ID` /
   `PETRUTERM_SOCKET`.
3. **A `claude` wrapper on the pane PATH** (`Resources/bin/cmux-claude-wrapper`). When
   `CMUX_SURFACE_ID` is set it injects `--session-id` and a `--settings` JSON that registers
   hooks (`SessionStart`, `Stop`, `Notification`, `UserPromptSubmit`, `PreToolUse`,
   `PostToolUse`, `PermissionRequest`, `SubagentStop`, `SessionEnd`) calling
   `cmux hooks claude <event>` over the socket. It also sets `preferredNotifChannel` to
   `notifications_disabled` so hooks are the only source. Outside cmux it passes straight
   through. It needs `node` to merge the user's own `--settings`.

Plain OSC 9/99/777 and `cmux notify` still work for any other program; cmux suppresses them
for panes with a hooked agent to avoid duplicates. `cmux hooks setup` is the persistent
alternative for other agents (e.g. Codex).

Adopted here: pane env vars, socket, `hook` subcommand, PATH wrapper with `--settings`
injection, explicit installer. Not adopted: TERM_PROGRAM spoof, disabling Claude's OSC channel
(unneeded, see section 4), OSC suppression per pane (the rate limit is enough for now),
node-based settings merge.

## Risks

- Wrapper reliability: a user's shell rc can re-prepend its own PATH entries after the pane
  spawns, putting the real `claude` ahead of the wrapper (mitigation: `hooks setup` as the
  persistent fallback; document it). A `--settings` flag from the user disables injection.
- Claude Code's handling of repeated `--settings` flags and the hook payload schema have
  changed between versions; the wrapper and the `hook` parser must tolerate unknown fields and
  a missing stdin payload.
- Untrusted PTY text in notifications (mitigated: sanitize, cap, rate-limit).
- Socket exposure (mitigated: 0600 socket in a 0700 dir, ignore unknown pane ids, no command
  execution from payloads).
- Writing another tool's config (mitigated: explicit command, diff, backup).
- Two event-drain loops (wgpu mux and gpui poll) drifting apart; prefer putting the
  attention update in shared code called from both.
