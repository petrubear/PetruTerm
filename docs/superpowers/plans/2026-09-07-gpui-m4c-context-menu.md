# gpui M4c: Context Menu (Scoped Down) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Right-click the terminal grid for a Copy/Paste/Clear menu; right-click a tab for a
tab-color picker. `Cmd+K` (clear screen + scrollback) also gets wired -- a real, documented keybind
gap this milestone closes as a side effect of building the menu's own Clear action.

**Architecture:** `crate::ui::context_menu::{ContextAction, ContextMenuItem}` are pure data (no
wgpu/coordinate coupling) and are reused directly. `ContextMenu` itself is NOT reused -- its
`open()`/`hit_test()`/`col`/`row` are built around the wgpu build's manual terminal-cell
hit-testing, which has no equivalent here. `gpui_shell` builds its own minimal state (visible,
pixel position, items) and renders each row as a real clickable `div()`, matching every other
list this migration has built (M3d's sidebar sections, M4a's palette, M4b's search bar). Two
independent trigger surfaces, built from scratch (`gpui_shell` has zero right-click handling
anywhere yet): the terminal grid's own paint path gets a new, independent
`window.on_mouse_event` registration; the tab bar's existing per-cell `div`s get a second
`.on_mouse_down(MouseButton::Right, ..)` alongside their existing left-click handler.

**Tech Stack:** Rust, gpui 0.2.2 (existing `gpui_shell` conventions only -- no new crates).

**Spec:** `docs/superpowers/specs/2026-09-07-gpui-m4-remaining-surfaces-design.md` (§5 M4c design,
§8 M4c manual-test checklist, §9 Global Constraints). This plan's own rendering-shape decision
(reuse `ContextAction`/`ContextMenuItem` only, not `ContextMenu`) supersedes the spec's original
assumption of reusing `ContextMenu` wholesale -- verified against real source during this plan's
own writing, not a design still open for reconsideration.

## Global Constraints

- 400-line module limit. `mod.rs` (394), `input.rs` (393), and `tabs.rs` (385) are ALL close to
  the limit before this plan starts -- every task keeps new logic in new files as much as
  possible (a new `context_menu.rs` holds nearly everything); `input.rs`/`mod.rs`/`tabs.rs` each
  get only small, fixed-size additions. `mouse.rs` (874, already over from pre-existing work
  unrelated to this milestone) is never touched by this plan -- the terminal grid's right-click
  handler is a new, independent registration added from `terminal_element.rs`'s own `paint()`,
  not folded into `mouse.rs`.
- `scripts/ci-local.sh` must stay green after every task, including `cargo fmt --check` --
  three separate controller review-fixes in M4a/M4b were `cargo fmt` violations from
  controller-authored extractions; run `cargo fmt` as a matter of course before every commit
  this plan makes, not just build/test/clippy.
- Commit format: `type: Message.` per `AGENTS.md`.
- The context menu is neither a blocking modal (M4a's palette) nor fully non-modal (M4b's search
  bar) -- it closes on any click outside itself (via `on_mouse_down_out`, which fires during the
  CAPTURE phase and does NOT call `cx.stop_propagation()`), and that same outside click continues
  through to whatever it actually landed on (the terminal, a different tab, etc.) rather than
  being swallowed. This is a real, deliberate third shape, not an oversight.
- Tests for logic only -- no painting/hit-testing tests (dogfooded; GPU windows can't be driven
  interactively from the agent sandbox).
- `#[allow(dead_code)]` (narrowly scoped, comment naming the removing task) for anything built
  ahead of its first real caller.

---

## Task 1: `ContextMenu` state, dispatch logic, and the `Cmd+K` keybind

**Purpose:** Wire the menu's state machine and every action it can dispatch (Copy, Paste, Clear,
SetTabColor), plus `Cmd+K` as a real, independently useful, immediately dogfoodable keybind (no
context-menu UI needed to test it). No right-click trigger and no render yet -- Task 2/3 add
those; the menu genuinely cannot open until then, the same "state before UI" shape M4a/M4b's own
Task 1s used.

**Files:**
- Create: `src/gpui_shell/context_menu.rs`
- Modify: `src/gpui_shell/mod.rs`
- Modify: `src/gpui_shell/standalone_keys.rs`
- Modify: `src/gpui_shell/input.rs`

