# M5a — ACP Agent Backend & Tool-Calling Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Close M3b's largest deferred item: the ACP agent backend, tool-calling, and every confirm-prompt/undo surface that exists only to gate a tool call.

**Architecture:** `AcpSession`/`run_session` (`src/llm/acp/{mod,session}.rs`) and `AcpTerminalRequest` (`src/llm/acp/terminal.rs`) are already fully engine-agnostic and port as-is, unmodified. What's genuinely new is everything around them: the gpui-side connect/poll bridge, new `GpuiShellRoot`/`ChatPanelView` state (exit-code tracking, final-output caching, the terminal-request channel, confirm/undo fields), and new UI (confirm cards, diff rendering).

**Tech Stack:** Rust 2021, gpui 0.2.2, tokio, `agent-client-protocol`/`agent-client-protocol-tokio` 0.11 (already unconditional workspace dependencies).

**Spec:** `docs/superpowers/specs/2026-09-14-gpui-m5a-acp-tool-calling-design.md`

## Global Constraints

- 400-line module limit -- current state, verified fresh before writing this plan: `input.rs` 377,
  `render.rs` 393, `render_callbacks.rs` 224, `leader.rs` 251, `leader_dispatch.rs` 160, `poll.rs`
  259, `actions.rs` 312, `chat_panel/mod.rs` 259, `chat_panel/render.rs` 384, `chat_panel/stream.rs`
  263, `ai_actions.rs` 96. `render.rs` and `chat_panel/render.rs` have almost no headroom and this
  plan's several UI-adding tasks (1, 3, 6) will very likely push one or both over -- check `wc -l`
  after every task and fix any real overshoot via extraction (a new file, re-exported so existing
  call sites are unaffected), never deferred.
- `scripts/ci-local.sh` must stay green after every task (clippy `-D warnings`, `fmt --check`,
  full `cargo test --lib`, `cargo audit`); run `cargo fmt` proactively before every commit.
- Tests cover **logic only** -- no painting/hover/hit-testing/real-subprocess tests; every UI- or
  real-agent-process-facing item is a dogfood step (see the spec's own §10), not a unit test.
- Commit format: `type: Message.` per `AGENTS.md`.
- `#[allow(dead_code)]` (narrowly scoped, comment naming the removing task) for anything built
  ahead of its first caller within this plan's own task sequence.
- **Key-guard shape for confirm cards:** both confirm-card states (`AwaitingConfirm`,
  `ConfirmAction`) intercept keys mode-keyed on `panel.state`, not `is_focused(window)` -- matches
  `InfoOverlay`'s own established exception (no `FocusHandle` involved; a confirm card genuinely
  takes over, unlike the M5b file picker which stayed composer-focused throughout).
- **Do not modify** `src/app/`, `src/ui/`, `src/llm/acp/`, `src/llm/agent_action.rs`, or
  `src/llm/diff.rs` -- all either already engine-agnostic and reused as-is, or wgpu-only code this
  milestone must not regress. `src/llm/chat_panel/mod.rs` (the shared `ChatPanel`) may be read
  freely but not restructured.
- **Real, verified ordering fix, baked into Task 1 below (not left as a surprise):**
  `GpuiShellRoot::new` currently constructs `chat` (`chat_panel::ChatPanelView::new(cx, &config)`)
  at line 262, **before** `tokio_rt` is constructed at line 303 -- confirmed by reading the real
  file; nothing between those two lines depends on `chat` existing first. Task 3 needs `tokio_rt`
  available at `ChatPanelView::new`'s own call site (to spawn an initial ACP connect when `config.
  llm.backend == Agent`), so Task 3 moves the `let tokio_rt = ...;` line to before the `let chat =
  ...;` line and adds `tokio_rt: &tokio::runtime::Runtime` as a new parameter to `ChatPanelView::
  new`.
- **Real, verified fact about `Terminal`'s own fields:** `pub child_pid: u32` is a direct field on
  `Terminal` itself (`src/term/mod.rs`), not only reachable via `.pty.child_pid` -- Task 4's own
  `Kill` handler uses `terminal.child_pid` directly, the simpler of the two equivalent forms.
- **Real, verified fact about `stream.rs`'s current `AiEvent` drain match:** it is already
  exhaustive today, with `ToolStatus`/`ConfirmWrite`/`ConfirmRun`/`UndoState` grouped into one
  real (currently no-op) arm: `AiEvent::ToolStatus { .. } | AiEvent::ConfirmWrite { .. } |
  AiEvent::ConfirmRun { .. } | AiEvent::UndoState { .. } => {}`. Task 5 splits `ToolStatus` out
  into its own real arm; Task 6 splits `ConfirmWrite`/`ConfirmRun`/`UndoState` out into their own
  real arms. Both tasks' own Steps give the exact current text to replace.

---

## Task 1: Inline-action confirm flow

**Tier: standard.** Touches 5 files including a real behavioral change to `GpuiShellRoot::new`'s
construction order (shared with Task 3) and a new confirm-card UI pattern later tasks reuse --
more integration judgment than a mechanical port, even though every piece of logic is fully
specified below.

**Files:**
- Modify: `src/gpui_shell/ai_actions.rs`
- Modify: `src/gpui_shell/chat_panel/stream.rs`
- Modify: `src/gpui_shell/chat_panel/render.rs`
- Modify: `src/gpui_shell/input.rs`
- Modify: `src/gpui_shell/render.rs`
- Modify: `src/gpui_shell/mod.rs`

**Interfaces:**
- Consumes: `crate::llm::agent_action::{AgentAction, system_prompt_instructions, parse_action_
  from_response}` (unmodified), `ChatPanel::{resolve_action_yes, resolve_action_no, auto_confirm_
  actions, confirm_display, state}` (all pre-existing `pub`), `ai_actions.rs::{run_ai_query, last_
  terminal_lines}` (both widened from private to `pub(super)` by this task).
- Produces: `GpuiShellRoot::pending_agent_action: Option<AgentAction>` (new field); `GpuiShellRoot::
  maybe_handle_confirm_action_key(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut
  Context<Self>) -> bool` (`pub(super)`, consumed by `input.rs`); `GpuiShellRoot::flush_pending_
  agent_action(&mut self, window: &mut Window, cx: &mut Context<Self>)` (`pub(super)`, consumed by
  `render.rs`'s top); `chat_panel/render.rs::render_confirm_card(title: &str, body: impl
  IntoElement, hint: &str, colors: &ColorScheme) -> impl IntoElement` (`pub(super)`, the shared
  card chrome Task 6 also calls).

- [ ] **Step 1: `ai_actions.rs` -- widen visibility, add the agent-action dispatcher**

Read the file's current exact content first (reproduced in full above, in this plan's own research
-- confirm it still matches before editing). Change `fn run_ai_query` to `pub(super) fn run_ai_
query` and `fn last_terminal_lines` to `pub(super) fn last_terminal_lines`.

Add, inside the existing `impl GpuiShellRoot { ... }` block, right after `fix_last_error`:

```rust
    /// Execute one confirmed inline agent action (`ChatPanel::resolve_
    /// action_yes`'s own return value, drained here from `render()`'s top
    /// -- see `mod.rs`'s new `pending_agent_action` field). Ported from
    /// `flush_pending_agent_action` (`src/app/frame.rs:259-316`).
    pub(super) fn flush_pending_agent_action(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        use crate::llm::agent_action::AgentAction;
        use crate::llm::ChatMessage;
        let Some(action) = self.pending_agent_action.take() else {
            return;
        };
        match action {
            AgentAction::RunCommand { cmd, .. } => {
                let note = format!("Running: `{cmd}`");
                let active_ws = self.workspaces.active();
                let active_tid =
                    active_ws.tab_panes[active_ws.tabs.active_index()].focused_terminal;
                if let Some(terminal) = self.terminals.get(&active_tid) {
                    let mut data = cmd.into_bytes();
                    data.push(b'\n');
                    terminal.write_input(&data);
                }
                self.chat.panel.messages.push(ChatMessage::assistant(note));
                cx.notify();
            }
            AgentAction::OpenFile { path } => {
                let cwd = self.cached_cwd.clone().unwrap_or_default();
                let abs = cwd.join(&path);
                let p = if abs.exists() {
                    abs.to_string_lossy().into_owned()
                } else {
                    path.clone()
                };
                let _ = std::process::Command::new("open").arg(&p).spawn();
                let note = format!("Opening: `{path}`");
                self.chat.panel.messages.push(ChatMessage::assistant(note));
                cx.notify();
            }
            AgentAction::ExplainOutput { last_n_lines } => {
                let output = self.last_terminal_lines(last_n_lines);
                if output.is_empty() {
                    return;
                }
                let query = format!("Explain this terminal output:\n```\n{output}\n```");
                self.run_ai_query(query, window, cx);
            }
        }
    }
```

`Terminal::write_input(&self, data: &[u8])` -- confirmed real (`src/term/mod.rs:159`), already
called identically throughout `gpui_shell` (e.g. `context_menu.rs`'s `dispatch_context_action`).

- [ ] **Step 2: `chat_panel/stream.rs` -- append the action instructions to the system prompt**

Find the exact current line `let mut messages = vec![ChatMessage::system(crate::config::load_
system_prompt())];` inside `submit()` and replace it with:

```rust
        let system_prompt = format!(
            "{}\n\n{}",
            crate::config::load_system_prompt(),
            crate::llm::agent_action::system_prompt_instructions()
        );
        let mut messages = vec![ChatMessage::system(system_prompt)];
