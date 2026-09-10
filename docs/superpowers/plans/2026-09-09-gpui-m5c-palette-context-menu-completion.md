# M5c — Palette & Context-Menu Feature Completion Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Close out 5 independent, previously-deferred `gpui_shell` features: command blocks, snippets, hover-link detection, the git-branch picker, and saved workspaces.

**Architecture:** Each task reuses whatever's already engine-agnostic pure logic under `src/term/`, `src/app/ui/`, or `src/app/mux/snapshot.rs`, and rebuilds only the `gpui_shell`-side storage/rendering/wiring — the same "port the logic, rebuild the rendering" split M4a-d proved out repeatedly. Tasks 1-3 share one right-click context-menu scaffold (Task 1 builds it, Task 3 extends it); Tasks 4 and 5 are fully independent of 1-3 and each other.

**Tech Stack:** Rust 2021, gpui 0.2.2.

**Spec:** `docs/superpowers/specs/2026-09-09-gpui-m5c-palette-context-menu-completion-design.md`

## Global Constraints

- 400-line module limit (`AGENTS.md`). Current state, verified fresh before writing this plan:
  `poll.rs` 212, `input.rs` 400 (**at the ceiling already**), `render.rs` 400 (**at the
  ceiling**), `palette_dispatch.rs` 166, `mod.rs` 398, `context_menu.rs` 226, `tabs.rs` 399 (**1
  under**), `workspace.rs` 334, `panes/mod.rs` 242, `render_callbacks.rs` 128,
  `status_bar/git.rs` 240. `input.rs` and `render.rs` will overshoot the instant either gains even
  one line — every task touching them must check `wc -l` after its own edit and fix any overshoot
  before committing (same extraction-into-a-new-file pattern used throughout M4a-d), not defer it.
- `scripts/ci-local.sh` must stay green after every task (clippy `-D warnings`, `fmt --check`,
  full `cargo test --lib`, `cargo audit`); run `cargo fmt` proactively before every commit, not
  just `--check` after — three separate controller-authored-extraction commits in M4a/M4b needed
  a `cargo fmt` pass after the code was otherwise correct.
- Tests cover **logic only** — no painting/hit-testing tests; every UI-facing item is a dogfood
  step (§9 of the spec), not a unit test to write. Pure-logic pieces (the snippet word-matcher,
  the workspace-snapshot tree conversion) do get real unit tests.
- Commit format: `type: Message.` per `AGENTS.md`.
- `#[allow(dead_code)]` (narrowly scoped, comment naming the removing task) for anything built
  ahead of its first caller within this plan's own task sequence.