**Interfaces:**
- Consumes: `crate::ui::context_menu::{ContextAction, ContextMenuItem}` (pure data, reused
  directly), `Terminal::clear_screen_and_scrollback(&self)` (pre-existing), `TabManager::
  set_tab_color(&mut self, idx: usize, color: Option<[f32; 4]>)` (pre-existing, M3d).
- Produces: `context_menu::ContextMenu { visible: bool, position: gpui::Point<gpui::Pixels>,
  items: Vec<ContextMenuItem> }`; `GpuiShellRoot::{context_menu: ContextMenu, dispatch_context_action(&mut self, action: ContextAction, cx: &mut Context<Self>)}`. Task 2 is the first caller of
  `dispatch_context_action` via a real UI row click; this task's own dogfood only exercises
  `Cmd+K` directly.

- [ ] **Step 1: Write `src/gpui_shell/context_menu.rs`**

`Copy`'s real mechanism (already verified against `mouse.rs`'s own left-mouse-up handler,
which is what `Cmd+C`'s existing "select then it's on the clipboard" behavior already uses):
`Terminal::selection_text(&self) -> Option<String>` (`src/term/mod.rs`), written to the
clipboard via `cx.write_to_clipboard(gpui::ClipboardItem::new_string(text))` -- the same call
`mouse.rs` already makes. `Paste`'s real mechanism (already verified against `input.rs`'s own
existing `Cmd+V` block): read `cx.read_from_clipboard()`, then wrap in `\x1b[200~`/`\x1b[201~`
markers when `terminal.bracketed_paste_mode()` is true, else write the raw bytes. This step
extracts that exact paste logic into a shared method (`paste_text_to_active_terminal`) so the
context menu's own `Paste` item and `input.rs`'s existing `Cmd+V` handler share one code path
instead of the bracketed-paste branching existing twice -- `input.rs`'s own `Cmd+V` block is
updated to call it too, in this same step (see the end of this step, after the main file).

```rust
// gpui chrome migration (M4c Task 1): the right-click context menu's own
// state and action dispatch. `crate::ui::context_menu::{ContextAction,
// ContextMenuItem}` are pure data (no wgpu/coordinate coupling) and are
// reused directly -- `ContextMenu` itself is NOT reused: its `open()`/
// `hit_test()`/`col`/`row` are built around the wgpu build's manual
// terminal-cell hit-testing (a custom rect renderer over grid cells),
// which has no equivalent in gpui_shell. This file's own `ContextMenu`
// is a genuinely new, minimal type: visible + a real pixel position +
// the item list, rendered as real clickable `div()`s (Task 2/3) the same
// way every other list this migration has built already works.

use gpui::{Context, Pixels, Point};

use crate::ui::context_menu::{ContextAction, ContextMenuItem};

use super::GpuiShellRoot;

/// The right-click menu's own state. `position` is a real window-relative
/// pixel point (the click that opened it), not a terminal-cell coordinate.
#[derive(Default)]
pub struct ContextMenu {
    pub visible: bool,
    pub position: Point<Pixels>,
    pub items: Vec<ContextMenuItem>,
}

impl ContextMenu {
    pub fn close(&mut self) {
        self.visible = false;
    }
}

impl GpuiShellRoot {
    /// Run one confirmed context-menu action. `ContextAction`'s other
    /// variants (`SendToChat`, `CopyLastCommand`, `OpenLink`, `CopyLink`,
    /// `CopyBlockOutput`, `ReRunCommand`, `Separator`, `Label`) are never
    /// constructed by this milestone's own item lists (Task 2/3) -- no
    /// arm needed for them here, `_ => {}` covers anything unreachable in
    /// practice the same way M4a's palette dispatch does.
    pub(super) fn dispatch_context_action(&mut self, action: ContextAction, cx: &mut Context<Self>) {
        self.context_menu.close();
        match action {
            ContextAction::Copy => {
                let active_ws = self.workspaces.active();
                let active_tid = active_ws.tab_panes[active_ws.tabs.active_index()].focused_terminal;
                if let Some(terminal) = self.terminals.get(&active_tid) {
                    if let Some(text) = terminal.selection_text() {
                        cx.write_to_clipboard(gpui::ClipboardItem::new_string(text));
                    }
                }
            }
            ContextAction::Paste => {
                if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
                    self.paste_text_to_active_terminal(&text);
                }
                cx.notify();
            }
            ContextAction::Clear => self.clear_active_terminal(),
            ContextAction::SetTabColor(idx, color) => {
                self.workspaces.active_mut().tabs.set_tab_color(idx, color);
                cx.notify();
            }
            _ => {}
        }
    }

    fn clear_active_terminal(&mut self) {
        let active_ws = self.workspaces.active();
        let active_tid = active_ws.tab_panes[active_ws.tabs.active_index()].focused_terminal;
        if let Some(terminal) = self.terminals.get(&active_tid) {
            terminal.clear_screen_and_scrollback();
        }
    }

    /// Shared with `input.rs`'s own `Cmd+V` handler (updated below in this
    /// same step) so the two can't drift apart. Ported verbatim from that
    /// handler's own pre-existing bracketed-paste branching.
    pub(super) fn paste_text_to_active_terminal(&mut self, text: &str) {
        let active_ws = self.workspaces.active();
        let active_tid = active_ws.tab_panes[active_ws.tabs.active_index()].focused_terminal;
        let Some(terminal) = self.terminals.get(&active_tid) else {
            return;
        };
        if terminal.bracketed_paste_mode() {
            let mut data = b"\x1b[200~".to_vec();
            data.extend_from_slice(text.as_bytes());
            data.extend_from_slice(b"\x1b[201~");
            terminal.write_input(&data);
        } else {
            terminal.write_input(text.as_bytes());
        }
    }
}
```

Now update `input.rs`'s existing `Cmd+V` block to call this new shared method instead of its own
inline bracketed-paste branching -- replace:

```rust
        if event.keystroke.modifiers.platform && event.keystroke.key == "v" {
            if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
                if terminal.bracketed_paste_mode() {
                    let mut data = b"\x1b[200~".to_vec();
                    data.extend_from_slice(text.as_bytes());
                    data.extend_from_slice(b"\x1b[201~");
                    terminal.write_input(&data);
                } else {
                    terminal.write_input(text.as_bytes());
                }
                cx.notify();
            }
            return;
        }