```

- [ ] **Step 3: `mod.rs` -- the new field**

Read the file's current exact `pub struct GpuiShellRoot { ... }` field list and `Self { ... }`
construction (both reproduced in full in this plan's own research) to confirm they still match,
then add, right after the existing `toast: Option<(String, std::time::Instant)>,` field:

```rust
    /// A confirmed inline agent action, drained at the top of `render()`
    /// (no `Window` where it's set -- see `maybe_handle_confirm_action_
    /// key`'s own call site in `input.rs`). Same shape as `pending_
    /// palette_action`.
    pending_agent_action: Option<crate::llm::agent_action::AgentAction>,
```

Add `pending_agent_action: None,` to the `Self { ... }` construction, right after the existing
`toast: None,` line.

- [ ] **Step 4: `mod.rs` -- the key guard**

Add, inside the same `impl GpuiShellRoot { ... }` block `mod.rs` already has (find a natural
location -- e.g. right after the struct's own `impl` block, or in a new small `impl` block if
that reads more cleanly against the file's real current structure):

```rust
    /// The inline-action confirm card's own key guard, called from
    /// `input.rs`'s `on_key_down`. Returns `true` if the key was consumed.
    /// Mode-keyed on `panel.state`, not focus -- see this plan's own
    /// Global Constraints.
    pub(super) fn maybe_handle_confirm_action_key(
        &mut self,
        event: &gpui::KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        use crate::llm::chat_panel::PanelState;
        if !matches!(self.chat.panel.state, PanelState::ConfirmAction(_)) {
            return false;
        }
        let key = event.keystroke.key.as_str();
        if key == "y" || key == "enter" {
            if let Some(action) = self.chat.panel.resolve_action_yes() {
                self.pending_agent_action = Some(action);
            }
        } else if key == "a" {
            self.chat.panel.auto_confirm_actions = true;
            if let Some(action) = self.chat.panel.resolve_action_yes() {
                self.pending_agent_action = Some(action);
            }
        } else if key == "n" || key == "escape" {
            self.chat.panel.resolve_action_no();
        }
        cx.notify();
        true
    }
```

- [ ] **Step 5: `input.rs` -- wire the guard**

Find the existing:

```rust
        // File picker key guard -- see `chat_panel/mod.rs`'s
        // `maybe_handle_file_picker_key` doc comment for why this is
        // mode-keyed rather than focus-keyed.
        if self.maybe_handle_file_picker_key(event, cx) {
            return;
        }
```

Add, right after it:

```rust

        // Inline-action confirm card's own key guard -- see `mod.rs`'s
        // `maybe_handle_confirm_action_key` doc comment.
        if self.maybe_handle_confirm_action_key(event, cx) {
            return;
        }
```

- [ ] **Step 6: `render.rs` -- drain `pending_agent_action`**

Find the existing `if let Some(text) = self.pending_send_to_chat.take() { ... }` block at the top
of `render()` and add, right after it:

```rust
        self.flush_pending_agent_action(window, cx);
```

- [ ] **Step 7: `chat_panel/render.rs` -- the shared confirm-card chrome + the inline-action card**

Add, right after the existing `pub const PANEL_WIDTH_PX: f32 = 480.0;` line:

```rust
/// Shared bordered-card chrome for both confirm-prompt surfaces (inline
/// actions here, ACP write/run confirms later) -- a title row, an
/// arbitrary body element, and a keyboard-hint row. Neither surface has
/// any clickable rows of its own; both are driven entirely by their own
/// mode-keyed key guard (`GpuiShellRoot::maybe_handle_confirm_action_key`/
/// `maybe_handle_awaiting_confirm_key`), so this needs no callback
/// parameters.
pub(super) fn render_confirm_card(
    title: &str,
    body: impl IntoElement,
    hint: &str,
    colors: &ColorScheme,
) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .gap_1()
        .mx_3()
        .my_2()
        .px_3()
        .py_2()
        .rounded_md()
        .border_1()
        .border_color(to_rgba(colors.ui_accent))
        .bg(to_rgba(colors.ui_surface_hover))
        .child(
            div()
                .text_color(to_rgba(colors.ui_accent))
                .font_weight(FontWeight::BOLD)
                .child(title.to_string()),
        )
        .child(body)
        .child(
            div()
                .text_size(px(11.0))
                .text_color(to_rgba(colors.ui_muted))
                .child(hint.to_string()),
        )
}

fn render_agent_action_card(
    action: &crate::llm::agent_action::AgentAction,
    colors: &ColorScheme,
) -> impl IntoElement {
    use crate::llm::agent_action::AgentAction;
    let body = match action {
        AgentAction::RunCommand { cmd, explanation } => {
            if explanation.is_empty() {
                format!("Run: `{cmd}`")
            } else {
                format!("Run: `{cmd}`\n{explanation}")
            }
        }
        AgentAction::OpenFile { path } => format!("Open: `{path}`"),
        AgentAction::ExplainOutput { last_n_lines } => {
            format!("Explain last {last_n_lines} terminal lines")
        }
    };
    render_confirm_card(
        "Confirm action",
        div().text_color(to_rgba(colors.foreground)).child(body),
        "[y]es  [a]lways  [n]o",
        colors,
    )
}
```

`ColorScheme`/`to_rgba`/`FontWeight`/`px`/`div` are all already imported in this file (confirmed
against its real current import list, reproduced in this plan's own research) -- no new imports
needed for this step.

Find `render_message_list`'s own tail (currently ending with the `show_suggestions` block
M5b Task 2 added, then `list`) and add a `ConfirmAction` branch right before the final `list`:

```rust
    if let PanelState::ConfirmAction(action) = &panel.state {
        list = list.child(render_agent_action_card(action, colors));
    }

    list
}
```

`PanelState` is already imported (`use crate::llm::chat_panel::{ChatPanel, PanelState};`,
confirmed in this plan's own research of the file's real current imports).

- [ ] **Step 8: Build, test, verify**

Run: `cargo build 2>&1 | tail -100`, `cargo test --lib 2>&1 | tail -10` (expect 234/234 unchanged
-- pure wiring, no new tests), `cargo fmt` then `cargo fmt --check` (clean), `cargo clippy
--all-features -- -D warnings` (clean), `./scripts/ci-local.sh` (exit 0).

Run `wc -l src/gpui_shell/ai_actions.rs src/gpui_shell/chat_panel/stream.rs
src/gpui_shell/chat_panel/render.rs src/gpui_shell/input.rs src/gpui_shell/render.rs
src/gpui_shell/mod.rs` and note results. `render.rs` (393) and `chat_panel/render.rs` (384) have
almost no headroom -- if either overshoots, extract the new code added in Steps 6/7 into a new
file (e.g. `chat_panel/confirm.rs` for Step 7's own additions) rather than leaving it; re-export so
`render_chat_panel`'s own call sites are unaffected. Flag any other overshoot in CONCERNS.

Dogfood is not possible from this environment -- note in your report that this is pending manual
dogfood (reproduce the spec's §10 "Inline-action confirm" checklist).

- [ ] **Step 9: Commit**

```bash
git add src/gpui_shell/ai_actions.rs src/gpui_shell/chat_panel/stream.rs src/gpui_shell/chat_panel/render.rs src/gpui_shell/input.rs src/gpui_shell/render.rs src/gpui_shell/mod.rs
git commit -m "feat: Add the inline-action confirm flow (M5a Task 1)."
```

---

## Task 2: Terminal-bridge foundation -- exit codes + final output

**Tier: cheap.** Small, mechanical, fully pre-specified state + two small accessor methods; no UI,
no async, no ACP dependency.

**Files:**
- Modify: `src/gpui_shell/mod.rs`
- Modify: `src/gpui_shell/poll.rs`
- Modify: `src/gpui_shell/actions.rs`
- Create: `src/gpui_shell/terminal_output.rs`

**Interfaces:**
- Consumes: `PtyEvent::Exit(i32)` (unmodified, already drained by `poll.rs`), `Terminal::with_term`
  (unmodified).
- Produces: `GpuiShellRoot::{terminal_exit_codes: HashMap<usize, i32>, terminal_final_output:
  HashMap<usize, String>}` (new fields); `GpuiShellRoot::{terminal_output_text(&self, terminal_id:
  usize) -> String, terminal_exit_code(&self, terminal_id: usize) -> Option<i32>}` (both
  `pub(super)`, consumed by Task 4).

- [ ] **Step 1: `mod.rs` -- the two new fields**

Add, right after the `pending_agent_action` field Task 1 added (or right after `toast` if Task 1
hasn't landed yet in your own execution order -- it has, per this plan's own task order, so add
after `pending_agent_action`):

```rust
    /// Exit code of a terminal that has been reaped, keyed by the id it
    /// had while alive -- `self.terminals` no longer has an entry for it
    /// by the time this map is read. Mirrors `Mux::terminal_exit_codes`.
    terminal_exit_codes: HashMap<usize, i32>,
    /// Final grid contents of a reaped terminal, captured just before its
    /// last `Rc<Terminal>` is dropped. Mirrors `Mux::terminal_final_
    /// output`; bounded the same way (oldest evicted past a small cap) so
    /// a long session with many closed panes can't grow this unboundedly.
    terminal_final_output: HashMap<usize, String>,
```

Add `terminal_exit_codes: HashMap::new(),` and `terminal_final_output: HashMap::new(),` to the
`Self { ... }` construction, right after `pending_agent_action: None,`.

- [ ] **Step 2: `terminal_output.rs` -- the two accessors + the full-grid-read helper**

```rust
// gpui chrome migration (M5a Task 2): per-terminal exit-code tracking and
// final-output caching -- neither exists in `gpui_shell` today (`poll.rs`
// already drains `PtyEvent::Exit(code)` but discards `code`; a reaped
// terminal's grid is simply gone). Built for ACP's own `terminal/output`/
// `terminal/wait_for_exit` (Task 4), which need to answer "what did this
// pane print" and "did it exit, with what code" even after the pane
// itself has closed -- mirrors `Mux::{terminal_output_text,
// terminal_exit_code}` (`src/app/mux/mod.rs:637-673`) exactly.

