# M3b — AI Chat Panel Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development or superpowers:executing-plans. Steps use checkbox (`- [ ]`) syntax.

**Goal:** A working AI chat panel in `gpui-petruterm`: an animated drawer holding a real streaming conversation with markdown rendering, an IME-capable composer, and slash commands.

**Architecture:** `ChatPanel` (`src/llm/chat_panel/`, 919 lines, engine-agnostic, 12 passing tests) is **ported as-is** — it is plain Rust over `String`/`Vec` with zero winit/wgpu references. What gets rewritten is `src/app/renderer/chat.rs` (1567 lines of `RoundedRectInstance` pixel math) as a gpui `div()` tree, the same "port the logic, rewrite the painting" split `status_bar` and `tabs` already went through. The composer is the `TextInput` entity M3a built. Streaming reuses the existing `crossbeam_channel` shape, owned by `GpuiShellRoot` and drained by the poll loop — no static-slot bridge needed, since the task is spawned from a method on the root rather than predating it.

**Tech Stack:** Rust 2021, gpui 0.2.2, tokio, crossbeam-channel.

**Spec:** `docs/superpowers/specs/2026-09-06-gpui-m3-sidebars-design.md` (§2 M3b, §3.2 markdown, §3.3 layout, §3.4 drawers, §3.5 streaming)

## Scope

**In:** panel drawer + animation, toggle via `Leader a a`, header, message list with markdown, composer, streaming from the configured LLM provider, slash commands (`/q`, `/clear`, `/skills`, `/mcp`, `/model`, `/agent`), inline AI block (`Ctrl+Space`).

**Deliberately deferred** — recorded here so nobody discovers it mid-task. These are real features of the wgpu build that M3b does not port, because each carries its own surface and the milestone is already the largest in the migration:
- **The ACP agent backend and tool-calling.** M3b ports the *direct provider* streaming path only (`LlmProvider::stream`). This is what makes the confirm-prompt surfaces (write/run confirmation cards, `resolve_action_yes`/`no`, `UndoState`) out of scope too — with no tool calls there is nothing to confirm. `Leader a z` (undo last write) therefore stays unwired.
- **File attachment picker** (`Tab` in the composer, `filtered_picker_items`).
- **Suggestion pills and the zero-state's hover treatment** — the panel renders a plain empty state instead.
- `Leader a e` / `Leader a f` (explain last output / fix last error) — they need shell-context plumbing; `Leader a a` is the milestone's gate.

## Global Constraints

- Module files stay under 400 lines (`AGENTS.md`); split into a directory module when exceeded, following `status_bar/` and `text_input/`.
- Tests cover **logic only**. No tests for painting, layout, or hit-testing — GPU windows cannot be captured from the agent sandbox; those are dogfooded. The ported `ChatPanel` tests must keep passing untouched.
- `bash scripts/ci-local.sh` exits 0; `RUSTFLAGS="-D warnings" cargo build --bin gpui-petruterm --bin petruterm` clean (**both** binaries); `cargo fmt --check` clean.
- Commit format: `type: Message.` — feat/fix/chore/refactor.
- Do not modify `src/app/`, `src/ui/`, or `src/renderer/` — the shipping wgpu binary must not regress. `src/llm/` may be read and re-exported but not restructured.
- Baseline: **217 lib tests**, branch `worktree-gpui-migration`.

## File Structure

| File | Responsibility |
|---|---|
| `src/gpui_shell/chat_panel/mod.rs` (new) | Panel state owned by the shell: visibility, focus, the `TextInput` composer entity, toggle/open/close |
| `src/gpui_shell/chat_panel/render.rs` (new) | The `div()` tree: drawer frame, header, message list, composer row |
| `src/gpui_shell/chat_panel/markdown.rs` (new) | `AnnotatedLine`/`SpanKind` → gpui styled text |
| `src/gpui_shell/chat_panel/stream.rs` (new) | Channel ownership, tokio spawn, `AiEvent` drain |
| `src/gpui_shell/ai_block.rs` (new) | Inline `Ctrl+Space` surface (separate state machine from `ChatPanel`) |
| `src/gpui_shell/mod.rs`, `render.rs`, `actions.rs`, `input.rs`, `poll.rs`, `leader.rs` (modify) | Field, drawer as flex sibling, leader actions, key routing, drain tick |

