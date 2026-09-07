# gpui M4a: Command Palette Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `Leader o` opens a centered modal command palette in the gpui shell -- fuzzy-filtered action
list, keyboard and mouse both work, Enter runs the selected action, Escape closes.

**Architecture:** Reuse `crate::ui::palette::CommandPalette` directly (pure, engine-agnostic state
machine, zero wgpu coupling -- the same "used directly, never copied" relationship M3b/M3d
established for `ChatPanel`/`SkillManager`/`McpManager`). Render a dimmed-backdrop modal (structurally
like M3d's `InfoOverlay`) containing an M3a `TextInput` query field plus a scrollable result list
(M3d's sidebar-sections `.id()` + `.overflow_y_scroll()` + `impl IntoElement` pattern). A persistent
`TextInput` entity's own `Submit`/`Cancel` actions are consumed internally by gpui's action-dispatch
before they ever reach the outer key-guard bubble listener (verified against `text_input/mod.rs`'s own
`register_key_bindings`), so Enter/Escape route through a `cx.subscribe` callback -- which gpui hands
no `Window` -- exactly the constraint M3b's chat-panel `/q` close already worked around: the callback
stashes the confirmed `Action` in a new `pending_palette_action` field, and `render()`'s own top (which
does have `Window`) drains and dispatches it every frame.

**Tech Stack:** Rust, gpui 0.2.2 (existing `gpui_shell` conventions only -- no new crates).

**Spec:** `docs/superpowers/specs/2026-09-07-gpui-m4-remaining-surfaces-design.md` (§3 M4a design, §5
dispatch table, §7 deferred list, §8 M4a manual-test checklist, §9 Global Constraints).

## Global Constraints

- 400-line module limit. `input.rs` is currently 393 lines and `render.rs` 337 -- both close enough
  that this plan puts all new logic (render, key-guard, dispatch) in new files (`palette.rs`,
  `palette_dispatch.rs`), touching `input.rs`/`render.rs` only for small, fixed-size wiring calls, the
  same lesson M3d's `sidebar_nav.rs`/`render_sidebar.rs` splits learned the hard way (both files
  crossed 400 mid-milestone and needed a post-hoc split).
- `scripts/ci-local.sh` must stay green after every task (clippy `-D warnings` included).
- Commit format: `type: Message.` per `AGENTS.md`.
- Key/focus guards key on real focus (`is_focused(window)`), never on visibility/open state. The
  palette's query `TextInput` is a real, focus-grabbing widget -- unlike M3d's `InfoOverlay`, it does
  NOT get a visibility-keyed exception. Its keyboard guard (Up/Down, Task 2) is keyed on
  `palette_query.focus_handle(cx).is_focused(window)`, the same pattern every other `TextInput`
  consumer in this codebase uses (tab rename, workspace rename, chat composer).
- Tests for logic only -- no painting/layout/hit-testing tests (dogfooded; GPU windows can't be driven
  interactively from the agent sandbox -- confirmed this session: screenshots of a launched instance
  work, synthetic keyboard/mouse input does not).
- `#[allow(dead_code)]` (narrowly scoped, comment naming the removing task) for anything built ahead
  of its first real caller.

---

## Task 1: Wire `CommandPalette` state + `Leader o` keybind (plumbing + event round-trip)

**Purpose:** Get the full state machine wired end-to-end -- opening, the `Submit`/`Cancel` event
round-trip via `cx.subscribe`, and the `render()` drain hook -- with no visible UI yet. Dogfoodable
only in the sense M3d Task 1 was ("app starts exactly as before"); Task 2 is the first task that
produces anything on screen.

**Files:**
- Modify: `src/gpui_shell/mod.rs`
- Modify: `src/gpui_shell/leader.rs`
- Modify: `src/gpui_shell/leader_dispatch.rs`

**Interfaces:**
- Consumes: `crate::ui::palette::CommandPalette` (`new(&Config) -> Self`, `open_with_items(Vec<PaletteAction>)`, `confirm(&mut self) -> Option<Action>`, `close(&mut self)`), `crate::ui::palette::Action`, `super::text_input::{TextInput, TextInputEvent}`.
- Produces: `GpuiShellRoot::{palette: CommandPalette, palette_query: Entity<TextInput>, pending_palette_action: Option<crate::ui::palette::Action>}`; `LeaderAction::OpenCommandPalette`. Task 2 is the first reader of `palette_query`'s render output and the first task to populate `pending_palette_action` with anything a real dispatcher consumes (this task's own drain hook exists but has nothing to dispatch yet, since nothing can focus `palette_query` from a real UI until Task 2).

- [ ] **Step 1: `leader.rs` -- new variant, `TryFrom` arm, seed check, test fixes**

In `src/gpui_shell/leader.rs`, add `OpenCommandPalette` to the `LeaderAction` enum, right after `ToggleWorkspaceSidebar`:

```rust
    ToggleWorkspaceSidebar,
    OpenCommandPalette,
}
```

Add a matching arm to `TryFrom<&str>`, right after `"ToggleWorkspaceSidebar" => Ok(LeaderAction::ToggleWorkspaceSidebar),`:

```rust
            "ToggleWorkspaceSidebar" => Ok(LeaderAction::ToggleWorkspaceSidebar),
            "CommandPalette" => Ok(LeaderAction::OpenCommandPalette),
```