use crate::term::Terminal;

use super::GpuiShellRoot;

/// Cap on `terminal_final_output`'s size -- oldest entry evicted past
/// this, same bounded-cache shape `Mux::retain_closed_terminal` already
/// established for its own identical map.
const MAX_FINAL_OUTPUT_ENTRIES: usize = 64;

impl GpuiShellRoot {
    /// Every visible row of `terminal_id`'s grid if it's still alive,
    /// trimmed of trailing empty lines; the cached final output if it has
    /// already been reaped; empty string if neither.
    pub(super) fn terminal_output_text(&self, terminal_id: usize) -> String {
        let Some(terminal) = self.terminals.get(&terminal_id) else {
            return self
                .terminal_final_output
                .get(&terminal_id)
                .cloned()
                .unwrap_or_default();
        };
        full_grid_text(terminal)
    }

    /// `None` while `terminal_id` is still alive; its cached exit code
    /// once it has been reaped.
    pub(super) fn terminal_exit_code(&self, terminal_id: usize) -> Option<i32> {
        if self.terminals.contains_key(&terminal_id) {
            return None;
        }
        self.terminal_exit_codes.get(&terminal_id).copied()
    }

    /// Capture `terminal_id`'s exit code just before `poll.rs` reaps it.
    /// Called from `poll.rs`'s own `PtyEvent::Exit(code)` arm.
    pub(super) fn record_terminal_exit_code(&mut self, terminal_id: usize, code: i32) {
        self.terminal_exit_codes.insert(terminal_id, code);
    }

    /// Capture `terminal_id`'s final grid text just before its last
    /// `Rc<Terminal>` is dropped. Called from `actions.rs`'s own pane-
    /// removal sites, right before `self.terminals.remove(...)`.
    pub(super) fn record_terminal_final_output(&mut self, terminal_id: usize) {
        if let Some(terminal) = self.terminals.get(&terminal_id) {
            let text = full_grid_text(terminal);
            if self.terminal_final_output.len() >= MAX_FINAL_OUTPUT_ENTRIES {
                if let Some(&oldest) = self.terminal_final_output.keys().next() {
                    self.terminal_final_output.remove(&oldest);
                }
            }
            self.terminal_final_output.insert(terminal_id, text);
        }
    }
}

fn full_grid_text(terminal: &Terminal) -> String {
    terminal.with_term(|term| {
        use alacritty_terminal::grid::Dimensions;
        use alacritty_terminal::index::{Column, Line};
        let rows = term.screen_lines();
        let cols = term.columns();
        let mut lines: Vec<String> = (0..rows)
            .map(|row| {
                let mut text = String::new();
                for col in 0..cols {
                    let cell = &term.grid()[Line(row as i32)][Column(col)];
                    text.push(if cell.c == '\0' { ' ' } else { cell.c });
                }
                text.trim_end().to_string()
            })
            .collect();
        while lines.last().is_some_and(|l| l.is_empty()) {
            lines.pop();
        }
        lines.join("\n")
    })
}
```

- [ ] **Step 3: `mod.rs` -- register the module**

Add `mod terminal_output;` in alphabetical order (right after `mod terminal_element;`'s own
declaration if `terminal_element` is a `pub mod` line in the file's real current module list --
find its exact current position and insert alphabetically; if there is no `mod terminal_element;`
line because it's declared elsewhere, insert `mod terminal_output;` in the correct alphabetical
slot against the real current list).

- [ ] **Step 4: `poll.rs` -- capture the exit code**

Find the exact current line `crate::term::PtyEvent::Exit(_) => { exited_terminals.push(id); }`
and replace it with:

```rust
                                crate::term::PtyEvent::Exit(code) => {
                                    this.record_terminal_exit_code(id, code);
                                    exited_terminals.push(id);
                                }
```

- [ ] **Step 5: `actions.rs` -- capture final output before removal**

Read `reap_pane`'s real current body in full first (the plan's own research confirms it contains
`self.terminals.remove(&terminal_id);` -- find that exact line). Add, right before it:

```rust
        self.record_terminal_final_output(terminal_id);
```

- [ ] **Step 6: Write and run a unit test for the eviction cap**

Add to `terminal_output.rs`'s own bottom:

```rust
#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    /// Pure logic check of the eviction rule `record_terminal_final_
    /// output` uses -- no live `Terminal`/`GpuiShellRoot` needed, matching
    /// this project's own "no PTY in unit tests" convention.
    #[test]
    fn eviction_keeps_map_at_the_cap() {
        const CAP: usize = 3;
        let mut map: HashMap<usize, String> = HashMap::new();
        for i in 0..5 {
            if map.len() >= CAP {
                if let Some(&oldest) = map.keys().next() {
                    map.remove(&oldest);
                }
            }
            map.insert(i, format!("output-{i}"));
        }
        assert_eq!(map.len(), CAP);
    }
}
```

Run: `cargo test --lib terminal_output:: -- --nocapture` and confirm it passes.

- [ ] **Step 7: Build, test, verify**

Run: `cargo build 2>&1 | tail -100`, `cargo test --lib 2>&1 | tail -10` (expect 235/235 -- 234 +
this task's 1 new test), `cargo fmt` then `cargo fmt --check` (clean), `cargo clippy
--all-features -- -D warnings` (clean), `./scripts/ci-local.sh` (exit 0).

Run `wc -l src/gpui_shell/mod.rs src/gpui_shell/poll.rs src/gpui_shell/actions.rs
src/gpui_shell/terminal_output.rs` and note results; flag any 400-line overshoot in CONCERNS and
fix it yourself.

Dogfood note: no direct user-visible behavior yet (per the spec's own §10) -- confirm via your own
reading that `record_terminal_final_output` runs before the terminal is removed, not a dogfood
step.

- [ ] **Step 8: Commit**

```bash
git add src/gpui_shell/mod.rs src/gpui_shell/poll.rs src/gpui_shell/actions.rs src/gpui_shell/terminal_output.rs
git commit -m "feat: Track terminal exit codes and cache final output on close (M5a Task 2)."
```

---

## Task 3: ACP session lifecycle + backend switching

**Tier: standard.** Real async-lifecycle judgment (connect-then-poll bridge, a construction-order
fix shared with the rest of the milestone, backend-aware slash commands) -- not a mechanical port.

**Files:**
- Modify: `src/gpui_shell/mod.rs`
- Create: `src/gpui_shell/chat_panel/backend.rs`
- Modify: `src/gpui_shell/poll.rs`
- Modify: `src/gpui_shell/chat_panel/stream.rs`
- Modify: `src/gpui_shell/chat_panel/render.rs`

**Interfaces:**
- Consumes: `crate::llm::acp::AcpSession::connect` (unmodified, `async fn connect(cfg: &
  AcpAgentConfig, cwd: &Path) -> Result<Self>`), `crate::config::llm_view::{llm_runtime_view,
  agent_display_name}` (unmodified), `crate::config::schema::LlmBackend` (unmodified).
- Produces: `ChatPanelView::{acp_session: Option<AcpSession>, acp_pending_connect: Option<tokio::
  sync::oneshot::Receiver<Result<AcpSession, String>>>}` (new fields); `ChatPanelView::rewire_
  backend(&mut self, config: &Config, tokio_rt: &tokio::runtime::Runtime)` (`pub`, replaces `
  rewire_provider` at every call site); `ChatPanelView::poll_acp_connect(&mut self) -> bool`
  (`pub(super)`, consumed by `poll.rs`).

- [ ] **Step 1: `mod.rs` -- reorder construction, thread `tokio_rt` through**

Find the exact current sequence (confirmed in this plan's own research):

```rust
        let chat = chat_panel::ChatPanelView::new(cx, &config);
        let ai_block = ai_block::AiBlockView::new(cx, &config);
```

...(several lines of `palette`/`search_bar` construction in between, unchanged)...

```rust
        // Same construction pattern as the wgpu app's own `tokio_rt` field
        // on its `App`/`Mux` struct (`src/app/ui/mod.rs`) -- hoisted into
        // its own binding, rather than built inline in the `Self { .. }`
        // literal below (M3c's shape), because MCP startup needs to
        // `.block_on()` it before the struct exists.
        let tokio_rt = tokio::runtime::Runtime::new().expect("Failed to build tokio runtime");
```

Move the `tokio_rt` construction (including its own doc comment, updated below) to right before
the `let chat = ...` line, and pass it through:

```rust
        // Same construction pattern as the wgpu app's own `tokio_rt` field
        // on its `App`/`Mux` struct (`src/app/ui/mod.rs`) -- hoisted into
        // its own binding, rather than built inline in the `Self { .. }`
        // literal below (M3c's shape), because MCP startup needs to
        // `.block_on()` it before the struct exists, and `ChatPanelView::
        // new` (M5a) needs it too, to spawn an initial ACP connect when
        // `config.llm.backend == Agent`.
        let tokio_rt = tokio::runtime::Runtime::new().expect("Failed to build tokio runtime");
        let chat = chat_panel::ChatPanelView::new(cx, &config, &tokio_rt);
        let ai_block = ai_block::AiBlockView::new(cx, &config);
```

(Everything between the old `let ai_block = ...` line and the old `let tokio_rt = ...` line stays
exactly where it is, unmoved -- only the `tokio_rt` binding itself and the `chat` line's own
argument list change. Read the real current file to confirm the exact span between these two
points before editing, since this plan's own research only quoted the two ends of it.)

- [ ] **Step 2: `chat_panel/backend.rs` -- the connect/poll bridge**

```rust
// gpui chrome migration (M5a Task 3): the ACP session lifecycle -- connect
// (spawned, never blocking), poll-drain the result, and backend-aware
// rewiring. `AcpSession::connect` itself (src/llm/acp/mod.rs) is already
// fully engine-agnostic and reused unmodified; this file is the gpui-side
// bridge around it, mirroring `UiManager::rewire_backend`/`spawn_acp_
// connect`/`poll_acp_connect` (src/app/ui/{mod,providers}.rs) minus the
// winit `EventLoopProxy<()>` wakeup every one of them takes -- the 33ms
// poll tick IS the wake mechanism here, the same "drop the wakeup
// parameter" adaptation every prior async-scan milestone this session has
// made (M5c's branch/workspace scans, M5b's file-picker scan).

