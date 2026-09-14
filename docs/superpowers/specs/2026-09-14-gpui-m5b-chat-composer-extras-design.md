# M5b — Chat Composer Extras Design

## 1. Overview

M5b is the second sub-milestone of M5 (after M5c, already complete). It closes M3b's own
deliberately-deferred scope: the composer's file attachment picker, the suggestion pills (zero-
state's two + post-response's two), and `Leader a e`/`Leader a f` (explain last output / fix last
error). All three build on `ChatPanel` state M3b already ported as-is but `gpui_shell` never reads
(`file_picker_*`, `attached_files`, `zero_state_hover`, `show_suggestions`, `suggestion_hover`) --
same "reuse the state, build the gpui rendering" shape as every prior milestone.

Two real findings from investigating the wgpu build, changing this from "three separate ports"
into "one small backend plus two thin UI layers on top of it":

1. **`Leader a e`/`Leader a f` need far less new plumbing than M3b's plan text assumed.**
   `ShellContext::load()` is *already* called in `gpui_shell` (`ai_block.rs:183`) -- no port
   needed. The grid-reading pattern `explain_last_output`/`fix_last_error` need (`Mux::
   last_terminal_lines(n)`, `src/app/mux/mod.rs:611-627`: read the last N visible rows) is the
   exact `terminal.with_term(|t| { ... t.grid()[Line][Column] ... })` shape `blocks.rs` (M5c)
   already established. This milestone's Task 1 is genuinely small: two query-building functions
   plus wiring, not new infrastructure.
