# M5a — ACP Agent Backend & Tool-Calling Design

## 1. Overview

M5a is the third and final sub-milestone of M5, closing M3b's own largest deferred item: the ACP
(Agent Client Protocol) agent backend, tool-calling, and every confirm-prompt/undo surface that
exists only to gate a tool call. It is the largest milestone in the whole gpui migration by a wide
margin -- larger than any single milestone in M0-M5c -- because it is a genuinely new subsystem
(a real async protocol client, a new terminal-request bridge, new confirm-card UI, undo state),
not a "port the logic, rebuild the rendering" task against something already ported.

Three real findings from investigating the wgpu build, each changing scope in a load-bearing way:

1. **The inline-action confirm flow (`PanelState::ConfirmAction`) needs NO ACP at all.** M3b's own
   `stream.rs` doc comment says "with no tools ... there is nothing for those surfaces to
   confirm" -- but `ChatPanel::mark_done()` (already called by `gpui_shell`'s own `stream.rs`,
   confirmed via `AiEvent::Done => self.panel.mark_done()`) unconditionally runs `parse_action_
   from_response` on every assistant reply, from EITHER backend. The only reason this path is
   dead today is that `agent_action::system_prompt_instructions()` is never appended to the
   system prompt, so the LLM is never told the `<action>` tag format exists. Wiring this up is a
   small, independent, immediately-valuable task with zero ACP dependency -- Task 1, and it goes
   first because the confirm-card UI it needs is shared with ACP's own write/run confirm flow
   (Task 5), so building it once against the simpler trigger pays for both.
2. **`agent-client-protocol`/`agent-client-protocol-tokio` are already unconditional workspace
   dependencies** (`Cargo.toml`, not feature-gated to the `petruterm` binary) -- both binaries
   link the same `petruterm` lib crate, so `AcpSession`/the whole protocol handler chain is
   already reachable from `gpui_shell` with zero `Cargo.toml` change.
3. **`AcpSession`/`run_session` (`src/llm/acp/{mod,session}.rs`, 472 lines combined) are already
   fully engine-agnostic** -- pure tokio/`agent_client_protocol` code, zero winit references. This
   is the single largest piece of real logic this milestone needs and it ports as-is, unmodified,
   the same "port the logic" relationship `ChatPanel` itself already has. What's genuinely new is
   everything AROUND it: the gpui-side connect/poll bridge (mirrors `spawn_acp_connect`/`poll_acp_
   connect`, dropping the winit `EventLoopProxy` wakeup the same way every prior async-scan
   milestone already has), the terminal-request bridge (`AcpTerminalRequest`, `src/llm/acp/
   terminal.rs`, also engine-agnostic and reused as-is -- but its MUX-side handler,
   `handle_acp_terminal_requests`, needs new `gpui_shell` state that doesn't exist yet: per-
   terminal exit-code tracking and final-output caching, neither of which any current `gpui_shell`
   code needs for anything else), and all the new UI (confirm cards, diff rendering, undo).

## 2. Global Constraints

Carried forward unchanged from every milestone this session:

- 400-line module limit -- current state, verified fresh while writing this spec: `input.rs` 377,
  `render.rs` 393, `render_callbacks.rs` 224, `leader_dispatch.rs` 160, `leader.rs` 251,
  `palette_dispatch.rs` 220, `poll.rs` 259, `chat_panel/mod.rs` 259, `chat_panel/render.rs` 384,
  `chat_panel/stream.rs` 263, `ai_actions.rs` 96, `key_write.rs` 64. `chat_panel/render.rs` and
  `render.rs` have the least headroom and are the most likely to need a split across this
  milestone's several UI-adding tasks -- watch `wc -l` after every task, fix any real overshoot
  via extraction rather than deferring it.
- `scripts/ci-local.sh` must stay green after every task (clippy `-D warnings`, `fmt --check`,
  full `cargo test --lib`, `cargo audit`); run `cargo fmt` proactively before every commit.