(`"CommandPalette"` -- not `"OpenCommandPalette"` -- because that's the literal string
`config/default/keybinds.lua`'s `petruterm.action.CommandPalette` produces, matching this project's
own `{ mods = "LEADER", key = "o", action = petruterm.action.CommandPalette }` entry exactly. `Leader
o` is real, existing config, not a new binding this task invents.)

`build_leader_map` needs no change: `"o"` isn't seeded like `"z"`/`"w"`/`"s"` are, because -- unlike
those three -- it already has a real entry in `config/default/keybinds.lua`, so `TryFrom`'s new arm is
all that's needed for `build_leader_map`'s existing loop to pick it up.

Two existing tests need updating because `"CommandPalette"` was, until this step, a deliberately
unparseable string. Replace `build_leader_map_matches_default_keybinds_lua`'s `bindings` vec and its
assertions (only the parts shown change -- everything else in the test stays as-is):

```rust
        let bindings = vec![
            kb("c", "NewTab"),
            kb("&", "CloseTab"),
            kb("n", "NextTab"),
            kb("b", "PrevTab"),
            kb(",", "RenameTab"),
            kb("%", "SplitHorizontal"),
            kb("\"", "SplitVertical"),
            kb("x", "ClosePane"),
            kb("h", "FocusPaneLeft"),
            kb("j", "FocusPaneDown"),
            kb("k", "FocusPaneUp"),
            kb("l", "FocusPaneRight"),
            kb("o", "CommandPalette"),
        ];
```

and add one assertion, right after the existing `assert_eq!(map.get("s"), Some(&LeaderAction::ToggleWorkspaceSidebar));` line:

```rust
        assert_eq!(map.get("s"), Some(&LeaderAction::ToggleWorkspaceSidebar));
        assert_eq!(map.get("o"), Some(&LeaderAction::OpenCommandPalette));
    }
```

Replace `unparseable_action_strings_are_skipped` in full (it used `"CommandPalette"` as its example of
an unparseable string, which is no longer true):

```rust
    #[test]
    fn unparseable_action_strings_are_skipped() {
        let bindings = vec![kb("o", "SomeUnknownFutureAction")];
        let map = build_leader_map(&bindings);
        assert_eq!(map.get("o"), None);
        assert_eq!(map.len(), 3); // just the seeded "z", "w", and "s"
    }
}
```

- [ ] **Step 2: `mod.rs` -- imports, fields, construction**

Add to the imports (alongside the existing `use crate::llm::mcp::manager::McpManager;` block):

```rust
use crate::ui::palette::{Action, CommandPalette};
```

Add to the `GpuiShellRoot` struct, right after the `info_overlay` field:

```rust
    /// The read-only content popup every sidebar row's activation opens
    /// (Task 4) -- see `info_overlay.rs`'s own doc comment for why it's
    /// modal and why that makes its `is_visible()` guard (`input.rs`)
    /// correct rather than a shortcut.
    info_overlay: info_overlay::InfoOverlay,
    /// The command palette's own state (query, filtered results, selected
    /// index, visibility) -- `crate::ui::palette::CommandPalette`, used
    /// directly rather than copied, the same relationship M3b/M3d
    /// established for `ChatPanel`/`SkillManager`/`McpManager`. `gpui_shell`
    /// always opens it via `open_with_items(..)` with its own filtered list
    /// (`palette_dispatch.rs`, Task 3) rather than `open()`'s unfiltered
    /// `all_actions` -- several of the wgpu build's own actions have no
    /// `gpui_shell` equivalent yet (see the M4 spec's §7 deferred list).
    palette: CommandPalette,
    /// The palette's query input -- a single persistent `TextInput` entity
    /// (unlike tab/workspace rename, which build a fresh one per edit; the
    /// palette opens/closes far more often, so its content is cleared and
    /// refocused on each open instead of rebuilding the widget). Its own
    /// `Submit`/`Cancel` actions are bound inside `TextInput`'s own
    /// `"TextInput"` key context (`text_input/mod.rs`'s
    /// `register_key_bindings`) and consumed by gpui's action-dispatch
    /// before they ever reach `on_key_down`'s bubble listener -- the
    /// `cx.subscribe` callback below (Step 4) is how this struct reacts to
    /// them instead.
    palette_query: gpui::Entity<text_input::TextInput>,
    /// Set by the `palette_query` subscription (Step 4) when `Submit` fires
    /// and `CommandPalette::confirm()` returns an action to run;
    /// `render()`'s own top (`render.rs`, Task 2) drains and dispatches it
    /// every frame. Needed because `cx.subscribe`'s callback is handed no
    /// `Window` -- the same constraint M3b's chat-panel `/q` close already
    /// worked around (`render.rs`'s own doc comment on its focus-reclaim
    /// guard has the full precedent).
    pending_palette_action: Option<Action>,
```

Replace the tail of `new()` -- from the existing `let ai_block = ai_block::AiBlockView::new(cx, &config);` line through the `Self { .. }` literal's closing brace -- with this version (only the new lines shown; the code AROUND the shown snippets is unchanged from what it already is: still the same skill/steering/MCP-loading block, `workspaces.active_mut()...push(...)` line, and the rest of the `Self { .. }` fields in their existing order):

