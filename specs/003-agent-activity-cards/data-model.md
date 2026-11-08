# Data Model: Agent Activity Line and State-Colored Cards

All state is in memory. Nothing is persisted.

## Activity (text)

A single-line plain string tied to one pane.

| Rule | Value |
|------|-------|
| Grammar | `Label: detail`, or `Label` alone |
| Max length | 80 characters (not bytes), cut with `...` |
| Allowed | printable characters; every control character and run of whitespace becomes one space; leading and trailing space trimmed |
| Empty after cleaning | treated as absent |

Examples: `Bash: cargo test`, `Edit: src/term/attention.rs`, `Grep: AgentState`, `Prompt: fix the sidebar`, `mcp__github__search`.

## PaneAttention (existing, one new field)

| Field | Type | Change |
|-------|------|--------|
| `state` | `AgentState` | unchanged |
| `unread` | `bool` | unchanged |
| `last_message` | `Option<String>` | unchanged; still the notification text only |
| `updated_at` | `Option<Instant>` | unchanged; also bumped when activity changes |
| **`activity`** | `Option<String>` | **new** |

### Activity lifecycle

| Event | Effect on `activity` |
|-------|----------------------|
| `State{working, activity: Some(a)}` | set to `a` (replaces the previous one) |
| `State{working, activity: None}` | unchanged |
| `State{needs-input / done / idle, ...}` | cleared |
| `Notify` / `Bell` (state becomes needs-input) | cleared |
| `clear(terminal)` | cleared if it leaves `needs-input` or `done` |
| `remove(terminal)` | dropped with the pane |

## AttentionEvent::State (existing, one new field)

| Field | Type | Change |
|-------|------|--------|
| `state` | `AgentState` | unchanged |
| `title` | `Option<String>` | unchanged |
| `message` | `Option<String>` | unchanged |
| **`activity`** | `Option<String>` | **new**, already cleaned |

## ipc::Message (existing, one new field)

| Field | Type | Change |
|-------|------|--------|
| ... | ... | unchanged |
| **`activity`** | `Option<String>` | **new**, optional, skipped when absent |

## AgentRow (existing, one new field)

| Field | Type | Change |
|-------|------|--------|
| ... | ... | unchanged |
| **`activity`** | `Option<String>` | **new**; already filtered by the detail level; `None` when off, not working, or never reported |

## ActivityDetail (new config enum)

`full` (default) | `tool` | `off`. Lives in `AgentsConfig.activity`.

| Level | Row shows |
|-------|-----------|
| `full` | the whole activity text |
| `tool` | the label before the first `": "` (the whole text if there is none) |
| `off` | nothing; the row looks as it does today |

## CardStyle (new, pure, render-only)

| Field | Type | Meaning |
|-------|------|---------|
| `tint` | `Option<[f32; 4]>` | background tint over the row; alpha 0.12 |
| `edge` | `Option<[f32; 4]>` | opaque left edge color |

### State to style

| State | Color source | Tint | Edge |
|-------|--------------|------|------|
| Working | `ui_accent` | yes | yes |
| NeedsInput | `ATTENTION_COLOR` | yes | yes |
| Done | `ui_success` | yes | yes |
| Idle / unreported (`None`) | none | no | no |

Selection: the cursor row keeps `ui_surface_active` as its background; the edge is still drawn.
