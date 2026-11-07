# Data Model: Agents Sidebar Tab

All data is in memory and derived; nothing is persisted.

## RecognizedAgent (config)
| Field | Type | Notes |
|---|---|---|
| name | String | Display name (e.g. "Claude Code"); unique key for overrides |
| process | Vec<String> | Process basenames / interpreter script names that identify it; non-empty |

Validation: empty `process` or empty `name` entries are ignored with a warning. Defaults built in; user entries merged by `name`.

## ProcessRow (scan input, transient)
`pid`, `ppid`, `pgid`, `tpgid`, `comm`, `args` parsed from one `ps` run. Malformed lines skipped. `args` is used only for interpreter-script matching and MUST NOT be logged, stored in `AgentDetection`/`AgentRow`, or displayed.

## PaneProbe (scan input)
`terminal_id: usize`, `shell_pid: u32`. Captured on the main thread per live terminal.

## AgentDetection (scan output)
`terminal_id`, `agent: String` (RecognizedAgent.name), `pid` of the matching process. At most one per terminal (first match by priority: foreground group leader, then group members, then shell children).

## AgentRow (view model, built per render from detections + workspaces + tracker)
| Field | Source |
|---|---|
| terminal_id | detection |
| agent | detection |
| workspace_name | `Workspace.name` of the owning workspace |
| tab_title | owning tab's title |
| pane_number | 1-based position of the pane within its tab (shown only when the tab has 2+ panes) |
| state | `AttentionTracker` state, or `Running` when no state was ever reported |
| unread | tracker |
| message | tracker `last_message`, one line, already sanitized |
| updated_at | tracker `updated_at()` (None for Running) |

State display order: NeedsInput, Working, Done, Idle, Running; ties by most recent `updated_at`, then stable by workspace/tab/pane order.

### State transitions
`Running` -> any reported state on first hook report -> follows tracker. Row is removed when the detection disappears (process gone, pane closed, or foreground process no longer recognized), regardless of tracker state.

## Relationships
Workspace 1..* Tab 1..* Pane(terminal) 0..1 AgentDetection; AgentDetection joins AttentionTracker by `terminal_id`.
