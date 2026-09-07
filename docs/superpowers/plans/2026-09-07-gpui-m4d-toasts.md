# M4d — Toasts Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Port the wgpu build's transient top-right toast notification to `gpui_shell`, as a real gpui `div()` rather than a hand-shaped GPU rect, and wire it to a real, already-in-scope trigger (config hot-reload) so it's exercised end-to-end. This completes M4.

**Architecture:** A single small `GpuiShellRoot` field, `toast: Option<(String, Instant)>` -- the exact shape `src/app/mod.rs`'s own `toast` field already uses. `poll.rs`'s existing ~33ms tick drains an expired toast the same way it already drains cursor blink and leader-deadline expiry. A new `toast.rs` holds the render function and a `show_toast(message, duration, cx)` trigger method. Deliberately non-modal (no backdrop, no `stop_propagation`, no `FocusHandle`, no key guard) -- a toast is a passive notification the user can click straight through.

**Tech Stack:** Rust, gpui 0.2.2.

**Spec:** `docs/superpowers/specs/2026-09-07-gpui-m4-remaining-surfaces-design.md` (§6 M4d design, §7 deferred items, §8 M4d manual-test checklist, §9 Global Constraints).

## Global Constraints

- 400-line module limit -- `mod.rs` is currently exactly 400 lines and this milestone must add a
  module declaration, a field, and an initializer to it; Task 1 includes two small doc-comment
  trims (exact text given below) to keep it at or under the ceiling. `render.rs` (391) and
  `poll.rs` (195) both have headroom.
- `scripts/ci-local.sh` (including `cargo fmt --check`) must stay green after every task; run
  `cargo fmt` proactively before every commit, not just `--check` after.
- Commit format: `type: Message.` per `AGENTS.md`.
- Key/focus guards key on real focus (`is_focused(window)`), never on visibility/open state --
  does not apply to this milestone's own surface: the toast grabs no `FocusHandle` at all (it is
  never interactive), so it needs no guard of any kind, matching `InfoOverlay`'s "no focus
  handle, no guard needed" condition (M3d) rather than the text-input-holding precedent (M4a/b).
- Tests for logic only -- no painting/layout tests; every item in the spec's §8 M4d checklist is
  a dogfood step, not a unit test to write.