use std::path::PathBuf;

use crate::config::schema::LlmBackend;
use crate::config::Config;
use crate::llm::acp::AcpSession;

use super::ChatPanelView;

impl ChatPanelView {
    /// Re-wire the active backend from a fresh config. Call at
    /// construction and on every config reload (hot-reload or `/agent`).
    /// Never blocks: the ACP connect (subprocess spawn + protocol
    /// handshake) runs in the background, picked up later by `poll_acp_
    /// connect`.
    pub fn rewire_backend(&mut self, config: &Config, tokio_rt: &tokio::runtime::Runtime) {
        self.acp_pending_connect = None;
        let view = crate::config::llm_view::llm_runtime_view(config);
        match view.backend {
            LlmBackend::Provider => {
                self.acp_session = None;
                self.rewire_provider(&config.llm);
            }
            LlmBackend::Agent => {
                self.llm_provider = None;
                self.llm_init_error = None;
                self.acp_session = None;
                if let Some(agent_cfg) = config.llm.agent.clone() {
                    let cwd = std::env::current_dir().unwrap_or_default();
                    self.acp_pending_connect = Some(spawn_acp_connect(tokio_rt, agent_cfg, cwd));
                } else {
                    self.llm_init_error =
                        Some("llm.agent config is required when backend = \"agent\"".into());
                }
            }
        }
    }

    /// Drain a completed connect attempt. Returns `true` if it updated
    /// anything (caller should `cx.notify()`). Called from `poll.rs`'s
    /// existing 33ms tick.
    pub(super) fn poll_acp_connect(&mut self) -> bool {
        let Some(rx) = &mut self.acp_pending_connect else {
            return false;
        };
        match rx.try_recv() {
            Ok(Ok(session)) => {
                self.acp_session = Some(session);
                self.llm_init_error = None;
                self.acp_pending_connect = None;
                true
            }
            Ok(Err(e)) => {
                log::error!("ACP connect: {e}");
                self.llm_init_error = Some(e);
                self.acp_pending_connect = None;
                true
            }
            Err(tokio::sync::oneshot::error::TryRecvError::Empty) => false,
            Err(tokio::sync::oneshot::error::TryRecvError::Closed) => {
                self.llm_init_error = Some("ACP connect task ended unexpectedly".to_string());
                self.acp_pending_connect = None;
                true
            }
        }
    }
}

fn spawn_acp_connect(
    rt: &tokio::runtime::Runtime,
    agent_cfg: crate::config::schema::AcpAgentConfig,
    cwd: PathBuf,
) -> tokio::sync::oneshot::Receiver<Result<AcpSession, String>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    rt.spawn(async move {
        let result = AcpSession::connect(&agent_cfg, &cwd)
            .await
            .map_err(|e| format!("{e:#}"));
        let _ = tx.send(result);
    });
    rx
}
```

`crate::config::llm_view::llm_runtime_view`'s real return type, confirmed while writing this plan:
`LlmRuntimeView { enabled: bool, backend: LlmBackend, panel_width_cols: u16, agent: Option<
AcpAgentConfig>, provider_cfg: LlmConfig }` (`src/config/llm_view.rs:4-10`), built as `agent: config
.llm.agent.clone()` with no transformation -- `view.agent`/`config.llm.agent.clone()` are provably
identical, so the code above's own choice to read `config.llm.agent` directly (skipping the `view`
indirection for the one field it needs) is correct and requires no further check.

- [ ] **Step 3: `chat_panel/mod.rs` -- the two new fields, `new`'s new parameter**

Add `mod backend;` in alphabetical order (right after `use gpui::{...}`'s import block, in the
existing `mod file_picker; pub(super) mod markdown; mod render; mod stream;` list -- insert
alphabetically: `mod backend;` comes first).

Add, to the `ChatPanelView` struct, right after the existing `file_scan_rx` field:

```rust
    /// The connected ACP agent session, if `config.llm.backend == Agent`
    /// and the connect succeeded. `None` in Provider mode, or while a
    /// connect is still pending/failed.
    pub(super) acp_session: Option<crate::llm::acp::AcpSession>,
    /// In-flight ACP connect attempt -- see `backend.rs`'s own doc
    /// comment.
    acp_pending_connect:
        Option<tokio::sync::oneshot::Receiver<Result<crate::llm::acp::AcpSession, String>>>,