2. **Suggestion pills need no hover-state tracking.** `ChatPanel::zero_state_hover`/
   `suggestion_hover` exist because the wgpu renderer has no real hover primitive and must
   manually track "which pill is the mouse over" across frames to recolor it. gpui's own
   `.hover(|el| el.bg(...))` builder (already used once, M4c's context-menu rows,
   `context_menu.rs:130`) does this per-frame with zero state -- those two `ChatPanel` fields stay
   permanently unread by `gpui_shell`, same as every other wgpu-only field this migration has left
   alone (e.g. M4c's `hovered`/`col`/`row` on the wgpu-only `ContextMenu`).

## 2. Global Constraints

Carried forward unchanged from every milestone this session:

- 400-line module limit -- current state, verified fresh while writing this spec: `input.rs` 392,
  `render.rs` 384, `render_callbacks.rs` 206, `leader_dispatch.rs` 148, `chat_panel/mod.rs` 206,
  `chat_panel/render.rs` 242, `chat_panel/stream.rs` 263. All have headroom, but watch cumulative
  growth per task the same way every prior milestone has needed to.
- `scripts/ci-local.sh` must stay green after every task (clippy `-D warnings`, `fmt --check`,
  full `cargo test --lib`, `cargo audit`); run `cargo fmt` proactively before every commit.
- Tests cover **logic only** -- no painting/hover/hit-testing tests; every UI-facing item is a
  dogfood step. The pure-logic pieces (query-string building, fuzzy-filter reuse) get real tests
  where new logic is added; reused `ChatPanel`/`picker.rs` logic already has its own coverage and
  is not re-tested here.
- Commit format: `type: Message.` per `AGENTS.md`.
- `#[allow(dead_code)]` (narrowly scoped, comment naming the removing task) for anything built
  ahead of its first caller within this spec's own task sequence.
- **Key/focus guard exception, matching `InfoOverlay`'s own precedent (M3d):** the file picker's
  own key guard (§4) is keyed on `self.chat.panel.file_picker_open` (a mode flag), **not**
  `is_focused(window)`. This is not a new exception to the "key off real focus" rule -- it's the
  same one `InfoOverlay` already established: the file picker grabs no `FocusHandle` of its own
  (the composer's real `TextInput` keeps gpui focus throughout; the picker is a alternate key-
  interpretation mode layered on top of that same focus, exactly matching the wgpu build's own
  `file_picker_focused: bool` sub-mode, not a real widget-focus concept either). A visibility-
  keyed guard is provably correct here for the identical reason it's correct for `InfoOverlay`.

## 3. Task 1 -- `Leader a e` / `Leader a f` + palette entries

**What's reused:** `ShellContext::load()` (already called in `gpui_shell`, `ai_block.rs:183`),
`ChatPanel::set_input`/`submit_input` (already the exact mechanism `handle_chat_composer_submit`
uses), `ChatPanelView::submit`/`toggle` (both already exist, M3b).

**New code:**
- A small grid-reading helper mirroring `Mux::last_terminal_lines(n: usize) -> String`
  (`src/app/mux/mod.rs:611-627`), adapted to take `&Terminal` directly (same adaptation style
  M5c's `blocks.rs::row_text_and_absolute_row` already used for a sibling grid read): read the
  bottom `n` visible rows via `terminal.with_term(|t| { ... t.grid()[Line][Column] ... })`,
  joined with `\n`, trimmed.
- Two new `GpuiShellRoot` methods, `explain_last_output(&mut self, window: &mut Window, cx: &mut
  Context<Self>)` and `fix_last_error(&mut self, window: &mut Window, cx: &mut Context<Self>)` --
  both take `Window` directly (unlike M5c's `SendToChat`, both of this task's real call sites --
  `on_key_down`'s leader-prefix continuation and a pill's `on_mouse_down` handler, Task 2 -- already
  have `Window` in hand, so no deferred `pending_*`/render-drain pattern is needed here):
  - `explain_last_output`: build `format!("Explain this terminal output:\n```\n{output}\n```")`
    from the last-30-lines helper above (matching the wgpu build's own `mux.last_terminal_lines
    (30)`); if the output is empty, return without doing anything (matching the wgpu build's own
    early-return).
  - `fix_last_error`: same last-30-lines read, plus `ShellContext::load()` -- if it has a non-empty
    `last_command`, build `format!("The command `{cmd}` failed (exit code {code}). Output:\n```\n
    {output}\n```\nHow do I fix this?")`; otherwise the generic `format!("This command failed.
    Output:\n```\n{output}\n```\nHow do I fix this?")` (both matching the wgpu build's own two
    branches verbatim, `src/app/ui/mod.rs:1240-1263`).
  - Both then: `if !self.chat.is_visible() { self.chat.toggle(window, cx); }`, `self.chat.panel.
    set_input(query);`, clear the composer's own visible `TextInput` (`self.chat.composer.update
    (cx, |input, cx| input.set_content("", cx));` -- keeps the real widget in sync even though
    `submit()` reads from `panel.input`, not the composer; a real discrepancy the wgpu build
    doesn't have to consider, since it has no separate widget), then `self.chat.submit(&self.
    tokio_rt, cx);`.
- `LeaderAction` gains two variants, `ExplainLastOutput`/`FixLastError` (plus their `TryFrom<&str>`
  string-parse entries, matching every existing variant's own round-trippable style -- not load-
  bearing for this task's own wiring, since the `'a'`-prefix continuation dispatches them directly
  by constructing the variant, the same way the existing `'a'`+`"a"` case already does; included
  for consistency, not because anything requires it).
- `input.rs`'s existing `if let Some(prefix) = self.leader_prefix.take() { if prefix == 'a' &&
  event.keystroke.key == "a" { ... } ... }` block gains two more arms:
  `if prefix == 'a' && event.keystroke.key == "e" { self.dispatch_leader_action(LeaderAction::
  ExplainLastOutput, window, cx); }` and the `"f"` sibling for `FixLastError` -- removing them from
  the comment's own "out of scope" list (`c`/`z` -- `ClearAiContext`/`UndoLastWrite` -- stay there,
  still correctly ACP/tool-calling-dependent and out of scope for M5b).
- `leader_dispatch.rs`'s `dispatch_leader_action` match gains two arms, each calling this task's
  new `self.explain_last_output(window, cx)` / `self.fix_last_error(window, cx)`.
- Command palette: `gpui_shell_actions` (`palette_dispatch.rs`) currently filters `Action::
  ExplainLastOutput`/`Action::FixLastError` out entirely (M3b's own deferred-AI-actions list) --
  this task removes that filter and adds two real `dispatch_palette_action` arms calling the same
  two new `GpuiShellRoot` methods (the palette's own `dispatch_palette_action` already receives
  `window: &mut Window`, confirmed against its existing signature -- no deferred pattern needed
  here either).

## 4. Task 2 -- Suggestion pills (zero-state + post-response)

**Depends on Task 1** (both pill kinds dispatch into `explain_last_output`/`fix_last_error`).

**What's reused:** `ChatPanel::messages`/`is_idle()` (zero-state condition: no messages yet),
`ChatPanel::show_suggestions` (post-response condition: set `true` after an assistant reply
completes, already maintained by the ported `ChatPanel` logic itself -- confirmed via `mod.rs:588,
636-642`, unmodified by this task).

**New code:**
- `chat_panel/render.rs`'s `render_message_list` gains a zero-state branch: when `panel.messages.
  is_empty()`, replace the current plain `"Ask a question to get started."` text with a centered
  `✦` icon row, a "Ask a question below" row, and two pill rows ("Fix last error", "Explain
  command") -- each a `div()` with `.cursor_pointer()`, `.hover(|el| el.bg(to_rgba(colors.
  ui_surface_active)).border_color(to_rgba(colors.ui_accent)))`, a rounded border/background
  matching `ContextMenu`'s own row-styling conventions, and an `.on_mouse_down(MouseButton::Left,
  move |_, window, cx| on_fix(window, cx))` / `on_explain(...)` callback (Task 2's own two new
  parameters, threaded through the same way `render_context_menu`'s `on_action`/`on_close_outside`
  already are).
- `render_message_list` gains a second branch: when `panel.show_suggestions` is true (checked
  after the settled-messages loop, before/after the streaming-buffer child depending on which
  reads more naturally against the real current code), render the same two-pill row (label text
  differs slightly per the wgpu build: "Fix last error" / "Explain more") with the same `.hover()`
  + click-dispatch shape, reusing a small shared `render_pill(label, on_click, colors) ->
  impl IntoElement` helper factored out of the zero-state's own two pills so the two call sites
  don't duplicate the styling block.
- `render_chat_panel`'s own signature gains two new `Rc<dyn Fn(&mut Window, &mut App)>` parameters
  (`on_fix_last_error`, `on_explain_last_output`) -- threaded straight through to both pill
  locations, matching the exact "callback built where `cx` is in scope, passed down as a render
  parameter" pattern `render_tab_bar`'s `on_select`/`on_right_click` and `render_context_menu`'s
  `on_action`/`on_close_outside` already established.
- `render_callbacks.rs`'s `build_frame_callbacks` gains these two new callbacks in its own return
  tuple (same weak-handle pattern every other callback there already uses): `on_fix_last_error`
  calls `root.fix_last_error(window, cx)`, `on_explain_last_output` calls `root.
  explain_last_output(window, cx)`. `render.rs`'s own `render_chat_panel(...)` call site is updated
  to pass both through.

## 5. Task 3 -- File attachment picker

**Independent of Tasks 1-2.**

**What's reused:** `ChatPanel::{file_picker_open, file_picker_query, file_picker_items,
file_picker_cursor, attached_files, attached_file_chars}` (all already-ported fields, unread by
`gpui_shell` until now), `ChatPanel::{close_file_picker, picker_type_char, picker_backspace,
picker_move_up, picker_move_down, picker_confirm, filtered_picker_items, attach_file, detach_file,
init_default_files}` (all pre-existing pure methods, `src/llm/chat_panel/picker.rs`, zero winit/
wgpu coupling -- ready to call directly), `crate::llm::chat_panel::scan_files(dir, max_depth)`
(pure, already used by the wgpu build's own async-spawn pattern this task mirrors).

**New code:**
- `ChatPanelView` gains `file_scan_rx: Option<crossbeam_channel::Receiver<Vec<PathBuf>>>` --
  **not** on `GpuiShellRoot` (a real design correction made during this spec's own self-review):
  `ChatPanelView` already owns exactly this shape of async state for its streaming channel (`ai_tx`/
  `ai_rx`/`in_flight`), so the file-scan channel belongs there too, both for consistency and to
  keep `GpuiShellRoot` (already near the 400-line ceiling in `mod.rs`) from growing for state that
  is purely chat-panel-local. `M5c`'s `branch_scan_rx` living on `GpuiShellRoot` was the right call
  there (the branch picker has no owning sub-view to live on) but is not the precedent to follow
  here.
- `chat_panel/mod.rs` (or a new `chat_panel/file_picker.rs` if the task's own line-count pressure
  warrants the split -- decided at plan time against real current line counts) gains:
  - `open_file_picker_async(&mut self, cwd: PathBuf)` on `ChatPanelView`: mirrors `UiManager::
    open_file_picker_async` (`src/app/ui/mod.rs:650-665`) -- clears `file_picker_query`/`cursor`,
    sets `file_picker_open = true`, clears `file_picker_items`, spawns `std::thread::spawn(move ||
    { let mut items = scan_files(&cwd, 3); items.sort(); let _ = tx.send(items); })` against a
    fresh `crossbeam_channel::bounded(1)`, stores the receiver in the new `file_scan_rx` field.
  - `poll_file_scan(&mut self) -> bool` on `ChatPanelView`: mirrors `UiManager::poll_file_scan`
    (`src/app/ui/mod.rs:668+`) -- drains `file_scan_rx`, sets `self.panel.file_picker_items` on
    arrival, returns whether anything changed. Called from `poll.rs`'s existing 33ms tick as `this.
    chat.poll_file_scan()`, alongside M5c's `poll_branch_scan` call (same tick, same "poll an async
    scan channel" shape, now with one precedent on each of the two ownership models).
  - `maybe_handle_file_picker_key(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) -> bool`
    on `GpuiShellRoot`: the picker's own key guard (see §2's Global Constraints for why this is
    keyed on `self.chat.panel.file_picker_open`, not focus) -- `Escape`/`Tab` closes (`close_file_
    picker()`, matching the wgpu build's own dual-key close), `Enter` confirms (`picker_confirm
    (&cwd, &filtered)` using the active pane's cached cwd), `Up`/`Down` move the cursor (`picker_
    move_up`/`picker_move_down(filtered.len())`), `Backspace` (`picker_backspace`), `Space` and
    single-character keys (`picker_type_char`). Returns `true` (consumed) whenever `file_picker_
    open` is true, regardless of which specific key matched -- same "swallow everything while this
    mode is active" shape `maybe_handle_palette_key`/`maybe_handle_search_key` already use.
- `input.rs`'s `on_key_down` gains a new guard, checked early (alongside the palette/search-bar/
  info-overlay guards already at the top, all ahead of the chat-composer's own blanket-return):
  `if self.maybe_handle_file_picker_key(event, cx) { return; }`.
- `input.rs`'s existing chat-composer `Tab` handling: currently there is none (`TextInput` has no
  `Tab` binding of its own, confirmed against `text_input/mod.rs`'s key-context registration, and
  nothing in `gpui_shell` intercepts it today -- a real, confirmed gap: Tab is currently a silent
  no-op while the composer is focused). Add, inside the existing `if self.chat.composer_focused
  (window, cx) { ... }` block (which currently just `return`s unconditionally) a check BEFORE that
  return: `if event.keystroke.key == "tab" { let cwd = self.cached_cwd.clone().unwrap_or_default();
  self.chat.open_file_picker_async(cwd); cx.notify(); return; }`.
- `chat_panel/render.rs`'s `render_composer` gains: an attached-files chip row (rendered above the
  input row whenever `panel.attached_files` is non-empty -- one small pill per file showing its
  file name, `Path::file_name()`), and, when `panel.file_picker_open`, a scrollable list of
  `filtered_picker_items()` inserted above the composer's own input row inside the same flex
  column (no `.absolute()` overlay needed -- a normal, in-flow child, since the composer already
  sits at the bottom of a `flex_col` drawer and pushing the input row down while the picker is open
  is the simplest correct layout, matching the "layout shift is fine, this isn't a floating
  overlay" reasoning already used for M4b's non-modal search bar). Each row highlights the cursor
  position (`idx == panel.file_picker_cursor`) and shows a checkmark when already attached
  (`panel.attached_files.contains(path)`).
- `ChatPanelView::new` (or wherever the panel first becomes visible -- decided at plan time by
  reading the real current open path) gains a call to `init_default_files(&cwd)` the first time the
  panel opens with a known cwd, matching the wgpu build's own auto-attach-`AGENTS.md` behavior
  (`self.panel_mut().init_default_files(&cwd)`, `src/app/ui/mod.rs:767`).

## 6. Deferred (recorded, not reopened for reconsideration here)

- `Leader a c` (`ClearAiContext`) / `Leader a z` (`UndoLastWrite`) -- both ACP/tool-calling-
  dependent, M3b's own scope cut, still correctly out of scope until M5a lands.
- Continuous keyboard navigation inside the file picker beyond what §5 lists (e.g. `Tab`-cycling
  through picker items) -- the wgpu build itself has no such affordance either; Up/Down + Enter is
  the real, complete interaction model being ported.
- A floating/overlay-positioned file picker (vs. this spec's in-flow layout-shift choice) -- a
  legitimate future polish item if dogfood finds the layout shift jarring, not part of this spec's
  scope.

## 7. Manual testing required (cannot be verified from the agent sandbox)

**`Leader a e` / `Leader a f`:**
- `Leader a e` with terminal output present opens the chat panel (if closed) and submits an
  "Explain this terminal output" query using the last 30 visible lines.
- `Leader a f` after a failed command includes the real failed command + exit code (from
  `ShellContext`) when available, falls back to the generic phrasing otherwise.
- Both also reachable from the command palette (`Leader o` → "Explain Last Output" / "Fix Last
  Error").

**Suggestion pills:**
- Opening the chat panel with no messages yet shows the zero-state's two pills; hovering either
  visibly highlights it (border + background change) with no keyboard involvement.
- Clicking a zero-state pill runs the same query `Leader a e`/`Leader a f` would.
- After an assistant reply completes, the same two pills appear again just above the input row;
  clicking them re-runs the same queries.

**File attachment picker:**
- `Tab` in the composer (not the terminal) opens a file list; typing filters it fuzzily; Up/Down
  moves the highlight; Enter attaches/detaches the highlighted file (toggle); Escape or Tab again
  closes the picker without changing the composer's own typed text (if any).
- An attached file shows as a small chip above the input row; re-selecting it in the picker detaches
  it and the chip disappears.
- Opening the chat panel for the first time in a project with an `AGENTS.md` shows it already
  attached.