```rust
        let chat = chat_panel::ChatPanelView::new(cx, &config);
        let ai_block = ai_block::AiBlockView::new(cx, &config);
        let palette = CommandPalette::new(&config);
        let palette_query = cx.new(|cx| {
            text_input::TextInput::new(cx, &config.colors, "", "Type a command...")
        });
        // Set up once, for the widget's whole lifetime -- `palette_query` is
        // a persistent entity (Step 2's own doc comment), not rebuilt per
        // open like a rename editor, so this subscription only needs
        // creating once too.
        cx.subscribe(&palette_query, |this, _input, event, cx| {
            match event {
                text_input::TextInputEvent::Submit => {
                    if let Some(action) = this.palette.confirm() {
                        this.pending_palette_action = Some(action);
                    }
                }
                text_input::TextInputEvent::Cancel => {
                    this.palette.close();
                }
            }
            cx.notify();
        })
        .detach();
```

(This block goes right after the existing `let ai_block = ...` line, before the `// Same construction
pattern as the wgpu app's own tokio_rt field...` comment that starts the `tokio_rt` binding.)

Add three fields to the `Self { .. }` literal, right after `info_overlay: info_overlay::InfoOverlay::new(),`:

```rust
            info_overlay: info_overlay::InfoOverlay::new(),
            palette,
            palette_query,
            pending_palette_action: None,
```

- [ ] **Step 3: `leader_dispatch.rs` -- the `OpenCommandPalette` arm**

Add a new arm to `dispatch_leader_action`'s match, right after the `LeaderAction::ToggleWorkspaceSidebar => { .. }` arm:

```rust
            LeaderAction::OpenCommandPalette => {
                self.palette_query
                    .update(cx, |input, cx| input.set_content("", cx));
                self.palette.open_with_items(Vec::new());
                self.palette_query.focus_handle(cx).focus(window);
            }
```

(An empty `Vec::new()` is correct for this task -- Task 2 replaces it with a real interim list. With
no render yet, `open_with_items` here has no visible effect either way; this arm exists so the match
stays exhaustive and the state machine transitions correctly once Task 2 adds a render.)

- [ ] **Step 4: `render.rs` -- the drain hook (dispatches nothing yet)**

Add this as the very first statement in `render()`'s body, before the existing `// Skipped while a
child owns focus...` comment and its `if self.tab_rename.is_none() && ...` guard:

```rust
        // Runs before the focus-reclaim guard below: dispatching a
        // confirmed palette action (Task 3's `dispatch_palette_action`) can
        // itself change `self.palette.is_visible()` this same frame, and
        // the guard needs to see that change to correctly reclaim focus
        // for the terminal without a one-frame lag. `CommandPalette::
        // confirm()` (called from `palette_query`'s `cx.subscribe`
        // callback, `mod.rs`) already closes the palette itself before
        // this ever fires, so no explicit close call is needed here.
        if let Some(_action) = self.pending_palette_action.take() {
            // Task 3 replaces this with a real
            // `self.dispatch_palette_action(action, window, cx);` call.
            // Nothing to dispatch to yet -- Task 2 is the first task with
            // any UI able to produce a confirmed action at all.
            window.focus(&self.focus_handle);
        }

```

- [ ] **Step 5: Build, test, verify**

Run: `cargo build 2>&1 | tail -60` -- zero errors, zero warnings.

Run: `cargo test --lib 2>&1 | tail -10` -- 230/230 passing, plus the two updated `leader::tests`
still passing (`build_leader_map_matches_default_keybinds_lua`, `unparseable_action_strings_are_skipped`).

Run: `./scripts/ci-local.sh` -- must exit 0.

Dogfood: launch the app. `Leader o` should be silently inert (no crash, nothing visible) -- there is
no render yet. Confirms this task's plumbing doesn't break startup or existing keybinds.

- [ ] **Step 6: Commit**

```bash
git add src/gpui_shell/mod.rs src/gpui_shell/leader.rs src/gpui_shell/leader_dispatch.rs src/gpui_shell/render.rs
git commit -m "feat: Wire CommandPalette state and the Leader o keybind (M4a Task 1)."
```

---

## Task 2: `palette.rs` -- render, focus guard, and an interim 3-action dispatch

**Purpose:** The first task with anything visible: opening the palette shows a real modal with a
working query field, live-filtered list, keyboard nav, and mouse clicks -- exercised against a small,
real (not placeholder-text) 3-action subset (`NewTab`, `CloseTab`, `Quit`) so this task's own dogfood
step is meaningful end-to-end. Task 3 replaces the 3-action subset with the full filtered list.

**Files:**
- Create: `src/gpui_shell/palette.rs`
- Modify: `src/gpui_shell/mod.rs` (register module, wire the interim item list into Task 1's dispatch arm)
- Modify: `src/gpui_shell/leader_dispatch.rs` (point `OpenCommandPalette` at the interim list)
- Modify: `src/gpui_shell/input.rs` (one guard call)
- Modify: `src/gpui_shell/render.rs` (wire into the tree, extend the focus-reclaim guard, replace Task 1's drain-hook stub)

**Interfaces:**
- Consumes: `CommandPalette::{is_visible, query, results, selected, type_char, backspace, select_up, select_down}` (all pre-existing on the reused state machine), `TextInput`/`InfoOverlay`'s established modal-backdrop render shape, `super::pane_view::to_rgba`, `super::font_state`.
- Produces: `palette::render_command_palette(palette: &CommandPalette, query_input: &Entity<TextInput>, colors: &ColorScheme) -> impl IntoElement`; `GpuiShellRoot::{handle_palette_focused_key(&mut self, event, window, cx), palette_query_focused(&self, window, cx) -> bool, dispatch_palette_action(&mut self, action: Action, window, cx)}` (this task's own 3-arm version; Task 3 replaces its body).

- [ ] **Step 1: Write `src/gpui_shell/palette.rs`**

```rust
// gpui chrome migration (M4a Task 2): the command palette's render tree and
// its own keyboard guard. State (`CommandPalette`) is reused directly from
// `crate::ui::palette` -- see `mod.rs`'s own doc comment on the `palette`
// field for why. This file owns everything that touches gpui: rendering,
// the Up/Down key guard (Enter/Escape route through `palette_query`'s own
// `Submit`/`Cancel` actions and the `cx.subscribe` callback in `mod.rs`
// instead -- see that subscribe's own doc comment), and (this task only)
// a small interim action dispatch. Task 3 (`palette_dispatch.rs`) replaces
// the interim dispatch and the interim item list with the real, full ones.