---

### Task 1: Panel shell — drawer, layout, markdown

**Files:** create `chat_panel/{mod,render,markdown}.rs`; modify `gpui_shell/{mod,render,actions,leader,input}.rs`

**Interfaces produced (Tasks 2 and 3 consume these):**
- `ChatPanelView { panel: crate::llm::chat_panel::ChatPanel, composer: Entity<TextInput>, visible: bool }` on `GpuiShellRoot` as `chat: ChatPanelView`
- `ChatPanelView::toggle(&mut self, window, cx)`, `::is_visible(&self) -> bool`
- `render_chat_panel(view: &ChatPanelView, colors: &ColorScheme) -> Div`
- `markdown::render_line(line: &AnnotatedLine, colors: &ColorScheme) -> Div`

- [ ] **Step 1: Wire `ChatPanel` state onto the shell**

`crate::llm::chat_panel::ChatPanel` already exists and is engine-agnostic — **use it directly, do not copy it**. Add `ChatPanelView` holding it plus a `TextInput` composer entity (`TextInput::new(cx, colors, "", "Ask anything…")`) and a `visible: bool`.

Per the M3 design's §1, **there is one global panel, not one per pane.** The wgpu build's `panel_id`/`set_active_terminal` plumbing is dead — `set_active_terminal` is an empty function and `active_panel_id()` returns a hardcoded `0`. Do not reproduce it.

- [ ] **Step 2: `Leader a a` toggles the panel**

`leader.rs`'s `LeaderAction` has no AI variants — its own doc comment names this milestone as the one that adds them. Add `ToggleAiPanel`, parse `"ToggleAiPanel"` in the `TryFrom<&str>` impl, and dispatch it in `actions.rs` to `self.chat.toggle(window, cx)`. `Leader a` is a **sub-prefix** (two keys: `a` then `a`), unlike the single-key chords the leader map handles today — read `src/app/input/mod.rs:312-371` for the wgpu build's sub-prefix shape and mirror it in `input.rs`'s leader block.

Toggling open focuses the composer; toggling closed returns focus to the terminal. **Reuse M3a's pattern exactly**: `render.rs`'s focus guard must now also skip when the composer holds focus, and `input.rs`'s key guard must check the composer's `is_focused(window)` — not merely "is the panel visible". M3a shipped that bug (a state-keyed guard froze the whole app when focus moved); do not repeat it. See `input.rs`'s existing `tab_rename` guard comment for the reasoning.

- [ ] **Step 3: Drawer layout — a flex sibling, never a manual viewport**

Per §3.3: **do not port `resize_terminals_for_panel`.** Add the panel as a flex sibling of the pane tree in `render.rs`'s root `div()`, with a fixed width (start with `w(px(480.))`) when visible and nothing when hidden. `pane_view.rs`'s `fit_terminal`/`on_children_prepainted` already resizes each PTY to whatever box taffy gives it, so terminal reflow on toggle is automatic. Verify that by dogfood: opening the panel should reflow the shell's columns with no extra code.

Animate the open/close per §3.4 (the wgpu build toggles instantly; "real gpui styling/animation" is what the parent spec asks M3 for). Use gpui's own animation facility — check `~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/gpui-0.2.2/` for what 0.2.2 actually offers (`with_animation`, `Animation::new`) and cite what you used in your report. If 0.2.2 has no workable animation API, say so and ship it unanimated rather than hand-rolling a tween in the poll loop.

- [ ] **Step 4: Render the panel**

`src/app/renderer/chat.rs` is the reference for *what* to draw, not *how* — every line of its pixel math is replaced by real elements. Structure:
- **Header:** provider/model name (`ChatPanel` exposes what it needs; see `build_panel_header` at `chat.rs:437-539` for the content), and a close affordance.
- **Message list:** one block per `ChatMessage`, scrollable, newest at the bottom.
- **Composer row:** the `TextInput` entity, plus a hint line.

Keep each file under 400 lines; split `render.rs` further if the three sections push past it.

- [ ] **Step 5: Markdown**

Per §3.2, **messages get native gpui text layout, the composer stays fixed-width monospace.**

