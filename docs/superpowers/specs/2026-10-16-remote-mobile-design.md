# Remote access from a phone (Android) - design

Status: draft, 2026-10-16. Nothing implemented.

## Goal

Use the desktop PetruTerm from a phone, like cmux's mobile app or Claude's `/rc`:
see the workspaces and panes, read live output, type into a pane, and get told when an
agent finishes or needs input. The desktop runs the real shells; the phone is a client.

Non-goals: running PetruTerm natively on Android; exposing the desktop to the open
internet by default; replacing the local control socket (it stays local-only).

## What cmux does (docs only)

- Iroh QUIC transport with managed relays; the endpoint id is the peer identity.
- QR pairing, backend-signed grant binding two devices.
- Versioned delta sync of the workspace list; terminal bytes over a separate lane.
- Early snapshots were plain text with no colors; styled cells came later.

Too heavy for a first version. We take the shape (host streams, client attaches,
pairing grant, attention push) and defer the transport.

## Architecture

```
phone (web client / Android app)
      |  WebSocket over TLS (LAN / Tailscale), later relay
      v
src/remote/  (lib crate, shared by both binaries)
  server   - listener, TLS, auth, one task per client
  session  - per-client subscriptions, resize policy, input policy
  snapshot - alacritty grid -> ANSI
  pair     - QR payload, device keys, revocation
      |
      v
pane tee (Pty reader_loop)  +  write_input  +  attention events
```

`src/remote/` mirrors `src/ipc/`: pure logic in the lib, each binary starts it with a
wakeup callback. A tokio runtime already exists for LLM work; the server runs there.

## Host side

### Output tee and late join

Late joiners must get the current screen, then live bytes, with no gap and no
duplicate. Replaying recent bytes is wrong for TUIs (Claude Code is one).

Approach: the reader thread owns the subscriber list.

1. Client asks to attach to `pane_uid`. A `Subscribe{tx, size}` command is sent to that
   pane's reader thread over a channel.
2. `reader_loop`, after its next `processor.advance` (term lock held), drains pending
   subscribe commands: serializes the grid to ANSI (`snapshot`), sends it on `tx`, and
   adds `tx` to the list.
3. Every later chunk is sent to all subscribers after `advance`.

Snapshot and stream are ordered by construction because both are produced by the one
thread that mutates the grid. Per-subscriber channels are bounded; a slow client is
dropped (and can re-attach to get a fresh snapshot), never blocks the PTY.

### Snapshot (`snapshot.rs`)

Serialize visible grid plus enough state to resume:
cells with fg/bg/attrs (SGR runs), wide chars, cursor position and shape, alternate
screen flag, scroll region, bracketed paste / mouse / application cursor modes, window
title. Scrollback is not sent initially; the phone can request ranges later.

Test by round trip: feed the snapshot into a fresh alacritty `Term` and compare grids.

### Input

Client bytes go through `Terminal::write_input`. Policy per device: read-write or
read-only. Paste is sent as bracketed paste if the pane enabled it.

### Resize

A phone is about 40 columns wide. Options:

- Mirror (default): the pane keeps the desktop size; the client scrolls and zooms.
  No effect on the desktop.
- Fit: resize the PTY to the phone while attached, restore on detach (tmux style).
  Opt-in per attach, because it reflows the desktop view.

### Workspace state

`list` returns workspaces, tabs and panes (pane_uid, title, cwd, agent state, unread).
Changes are pushed as small events (`pane_added`, `pane_removed`, `title`, `state`).
Full versioned delta sync (cmux v2) is not needed until there are many clients.

### Attention

The attention layer already produces `AttentionEvent::State` per pane. The remote
session forwards `needs-input` and `done` to attached or subscribed clients. For a
phone that is not connected, phase 3 adds push (ntfy or UnifiedPush; no cloud account).

## Protocol

WebSocket, binary frames, one connection per client, multiplexed by pane:

- Control: JSON text frames (`hello`, `list`, `attach`, `detach`, `resize`, `event`).
- Data: binary frames `[pane_uid u64][kind u8][payload]` with kind = snapshot, output,
  input.
- Heartbeat every 15s; the client reconnects and re-attaches (gets a fresh snapshot).

Versioned `hello` so old clients fail cleanly.

## Security

This is shell access over a network. Requirements, all phase 2:

- Off by default (`remote.enabled = false`); explicit command or config to enable.
- Listen address configurable; default loopback-only with an explicit "listen on
  tailnet/LAN" choice. Never `0.0.0.0` implicitly.
- TLS with a self-signed certificate generated on first run; the QR carries the
  certificate fingerprint and the client pins it (no CA, works on a LAN).
- Pairing: desktop shows a QR with `{host candidates, port, fingerprint, one-time
  secret}`. The secret expires in 2 minutes and is single use. Pairing registers a
  per-device Ed25519 key; every later connection proves possession (challenge-response).
- Device list with revoke, read-only flag, last-seen. Stored in the state dir (0600).
- Rate-limit failed auth; log attach/detach and writes (count, not content).
- UI indicator on the desktop while any client is attached.

Threat note: anyone who can reach the port and holds a paired key has a shell. Keys live
in Android Keystore on the client.

## Client

Phase 2: a web client served by the desktop app itself (xterm.js, key toolbar for Esc,
Ctrl, Tab, arrows, pane/workspace switcher). Works from Chrome on the phone, installable
as a PWA, no app store. Phone limits: no reliable background connection, so no push.

Phase 4 (only if the web client is not enough): native Android app (Kotlin) with a
terminal view (Termux's terminal-view or a WebView xterm.js), foreground service for the
connection and notifications, Keystore-held device key, Iroh via FFI if we adopt it.

## Phases

1. Host core (no network): subscriber tee in `reader_loop`, `snapshot.rs` with round
   trip tests, input, resize policy, `remote::session` driven by in-memory channels.
2. Server and web client: WebSocket+TLS, pairing QR, device store, xterm.js page,
   workspace/pane list, `remote` config keys, desktop indicator. Usable from a phone
   over LAN or Tailscale.
3. Attention push: forward agent states to connected clients; ntfy/UnifiedPush for
   disconnected phones with a "Claude needs input" action that opens the pane.
4. Reach and native: relay or Iroh for NAT traversal; native Android app.

Phase 1 is the spike: it answers whether snapshot fidelity and resize feel right before
any security or transport work.

## Risks

- Snapshot fidelity for TUIs (alt screen, mouse modes, Kitty keyboard protocol state).
- Slow-client backpressure and memory (bounded channels, drop policy).
- Phone keyboard and narrow-screen TUI usability (toolbar, mirror vs fit).
- Mobile OS kills background sockets; web client cannot notify when closed.
- Security surface: pairing flow and TLS pinning must be reviewed before phase 2 ships.
- Resize "fit" reflows the desktop pane; keep it opt-in.

## Open questions

- Web client first (recommended) or straight to native?
- Tailscale-only for v1, or also a built-in relay?
- Read-only by default for new devices?