use gpui::{
    div, prelude::*, px, rgba, App, Context, Entity, FontWeight, KeyDownEvent, MouseButton,
    MouseDownEvent, Window,
};

use crate::config::schema::ColorScheme;
use crate::ui::palette::{Action, CommandPalette, PaletteAction};

use super::font_state;
use super::leader::LeaderAction;
use super::pane_view::to_rgba;
use super::text_input::TextInput;
use super::GpuiShellRoot;

/// A small, real (not placeholder) subset of `Action` this task can already
/// dispatch end-to-end, so Task 2's own dogfood step (open, type, arrow,
/// Enter, Escape) exercises the full round-trip rather than an empty list.
/// Task 3 removes this function entirely, replacing every call site with
/// its own filtered `built_in_actions`-derived list.
pub(super) fn interim_actions() -> Vec<PaletteAction> {
    vec![
        PaletteAction {
            name: "New Tab".to_string(),
            action: Action::NewTab,
            keybind: Some("^F c".into()),
        },
        PaletteAction {
            name: "Close Tab".to_string(),
            action: Action::CloseTab,
            keybind: Some("^F &".into()),
        },
        PaletteAction {
            name: "Quit".to_string(),
            action: Action::Quit,
            keybind: Some("Cmd+Q".into()),
        },
    ]
}

/// Build the palette's `div()` tree: a dimmed, window-covering backdrop
/// (`InfoOverlay`'s own shape, `info_overlay.rs`) centered on a fixed-size
/// content box holding the query field and the scrollable result list.
pub fn render_command_palette(
    palette: &CommandPalette,
    query_input: &Entity<TextInput>,
    colors: &ColorScheme,
) -> impl IntoElement {
    let selected = palette.selected;
    let rows: Vec<_> = palette
        .results
        .iter()
        .enumerate()
        .map(|(idx, item)| {
            let is_selected = idx == selected;
            let row = div()
                .flex()
                .flex_row()
                .items_center()
                .justify_between()
                .px_2()
                .py_1()
                .child(item.name.clone())
                .when_some(item.keybind.clone(), |el, kb| {
                    el.child(div().text_size(px(11.0)).child(kb))
                });
            if is_selected {
                row.bg(to_rgba(colors.ui_surface_active))
                    .text_color(to_rgba(colors.foreground))
            } else {
                row.text_color(to_rgba(colors.ui_muted))
            }
        })
        .collect();

    div()
        .id("command-palette-backdrop")
        .absolute()
        .inset_0()
        .flex()
        .items_center()
        .justify_center()
        .bg(rgba(0x0000_0099))
        .on_mouse_down(
            MouseButton::Left,
            |_: &MouseDownEvent, _window: &mut Window, cx: &mut App| {
                cx.stop_propagation();
            },
        )
        .child(
            div()
                .id("command-palette-content")
                .flex()
                .flex_col()
                .w(px(480.0))
                .h(px(360.0))
                .bg(to_rgba(colors.ui_surface))
                .border_1()
                .border_color(to_rgba(colors.ui_border))
                .on_mouse_down(
                    MouseButton::Left,
                    |_: &MouseDownEvent, _window: &mut Window, cx: &mut App| {
                        cx.stop_propagation();
                    },
                )
                .child(
                    div()
                        .px_2()
                        .py_1()
                        .border_b_1()
                        .border_color(to_rgba(colors.ui_border))
                        .font_family(font_state::font_family())
                        .font_weight(FontWeight::BOLD)
                        .child(query_input.clone()),
                )
                .child(
                    div()
                        .id("command-palette-results")
                        .flex()
                        .flex_col()
                        .flex_1()
                        .min_h_0()
                        .overflow_y_scroll()
                        .font_family(font_state::font_family())
                        .text_size(px(font_state::font_size()))
                        .children(rows),
                ),
        )
}