Call `crate::llm::markdown::parse_markdown(content, width, &mut state)` with a very large `width` so its character-count wrapping does not fire, and use the returned `AnnotatedLine { display, kind, spans }` purely for *styling* — let gpui wrap. `BlockKind` is `Normal | Heading(u8) | CodeBlock | ListItem{..}`; `SpanKind` is `Bold | Italic | Code | Syntax(TokenKind)` with spans as `(start, end, kind)` byte ranges into `display`.

Map `BlockKind` to block styling (headings larger/bolder, code blocks on a `ui_surface` background in the terminal font) and `SpanKind` to inline runs. gpui's `StyledText::with_highlights` (or building `TextRun`s directly, as `text_input/element.rs` already does) is the mechanism — pick one and say which in your report.

- [ ] **Step 6: Gate**

```bash
cargo build --bin gpui-petruterm --bin petruterm 2>&1 | tail -20
bash scripts/ci-local.sh 2>&1 | tail -30
RUSTFLAGS="-D warnings" cargo build --bin gpui-petruterm --bin petruterm 2>&1 | tail -20
cargo fmt --check
wc -l src/gpui_shell/chat_panel/*.rs
```
Expected **217 lib tests** (this task adds none — its surface is layout, which is dogfooded), all files under 400 lines.

- [ ] **Step 7: Commit** as `feat: Add the gpui chat panel drawer, layout and markdown rendering.`

---

### Task 2: Streaming and slash commands

**Files:** create `chat_panel/stream.rs`; modify `chat_panel/mod.rs`, `gpui_shell/{mod,poll,input}.rs`

**Interfaces consumed:** Task 1's `ChatPanelView`. **Produced:** `ChatPanelView::submit(&mut self, cwd, tokio_rt, cx)`, `ChatPanelView::drain_events(&mut self) -> bool` (returns whether anything changed, for the poll tick).

- [ ] **Step 1: Own the channel on the shell**

Per §3.5: keep the existing `crossbeam_channel::bounded(256)` shape, store the pair on `ChatPanelView`, and **drop the `(panel_id, event)` tuple** — send bare `AiEvent`, since there is one panel. No static-slot bridge: `config_watch.rs`'s `Mutex`+`AtomicBool` pattern exists only because the config watcher predates any entity; this task is spawned from a method on the root, so it can hold a channel end directly.

