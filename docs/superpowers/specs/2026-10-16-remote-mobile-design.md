# Remote access from a phone (Android) - design

Status: implemented 2026-10-16 (phases 1-3 core). Verified end to end on the Android
emulator: pairing through a local relay, pane list, snapshot, live output, typing, reconnect,
needs-input notification. See "Implementation status" at the end.

## Decisions (2026-10-16)

1. Native Android app first (no web client). The app lives in a separate repo at
   /Users/edison/Documents/Projects/Kotlin/PetruTermMobile (path given by the user);
   the host side lives in this repo.
2. Built-in relay from the start (like cmux). Security is the priority, so the relay is
   untrusted: the phone and desktop run an end-to-end encrypted session (Noise, keys
   pinned at pairing) and the relay only forwards ciphertext between two endpoint ids.
   LAN direct and Tailscale remain as optional routes under the same E2E session.
3. Remote access starts on demand, like Claude Code's `/rc`. Nothing listens or dials
   out until the user runs a command; see "On-demand activation".
4. Paired devices are read-write by default (the point is sending commands); a per-device
   read-only flag still exists.

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

## On-demand activation

- Off at startup, always. No config key can make it start automatically.
- Start: command palette entry "Remote: Start" and a leader keybind (`Leader R`), plus
  the CLI `petruterm remote start|stop|status|devices` through the local control socket.
- Starting opens the relay connection (and optional LAN listener), then shows a QR and
  a short pairing code for new devices; already paired devices just reconnect.
- Stop: "Remote: Stop", `remote stop`, or quitting the app. Stopping drops all client
  sessions and closes the relay link. The state does not persist across restarts.
- While active: status bar / tab indicator, plus a count of attached devices.
- Revoking a device is separate from stopping and persists.

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

- Off by default and only started by an explicit command (see On-demand activation).
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
connection and notifications, device key stored encrypted under a Keystore AES key (Android Keystore cannot hold an X25519 key that Noise can use), Iroh via FFI if we adopt it.

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

## Consequences of the decisions

- Transport: an E2E session (Noise IK/XX with device Ed25519/X25519 keys) carried over
  any byte route: relay (outbound WebSocket/QUIC from both sides), LAN, or Tailscale.
  TLS pinning is replaced by the Noise handshake; the relay never sees plaintext or keys.
- Relay: small Rust service, stateless forwarding by endpoint id, rate limits, no
  accounts, no logs of content. Host and phone both dial out, so no inbound port on the
  desktop. Where it is hosted (self-hosted vs ours) is a separate decision.
- Pairing is still a QR with a one-time secret, now carrying endpoint id + public key +
  relay URL. A stolen relay or QR (after expiry) must not grant access.
- Android client: Kotlin, terminal view, foreground service, device key stored encrypted under a Keystore AES key (Android Keystore cannot hold an X25519 key that Noise can use),
  FCM-free push via its own relay connection. The Noise/frame codec should be a shared
  Rust crate used by desktop and Android (JNI) to avoid two implementations.
- Phase order becomes: 1 host core, 2 E2E session + pairing + relay, 3 Android app,
  4 attention push and polish. The web client is dropped.

## Open questions

- Where to host the relay (self-hosted vs shared)?

## Implementation status

Built:
- `crates/remote-proto`: Noise IK / IKpsk2 (snow), chunked framing, packets, pairing code. Also
  compiled for Android and called through JNI by the app (`PetruTermMobile/rust`).
- `crates/relay` (`petruterm-relay`): untrusted forwarder keyed by endpoint id, limits per IP,
  per host and per client rate, newest host registration wins.
- `src/remote/`: `host` (relay link and per-phone sessions), `bridge` (GUI-thread hand-off, used by
  both binaries), `store` (host key, devices, status), `control` (`petruterm remote ...`).
- On-demand only: `petruterm remote start|stop|status|pair|devices|revoke|readonly`, palette entry
  "Remote: Start / Stop phone access", `Leader R` (gpui). `pair` refuses to print a code to a
  non-terminal so an agent running in a pane cannot read it.
- Output tee + grid snapshot (`term::tee`, `term::snapshot`).
- Android app (separate repo): pairing by QR or pasted code, pane list, xterm.js terminal, key
  toolbar, agent notifications.

Not done / follow-ups:
- Resize "fit" mode (the phone mirrors the desktop size, with a minimum zoom and horizontal
  scroll); scrollback; primary screen under the alt screen.
- Android foreground service so notifications arrive while the app is closed; push for a
  disconnected phone.
- Remote indicator in the desktop UI (status via `petruterm remote status` only); pane titles in
  the gpui list are generic ("terminal N").
- Relay hosting and TLS (use a TLS-terminating proxy for wss://); relay-side registration proof.
- Control-socket caveat: any process of the same user can already read `host.key` and edit
  `devices.json`, so the socket commands do not widen the trust boundary.

## Update: relay inside the app (2026-10-16)

No separate relay process: `remote.relay = "builtin"` (default) binds a relay on `0.0.0.0:<port>`
inside the app's remote runtime while remote access is on, and the desktop dials it over loopback.
The pairing address defaults to the LAN IP; `remote.relay_public` overrides it for internet use
(the built-in relay is only reachable from outside if the router forwards the port or a tunnel
points at it). The gpui binary shows the QR in a popup (`gpui_shell/remote_overlay.rs`).