```

with:

```rust
        if event.keystroke.modifiers.platform && event.keystroke.key == "v" {
            if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
                self.paste_text_to_active_terminal(&text);
                cx.notify();
            }
            return;
        }
```

(The `terminal` local this block used to reference stays in scope and is still used later in the
same function for `key_map::translate_key` -- this change only removes its use from the `Cmd+V`
block's own body.)

- [ ] **Step 2: `mod.rs` -- the `context_menu` field**

Add `mod context_menu;` to the module list, alphabetically after `mod config_watch;` and before
`mod info_overlay;`.

Add to the imports:

```rust
use crate::ui::context_menu::ContextMenu;
```

Add to the `GpuiShellRoot` struct, right after the `search_query` field:

```rust
    search_query: gpui::Entity<text_input::TextInput>,
    /// The right-click context menu's own state -- see `context_menu.rs`'s
    /// own doc comment on why this is a new, minimal type rather than a
    /// reuse of `crate::ui::context_menu::ContextMenu`.
    context_menu: ContextMenu,
```

Add to the `Self { .. }` literal, right after `search_query,`:

```rust
            search_query,
            context_menu: ContextMenu::default(),
```

- [ ] **Step 3: `standalone_keys.rs` -- `Cmd+K` clears the active terminal**

Add a new method to the existing `impl GpuiShellRoot` block:

```rust
    /// `Cmd+K` -- clear the focused terminal's screen and scrollback.
    /// Shares its actual clear logic with the context menu's own `Clear`
    /// item (`context_menu.rs`'s `dispatch_context_action`) via
    /// `clear_active_terminal`, so the two can't drift apart.
    pub(super) fn clear_focused_terminal(&mut self, cx: &mut Context<Self>) {
        self.clear_active_terminal();
        cx.notify();
    }
```

- [ ] **Step 4: `input.rs` -- the `Cmd+K` guard**

Add this guard right after the existing `Cmd+F` guard block, before the `Cmd+1-9` block:

```rust
        // ── Cmd+K — clear screen + scrollback ─────────────────────────────
        // Standalone combo, not a leader chord, same shape as `Cmd+F`
        // above. See `standalone_keys.rs`'s own doc comment on
        // `clear_focused_terminal` for the full reasoning.
        if event.keystroke.modifiers.platform
            && !event.keystroke.modifiers.shift
            && !event.keystroke.modifiers.control
            && !event.keystroke.modifiers.alt
            && event.keystroke.key == "k"
        {
            self.clear_focused_terminal(cx);
            return;
        }