impl GpuiShellRoot {
    /// True while the palette's query field genuinely holds keyboard focus
    /// -- same shape as `ChatPanelView::composer_focused`
    /// (`chat_panel/mod.rs`), used both by `input.rs`'s Up/Down guard and
    /// by `render()`'s own focus-reclaim guard (`render.rs`).
    pub(super) fn palette_query_focused(&self, window: &Window, cx: &App) -> bool {
        self.palette_query.focus_handle(cx).is_focused(window)
    }

    /// Up/Down move the highlighted result while the query field holds
    /// focus. Enter and Escape are NOT handled here -- `TextInput`'s own
    /// `"TextInput"`-scoped key bindings (`text_input/mod.rs`) consume
    /// those as its own `Submit`/`Cancel` actions before this bubble
    /// listener ever sees them; `mod.rs`'s `cx.subscribe` callback on
    /// `palette_query` is where this struct reacts to them instead.
    pub(super) fn handle_palette_focused_key(
        &mut self,
        event: &KeyDownEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event.keystroke.key.as_str() {
            "down" => self.palette.select_down(),
            "up" => self.palette.select_up(),
            _ => {}
        }
        cx.notify();
    }

    /// Run one confirmed palette action. This task's version covers only
    /// the 3-item `interim_actions()` subset; Task 3 replaces this entire
    /// function body with the full, spec-table-driven dispatch
    /// (`palette_dispatch.rs`).
    pub(super) fn dispatch_palette_action(
        &mut self,
        action: Action,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match action {
            Action::NewTab => self.dispatch_leader_action(LeaderAction::NewTab, window, cx),
            Action::CloseTab => self.dispatch_leader_action(LeaderAction::CloseTab, window, cx),
            Action::Quit => cx.quit(),
            _ => {}
        }
    }
}
```

- [ ] **Step 2: Register the module**

In `src/gpui_shell/mod.rs`, add `mod palette;` to the module list, alphabetically after `mod pane_view;` and before `mod panes;`.

- [ ] **Step 3: Point Task 1's dispatch arm at the interim list**

In `src/gpui_shell/leader_dispatch.rs`, replace:

```rust
            LeaderAction::OpenCommandPalette => {
                self.palette_query
                    .update(cx, |input, cx| input.set_content("", cx));
                self.palette.open_with_items(Vec::new());
                self.palette_query.focus_handle(cx).focus(window);
            }
```

with:

```rust
            LeaderAction::OpenCommandPalette => {
                self.palette_query
                    .update(cx, |input, cx| input.set_content("", cx));
                self.palette
                    .open_with_items(super::palette::interim_actions());
                self.palette_query.focus_handle(cx).focus(window);
            }
```

- [ ] **Step 4: `input.rs` -- the Up/Down guard call**

Add this guard as the very first statement inside `on_key_down`, before the existing `InfoOverlay`
guard (`if self.info_overlay.is_visible() { .. }`) -- the palette sits on top of everything, including
`InfoOverlay`, since only one modal can plausibly be open at a time in this app's own flow, but this
ordering costs nothing either way and keeps "most recently added modal checked first" as a simple,
consistent rule:

```rust
        // The palette's query field intercepts Up/Down directly (Enter/
        // Escape are TextInput's own bound actions, handled via
        // `palette_query`'s `cx.subscribe` callback in `mod.rs` instead --
        // see `palette.rs`'s own doc comment on `handle_palette_focused_key`).
        // Keyed on real focus, not `self.palette.is_visible()`: unlike
        // `InfoOverlay`, the palette's query field is a genuine focus-
        // grabbing `TextInput`, so it follows the same guard shape every
        // other `TextInput` consumer in this codebase uses.
        if self.palette_query_focused(window, cx) {
            self.handle_palette_focused_key(event, window, cx);
            return;
        }

```

- [ ] **Step 5: `render.rs` -- wire into the tree, extend the focus-reclaim guard, replace the drain stub**

Add `use super::palette;` to the imports, alongside the existing `use super::sidebar;`.

Replace Task 1's drain-hook stub:

```rust
        if let Some(_action) = self.pending_palette_action.take() {
            // Task 3 replaces this with a real
            // `self.dispatch_palette_action(action, window, cx);` call.
            // Nothing to dispatch to yet -- Task 2 is the first task with
            // any UI able to produce a confirmed action at all.
            window.focus(&self.focus_handle);
        }
```

with:

```rust
        if let Some(action) = self.pending_palette_action.take() {
            self.dispatch_palette_action(action, window, cx);
            window.focus(&self.focus_handle);
        }
```

Extend the top-of-function focus-reclaim guard -- add a fourth condition, right after the existing
`ai_block` one:

```rust
        if self.tab_rename.is_none()
            && self.workspace_rename.is_none()
            && (!self.chat.is_visible() || !self.chat.composer_focused(window, cx))
            && (!self.ai_block.is_visible() || !self.ai_block.composer_focused(window, cx))
            && (!self.palette.is_visible() || !self.palette_query_focused(window, cx))
        {
            window.focus(&self.focus_handle);
        }
