# M5c — Palette & Context-Menu Feature Completion Design

## 1. Overview

M5c is the first sub-milestone of **M5 — ACP & Deferred Features**, a new top-level milestone in
the gpui chrome migration inserted after M4 (the spec's original "M5 — Cleanup & merge" is
renumbered **M6**, since this is new feature work, not cleanup). M5 covers the deferred-items
list reported to the user after M4 closed, split into three independent sub-milestones:

- **M5a — ACP agent backend + tool-calling** (not yet designed).
- **M5b — Chat composer extras** (file attachment picker, suggestion pills, `Leader a e/f`; not
  yet designed).
- **M5c — this spec.** Five independent, mutually-unrelated features that were filtered out of
  M4a's command palette and M4c's context menu for lack of a data source: snippets, saved
  workspaces, the git-branch picker, command-block tracking, and hover-link detection. Chosen to
  go first because each is small, mechanical, and reuses already-engine-agnostic logic — the same
  "port the logic, rebuild the rendering" shape M4a-d proved out repeatedly, with no dependency on
  M5a or M5b.

Explicitly **not** in scope for M5 at all: the full Lua `petruterm.notify()` bridge (M4d's own
finding — `gpui_shell` has no live Lua VM; any future notification need routes through the
already-portable, Lua-independent `crate::platform::notifications::send()` instead, no code
change required for that decision).

## 2. Global Constraints

Carried forward unchanged from every milestone this session:

- 400-line module limit (`AGENTS.md`) — watch cumulative growth per task, same discipline M4a-d
  established (extract a self-contained unit into a new file on overshoot, re-export to keep call
  sites unchanged).
- `scripts/ci-local.sh` must stay green after every task (clippy `-D warnings`, `fmt --check`,
  full `cargo test --lib`, `cargo audit`); run `cargo fmt` proactively before every commit.
- Tests cover **logic only** — no painting/hit-testing tests; every UI-facing item below is a
  dogfood step, not a unit test.
- Commit format: `type: Message.` per `AGENTS.md`.
- Key/focus guards key on real focus (`is_focused(window)`), never on visibility/open state —
  none of M5c's five features add a new focus-grabbing widget, so this constraint is inherited,
  not newly exercised.
- `#[allow(dead_code)]` (narrowly scoped, comment naming the removing task) for anything built
  ahead of its first caller within a task sequence.
- Do not restructure `src/app/`, `src/ui/`, `src/term/`, or `src/renderer/` beyond what each task
  below explicitly calls out — the shipping wgpu binary must not regress. Where a task reuses
  logic from `src/app/`, it reads and calls that code's *sibling* in `src/term/`/`src/ui/`
  (already engine-agnostic) rather than the `src/app/`-level wrapper around it; where no
  engine-agnostic sibling exists yet, the task ports the minimum needed as new `gpui_shell`-local
  code rather than editing the wgpu-only source it was reading.

## 3. Task 1 — Command blocks

**What's reused:** `src/term/blocks.rs`'s `Block`/`BlockManager` (233 lines) is already
engine-agnostic — no winit/wgpu coupling, driven purely by `Osc133Marker` values. `Terminal`
(`src/term/mod.rs`) already carries a `pub block_manager: BlockManager` field, constructed
automatically — `gpui_shell`'s own `Terminal` instances already have one, just never fed.

**The real gap (verified, not assumed):** `BlockManager::on_marker(marker, absolute_row,
command_text)` is the only thing that populates blocks, and it is called from exactly one place
in the whole codebase — `Mux::apply_osc133_events` (`src/app/mux/mod.rs:455-479`), itself fed by
`PtyEvent::Osc133(marker)`/`PtyEvent::ScreenCleared` events that `gpui_shell`'s own poll loop
currently drains and **discards** (confirmed: `poll.rs`'s PTY-event loop only acts on
`PtyEvent::Exit`; this is `TD-GPUI-04` in `.context/quality/TECHNICAL_DEBT.md`, already tracked as
open debt). This task closes that specific slice of `TD-GPUI-04` as a side effect.