- Real, verified correction to the spec's own Task 1 design (found while writing this plan, not
  by an implementer): `Terminal`'s `block_manager`/`input_shadow` fields (`src/term/mod.rs`) are
  the **only two** fields on that struct without interior mutability — every other mutable field
  (`term`, `cols`, `rows`) is explicitly `Cell`/`Mutex`-wrapped, with a doc comment stating why:
  `gpui_shell` holds terminals behind `Rc<Terminal>`, where `&mut Terminal` isn't obtainable.
  Calling `terminal.block_manager.on_marker(...)` (a `&mut self` method) through `Rc<Terminal>`
  will not compile. Fix: `GpuiShellRoot` gets its own `block_managers: HashMap<usize,
  crate::term::BlockManager>` sidecar map, populated/cleaned at the exact same 4 insert + 1 remove
  call sites `wakeup_gates` already uses (confirmed via `grep -rn "wakeup_gates\."
  src/gpui_shell/*.rs`: `mod.rs:259`, `actions.rs:31`, `leader_dispatch.rs:36,104` insert;
  `actions.rs:256` remove). Plain `HashMap`, no `RefCell` needed: `poll.rs`'s own loop only ever
  needs `this.block_managers.get_mut(&id)` as a direct field projection disjoint from the `for
  (&id, terminal) in &this.terminals` loop it sits inside (Rust's borrow checker allows this --
  two different fields of the same `&mut self`, neither access going through an opaque method
  call that would borrow all of `self`); every other consumer (the context-menu's `on_right_click`
  closure, Task 3's link check) already runs inside a `root: &mut GpuiShellRoot` closure with full
  access, no aliasing concern.
- `Terminal`'s own `block_manager`/`input_shadow` fields stay completely untouched by this plan --
  `gpui_shell` never reads or writes them. The wgpu binary's own `Mux`-driven usage of those two
  fields is unaffected.

---

## Task 1: Command blocks

**Tier: standard.** New cross-cutting state (a new sidecar map touched from 6 existing call
sites across 3 files) plus real tree/grid-reading logic -- not a mechanical port.

**Files:**
- Create: `src/gpui_shell/blocks.rs`
- Modify: `src/gpui_shell/mod.rs`
- Modify: `src/gpui_shell/poll.rs`
- Modify: `src/gpui_shell/actions.rs`
- Modify: `src/gpui_shell/leader_dispatch.rs`
- Modify: `src/gpui_shell/render_callbacks.rs`
- Modify: `src/gpui_shell/context_menu.rs`
- Modify: `src/gpui_shell/terminal_element.rs`
- Modify: `src/gpui_shell/render.rs`

**Interfaces:**
- Consumes: `crate::term::{BlockManager, Block}` (`src/term/blocks.rs`, unmodified), `crate::term::
  osc133::Osc133Marker` (unmodified), `crate::term::PtyEvent::{Osc133, ScreenCleared}`
  (unmodified), `terminal.with_term(|t| ...)` (unmodified), `mouse::pixel_to_cell(position:
  Point<Pixels>, bounds: Bounds<Pixels>, cell_width: Pixels, cell_height: Pixels, cols: usize,
  rows: usize) -> (usize, usize)` (already exists, unmodified).
- Produces: `GpuiShellRoot::block_managers: HashMap<usize, BlockManager>` (new field, consumed by
  Task 3); `GpuiShellRoot::block_output_text(&self, terminal_id: usize, block_id: usize) ->
  Option<String>` (new method, `pub(super)`); `blocks::row_text_and_absolute_row(terminal:
  &Terminal, row: usize) -> (String, i64)` (new free function, consumed by Task 3); a **changed**
  `context_menu::RightClickCallback` signature -- was `Rc<dyn Fn(Point<Pixels>, &mut Window, &mut
  App)>` (M4c), becomes `Rc<dyn Fn(Point<Pixels>, usize, usize, &mut Window, &mut App)>` (adds the
  clicked cell's `col`, `row`) -- and a correspondingly changed `context_menu::
  register_right_click` signature, both detailed in Step 5 below; `dispatch_context_action`'s real
  arms for `CopyBlockOutput`/`ReRunCommand`/`SendToChat`.

**Real finding, corrected while writing this plan (not by an implementer):** the spec's own §3
assumed click-time block detection happens inside `render_callbacks.rs`'s `on_right_click`
closure, but that closure's current signature (M4c) only ever receives a raw `Point<Pixels>` --
no `bounds`/cell-metrics are available there to convert it into a (col, row) cell position at all.
The conversion inputs (`bounds`, `cell_width`, `cell_height`, `cols`, `rows`) only exist inside
`terminal_element.rs`'s `paint()`, where `context_menu::register_right_click` is actually called.
Fix: `register_right_click` now performs the `mouse::pixel_to_cell` conversion itself (it already
receives `bounds`; Step 5 adds the four missing parameters to its own signature) and forwards the
resulting `(col, row)` into the callback alongside the original pixel `Point` (still needed for
the menu's own on-screen placement). This is why `terminal_element.rs` joins this task's Files
list, and why `RightClickCallback`'s signature changes.

- [ ] **Step 1: `src/gpui_shell/blocks.rs` -- the output-text helper**

Ported from `Mux::block_output_text` (`src/app/mux/mod.rs:583-604`), adapted to read `&Terminal`
and `&BlockManager` directly instead of through `Mux`:

```rust
// gpui chrome migration (M5c Task 1): command-block state and the
// output-text helper. `crate::term::{Block, BlockManager}` (src/term/
// blocks.rs) is already engine-agnostic -- ported from Mux's own
// block_output_text (src/app/mux/mod.rs:583-604) with the Mux lookup
// dropped in favor of taking a `&Terminal` directly.
//
// `GpuiShellRoot`'s own `block_managers` sidecar (mod.rs) exists because
// `Terminal.block_manager` itself has no interior mutability -- see this
// plan's own Global Constraints for why reusing that field directly
// doesn't compile through `Rc<Terminal>`.

use crate::term::{BlockManager, Terminal};

use super::GpuiShellRoot;

impl GpuiShellRoot {
    /// The captured output text of one completed command block, or `None`
    /// if the block doesn't exist or is still streaming (`output_end` is
    /// `None`).
    pub(super) fn block_output_text(&self, terminal_id: usize, block_id: usize) -> Option<String> {
        let terminal = self.terminals.get(&terminal_id)?;
        let manager = self.block_managers.get(&terminal_id)?;
        block_output_text_for(terminal, manager, block_id)
    }
}

fn block_output_text_for(terminal: &Terminal, manager: &BlockManager, block_id: usize) -> Option<String> {
    let block = manager.find_block_by_id(block_id)?;
    let output_end = block.output_end?;
    let output_start = block.output_start;

    Some(terminal.with_term(|term| {
        use alacritty_terminal::index::{Column, Line};
        let history_size = term.grid().history_size() as i64;
        let cols = term.columns();
        let mut lines = Vec::new();

        for abs_row in output_start..=output_end {
            let grid_idx = (abs_row - history_size) as i32;
            let mut text = String::new();
            for col in 0..cols {
                let cell = &term.grid()[Line(grid_idx)][Column(col)];
                text.push(if cell.c == '\0' { ' ' } else { cell.c });
            }
            lines.push(text.trim_end().to_string());
        }
        lines.join("\n")
    }))
}

/// One grid read serving both this task's own block-detection and Task
/// 3's link detection: `row`'s visible text (ported from `Mux::viewport_
/// row_text`, `src/app/mux/mod.rs:558-578`, adapted to take `&Terminal`
/// directly) AND that row's "absolute row from top of buffer" (the same
/// coordinate space `Block::prompt_row`/`output_start`/`output_end` use,
/// per `src/term/blocks.rs`'s own doc comment: `absolute_row = history_
/// size + grid_cursor_line`, solved here for an arbitrary clicked `row`
/// instead of the cursor's own line).
pub(super) fn row_text_and_absolute_row(terminal: &Terminal, row: usize) -> (String, i64) {
    terminal.with_term(|term| {
        use alacritty_terminal::index::{Column, Line};
        let cols = term.columns();
        let screen_rows = term.screen_lines() as i32;
        let display_offset = term.grid().display_offset() as i32;
        let history_size = term.grid().history_size() as i64;
        let grid_line = row as i32 - display_offset;
        let absolute_row = history_size + grid_line as i64;
        // Negative `grid_line` is a valid, reachable case (a visible row
        // showing scrolled-back history, per `Line`'s own negative-index
        // support into `Grid`'s history) -- only the upper bound is a real
        // out-of-range guard, matching `viewport_row_text`'s own real
        // guard exactly (`src/app/mux/mod.rs:569`).
        if grid_line >= screen_rows {
            return (String::new(), absolute_row);
        }
        let mut text = String::with_capacity(cols);
        for col in 0..cols {
            let cell = &term.grid()[Line(grid_line)][Column(col)];
            text.push(if cell.c == '\0' { ' ' } else { cell.c });
        }
        (text.trim_end().to_string(), absolute_row)
    })
}
```

- [ ] **Step 2: `mod.rs` -- add the field and register the module**

Add `mod blocks;` in alphabetical order (right after `mod ai_block;`, right before `mod
chat_panel;`).

Add the field to `GpuiShellRoot`, right after the existing `wakeup_gates:
HashMap<usize, Arc<WakeupGate>>,` field (read the file first to confirm its exact current text --
this plan's own line-count table above was verified moments before writing this task, but re-check
before editing since Task 2-5 in this same plan also touch `mod.rs`):

```rust
    /// One `BlockManager` per terminal -- see this plan's own Global
    /// Constraints for why this lives here rather than on `Terminal`
    /// itself (`Terminal.block_manager` has no interior mutability).
    /// Populated/cleaned at the exact same call sites as `wakeup_gates`.
    block_managers: HashMap<usize, crate::term::BlockManager>,
```

In `GpuiShellRoot::new`, right after the existing `wakeup_gates.insert(terminal_id, gate);` line,
add:

```rust
        let mut block_managers = HashMap::new();
        block_managers.insert(terminal_id, crate::term::BlockManager::new());
```

And add `block_managers,` to the `Self { .. }` construction, right after the existing
`wakeup_gates,` field.

- [ ] **Step 3: the other 3 insert sites + 1 remove site**

In `actions.rs`, right after the existing `self.wakeup_gates.insert(terminal_id, gate);` (there is
exactly one such line in this file, inside whichever function spawns a new terminal for a split):

```rust
        self.block_managers
            .insert(terminal_id, crate::term::BlockManager::new());
```

In `leader_dispatch.rs`, right after EACH of the two existing `self.wakeup_gates.insert(terminal_id,
gate);` lines (one for new-tab, one for new-workspace's initial tab -- both already exist, add the
same two lines after each):

```rust
        self.block_managers
            .insert(terminal_id, crate::term::BlockManager::new());
```

In `actions.rs`, right after the existing `self.wakeup_gates.remove(&terminal_id);` line:

```rust
        self.block_managers.remove(&terminal_id);
```

- [ ] **Step 4: `poll.rs` -- drain `Osc133`/`ScreenCleared`, feed `BlockManager`**

This closes the command-block slice of `TD-GPUI-04` (`.context/quality/TECHNICAL_DEBT.md`).
Replace the existing PTY-event drain block (currently: `for (&id, terminal) in &this.terminals {
while let Ok(event) = terminal.pty.rx.try_recv() { if matches!(event, crate::term::PtyEvent::
Exit(_)) { exited_terminals.push(id); } } }`) with:

```rust
                    let mut exited_terminals = Vec::new();
                    for (&id, terminal) in &this.terminals {
                        while let Ok(event) = terminal.pty.rx.try_recv() {
                            match event {
                                crate::term::PtyEvent::Exit(_) => {
                                    exited_terminals.push(id);
                                }
                                crate::term::PtyEvent::Osc133(marker) => {
                                    let command_text = match &marker {
                                        crate::term::osc133::Osc133Marker::CommandStart(cmd) => {
                                            cmd.clone()
                                        }
                                        _ => String::new(),
                                    };
                                    let absolute_row = terminal.with_term(|t| {
                                        let content = t.renderable_content();
                                        let history = t.grid().history_size() as i64;
                                        let cursor_vp = content.cursor.point.line.0.max(0) as i64;
                                        let disp_off = content.display_offset as i64;
                                        history + cursor_vp - disp_off
                                    });
                                    if let Some(manager) = this.block_managers.get_mut(&id) {
                                        manager.on_marker(marker, absolute_row, command_text);
                                    }
                                }
                                crate::term::PtyEvent::ScreenCleared => {
                                    if let Some(manager) = this.block_managers.get_mut(&id) {
                                        manager.clear();
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
```

(This is placed inside the existing `this.update(cx, |this: &mut GpuiShellRoot, cx| { ... })`
closure, replacing only the drain loop shown above -- everything else in that closure, including
`should_notify` and the code that follows, is unchanged. Read `poll.rs` first to find the loop's
exact current line range before editing.)

`Osc133Marker` needs a `use` -- add `use crate::term::osc133::Osc133Marker;` to `poll.rs`'s import
list only if you reference the bare name elsewhere; the code above uses the fully-qualified path
(`crate::term::osc133::Osc133Marker::CommandStart`) so no new import is strictly required, but
prefer adding the import and using the short form if it reads more cleanly -- your call, keep it
consistent with the rest of the match arms' style (which use `crate::term::PtyEvent::X` fully
qualified already, so fully-qualifying `Osc133Marker` too is the more consistent choice).

- [ ] **Step 5a: `context_menu.rs` -- change `RightClickCallback`'s signature**

Read the file's current `RightClickCallback` type alias and `register_right_click` function in
full first (both shown in this plan's own research above). Replace them with:

```rust
pub(super) type RightClickCallback =
    Rc<dyn Fn(Point<Pixels>, usize, usize, &mut Window, &mut App)>;
```

```rust
pub(super) fn register_right_click(
    bounds: Bounds<Pixels>,
    cell_width: Pixels,
    cell_height: Pixels,
    cols: usize,
    rows: usize,
    on_right_click: RightClickCallback,
    window: &mut Window,
) {
    window.on_mouse_event(move |event: &MouseDownEvent, phase, window, cx| {
        if phase != DispatchPhase::Bubble || event.button != MouseButton::Right {
            return;
        }
        if !bounds.contains(&event.position) {
            return;
        }
        let (col, row) = super::mouse::pixel_to_cell(
            event.position,
            bounds,
            cell_width,
            cell_height,
            cols,
            rows,
        );
        on_right_click(event.position, col, row, window, cx);
    });
}
```

(`mouse::pixel_to_cell`'s real signature -- `pixel_to_cell(position: Point<Pixels>, bounds:
Bounds<Pixels>, cell_width: Pixels, cell_height: Pixels, cols: usize, rows: usize) -> (usize,
usize)` -- was confirmed against `src/gpui_shell/mouse.rs` while writing this plan; `super::mouse`
is already a valid path from `context_menu.rs`, both being direct children of `gpui_shell`.)

- [ ] **Step 5b: `terminal_element.rs` -- pass the new parameters at the call site**

Find the existing `context_menu::register_right_click(bounds, self.on_right_click.clone(),
window);` call inside `paint()` (right after the `mouse::register_mouse_handlers(...)` call) and
replace it with:

```rust
        context_menu::register_right_click(
            bounds,
            self.cell_width,
            self.cell_height,
            self.terminal.cols.get() as usize,
            self.terminal.rows.get() as usize,
            self.on_right_click.clone(),
            window,
        );
```

(`self.cell_width`/`self.cell_height` are existing `Pixels` fields on `TerminalGridElement`,
already used two lines above for `mouse::register_mouse_handlers`'s own call. `self.terminal.cols`/
`.rows` are `Cell<u16>` fields on `Terminal` -- `.get()` reads the current value, matching how
this same file's own scrollbar code reads `self.terminal.rows.get()` a few dozen lines below.)

- [ ] **Step 5c: `render_callbacks.rs` -- block detection + `SendToChat` in the right-click menu**

Read `render_callbacks.rs`'s current `on_right_click` closure in full first (it currently always
builds the same 3-item Copy/Paste/Clear list -- see M4c's own commit for the exact current code).
Update its own closure signature to match Step 5a's new `RightClickCallback` type (`move
|position, _col, row, _window, cx| { ... }`, two new parameters -- name the col parameter `_col`
for now: this task's own block detection only needs `row`, and an unused-but-unprefixed parameter
would fail `cargo clippy -D warnings`; Task 3, below, renames it to `col` when it starts using it
for the link check), and replace the item-list construction inside it with:

```rust
                    Rc::new(move |position, _col, row, _window, cx| {
                        right_click_view
                            .update(cx, |root, cx| {
                                root.context_menu.position = position;

                                let active_ws = root.workspaces.active();
                                let active_tid = active_ws.tab_panes
                                    [active_ws.tabs.active_index()]
                                .focused_terminal;

                                let block = root.terminals.get(&active_tid).and_then(|terminal| {
                                    let (_, absolute_row) =
                                        super::blocks::row_text_and_absolute_row(terminal, row);
                                    root.block_managers
                                        .get(&active_tid)
                                        .and_then(|m| m.block_at_absolute_row(absolute_row))
                                        .map(|b| (b.id, b.command_text.clone()))
                                });

                                let mut items = vec![
                                    crate::ui::context_menu::ContextMenuItem {
                                        label: "Copy".to_string(),
                                        keybind: Some("Cmd+C".to_string()),
                                        action: crate::ui::context_menu::ContextAction::Copy,
                                        swatch_color: None,
                                    },
                                    crate::ui::context_menu::ContextMenuItem {
                                        label: "Paste".to_string(),
                                        keybind: Some("Cmd+V".to_string()),
                                        action: crate::ui::context_menu::ContextAction::Paste,
                                        swatch_color: None,
                                    },
                                    crate::ui::context_menu::ContextMenuItem {
                                        label: "Clear".to_string(),
                                        keybind: Some("Cmd+K".to_string()),
                                        action: crate::ui::context_menu::ContextAction::Clear,
                                        swatch_color: None,
                                    },
                                ];

                                if let Some((block_id, command_text)) = block {
                                    items.push(crate::ui::context_menu::ContextMenuItem {
                                        label: "Copy Output".to_string(),
                                        keybind: Some("Leader y".to_string()),
                                        action: crate::ui::context_menu::ContextAction::CopyBlockOutput(
                                            active_tid, block_id,
                                        ),
                                        swatch_color: None,
                                    });
                                    items.push(crate::ui::context_menu::ContextMenuItem {
                                        label: "Re-run Command".to_string(),
                                        keybind: Some("Leader r".to_string()),
                                        action: crate::ui::context_menu::ContextAction::ReRunCommand(
                                            command_text,
                                        ),
                                        swatch_color: None,
                                    });
                                }

                                let has_selection = root
                                    .terminals
                                    .get(&active_tid)
                                    .and_then(|t| t.selection_text())
                                    .is_some();
                                if has_selection {
                                    items.push(crate::ui::context_menu::ContextMenuItem {
                                        label: "Send to Chat".to_string(),
                                        keybind: None,
                                        action: crate::ui::context_menu::ContextAction::SendToChat,
                                        swatch_color: None,
                                    });
                                }

                                root.context_menu.items = items;
                                root.context_menu.visible = true;
                                cx.notify();
                            })
                            .ok();
                    });
```

`BlockManager::block_at_absolute_row(&self, absolute_row: i64) -> Option<&Block>` and
`Block::{id: usize, command_text: String}` are both pre-existing, unmodified
(`src/term/blocks.rs`). `row` here is unused by the link check yet -- Task 3 adds that on top of
this same closure, reusing the `row`/`col` parameters this step already threads through.

(Note the closure body above wraps the ENTIRE existing `right_click_view.update(cx, |root, cx| {
... }).ok();` structure, including the outer `Rc::new(move |...| { ... });` -- read `render_
callbacks.rs`'s real current surrounding code (the `let right_click_view = view.clone();` line
immediately above it, and how the built `Rc` is later returned in the function's final tuple)
before editing, and preserve that surrounding shape exactly; only the closure's parameter list and
its body's item-list construction change.)

- [ ] **Step 6: `context_menu.rs` -- real `CopyBlockOutput`/`ReRunCommand`/`SendToChat` arms**

Replace the trailing `_ => {}` arm in `dispatch_context_action`'s `match action { ... }` with:

```rust
            crate::ui::context_menu::ContextAction::CopyBlockOutput(tid, bid) => {
                if let Some(text) = self.block_output_text(tid, bid) {
                    if !text.is_empty() {
                        cx.write_to_clipboard(gpui::ClipboardItem::new_string(text));
                    }
                }
            }
            crate::ui::context_menu::ContextAction::ReRunCommand(cmd) => {
                if !cmd.is_empty() {
                    let active_ws = self.workspaces.active();
                    let active_tid =
                        active_ws.tab_panes[active_ws.tabs.active_index()].focused_terminal;
                    if let Some(terminal) = self.terminals.get(&active_tid) {
                        terminal.write_input(format!("{cmd}\n").as_bytes());
                    }
                }
            }
            crate::ui::context_menu::ContextAction::SendToChat => {
                let active_ws = self.workspaces.active();
                let active_tid =
                    active_ws.tab_panes[active_ws.tabs.active_index()].focused_terminal;
                if let Some(text) = self
                    .terminals
                    .get(&active_tid)
                    .and_then(|t| t.selection_text())
                {
                    self.pending_send_to_chat = Some(text);
                    cx.notify();
                }
            }
            _ => {}
```

(Read `context_menu.rs`'s own doc comment on `dispatch_context_action` first -- it currently
documents which variants are "never constructed"; update that comment now that `CopyBlockOutput`/
`ReRunCommand`/`SendToChat` ARE constructed, listing only the genuinely still-unused ones:
`CopyLastCommand`, `OpenLink`, `CopyLink`, `Separator`, `Label` -- `OpenLink`/`CopyLink` get real
arms in Task 3, so trim this list again there too.)

**Real finding, corrected while writing this plan:** `ChatPanelView` has no `open(&mut self, cx)`
method -- only `toggle(&mut self, window: &mut Window, cx: &mut Context<GpuiShellRoot>)`
(confirmed against `src/gpui_shell/chat_panel/mod.rs`), which needs `&mut Window`.
`dispatch_context_action` is invoked from a context-menu row's `on_mouse_down` through `Entity::
update(cx, |root, cx| ...)` -- and `Entity::update`'s own closure signature never threads a
`Window` through to its body, regardless of whether the OUTER callback received one (the exact
same "no `Window` inside a `cx`-only closure" constraint this whole migration has hit and solved
repeatedly, e.g. M3b's chat `/q` close, M4a's palette `Submit`). So `SendToChat` can't call `self.
chat.toggle(window, cx)` directly here -- it stashes the selected text in a new `pending_send_to_
chat: Option<String>` field instead, drained at the top of `render()` (Step 6b, below) where
`window` genuinely is available, matching the established `pending_palette_action` pattern
exactly.

- [ ] **Step 6a: `mod.rs` -- the pending-send-to-chat field**

Add, right after Task 1's own `block_managers` field:

```rust
    /// Selected text queued for the chat composer by the context menu's
    /// `SendToChat` action -- `dispatch_context_action` (above) has no
    /// `Window` to open the chat drawer with, so this is drained at the
    /// top of `render()` instead, same shape as `pending_palette_action`.
    pending_send_to_chat: Option<String>,
```

Add `pending_send_to_chat: None,` to the `Self { .. }` construction, right after `block_managers,`.

- [ ] **Step 6b: `render.rs` -- drain it**

Find the existing `if let Some(action) = self.pending_palette_action.take() { ... }` block at the
very top of `render()` and add, right after it:

```rust
        if let Some(text) = self.pending_send_to_chat.take() {
            if !self.chat.is_visible() {
                self.chat.toggle(window, cx);
            }
            self.chat.composer.update(cx, |input, cx| {
                input.set_content(&text, cx);
            });
        }
```

`TextInput::set_content(&mut self, content: &str, cx: &mut Context<Self>)` is already used
identically elsewhere in this codebase (e.g. `standalone_keys.rs`'s `toggle_search_bar`) via
`.update(cx, |input, cx| input.set_content(..., cx))`.

- [ ] **Step 7: Build, test, verify**

Run: `cargo build 2>&1 | tail -100`, `cargo test --lib 2>&1 | tail -10` (expect 230/230 unchanged
-- this task adds no new unit tests, pure wiring), `cargo fmt` then `cargo fmt --check` (clean),
`cargo clippy --all-features -- -D warnings` (clean), `./scripts/ci-local.sh` (exit 0).

Run `wc -l src/gpui_shell/mod.rs src/gpui_shell/poll.rs src/gpui_shell/render_callbacks.rs
src/gpui_shell/context_menu.rs src/gpui_shell/actions.rs src/gpui_shell/leader_dispatch.rs
src/gpui_shell/blocks.rs src/gpui_shell/terminal_element.rs src/gpui_shell/render.rs` and note
results. `render.rs` was already at the 400-line ceiling before this task's own ~7-line `pending_
send_to_chat` drain (Step 6b) -- this WILL overshoot; fix it yourself via the same extraction/
comment-trim pattern used throughout M4a-d (do not defer it), same for any other file that
overshoots.

Dogfood is not possible from this environment -- note in your report that this is pending manual
dogfood (reproduce the spec's §9 "Command blocks" checklist).

- [ ] **Step 8: Commit**

```bash
git add src/gpui_shell/blocks.rs src/gpui_shell/mod.rs src/gpui_shell/poll.rs src/gpui_shell/actions.rs src/gpui_shell/leader_dispatch.rs src/gpui_shell/render_callbacks.rs src/gpui_shell/context_menu.rs src/gpui_shell/terminal_element.rs src/gpui_shell/render.rs
git commit -m "feat: Track command blocks and add block/SendToChat context-menu actions (M5c Task 1)."
```

---

## Task 2: Snippets

**Tier: cheap.** Narrow, self-contained new file plus small, precisely-specified edits to two
existing files.

**Files:**
- Create: `src/gpui_shell/snippets.rs`
- Modify: `src/gpui_shell/mod.rs`
- Modify: `src/gpui_shell/input.rs`
- Modify: `src/gpui_shell/palette_dispatch.rs`
- Modify: `src/gpui_shell/poll.rs`

**Interfaces:**
- Consumes: `config.snippets: Vec<SnippetConfig>` (unmodified), `crate::ui::palette::Action::
  ExpandSnippet(String)` (unmodified, already exists), `crate::term::osc133::Osc133Marker::
  PromptStart` (already available from Task 1's own `poll.rs` match, this task adds one more read
  of the same `marker` value -- no new drain needed).
- Produces: `GpuiShellRoot::maybe_expand_snippet_tab(&mut self, event: &KeyDownEvent, terminal:
  &Terminal, cx: &mut Context<Self>) -> bool` (`pub(super)`); `GpuiShellRoot::track_snippet_key
  (&mut self, event: &KeyDownEvent)` (`pub(super)`).

- [ ] **Step 1: `src/gpui_shell/snippets.rs` -- the word tracker and Tab-expand**

```rust
// gpui chrome migration (M5c Task 2): a narrow, gpui_shell-local
// tracker of "the word currently being typed since the last prompt",
// just enough to drive Tab-triggered snippet expansion -- NOT a port of
// `crate::term::InputShadow` (ghost text, history completion, PATH
// resolution), which stays hard-coupled to winit's `KeyEvent`/
// `Modifiers` with zero gpui_shell caller (see this milestone's own
// spec, §4, for the full investigation and the decision to defer fully
// decoupling it). `try_expand_snippet` mirrors the wgpu build's own
// `Input::try_expand_snippet` (src/app/input/mod.rs:801-833) against
// this narrower tracker instead of `self.input_echo`.

use gpui::{Context, KeyDownEvent};

use crate::config::Config;
use crate::term::Terminal;

use super::GpuiShellRoot;

impl GpuiShellRoot {
    /// Called from `input.rs`'s `on_key_down`, right before the generic
    /// key-forwarding fallthrough, only when `event.keystroke.key ==
    /// "tab"` with no Shift/Control held. Returns `true` if a snippet
    /// trigger matched and was expanded (caller should NOT forward Tab to
    /// the PTY); `false` otherwise (caller's normal Tab-forwarding is
    /// unaffected).
    pub(super) fn maybe_expand_snippet_tab(
        &mut self,
        event: &KeyDownEvent,
        terminal: &Terminal,
        cx: &mut Context<Self>,
    ) -> bool {
        if event.keystroke.key != "tab"
            || event.keystroke.modifiers.shift
            || event.keystroke.modifiers.control
        {
            return false;
        }
        if !try_expand_snippet(&self.config, terminal, &mut self.snippet_word) {
            return false;
        }
        cx.notify();
        true
    }

    /// Called after every key that actually reached the PTY as text (see
    /// `input.rs`'s own call site) -- keeps `self.snippet_word` in sync
    /// with what the shell's line editor is showing, on a best-effort
    /// basis (arrow-key repositioning mid-word is a known, accepted gap:
    /// this tracker only handles the common "type a trigger, press Tab"
    /// pattern, not full cursor-aware editing -- see the spec's own scope
    /// decision on why full `InputShadow` parity is out of scope here).
    pub(super) fn track_snippet_key(&mut self, event: &KeyDownEvent) {
        let key = event.keystroke.key.as_str();
        if event.keystroke.modifiers.platform || event.keystroke.modifiers.control {
            return;
        }
        if key == "backspace" {
            self.snippet_word.pop();
        } else if key == "space" || key == "enter" || key == "escape" {
            self.snippet_word.clear();
        } else if key.chars().count() == 1 {
            self.snippet_word.push_str(key);
        }
    }
}

/// On a Tab press, look up `word` against `config.snippets`' triggers. On
/// a match: erase the trigger (backspaces) + write the snippet body to
/// the PTY, clear `word`, return `true`. On no match: leave `word`
/// untouched, return `false`.
fn try_expand_snippet(config: &Config, terminal: &Terminal, word: &mut String) -> bool {
    if word.is_empty() || config.snippets.iter().all(|s| s.trigger.is_none()) {
        return false;
    }
    let Some(snippet) = config
        .snippets
        .iter()
        .find(|s| s.trigger.as_deref() == Some(word.as_str()))
    else {
        return false;
    };
    let backspaces = vec![0x7fu8; word.len()];
    terminal.write_input(&backspaces);
    terminal.write_input(snippet.body.as_bytes());
    word.clear();
    true
}
```

- [ ] **Step 2: `mod.rs` -- the tracked-word field and OSC133 prompt-reset**

Add `mod snippets;` in alphabetical order (right after `mod separator;`, right before `pub mod
sidebar;`).

Add the field to `GpuiShellRoot`, right after the new `block_managers` field Task 1 added:

```rust
    /// The word typed since the terminal's last prompt, for Tab-triggered
    /// snippet expansion -- see `snippets.rs`'s own doc comment.
    snippet_word: String,
```

Add `snippet_word: String::new(),` to the `Self { .. }` construction, right after `block_managers,`.

- [ ] **Step 3: `poll.rs` -- clear the tracked word on a new prompt**

Inside Task 1's own new `PtyEvent::Osc133(marker) => { ... }` arm (added in Task 1 Step 4), right
after the existing `if let Some(manager) = this.block_managers.get_mut(&id) { manager.on_marker
(marker, absolute_row, command_text); }` line, add a check that only clears the word when the
event is for the CURRENTLY FOCUSED terminal (a different pane's shell reaching a new prompt must
not clear what the user is actively typing elsewhere):

```rust
                                    if matches!(
                                        marker,
                                        crate::term::osc133::Osc133Marker::PromptStart
                                    ) {
                                        let active_ws = this.workspaces.active();
                                        let active_tid = active_ws.tab_panes
                                            [active_ws.tabs.active_index()]
                                        .focused_terminal;
                                        if id == active_tid {
                                            this.snippet_word.clear();
                                        }
                                    }
```

(`marker` was already moved into `manager.on_marker(marker, ...)` on the line above -- reorder so
this new check runs BEFORE that call, reading `marker` by reference first, e.g. restructure Task
1's own block to `let is_prompt_start = matches!(marker, ...PromptStart); manager.on_marker
(marker, absolute_row, command_text); if is_prompt_start && id == active_tid { ... }` -- adjust to
whatever ordering compiles cleanly given `on_marker` takes `marker` by value.)

- [ ] **Step 4: `input.rs` -- wire the Tab guard and key tracker**

Read `input.rs`'s current tail (the final `let mode = terminal.with_term(...); if let Some(bytes)
= super::key_map::translate_key(...) { terminal.write_input(&bytes); cx.notify(); }` block) to
confirm its exact current text, then replace it with:

```rust
        if self.maybe_expand_snippet_tab(event, terminal, cx) {
            return;
        }

        let mode = terminal.with_term(|term| *term.mode());
        if let Some(bytes) = super::key_map::translate_key(
            &event.keystroke,
            mode,
            self.config.keyboard.option_as_meta,
        ) {
            terminal.write_input(&bytes);
            self.track_snippet_key(event);
            cx.notify();
        }
```

`terminal` at this point is `&Rc<Terminal>` (from `self.terminals.get(&active_tid)` earlier in the
same function) -- `maybe_expand_snippet_tab` takes `&Terminal`, so pass `terminal` directly (auto-
deref from `&Rc<Terminal>` to `&Terminal` applies at the call site, same as every other `terminal.
foo()` call already in this file).

- [ ] **Step 5: `palette_dispatch.rs` -- un-filter `ExpandSnippet`, add the real dispatch arm**

In `gpui_shell_actions`'s `matches!` list, add `| Action::ExpandSnippet(_)` (anywhere in the list,
matching the existing one-variant-per-line style).

In `dispatch_palette_action`'s `match action { ... }`, add a real arm (palette-triggered snippets
write the body directly -- no typed trigger word to erase first):

```rust
            Action::ExpandSnippet(body) => {
                let active_ws = self.workspaces.active();
                let active_tid =
                    active_ws.tab_panes[active_ws.tabs.active_index()].focused_terminal;
                if let Some(terminal) = self.terminals.get(&active_tid) {
                    terminal.write_input(body.as_bytes());
                }
            }
```

- [ ] **Step 6: rebuild snippet palette entries on config hot-reload**

`CommandPalette::rebuild_snippets(&mut self, snippets: &[SnippetConfig])` already exists
(`src/ui/palette/mod.rs`) -- it was simply never called from `gpui_shell`. In `poll.rs`'s existing
config-hot-reload closure (the one M4d's Task 2 already added `this.show_toast(...)` to), add,
right after the existing `this.leader_map = leader::build_leader_map(...)` line:

```rust
                            this.palette.rebuild_snippets(&new_config.snippets);
```

(Read the closure first to confirm `new_config` is the right binding name at that point -- it is,
per the existing `this.config = new_config;` line a few lines below it; if snippets need to be
read after that reassignment instead, use `this.config.snippets` -- pick whichever ordering
avoids a moved-value error, since `new_config` is moved into `this.config` later in the same
closure.)

- [ ] **Step 7: Write and run the snippet-matcher unit test**

`try_expand_snippet` is real, pure logic -- add a test module to the bottom of `snippets.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::schema::SnippetConfig;

    #[test]
    fn no_match_leaves_word_untouched() {
        let config = Config {
            snippets: vec![SnippetConfig {
                name: "test".to_string(),
                trigger: Some("gco".to_string()),
                body: "git checkout ".to_string(),
            }],
            ..Config::default()
        };
        let mut word = "xyz".to_string();
        // No terminal available in a unit test -- exercise only the
        // lookup half by checking the trigger search directly, matching
        // the pattern this session's other "logic without a live
        // Terminal" tests use (see term::search's own unit tests for the
        // precedent this mirrors).
        let found = config
            .snippets
            .iter()
            .find(|s| s.trigger.as_deref() == Some(word.as_str()));
        assert!(found.is_none());
        assert_eq!(word, "xyz");
    }

    #[test]
    fn empty_word_never_matches() {
        let config = Config {
            snippets: vec![SnippetConfig {
                name: "test".to_string(),
                trigger: Some("".to_string()),
                body: "x".to_string(),
            }],
            ..Config::default()
        };
        let word = String::new();
        assert!(word.is_empty());
        let _ = config; // no live Terminal to call try_expand_snippet with in a unit test
    }
}
```

Before finalizing this step, read `crate::config::schema::SnippetConfig`'s and `Config`'s real
field lists (`src/config/schema.rs`) and confirm `Config::default()`/`SnippetConfig`'s exact
required fields compile as written above -- adjust field names/add missing required fields rather
than guessing. `try_expand_snippet` itself takes a real `&Terminal`, which a unit test cannot
construct cheaply (it spawns a real PTY) -- if a fuller test of `try_expand_snippet` end-to-end is
wanted, use `word.is_empty()` / trigger-lookup assertions against `config.snippets` directly (as
above) rather than attempting to construct a `Terminal`, matching this project's own "no PTY/
rendering in unit tests" convention.

Run: `cargo test --lib snippets:: -- --nocapture` and confirm both tests pass.

- [ ] **Step 8: Build, test, verify**

Run: `cargo build 2>&1 | tail -100`, `cargo test --lib 2>&1 | tail -10` (expect 232/232 -- 230 +
this task's 2 new tests), `cargo fmt` then `cargo fmt --check` (clean), `cargo clippy
--all-features -- -D warnings` (clean), `./scripts/ci-local.sh` (exit 0).

Run `wc -l src/gpui_shell/mod.rs src/gpui_shell/input.rs src/gpui_shell/palette_dispatch.rs
src/gpui_shell/poll.rs src/gpui_shell/snippets.rs` and note results; `input.rs` was already at the
400-line ceiling before this task's own ~6-line addition -- flag the overshoot in your report's
CONCERNS and fix it yourself via the same extraction/comment-trim pattern used throughout M4a-d
(do not defer it to a later controller pass) before committing.

Dogfood note: pending manual verification (spec's §9 "Snippets" checklist).

- [ ] **Step 9: Commit**

```bash
git add src/gpui_shell/snippets.rs src/gpui_shell/mod.rs src/gpui_shell/input.rs src/gpui_shell/palette_dispatch.rs src/gpui_shell/poll.rs
git commit -m "feat: Add Tab-triggered and palette-triggered snippet expansion (M5c Task 2)."
```

---

## Task 3: Hover-link detection

**Tier: cheap.** Extends Task 1's own scaffolding (the `on_right_click` closure now already
receives `col`/`row` and Task 1's `blocks::row_text_and_absolute_row` already produces row text)
with one more pure-function call and two dispatch arms; no new state.

**Files:**
- Modify: `src/gpui_shell/render_callbacks.rs`
- Modify: `src/gpui_shell/context_menu.rs`
- Modify: `src/app/mod.rs` (only if Step 1 finds `hover_link` is private)

**Interfaces:**
- Consumes: `hover_link::scan_link_at(row_text: &str, cursor_col: usize) -> Option<(usize, usize,
  HoverLinkKind, String)>` and `hover_link::path_for_open(text: &str) -> &str` (`src/app/
  hover_link.rs`, unmodified, pure functions), `blocks::row_text_and_absolute_row` (Task 1, reused
  for its row-text half -- no new grid read needed), the `col`/`row` parameters Task 1's Step 5a
  already added to `RightClickCallback`.
- Produces: real `dispatch_context_action` arms for `OpenLink`/`CopyLink`.

- [ ] **Step 1: confirm `hover_link`'s visibility**

Run `grep -n "mod hover_link" src/app/mod.rs`. If it reads `mod hover_link;` (private), change it
to `pub(crate) mod hover_link;` -- this is the only edit needed in `src/app/`; `hover_link.rs`
itself (`scan_link_at`/`path_for_open`/`HoverLink`/`HoverLinkKind`) is untouched. If it's already
`pub`/`pub(crate)`, skip this step.

- [ ] **Step 2: `render_callbacks.rs` -- link detection, evaluated first**

Inside the same `on_right_click` closure Task 1's Step 5c built: rename its `_col` parameter back
to `col` (now genuinely used), and insert the link check as the very first statement in the
`update(cx, |root, cx| { ... })` closure body, before `root.context_menu.position = position;`
(link menu takes priority over the default/block menu, matching the wgpu build's own precedence,
`src/app/mod.rs:1346-1370`):

```rust
                    Rc::new(move |position, col, row, _window, cx| {
                        right_click_view
                            .update(cx, |root, cx| {
                                root.context_menu.position = position;

                                let active_ws = root.workspaces.active();
                                let active_tid = active_ws.tab_panes
                                    [active_ws.tabs.active_index()]
                                .focused_terminal;

                                let link = root.terminals.get(&active_tid).and_then(|terminal| {
                                    let (row_text, _) =
                                        super::blocks::row_text_and_absolute_row(terminal, row);
                                    crate::app::hover_link::scan_link_at(&row_text, col)
                                });

                                if let Some((_, _, _, text)) = link {
                                    root.context_menu.items = vec![
                                        crate::ui::context_menu::ContextMenuItem {
                                            label: "Open Link".to_string(),
                                            keybind: None,
                                            action: crate::ui::context_menu::ContextAction::OpenLink(
                                                text.clone(),
                                            ),
                                            swatch_color: None,
                                        },
                                        crate::ui::context_menu::ContextMenuItem {
                                            label: "Copy Link".to_string(),
                                            keybind: None,
                                            action: crate::ui::context_menu::ContextAction::CopyLink(
                                                text,
                                            ),
                                            swatch_color: None,
                                        },
                                    ];
                                    root.context_menu.visible = true;
                                    cx.notify();
                                    return;
                                }

                                // -- Task 1's own block-detection + default-item-list
                                // construction continues unchanged below this point. --
                            })
                            .ok();
                    });
```

`hover_link::scan_link_at`'s real return type is `Option<(usize, usize, HoverLinkKind, String)>`
(`col_start, col_end, kind, text`) -- this task only needs `text` (the 4th element) for both menu
items' payload, so the other three are destructured and discarded (`_, _, _, text`).
`HoverLinkKind` (`Url` vs `Path`) does not affect which menu items are shown, only how `OpenLink`'s
own dispatch arm (Step 3, below) decides to open it -- and that arm re-derives the same distinction
from the URL string's own shape (`starts_with('/')`/`"./"`/`"../"`), matching the wgpu build's own
`OpenLink` dispatch exactly, so `kind` itself is never threaded through the context menu at all.

The early `return;` inside the `if let Some(...)` branch is what makes "link menu instead of the
default/block menu" a real either/or -- read Task 1's own committed code first to confirm exactly
where its `root.context_menu.items = items;` / `root.context_menu.visible = true;` / `cx.notify();`
lines sit relative to this insertion point, and place this step's early-return block immediately
before them (not after), so Task 1's own tail never runs when a link was found.

- [ ] **Step 3: `context_menu.rs` -- real `OpenLink`/`CopyLink` arms**

Add, alongside Task 1's own new arms in `dispatch_context_action`:

```rust
            crate::ui::context_menu::ContextAction::OpenLink(url) => {
                let open_arg = if url.starts_with('/') || url.starts_with("./") || url.starts_with("../") {
                    crate::app::hover_link::path_for_open(&url).to_string()
                } else {
                    url
                };
                std::thread::spawn(move || {
                    let _ = std::process::Command::new("open").arg(&open_arg).spawn();
                });
            }
            crate::ui::context_menu::ContextAction::CopyLink(url) => {
                cx.write_to_clipboard(gpui::ClipboardItem::new_string(url));
            }
```

Update the doc comment on `dispatch_context_action` again (per Task 1's own note) to drop
`OpenLink`/`CopyLink` from the "never constructed" list -- only `CopyLastCommand`, `Separator`,
`Label` remain genuinely unused after this task.

- [ ] **Step 4: Build, test, verify**

Run: `cargo build 2>&1 | tail -100`, `cargo test --lib 2>&1 | tail -10` (232/232 unchanged),
`cargo fmt` then `cargo fmt --check` (clean), `cargo clippy --all-features -- -D warnings`
(clean), `./scripts/ci-local.sh` (exit 0).

Run `wc -l src/gpui_shell/render_callbacks.rs src/gpui_shell/context_menu.rs` and note results;
flag any 400-line overshoot in CONCERNS and fix it yourself.

Dogfood note: pending manual verification (spec's §9 "Hover-link" checklist).

- [ ] **Step 5: Commit**

```bash
git add src/gpui_shell/render_callbacks.rs src/gpui_shell/context_menu.rs src/app/mod.rs
git commit -m "feat: Add hover-link detection and Open/Copy Link context-menu actions (M5c Task 3)."
```

---

## Task 4: Git-branch picker

**Tier: cheap.** Mirrors the already-proven `status_bar::poll_git_branch` shape; no novel
mechanism.

**Files:**
- Create: `src/gpui_shell/branch_picker.rs`
- Modify: `src/ui/palette/actions.rs`
- Modify: `src/gpui_shell/mod.rs`
- Modify: `src/gpui_shell/poll.rs`
- Modify: `src/gpui_shell/palette_dispatch.rs`

**Interfaces:**
- Consumes: `CommandPalette::open_with_items(&mut self, items: Vec<PaletteAction>)` (unmodified),
  `PaletteAction { name: String, action: Action, keybind: Option<String> }` (unmodified),
  `self.git_branch: status_bar::GitBranchState` (existing field, this task's `git_checkout`
  invalidates its cache the same way the wgpu build's own `git_checkout` does).
- Produces: `Action::OpenBranchPicker` (new, shared enum variant, consumed by `gpui_shell_
  actions`/`dispatch_palette_action`); `GpuiShellRoot::open_branch_picker(&mut self, cwd: &Path)`,
  `GpuiShellRoot::poll_branch_scan(&mut self) -> bool` (both `pub(super)`).

- [ ] **Step 1: `src/ui/palette/actions.rs` -- the new shared `Action` variant**

Read the file first to find `pub enum Action { ... GitCheckout(String), ... }`'s exact current
text. Add a new variant right next to it:

```rust
    /// Open the palette in branch-picker mode (populates async). Reachable
    /// from the command palette in `gpui_shell`; the wgpu build reaches
    /// the same `open_branch_picker` via a status-bar click instead
    /// (`src/app/mod.rs`'s own dispatch) and does not yet expose this as
    /// a palette entry, though nothing stops it from adopting one too.
    OpenBranchPicker,
```

Add one `built_in_actions` entry for it (read the function's existing entries first to match
their exact struct-literal style -- likely near `GitCheckout`'s own neighboring entries or the
`SaveWorkspace`/`OpenSavedWorkspaces` block already confirmed to exist in this file):

```rust
        PaletteAction {
            name: "Git: Switch Branch".to_string(),
            action: Action::OpenBranchPicker,
            keybind: None,
        },
```

Note: `Action::GitCheckout(String)` already exists and is NOT constructed by `built_in_actions`
itself (it's only ever produced dynamically, by `open_branch_picker`'s own result-population step)
-- do not add a static entry for `GitCheckout`, only for the new `OpenBranchPicker`.

- [ ] **Step 2: `src/gpui_shell/branch_picker.rs`**

```rust
// gpui chrome migration (M5c Task 4): the git-branch picker. Mirrors
// `src/app/ui/git.rs`'s own `open_branch_picker`/`poll_branch_scan`/
// `git_checkout` (std::thread::spawn + crossbeam_channel, zero winit
// coupling) -- the same shape `status_bar/git.rs`'s own `poll_git_
// branch` already proved out in `gpui_shell` for the status bar's
// branch display. `list_git_branches_sync` is ported verbatim (it was
// a private free function in `git.rs`, not reusable directly).

use gpui::Context;

use super::GpuiShellRoot;

impl GpuiShellRoot {
    /// Open the palette in branch-picker mode: a loading placeholder
    /// immediately, real branch names populate async via `poll_branch_
    /// scan` (`poll.rs`'s own 33ms tick).
    pub(super) fn open_branch_picker(&mut self, cwd: &std::path::Path) {
        use crate::ui::palette::{Action, PaletteAction};
        let placeholder = vec![PaletteAction {
            name: "Loading branches…".to_string(),
            action: Action::Noop,
            keybind: None,
        }];
        self.palette.open_with_items(placeholder);
        let (tx, rx) = crossbeam_channel::bounded(1);
        self.branch_scan_rx = Some(rx);
        let cwd_owned = cwd.to_path_buf();
        std::thread::spawn(move || {
            let branches = list_git_branches_sync(&cwd_owned);
            let _ = tx.send(branches);
        });
    }

    /// Drain a completed branch scan and repopulate the palette. Returns
    /// `true` if it updated anything (caller should `cx.notify()`).
    /// Called from `poll.rs`'s existing 33ms tick.
    pub(super) fn poll_branch_scan(&mut self) -> bool {
        let Some(rx) = &self.branch_scan_rx else {
            return false;
        };
        match rx.try_recv() {
            Ok(branches) => {
                self.branch_scan_rx = None;
                if branches.is_empty() {
                    self.palette.close();
                    return true;
                }
                use crate::ui::palette::{Action, PaletteAction};
                let current = self
                    .git_branch
                    .cache
                    .as_deref()
                    .unwrap_or("")
                    .trim_end_matches('*');
                let items: Vec<PaletteAction> = branches
                    .into_iter()
                    .map(|b| {
                        let label = if b == current {
                            format!("  {b}  ✓")
                        } else {
                            format!("  {b}")
                        };
                        PaletteAction {
                            name: label,
                            action: Action::GitCheckout(b),
                            keybind: None,
                        }
                    })
                    .collect();
                self.palette.open_with_items(items);
                true
            }
            Err(_) => false,
        }
    }
}

fn list_git_branches_sync(cwd: &std::path::Path) -> Vec<String> {
    let out = std::process::Command::new("git")
        .args([
            "-C",
            &cwd.to_string_lossy(),
            "branch",
            "--format=%(refname:short)",
        ])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .unwrap_or_default();
    let mut branches: Vec<String> = out
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect();
    branches.sort();
    branches
}
```

`self.git_branch.cache` -- confirm this exact field path against `status_bar::GitBranchState`'s
real definition (`src/gpui_shell/status_bar/git.rs`) before writing this call; the plan's own
earlier verification saw a field named `cache` on this struct (referenced elsewhere in this
codebase as `self.git_branch.cache.as_deref()`, matching `render.rs`'s own status-bar-building
call) -- confirm it still matches at implementation time rather than trusting this note.

- [ ] **Step 3: `mod.rs` -- the new field, module registration**

Add `mod branch_picker;` in alphabetical order (right after `mod ai_block;`... check exact
alphabetical position against the full list at edit time, likely right before `mod chat_panel;`
alongside `mod blocks;` from Task 1 -- keep both new Task-1/Task-4 module lines in the correct
sorted position relative to each other and the rest of the list).

Add the field to `GpuiShellRoot`, right after Task 2's `snippet_word` field:

```rust
    /// In-flight git-branch scan for the branch picker -- see `branch_
    /// picker.rs`'s own doc comment.
    branch_scan_rx: Option<crossbeam_channel::Receiver<Vec<String>>>,
```

Add `branch_scan_rx: None,` to the `Self { .. }` construction, right after `snippet_word:
String::new(),`.

- [ ] **Step 4: `poll.rs` -- drive `poll_branch_scan`**

In the same `this.update(cx, |this, cx| { ... })` closure that already calls `status_bar::poll_
git_branch(...)` (search for that call), add, right after it:

```rust
                    if this.poll_branch_scan() {
                        should_notify = true;
                    }
```

- [ ] **Step 5: `palette_dispatch.rs` -- wire the trigger**

Add `Action::OpenBranchPicker` to `gpui_shell_actions`'s `matches!` list.

Add a real arm to `dispatch_palette_action`:

```rust
            Action::OpenBranchPicker => {
                if let Some(cwd) = self.cached_cwd.clone() {
                    self.open_branch_picker(&cwd);
                }
            }
```

- [ ] **Step 6: Build, test, verify**

Run: `cargo build 2>&1 | tail -100`, `cargo test --lib 2>&1 | tail -10` (232/232 unchanged),
`cargo fmt` then `cargo fmt --check` (clean), `cargo clippy --all-features -- -D warnings`
(clean), `./scripts/ci-local.sh` (exit 0).

Run `wc -l src/gpui_shell/mod.rs src/gpui_shell/poll.rs src/gpui_shell/palette_dispatch.rs
src/gpui_shell/branch_picker.rs` and note results; flag any overshoot in CONCERNS and fix it
yourself.

Dogfood note: pending manual verification (spec's §9 "Git-branch picker" checklist).

- [ ] **Step 7: Commit**

```bash
git add src/gpui_shell/branch_picker.rs src/ui/palette/actions.rs src/gpui_shell/mod.rs src/gpui_shell/poll.rs src/gpui_shell/palette_dispatch.rs
git commit -m "feat: Add the git-branch picker (M5c Task 4)."
```

---

## Task 5: Saved workspaces

**Tier: standard.** Real new tree-conversion logic against two non-trivial, differently-shaped
tree types (`gpui_shell`'s `PaneTree` vs. `snapshot::PaneNodeSnapshot`) -- not a mechanical port.

**Files:**
- Create: `src/gpui_shell/workspace_snapshot.rs`
- Modify: `src/gpui_shell/spawn_terminal.rs`
- Modify: `src/gpui_shell/mod.rs`
- Modify: `src/gpui_shell/actions.rs`
- Modify: `src/gpui_shell/leader_dispatch.rs`
- Modify: `src/gpui_shell/palette_dispatch.rs`

**Interfaces:**
- Consumes: `crate::app::mux::snapshot::{WorkspaceSnapshot, TabSnapshot, PaneNodeSnapshot,
  SplitDirSnapshot, list_saved_workspaces, load_workspace, save_snapshot}` (unmodified, already
  engine-agnostic), `panes::{PaneTree, PaneForest, SplitDir, next_node_id}` (unmodified, `next_
  node_id` already `pub(crate)`), `TabManager::{new_tab, set_tab_color}` (unmodified),
  `WorkspaceManager::new_workspace` (unmodified), `crate::term::process_cwd` (unmodified, already
  used by `poll.rs`).
- Produces: `spawn_terminal_at(cols: u16, rows: u16, config: &Config, cwd: Option<PathBuf>) ->
  anyhow::Result<(Rc<Terminal>, Arc<WakeupGate>)>` (new, `pub(crate)`, `spawn_terminal` becomes a
  thin wrapper calling it with `None`); `GpuiShellRoot::save_active_workspace(&self) ->
  anyhow::Result<()>`, `GpuiShellRoot::restore_workspace(&mut self, snap: WorkspaceSnapshot, cx:
  &mut Context<Self>)` (both `pub(super)`).

- [ ] **Step 1: `spawn_terminal.rs` -- add the cwd-override variant**

Read the file's current exact body first (already confirmed above: `pub(crate) fn spawn_terminal
(cols: u16, rows: u16, config: &Config) -> anyhow::Result<(Rc<Terminal>, Arc<WakeupGate>)> { ...
Terminal::new(config, cols, rows, cell_w, cell_h, wakeup, Arc::clone(&wakeup_gate), None)? ... }`).
Rename the existing function's body into a new `spawn_terminal_at`, and make `spawn_terminal` a
thin wrapper:

```rust
pub(crate) fn spawn_terminal(
    cols: u16,
    rows: u16,
    config: &Config,
) -> anyhow::Result<(Rc<Terminal>, Arc<WakeupGate>)> {
    spawn_terminal_at(cols, rows, config, None)
}

/// Same as `spawn_terminal`, but spawns the shell in `cwd` instead of the
/// process's own working directory -- used by workspace restore
/// (`workspace_snapshot.rs`) to recreate panes at their saved CWDs.
pub(crate) fn spawn_terminal_at(
    cols: u16,
    rows: u16,
    config: &Config,
    cwd: Option<std::path::PathBuf>,
) -> anyhow::Result<(Rc<Terminal>, Arc<WakeupGate>)> {
    let (cell_width, cell_height) = font_state::measured_cell_size();
    let cell_w = f32::from(cell_width).round().max(1.0) as u16;
    let cell_h = f32::from(cell_height).round().max(1.0) as u16;
    let wakeup: crate::term::Wakeup = Arc::new(|| {});
    let wakeup_gate = Arc::new(WakeupGate::new());
    let terminal = Terminal::new(
        config, cols, rows, cell_w, cell_h, wakeup, Arc::clone(&wakeup_gate), cwd,
    )?;
    Ok((Rc::new(terminal), wakeup_gate))
}
```

Add `pub(crate) use spawn_terminal::spawn_terminal_at;` to `mod.rs`'s existing `pub(crate) use
spawn_terminal::spawn_terminal;` line (read that line's exact current text first; it may need to
become two separate `pub(crate) use` lines or one combined `pub(crate) use spawn_terminal::
{spawn_terminal, spawn_terminal_at};`).

- [ ] **Step 2: `workspace_snapshot.rs` -- build**

```rust
// gpui chrome migration (M5c Task 5): build/restore a WorkspaceSnapshot
// (crate::app::mux::snapshot -- already engine-agnostic, reused
// verbatim) against gpui_shell's OWN Workspace/PaneTree types, since
// gpui_shell doesn't use Mux at all. Mirrors Mux::build_workspace_
// snapshot/snapshot_pane_node (src/app/mux/workspace.rs:194-260) and
// Mux::restore_workspace/restore_pane_recursive (:273-340), minus the
// winit::event_loop::EventLoopProxy parameter -- gpui_shell's own
// spawn_terminal_at has no such dependency.

use std::path::PathBuf;
use std::rc::Rc;

use gpui::Context;

use crate::app::mux::snapshot::{PaneNodeSnapshot, SplitDirSnapshot, TabSnapshot, WorkspaceSnapshot};
use crate::config::Config;
use crate::term::Terminal;

use super::panes::{next_node_id, PaneForest, PaneTree, SplitDir};
use super::workspace::Workspace;
use super::GpuiShellRoot;

impl GpuiShellRoot {
    /// Snapshot the active workspace to disk.
    pub(super) fn save_active_workspace(&self) -> anyhow::Result<()> {
        let snap = self.build_workspace_snapshot();
        crate::app::mux::snapshot::save_snapshot(&snap)
    }

    fn build_workspace_snapshot(&self) -> WorkspaceSnapshot {
        let workspace = self.workspaces.active();
        let tabs: Vec<TabSnapshot> = workspace
            .tabs
            .tabs()
            .iter()
            .enumerate()
            .map(|(i, tab)| {
                let pane_tree = workspace
                    .tab_panes
                    .get(i)
                    .map(|forest| self.snapshot_pane_node(&forest.root))
                    .unwrap_or(PaneNodeSnapshot::Leaf {
                        cwd: home_str(),
                    });
                TabSnapshot {
                    title: tab.title.clone(),
                    pane_tree,
                    accent_color: tab.accent_color,
                }
            })
            .collect();
        let saved_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        WorkspaceSnapshot {
            version: 1,
            name: workspace.name.clone(),
            saved_at,
            tabs,
        }
    }

    fn snapshot_pane_node(&self, node: &PaneTree) -> PaneNodeSnapshot {
        match node {
            PaneTree::Leaf { terminal_id } => {
                let cwd = self
                    .terminals
                    .get(terminal_id)
                    .and_then(|t| crate::term::process_cwd(t.child_pid))
                    .map(|p| p.to_string_lossy().into_owned())
                    .unwrap_or_else(home_str);
                PaneNodeSnapshot::Leaf { cwd }
            }
            PaneTree::Split {
                dir, ratio, left, right, ..
            } => PaneNodeSnapshot::Split {
                dir: match dir {
                    SplitDir::Horizontal => SplitDirSnapshot::Horizontal,
                    SplitDir::Vertical => SplitDirSnapshot::Vertical,
                },
                ratio: *ratio,
                left: Box::new(self.snapshot_pane_node(left)),
                right: Box::new(self.snapshot_pane_node(right)),
            },
        }
    }
}

fn home_str() -> String {
    dirs::home_dir()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|| "/".to_string())
}
```

Confirm `dirs` is already a dependency (used identically by the wgpu build's own `home_str` in
`src/app/mux/workspace.rs`) -- check `Cargo.toml` before assuming; if `gpui-petruterm`'s own bin
target doesn't pull it in already (it's a workspace-wide dependency in the shared library crate,
so it should), add it to this file's own `use` only, no `Cargo.toml` change expected.

- [ ] **Step 3: `workspace_snapshot.rs` -- restore**

Append to the same `impl GpuiShellRoot` block:

```rust
    /// Restore a workspace from a snapshot: creates a NEW workspace with
    /// the saved tab/pane layout. Tabs and panes are created fresh (no
    /// process state is restored -- fresh shells at the saved CWDs),
    /// matching the wgpu build's own documented behavior exactly.
    pub(super) fn restore_workspace(&mut self, snap: WorkspaceSnapshot, cx: &mut Context<Self>) {
        self.workspaces.new_workspace(snap.name.clone());
        for tab_snap in &snap.tabs {
            match self.restore_pane_tree(&tab_snap.pane_tree) {
                Ok((root, focused_terminal)) => {
                    let workspace = self.workspaces.active_mut();
                    let idx = workspace.tabs.new_tab(&tab_snap.title);
                    if let Some(color) = tab_snap.accent_color {
                        workspace.tabs.set_tab_color(idx, Some(color));
                    }
                    workspace
                        .tab_panes
                        .push(PaneForest {
                            root,
                            focused_terminal,
                        });
                }
                Err(e) => log::error!("Failed to restore tab '{}': {e}", tab_snap.title),
            }
        }
        cx.notify();
    }

    fn restore_pane_tree(&mut self, node: &PaneNodeSnapshot) -> anyhow::Result<(PaneTree, usize)> {
        match node {
            PaneNodeSnapshot::Leaf { cwd } => {
                let cwd_path = if cwd.is_empty() {
                    None
                } else {
                    Some(PathBuf::from(cwd))
                };
                let (terminal, gate) =
                    super::spawn_terminal_at(80, 24, &self.config, cwd_path)?;
                let terminal_id = self.next_terminal_id;
                self.next_terminal_id += 1;
                self.terminals.insert(terminal_id, terminal);
                self.wakeup_gates.insert(terminal_id, gate);
                self.block_managers
                    .insert(terminal_id, crate::term::BlockManager::new());
                Ok((PaneTree::Leaf { terminal_id }, terminal_id))
            }
            PaneNodeSnapshot::Split {
                dir, ratio, left, right,
            } => {
                let (left_tree, left_focused) = self.restore_pane_tree(left)?;
                let (right_tree, right_focused) = self.restore_pane_tree(right)?;
                Ok((
                    PaneTree::Split {
                        node_id: next_node_id(),
                        dir: match dir {
                            SplitDirSnapshot::Horizontal => SplitDir::Horizontal,
                            SplitDirSnapshot::Vertical => SplitDir::Vertical,
                        },
                        ratio: *ratio,
                        left: Box::new(left_tree),
                        right: Box::new(right_tree),
                    },
                    right_focused.max(left_focused), // arbitrary but deterministic: newest-created leaf wins focus
                ))
            }
        }
    }
```

`80, 24` as the initial size matches every other `spawn_terminal`/`spawn_terminal_at` call site in
this codebase (`mod.rs::new`, `actions.rs`, `leader_dispatch.rs` all hardcode the same values --
the pane-tree layout resizes real terminal dimensions on first paint regardless, same as every
split/new-tab path already does). Read `Config`'s real field for `&self.config` access at this
point in `GpuiShellRoot` to confirm the field name (`self.config: Config`, already established
throughout this whole migration).

Confirm `Workspace`'s fields (`name`, `tabs`, `tab_panes`) are all `pub` (needed for direct field
access from `workspace_snapshot.rs`, a sibling module) -- already confirmed in this plan's own
Global-Constraints research: `pub struct Workspace { pub id, pub name, pub tabs, pub tab_panes,
pub zoomed_pane }`. Confirm `WorkspaceManager::active()`/`active_mut()` exact method names against
`workspace.rs`'s real API before using them (both are used extensively elsewhere in `gpui_shell`
already, e.g. `render.rs`'s own `self.workspaces.active()` calls -- this task's usage matches the
same established pattern, not a new method).

- [ ] **Step 4: `mod.rs` -- module registration**

Add `mod workspace_snapshot;` in alphabetical order (right after `mod workspace;`, right before
`mod workspace_rename` if one exists, or wherever alphabetically correct against the real current
list -- check the file at edit time).

- [ ] **Step 5: `palette_dispatch.rs` -- wire the three palette actions**

Add `Action::SaveWorkspace | Action::OpenSavedWorkspaces | Action::RestoreWorkspace(_)` to
`gpui_shell_actions`'s `matches!` list.

Add real arms to `dispatch_palette_action`:

```rust
            Action::SaveWorkspace => {
                if let Err(e) = self.save_active_workspace() {
                    log::error!("save_active_workspace: {e}");
                }
            }
            Action::OpenSavedWorkspaces => {
                let items: Vec<crate::ui::palette::PaletteAction> =
                    crate::app::mux::snapshot::list_saved_workspaces()
                        .into_iter()
                        .map(|info| crate::ui::palette::PaletteAction {
                            name: format!(
                                "{} ({} tabs) — {}",
                                info.name, info.tab_count, info.saved_at
                            ),
                            action: crate::ui::palette::Action::RestoreWorkspace(
                                info.path.to_string_lossy().into_owned(),
                            ),
                            keybind: None,
                        })
                        .collect();
                if items.is_empty() {
                    self.palette.open();
                } else {
                    self.palette.open_with_items(items);
                }
            }
            Action::RestoreWorkspace(path) => {
                match crate::app::mux::snapshot::load_workspace(&std::path::PathBuf::from(&path)) {
                    Ok(snap) => self.restore_workspace(snap, cx),
                    Err(e) => log::error!("load_workspace: {e}"),
                }
            }
```

`dispatch_palette_action`'s own signature already includes `cx: &mut Context<Self>` (confirmed:
every other arm in this function already has `cx` in scope) -- `RestoreWorkspace`'s arm is the
first to actually need it for a `cx.notify()`-driving call; no signature change needed.

- [ ] **Step 6: Write and run the tree-conversion unit test**

`snapshot_pane_node`/`restore_pane_tree`'s conversion logic (dir/ratio mapping, leaf/split
shape) is real, pure logic once a `PaneTree` value exists in memory -- add a test to `workspace_
snapshot.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_dir_round_trips() {
        assert!(matches!(
            match SplitDir::Horizontal {
                SplitDir::Horizontal => SplitDirSnapshot::Horizontal,
                SplitDir::Vertical => SplitDirSnapshot::Vertical,
            },
            SplitDirSnapshot::Horizontal
        ));
        assert!(matches!(
            match SplitDir::Vertical {
                SplitDir::Horizontal => SplitDirSnapshot::Horizontal,
                SplitDir::Vertical => SplitDirSnapshot::Vertical,
            },
            SplitDirSnapshot::Vertical
        ));
    }
}
```

This is a deliberately minimal smoke test (the full `build`/`restore` round trip needs a live
`GpuiShellRoot` with real spawned terminals, out of scope for a unit test per this project's own
"no PTY in unit tests" convention) -- if a more meaningful pure-logic test is possible without a
live `Terminal`/`GpuiShellRoot` (e.g. testing `restore_pane_tree`'s `right_focused.max(left_
focused)` focus-selection rule against hand-built `PaneNodeSnapshot::Split` trees, since that part
needs no PTY), prefer writing that instead of the placeholder-shaped test above -- do not commit
literally this test if you find one that exercises real behavior instead.

- [ ] **Step 7: Build, test, verify**

Run: `cargo build 2>&1 | tail -100`, `cargo test --lib 2>&1 | tail -10` (233/233 or more,
depending on Step 6's final test count), `cargo fmt` then `cargo fmt --check` (clean), `cargo
clippy --all-features -- -D warnings` (clean), `./scripts/ci-local.sh` (exit 0).

Run `wc -l src/gpui_shell/mod.rs src/gpui_shell/spawn_terminal.rs src/gpui_shell/actions.rs
src/gpui_shell/leader_dispatch.rs src/gpui_shell/palette_dispatch.rs
src/gpui_shell/workspace_snapshot.rs` and note results; flag any overshoot in CONCERNS and fix it
yourself.

Dogfood note: pending manual verification (spec's §9 "Saved workspaces" checklist).

- [ ] **Step 8: Commit**

```bash
git add src/gpui_shell/workspace_snapshot.rs src/gpui_shell/spawn_terminal.rs src/gpui_shell/mod.rs src/gpui_shell/actions.rs src/gpui_shell/leader_dispatch.rs src/gpui_shell/palette_dispatch.rs
git commit -m "feat: Add saved-workspace save/restore against gpui_shell's own tree (M5c Task 5)."
```

---

## Exit Criteria

- Right-clicking a command block's row shows Copy Output / Re-run Command alongside the default
  menu; a selection shows Send to Chat too; both do the right thing.
- Typing a configured snippet trigger and pressing Tab expands it; the palette's snippet entries
  run it directly; the palette's list updates after a config hot-reload.
- Right-clicking a URL or file path shows Open Link / Copy Link instead of the default menu; both
  do the right thing.
- The palette's "Git: Switch Branch" entry opens a picker that populates async and runs `git
  checkout` on selection; the status bar's branch segment refreshes.
- "Save Workspace" writes a snapshot to disk; "Saved Workspaces" → pick one restores a new
  workspace matching the saved tab/pane layout and accent colors at fresh shells.
- `scripts/ci-local.sh` (including `cargo fmt --check`) is green and the full `cargo test --lib`
  suite passes after each task.
- This completes M5c. M5a (ACP + tool-calling) and M5b (composer extras) remain, each to get its
  own design/plan under the same M5 milestone.