```

- [ ] **Step 5: Build, test, verify**

Run: `cargo build 2>&1 | tail -80` -- zero errors, zero warnings.

Run: `cargo test --lib 2>&1 | tail -10` -- 230/230 passing, no new tests (no new pure logic --
`clear_active_terminal`/`dispatch_context_action` are UI-adjacent wiring, exercised by dogfood,
matching this project's established convention).

Run: `cargo fmt --check` -- must report no diff. Run `cargo fmt` first if it does, then re-run
`cargo build`/`cargo test --lib` to confirm formatting didn't break anything.

Run: `./scripts/ci-local.sh` -- must exit 0.

Run `wc -l src/gpui_shell/mod.rs src/gpui_shell/input.rs src/gpui_shell/standalone_keys.rs
src/gpui_shell/context_menu.rs` and note the results in your report -- if `mod.rs` or `input.rs`
crossed 400, note the exact line count in CONCERNS; do not attempt a split yourself, that's a
controller decision.

Dogfood: launch the app, press `Cmd+K` -- the terminal's screen and scrollback clear completely
(same as running `clear` at the shell, but instant and via keybind). This is real, complete,
independently useful functionality on its own, closing a documented keybind gap -- no
context-menu UI exists yet to test anything else this task built.

- [ ] **Step 6: Commit**

```bash
git add src/gpui_shell/context_menu.rs src/gpui_shell/mod.rs src/gpui_shell/standalone_keys.rs src/gpui_shell/input.rs
git commit -m "feat: Wire the context menu's state, dispatch, and the Cmd+K keybind (M4c Task 1)."
```

---

## Task 2: Terminal grid right-click — Copy/Paste/Clear menu

**Purpose:** Right-click the terminal grid opens a real, working Copy/Paste/Clear menu at the
click position, closes on outside click or after running an action. Matches the wgpu build's own
established behavior: the menu always operates on the globally-active terminal (`self.workspaces
.active()`'s focused pane), not specifically whichever pane's grid the click landed on -- the
same scoping `Mux::active_terminal()` already uses for its own `open_default()`/Copy/Paste/Clear,
confirmed by reading `src/app/mod.rs`'s call sites during this task's own design. This means the
right-click callback needs only the click's pixel position, never a terminal id.

**Files:**
- Modify: `src/gpui_shell/context_menu.rs`
- Modify: `src/gpui_shell/terminal_element.rs`
- Modify: `src/gpui_shell/pane_view.rs`
- Modify: `src/gpui_shell/render.rs`

**Interfaces:**
- Consumes: `ContextMenu`/`dispatch_context_action` (Task 1), `TerminalGridElement`'s existing
  `paint()` structure and its `mouse::register_mouse_handlers` call (pre-existing), `render.rs`'s
  existing `view`/`on_focus`/`on_drag` weak-handle pattern (pre-existing -- this task's own
  callbacks are built the identical way).
- Produces: `context_menu::render_context_menu(menu: &ContextMenu, colors: &ColorScheme,
  on_action: ContextActionCallback, on_close_outside: ContextMenuCloseCallback) -> impl
  IntoElement`; `context_menu::{RightClickCallback, ContextActionCallback,
  ContextMenuCloseCallback}`; `context_menu::register_right_click(bounds: Bounds<Pixels>,
  on_right_click: RightClickCallback, window: &mut Window)`; `PaneRenderCx::on_right_click:
  RightClickCallback`; `TerminalGridElement::on_right_click: RightClickCallback`.

- [ ] **Step 1: Extend `src/gpui_shell/context_menu.rs`**

Replace the existing `use gpui::{Context, Pixels, Point};` line with:

```rust
use std::rc::Rc;

use gpui::{
    div, prelude::*, px, App, Bounds, Context, DispatchPhase, MouseButton, MouseDownEvent, Pixels,
    Point, Window,
};

use crate::config::schema::ColorScheme;