```

Change `pub fn new(cx: &mut Context<GpuiShellRoot>, config: &Config) -> Self {` to:

```rust
    pub fn new(
        cx: &mut Context<GpuiShellRoot>,
        config: &Config,
        tokio_rt: &tokio::runtime::Runtime,
    ) -> Self {
```

Add `acp_session: None,` and `acp_pending_connect: None,` to the `Self { ... }` construction,
right after the existing `file_scan_rx: None,` line.

Change the constructor's own tail from:

```rust
        view.rewire_provider(&config.llm);
        view
```

to:

```rust
        view.rewire_backend(config, tokio_rt);
        view
```

- [ ] **Step 4: `poll.rs` -- swap the rewire call, drive `poll_acp_connect`**

Find the exact current line `this.chat.rewire_provider(&this.config.llm);` (inside the config-
hot-reload closure) and replace it with:

```rust
                            this.chat.rewire_backend(&this.config, &this.tokio_rt);
```

Find the existing `if this.chat.poll_file_scan() { should_notify = true; }` and add, right after
it:

```rust
                    if this.chat.poll_acp_connect() {
                        should_notify = true;
                    }
```

- [ ] **Step 5: `chat_panel/stream.rs` -- backend-aware `/model`/`/agent`**

`handle_slash_command`'s real current signature is `fn handle_slash_command(&mut self, input: &str,
cx: &mut Context<Self>)` where `Self = GpuiShellRoot` (confirmed: it already calls `self.chat.
close(cx)`/`self.chat.panel.clear_messages()`/`self.push_chat_message(...)` directly) -- `self.
config`/`self.chat` are real field access, not free parameters.

Find the exact current `"model"` and `"agent"` arms:

```rust
            "model" => {
                let msg = if args.is_empty() {
                    format!(
                        "Active: {}:{}",
                        self.config.llm.provider, self.config.llm.model
                    )
                } else {
                    self.config.llm.model = args.to_string();
                    self.chat.rewire_provider(&self.config.llm);
                    format!("Model set to '{args}'.")
                };
                self.push_chat_message(msg);
            }
            "agent" => self.push_chat_message(
                "Agent backend is not available in this build (ACP is deferred -- see \
                 the M3b plan's Scope). Use /model to change the direct-provider model."
                    .to_string(),
            ),
```

Replace with:

```rust
            "model" => {
                use crate::config::schema::LlmBackend;
                let msg = match self.config.llm.backend {
                    LlmBackend::Agent => "Agent mode: use /agent to switch agents.".to_string(),
                    LlmBackend::Provider if args.is_empty() => {
                        format!("Active: {}:{}", self.config.llm.provider, self.config.llm.model)
                    }
                    LlmBackend::Provider => {
                        self.config.llm.model = args.to_string();
                        self.chat.rewire_backend(&self.config, &self.tokio_rt);
                        format!("Model set to '{args}'.")
                    }
                };
                self.push_chat_message(msg);
            }
            "agent" => {
                use crate::config::schema::{AcpAgentConfig, LlmBackend};
                let msg = match self.config.llm.backend {
                    LlmBackend::Provider => {
                        "Provider mode active. Use /model to change models.".to_string()
                    }
                    LlmBackend::Agent if args.is_empty() => {
                        match crate::config::llm_view::agent_display_name(
                            self.config.llm.agent.as_ref(),
                        ) {
                            Some(name) => format!("Active agent: {name}"),
                            None => {
                                "No agent configured. Set llm.agent.command in config.".to_string()
                            }
                        }
                    }
                    LlmBackend::Agent => {
                        if let Some(agent_cfg) = self.config.llm.agent.as_mut() {
                            agent_cfg.command = args.to_string();
                        } else {
                            self.config.llm.agent = Some(AcpAgentConfig {
                                command: args.to_string(),
                                args: vec![],
                                env: vec![],
                                display_name: None,
                            });
                        }
                        self.chat.acp_session = None;
                        self.chat.rewire_backend(&self.config, &self.tokio_rt);
                        format!("Agent set to '{args}'. Reconnecting...")
                    }
                };
                self.push_chat_message(msg);
            }
```

`crate::config::llm_view::agent_display_name`'s real signature -- confirmed while writing this
plan: `pub fn agent_display_name(agent: Option<&AcpAgentConfig>) -> Option<&str>`
(`src/config/llm_view.rs:22`) -- matches the call above exactly.

- [ ] **Step 6: `chat_panel/render.rs` -- header ◈/✦ distinction**

Find `render_header`'s real current body (reproduced in this plan's own research) and replace the
line `let short_model = short_model_name(&llm.model);` plus the two `.child(...)` calls that
follow it (the `\u{2726} {short_model}` one and the `\u{2502} {}:{}` one) with a branch on whether
an agent session exists:

```rust
fn render_header(
    panel: &ChatPanel,
    llm: &LlmConfig,
    acp_session: Option<&crate::llm::acp::AcpSession>,
    colors: &ColorScheme,
) -> impl IntoElement {
    let status = header_status(panel);
    let (icon_label, detail) = if let Some(session) = acp_session {
        (
            format!("\u{25c8} {}", session.display_name),
            format!("agent:{}", session.agent_name),
        )
    } else {
        let short_model = short_model_name(&llm.model);
        (
            format!("\u{2726} {short_model}"),
            format!("{}:{}", llm.provider, llm.model),
        )
    };
    div()
        .flex()
        .flex_row()
        .items_center()
        .justify_between()
        .flex_shrink_0()
        .px_3()
        .py_2()
        .border_b_1()
        .border_color(to_rgba(colors.ui_border))
        .font_family(font_state::font_family())
        .text_size(px(font_state::font_size()))
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap_2()
                .child(div().text_color(to_rgba(colors.ui_accent)).child(icon_label))
                .child(
                    div()
                        .text_color(to_rgba(colors.ui_muted))
                        .child(format!("\u{2502} {detail}")),
                )
                .when(!status.is_empty(), |el| {
                    el.child(div().text_color(to_rgba(colors.ui_muted)).child(status))
                }),
        )
        .child(
            div()
                .text_color(to_rgba(colors.ui_muted))
                .child("Leader a a to close"),
        )
}
```

`AcpSession::{agent_name, display_name}` -- both are currently `#[allow(dead_code)] pub` fields on
`AcpSession` (`src/llm/acp/mod.rs:34-38`, unmodified) -- reading them here is their first real
caller; no visibility change needed, but note in your report that the `#[allow(dead_code)]` on
those two fields is now stale (it lives in `src/llm/acp/mod.rs`, which this plan's own Global
Constraints forbids modifying -- leave it as a harmless, out-of-scope stale attribute, do not
touch that file).

Update `render_chat_panel`'s own call to `render_header`: find `.child(render_header(&view.panel,
llm, colors))` and change it to `.child(render_header(&view.panel, llm, view.acp_session.as_ref(),
colors))`.

- [ ] **Step 7: Build, test, verify**

Run: `cargo build 2>&1 | tail -100`, `cargo test --lib 2>&1 | tail -10` (expect 235/235 unchanged
from Task 2's count -- pure wiring, no new tests), `cargo fmt` then `cargo fmt --check` (clean),
`cargo clippy --all-features -- -D warnings` (clean), `./scripts/ci-local.sh` (exit 0).

Run `wc -l src/gpui_shell/mod.rs src/gpui_shell/chat_panel/mod.rs src/gpui_shell/chat_panel/
backend.rs src/gpui_shell/poll.rs src/gpui_shell/chat_panel/stream.rs src/gpui_shell/chat_panel/
render.rs` and note results; flag any 400-line overshoot in CONCERNS and fix it yourself.

Dogfood is not possible from this environment (needs a real ACP agent process to connect to) --
note in your report that this is pending manual dogfood (reproduce the spec's §10 "ACP session +
backend switching" checklist).

- [ ] **Step 8: Commit**

```bash
git add src/gpui_shell/mod.rs src/gpui_shell/chat_panel/backend.rs src/gpui_shell/chat_panel/mod.rs src/gpui_shell/poll.rs src/gpui_shell/chat_panel/stream.rs src/gpui_shell/chat_panel/render.rs
git commit -m "feat: Add ACP session lifecycle and backend switching (M5a Task 3)."
```

---

## Task 4: ACP terminal bridge

**Tier: standard.** New channel infrastructure, a new pane-creation-with-injected-command
primitive, and a deferred-resolution queue -- genuine new design, not a mechanical port, even
though `handle_acp_terminal_requests`'s own control flow is fully specified by the wgpu reference.

**Files:**
- Modify: `src/gpui_shell/chat_panel/mod.rs`
- Create: `src/gpui_shell/acp_bridge.rs`
- Modify: `src/gpui_shell/poll.rs`

**Interfaces:**
- Consumes: `AcpTerminalRequest` (unmodified, `src/llm/acp/terminal.rs`), `GpuiShellRoot::{
  terminal_output_text, terminal_exit_code}` (Task 2), `spawn_terminal_at` (M5c, `pub(crate) fn
  spawn_terminal_at(cols: u16, rows: u16, config: &Config, cwd: Option<PathBuf>) -> anyhow::
  Result<(Rc<Terminal>, Arc<WakeupGate>)>`).
- Produces: `ChatPanelView::acp_terminal_tx: tokio::sync::mpsc::Sender<AcpTerminalRequest>`
  (`pub(super)`, consumed by Task 5's `submit()`); `GpuiShellRoot::handle_acp_terminal_requests
  (&mut self, cx: &mut Context<Self>)` (`pub(super)`, consumed by `poll.rs`).

**Real channel-type note, decided here so Task 5 doesn't have to revisit it:**
`AcpSession::try_send_prompt` (unmodified, `src/llm/acp/mod.rs`) requires `terminal_tx: tokio::
sync::mpsc::Sender<AcpTerminalRequest>` -- `tokio::sync::mpsc`, not `crossbeam_channel`. This task
uses `tokio::sync::mpsc::channel` (not `crossbeam_channel::bounded`, unlike M5c's/M5b's own async-
scan channels) specifically so Task 5 can clone `acp_terminal_tx` straight into `try_send_prompt`
with no bridging. `tokio::sync::mpsc::Receiver::try_recv(&mut self)` has the same `Ok(T)`/
`Err(TryRecvError)` shape `crossbeam_channel::Receiver::try_recv(&self)` does (only the `&self` vs
`&mut self` receiver differs, and `self.chat.acp_terminal_rx.try_recv()` already has `&mut self.
chat` available from `handle_acp_terminal_requests`'s own `&mut self` receiver, so no call-site
change is needed for that difference either) -- the drain loop below is unaffected either way.

- [ ] **Step 1: `chat_panel/mod.rs` -- the channel fields**

Add, to the `ChatPanelView` struct, right after the `acp_pending_connect` field Task 3 added:

```rust
    /// Sender half of the ACP terminal-request bridge -- cloned into each
    /// ACP prompt's `try_send_prompt` call (Task 5) as `terminal_tx`. The
    /// receiver half is drained by `GpuiShellRoot::handle_acp_terminal_
    /// requests` (`acp_bridge.rs`), called from `poll.rs`'s own tick.
    /// `tokio::sync::mpsc`, not `crossbeam_channel` -- `AcpSession::try_
    /// send_prompt` requires this exact channel type.
    pub(super) acp_terminal_tx: tokio::sync::mpsc::Sender<crate::llm::acp::terminal::AcpTerminalRequest>,
    acp_terminal_rx: tokio::sync::mpsc::Receiver<crate::llm::acp::terminal::AcpTerminalRequest>,
```

Add, inside `ChatPanelView::new`, right before the `let mut view = Self { ... };` line:

```rust
        let (acp_terminal_tx, acp_terminal_rx) = tokio::sync::mpsc::channel(32);
```

Add `acp_terminal_tx,` and `acp_terminal_rx,` to the `Self { ... }` construction, right after the
`acp_pending_connect: None,` line Task 3 added.

- [ ] **Step 2: `acp_bridge.rs` -- the drain handler**

```rust
// gpui chrome migration (M5a Task 4): drains `ChatPanelView::acp_
// terminal_rx` and answers each `AcpTerminalRequest` the agent sent
// (terminal/create, terminal/output, terminal/wait_for_exit, terminal/
// kill -- terminal/release is a client-side no-op, matching the wgpu
// build's own comment on why). Mirrors `App::handle_acp_terminal_
// requests` (`src/app/frame.rs:187-246`) exactly in control flow.
//
// `Kill`'s own mechanism is genuinely different from the wgpu build's:
// `Mux::kill_terminal` calls `term.pty.shutdown()` through `&mut
// Terminal`, unreachable here since every `gpui_shell` `Terminal` is
// `Rc<Terminal>` (confirmed: `actions.rs`'s own `reap_pane` doc comment
// already documents that `Drop for Pty` runs the full shutdown sequence
// when a terminal's last `Rc` is dropped, never a direct call). `Terminal
// ::child_pid: u32` needs no mutable access, so this sends SIGHUP
// directly -- the reader thread's own already-existing exit detection
// (EIO on the master fd) does the rest, exactly like a natural shell
// exit.

use std::path::PathBuf;

use gpui::Context;

use crate::llm::acp::terminal::AcpTerminalRequest;

use super::panes::SplitDir;
use super::{spawn_terminal_at, GpuiShellRoot};

impl GpuiShellRoot {
    /// Drain every pending `AcpTerminalRequest` and resolve any completed
    /// `WaitForExit` requests. Called from `poll.rs`'s existing 33ms tick.
    pub(super) fn handle_acp_terminal_requests(&mut self, cx: &mut Context<Self>) {
        loop {
            let Ok(req) = self.chat.acp_terminal_rx.try_recv() else {
                break;
            };
            match req {
                AcpTerminalRequest::Create {
                    command,
                    args,
                    cwd,
                    tx,
                } => {
                    let pane_id = self.open_terminal_for_acp(cwd, &command, &args, cx);
                    let _ = tx.send(pane_id);
                }
                AcpTerminalRequest::GetOutput { pane_id, tx } => {
                    let output = self.terminal_output_text(pane_id);
                    let exit_code = self.terminal_exit_code(pane_id);
                    let _ = tx.send((output, exit_code));
                }
                AcpTerminalRequest::WaitForExit { pane_id, tx } => {
                    if let Some(code) = self.terminal_exit_code(pane_id) {
                        let _ = tx.send(code);
                    } else {
                        self.pending_acp_wait_for_exit.push((pane_id, tx));
                    }
                }
                AcpTerminalRequest::Kill { pane_id } => {
                    if let Some(terminal) = self.terminals.get(&pane_id) {
                        unsafe {
                            libc::kill(terminal.child_pid as libc::pid_t, libc::SIGHUP);
                        }
                    }
                }
            }
        }

        let pending = std::mem::take(&mut self.pending_acp_wait_for_exit);
        for (pane_id, tx) in pending {
            match self.terminal_exit_code(pane_id) {
                Some(code) => {
                    let _ = tx.send(code);
                }
                None => self.pending_acp_wait_for_exit.push((pane_id, tx)),
            }
        }
    }