- **Real, verified scope-down finding (authoritative, not open for reconsideration):** the spec's
  own §6 State section describes draining `crate::config::lua::drain_lua_toast(lua)` -- the wgpu
  build's real Lua-triggered path (`petruterm.notify(msg, ms)`). Confirmed via direct
  investigation this session: `gpui_shell` keeps **no live Lua VM at all** -- `config_watch.rs`'s
  `spawn_config_watcher` calls `crate::config::reload()` and immediately discards its `_lua`
  return value; no `GpuiShellRoot` field holds a `Lua` handle; no `fire_lua_event`/
  `pending_lua_events`-equivalent exists anywhere in `src/gpui_shell/`; the default config
  (`config/default/*.lua`) doesn't call `petruterm.notify()` either, so nothing already-shipped
  depends on this path. Porting a live Lua VM + event-firing into `gpui_shell` is a full
  standalone milestone of its own scope (comparable to M3b's LLM-provider port), not a "toasts"
  task -- deferred, recorded in this plan's own Task 2 and in the M4 deferred-items list. This
  plan instead builds the real, reusable `show_toast(message, duration, cx)` primitive (the exact
  method a future Lua bridge would call once it exists) and wires it to one concrete, already-
  in-scope trigger: config hot-reload, which already flows through one single drain point in
  `poll.rs` regardless of whether it came from the file-watcher thread or the command palette's
  `ReloadConfig` action -- confirmed via reading both call sites (`config_watch.rs`'s watcher
  thread and `palette_dispatch.rs`'s `Action::ReloadConfig` arm both push into the same
  `PENDING_CONFIG_RELOAD`/`CONFIG_CHANGED` statics that `poll.rs` alone drains), so one small edit
  covers both triggers for free.

---

## Task 1: Toast state, render, and poll-tick auto-expiry

**Files:**
- Create: `src/gpui_shell/toast.rs`
- Modify: `src/gpui_shell/mod.rs`
- Modify: `src/gpui_shell/poll.rs`
- Modify: `src/gpui_shell/render.rs`

**Interfaces:**
- Consumes: `crate::config::schema::ColorScheme`'s `ui_overlay`/`ui_accent`/`foreground` fields
  (all pre-existing); `super::font_state::font_family()`/`font_size()` (pre-existing, used
  identically by `search_bar.rs`/`info_overlay.rs`); `super::pane_view::to_rgba` (pre-existing).
- Produces: `GpuiShellRoot::show_toast(&mut self, message: impl Into<String>, duration:
  std::time::Duration, cx: &mut Context<Self>)` (`pub(super)`, Task 2 calls this from `poll.rs`);
  `toast::render_toast(message: &str, colors: &ColorScheme) -> impl IntoElement` (wired into
  `render()` by this same task, since a render primitive with nothing ever rendering it is an
  incomplete deliverable).

- [ ] **Step 1: Create `src/gpui_shell/toast.rs`**

```rust
// gpui chrome migration (M4d Task 1): a transient top-right notification,
// mirroring the wgpu build's own toast (`src/app/mod.rs`'s `toast` field,
// `src/app/renderer/overlay.rs`'s `build_toast_instances`) as a real gpui
// `div()` rather than a hand-shaped GPU rect. Deliberately non-modal: no
// backdrop, no `cx.stop_propagation()`, no `FocusHandle` -- the ONE surface
// in this codebase that needs no key guard at all, since nothing about it
// is interactive and `input.rs`'s `on_key_down` never needs to ask it
// anything (see this milestone's own Global Constraints for why this
// differs from every text-input-holding surface M4a/b built).
//
// The Lua-triggered path the wgpu build uses (`petruterm.notify()`) is
// deferred -- see this plan's own Global Constraints for the full,
// verified reasoning (`gpui_shell` has no live Lua VM at all yet). This
// file's own `show_toast` is the primitive a future Lua bridge would call;
// Task 2 wires it to the one concrete trigger this milestone actually
// ships: config hot-reload.

use std::time::{Duration, Instant};

use gpui::{div, prelude::*, px, Context};

use crate::config::schema::ColorScheme;

use super::pane_view::to_rgba;
use super::{font_state, GpuiShellRoot};

impl GpuiShellRoot {
    /// Queue a toast. Single-slot (matching the wgpu build's own
    /// `Option<(String, Instant)>` shape) -- a second call while one is
    /// already showing simply replaces the message and restarts the
    /// clock, rather than queueing both; this is exactly the wgpu build's
    /// own `dispatch_notification`'s behavior (`self.toast = Some((msg,
    /// deadline))`, unconditional overwrite). Drained on expiry by
    /// `poll.rs`'s own tick (Task 2).
    pub(super) fn show_toast(
        &mut self,
        message: impl Into<String>,
        duration: Duration,
        cx: &mut Context<Self>,
    ) {
        self.toast = Some((message.into(), Instant::now() + duration));
        cx.notify();
    }
}

/// Build the toast's `div()` tree: a small, floating, top-right label,
/// styled to match the wgpu build's own `build_toast_instances` (rounded
/// rect, `ui_overlay` background, `ui_accent` border, `foreground` text).
/// Non-modal by design (this module's own doc comment) -- a click landing
/// on the toast's own screen area still reaches whatever is underneath it.
pub fn render_toast(message: &str, colors: &ColorScheme) -> impl IntoElement {
    div()
        .id("toast")
        .absolute()
        .top_2()
        .right_2()
        .px_3()
        .py_2()
        .rounded_md()
        .border_1()
        .border_color(to_rgba(colors.ui_accent))
        .bg(to_rgba(colors.ui_overlay))
        .font_family(font_state::font_family())
        .text_size(px(font_state::font_size()))
        .text_color(to_rgba(colors.foreground))
        .child(message.to_string())
}
```

- [ ] **Step 2: `mod.rs` -- register the module**

Add, keeping the existing alphabetical order (right after `pub mod text_input;`, right before
`mod workspace;`):

```rust
mod toast;
```

- [ ] **Step 3: `mod.rs` -- add the field, trimming two existing comments to hold the 400-line ceiling**

`mod.rs` is currently exactly 400 lines. Adding the module declaration (Step 2), the field, and
its initializer (Step 4 below) adds 4 lines net. To stay at or under 400, tighten two existing
doc comments first -- find and replace exactly:

Replace:
```rust
    /// Set by the `palette_query` subscription (Step 4) when `Submit` fires
    /// and `CommandPalette::confirm()` returns an action to run;
    /// `render()`'s own top (`render.rs`, Task 2) drains and dispatches it
    /// every frame. Needed because `cx.subscribe`'s callback is handed no
    /// `Window` -- the same constraint M3b's chat-panel `/q` close already
    /// worked around (`render.rs`'s own doc comment on its focus-reclaim
    /// guard has the full precedent).
    pending_palette_action: Option<Action>,
```
with:
```rust
    /// Set when the palette confirms an action with no `Window` in hand
    /// (`cx.subscribe` callback) -- `render()`'s own top drains and
    /// dispatches it every frame. Same constraint M3b's chat `/q` close hit.
    pending_palette_action: Option<Action>,
```

Replace:
```rust
    /// In-terminal text search (`Cmd+F`) -- `crate::ui::search_bar::
    /// SearchBar`, used directly, same relationship as `CommandPalette`.
    /// Unlike the palette, this drives real GPU-paint highlighting (M4b
    /// Task 3) rather than only its own popup content.
    search_bar: SearchBar,
```
with:
```rust
    /// In-terminal text search (`Cmd+F`) -- `crate::ui::search_bar::
    /// SearchBar`, reused directly; drives real GPU-paint highlighting too.
    search_bar: SearchBar,
```

Then add the new field right after the existing `context_menu` field (last field in the struct,
right before the struct's closing `}`):

```rust
    /// The right-click context menu's own state -- see `context_menu.rs`'s
    /// own doc comment for why this isn't a reuse of `ContextMenu`.
    context_menu: context_menu::ContextMenu,
    /// Transient top-right notification -- see `toast.rs`'s own doc comment.
    toast: Option<(String, std::time::Instant)>,
}
```

After these three edits, `mod.rs` should be exactly 399 lines -- run `wc -l src/gpui_shell/
mod.rs` and confirm before moving on; if it differs, note the exact number in your report's
CONCERNS rather than guessing at a further trim yourself.

- [ ] **Step 4: `mod.rs` -- initialize the field**

Find the `Self { ... }` construction at the end of `GpuiShellRoot::new` (the block ending with
`context_menu: context_menu::ContextMenu::default(),`) and add `toast: None,` right after it:

```rust
            context_menu: context_menu::ContextMenu::default(),
            toast: None,
        }
