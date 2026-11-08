# Implementation Plan: Agent Activity Line and State-Colored Cards

**Branch**: `003-agent-activity-cards` | **Date**: 2026-11-07 | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `/specs/003-agent-activity-cards/spec.md`

## Summary

Carry a short "what is the agent doing" text from the Claude Code hooks to the Agents row, and tint each card by state. The text rides a new optional `activity` field through the existing path (hook CLI -> control socket `Message` -> `AttentionEvent::State` -> `AttentionTracker` -> `AgentRow`), stored per pane in its own field so it never mixes with the notification message and is cleared whenever the pane leaves `working`. Extraction and cleaning are pure functions in a new `src/term/agents/activity.rs`. The detail level (`full`, `tool`, `off`) is applied when rows are built, so it follows config hot reload. Card color is a pure function of the state and the theme tokens, rendered as a low-opacity tint plus a colored left edge.

No new crates, no new threads, no change to the remote (phone) payloads, and no change for agents that report no activity.

## Technical Context

**Language/Version**: Rust (edition 2021)

**Primary Dependencies**: existing only (serde_json, mlua, gpui). No new crates.

**Storage**: none. Activity is in-memory per pane and dropped with the pane's attention state.

**Testing**: `cargo test` unit tests for extraction, cleaning, tracker rules, config parsing and card style; `tests/hook_cli.rs`-style CLI test for `notify --activity`; manual quickstart with a real Claude Code session.

**Target Platform**: macOS. The gpui binary renders it; the wgpu binary must keep compiling and behaving as before.

**Project Type**: desktop app (two binaries, one shared core)

**Performance Goals**: no added hook latency (the hook process already runs per tool call); a burst of 50 events in 1 s leaves the sidebar responsive.

**Constraints**: activity <= 80 chars, single line, no control characters; row height constant; modules under 400 lines; activity never forwarded to `remote::notify_attention`.

**Scale/Scope**: about 8 files touched, 2 new source files, 1 new config key.

## Constitution Check

*GATE: passes before research; re-checked after design (still passes).*

| Principle | Result |
|-----------|--------|
| I. Two binaries, one core | Pass. Extraction, cleaning and tracker state live in `src/term` and `src/ipc` (shared). Only rendering is in `src/gpui_shell`. The wgpu build gets the new field through shared types and ignores it. |
| II. Terminal engine | N/A. No grid, VTE or PTY logic. |
| III. Lua config | Pass. One new key `agents.activity` parsed in `src/config/lua.rs`, default in `config/default/notifications.lua`, no view/wrapper type. Applied at render time, so hot reload works with the existing full re-evaluation. |
| IV. Threading | Pass. No blocking calls on the UI thread; the hook process and socket path are unchanged. Row building already allocates per render; this adds one short string per working row. |
| V. LLM optional | N/A. |
| VI. Stable deps | Pass. No dependency changes. |
| VII. Simplicity, module size | Pass with one note: `src/term/attention.rs` is already 485 lines with its inline tests. This feature adds under 20 production lines there and puts all new logic in new files (see Complexity Tracking). |

## Project Structure

### Documentation (this feature)

```text
specs/003-agent-activity-cards/
├── plan.md
├── research.md
├── data-model.md
├── quickstart.md
├── contracts/
│   ├── socket-activity-field.md
│   ├── cli-notify-activity.md
│   ├── config-agents-activity.md
│   └── sidebar-agent-card.md
└── tasks.md             # /speckit-tasks (not created here)
```

### Source Code (repository root)