```

Add the palette to the root div's child chain, right after the existing `InfoOverlay` `.when(...)`
block (the function's final `.when(self.info_overlay.is_visible(), ...)` call, currently the last
thing before the closing `}` of `render()`):

```rust
            .when(self.info_overlay.is_visible(), |el| {
                el.child(info_overlay::render_info_overlay(
                    &self.info_overlay,
                    &self.config.colors,
                ))
            })
            .when(self.palette.is_visible(), |el| {
                el.child(palette::render_command_palette(
                    &self.palette,
                    &self.palette_query,
                    &self.config.colors,
                ))
            })
    }
}
```

(This replaces the function's existing closing `.when_some(status_bar_row, ...)`-then-`InfoOverlay`
tail -- only the new `.when(self.palette.is_visible(), ...)` block is added, appended after what's
already there.)

- [ ] **Step 6: Build, test, verify**

Run: `cargo build 2>&1 | tail -80`, `cargo test --lib 2>&1 | tail -10` (230/230, no new tests -- no
new pure-logic; every function this task adds is UI/wiring exercised by dogfood), `./scripts/ci-local.sh`
(exit 0).

Run `wc -l src/gpui_shell/input.rs src/gpui_shell/render.rs src/gpui_shell/palette.rs` and confirm all
three stay under 400 -- if `input.rs` or `render.rs` crossed it, note the exact line count in your
report; do not attempt a further split yourself, that's a controller decision.

Dogfood: `Leader o` opens a centered modal palette, focused on the query field immediately (confirm by
typing right away). Typing "new" filters to "New Tab" (fuzzy match). Up/Down move the highlighted row,
wrapping at both ends. Enter on "New Tab" opens a new tab and closes the palette. Reopen, Escape closes
without running anything. Reopen, click a row directly -- opens that row's `TextInput`... no, clicking
a row does NOT run it yet (this task's render has no `on_mouse_down` on each row -- confirm this is
the case, not a bug: mouse-confirm is deliberately deferred, since `CommandPalette` has no
"select-then-confirm-by-index" method a mouse click alone can drive without first calling something
equivalent to arrow-key selection -- Task 3's own dogfood step is the right place to decide whether to
add row-click support, once the real action list exists to make it worth testing). Click the dimmed
backdrop -- palette stays open (only Escape/Enter close it, matching `InfoOverlay`'s own backdrop
behavior). Click into the terminal while... you can't: the palette is a true modal, backdrop blocks
all clicks by design (confirm the backdrop click really is swallowed -- nothing behind it reacts).

- [ ] **Step 7: Commit**

```bash
git add src/gpui_shell/palette.rs src/gpui_shell/mod.rs src/gpui_shell/leader_dispatch.rs src/gpui_shell/input.rs src/gpui_shell/render.rs
git commit -m "feat: Render the command palette with a 3-action interim dispatch (M4a Task 2)."
```

---

## Task 3: `palette_dispatch.rs` -- the full filtered action list + dispatch table

**Purpose:** Replace Task 2's 3-action interim subset with the real thing: every `Action` variant the
M4 spec's §3 table says `gpui_shell` can already support, filtered so nothing unsupported is ever shown
or dispatchable.

**Files:**
- Create: `src/gpui_shell/palette_dispatch.rs`
- Modify: `src/gpui_shell/palette.rs` (remove `interim_actions`, remove the interim `dispatch_palette_action`)
- Modify: `src/gpui_shell/leader_dispatch.rs` (point at the real list builder)
- Modify: `src/gpui_shell/mod.rs` (register the module)

**Interfaces:**
- Consumes: `crate::ui::palette::actions::built_in_actions(config) -> Vec<PaletteAction>` (existing, unfiltered), `LeaderAction` (M2/M3c), `super::panes::FocusDir` (M2), `crate::ui::panes::FocusDir` (wgpu-native, what `Action::FocusPane` carries), `Terminal::clear_screen_and_scrollback` -- not used this task, listed for completeness against the spec's §5 table (that's M4c's job).
- Produces: `palette_dispatch::gpui_shell_actions(config: &Config) -> Vec<PaletteAction>`; `GpuiShellRoot::dispatch_palette_action`'s real body (same signature Task 2 already established, only the implementation moves and grows).

- [ ] **Step 1: Write `src/gpui_shell/palette_dispatch.rs`**

```rust
// gpui chrome migration (M4a Task 3): the command palette's real action
// list and dispatch table -- replaces Task 2's 3-item `interim_actions`.
// Every `Action` variant NOT listed in either function here is one this
// milestone's own spec (docs/superpowers/specs/2026-09-07-gpui-m4-
// remaining-surfaces-design.md, §7) explicitly defers: no gpui_shell
// equivalent exists yet (snippets, saved workspaces, git-branch picker,
// theme picker, M3b's already-deferred AI actions, command-block/hover-
// link-dependent actions). `gpui_shell_actions` and `dispatch_
// palette_action` below are two views of the same "which actions does
// gpui_shell support" boundary and must be kept in sync by construction --
// every variant filtered IN here has a real arm below, and vice versa.

use gpui::{Context, Window};

use crate::config::Config;
use crate::ui::palette::actions::built_in_actions;
use crate::ui::palette::{Action, PaletteAction};

use super::leader::LeaderAction;
use super::GpuiShellRoot;