```

- [ ] **Step 5: `poll.rs` -- auto-expire on tick**

`poll.rs`'s top currently imports `use std::time::Duration;`. Change that single import line to:

```rust
use std::time::{Duration, Instant};
```

Then, inside the second `this.update(cx, |this: &mut GpuiShellRoot, cx| { ... })` closure (the
one already computing `should_notify` for cursor blink/leader-deadline/PTY exit -- NOT the first
`this.update` closure that applies a reloaded config), add this check anywhere among the other
`should_notify`-setting checks (e.g. right after the leader-deadline-expiry block, before the
status-bar CWD/exit-code/git-branch block):

```rust
                    // Toast auto-dismiss: cleared once its deadline passes,
                    // same shape as leader-deadline expiry just above --
                    // piggybacks on this same 33ms tick rather than a
                    // dedicated timer. See `toast.rs`'s own doc comment.
                    if this
                        .toast
                        .as_ref()
                        .is_some_and(|(_, deadline)| Instant::now() >= *deadline)
                    {
                        this.toast = None;
                        should_notify = true;
                    }
```

- [ ] **Step 6: `render.rs` -- wire the toast into the render tree**

`render.rs` already imports `context_menu, info_overlay, palette, pane_view, render_callbacks,
search_bar, status_bar, tabs` from `super::{...}` -- add `toast` to that same list (keep
alphabetical order: right after `tabs,`):

```rust
use super::{
    ai_block, chat_panel, context_menu, info_overlay, palette, pane_view, render_callbacks,
    search_bar, status_bar, tabs, toast, GpuiShellRoot,
};
```

Add a new child to the root div, right after the existing `.when(self.context_menu.visible,
...)` block (the final child before the closing of the `div()...` chain and the `}` that ends
`render()`):

```rust
            .when(self.context_menu.visible, |el| {
                el.child(context_menu::render_context_menu(
                    &self.context_menu,
                    &self.config.colors,
                    on_context_action,
                    on_close_context_menu,
                ))
            })
            .when_some(self.toast.clone(), |el, (msg, _)| {
                el.child(toast::render_toast(&msg, &self.config.colors))
            })
    }
}
```

(That is: insert the new `.when_some(...)` block between the existing `.when(self.context_menu
...)` block and the `render()` function's closing `}`/`}`. `self.toast.clone()` is a cheap clone
of a `String` + `Instant` that only happens once per frame while a toast is actually showing --
same cost class as `self.search_bar.matches.clone()` a few lines above it in this same
function.)

- [ ] **Step 7: Build, test, verify**

Run: `cargo build 2>&1 | tail -100`, `cargo test --lib 2>&1 | tail -10` (230/230, unchanged),
`cargo fmt` then `cargo fmt --check` (clean), `cargo clippy --all-features -- -D warnings`
(clean), `./scripts/ci-local.sh` (exit 0).

Run `wc -l src/gpui_shell/mod.rs src/gpui_shell/toast.rs src/gpui_shell/poll.rs
src/gpui_shell/render.rs` and note the results; flag any 400-line overshoot in your report's
CONCERNS rather than splitting yourself.

Dogfood is not possible from this environment (no interactive access to the running app) --
nothing to trigger a toast yet regardless (Task 2 adds the first real trigger). Note in your
report that dogfood is deferred to Task 2's own dogfood step.

- [ ] **Step 8: Commit**

```bash
git add src/gpui_shell/toast.rs src/gpui_shell/mod.rs src/gpui_shell/poll.rs src/gpui_shell/render.rs
git commit -m "feat: Add the toast notification's state, render, and auto-expiry (M4d Task 1)."
```

---

## Task 2: Wire config-reload as the toast's first real trigger

**Files:**
- Modify: `src/gpui_shell/poll.rs`

**Interfaces:**
- Consumes: `GpuiShellRoot::show_toast` (Task 1).

- [ ] **Step 1: `poll.rs` -- fire a toast when a reloaded config is applied**

Inside the FIRST `this.update(cx, |this: &mut GpuiShellRoot, cx| { ... })` closure in
`spawn_poll_loop` (the one inside `if CONFIG_CHANGED.swap(...) { if let Some(new_config) = ... }`,
which currently ends with `this.ai_block.rewire_provider(&this.config.llm); cx.notify();`), add
the toast call right before the existing `cx.notify();` in that closure:

```rust
                            this.ai_block.rewire_provider(&this.config.llm);
                            this.show_toast(
                                "Config reloaded.",
                                std::time::Duration::from_millis(3000),
                                cx,
                            );
                            cx.notify();