```text
src/
├── term/
│   ├── agents/
│   │   ├── activity.rs        # NEW: tool/prompt -> text, clean_activity, detail filter (pure, tested)
│   │   └── names.rs           # unchanged
│   ├── agents.rs              # AgentRow gains `activity`; `mod activity; pub use activity::*;`
│   └── attention.rs           # PaneAttention.activity; State event gains `activity`; clear rules
├── ipc/
│   ├── mod.rs                 # Message.activity; into_event cleans and forwards it
│   ├── claude_hooks.rs        # PreToolUse / UserPromptSubmit build activity from the payload
│   └── cli.rs                 # `notify --activity <text>`
├── config/
│   ├── schema.rs              # AgentsConfig.activity: ActivityDetail (full | tool | off)
│   └── lua.rs                 # parse agents.activity
└── gpui_shell/
    ├── agent_rows.rs          # fill AgentRow.activity from the tracker, apply the detail level
    └── sidebar/
        ├── agents.rs          # detail line shows activity while working; apply card style
        └── agent_card.rs      # NEW: state -> tint + edge color (pure, tested)

config/default/notifications.lua   # document `activity`
tests/hook_cli.rs                  # extend: notify --activity round trip
```

**Structure Decision**: single crate, two binaries (existing). Pure, testable logic goes in new small files under `src/term/agents/` and `src/gpui_shell/sidebar/`; existing files get small, local edits. `app/attention.rs` and `gpui_shell/poll.rs` already match `AttentionEvent::State { .. }` with `..`, so they need no change.

## Design Decisions (detail in research.md)

1. **Separate `activity` field** in `PaneAttention`, not `last_message`. `apply_state` never clears `last_message`, and `last_message` also feeds the workspace row detail (`gpui_shell/attention.rs`) and `remote::notify_attention`. Reusing it would show "done: Bash: cargo test" and leak commands to the phone.
2. **Activity text grammar is `Label: detail`** (`Bash: cargo test`, `Edit: src/foo.rs`, `Prompt: fix the bug`). The `tool` detail level shows only the label, with no per-agent knowledge in the UI.
3. **The detail level is applied when rows are built**, not in the hook, because the hook runs in a separate short-lived process that has no config. The hook sends the full cleaned text; the app filters. This also gives hot reload for free.
4. **Clean twice.** The hook cleans (cap and one-line), and `Message::into_event` cleans again, because the socket input is untrusted.
5. **Paths are shown relative to the payload `cwd`** when they start with it, so the 80-char cut keeps the informative end of the path.
6. **Card style is a pure function** `card_style(state, colors) -> CardStyle { tint, edge }`: working = `ui_accent`, needs-input = the existing `ATTENTION_COLOR`, done = `ui_success`, idle/unreported = none. Tint alpha is low (0.12); the left edge is opaque. The keyboard cursor keeps `ui_surface_active` as the background and keeps the colored edge.
7. **Wire change is additive.** `activity` is `#[serde(default, skip_serializing_if = "Option::is_none")]`, so older hooks and scripts keep working and unknown fields are already ignored.

## Complexity Tracking

| Item | Why | Simpler alternative rejected |
|------|-----|------------------------------|
| `attention.rs` is over 400 lines (485, mostly inline tests) | Pre-existing. The feature adds under 20 production lines and one test module section there. | Splitting the file is a separate refactor and would widen this diff; new logic lives in new files instead. |
| New `AttentionEvent::State.activity` field touches 7 construction sites | Keeping activity out of `message` is required for correctness (design decision 1). | Reusing `message` would show stale text after `Stop` and leak it to the remote protocol. |

## Risks

- **Interrupted agents.** If Claude Code does not fire `Stop` on Esc, the card keeps showing working with an old activity. The existing age on the row exposes it; handling it is out of scope. Verify behavior in the quickstart (step 7) and record it in `research.md`.
- **Field names per tool** come from Claude Code's documentation and may change; unknown or missing fields fall back to the tool name only, so a change degrades gracefully.
- **Contrast.** The default `ui_muted` is the foreground at 35% alpha, so absolute 4.5:1 text contrast is likely already unmet today (see research R7). The plan tests that the tint does not worsen contrast, instead of asserting an absolute ratio the existing UI does not meet. The spec's FR-008 is amended accordingly.