/// Build the palette's item list for `gpui_shell`: the wgpu build's own
/// `built_in_actions(config)`, filtered down to variants this milestone's
/// `dispatch_palette_action` actually handles. Called fresh on every open
/// (matches `CommandPalette::open()`'s own "rebuild from `all_actions`"
/// behavior for the has-a-dispatch-target case).
pub(super) fn gpui_shell_actions(config: &Config) -> Vec<PaletteAction> {
    built_in_actions(config)
        .into_iter()
        .filter(|item| {
            matches!(
                item.action,
                Action::NewTab
                    | Action::CloseTab
                    | Action::NextTab
                    | Action::PrevTab
                    | Action::RenameTab
                    | Action::NewWorkspace
                    | Action::CloseWorkspace
                    | Action::RenameWorkspace
                    | Action::NextWorkspace
                    | Action::PrevWorkspace
                    | Action::SplitHorizontal
                    | Action::SplitVertical
                    | Action::ClosePane
                    | Action::ZoomPane
                    | Action::FocusPane(_)
                    | Action::ToggleAiPanel
                    | Action::FocusAiPanel
                    | Action::ToggleFullscreen
                    | Action::Quit
                    | Action::ToggleStatusBar
                    | Action::OpenConfigFile
                    | Action::OpenConfigFolder
                    | Action::ReloadConfig
                    | Action::SwitchToTab(_)
            )
        })
        .collect()
}

/// Convert the wgpu-native `crate::ui::panes::FocusDir` (what `Action::
/// FocusPane` carries) to `gpui_shell`'s own, structurally identical but
/// separately defined, `panes::FocusDir` (M2) -- the two are parallel
/// types, not the same one, matching this codebase's established "mirror,
/// don't wrap" relationship for anything ported from `Mux`/`src/ui/`.
fn convert_focus_dir(dir: crate::ui::panes::FocusDir) -> super::panes::FocusDir {
    match dir {
        crate::ui::panes::FocusDir::Left => super::panes::FocusDir::Left,
        crate::ui::panes::FocusDir::Right => super::panes::FocusDir::Right,
        crate::ui::panes::FocusDir::Up => super::panes::FocusDir::Up,
        crate::ui::panes::FocusDir::Down => super::panes::FocusDir::Down,
    }
}

impl GpuiShellRoot {
    /// Run one confirmed palette action -- the real dispatch table, per the
    /// M4 spec's §3 mapping. Replaces Task 2's 3-arm interim version.
    pub(super) fn dispatch_palette_action(
        &mut self,
        action: Action,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match action {
            Action::NewTab => self.dispatch_leader_action(LeaderAction::NewTab, window, cx),
            Action::CloseTab => self.dispatch_leader_action(LeaderAction::CloseTab, window, cx),
            Action::NextTab => self.dispatch_leader_action(LeaderAction::NextTab, window, cx),
            Action::PrevTab => self.dispatch_leader_action(LeaderAction::PrevTab, window, cx),
            Action::RenameTab => self.dispatch_leader_action(LeaderAction::RenameTab, window, cx),
            Action::NewWorkspace => {
                self.dispatch_leader_action(LeaderAction::NewWorkspace, window, cx)
            }
            Action::CloseWorkspace => {
                self.dispatch_leader_action(LeaderAction::CloseWorkspace, window, cx)
            }
            Action::RenameWorkspace => {
                self.dispatch_leader_action(LeaderAction::RenameWorkspace, window, cx)
            }
            Action::NextWorkspace => {
                self.dispatch_leader_action(LeaderAction::NextWorkspace, window, cx)
            }
            Action::PrevWorkspace => {
                self.dispatch_leader_action(LeaderAction::PrevWorkspace, window, cx)
            }
            Action::SplitHorizontal => {
                self.dispatch_leader_action(LeaderAction::SplitHorizontal, window, cx)
            }
            Action::SplitVertical => {
                self.dispatch_leader_action(LeaderAction::SplitVertical, window, cx)
            }
            Action::ClosePane => self.dispatch_leader_action(LeaderAction::ClosePane, window, cx),
            Action::ZoomPane => self.dispatch_leader_action(LeaderAction::ZoomPane, window, cx),
            Action::FocusPane(dir) => self.dispatch_leader_action(
                LeaderAction::FocusPane(convert_focus_dir(dir)),
                window,
                cx,
            ),
            Action::ToggleAiPanel => {
                self.dispatch_leader_action(LeaderAction::ToggleAiPanel, window, cx)
            }
            // `ChatPanelView` has no separate "focus without toggling
            // closed" entry point (confirmed: no `focus_composer` method
            // exists) -- `FocusAiPanel` falls back to the same behavior as
            // `ToggleAiPanel` itself, which already opens-and-focuses when
            // closed via `self.chat.toggle` (`leader_dispatch.rs`'s own
            // `ToggleAiPanel` arm). Not a perfect "focus without toggle"
            // semantic if the panel is already open, but inventing a new
            // entry point is out of this task's scope.
            Action::FocusAiPanel => {
                self.dispatch_leader_action(LeaderAction::ToggleAiPanel, window, cx)
            }
            Action::ToggleFullscreen => window.toggle_fullscreen(),
            Action::Quit => cx.quit(),
            Action::ToggleStatusBar => {
                self.config.status_bar.enabled = !self.config.status_bar.enabled;
            }
            Action::OpenConfigFile => {
                let _ = std::process::Command::new("open")
                    .arg(crate::config::config_path())
                    .spawn();
            }
            Action::OpenConfigFolder => {
                let _ = std::process::Command::new("open")
                    .arg(crate::config::config_dir())
                    .spawn();
            }
            Action::ReloadConfig => {
                if let Ok((new_config, _lua)) = crate::config::reload() {
                    *super::config_watch::PENDING_CONFIG_RELOAD.lock().unwrap() =
                        Some(new_config);
                    super::config_watch::CONFIG_CHANGED
                        .store(true, std::sync::atomic::Ordering::Release);
                }
            }
            Action::SwitchToTab(n) => {
                if self.workspaces.active_mut().tabs.switch_to_index(n) {
                    cx.notify();
                }
            }
            // Every other variant is filtered out of `gpui_shell_actions`
            // (see its own doc comment) -- unreachable in practice, kept as
            // an explicit no-op rather than a `panic!`/`unreachable!()`
            // since a stale `CommandPalette::results` entry surviving a
            // hot-reload race is a cosmetic miss, not a crash-worthy one.
            _ => {}
        }
    }
}
```

- [ ] **Step 2: Remove Task 2's interim code from `palette.rs`**

Delete `interim_actions()` in full (the whole function, including its doc comment) from
`src/gpui_shell/palette.rs`.

Delete the `impl GpuiShellRoot` block's `dispatch_palette_action` method in full from
`src/gpui_shell/palette.rs` (the real version now lives in `palette_dispatch.rs`, Step 1 above --
having it defined in two places would be a duplicate-definition compile error).

- [ ] **Step 3: Register the module, point the open call at the real list**

In `src/gpui_shell/mod.rs`, add `mod palette_dispatch;` to the module list, alphabetically after `mod palette;` and before `mod panes;`.

In `src/gpui_shell/leader_dispatch.rs`, replace:

```rust
                self.palette
                    .open_with_items(super::palette::interim_actions());