use super::pane_view::to_rgba;
```

Add these three type aliases and two functions after the existing `ContextMenu` struct's `impl`
block (after its closing `}`):

```rust
/// Called with the click's real window-space pixel position on a right
/// mouse-down over the terminal grid. No terminal id: Copy/Paste/Clear all
/// operate on the globally-active terminal (this file's own doc comment
/// has the reasoning), the same scoping the wgpu build's own
/// `Mux::active_terminal()`-based menu already uses.
pub(super) type RightClickCallback = Rc<dyn Fn(Point<Pixels>, &mut Window, &mut App)>;

/// Called when a menu row is clicked, with that row's own `ContextAction`.
pub type ContextActionCallback = Rc<dyn Fn(&crate::ui::context_menu::ContextAction, &mut Window, &mut App)>;

/// Called when a click lands outside the menu (`on_mouse_down_out`, Step 2).
pub type ContextMenuCloseCallback = Rc<dyn Fn(&mut Window, &mut App)>;

/// Register the terminal grid's own right-click handler -- a new,
/// independent `window.on_mouse_event` registration alongside (not
/// replacing) `mouse::register_mouse_handlers`'s existing left-click/drag/
/// scroll handling in `terminal_element.rs`'s `paint()`. Deliberately NOT
/// added to `mouse.rs` itself (874 lines already, well over this project's
/// 400-line convention from pre-existing work) -- this keeps that
/// pre-existing overshoot from growing worse for no reason.
pub(super) fn register_right_click(
    bounds: Bounds<Pixels>,
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
        on_right_click(event.position, window, cx);
    });
}

/// Build the context menu's `div()` tree: a small popup positioned at
/// `menu.position`, one clickable row per item, closing on any click
/// outside itself. `on_mouse_down_out` fires during the CAPTURE phase and
/// does NOT call `cx.stop_propagation()`, so the outside click that closed
/// this menu still reaches whatever it actually landed on (the terminal, a
/// different tab) afterward -- unlike M4a's palette or M3d's
/// `InfoOverlay`, both genuine blocking modals with a `stop_propagation`-
/// backed backdrop.
pub fn render_context_menu(
    menu: &ContextMenu,
    colors: &ColorScheme,
    on_action: ContextActionCallback,
    on_close_outside: ContextMenuCloseCallback,
) -> impl IntoElement {
    let rows: Vec<_> = menu
        .items
        .iter()
        .enumerate()
        .filter(|(_, item)| !item.is_non_interactive())
        .map(|(idx, item)| {
            let action = item.action.clone();
            let label = item.label.clone();
            let keybind = item.keybind.clone();
            let swatch = item.swatch_color;
            let on_action = on_action.clone();
            div()
                .id(("context-menu-row", idx))
                .flex()
                .flex_row()
                .items_center()
                .justify_between()
                .px_2()
                .py_1()
                .cursor_pointer()
                .text_color(to_rgba(colors.foreground))
                .hover(|el| el.bg(to_rgba(colors.ui_surface_active)))
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_2()
                        .when_some(swatch, |el, color| {
                            el.child(
                                div()
                                    .w(px(10.0))
                                    .h(px(10.0))
                                    .rounded_full()
                                    .bg(to_rgba(color)),
                            )
                        })
                        .child(label),
                )
                .when_some(keybind, |el, kb| {
                    el.child(div().text_size(px(11.0)).child(kb))
                })
                .on_mouse_down(MouseButton::Left, move |_: &MouseDownEvent, window, cx| {
                    on_action(&action, window, cx)
                })
        })
        .collect();

    div()
        .id("context-menu")
        .absolute()
        .left(menu.position.x)
        .top(menu.position.y)
        .flex()
        .flex_col()
        .min_w(px(160.0))
        .bg(to_rgba(colors.ui_surface))
        .border_1()
        .border_color(to_rgba(colors.ui_border))
        .on_mouse_down_out(move |_: &MouseDownEvent, window, cx| on_close_outside(window, cx))
        .children(rows)
}
```

(This step already renders each item's `swatch_color` dot -- Task 3's own color-picker items
populate it, Task 2's own Copy/Paste/Clear items leave it `None`, so no separate change is needed
later for that part of Task 3.)

- [ ] **Step 2: `terminal_element.rs` -- the new field and its `paint()` wiring**

Add `use super::context_menu;` to the imports.

Add a new field to `TerminalGridElement`, right after `pub search: Option<(Vec<SearchMatch>,
usize)>,`:

```rust
    pub search: Option<(Vec<SearchMatch>, usize)>,
    /// Opens the context menu at a right-click's position over this pane.
    /// See `context_menu.rs`'s own doc comment on `RightClickCallback` for
    /// why this carries no terminal id.
    pub on_right_click: context_menu::RightClickCallback,