```

This single edit covers BOTH real triggers already wired to `CONFIG_CHANGED`/
`PENDING_CONFIG_RELOAD`: the file-watcher thread (`config_watch.rs`'s `spawn_config_watcher`,
fires on an on-disk edit to any Lua config file) and the command palette's `Leader o` ->
"Reload Config" row (`palette_dispatch.rs`'s `Action::ReloadConfig` arm) -- both push into the
same two statics this one `if CONFIG_CHANGED.swap(...)` block alone drains, confirmed by reading
both call sites before writing this plan. No second call site to wire.

3000ms matches `crate::config::lua::drain_lua_toast`'s own default duration (`ms.unwrap_or
(3000)`) -- kept identical here for visual/timing consistency with the wgpu build's own toast,
even though this path doesn't go through that function.

- [ ] **Step 2: Build, test, verify**

Run: `cargo build 2>&1 | tail -100`, `cargo test --lib 2>&1 | tail -10` (230/230, unchanged),
`cargo fmt` then `cargo fmt --check` (clean), `cargo clippy --all-features -- -D warnings`
(clean), `./scripts/ci-local.sh` (exit 0).

Run `wc -l src/gpui_shell/poll.rs` and note the result; flag any 400-line overshoot in your
report's CONCERNS rather than splitting yourself (unlikely -- this step adds ~5 lines to a file
this plan already confirmed has headroom).

Dogfood, reproducing the M4 spec's §8 M4d checklist:
1. Edit any file under `~/.config/petruterm/` (or the project's own config dir) that the running
   app's config watcher tracks -- e.g. touch `config.lua` with a trivial whitespace change and
   save. Within ~3 seconds, a "Config reloaded." toast should appear in the top-right corner,
   styled with a rounded border and the theme's accent color.
2. Confirm the toast disappears on its own after ~3 seconds without any user action.
3. While the toast is visible, click into the terminal and type -- confirm the toast does NOT
   block or intercept the click/keystrokes (it's non-modal, no backdrop).
4. Trigger two reloads in quick succession (edit-save twice within ~1 second) -- confirm the
   second toast cleanly replaces the first (no visual overlap/stacking, no crash, no stuck
   toast from the first that never clears).
5. Open the command palette (`Leader o`) and run "Reload Config" -- confirm this ALSO shows the
   toast (the second of the two triggers this task wires, both through the same code path).

- [ ] **Step 3: Commit**

```bash
git add src/gpui_shell/poll.rs
git commit -m "feat: Show a toast when the config hot-reloads (M4d Task 2)."
```

---

## Exit Criteria

- A toast actually appears top-right (rounded, accent-bordered, matching the wgpu build's own
  visual style) and auto-dismisses after its configured duration, driven by `poll.rs`'s existing
  ~33ms tick, no dedicated timer.
- Two toasts firing in quick succession don't overlap illegibly or crash the poll loop -- the
  single-slot `Option<(String, Instant)>` model makes the second cleanly replace the first.
- A toast doesn't block or intercept clicks/keys meant for the terminal underneath -- no
  backdrop, no `stop_propagation`, no `FocusHandle`, no key guard.
- Config hot-reload (both the file-watcher thread and the palette's "Reload Config" action) shows
  a real, dogfoodable toast, exercising the whole feature end-to-end without inventing a
  Lua-VM port this milestone doesn't need.
- `scripts/ci-local.sh` (including `cargo fmt --check`) is green and the full `cargo test --lib`
  suite passes after each task.
- This completes M4 (command palette, search bar, context menu, toasts). The full Lua
  `petruterm.notify()` bridge stays deferred (see Global Constraints) until `gpui_shell` gets a
  live Lua VM of its own -- a separate, future milestone.
