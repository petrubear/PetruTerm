# Contract: Agents sidebar section (UI behavior)

Section tab: first label "Agents", before Workspaces. Tab/Shift+Tab cycling includes it (order: Agents, Workspaces, MCP, Skills, Steering). Agents is the default section when the sidebar opens. Click selects.

Row (single list, scrollable):
- Line 1: state dot (accent when unread/needs-input, muted otherwise), agent name, then `workspace / tab` truncated; when the tab has more than one pane, ` #N` (pane number in the tab) is appended.
- Line 2 (when present): last message, one line, truncated; trailing relative time ("12s", "3m", "1h").
- `Running` rows show the dot muted, state text "running", no time.

Ordering: NeedsInput, Working, Done, Idle, Running; ties by most recent update.

Interaction
- Click row: focus that pane (switch workspace + tab, focus, unzoom) and set the cursor.
- Keyboard (sidebar focused): Down/j, Up/k move the cursor (wraps), Enter activates; Escape closes the sidebar (existing behavior).
- Empty state: "No agents running" centered muted text.

Lifecycle
- Scan every 2 s only while sidebar visible + section active + window focused; one immediate scan on activation.
- Closing the sidebar or switching section stops scanning.

Non-goals (explicit): no cwd/session id, no actions beyond jump, no closed-sidebar indicator, no wgpu implementation.

Unchanged contracts: control-socket `Message` schema, `petruterm notify`/`hook` CLI, tab dots, pane rings, notifications, jump-to-unread.