```

with:

```rust
                self.palette
                    .open_with_items(super::palette_dispatch::gpui_shell_actions(&self.config));
```

- [ ] **Step 4: Build, test, verify**

Run: `cargo build 2>&1 | tail -100`, `cargo test --lib 2>&1 | tail -10` (230/230, unchanged -- no new pure-logic tests this task
either, per this project's "no painting/hit-testing tests" convention: `gpui_shell_actions`'s filter
and `dispatch_palette_action`'s match are both exercised by dogfood, not unit tests, since correctness
here means "does the right UI action happen," not an isolable pure function), `cargo clippy
--all-features -- -D warnings` (clean), `./scripts/ci-local.sh` (exit 0).

Run `wc -l src/gpui_shell/*.rs | sort -n | tail -10` and confirm nothing crossed 400; note exact counts
in your report if anything did, do not split it yourself.

Dogfood, in order (reproducing the M4 spec's §8 M4a checklist):
1. `Leader o` opens the palette, focuses the query field immediately (type without clicking first).
2. Typing filters the list live; try a fuzzy (non-prefix) query like "ntb" against "New Tab" -- it
   should still match, confirming this is real fuzzy matching, not `starts_with`.
3. Up/Down move the highlighted row, wrapping at both ends.
4. Enter on each of these, confirming each does the right thing (not just "doesn't crash"): New Tab,
   Close Tab, Next/Prev Tab, Rename Tab (opens the rename editor), New/Close/Rename/Next/Prev
   Workspace, Split Horizontal/Vertical, Close Pane, Zoom Pane, Focus Pane (all 4 directions), Toggle
   AI Panel, Focus AI Panel, Toggle Fullscreen, Toggle Status Bar (bar visibly appears/disappears),
   Open Config File (opens your editor/default app on the config file), Open Config Folder (opens
   Finder on the config dir), Reload Config, Switch To Tab N (if you have 2+ tabs open).
5. Escape closes without running anything.
6. Confirm none of the deferred actions (snippets, saved workspaces, git-branch picker, theme picker,
   the AI actions M3b already deferred, `TrustLocalMcp`) appear in the list at all.
7. Clicking into the terminal while the palette is open does NOT get intercepted -- wait, the palette
   is a true modal (backdrop blocks all clicks by design, confirmed in Task 2's own dogfood) -- so
   instead confirm Escape closes it and typing immediately afterward lands in the terminal normally.

- [ ] **Step 5: Commit**

```bash
git add src/gpui_shell/palette_dispatch.rs src/gpui_shell/palette.rs src/gpui_shell/leader_dispatch.rs src/gpui_shell/mod.rs
git commit -m "feat: Add the full command-palette action dispatch table (M4a Task 3)."
```

---

## Exit Criteria

- `Leader o` opens a centered modal command palette matching `InfoOverlay`'s established backdrop
  shape; typing filters fuzzily; Up/Down navigate; Enter runs the selected action; Escape closes.
- Every `Action` variant the M4 spec's §3 table lists as supported is reachable and does the right
  thing; every deferred variant (§7) is absent from the list entirely, never shown-and-inert.
- `scripts/ci-local.sh` is green and the full `cargo test --lib` suite passes after each task.
- This completes M4a. M4b (search bar), M4c (context menu, scoped down), M4d (toasts) remain, each
  gets its own plan under the same M4 spec, following the same one-at-a-time rhythm M3a-d used.