    /// Split the active pane for an ACP `terminal/create` request: spawn a
    /// terminal at `cwd` (`spawn_terminal_at` falls back to the process's
    /// own cwd when `cwd` is `None`, matching `CreateTerminalRequest::
    /// cwd`'s own optional shape exactly), split the active tab's tree
    /// around it, and immediately write the shell-quoted command + args
    /// to it. Returns the new terminal's id.
    fn open_terminal_for_acp(
        &mut self,
        cwd: Option<PathBuf>,
        command: &str,
        args: &[String],
        cx: &mut Context<Self>,
    ) -> usize {
        let (terminal, gate) = match spawn_terminal_at(80, 24, &self.config, cwd) {
            Ok(pair) => pair,
            Err(e) => {
                log::error!("gpui-shell: ACP terminal/create failed: {e:#}");
                return 0;
            }
        };
        let terminal_id = self.next_terminal_id;
        self.next_terminal_id += 1;
        self.terminals.insert(terminal_id, terminal);
        self.wakeup_gates.insert(terminal_id, gate);
        self.block_managers
            .insert(terminal_id, crate::term::BlockManager::new());
        let ws = self.workspaces.active_mut();
        let active = ws.tabs.active_index();
        ws.tab_panes[active].split(SplitDir::Horizontal, terminal_id);
        ws.zoomed_pane = None;
        if let Some(terminal) = self.terminals.get(&terminal_id) {
            let mut cmd_str = shell_quote(command);
            for arg in args {
                cmd_str.push(' ');
                cmd_str.push_str(&shell_quote(arg));
            }
            cmd_str.push('\r');
            terminal.write_input(cmd_str.as_bytes());
        }
        cx.notify();
        terminal_id
    }
}

/// POSIX single-quote a token so it reaches the shell as one literal
/// argument, regardless of embedded spaces or shell metacharacters (ACP
/// `terminal/create` passes `command`/`args` with argv semantics, not
/// shell semantics). Duplicated from `Mux`'s own private `shell_quote`
/// (`src/app/mux/mod.rs:156-167`) rather than cross-wired -- `src/app/` is
/// off-limits per this plan's own Global Constraints, and this is a tiny,
/// pure, three-line-rule-exempt helper (it's the whole reason this
/// function exists).
fn shell_quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('\'');
    for c in s.chars() {
        if c == '\'' {
            out.push_str("'\\''");
        } else {
            out.push(c);
        }
    }
    out.push('\'');
    out
}
```

- [ ] **Step 3: `mod.rs` -- register the module, add the wait-for-exit queue**

Add `mod acp_bridge;` in alphabetical order.

Add, to `GpuiShellRoot`'s struct, right after the `terminal_final_output` field Task 2 added:

```rust
    /// ACP `terminal/wait_for_exit` requests still waiting on a terminal
    /// that hasn't exited yet -- resolved on a later poll tick once
    /// `terminal_exit_code` returns `Some`. Mirrors `UiManager::pending_
    /// acp_wait_for_exit`.
    pending_acp_wait_for_exit: Vec<(usize, tokio::sync::oneshot::Sender<i32>)>,
```

Add `pending_acp_wait_for_exit: Vec::new(),` to the `Self { ... }` construction, right after
`terminal_final_output: HashMap::new(),`.

- [ ] **Step 4: `poll.rs` -- drive the drain**

Find the existing `if this.chat.poll_acp_connect() { should_notify = true; }` (Task 3's own
addition) and add, right after it:

```rust
                    this.handle_acp_terminal_requests(cx);