```

In `paint()`, right after the existing `mouse::register_mouse_handlers(...)` call's closing
`);`, add:

```rust
        context_menu::register_right_click(bounds, self.on_right_click.clone(), window);
```

- [ ] **Step 3: `pane_view.rs` -- thread `on_right_click` through `PaneRenderCx`**

Add a new field to `PaneRenderCx`, right after `pub search: Option<(Vec<SearchMatch>, usize)>,`:

```rust
    pub search: Option<(Vec<SearchMatch>, usize)>,
    /// Opens the context menu on a right-click anywhere in the pane area --
    /// passed straight through to every leaf's own `TerminalGridElement`
    /// unchanged (no per-leaf wrapping needed, since it carries no
    /// terminal id -- see `context_menu.rs`'s own doc comment on why).
    pub on_right_click: super::context_menu::RightClickCallback,
```

In `render_leaf`, update the `TerminalGridElement { .. }` literal -- add one field right after
`on_focus,`:

```rust
            on_focus,
            on_right_click: ctx.on_right_click.clone(),
```

- [ ] **Step 4: `render.rs` -- build the three callbacks and wire the menu into the tree**

Add `use super::context_menu;` to the imports.

Change `let drag_view = view;` to `let drag_view = view.clone();` (this task needs `view` cloned
again afterward; the existing code moves it outright into `drag_view`, which would leave nothing
for this task's own clones below).

Add three new callback bindings, right after the existing `on_drag` block's closing `});`:

```rust
        let right_click_view = view.clone();
        let on_right_click: context_menu::RightClickCallback = Rc::new(move |position, _window, cx| {
            right_click_view
                .update(cx, |root, cx| {
                    root.context_menu.position = position;
                    root.context_menu.items = vec![
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
                    root.context_menu.visible = true;
                    cx.notify();
                })
                .ok();
        });

        let action_view = view.clone();
        let on_context_action: context_menu::ContextActionCallback =
            Rc::new(move |action, _window, cx| {
                let action = action.clone();
                action_view
                    .update(cx, |root, cx| root.dispatch_context_action(action, cx))
                    .ok();
            });

        let close_menu_view = view.clone();
        let on_close_context_menu: context_menu::ContextMenuCloseCallback =
            Rc::new(move |_window, cx| {
                close_menu_view
                    .update(cx, |root, cx| {
                        root.context_menu.close();
                        cx.notify();
                    })
                    .ok();
            });
```

Find the existing `let pane_ctx = pane_view::PaneRenderCx { .. };` construction and add one field
to its literal, right after `on_drag,`:

```rust
            on_drag,
            search: self
                .search_bar
                .visible
                .then(|| (self.search_bar.matches.clone(), self.search_bar.current)),
            on_right_click,
```

Add the context menu to the root div's child chain, right after the existing
`.when(self.palette.visible, ...)` block (the function's final child before `render()`'s closing
`}`):

```rust
            .when(self.palette.visible, |el| {
                el.child(palette::render_command_palette(
                    &self.palette,
                    &self.palette_query,
                    &self.config.colors,
                ))
            })
            .when(self.context_menu.visible, |el| {
                el.child(context_menu::render_context_menu(
                    &self.context_menu,
                    &self.config.colors,
                    on_context_action,
                    on_close_context_menu,
                ))
            })
    }
}
```

- [ ] **Step 5: Build, test, verify**

Run: `cargo build 2>&1 | tail -100` -- zero errors, zero warnings.

Run: `cargo test --lib 2>&1 | tail -10` (230/230, unchanged), `cargo fmt` then `cargo fmt --check`
(clean), `cargo clippy --all-features -- -D warnings` (clean), `./scripts/ci-local.sh` (exit 0).

Run `wc -l src/gpui_shell/context_menu.rs src/gpui_shell/terminal_element.rs
src/gpui_shell/pane_view.rs src/gpui_shell/render.rs` and note the results; flag any 400-line
overshoot in your report's CONCERNS rather than splitting yourself.

Dogfood: right-click anywhere in the terminal grid -- a small menu opens at the click position
with Copy/Paste/Clear. Click "Clear" -- screen and scrollback clear, menu closes. Right-click
again, click "Copy" with an active selection -- clipboard gets the selected text, menu closes.
Right-click again, click "Paste" -- clipboard content is typed into the terminal, menu closes.
Right-click, then click somewhere OUTSIDE the menu (on the terminal itself) -- menu closes AND
that click also registers normally (if it landed on the terminal, confirm the cursor/selection
responds to it too, not just the menu closing). Right-click near the window's right/bottom edge
-- note whether the menu renders off-screen (this task's own code does not clamp position to the
viewport; if it's a real, visible problem in your dogfood pass, note it as a CONCERN for the
controller rather than fixing it yourself).

- [ ] **Step 6: Commit**

```bash
git add src/gpui_shell/context_menu.rs src/gpui_shell/terminal_element.rs src/gpui_shell/pane_view.rs src/gpui_shell/render.rs
git commit -m "feat: Add the terminal grid's right-click Copy/Paste/Clear menu (M4c Task 2)."
```

---

## Task 3: Tab-bar right-click — tab color picker

**Purpose:** Right-click a tab opens a color picker (7 bright-ANSI swatches + Reset), reusing
Task 2's own render/dispatch machinery with a different item list and trigger surface.

**Files:**
- Modify: `src/gpui_shell/tabs.rs`
- Modify: `src/gpui_shell/render.rs`

**Interfaces:**
- Consumes: `context_menu::render_context_menu`/`ContextActionCallback`/
  `ContextMenuCloseCallback` (Task 2, unchanged), `TabManager::set_tab_color` (pre-existing,
  Task 1's `dispatch_context_action` already handles `ContextAction::SetTabColor`).
- Produces: `tabs::TabRightClickCallback` (a new callback type, parallel to the existing
  `TabSelectCallback`).

- [ ] **Step 1: `tabs.rs` -- the right-click callback type and wiring**

Add this type alias right after the existing `pub(super) type TabSelectCallback = ...;` line
(check its exact current name/visibility first: `grep -n "TabSelectCallback" src/gpui_shell/tabs.rs`):

```rust
pub(super) type TabRightClickCallback =
    Rc<dyn Fn(usize, gpui::Point<gpui::Pixels>, &mut Window, &mut App)>;