**New code:**
- `poll.rs`'s PTY-event drain gains two new match arms, mirroring `Mux::apply_osc133_events`
  exactly:
  - `PtyEvent::Osc133(marker)`: compute `absolute_row` via `terminal.with_term(|t| { let content =
    t.renderable_content(); let history = t.grid().history_size() as i64; let cursor_vp =
    content.cursor.point.line.0.max(0) as i64; let disp_off = content.display_offset as i64;
    history + cursor_vp - disp_off })` (verbatim formula from `mux/mod.rs`), `command_text` from
    `Osc133Marker::CommandStart(cmd) => cmd.clone()` (empty string for every other marker variant,
    same as the wgpu build), then `terminal.block_manager.on_marker(marker, absolute_row,
    command_text)`.
  - `PtyEvent::ScreenCleared`: `terminal.block_manager.clear()`.
- A new small helper mirroring `Mux::block_output_text(terminal_id, block_id) -> Option<String>`
  (`src/app/mux/mod.rs:583-604`) — the logic is pure `Terminal`-level (grid-cell text extraction
  between a block's `output_start..=output_end` rows), no `Mux` state actually used beyond
  `terminal.block_manager`/`terminal.with_term`, so it ports as a free function or a
  `GpuiShellRoot`-local method taking `&Terminal` directly rather than a `Mux` lookup.
- Context menu: the terminal-grid right-click handler (`render.rs`'s `on_right_click`, M4c) gains
  block detection alongside its existing Copy/Paste/Clear list — at click time (not continuously
  tracked; see §5's identical decision for hover-links), convert the click's pixel position to
  (col, row) via `mouse::pixel_to_cell` (already used for selection), compute the click's absolute
  row the same way this task's `Osc133` handler does, and check `terminal.block_manager
  .block_at_absolute_row(absolute_row)`. If a block is found, prepend `CopyBlockOutput(terminal_id,
  block_id)` and `ReRunCommand(command_text)` items to the menu (both dispatch through
  `dispatch_context_action`'s existing `_ => {}` catch-all, which gains two real arms:
  `CopyBlockOutput` writes this task's new output-text helper's result to the clipboard via
  `cx.write_to_clipboard`, matching `Copy`'s own established pattern; `ReRunCommand` writes
  `format!("{cmd}\n")` to the terminal via `terminal.write_input`, matching `Clear`'s own pattern).

**Scope decision:** the wgpu build has a *separate* trigger surface for the block menu (right-
clicking the exit-code gutter pill specifically, `open_with_block`) distinct from the default
terminal-grid menu. `gpui_shell` has no equivalent gutter-pill visual element. Rather than build
one, this task folds the block actions into the *same* click-time detection this task already
performs, alongside the existing default items — one right-click on a block's row shows Copy /
Paste / Clear plus (when applicable) Copy Output / Re-run Command in one flat list, rather than
two visually distinct menu modes. Simpler, no new trigger surface, still unblocks both actions.

**`SendToChat` — included here, not deferred further.** Re-checked against the wgpu build's own
dispatch (`src/app/mod.rs:882-895`): `ContextAction::SendToChat` operates purely on
`terminal.selection_text()` (opens the chat panel, pre-fills the composer with the selection) —
it has **no dependency on command blocks or ACP/tool-calling** despite being grouped under "blocks
`SendToChat`" in the M4 spec's own §7. `gpui_shell`'s chat panel (M3b) already exists with a real
composer `TextInput`. Added to the default menu's item list (visible whenever a selection is
active, matching the wgpu build's own conditional) with a new `dispatch_context_action` arm:
`self.chat.open(cx)` (or equivalent visibility toggle already used by `ToggleAiPanel`) +
`self.chat.composer` content set to the selection text, mirroring `ChatPanelView`'s established
`set_content` pattern from M3a/M4a.

## 4. Task 2 — Snippets

**What's reused:** `config.snippets: Vec<SnippetConfig>` (already parsed, already flows through
`gpui_shell`'s config — no new config-schema work). `Action::ExpandSnippet(String)` and
`CommandPalette::rebuild_snippets` already exist in `src/ui/palette/`.

**The real gap (verified, not assumed):** the wgpu build's `try_expand_snippet`
(`src/app/input/mod.rs:801-833`) matches the Tab key against the **last word** of
`self.input_echo` — a field on the wgpu-only `Input` struct, kept in sync via
`InputShadow::on_key(&winit::event::KeyEvent, &winit::event::Modifiers)`
(`src/term/input_shadow.rs:84`). Two real findings from investigating this:
1. `gpui_shell` **never drives `InputShadow` at all** (confirmed via grep: zero calls to
   `input_shadow.on_key`/`.on_osc133` anywhere under `src/gpui_shell/`) — the field exists on
   every `gpui_shell` `Terminal` (it's on the shared struct) but stays permanently inert.
2. `InputShadow::on_key`'s own signature is hard-coupled to `winit::event::KeyEvent`/`Modifiers`/
   `Key`/`NamedKey` — `gpui_shell` has none of these types in its input path (`key_map.rs`
   translates gpui's own `Keystroke` straight to PTY bytes, bypassing winit entirely).

Fully decoupling `InputShadow` (an engine-agnostic key-action enum both binaries feed, unlocking
ghost-text/history-completion in `gpui_shell` too, not just snippets) is real, valuable,
**separately-scoped** work — recorded as a new deferred item (§7) rather than folded into this
task, because it touches shared code both binaries depend on and `InputShadow`'s other features
(ghost text, PATH resolution, history) are out of scope for "snippets" specifically. Per the
brainstorm's own scope decision: build a minimal, `gpui_shell`-local, snippet-only word-tracker
instead.

**New code — `src/gpui_shell/snippets.rs`:**
- A tiny state struct (or a field directly on `GpuiShellRoot`, decided at plan time based on
  actual line-count pressure): `snippet_word_buf: String`, mirroring only what `try_expand_snippet`
  actually reads — the last non-whitespace word since the terminal's own prompt last started.
  Driven by two hooks already available from this task's own Task 1 sibling wiring: `Osc133Marker
  ::PromptStart` (from the same `poll.rs` match this task's Task 1 adds) clears the buffer;
  every regular (non-control, non-Tab) character key that reaches the PTY (`input.rs`'s existing
  key-forwarding path) appends to it; Backspace/arrow/history-nav keys reset or trim it using the
  same boundary rules `InputShadow::on_key`'s own match arms already encode (ported narrowly, not
  the whole struct).
- `try_expand_snippet(config: &Config, terminal: &Terminal) -> bool` — same shape as the wgpu
  build's own function: on Tab, look up `config.snippets` for a trigger matching the tracked word;
  on match, write backspaces (word length) + the snippet body to the PTY via `terminal
  .write_input`, clear the tracked word, return `true` (caller doesn't forward Tab to the PTY);
  on no match, return `false` (caller's existing Tab-forwarding behavior is unchanged).
- Wired into `input.rs`'s existing Tab handling, in front of the ordinary key-forwarding path —
  same "try snippet first, fall through to PTY" order the wgpu build uses.
- Command palette: `gpui_shell_actions` (`palette_dispatch.rs`) currently filters `ExpandSnippet`
  out entirely — this task removes that filter and adds a real `ExpandSnippet(body)` arm to
  `dispatch_palette_action`, writing `body` to the active terminal directly (palette-triggered
  snippets don't need the trigger-word erase step, since there's no typed trigger to erase).
  `gpui_shell`'s own palette item list needs the snippet entries themselves too — mirrors
  `CommandPalette::rebuild_snippets`'s existing snippet-to-`PaletteAction` conversion, called once
  at `GpuiShellRoot::new` and again on config hot-reload (`poll.rs`'s existing reload branch,
  alongside the palette's `rebuild_keybinds` call this task's sibling work in that branch already
  makes).

## 5. Task 3 — Hover-link detection

**What's reused:** `src/app/hover_link.rs`'s `scan_link_at(row_text: &str, cursor_col: usize) ->
Option<(usize, usize, HoverLinkKind, String)>` and `path_for_open(text: &str) -> &str` (147 lines
total) — both pure `&str`/`usize` functions, zero coupling to anything winit/wgpu-specific.
Reused directly, unmodified.

**Scope decision — click-time detection, not continuous hover tracking.** The wgpu build tracks
`hover_link` continuously (recomputed on every mouse-move, driving a live underline/cursor-change
affordance while the pointer sits over a link) — `gpui_shell` has no such continuous-hover
rendering today, and building one is a separate visual feature this milestone doesn't need to add
to unblock `OpenLink`/`CopyLink`. Instead, matching this spec's identical decision for command
blocks (§3): compute link-at-position **once, at right-click time**. Convert the click's pixel
position to (col, row) via `mouse::pixel_to_cell`, extract that row's text via `terminal
.with_term(...)` grid access (same pattern this spec's Task 1 block-output helper and the wgpu
build's `Mux::viewport_row_text` both already use), call `scan_link_at(&row_text, col)`. A
continuous hover-highlight affordance is a legitimate future nicety, explicitly not part of this
task.

**New code:**
- The terminal-grid `on_right_click` handler (already gaining block detection in Task 1 above)
  gains a *third* check, evaluated first (matching the wgpu build's own
  precedence: link menu takes priority over block menu takes priority over the default menu): if
  `scan_link_at` finds a link at the click position, the menu's item list becomes `[OpenLink(text),
  CopyLink(text), Copy, Paste]` (mirrors `ContextMenu::open_with_link`'s own item list,
  `src/ui/context_menu.rs:151-165`) instead of the default/block list.
- `dispatch_context_action` gains two real arms: `OpenLink(url)` — same path-vs-URL branch the
  wgpu build uses (`url.starts_with('/') || "./" || "../"` → `hover_link::path_for_open(&url)`,
  else use `url` as-is) piped into `std::process::Command::new("open").arg(open_arg).spawn()`
  (fire-and-forget, matching the wgpu build's own un-awaited spawn); `CopyLink(url)` — `cx
  .write_to_clipboard(gpui::ClipboardItem::new_string(url))`, matching `Copy`'s own established
  pattern.

## 6. Task 4 — Git-branch picker

**What's reused:** `src/app/ui/git.rs`'s `open_branch_picker`/`poll_branch_scan`/`git_checkout`
(226 lines) already use plain `std::thread::spawn` + `crossbeam_channel`, zero winit coupling.
`gpui_shell` already has the sibling mechanism working end-to-end for the status bar
(`status_bar::poll_git_branch`, M2) — this task is the same shape, proven low-risk.

**New code:**
- `GpuiShellRoot` gains `branch_scan_rx: Option<crossbeam_channel::Receiver<Vec<String>>>` and
  `branch_scan_cwd: Option<PathBuf>` (mirrors `UiManager`'s own fields).
- `open_branch_picker(&mut self, cwd: &Path)`: opens the palette with a one-item loading
  placeholder (`PaletteAction { name: "Loading branches…", action: Action::Noop, keybind: None
  }`, same as the wgpu build), spawns `list_git_branches_sync` (reused from `git.rs` — pure,
  already engine-agnostic, just a `git branch` subprocess call) on `std::thread::spawn`, sends the
  result back over the new channel.
- `poll_branch_scan(&mut self) -> bool`, called from `poll.rs`'s existing 33ms tick alongside the
  already-wired `status_bar::poll_git_branch`: drains the channel, converts each branch name into
  a `PaletteAction { action: Action::GitCheckout(name), .. }` (marking the current branch with a
  trailing `✓`, matching the wgpu build's own display), repopulates the palette via
  `CommandPalette::open_with_items` (already exists, already used by M4a's palette).
- `gpui_shell_actions`/`dispatch_palette_action` gain `GitCheckout(branch)` — a real arm calling a
  new small `git_checkout(branch: &str, cwd: &Path)` (ported verbatim from `git.rs`'s own
  synchronous `std::process::Command::new("git").args(["-C", cwd, "checkout", branch]).status()`),
  then invalidating `self.git_branch` (the same cache `status_bar::poll_git_branch` already
  maintains) so the status bar's branch segment refreshes immediately.
- **Trigger — corrected during this spec's own self-review, not assumed.** The wgpu build does
  NOT expose a palette entry for this at all: `open_branch_picker` is only ever called from one
  site, `src/app/mod.rs:1092`, reached by *clicking the status bar's git-branch segment*
  (`bar.click_kind(col, total_cols)` hit-testing). `gpui_shell`'s status bar has no click handling
  of any kind yet (confirmed via grep — zero `on_mouse_down` anywhere in `status_bar.rs`), and
  building general segment-click hit-testing is a bigger, differently-scoped addition than this
  task warrants. Instead: add one new variant to the *shared* `crate::ui::palette::actions::Action`
  enum, `Action::OpenBranchPicker` (no payload), plus one new entry in `built_in_actions` (small,
  additive change to shared code — explicitly permitted by this spec's own Global Constraints,
  §2 — and a strict improvement for the wgpu build too, which gains a second, palette-driven way
  to reach a picker it previously could only open by clicking a status-bar pixel). `gpui_shell_
  actions` gains that variant; `dispatch_palette_action` gains an arm calling `self
  .open_branch_picker(&cwd)` using the active terminal's cached cwd (`self.cached_cwd`, already
  tracked by the poll loop).

## 7. Task 5 — Saved workspaces

**What's reused:** `src/app/mux/snapshot.rs` (103 lines) — `WorkspaceSnapshot`/`TabSnapshot`/
`PaneNodeSnapshot`/`SplitDirSnapshot` (plain `serde`-derived structs) plus `list_saved_workspaces`/
`load_workspace`/`save_snapshot` (pure disk I/O, no `Mux` coupling in this file at all). Reused
verbatim, unmodified — this is already a clean, engine-agnostic serialization boundary.

**What's rebuilt:** `Mux::save_workspace`/`build_workspace_snapshot`/`snapshot_pane_node`
(`src/app/mux/workspace.rs:178-260`) walk `Mux`'s own `TabManager`/`PaneNode` tree — `gpui_shell`
has its own, structurally similar but separately-defined tree (`Workspace`/`WorkspaceManager`,
`src/gpui_shell/workspace.rs`; `PaneTree::{Leaf{terminal_id}, Split{node_id, dir, ratio, left,
right}}`, `src/gpui_shell/panes/mod.rs`) — new build/apply functions against `gpui_shell`'s own
types, following the snapshot *format* exactly:
- `build_workspace_snapshot(workspace: &Workspace, terminals: &HashMap<usize, Rc<Terminal>>) ->
  WorkspaceSnapshot`: walks `workspace.tab_panes`, converting each `PaneTree::Leaf{terminal_id}`
  into `PaneNodeSnapshot::Leaf{cwd}` via `crate::term::process_cwd(terminal.child_pid)` (already
  used identically by `poll.rs`'s own status-bar CWD tracking), each `Split{dir, ratio, left,
  right}` into `PaneNodeSnapshot::Split` (dropping `node_id` — snapshot's `Split` variant has no
  id field, a fresh one is assigned on restore via `panes::next_node_id()`, already `pub(crate)`).
- `restore_workspace(snap: WorkspaceSnapshot, config: &Config) -> Workspace`-shaped logic:
  `self.workspaces.new_workspace(snap.name)`, then for each `TabSnapshot`, recursively walk
  `pane_tree` spawning a real terminal per `Leaf` via the existing `spawn_terminal(cols, rows,
  config)` (used by every other terminal-creation path in `gpui_shell` already — `mod.rs::new`,
  splits, new tabs) at the leaf's saved `cwd` (`spawn_terminal`'s signature gains an optional cwd
  override if it doesn't already take one — confirmed at plan time), rebuilding `PaneTree::Split`
  nodes with fresh `node_id`s, then `tabs.new_tab(title)` + `tabs.set_tab_color(idx, accent_color)`
  (both pre-existing `TabManager` methods, already used by M4c). The wgpu build's own
  `restore_pane_recursive` needs a `winit::event_loop::EventLoopProxy<()>` purely to construct new
  PTYs with a wakeup handle — `gpui_shell`'s `spawn_terminal` already has its own wakeup
  mechanism (no winit dependency, confirmed by every prior terminal-spawn call site this
  migration has built), so this parameter simply doesn't carry over.
- Command palette: `gpui_shell_actions` gains `SaveWorkspace`, `OpenSavedWorkspaces`,
  `RestoreWorkspace(String)`. `dispatch_palette_action` gains: `SaveWorkspace` → build + save a
  snapshot of the active workspace; `OpenSavedWorkspaces` → `snapshot::list_saved_workspaces()`
  converted to `PaletteAction`s (same display format as the wgpu build: `"{name} ({tab_count}
  tabs) — {saved_at}"`) fed through `CommandPalette::open_with_items` (same mechanism Task 4's
  branch picker already uses); `RestoreWorkspace(path)` → `snapshot::load_workspace(&path)` then
  this task's own `restore_workspace`.

## 8. Deferred (recorded, not reopened for reconsideration here)

- **`InputShadow`/winit decoupling** (new finding, §4) — full engine-agnostic key-action bridge
  for `src/term/input_shadow.rs`, unlocking ghost-text/history-completion parity in `gpui_shell`
  too. Real, valuable, but bigger than "snippets" and touches shared code both binaries depend
  on. Candidate for a future milestone, not blocking M5c.
- **Continuous hover-highlight rendering** (§5) — a live underline/cursor-change affordance while
  the pointer sits over a link, matching the wgpu build's own `hover_link` field tracking. This
  spec's click-time detection unblocks `OpenLink`/`CopyLink` without it.
- **Exit-code info popup** (`ContextMenu::open_exit_info`) — a third wgpu-build context-menu mode
  (right-clicking the status bar's exit-code segment specifically) not mentioned in the original
  M4 deferred list and not added here; left for a future milestone if wanted.
- Everything M5a/M5b own (ACP, tool-calling, confirm cards, undo, file picker, suggestion pills,
  `Leader a e/f`) — untouched by this spec, designed separately.

## 9. Manual testing required (cannot be verified from the agent sandbox)

Per this project's established testing discipline — every item below needs the user's own hands:

**Command blocks:**
- Run a few commands, right-click a completed command's output row — "Copy Output" and "Re-run
  Command" appear alongside Copy/Paste/Clear; both do the right thing.
- Right-click a row with a selection active — "Send to Chat" appears, opens the chat panel with
  the selection pre-filled in the composer.
- `CSI 2 J`/`CSI 3 J` (e.g. `clear` then scrollback-clear) removes block decorations — right-click
  no longer shows block actions for rows before the clear.

**Snippets:**
- Configure a snippet trigger in `config.lua`, type the trigger word in the terminal, press Tab —
  expands to the snippet body, trigger word erased first.
- Tab with no matching trigger word still sends a literal Tab to the shell (completion etc. still
  works).
- `Leader o` → a snippet entry runs it (writes the body directly, no trigger-erase needed).
- Snippet list updates after a config hot-reload that changes `config.snippets`.

**Hover-link:**
- Right-click a URL or file path in terminal output — "Open Link"/"Copy Link" appear instead of
  the default menu; both do the right thing (URL opens in the default browser, path opens via
  `open`, clipboard gets the raw text).
- Right-click text that isn't a link — ordinary default menu, unaffected.

**Git-branch picker:**
- Trigger the picker from the palette — a loading placeholder appears immediately, then real
  branch names populate; the current branch is marked.
- Selecting a branch runs `git checkout`, the status bar's branch segment updates.

**Saved workspaces:**
- `Leader o` → Save Workspace — a snapshot file appears on disk.
- `Leader o` → Saved Workspaces → pick one — a new workspace is created matching the saved tab/
  pane layout and accent colors (fresh shells at the saved CWDs, not restored process state,
  matching the wgpu build's own documented behavior).