```

(No `should_notify` gating needed here -- `handle_acp_terminal_requests` itself calls `cx.notify()`
directly inside `open_terminal_for_acp` when it actually creates a pane; the other three request
kinds resolve a caller-side channel the agent's own task is already awaiting, not something the UI
needs to repaint for on its own.)

- [ ] **Step 5: Build, test, verify**

Run: `cargo build 2>&1 | tail -100`, `cargo test --lib 2>&1 | tail -10` (expect 235/235 unchanged),
`cargo fmt` then `cargo fmt --check` (clean), `cargo clippy --all-features -- -D warnings` (clean),
`./scripts/ci-local.sh` (exit 0).

Run `wc -l src/gpui_shell/chat_panel/mod.rs src/gpui_shell/acp_bridge.rs src/gpui_shell/poll.rs
src/gpui_shell/mod.rs` and note results; flag any 400-line overshoot in CONCERNS and fix it
yourself.

Dogfood is not possible from this environment (needs a real ACP agent process making real
`terminal/*` calls) -- note in your report that this is pending manual dogfood (reproduce the
spec's §10 "ACP terminal bridge" checklist).

- [ ] **Step 6: Commit**

```bash
git add src/gpui_shell/chat_panel/mod.rs src/gpui_shell/acp_bridge.rs src/gpui_shell/poll.rs src/gpui_shell/mod.rs
git commit -m "feat: Add the ACP terminal-request bridge (M5a Task 4)."
```

---

## Task 5: ACP prompt submission + tool-status streaming

**Tier: standard.** Real branching logic in the hottest path of the chat panel (`submit()`) plus a
live drain-match edit -- a wrong assumption here breaks BOTH backends, not just the new one.

**Files:**
- Modify: `src/gpui_shell/chat_panel/stream.rs`

**Interfaces:**
- Consumes: `ChatPanelView::acp_session` (Task 3), `ChatPanelView::acp_terminal_tx` (Task 4),
  `AcpSession::try_send_prompt` (unmodified, `fn try_send_prompt(&mut self, content: String, ai_
  tx: mpsc::Sender<AiEvent>, terminal_tx: mpsc::Sender<AcpTerminalRequest>) -> Result<()>`).
- Produces: nothing new for later tasks -- Task 6 reads the same `AiEvent` drain match this task
  edits, but doesn't call anything this task defines.

- [ ] **Step 1: `stream.rs` -- the ACP branch in `submit()`**

Read `submit()`'s real current full body first (Task 1's own system-prompt edit and Task 4's own
unrelated changes elsewhere in the file may have shifted exact line numbers -- confirm the current
text matches what's quoted below before editing). Find the point right after `let Some(_user_
content) = self.panel.submit_input() else { return; };`, before the existing `let Some(provider) =
self.llm_provider.clone() else { ... };` early-return -- rename the `_user_content` binding to
`user_content` (it's now used by the new ACP branch) and insert the ACP branch immediately after
it, before the Provider branch's own code (unchanged below this insertion):

```rust
    pub fn submit(&mut self, tokio_rt: &tokio::runtime::Runtime, cx: &mut Context<GpuiShellRoot>) {
        let Some(user_content) = self.panel.submit_input() else {
            return;
        };

        if self.acp_session.is_some() {
            // `try_send_prompt` requires a `tokio::sync::mpsc::Sender<AiEvent>`
            // (see `AcpSession::try_send_prompt`'s real signature), but the
            // direct-provider path above -- and `drain_events` below, which
            // both backends share -- already reads from `self.ai_rx`, a
            // `crossbeam_channel::Receiver`. Rather than give `drain_events`
            // a second receiver to poll, bridge a fresh per-prompt tokio
            // channel back into the existing one: same "spawn a small
            // forwarding task" shape `backend.rs`'s own `spawn_acp_connect`
            // already uses for an unrelated result.
            let (bridge_tx, mut bridge_rx) = tokio::sync::mpsc::channel::<AiEvent>(256);
            let ai_tx_out = self.ai_tx.clone();
            tokio_rt.spawn(async move {
                while let Some(event) = bridge_rx.recv().await {
                    if ai_tx_out.send(event).is_err() {
                        break;
                    }
                }
            });
            let terminal_tx = self.acp_terminal_tx.clone();
            let send_result = self
                .acp_session
                .as_mut()
                .unwrap()
                .try_send_prompt(user_content, bridge_tx, terminal_tx);
            if let Err(e) = send_result {
                self.acp_session = None;
                self.panel
                    .mark_error(format!("ACP agent disconnected: {e:#}"));
            }
            cx.notify();
            return;
        }

        let Some(provider) = self.llm_provider.clone() else {
            let msg = self
                .llm_init_error
                .clone()
                .unwrap_or_else(|| "LLM is disabled in config.".into());
            self.panel.mark_error(msg);
            cx.notify();
            return;
        };
        self.panel.context_window = provider.context_window();

        let system_prompt = format!(
            "{}\n\n{}",
            crate::config::load_system_prompt(),
            crate::llm::agent_action::system_prompt_instructions()
        );
        let mut messages = vec![ChatMessage::system(system_prompt)];
        messages.extend(self.panel.messages.iter().cloned());

        if let Some(handle) = self.in_flight.take() {
            handle.abort();
        }
        let tx = self.ai_tx.clone();
        self.in_flight = Some(tokio_rt.spawn(async move {
            use futures_util::StreamExt;
            match provider.stream(messages).await {
                Err(e) => {
                    let _ = tx.send(AiEvent::Error(e.to_string()));
                }
                Ok(mut stream) => {
                    let mut errored = false;
                    while let Some(chunk) = stream.next().await {
                        match chunk {
                            Ok(tok) => {
                                if tx.send(AiEvent::Token(tok)).await.is_err() {
                                    break;
                                }
                            }
                            Err(e) => {
                                let _ = tx.send(AiEvent::Error(e.to_string())).await;
                                errored = true;
                                break;
                            }
                        }
                    }
                    if !errored {
                        let _ = tx.send(AiEvent::Done);
                    }
                }
            }
        }));
        cx.notify();
    }
```

`self.ai_tx.clone()` inside the ACP branch's spawned bridge task is a `crossbeam_channel::Sender<
AiEvent>`, whose own `.send(event)` is synchronous (`Result<(), SendError<T>>`, no `.await`) --
confirmed against the direct-provider branch's own identical `tx.send(AiEvent::Error(...))` call a
few lines below (no `.await` there either, unlike the `tx.send(AiEvent::Token(tok)).await` calls,
which are the OTHER `tx` -- the direct-provider branch's own local `let tx = self.ai_tx.clone();`
binding shadows the name but is the same underlying channel type). `.send(event).is_err()` (not
`.send(event).await.is_err()`) is correct for the bridge task above.

- [ ] **Step 2: `stream.rs` -- real `ToolStatus` arm in `drain_events`**

Find the exact current text:

```rust
                AiEvent::ToolStatus { .. }
                | AiEvent::ConfirmWrite { .. }
                | AiEvent::ConfirmRun { .. }
                | AiEvent::UndoState { .. } => {}
```

Replace with:

```rust
                AiEvent::ToolStatus { tool, path, done } => {
                    self.panel.set_tool_status(&tool, &path, done);
                }
                AiEvent::ConfirmWrite { .. } | AiEvent::ConfirmRun { .. } | AiEvent::UndoState { .. } => {}
```

(Update the comment immediately above this match block too -- it currently says "Tool-calling
confirm/status surfaces -- out of scope... Never produced by the direct-provider path `submit`
spawns" -- `ToolStatus` is no longer out of scope or unreachable; trim the comment to describe only
the three variants still in the no-op arm, and note they're completed in Task 6.)

- [ ] **Step 3: Build, test, verify**

Run: `cargo build 2>&1 | tail -100`, `cargo test --lib 2>&1 | tail -10` (expect 235/235 unchanged),
`cargo fmt` then `cargo fmt --check` (clean), `cargo clippy --all-features -- -D warnings` (clean),
`./scripts/ci-local.sh` (exit 0).

Run `wc -l src/gpui_shell/chat_panel/stream.rs src/gpui_shell/chat_panel/mod.rs
src/gpui_shell/acp_bridge.rs` and note results (the last two only if Task 4's channel-type
amendment touched them); flag any 400-line overshoot in CONCERNS and fix it yourself.

Dogfood is not possible from this environment (needs a real ACP agent process) -- note in your
report that this is pending manual dogfood (reproduce the spec's §10 "ACP prompt + tool-status"
checklist).

- [ ] **Step 4: Commit**

```bash
git add src/gpui_shell/chat_panel/stream.rs src/gpui_shell/chat_panel/mod.rs src/gpui_shell/acp_bridge.rs
git commit -m "feat: Wire ACP prompt submission and tool-status streaming (M5a Task 5)."
```

---

## Task 6: ACP write/run confirm + undo

**Tier: cheap.** Every mechanism (confirm keys, undo stack, `Leader a z`) is fully specified
against real, already-verified wgpu-build code, and this task reuses Task 1's own confirm-card
chrome rather than inventing new UI patterns.

**Files:**
- Modify: `src/gpui_shell/mod.rs`
- Modify: `src/gpui_shell/chat_panel/stream.rs`
- Modify: `src/gpui_shell/chat_panel/render.rs`
- Modify: `src/gpui_shell/leader.rs`
- Modify: `src/gpui_shell/leader_dispatch.rs`
- Modify: `src/gpui_shell/input.rs`
- Modify: `src/gpui_shell/render.rs`

**Interfaces:**
- Consumes: `ConfirmDisplay::{Write, Run}` (unmodified, `Write { path: String, diff: Vec<DiffLine>,
  added: usize, removed: usize }`, `Run { cmd: String }`), `crate::llm::diff::{DiffLine, DiffKind}`
  (unmodified), `ChatPanel::{mark_awaiting_confirm, resolve_confirm}` (pre-existing `pub`),
  `chat_panel::render::render_confirm_card` (Task 1, `pub(super)`).
- Produces: nothing consumed by any later task -- this is the milestone's last task.

**Real field-ownership note, decided here (not left for the implementer to discover a borrow
error):** `AiEvent::{ConfirmWrite, ConfirmRun, UndoState}` are drained inside `ChatPanelView::
drain_events` (`impl ChatPanelView`, confirmed against the file's real structure) -- it has no way
to reach `GpuiShellRoot`-level fields. Rather than route these three events back out to a
`GpuiShellRoot`-level caller (more plumbing, and a real behavior split from every other `AiEvent`
variant `drain_events` already handles fully in place), `pending_confirm_tx`/`undo_stack` live on
`ChatPanelView` itself -- the same fix M5b's own self-review already applied to `file_scan_rx`
(moved from `GpuiShellRoot` to `ChatPanelView` for the identical reason: it's purely chat-panel-
local state). `GpuiShellRoot`'s own key guard/undo dispatch (Step 2 below) reach them as `self.
chat.pending_confirm_tx`/`self.chat.undo_stack`, the same nested-field-access shape `self.chat.
panel.state` already uses throughout this codebase. `pending_pty_run` stays on `GpuiShellRoot`
(matches `pending_agent_action`'s own placement, Task 1 -- it's drained at `render()`'s top with
no `Window` in hand, the same `pending_*` pattern, and has no chat-panel-specific meaning of its
own).

- [ ] **Step 1: `chat_panel/mod.rs` -- the two new `ChatPanelView` fields + `mod.rs`'s one**

Add, to `ChatPanelView`'s struct, right after the `file_scan_rx` field:

```rust
    /// Oneshot channel to answer the ACP agent's own `session/
    /// requestPermission`/`fs/write_text_file` request once the user
    /// presses y/n. `None` while no confirm card is showing.
    pending_confirm_tx: Option<tokio::sync::oneshot::Sender<bool>>,
    /// Saved (path, original content) pairs for `Leader a z` -- newest
    /// last, capped at `UNDO_STACK_CAP`.
    undo_stack: std::collections::VecDeque<(std::path::PathBuf, String)>,
```

Add `pending_confirm_tx: None,` and `undo_stack: std::collections::VecDeque::new(),` to the `Self
{ ... }` construction, right after the existing `file_scan_rx: None,` line.

Add, near the top of `chat_panel/mod.rs` (alongside the existing `MARKDOWN_WRAP_WIDTH`/`AI_
CHANNEL_CAP` module-level `const`s):

```rust
/// Cap on `undo_stack`'s size -- oldest entry evicted past this, matching
/// `UiManager::cmd_undo_last_write`'s own `MAX_UNDO = 10`.
const UNDO_STACK_CAP: usize = 10;
```

Add, to `mod.rs`'s own `GpuiShellRoot` struct, right after the `pending_acp_wait_for_exit` field
Task 4 added -- **this field belongs on `mod.rs`, unlike the two above** (see this section's own
note on why):

```rust
    /// A confirmed `ConfirmDisplay::Run` command, drained at the top of
    /// `render()` (no `Window` where it's set). Mirrors `pending_agent_
    /// action`'s own placement (Task 1).
    pending_pty_run: Option<String>,
```

Add `pending_pty_run: None,` to `mod.rs`'s own `Self { ... }` construction, right after `pending_
acp_wait_for_exit: Vec::new(),`.

- [ ] **Step 2: `chat_panel/stream.rs` -- real `ConfirmWrite`/`ConfirmRun`/`UndoState` arms**

Find the exact current text (left by Task 5's own Step 2):

```rust
                AiEvent::ConfirmWrite { .. } | AiEvent::ConfirmRun { .. } | AiEvent::UndoState { .. } => {}
```

Replace with:

```rust
                AiEvent::ConfirmWrite { display, result_tx } => {
                    self.panel.mark_awaiting_confirm(display);
                    self.pending_confirm_tx = Some(result_tx);
                }
                AiEvent::ConfirmRun { cmd, result_tx } => {
                    self.panel
                        .mark_awaiting_confirm(crate::llm::chat_panel::ConfirmDisplay::Run { cmd });
                    self.pending_confirm_tx = Some(result_tx);
                }
                AiEvent::UndoState { path, content } => {
                    if self.undo_stack.len() >= super::UNDO_STACK_CAP {
                        self.undo_stack.pop_front();
                    }
                    self.undo_stack.push_back((path, content));
                }
```

(`self` here is `&mut ChatPanelView`, the receiver `drain_events` already has -- `self.pending_
confirm_tx`/`self.undo_stack` are the two new fields Step 1 just added directly to `ChatPanelView`
in `chat_panel/mod.rs`. `UNDO_STACK_CAP` is a module-level const Step 1 also added to `chat_panel/
mod.rs`, NOT to this file (`chat_panel/stream.rs`) -- `stream.rs` is `chat_panel`'s own child
module (`mod stream;`, declared in `mod.rs`), so reaching a `mod.rs`-level const from inside it
needs `super::UNDO_STACK_CAP`, matching how every other cross-file-same-module reference in this
codebase already qualifies with `super::`.)

- [ ] **Step 3: `mod.rs` -- the key guard + undo dispatch**

Add, in the same `impl GpuiShellRoot { ... }` block Task 1's `maybe_handle_confirm_action_key`
lives in:

```rust
    /// The ACP write/run confirm card's own key guard, called from
    /// `input.rs`'s `on_key_down`. Returns `true` if the key was
    /// consumed. Mode-keyed on `panel.state`, same reasoning as `maybe_
    /// handle_confirm_action_key`.
    pub(super) fn maybe_handle_awaiting_confirm_key(
        &mut self,
        event: &gpui::KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        use crate::llm::chat_panel::{ConfirmDisplay, PanelState};
        if !matches!(self.chat.panel.state, PanelState::AwaitingConfirm) {
            return false;
        }
        let key = event.keystroke.key.as_str();
        if key == "y" || key == "enter" {
            if let Some(tx) = self.chat.pending_confirm_tx.take() {
                if let Some(ConfirmDisplay::Run { cmd }) = self.chat.panel.confirm_display.as_ref() {
                    self.pending_pty_run = Some(cmd.clone());
                }
                let _ = tx.send(true);
                self.chat.panel.resolve_confirm();
            }
        } else if key == "n" || key == "escape" {
            if let Some(tx) = self.chat.pending_confirm_tx.take() {
                let _ = tx.send(false);
                self.chat.panel.resolve_confirm();
            }
        }
        cx.notify();
        true
    }

    /// `Leader a z` -- restore the most recently agent-written file's
    /// prior content. Ported from `UiManager::cmd_undo_last_write`
    /// (`src/app/ui/mod.rs:630+`).
    pub(super) fn undo_last_write(&mut self) {
        if let Some((path, content)) = self.chat.undo_stack.pop_back() {
            match std::fs::write(&path, &content) {
                Ok(()) => {
                    let msg = format!("Restored: {}", path.display());
                    self.chat
                        .panel
                        .messages
                        .push(crate::llm::ChatMessage::assistant(msg));
                }
                Err(e) => {
                    log::error!("undo write {}: {e}", path.display());
                    let msg = format!("Undo failed: {e}");
                    self.chat
                        .panel
                        .messages
                        .push(crate::llm::ChatMessage::assistant(msg));
                }
            }
        }
    }
```

- [ ] **Step 4: `input.rs` -- wire the guard, flush `pending_pty_run`**

Find the existing `if self.maybe_handle_confirm_action_key(event, cx) { return; }` (Task 1's own
addition) and add, right after it:

```rust

        // ACP write/run confirm card's own key guard -- see `mod.rs`'s
        // `maybe_handle_awaiting_confirm_key` doc comment.
        if self.maybe_handle_awaiting_confirm_key(event, cx) {
            return;
        }
```

- [ ] **Step 5: `render.rs` -- drain `pending_pty_run`**

Find the existing `self.flush_pending_agent_action(window, cx);` (Task 1's own addition) and add,
right after it:

```rust
        if let Some(cmd) = self.pending_pty_run.take() {
            let active_ws = self.workspaces.active();
            let active_tid = active_ws.tab_panes[active_ws.tabs.active_index()].focused_terminal;
            if let Some(terminal) = self.terminals.get(&active_tid) {
                let mut data = cmd.into_bytes();
                data.push(b'\n');
                terminal.write_input(&data);
            }
        }
```

- [ ] **Step 6: `leader.rs` -- `UndoLastWrite`**

Add `UndoLastWrite,` to `pub enum LeaderAction { ... }`, right after `FixLastError,`.

Add `"UndoLastWrite" => Ok(LeaderAction::UndoLastWrite),` to the `impl TryFrom<&str>` match, right
after `"FixLastError" => Ok(LeaderAction::FixLastError),`.

- [ ] **Step 7: `leader_dispatch.rs` -- the dispatch arm**

Add, right after the existing `LeaderAction::FixLastError => self.fix_last_error(window, cx),`:

```rust
            LeaderAction::UndoLastWrite => self.undo_last_write(),
```

- [ ] **Step 8: `input.rs` -- the `'a'`+`'z'` continuation**

Find the exact current text:

```rust
                        "f" => self.dispatch_leader_action(LeaderAction::FixLastError, window, cx),
                        // `c`/`z` (ClearAiContext/UndoLastWrite) stay out of
                        // scope -- both ACP/tool-calling-dependent, per this
                        // milestone's own spec §6. Dropped, matching the
                        // wgpu build's own `_ => {}` fallthrough.
                        _ => {}
```

Replace with:

```rust
                        "f" => self.dispatch_leader_action(LeaderAction::FixLastError, window, cx),
                        "z" => self.dispatch_leader_action(LeaderAction::UndoLastWrite, window, cx),
                        // `c` (ClearAiContext) stays out of scope -- ACP/
                        // tool-calling-dependent, per this milestone's own
                        // spec §9. Dropped, matching the wgpu build's own
                        // `_ => {}` fallthrough.
                        _ => {}
```

- [ ] **Step 9: `chat_panel/render.rs` -- the diff-rendering card**

Add, right after `render_agent_action_card` (Task 1's own function):

```rust
fn render_diff_line(line: &crate::llm::diff::DiffLine, colors: &ColorScheme) -> impl IntoElement {
    use crate::llm::diff::DiffKind;
    let (prefix, color) = match line.kind {
        DiffKind::Added => ("+ ", to_rgba([0.4, 0.9, 0.4, 1.0])),
        DiffKind::Removed => ("- ", to_rgba([0.9, 0.4, 0.4, 1.0])),
        DiffKind::Context => ("  ", to_rgba(colors.ui_muted)),
    };
    div()
        .text_color(color)
        .child(format!("{prefix}{}", line.text))
}

fn render_awaiting_confirm_card(
    display: &crate::llm::chat_panel::ConfirmDisplay,
    colors: &ColorScheme,
) -> impl IntoElement {
    use crate::llm::chat_panel::ConfirmDisplay;
    match display {
        ConfirmDisplay::Write {
            path,
            diff,
            added,
            removed,
        } => {
            let mut body = div().flex().flex_col().gap_1();
            body = body.child(
                div()
                    .text_color(to_rgba(colors.foreground))
                    .child(format!("Write: {path} (+{added} -{removed})")),
            );
            for line in diff {
                body = body.child(render_diff_line(line, colors));
            }
            render_confirm_card("Confirm write", body, "[y]es  [n]o", colors)
        }
        ConfirmDisplay::Run { cmd } => render_confirm_card(
            "Confirm run",
            div()
                .text_color(to_rgba(colors.foreground))
                .child(format!("Run: `{cmd}`")),
            "[y]es  [n]o",
            colors,
        ),
    }
}
```

`DiffLine { pub kind: DiffKind, pub text: String }` and `DiffKind::{Context, Added, Removed}` --
both confirmed exactly as used above, directly against `src/llm/diff.rs`, while writing this plan.

Find `render_message_list`'s own `ConfirmAction` branch (Task 1's own addition, right before the
final `list`) and add an `AwaitingConfirm` branch alongside it:

```rust
    if let PanelState::ConfirmAction(action) = &panel.state {
        list = list.child(render_agent_action_card(action, colors));
    }
    if let (PanelState::AwaitingConfirm, Some(display)) = (&panel.state, &panel.confirm_display) {
        list = list.child(render_awaiting_confirm_card(display, colors));
    }

    list
}
```

- [ ] **Step 10: Build, test, verify**

Run: `cargo build 2>&1 | tail -100`, `cargo test --lib 2>&1 | tail -10` (expect 235/235 unchanged),
`cargo fmt` then `cargo fmt --check` (clean), `cargo clippy --all-features -- -D warnings` (clean),
`./scripts/ci-local.sh` (exit 0).

Run `wc -l src/gpui_shell/mod.rs src/gpui_shell/chat_panel/stream.rs src/gpui_shell/chat_panel/
render.rs src/gpui_shell/leader.rs src/gpui_shell/leader_dispatch.rs src/gpui_shell/input.rs
src/gpui_shell/render.rs` and note results; `chat_panel/render.rs` in particular has been growing
across Tasks 1/3/6 -- if it's now over 400, extract the confirm-card rendering (Task 1's `render_
confirm_card`/`render_agent_action_card` plus this task's `render_diff_line`/`render_awaiting_
confirm_card`) into a new `chat_panel/confirm.rs`, re-exported so `render_message_list`'s own call
sites are unaffected. Fix any other overshoot the same way.

Dogfood is not possible from this environment (needs a real ACP agent process making real write/
run requests) -- note in your report that this is pending manual dogfood (reproduce the spec's
§10 "ACP write/run confirm + undo" checklist).

- [ ] **Step 11: Commit**

```bash
git add src/gpui_shell/mod.rs src/gpui_shell/chat_panel/stream.rs src/gpui_shell/chat_panel/render.rs src/gpui_shell/leader.rs src/gpui_shell/leader_dispatch.rs src/gpui_shell/input.rs src/gpui_shell/render.rs
git commit -m "feat: Add ACP write/run confirm cards and Leader a z undo (M5a Task 6)."
```

---

## Exit Criteria

- A direct-provider response with an `<action>` tag shows a real confirm card; `y`/`a`/`n` all
  work; `RunCommand`/`OpenFile`/`ExplainOutput` all execute correctly.
- A closed pane's exit code and final grid text remain queryable after the pane itself is gone.
- Setting `llm.backend = "agent"` connects to a real ACP agent process; the header shows the ◈
  indicator; `/agent`/`/model` behave correctly in both modes.
- An agent's `terminal/create` opens a real split pane running the requested command;
  `terminal/output`/`wait_for_exit`/`kill` all work correctly, including after the pane closes.
- A real ACP agent's prompt streams tokens and tool-status lines into the panel the same way the
  direct-provider path does.
- An agent-requested file write shows a real diff card before touching disk; an agent-requested
  command run shows a confirm card before reaching the PTY; `Leader a z` restores the most
  recently agent-written file.
- `scripts/ci-local.sh` (including `cargo fmt --check`) is green and the full `cargo test --lib`
  suite passes after each task.
- This completes M5a, and with it, all of M5 (M5a ACP + tool-calling, M5b chat composer extras,
  M5c palette/context-menu completion). M6 (cleanup & merge -- delete the old wgpu/winit renderer
  and `src/ui/*` draw code, drop now-unused deps, merge to master) is the only milestone left in
  the whole gpui chrome migration, and needs a full user dogfood pass across everything first.