- Tests cover **logic only** -- no painting/hover/hit-testing/real-subprocess tests; every UI-
  facing or real-agent-process-facing item below is a dogfood step (§9), not a unit test.
  `agent_action::parse_action_from_response`/`diff::{diff_lines,compress_diff}` already have their
  own coverage (unmodified by this milestone) and are not re-tested here.
- Commit format: `type: Message.` per `AGENTS.md`.
- `#[allow(dead_code)]` (narrowly scoped, comment naming the removing task) for anything built
  ahead of its first caller within this plan's own task sequence.
- **Key-guard shape for confirm cards, matching `InfoOverlay`'s own precedent (not a new
  exception):** both confirm-card states (`AwaitingConfirm` for ACP writes/runs, `ConfirmAction`
  for inline actions) intercept keys the same way `InfoOverlay` does -- keyed on `panel.state`
  (a mode flag), not `is_focused(window)`. Unlike the file picker (M5b, still composer-focused
  throughout), a confirm card genuinely takes over: no typing is meaningful while one is showing,
  matching the wgpu build's own key routing (`y`/`n`/`a`/Enter/Escape only, nothing else).
- **Do not modify `src/app/`, `src/ui/`, `src/llm/acp/`, `src/llm/agent_action.rs`, or `src/llm/
  diff.rs`** -- every one of those is either already engine-agnostic and reused as-is, or is
  wgpu-only code this milestone must not regress. `src/llm/chat_panel/mod.rs` (the shared,
  already-ported `ChatPanel`) may be READ freely but not restructured -- everything this milestone
  needs from it (`mark_awaiting_confirm`, `resolve_confirm`, `resolve_action_yes/no`,
  `auto_confirm_actions`, `confirm_display`) already exists as real `pub` methods/fields.
- **Real, verified fact about the drain-and-execute split:** every wgpu-build "pending" field this
  milestone ports (`pending_agent_action`, `pending_pty_run`-equivalent for `ConfirmDisplay::Run`,
  `pending_confirm_tx`) exists because `poll_ai_events`'s own event-draining runs with no `Window`
  in hand (it's driven from the winit event loop's own tick, not a keyboard/mouse callback) --
  `gpui_shell`'s equivalent poll tick (`poll.rs`) has the identical constraint (no `Window`
  parameter). This is the same `pending_*`-drained-at-`render()` pattern this migration has used
  repeatedly (M4a's `pending_palette_action`, M5c's `pending_send_to_chat`) -- Tasks 1 and 5 both
  use it again, not inventing a new mechanism.

## 3. Task 1 -- Inline-action confirm flow (no ACP dependency)