```

Add a new parameter to `render_tab_bar`'s own signature -- replace:

```rust
pub fn render_tab_bar(
    tabs: &TabManager,
    colors: &ColorScheme,
    on_select: TabSelectCallback,
    rename: Option<(usize, gpui::AnyElement)>,
) -> Div {
```

with:

```rust
pub fn render_tab_bar(
    tabs: &TabManager,
    colors: &ColorScheme,
    on_select: TabSelectCallback,
    on_right_click: TabRightClickCallback,
    rename: Option<(usize, gpui::AnyElement)>,
) -> Div {
```

Inside the `cells` map closure, right after the existing `let on_select = on_select.clone();`
line, add:

```rust
            let on_select = on_select.clone();
            let on_right_click = on_right_click.clone();
```

Add a second `.on_mouse_down(..)` call to the same `cell` builder, right after the existing
`.on_mouse_down(MouseButton::Left, move |_: &MouseDownEvent, window, cx| { on_select(&idx,
window, cx) })` call -- gpui `Div`s support multiple `.on_mouse_down` calls for different
buttons on the same element (confirmed against gpui 0.2.2 source: `mouse_down_listeners: Vec<
MouseDownListener>` accumulates, does not overwrite):

```rust
                .on_mouse_down(MouseButton::Left, move |_: &MouseDownEvent, window, cx| {
                    on_select(&idx, window, cx)
                })
                .on_mouse_down(MouseButton::Right, move |event: &MouseDownEvent, window, cx| {
                    on_right_click(idx, event.position, window, cx)
                })
```

- [ ] **Step 2: `render.rs` -- build the tab-color-picker item list and callback**

Add a new callback binding, right after Task 2's own `on_right_click`/`on_context_action`/
`on_close_context_menu` bindings:

```rust
        let tab_color_view = view.clone();
        let on_tab_right_click: tabs::TabRightClickCallback =
            Rc::new(move |tab_idx, position, _window, cx| {
                tab_color_view
                    .update(cx, |root, cx| {
                        let brights = root.config.colors.brights;
                        let names = ["Red", "Green", "Yellow", "Blue", "Magenta", "Cyan", "White"];
                        let mut items: Vec<crate::ui::context_menu::ContextMenuItem> = names
                            .iter()
                            .enumerate()
                            .map(|(i, name)| {
                                let color = brights[i + 1];
                                crate::ui::context_menu::ContextMenuItem {
                                    label: (*name).to_string(),
                                    keybind: None,
                                    action: crate::ui::context_menu::ContextAction::SetTabColor(
                                        tab_idx,
                                        Some(color),
                                    ),
                                    swatch_color: Some(color),
                                }
                            })
                            .collect();
                        items.push(crate::ui::context_menu::ContextMenuItem {
                            label: "Reset".to_string(),
                            keybind: None,
                            action: crate::ui::context_menu::ContextAction::SetTabColor(
                                tab_idx, None,
                            ),
                            swatch_color: None,
                        });
                        root.context_menu.position = position;
                        root.context_menu.items = items;
                        root.context_menu.visible = true;
                        cx.notify();
                    })
                    .ok();
            });
```

Update the existing `tabs::render_tab_bar(...)` call site -- find it (`let tab_bar =
tabs::render_tab_bar(&self.workspaces.active().tabs, &self.config.colors, on_select_tab,
rename);`, or similar -- read the file to confirm its exact current argument order) and add
`on_tab_right_click` as a new argument in the matching position (right after `on_select_tab`,
matching Step 1's own new parameter position in `render_tab_bar`'s signature):

```rust
        let tab_bar = tabs::render_tab_bar(
            &self.workspaces.active().tabs,
            &self.config.colors,
            on_select_tab,
            on_tab_right_click,
            rename,
        );
```

- [ ] **Step 3: Build, test, verify**

Run: `cargo build 2>&1 | tail -100`, `cargo test --lib 2>&1 | tail -10` (230/230, unchanged),
`cargo fmt` then `cargo fmt --check` (clean), `cargo clippy --all-features -- -D warnings`
(clean), `./scripts/ci-local.sh` (exit 0).

Run `wc -l src/gpui_shell/tabs.rs src/gpui_shell/render.rs src/gpui_shell/context_menu.rs` and
note the results; flag any 400-line overshoot in your report's CONCERNS rather than splitting
yourself.

Dogfood, reproducing the M4 spec's §8 M4c checklist:
1. Right-click a tab -- a color picker opens (7 named color swatches + Reset) at the click
   position.
2. Click a color -- that tab's own underline (M3d's own accent-underline feature) changes to the
   chosen color, menu closes.
3. Right-click the same tab again, click "Reset" -- the tab's underline returns to the theme
   default accent color.
4. Right-click a DIFFERENT tab -- confirm the picker targets that tab specifically (its own
   underline changes, not the previously-colored one).
5. Right-click a tab, then click elsewhere (not on the menu) -- menu closes, that click also
   registers normally (e.g. if it landed on a different tab, that tab becomes active).
6. Re-confirm Task 2's own terminal-grid right-click menu still works unaffected (Copy/Paste/
   Clear) -- both surfaces share the same underlying `context_menu` state and render function,
   so this is the cross-surface regression check.

- [ ] **Step 4: Commit**

```bash
git add src/gpui_shell/tabs.rs src/gpui_shell/render.rs src/gpui_shell/context_menu.rs
git commit -m "feat: Add the tab bar's right-click color picker (M4c Task 3)."
```

---

## Exit Criteria

- Right-clicking the terminal grid opens a Copy/Paste/Clear menu at the click position; all three
  actions work; the menu closes on an outside click without swallowing that click.
- Right-clicking a tab opens a 7-swatch-plus-Reset color picker targeting that specific tab.
- `Cmd+K` clears the focused terminal's screen and scrollback, closing a real, previously
  undocumented-in-code gap against AGENTS.md's own keybind table.
- `scripts/ci-local.sh` (including `cargo fmt --check`) is green and the full `cargo test --lib`
  suite passes after each task.
- This completes M4c. M4d (toasts) remains, to get its own plan under the same M4 spec.
