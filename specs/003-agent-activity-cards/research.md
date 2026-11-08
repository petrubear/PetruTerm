# Research: Agent Activity Line and State-Colored Cards

All items verified against the repository on 2026-11-07 unless marked otherwise.

## R1 - Where the activity data comes from

**Decision**: Build it from the Claude Code hook payload in `src/ipc/claude_hooks.rs`.

**Findings**: Hooks run through the `claude` shim (`scripts/shims/claude`), which injects 7 hooks with `--settings`; nothing is installed by the user. Today `PreToolUse` and `UserPromptSubmit` both become `("working", None)`. Per Claude Code's hooks reference, the stdin JSON carries `session_id`, `cwd`, `hook_event_name`; `PreToolUse` adds `tool_name`, `tool_input` and `tool_use_id`; `UserPromptSubmit` adds `prompt`.

**Per-tool `tool_input` fields used** (from the reference; anything missing falls back to the tool name):

| Tool | Field shown |
|------|-------------|
| Bash | `command` |
| Edit, Write, Read, MultiEdit, NotebookEdit | `file_path` |
| Grep, Glob | `pattern` |
| WebFetch | `url` |
| WebSearch | `query` |
| Agent / Task | `description` |
| TodoWrite, MCP tools, anything else | tool name only |

**Not verified**: the reference excerpt did not document a TodoWrite shape, and did not say whether `Stop` fires when the user interrupts with Esc. The second one is checked manually in the quickstart.

**Alternatives considered**: reading Claude's transcript file for richer status (heavier, couples to a private format); parsing the terminal output (fragile, the engine owns the grid). Both rejected.

## R2 - Why activity cannot reuse `last_message`

**Decision**: new `PaneAttention.activity: Option<String>`.

**Findings**: `AttentionTracker::apply_state` overwrites `last_message` only when a message arrives and never clears it. `last_message` also feeds `message_for` (workspace row detail, `gpui_shell/attention.rs`) and the call to `remote::notify_attention` passes the message to phone clients. Storing activity there would (a) show "done: Bash: cargo test" after `Stop`, (b) put commands into workspace rows, (c) leak them to the remote protocol.

**Rule**: set when a `working` event carries activity; unchanged by a `working` event without it; cleared on `needs-input`, `done`, `idle`, on `clear()` leaving those states, and on `remove()`.

## R3 - Where the detail level is applied

**Decision**: when rows are built (`agent_rows.rs`), from `config.agents.activity`.

**Findings**: `petruterm hook claude <event>` is a short-lived process with no config, and `Message` is parsed in the app. Filtering in the app means one source of truth, immediate effect on hot reload (rows are rebuilt each render from the live config), and no config plumbing into the hook.

**Detail levels**: `full` shows the whole text; `tool` shows the part before the first `": "` (the label); `off` hides the line and falls back to today's behavior.

## R4 - Text grammar and cleaning

**Decision**: `Label: detail`, one line, at most 80 characters, cut with `...`.

**Findings**: the existing `osc_notify::sanitize` removes control characters but does not turn newlines into spaces (so `a\nb` becomes `ab`) and caps at 200. Activity needs whitespace collapse and a tighter cap, so it gets its own `clean_activity` in `src/term/agents/activity.rs`. It counts characters, not bytes, and never splits a multi-byte character.

**Prompt text**: first line of the prompt, label `Prompt`. In `tool` mode this shows just "Prompt".

## R5 - Paths

**Decision**: strip the payload `cwd` prefix from `file_path` and `Grep`/`Glob` `path` values; leave other paths unchanged.

**Rationale**: the 80-char cap cuts the tail, and the informative part of a long absolute path is the tail.

## R6 - Wire format and compatibility

**Decision**: add `activity: Option<String>` to `Message` with `#[serde(default, skip_serializing_if = "Option::is_none")]`.

**Findings**: `Message` already ignores unknown fields on parse (`parse_line_rejects_garbage_and_accepts_unknown_fields`), so an older app receiving the field is unaffected, and an old hook without it behaves as before. `into_event` re-cleans it because the socket is not trusted.

## R7 - Card colors and contrast

**Decision**: tint plus left edge. working = `ui_accent`, needs-input = `ATTENTION_COLOR`, done = `ui_success`, idle/unreported = none.

**Findings**: `ColorScheme` already has `ui_accent`, `ui_success`, `ui_surface_active` and `ui_muted` (`src/config/schema.rs`), derived for themes that do not set them. A light theme ships (`petru-theme-light`), so the tint must work on both.

**Contrast finding**: `ui_muted` defaults to the foreground at 35% alpha, so the existing row text probably does not reach 4.5:1 today. An absolute-ratio test would fail on code this feature does not touch. The plan therefore tests that the tint (alpha 0.12) lowers the text contrast by no more than a small bounded amount compared with the untinted row, and the activity line uses the same colors as the existing detail line. The spec's FR-008 is updated to say so.

**Selection**: the keyboard cursor keeps `ui_surface_active` as its background (as now). The left edge stays colored, so state remains readable on the selected row.

**Alternative rejected**: a full cmux-style fill. It needs per-theme foreground adjustments to stay readable.

## R8 - Module size

**Findings**: `src/term/attention.rs` is 485 lines including inline tests, already over the 400-line rule. Rather than widen the diff with a split, new logic goes in new files (`activity.rs`, `agent_card.rs`) and `attention.rs` gets under 20 production lines. Logged in Complexity Tracking.

## R9 - Open questions from the spec

1. Secrets in commands: default stays `full`, documented in the config comments and the contract. `tool` is the opt-out.
2. Stalled hint: not in this feature.
3. Card treatment: tint plus left edge (R7).

## R10 - Tooling note

`.specify/feature.json` pointed at `specs/002-agent-messaging`, so `setup-plan.sh` resolved the wrong feature (it skipped the copy, so 002's plan was not overwritten). It was updated to `specs/003-agent-activity-cards` for this plan.

## R11 - Implementation notes (2026-11-07)

- **Remote and notifications (T018)**: verified by review and grep. `remote::notify_attention` in `src/gpui_shell/poll.rs` and `src/app/attention.rs` still receives only `state.label()` and `message`; those call sites destructure `AttentionEvent::State { .. }` and never read `activity`. The `Alert` built in `AttentionTracker::apply_state` uses `message` only. Tests assert activity never reaches `last_message` or `message_for`.
- **Contrast rule (T014)**: the 10%-relative bound in the plan failed on the dark theme (muted text on the working tint: 8.50 -> 7.28, about 14%), while staying far above 4.5:1. The test now asserts `tinted >= min(4.5, plain * 0.9)`: text that is readable on the plain row stays readable under the tint, and text already below 4.5:1 cannot get more than 10% worse.
- **`tool_activity` returns `Option<String>`** (not `String`) so an empty tool name yields no activity.
- **Not done by code**: the Esc-interrupt check (does `Stop` fire?) and the visual checks in both themes need a real session; see quickstart steps 3-7.