**What's reused:** `agent_action::{AgentAction, system_prompt_instructions, parse_action_from_
response}` (unmodified, already called by `mark_done()`), `ChatPanel::{resolve_action_yes,
resolve_action_no, auto_confirm_actions, confirm_display}` (all pre-existing `pub`), `ai_actions.
rs::last_terminal_lines` (M5b, currently `fn last_terminal_lines(&self, n: usize) -> String`
`private` to that file -- widened to `pub(super)` for `ExplainOutput` to call it).

**New code:**
- `chat_panel/stream.rs`'s `submit()`: append `agent_action::system_prompt_instructions()` to the
  system message, mirroring the wgpu build's own `self.system_prompt` construction (`src/app/ui/
  mod.rs`'s `rewire_backend`'s Agent branch builds it once; the Provider branch's own `submit_ai_
  query` appends the same instructions -- confirmed by reading `providers.rs`/`mod.rs` in full).
- A new field on `GpuiShellRoot`: `pending_agent_action: Option<AgentAction>`, drained at the top
  of `render()` (same `pending_*`-drained-at-`render()` shape as `pending_palette_action`/
  `pending_send_to_chat`).
- `GpuiShellRoot::maybe_handle_confirm_action_key(&mut self, event: &KeyDownEvent, window: &mut
  Window, cx: &mut Context<Self>) -> bool`: keyed on `matches!(self.chat.panel.state, PanelState::
  ConfirmAction(_))`. `y`/Enter -> `resolve_action_yes()` result stored in `pending_agent_action`;
  `a` -> `self.chat.panel.auto_confirm_actions = true;` then the same `y` path; `n`/Escape ->
  `resolve_action_no()`. Checked in `input.rs`'s guard stack alongside `maybe_handle_file_picker_
  key` (both mode-keyed, both checked before the composer's own focus-keyed guard).
- `GpuiShellRoot::flush_pending_agent_action(&mut self, cx: &mut Context<Self>)` -- named to match
  the wgpu build's own method (`src/app/frame.rs:259-316`), called from `render()`'s own top
  (alongside the drain that stores into `pending_agent_action`, or as the guard's own direct
  effect -- decide the exact call shape at plan time): `RunCommand` writes to the active terminal
  (reusing the same active-terminal lookup `ai_actions.rs` already established) and pushes an
  assistant message noting it ran; `OpenFile` resolves against the active terminal's cached cwd
  and spawns `open`; `ExplainOutput` calls `ai_actions.rs`'s own `last_terminal_lines`, sets
  `panel.input`, and calls `self.chat.submit(...)` -- identical shape to `ai_actions.rs`'s own
  `explain_last_output`, confirming this and Task 1's own `ExplainOutput` arm should share code
  rather than duplicate it (a small refactor: extract `ai_actions.rs`'s `run_ai_query`-equivalent
  tail into something both call).
- A confirm-card UI in `chat_panel/render.rs`: replaces the plain header-status text ("confirm
  action") with a real card (bordered box, the pending action's own description -- `RunCommand`
  shows the command + explanation, `OpenFile` shows the path, `ExplainOutput` shows the line
  count -- plus a `[y]es / [a]lways / [n]o` hint row), rendered as a child of the message list
  when `matches!(panel.state, PanelState::ConfirmAction(_))`, following the same "read-only
  `impl IntoElement` built from plain `&ChatPanel` state" shape every other message-list element
  already uses (no new callback plumbing needed -- this card has no clickable rows, only the
  keyboard guard above).

## 4. Task 2 -- Terminal-bridge foundation: exit codes + final output

**Independent of Task 1.** Pure new state + wiring, no UI, no ACP dependency yet -- this is what
Task 6's own `AcpTerminalRequest::{GetOutput, WaitForExit}` handlers will read from, but it's
real, testable infrastructure on its own (gpui_shell currently discards every terminal's exit code
the instant its pane closes, which is itself a small, real, previously-unaddressed gap -- the
`PtyEvent::Exit(_)` `poll.rs` already drains and ignores).

**What's reused:** `PtyEvent::Exit(code)` (already drained, code currently discarded),
`Terminal::with_term`/grid-reading (the exact pattern `ai_actions.rs`/`blocks.rs` already use).

**New code:**
- Two new `GpuiShellRoot` fields: `terminal_exit_codes: HashMap<usize, i32>` (mirrors `Mux::
  terminal_exit_codes` exactly), `terminal_final_output: HashMap<usize, String>` (mirrors `Mux::
  terminal_final_output` exactly).
- `poll.rs`'s existing `PtyEvent::Exit(code) => { exited_terminals.push(id); }` -- the `code` is
  currently thrown away; capture it into `terminal_exit_codes` at this point.
- `reap_pane`/`close_tab_at`'s own terminal-removal step (`actions.rs`/`leader_dispatch.rs` --
  confirm exact call sites at plan time) gains one line each, right before the terminal is
  actually removed from `self.terminals`: capture its current grid text into `terminal_final_
  output` via a new small helper mirroring `Mux::terminal_output_text`'s own full-grid read
  (`src/app/mux/mod.rs:637-661` -- a different, fuller read than `ai_actions.rs`'s bottom-N-lines
  helper: this one reads every visible row and trims trailing empties, matching what ACP's
  `terminal/output` actually needs to hand back).
- Two new accessor methods on `GpuiShellRoot`: `terminal_output_text(&self, terminal_id: usize) ->
  String` (checks `self.terminals` first for a live terminal, falls back to `terminal_final_
  output`, mirroring `Mux::terminal_output_text`'s own two-branch shape exactly), `terminal_exit_
  code(&self, terminal_id: usize) -> Option<i32>` (mirrors `Mux::terminal_exit_code`'s own "`None`
  while still in `self.terminals`, else look up the cache" logic).
- A bound on both new maps' growth, matching `Mux`'s own (confirmed real, not assumed -- `Mux::
  retain_closed_terminal` evicts the oldest entry once a cap is hit): reuse the same shape for
  `terminal_final_output`, evicting the oldest entry past a small cap (e.g. 64) so a long session
  with many opened-and-closed panes can't grow this unboundedly.

## 5. Task 3 -- ACP session lifecycle + backend switching

**Independent of Task 2** (no shared state), but Task 5's ACP prompt submission (dispatched later)
needs this task's own `acp_session` field to branch on, so this task establishes it.

**What's reused:** `crate::llm::acp::AcpSession::connect`/`try_send_prompt`/`is_idle` (unmodified,
called exactly as the wgpu build calls them), `crate::config::llm_view::{llm_runtime_view,
agent_display_name}` (unmodified, pure config-reading), `crate::config::schema::LlmBackend`
(unmodified).

**New code:**
- `ChatPanelView` gains `acp_session: Option<AcpSession>` and `acp_pending_connect: Option<tokio::
  sync::oneshot::Receiver<Result<AcpSession, String>>>` (mirrors `UiManager`'s own two fields
  exactly).
- `ChatPanelView::rewire_backend(&mut self, config: &Config, tokio_rt: &tokio::runtime::Runtime)`
  -- mirrors `UiManager::rewire_backend` (`src/app/ui/providers.rs:49-86`), minus the
  `wakeup_proxy: EventLoopProxy<()>` parameter: the spawned connect task's own wakeup call
  (`wakeup.send_event(())`) is dropped entirely, matching every prior async-spawn-then-poll
  milestone this session has built (M5c's branch/workspace scans, M5b's file-picker scan) -- the
  33ms poll tick IS the wake mechanism here, no separate nudge needed. `Provider` branch calls the
  existing `rewire_provider`; `Agent` branch spawns the connect (a small new free function
  mirroring `spawn_acp_connect` minus its own wakeup parameter) and stores the receiver.
- `ChatPanelView::poll_acp_connect(&mut self) -> bool` -- mirrors `UiManager::poll_acp_connect`
  exactly (drain the oneshot, set `acp_session` or `llm_init_error`), called from `poll.rs`'s
  existing tick as `this.chat.poll_acp_connect()`.
- `rewire_backend` (this task's own new method) replaces `rewire_provider` as the call site used
  by `poll.rs`'s config-hot-reload branch and `ChatPanelView::new`'s own construction-time call --
  both currently call `rewire_provider` unconditionally regardless of `config.llm.backend`, a real,
  previously-unaddressed gap (today, setting `backend = "agent"` in Lua config silently does
  nothing different in `gpui_shell`).
- `chat_panel/stream.rs`'s `handle_slash_command`'s `"model"` and `"agent"` arms gain the same
  backend-aware branching the wgpu build's own `providers.rs` has (`LlmBackend::Agent` -> "use
  /agent"/"cannot set model" messages for `/model`; `/agent <name>` updates `config.llm.agent`'s
  `command` and calls `rewire_backend` again) -- both arms currently hardcode provider-only
  behavior (confirmed: `/agent` currently unconditionally reports "not available").
- `chat_panel/render.rs`'s header gains the ◈/✦ distinction (`src/app/renderer/chat.rs:456`'s own
  `format!(" \u{25c8} {name}")` for Agent vs the existing `\u{2726} {short_model}` for Provider),
  branching on `view.acp_session.is_some()` (or, while a connect is pending, a "connecting..."
  variant -- matching the wgpu build's own `llm_init_error`-during-pending-connect display, which
  the plan should confirm by reading the real header-building code before finalizing this step).

## 6. Task 4 -- ACP terminal bridge

**Depends on Task 2** (exit-code/output-cache state, read by this task's `GetOutput`/`WaitForExit`
handlers). **Does not depend on Task 3** -- the channel this task builds is constructed
unconditionally in `ChatPanelView::new`, the same way `ai_tx`/`ai_rx` already are, whether or not
an ACP session ever connects; it simply sits unused (drained, always empty) until Task 5 gives
`submit()`'s ACP branch something to send into it. **Ordered before Task 5 deliberately**: Task
5's own `try_send_prompt` call needs a real, already-existing `terminal_tx: mpsc::Sender<
AcpTerminalRequest>` to pass in, and that sender is this task's own `acp_terminal_tx` field, not a
fresh per-prompt channel -- building the channel here first, then wiring `submit()` to it in Task
5, avoids Task 5 ever needing a placeholder value for that parameter.

**What's reused:** `AcpTerminalRequest` (unmodified enum, `src/llm/acp/terminal.rs`), the exact
`handle_acp_terminal_requests` control flow (`src/app/frame.rs:187-246`) as the reference for what
each variant does.

**New code:**
- `ChatPanelView` gains `acp_terminal_tx: crossbeam_channel::Sender<AcpTerminalRequest>` /
  `acp_terminal_rx: crossbeam_channel::Receiver<AcpTerminalRequest>` (a bounded channel, mirrors
  `UiManager`'s own field pair exactly), both constructed once in `ChatPanelView::new` alongside
  the existing `ai_tx`/`ai_rx` construction. `acp_terminal_tx` is `pub(super)` so Task 5's own
  `submit()` (same module) can clone it into each ACP prompt's `try_send_prompt` call.
- `GpuiShellRoot::handle_acp_terminal_requests(&mut self, cx: &mut Context<Self>)`, mirroring
  `App::handle_acp_terminal_requests` (`src/app/frame.rs`) exactly, called from `poll.rs`'s
  existing tick:
  - `Create { command, args, cwd, tx }`: splits the active pane (reusing `GpuiShellRoot::split_
    focused`'s own terminal-spawn machinery, `actions.rs` -- confirm the exact call shape at plan
    time so this doesn't duplicate `spawn_terminal`'s own logic, just drives it with an injected
    initial command the same way the wgpu build's own `open_terminal_for_acp` writes the shell-
    quoted command string to the new pane immediately after creating it), responds with the new
    `terminal_id` as the `pane_id`.
  - `GetOutput { pane_id, tx }`: `tx.send((self.terminal_output_text(pane_id), self.terminal_exit_
    code(pane_id)))` (Task 2's own two accessors).
  - `WaitForExit { pane_id, tx }`: if `self.terminal_exit_code(pane_id)` is already `Some`, respond
    immediately; else push onto a new `pending_acp_wait_for_exit: Vec<(usize, oneshot::Sender<
    i32>)>` field (mirrors `UiManager`'s own field), resolved on a later tick once the terminal's
    exit code becomes available (mirrors `handle_acp_terminal_requests`'s own tail loop exactly).
  - `Kill { pane_id }`: **real finding, verified while writing this spec, not the wgpu build's own
    mechanism** -- `Mux::kill_terminal` calls `term.pty.shutdown()` (`&mut self`), unreachable in
    `gpui_shell` since every `Terminal` is held as `Rc<Terminal>` (confirmed: `reap_pane`'s own
    comment already documents that `gpui_shell` relies on `Drop for Pty` running the full
    shutdown sequence when a terminal's last `Rc` is dropped, never a direct `.shutdown()` call).
    `Pty::child_pid: u32` is already a public field needing no mutable access -- send the signal
    directly instead: `if let Some(terminal) = self.terminals.get(&pane_id) { unsafe { libc::kill
    (terminal.pty.child_pid as libc::pid_t, libc::SIGHUP); } }` (`libc` is already an
    unconditional workspace dependency). This only signals the process; the pane's own removal
    from `self.terminals`/the pane tree still happens through the normal `PtyEvent::Exit` ->
    `on_terminal_exited` path once the reader thread observes EIO, exactly mirroring how a natural
    shell exit is already handled -- no new pane-removal code needed here.

## 7. Task 5 -- ACP prompt submission + tool-status streaming

**Depends on Task 3** (needs `acp_session` to exist as a field to branch on) **and Task 4** (needs
the real, already-constructed `acp_terminal_tx` to pass as `try_send_prompt`'s `terminal_tx`
argument -- see Task 4's own note on why this task cannot come first).

**What's reused:** `AcpSession::try_send_prompt` (unmodified), the existing `AiEvent::ToolStatus`
variant + `ChatPanel::set_tool_status` (already defined/callable, simply never constructed by
`gpui_shell`'s own code today since only the direct-provider path runs).

**New code:**
- `chat_panel/stream.rs`'s `submit()` gains an ACP branch, checked first (mirrors the wgpu
  build's own `submit_ai_query`'s `if self.acp_session.is_some() { ... } else { ... existing
  provider code ... }` shape exactly): builds a fresh `mpsc::channel` for `ai_tx` (one per prompt,
  matching the wgpu build's own per-call channel for AI events), clones the already-existing
  `self.acp_terminal_tx` (Task 4) as `terminal_tx`, calls `self.acp_session.as_mut().unwrap().
  try_send_prompt(content, ai_tx, terminal_tx)`, and on error falls back to `self.acp_session =
  None;` plus an error message (matching the wgpu build's own `try_send_prompt` failure handling,
  `src/app/ui/mod.rs:804-820`).
- `stream.rs`'s existing `AiEvent` drain match (already handles `Token`/`Done`/`Error`/`Usage`)
  gains a real arm for `ToolStatus` (calls `panel.set_tool_status`, already exists) and stub arms
  for `ConfirmWrite`/`ConfirmRun`/`UndoState` that do nothing yet beyond compiling (`_ => {}` is
  not available here since `AiEvent`'s real current match exhaustiveness must be checked at plan
  time -- if the match already has a catch-all arm, no stub is needed at all; if it's fully
  exhaustive today with no unreachable-variant arm, these three need real match arms that simply
  do nothing until Task 6 completes them). Confirm which shape the real current code has before
  writing this step.

## 8. Task 6 -- ACP write/run confirm + undo

**Depends on Task 5** (needs the ACP event drain in place to route these events at all) and reuses
Task 1's confirm-card UI shape (a second, structurally similar card for `AwaitingConfirm`, not a
copy-paste duplicate -- factor the shared "bordered card + keyboard hint row" chrome out of Task
1's own card into a small helper both call, parameterized by title/body/hint-row content).

**What's reused:** `ConfirmDisplay::{Write, Run}` + `ConfirmDisplay::for_write` (unmodified,
already does diff computation), `crate::llm::diff::{DiffLine, DiffKind}` (unmodified), `ChatPanel::
{mark_awaiting_confirm, resolve_confirm}` (pre-existing `pub`).

**New code:**
- `GpuiShellRoot` gains `pending_confirm_tx: Option<tokio::sync::oneshot::Sender<bool>>` and
  `undo_stack: std::collections::VecDeque<(PathBuf, String)>` (both mirror `UiManager`'s own
  fields exactly, including the `MAX_UNDO = 10` cap on the deque).
- `stream.rs`'s `ConfirmWrite`/`ConfirmRun` arms (Task 5's stub, completed here): `panel.mark_
  awaiting_confirm(display)` + store `result_tx` in `pending_confirm_tx`. `UndoState` arm: push
  onto `undo_stack`, evicting the oldest past `MAX_UNDO`.
- `GpuiShellRoot::maybe_handle_awaiting_confirm_key`, same shape as Task 1's `maybe_handle_
  confirm_action_key`: keyed on `matches!(panel.state, PanelState::AwaitingConfirm)`. `y`/Enter:
  if `confirm_display` is `ConfirmDisplay::Run { cmd }`, stash `cmd` in a new `pending_pty_run:
  Option<String>` field (mirrors the wgpu build's own field+drain shape exactly) for `render()`'s
  top to write to the active terminal (a `Window`-free PTY write needs no deferral in principle,
  but matching the established pattern keeps this consistent with every other `pending_*` field
  here); send `true` on `pending_confirm_tx`; call `panel.resolve_confirm()`. `n`/Escape: send
  `false`; `resolve_confirm()`.
- The diff-rendering card in `chat_panel/render.rs`: reuses Task 1's shared card chrome, body
  built from `ConfirmDisplay::Write`'s `diff: Vec<DiffLine>` (one row per `DiffLine`, colored by
  `DiffKind::{Added,Removed,Context}` -- mirrors `render_line`'s own per-span coloring shape,
  `chat_panel/markdown.rs`) or `ConfirmDisplay::Run`'s bare `cmd` string.
- `Leader a z` -- currently unwired (`LeaderAction` has no variant for it; `input.rs`'s own `'a'`-
  prefix continuation match's `_ => {}` arm silently drops it, per M5b's own explicit comment).
  Add `LeaderAction::UndoLastWrite`, wire the `'a'`+`'z'` continuation, and a `GpuiShellRoot::
  undo_last_write` method mirroring `UiManager::cmd_undo_last_write` (pop the newest `undo_stack`
  entry, write its saved content back to disk, push a confirmation message onto `panel.messages`).

## 9. Deferred (recorded, not reopened for reconsideration here)

- **MCP tool injection into the ACP system prompt** -- the wgpu build's `McpManager` is not wired
  into `gpui_shell` at all (a pre-existing, larger gap this milestone doesn't touch); ACP agents
  still work without it (they have their own MCP config independent of PetruTerm's), just without
  PetruTerm surfacing MCP tool results into the direct-provider path the way tool-calling for
  Provider mode would need `execute_tool`/`AgentTool` (`src/llm/tools.rs`) -- also not ported here,
  since `gpui_shell`'s own direct-provider backend has no tool-calling loop of its own (only ACP
  agents do real tool-calling; the "inline action" mechanism, Task 1, is a much narrower text-
  parsing emulation, not real tool-calling).
- **Skill/steering-file injection into the ACP system prompt** -- `SkillManager`/`SteeringManager`
  are already loaded in `gpui_shell` (M3d) but the wgpu build's own `rewire_backend`'s Agent branch
  re-loads them freshly on every backend switch; this milestone reuses whatever's already loaded
  at startup rather than adding a reload-on-switch path, a real, minor behavior gap recorded here
  rather than silently matched.
- **Continuous ACP terminal-pane visual distinction** (the wgpu build doesn't have one either --
  an agent-created pane looks like any other split) -- nothing to defer, confirmed no gap exists.

## 10. Manual testing required (cannot be verified from the agent sandbox)

**Inline-action confirm (Task 1):**
- A direct-provider response containing an `<action>` tag shows a real confirm card (not just
  header-status text) with the action's own description; `y`/Enter runs it, `a` runs it and skips
  future confirms this session, `n`/Escape cancels.
- `RunCommand` writes the command to the active terminal; `OpenFile` opens the resolved path;
  `ExplainOutput` submits a fresh query using the last N terminal lines.

**Terminal-bridge foundation (Task 2):**
- No direct user-visible behavior yet (exercised end-to-end only via Task 4's own terminal bridge)
  -- confirm via code review that a closed pane's exit code/final output are captured before the
  pane's own state is discarded, not a dogfood step.

**ACP session + backend switching (Task 3):**
- Setting `llm.backend = "agent"` + a valid `llm.agent.command` in config, then hot-reloading (or
  restarting), connects to the agent process; the header shows the ◈ agent indicator instead of
  the ✦ provider one. `/agent` reports the active agent name; `/agent <cmd>` switches agents and
  reconnects. `/model` reports "use /agent" in agent mode.

**ACP terminal bridge (Task 4):**
- An agent's `terminal/create` tool call opens a real, visible split pane running the requested
  command. `terminal/output`/`terminal/wait_for_exit` correctly return output/exit status both
  while the pane is still open and after the user closes it. `terminal/kill` closes the pane.

**ACP prompt + tool-status (Task 5):**
- A prompt sent to a real ACP agent streams tokens into the panel the same way the direct-provider
  path does; tool-call status lines (⟳/✓ prefix) appear and update as the agent works.

**ACP write/run confirm + undo (Task 6):**
- An agent-requested file write shows a real diff card (added/removed line counts, colored diff
  lines) before anything touches disk; `y` applies it, `n` rejects it. An agent-requested command
  run shows the command in a confirm card before it reaches the PTY. `Leader a z` restores the
  most recently agent-written file's prior content.