`GpuiShellRoot` already owns `tokio_rt: tokio::runtime::Runtime` (added in M2 for the status bar's git-branch fetch) — spawn onto that, do not build a second runtime.

- [ ] **Step 2: Port the submit path — direct provider only**

Read `src/app/ui/mod.rs`'s `submit_ai_query` (from line 794). Port **only** the direct-provider branch: `ChatPanel::submit_input()` returns the user's text, then spawn a task that calls the configured `LlmProvider::stream` and sends `AiEvent::Token`/`Done`/`Error`/`Usage` down the channel.

Skip the ACP branch entirely (`self.acp_session.is_some()`, the `tokio::select!` relay, `AcpTerminalRequest`) and skip tool-calling — both are explicitly deferred, see this plan's Scope. Drop the `wakeup_proxy.send_event(())` calls: the winit wake has no gpui equivalent and none is needed, because the poll tick is the wake.

Abort any in-flight request when a new one is submitted, the way `streaming_handle.abort()` does today.

- [ ] **Step 3: Drain in the poll loop**

`poll.rs`'s ~33ms tick already has five responsibilities (config reload, PTY events, cursor blink, leader deadline, status-bar refresh). Add a sixth: drain the AI channel with a bounded loop and feed each event to `ChatPanel`'s existing handlers — `append_token`, `mark_done`, `mark_error`. Cap the drain per tick (the wgpu build uses `AI_POLL_CAP = 64`) so a fast stream cannot starve the rest of the tick, and `cx.notify()` only when something actually changed.

- [ ] **Step 4: Composer submit and slash commands**

`Enter` in the composer submits. The `TextInput` already emits `TextInputEvent::Submit` — subscribe to it the way `begin_tab_rename` does, read `content()`, clear the composer with `set_content("", cx)` (this is the consumer M3a's `set_content` was kept for), and route:
- Input starting with `/` → `handle_slash_command`. Port from `src/app/ui/providers.rs:114-290` — it is straight string dispatch plus `messages.push`, fully portable; its only non-agnostic parameter is an `EventLoopProxy` used for ACP reconnects, which is dead here since ACP is out of scope. Support `/q`, `/clear`, `/skills`, `/mcp`, `/model`, `/agent`.
- Anything else → `submit`.

- [ ] **Step 5: Gate** — same four commands as Task 1. Expected **217 lib tests**.

- [ ] **Step 6: Commit** as `feat: Stream LLM responses into the gpui chat panel.`

---

### Task 3: Inline AI block (`Ctrl+Space`)

**Files:** create `gpui_shell/ai_block.rs`; modify `gpui_shell/{mod,render,input}.rs`

- [ ] **Step 1: Port the state machine**

`src/llm/ai_block.rs` (118 lines, `AiBlock`/`AiState`: `Hidden → Typing → Loading → Streaming → Done/Error`) is engine-agnostic — use it directly. It is a genuinely separate surface from `ChatPanel`, with its own state, its own draw path, and its own simpler async submit (`submit_ai_block_query`, a plain `Sender<AiEvent>` rather than the panel's channel). Do not merge the two.

- [ ] **Step 2: Render and route**

The wgpu build draws it into the bottom `AI_BLOCK_ROWS` (=4) rows of the terminal's own cell grid (`chat.rs:1430-1567`). In gpui it is naturally a `div()` overlay anchored to the bottom of the focused pane's area. It needs a one-line composer — reuse the `TextInput` entity, a second instance.

`Ctrl+Space` toggles it, independent of leader state (`src/app/input/mod.rs:450-460`). While visible it takes keyboard focus; the same focus-keyed guard rule as Task 1 Step 2 applies — guard on the block composer's `is_focused(window)`, never on "is the block visible".

- [ ] **Step 3: Stream into it** — reuse Task 2's channel-plus-poll-drain shape with its own channel pair.

- [ ] **Step 4: Gate** — same four commands. Expected **217 lib tests**.

- [ ] **Step 5: Commit** as `feat: Add the inline Ctrl+Space AI block to the gpui shell.`

---

## Dogfood (after all three tasks)

1. `Leader a a` opens the panel as a drawer; the terminal reflows to the remaining width; `Leader a a` again closes it and the terminal reflows back.
2. Type a question, `Enter` → a response streams in progressively, not all at once at the end.
3. Markdown renders: headings, **bold**, `inline code`, and fenced code blocks are visually distinct.
4. IME works in the composer (compose in a non-ASCII input method; composition shows underlined).
5. `/clear` empties the conversation; `/q` closes the panel; `/skills` and `/mcp` list something sensible.
6. **Focus:** with the panel open, click the terminal → typing goes to the shell. Click back into the composer → typing goes to the composer. Neither ever freezes. (This is M3a's Critical, in a surface where it matters more — the composer lives for minutes.)
7. `Ctrl+Space` opens the inline block, streams a response, and closes.
8. Config hot-reload with the panel open: the panel recolors with the theme (see TD-GPUI-05 — if the composer's colors go stale, that is the known item, report it rather than treating it as new).

## Self-Review

**Spec coverage.** §3.2 markdown split → Task 1 Step 5. §3.3 flex-sibling layout, no `resize_terminals_for_panel` → Task 1 Step 3. §3.4 animated drawer → Task 1 Step 3. §3.5 channel-on-the-root, no static bridge, drop `panel_id` → Task 2 Steps 1-3. §1's "per-pane chat is fiction" → Task 1 Step 1. The design's M3b line also names the inline block → Task 3. Deferred items are listed in Scope rather than left implicit.

**Placeholders.** None: each step names the file to read, the function to port, and the decision to make. Two steps deliberately ask the implementer to check gpui 0.2.2's actual API (animation, styled text) and report what they used, rather than the plan asserting an API it has not verified — this plan does not repeat M3a's mistake of prescribing a signature that turned out not to exist.

**Type consistency.** `ChatPanelView` is defined in Task 1 and consumed by name in Tasks 2 and 3. `AiEvent` is the existing `crate::llm::chat_panel::AiEvent` (variants `Token`/`Done`/`Error`/`Usage`/`ToolStatus`/`ConfirmWrite`/`ConfirmRun`/`UndoState`); Task 2 handles only the first four, consistent with tool-calling being out of scope. `set_content` is M3a's, consumed in Task 2 Step 4 — the use it was retained for.
