# Agent Attention Layer - Design

Status: APPROVED (2026-10-15), decisions below resolved. Nothing implemented yet.

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

### 4. Hook installer (`petruterm hooks setup <agent>`)

Explicit command only, never automatic. Shows the diff and writes a backup before touching
the agent's own config.

- Claude Code (`~/.claude/settings.json`): `UserPromptSubmit` -> `--state working`,
  `Notification` -> `--state needs-input --body "$message"`, `Stop` -> `--state done`.
- Codex (`notify` program setting in its config) and OpenCode (plugin/event hooks): NOT
  verified. Confirm the current hook mechanisms from their docs before implementing; ship
  Claude Code first.

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
2. **Socket + CLI:** listener, env vars, `petruterm notify`, malformed-input tests.
3. **Hook installer** for Claude Code.
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

## Risks

- Untrusted PTY text in notifications (mitigated: sanitize, cap, rate-limit).
- Socket exposure (mitigated: 0600 socket in a 0700 dir, ignore unknown pane ids, no command
  execution from payloads).
- Writing another tool's config (mitigated: explicit command, diff, backup).
- Two event-drain loops (wgpu mux and gpui poll) drifting apart; prefer putting the
  attention update in shared code called from both.
