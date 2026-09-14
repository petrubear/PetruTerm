# M5b — Chat Composer Extras Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Close M3b's own deliberately-deferred chat-composer scope: `Leader a e`/`Leader a f` (explain/fix), the zero-state + post-response suggestion pills, and the Tab-triggered file attachment picker.

**Architecture:** All three reuse `ChatPanel` state M3b already ported as-is but `gpui_shell` never reads (`zero_state_hover`/`show_suggestions`/`suggestion_hover`/`file_picker_*`/`attached_files`) and pure methods already callable on it (`init_default_files`/`attach_file`/`detach_file`/`close_file_picker`/`picker_*`/`filtered_picker_items`, all `pub fn` on `ChatPanel` despite living in a private `picker` submodule -- inherent-method visibility isn't gated by module privacy, confirmed by the wgpu build's own `UiManager` already calling them across that same boundary). Suggestion pills use gpui's real `.hover()` builder instead of the wgpu build's manual hover-state tracking. `Leader a e`/`Leader a f` reuse `ShellContext::load()` (already called in `gpui_shell`, `ai_block.rs`) and a new grid-reading helper matching the exact pattern M5c's `blocks.rs` already established.

**Tech Stack:** Rust 2021, gpui 0.2.2.

**Spec:** `docs/superpowers/specs/2026-09-14-gpui-m5b-chat-composer-extras-design.md`

## Global Constraints

- 400-line module limit -- current state, verified fresh before writing this plan: `input.rs` 392,
  `render.rs` 384, `render_callbacks.rs` 206, `leader_dispatch.rs` 148, `leader.rs` 247,
  `palette_dispatch.rs` 216, `chat_panel/mod.rs` 206, `chat_panel/render.rs` 242,
  `chat_panel/stream.rs` 263, `poll.rs` 256. `input.rs`/`render.rs` are the tightest; `chat_panel/
  render.rs` will grow the most across Tasks 2-3 and is the most likely to need a split by Task 3
  -- check `wc -l` after every task's own edit and fix any real overshoot yourself (extraction into
  a new file, re-exported to keep call sites unchanged) rather than deferring it.
- `scripts/ci-local.sh` must stay green after every task (clippy `-D warnings`, `fmt --check`,
  full `cargo test --lib`, `cargo audit`); run `cargo fmt` proactively before every commit.
- Tests cover **logic only** -- no painting/hover/hit-testing tests; every UI-facing item below is
  a dogfood step (spec's own §7), not a unit test to write.
- Commit format: `type: Message.` per `AGENTS.md`.
- `#[allow(dead_code)]` (narrowly scoped, comment naming the removing task) for anything built
  ahead of its first caller within this plan's own task sequence.
- **Key-guard exception (matches `InfoOverlay`'s own precedent, not a new one):** the file
  picker's key guard (Task 3) is keyed on `self.chat.panel.file_picker_open` (a mode flag), not
  `is_focused(window)` -- the picker grabs no `FocusHandle` of its own; the composer's real
  `TextInput` keeps gpui focus throughout. A visibility-keyed guard is provably correct here for
  the identical reason it already is for `InfoOverlay`.
- Real, verified callback-capture fact (confirmed against gpui 0.2.2 source, `entity_map.rs`):
  `Entity::update`/`WeakEntity::update`'s own `update: impl FnOnce(&mut T, &mut Context<T>) -> R`
  parameter carries **no** `'static` bound -- an outer `Rc<dyn Fn(&mut Window, &mut App)>`
  callback's per-invocation `window: &mut Window` can be captured directly into the inner
  `.update(cx, |root, cx| ...)` closure (Task 2's pill callbacks do exactly this, passing `window`
  through to `root.fix_last_error(window, cx)`/`root.explain_last_output(window, cx)`). This is a
  new pattern in this codebase (every prior `Rc<dyn Fn(&mut Window, ...)>` callback either ignored
  `window` or used it only in the outer closure, never threaded into an inner `.update` body) --
  each task using it says so explicitly, this is not an assumption to re-verify per task.

---

## Task 1: `Leader a e` / `Leader a f` + palette entries

**Tier: cheap.** Small, mechanical, fully pre-specified wiring across several already-open files;
no new rendering, no new async state.

**Files:**
- Modify: `src/gpui_shell/leader.rs`
- Modify: `src/gpui_shell/leader_dispatch.rs`
- Modify: `src/gpui_shell/input.rs`
- Modify: `src/gpui_shell/palette_dispatch.rs`
- Create: `src/gpui_shell/ai_actions.rs`

**Interfaces:**
- Consumes: `ShellContext::load() -> Option<ShellContext>` (`src/llm/shell_context.rs`,
  `ShellContext { last_command: String, last_exit_code: i32, .. }`, unmodified), `ChatPanel::
  set_input(&mut self, text: String)` (unmodified), `ChatPanelView::{is_visible, toggle, submit}`
  (unmodified; `submit(&mut self, tokio_rt: &tokio::runtime::Runtime, cx: &mut Context<
  GpuiShellRoot>)`), `TextInput::set_content(&mut self, content: &str, cx: &mut Context<Self>)`
  (unmodified), `terminal.with_term(|t| ...)` (unmodified).
- Produces: `LeaderAction::{ExplainLastOutput, FixLastError}` (new variants, consumed by Task 2's
  pill callbacks too); `GpuiShellRoot::{explain_last_output, fix_last_error}(&mut self, window:
  &mut Window, cx: &mut Context<Self>)` (`pub(super)`, both consumed by Task 2).

- [ ] **Step 1: `src/gpui_shell/ai_actions.rs` -- the two query-building + dispatch methods**

```rust
// gpui chrome migration (M5b Task 1): "explain last output" / "fix last
// error" -- the two AI-query builders `Leader a e`/`Leader a f`, the
// command palette, and (Task 2) the suggestion pills all funnel into.
//
// Both real call sites (input.rs's 'a'-prefix leader continuation,
// palette_dispatch.rs's dispatch_palette_action, and Task 2's pill
// on_mouse_down callbacks) already have a real `&mut Window` in hand --
// unlike M5c's `SendToChat`, neither method here needs the deferred
// pending_*-drained-at-render() pattern.

use gpui::{Context, Window};

use crate::llm::shell_context::ShellContext;
use crate::term::Terminal;

use super::GpuiShellRoot;

impl GpuiShellRoot {
    /// `Leader a e` / palette "Explain Last Output" / the zero-state and
    /// post-response "Explain command"/"Explain more" pills (Task 2).
    pub(super) fn explain_last_output(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let output = self.last_terminal_lines(30);
        if output.is_empty() {
            return;
        }
        let query = format!("Explain this terminal output:\n```\n{output}\n```");
        self.run_ai_query(query, window, cx);
    }

    /// `Leader a f` / palette "Fix Last Error" / the zero-state and
    /// post-response "Fix last error" pills (Task 2).
    pub(super) fn fix_last_error(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let output = self.last_terminal_lines(30);
        let ctx = ShellContext::load();
        let query = match &ctx {
            Some(c) if !c.last_command.is_empty() => format!(
                "The command `{}` failed (exit code {}). Output:\n```\n{output}\n```\nHow do I \
                 fix this?",
                c.last_command, c.last_exit_code
            ),
            _ => format!(
                "This command failed. Output:\n```\n{output}\n```\nHow do I fix this?"
            ),
        };
        self.run_ai_query(query, window, cx);
    }

    /// Shared tail: open the panel if needed, drive `panel.input` (NOT the
    /// composer's own `TextInput` -- `submit()` reads from `panel.input`,
    /// confirmed against `stream.rs`'s own `handle_chat_composer_submit`),
    /// clear the composer's visible text to keep the real widget in sync
    /// (a discrepancy the wgpu build doesn't have, since it has no
    /// separate composer widget), then submit.
    fn run_ai_query(&mut self, query: String, window: &mut Window, cx: &mut Context<Self>) {
        if !self.chat.is_visible() {
            self.chat.toggle(window, cx);
        }
        self.chat.panel.set_input(query);
        self.chat
            .composer
            .update(cx, |input, cx| input.set_content("", cx));
        self.chat.submit(&self.tokio_rt, cx);
    }

    /// Read the bottom `n` visible terminal rows of the focused pane,
    /// joined with `\n`, trimmed. Ported from `Mux::last_terminal_lines`
    /// (`src/app/mux/mod.rs:611-627`), adapted to read the focused
    /// `Terminal` directly (same adaptation style M5c's `blocks.rs::
    /// row_text_and_absolute_row` already used for a sibling grid read).
    fn last_terminal_lines(&self, n: usize) -> String {
        let active_ws = self.workspaces.active();
        let active_tid = active_ws.tab_panes[active_ws.tabs.active_index()].focused_terminal;
        let Some(terminal) = self.terminals.get(&active_tid) else {
            return String::new();
        };
        last_terminal_lines_for(terminal, n)
    }
}

fn last_terminal_lines_for(terminal: &Terminal, n: usize) -> String {
    terminal.with_term(|term| {
        use alacritty_terminal::grid::Dimensions;
        use alacritty_terminal::index::{Column, Line};
        let rows = term.screen_lines();
        let cols = term.columns();
        let start = rows.saturating_sub(n);
        let mut lines = Vec::new();
        for row in start..rows {
            let mut text = String::new();
            for col in 0..cols {
                let cell = &term.grid()[Line(row as i32)][Column(col)];
                text.push(if cell.c == '\0' { ' ' } else { cell.c });
            }
            lines.push(text.trim_end().to_string());
        }
        lines.join("\n").trim().to_string()
    })
}
```

- [ ] **Step 2: `mod.rs` -- register the module**

Add `mod ai_actions;` in alphabetical order (right after `mod ai_block;`, right before `mod
blocks;` -- read `mod.rs`'s current module-declaration block first to confirm exact position).

- [ ] **Step 3: `leader.rs` -- the two new `LeaderAction` variants**

Find the current `pub enum LeaderAction { ... ToggleAiPanel, ... OpenCommandPalette, }` (18
variants) and add two more, right after `ToggleAiPanel`:

```rust
    ToggleAiPanel,
    ExplainLastOutput,
    FixLastError,
```

Find the matching `impl TryFrom<&str> for LeaderAction`'s match arms and add, right after
`"ToggleAiPanel" => Ok(LeaderAction::ToggleAiPanel),`:

```rust
            "ToggleAiPanel" => Ok(LeaderAction::ToggleAiPanel),
            "ExplainLastOutput" => Ok(LeaderAction::ExplainLastOutput),
            "FixLastError" => Ok(LeaderAction::FixLastError),
```

- [ ] **Step 4: `leader_dispatch.rs` -- the two new dispatch arms**

Find `dispatch_leader_action`'s `match action { ... LeaderAction::ToggleAiPanel => { ... } ...
}` and add two arms right after the `ToggleAiPanel` arm's closing `}`:

```rust
            LeaderAction::ExplainLastOutput => self.explain_last_output(window, cx),
            LeaderAction::FixLastError => self.fix_last_error(window, cx),
```

- [ ] **Step 5: `input.rs` -- wire the `'a'`-prefix continuation**

Find the exact current block:

```rust
            if let Some(prefix) = self.leader_prefix.take() {
                if prefix == 'a' && event.keystroke.key == "a" {
                    self.dispatch_leader_action(LeaderAction::ToggleAiPanel, window, cx);
                }
                if prefix == 'e' && event.keystroke.key == "e" {
                    self.dispatch_leader_action(LeaderAction::ToggleWorkspaceSidebar, window, cx);
                }
                if prefix == 'W' {
                    let action = match event.keystroke.key.as_str() {
                        "&" => Some(LeaderAction::CloseWorkspace),
                        "," => Some(LeaderAction::RenameWorkspace),
                        "j" => Some(LeaderAction::NextWorkspace),
                        "k" => Some(LeaderAction::PrevWorkspace),
                        _ => None,
                    };
                    if let Some(action) = action {
                        self.dispatch_leader_action(action, window, cx);
                    }
                }
                // Every other `a`-prefix subkey (c/e/f/z in the wgpu build)
                // is out of scope for this milestone -- see leader.rs's doc
                // comment and the M3b plan's Scope section. An unrecognized
                // subkey is simply dropped, matching the wgpu build's own
                // `_ => {}` fallthrough.
                return;
            }
```

Replace with:

```rust
            if let Some(prefix) = self.leader_prefix.take() {
                if prefix == 'a' {
                    match event.keystroke.key.as_str() {
                        "a" => self.dispatch_leader_action(LeaderAction::ToggleAiPanel, window, cx),
                        "e" => {
                            self.dispatch_leader_action(LeaderAction::ExplainLastOutput, window, cx)
                        }
                        "f" => self.dispatch_leader_action(LeaderAction::FixLastError, window, cx),
                        // `c`/`z` (ClearAiContext/UndoLastWrite) stay out of
                        // scope -- both ACP/tool-calling-dependent, per this
                        // milestone's own spec §6. Dropped, matching the
                        // wgpu build's own `_ => {}` fallthrough.
                        _ => {}
                    }
                }
                if prefix == 'e' && event.keystroke.key == "e" {
                    self.dispatch_leader_action(LeaderAction::ToggleWorkspaceSidebar, window, cx);
                }
                if prefix == 'W' {
                    let action = match event.keystroke.key.as_str() {
                        "&" => Some(LeaderAction::CloseWorkspace),
                        "," => Some(LeaderAction::RenameWorkspace),
                        "j" => Some(LeaderAction::NextWorkspace),
                        "k" => Some(LeaderAction::PrevWorkspace),
                        _ => None,
                    };
                    if let Some(action) = action {
                        self.dispatch_leader_action(action, window, cx);
                    }
                }
                return;
            }
```

- [ ] **Step 6: `palette_dispatch.rs` -- un-filter, add real arms**

In `gpui_shell_actions`'s `matches!` list, add, right after `Action::RestoreWorkspace(_)`:

```rust
                    | Action::ExplainLastOutput
                    | Action::FixLastError
```

In `dispatch_palette_action`'s `match action { ... }`, add two arms right before the trailing `_
=> {}`:

```rust
            Action::ExplainLastOutput => self.explain_last_output(window, cx),
            Action::FixLastError => self.fix_last_error(window, cx),
```

- [ ] **Step 7: Build, test, verify**

Run: `cargo build 2>&1 | tail -100`, `cargo test --lib 2>&1 | tail -10` (expect 234/234 unchanged
-- pure wiring, no new tests), `cargo fmt` then `cargo fmt --check` (clean), `cargo clippy
--all-features -- -D warnings` (clean), `./scripts/ci-local.sh` (exit 0).

Run `wc -l src/gpui_shell/mod.rs src/gpui_shell/leader.rs src/gpui_shell/leader_dispatch.rs
src/gpui_shell/input.rs src/gpui_shell/palette_dispatch.rs src/gpui_shell/ai_actions.rs` and note
results; flag any 400-line overshoot in your report's CONCERNS rather than splitting yourself.

Dogfood is not possible from this environment -- note in your report that this is pending manual
dogfood (reproduce the spec's §7 "`Leader a e` / `Leader a f`" checklist).

- [ ] **Step 8: Commit**

```bash
git add src/gpui_shell/ai_actions.rs src/gpui_shell/mod.rs src/gpui_shell/leader.rs src/gpui_shell/leader_dispatch.rs src/gpui_shell/input.rs src/gpui_shell/palette_dispatch.rs
git commit -m "feat: Wire Leader a e/f and their palette entries (M5b Task 1)."
```

---

## Task 2: Suggestion pills (zero-state + post-response)

**Tier: cheap.** Pure rendering plus two thin callbacks into Task 1's own methods; no new state.

**Files:**
- Modify: `src/gpui_shell/chat_panel/render.rs`
- Modify: `src/gpui_shell/chat_panel/mod.rs`
- Modify: `src/gpui_shell/render_callbacks.rs`
- Modify: `src/gpui_shell/render.rs`

**Interfaces:**
- Consumes: `GpuiShellRoot::{explain_last_output, fix_last_error}` (Task 1), `ChatPanel::{
  messages, streaming_buf, show_suggestions}` (unmodified, already `pub`).
- Produces: `chat_panel::ChatPillCallback = Rc<dyn Fn(&mut Window, &mut App)>` (new type alias,
  `pub(super)`); `render_chat_panel`'s signature gains two new parameters, consumed by `render.rs`'s
  own call site.

- [ ] **Step 1: `chat_panel/mod.rs` -- the shared callback type**

Add, right after the existing `pub use render::{render_chat_panel, PANEL_WIDTH_PX};` line:

```rust
pub(super) use render::ChatPillCallback;
```

- [ ] **Step 2: `chat_panel/render.rs` -- imports and the callback type**

Replace the existing `use gpui::{div, prelude::*, px, Div, FontWeight};` line with:

```rust
use gpui::{div, prelude::*, px, App, Div, FontWeight, MouseButton, MouseDownEvent, Window};
use std::rc::Rc;
```

(`Rc` is not re-exported by `gpui` -- confirmed against this codebase's own established
convention, e.g. `pane_view.rs`/`context_menu.rs` both `use std::rc::Rc;` separately from their
`gpui::{...}` import.)

Add, right after `pub const PANEL_WIDTH_PX: f32 = 480.0;`:

```rust
/// Called on a suggestion-pill click ("Fix last error" / "Explain
/// command"/"Explain more", both the zero-state's and the post-response
/// row's) -- built where `cx` is in scope (`render_callbacks.rs`,
/// `GpuiShellRoot::render`'s own indirect caller), same "callback passed
/// down as a render parameter" shape `render_tab_bar`'s `on_select`/
/// `render_context_menu`'s `on_action` already use.
pub(super) type ChatPillCallback = Rc<dyn Fn(&mut Window, &mut App)>;
```

- [ ] **Step 3: `chat_panel/render.rs` -- thread the two new parameters through**

Replace:

```rust
pub fn render_chat_panel(view: &ChatPanelView, llm: &LlmConfig, colors: &ColorScheme) -> Div {
    div()
        .flex()
        .flex_col()
        .flex_shrink_0()
        .h_full()
        .w(px(PANEL_WIDTH_PX))
        .bg(to_rgba(colors.ui_surface))
        .border_l_1()
        .border_color(to_rgba(colors.ui_border))
        .child(render_header(&view.panel, llm, colors))
        .child(render_message_list(&view.panel, colors))
        .child(render_composer(view, colors))
}
```

with:

```rust
pub fn render_chat_panel(
    view: &ChatPanelView,
    llm: &LlmConfig,
    colors: &ColorScheme,
    on_fix_last_error: ChatPillCallback,
    on_explain_last_output: ChatPillCallback,
) -> Div {
    div()
        .flex()
        .flex_col()
        .flex_shrink_0()
        .h_full()
        .w(px(PANEL_WIDTH_PX))
        .bg(to_rgba(colors.ui_surface))
        .border_l_1()
        .border_color(to_rgba(colors.ui_border))
        .child(render_header(&view.panel, llm, colors))
        .child(render_message_list(
            &view.panel,
            colors,
            on_fix_last_error,
            on_explain_last_output,
        ))
        .child(render_composer(view, colors))
}
```

- [ ] **Step 4: `chat_panel/render.rs` -- the shared pill helper**

Add, right after `render_message_body_lines`'s closing `}`:

```rust
/// One clickable suggestion pill -- shared by the zero-state's two pills
/// and the post-response row's two pills (Step 5/6). Real gpui `.hover()`
/// (already used once, M4c's context-menu rows) replaces the wgpu build's
/// manual `zero_state_hover`/`suggestion_hover` tracking entirely -- both
/// fields stay permanently unread by `gpui_shell`.
fn render_pill(label: &str, on_click: ChatPillCallback, colors: &ColorScheme) -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .justify_center()
        .mx_2()
        .px_3()
        .py_1()
        .rounded_md()
        .border_1()
        .border_color(to_rgba(colors.ui_border))
        .bg(to_rgba(colors.ui_surface_hover))
        .text_color(to_rgba(colors.ui_muted))
        .cursor_pointer()
        .hover(|el| {
            el.bg(to_rgba(colors.ui_surface_active))
                .border_color(to_rgba(colors.ui_accent))
                .text_color(to_rgba(colors.foreground))
        })
        .on_mouse_down(MouseButton::Left, move |_: &MouseDownEvent, window, cx| {
            on_click(window, cx)
        })
        .child(label.to_string())
}
```

Confirm `colors.ui_surface_hover`/`ui_surface_active`/`ui_border`/`ui_accent`/`ui_muted`/
`foreground` are all real `ColorScheme` fields before using them (all already used elsewhere in
this same file/`context_menu.rs` -- re-check `src/config/schema.rs`'s real field list if any name
doesn't match, rather than guessing).

- [ ] **Step 5: `chat_panel/render.rs` -- zero-state pills**

Replace:

```rust
fn render_message_list(panel: &ChatPanel, colors: &ColorScheme) -> impl IntoElement {
    let mut list = div()
        .id("chat-panel-messages")
        .flex()
        .flex_col()
        .flex_1()
        .min_h_0()
        .overflow_y_scroll()
        .gap_3()
        .px_3()
        .py_2()
        .font_family(font_state::font_family())
        .text_size(px(font_state::font_size()));

    if panel.messages.is_empty() && panel.streaming_buf.is_empty() {
        list = list.child(
            div()
                .text_color(to_rgba(colors.ui_muted))
                .child("Ask a question to get started."),
        );
    }
```

with:

```rust
fn render_message_list(
    panel: &ChatPanel,
    colors: &ColorScheme,
    on_fix_last_error: ChatPillCallback,
    on_explain_last_output: ChatPillCallback,
) -> impl IntoElement {
    let mut list = div()
        .id("chat-panel-messages")
        .flex()
        .flex_col()
        .flex_1()
        .min_h_0()
        .overflow_y_scroll()
        .gap_3()
        .px_3()
        .py_2()
        .font_family(font_state::font_family())
        .text_size(px(font_state::font_size()));

    if panel.messages.is_empty() && panel.streaming_buf.is_empty() {
        list = list.child(
            div()
                .flex()
                .flex_col()
                .items_center()
                .gap_2()
                .py_4()
                .child(div().text_color(to_rgba(colors.ui_accent)).child("\u{2726}"))
                .child(
                    div()
                        .text_color(to_rgba(colors.ui_muted))
                        .child("Ask a question below"),
                )
                .child(render_pill(
                    "Fix last error",
                    on_fix_last_error.clone(),
                    colors,
                ))
                .child(render_pill(
                    "Explain command",
                    on_explain_last_output.clone(),
                    colors,
                )),
        );
    }
```

- [ ] **Step 6: `chat_panel/render.rs` -- post-response pills**

Find the tail of `render_message_list` (currently ending `if !panel.streaming_buf.is_empty() { ...
} list }`) and add a `show_suggestions` branch right after that `if`, before the final `list`:

```rust
    if !panel.streaming_buf.is_empty() {
        let lines = parse_markdown(
            &panel.streaming_buf,
            MARKDOWN_WRAP_WIDTH,
            &mut ParseState::default(),
        );
        list = list.child(render_message_body_lines(&lines, colors));
    }

    if panel.show_suggestions {
        list = list.child(
            div()
                .flex()
                .flex_row()
                .justify_center()
                .gap_2()
                .py_2()
                .child(render_pill("Fix last error", on_fix_last_error, colors))
                .child(render_pill("Explain more", on_explain_last_output, colors)),
        );
    }

    list
}
```

(The zero-state branch above already `.clone()`s both callbacks since it's not the last use;
this branch consumes the originals -- confirmed mutually exclusive by `ChatPanel`'s own invariant,
`show_suggestions` is only ever set `true` after at least one assistant message exists, so it can
never be `true` at the same time `panel.messages.is_empty()` triggers the zero-state branch above,
meaning at most one of the two `render_pill` pairs is ever actually built per call -- but both
code paths still need their own owned `ChatPillCallback`, hence the zero-state branch's `.clone()`
calls.)

- [ ] **Step 7: `render_callbacks.rs` -- the two new callbacks**

Read the file's current `build_frame_callbacks` in full first (its own return-tuple type and the
`view.clone()` pattern every existing callback already uses). Change the function's return-type
tuple to add two more entries, and add two new callback bindings following the exact same
weak-handle pattern as `on_context_action`:

```rust
pub(super) fn build_frame_callbacks(
    cx: &mut Context<GpuiShellRoot>,
) -> (
    pane_view::PaneFocusCallback,
    pane_view::SeparatorDragCallback,
    context_menu::RightClickCallback,
    context_menu::ContextActionCallback,
    context_menu::ContextMenuCloseCallback,
    chat_panel::ChatPillCallback,
    chat_panel::ChatPillCallback,
) {
```

(Add `chat_panel` to the existing `use super::{context_menu, pane_view, GpuiShellRoot};` line --
change it to `use super::{chat_panel, context_menu, pane_view, GpuiShellRoot};`.)

Add, right after the existing `on_close_context_menu` binding and before the final return tuple:

```rust
    let fix_view = view.clone();
    let on_fix_last_error: chat_panel::ChatPillCallback = Rc::new(move |window, cx| {
        fix_view
            .update(cx, |root, cx| root.fix_last_error(window, cx))
            .ok();
    });

    let explain_view = view;
    let on_explain_last_output: chat_panel::ChatPillCallback = Rc::new(move |window, cx| {
        explain_view
            .update(cx, |root, cx| root.explain_last_output(window, cx))
            .ok();
    });
```

`view` was previously moved into `close_menu_view` at the end of the existing chain (`let
close_menu_view = view;`) -- change that line to `let close_menu_view = view.clone();` so `view`
stays available for these two new bindings; `explain_view = view` (a move, not a clone) is fine as
the LAST use.

Update the final return tuple:

```rust
    (
        on_focus,
        on_drag,
        on_right_click,
        on_context_action,
        on_close_context_menu,
        on_fix_last_error,
        on_explain_last_output,
    )
}
```

- [ ] **Step 8: `render.rs` -- thread the two new callbacks through**

Find the existing destructuring `let (on_focus, on_drag, on_right_click, on_context_action,
on_close_context_menu) = render_callbacks::build_frame_callbacks(cx);` and update it:

```rust
        let (
            on_focus,
            on_drag,
            on_right_click,
            on_context_action,
            on_close_context_menu,
            on_fix_last_error,
            on_explain_last_output,
        ) = render_callbacks::build_frame_callbacks(cx);
```

Find the existing `render_chat_panel(&self.chat, &self.config.llm, &self.config.colors)` call and
update it:

```rust
                let panel = chat_panel::render_chat_panel(
                    &self.chat,
                    &self.config.llm,
                    &self.config.colors,
                    on_fix_last_error,
                    on_explain_last_output,
                );
```

- [ ] **Step 9: Build, test, verify**

Run: `cargo build 2>&1 | tail -100`, `cargo test --lib 2>&1 | tail -10` (234/234 unchanged),
`cargo fmt` then `cargo fmt --check` (clean), `cargo clippy --all-features -- -D warnings`
(clean), `./scripts/ci-local.sh` (exit 0).

Run `wc -l src/gpui_shell/chat_panel/render.rs src/gpui_shell/chat_panel/mod.rs
src/gpui_shell/render_callbacks.rs src/gpui_shell/render.rs` and note results; flag any 400-line
overshoot in CONCERNS and fix it yourself.

Dogfood note: pending manual verification (spec's §7 "Suggestion pills" checklist).

- [ ] **Step 10: Commit**

```bash
git add src/gpui_shell/chat_panel/render.rs src/gpui_shell/chat_panel/mod.rs src/gpui_shell/render_callbacks.rs src/gpui_shell/render.rs
git commit -m "feat: Add the chat panel's zero-state and post-response suggestion pills (M5b Task 2)."
```

---

## Task 3: File attachment picker

**Tier: standard.** New async-scan wiring (a new owning-struct field + two new methods), a new
key-guard mode, and new in-flow rendering (attached-files chips + the picker list itself) --
genuinely more surface than Tasks 1-2, not a mechanical port.

**Files:**
- Modify: `src/gpui_shell/chat_panel/mod.rs`
- Modify: `src/gpui_shell/chat_panel/render.rs`
- Create: `src/gpui_shell/chat_panel/file_picker.rs`
- Modify: `src/gpui_shell/input.rs`
- Modify: `src/gpui_shell/poll.rs`
- Modify: `src/gpui_shell/leader_dispatch.rs`

**Interfaces:**
- Consumes: `crate::llm::chat_panel::scan_files(dir: &Path, max_depth: usize) -> Vec<PathBuf>`
  (pure, already `pub use`d at `crate::llm::chat_panel::scan_files`), `ChatPanel::{
  file_picker_open, file_picker_query, file_picker_items, file_picker_cursor, attached_files,
  attached_file_chars}` (all pre-existing `pub` fields), `ChatPanel::{close_file_picker,
  picker_type_char, picker_backspace, picker_move_up, picker_move_down, picker_confirm,
  filtered_picker_items, attach_file, detach_file, init_default_files}` (all pre-existing `pub`
  methods -- callable despite living in a private `picker` submodule, confirmed by the wgpu
  build's own cross-module calls to the same methods).
- Produces: `ChatPanelView::{open_file_picker_async, poll_file_scan}` (both `pub(super)`, `pub(
  super) fn open_file_picker_async(&mut self, cwd: PathBuf)`, `pub(super) fn poll_file_scan(&mut
  self) -> bool`); `GpuiShellRoot::maybe_handle_file_picker_key(&mut self, event: &KeyDownEvent,
  cx: &mut Context<Self>) -> bool` (`pub(super)`).

- [ ] **Step 1: `chat_panel/mod.rs` -- the new field**

Add `file_scan_rx: Option<crossbeam_channel::Receiver<Vec<std::path::PathBuf>>>,` to
`ChatPanelView`'s struct, right after the existing `in_flight: Option<tokio::task::JoinHandle<
()>>,` field.

Add `file_scan_rx: None,` to the `Self { ... }` construction in `ChatPanelView::new`, right after
the existing `in_flight: None,` line.

Add `mod file_picker;` in alphabetical order (right after `pub(super) mod markdown;`, right before
`mod render;` -- confirm exact position against the file's real current module-declaration lines).

- [ ] **Step 2: `chat_panel/file_picker.rs` -- open/poll**

```rust
// gpui chrome migration (M5b Task 3): the composer's file attachment
// picker -- async directory scan + fuzzy filter + attach/detach, all
// driven through already-ported, already-pure ChatPanel/picker.rs
// methods (src/llm/chat_panel/picker.rs). Mirrors UiManager::
// open_file_picker_async/poll_file_scan (src/app/ui/mod.rs:650-680)
// exactly, minus the winit-specific bits neither needs.

use std::path::PathBuf;

use super::ChatPanelView;

impl ChatPanelView {
    /// Open the picker in-place (composer keeps focus; see `input.rs`'s
    /// own key-guard doc comment on why this is a mode flag, not a real
    /// widget focus change) and kick off a background scan of `cwd`. The
    /// picker shows immediately with an empty list while the scan runs.
    pub(super) fn open_file_picker_async(&mut self, cwd: PathBuf) {
        self.panel.file_picker_query.clear();
        self.panel.file_picker_cursor = 0;
        self.panel.file_picker_open = true;
        self.panel.file_picker_items.clear();

        let (tx, rx) = crossbeam_channel::bounded(1);
        self.file_scan_rx = Some(rx);
        std::thread::spawn(move || {
            let mut items = crate::llm::chat_panel::scan_files(&cwd, 3);
            items.sort();
            let _ = tx.send(items);
        });
    }

    /// Drain a completed scan into `panel.file_picker_items`. Returns
    /// `true` if it updated anything (caller should `cx.notify()`).
    /// Called from `poll.rs`'s existing 33ms tick as `this.chat.
    /// poll_file_scan()`.
    pub(super) fn poll_file_scan(&mut self) -> bool {
        let Some(rx) = &self.file_scan_rx else {
            return false;
        };
        match rx.try_recv() {
            Ok(items) => {
                self.file_scan_rx = None;
                self.panel.file_picker_items = items;
                true
            }
            Err(_) => false,
        }
    }
}
```

- [ ] **Step 3: `GpuiShellRoot` gains the key guard**

Add to `chat_panel/mod.rs`, inside a new `impl GpuiShellRoot { ... }` block at the bottom of the
file (a `GpuiShellRoot`-receiver method living in `chat_panel/mod.rs` matches the existing
convention -- `stream.rs`'s own `on_composer_event`/`handle_chat_composer_submit` are `impl
GpuiShellRoot` methods defined inside the `chat_panel` directory too):

```rust
impl GpuiShellRoot {
    /// The file picker's own key guard, called from `input.rs`'s
    /// `on_key_down`. Returns `true` if the key was consumed. Keyed on
    /// `self.chat.panel.file_picker_open` (a mode flag), not
    /// `is_focused(window)` -- see this plan's own Global Constraints for
    /// why that's correct here, matching `InfoOverlay`'s own precedent.
    pub(super) fn maybe_handle_file_picker_key(
        &mut self,
        event: &gpui::KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.chat.panel.file_picker_open {
            return false;
        }
        let key = event.keystroke.key.as_str();
        if key == "escape" || key == "tab" {
            self.chat.panel.close_file_picker();
        } else if key == "enter" {
            let cwd = self.cached_cwd.clone().unwrap_or_default();
            let filtered: Vec<std::path::PathBuf> = self
                .chat
                .panel
                .filtered_picker_items()
                .into_iter()
                .cloned()
                .collect();
            self.chat.panel.picker_confirm(&cwd, &filtered);
        } else if key == "up" {
            self.chat.panel.picker_move_up();
        } else if key == "down" {
            let len = self.chat.panel.filtered_picker_items().len();
            self.chat.panel.picker_move_down(len);
        } else if key == "backspace" {
            self.chat.panel.picker_backspace();
        } else if !event.keystroke.modifiers.platform && !event.keystroke.modifiers.control {
            if key == "space" {
                self.chat.panel.picker_type_char(' ');
            } else if key.chars().count() == 1 {
                self.chat.panel.picker_type_char(key.chars().next().unwrap());
            }
        }
        cx.notify();
        true
    }
}
```

`Context` needs to be in scope -- confirm the file's existing `use gpui::{App, AppContext,
Context, Entity, Focusable, Window};` import already covers it (it does, per this plan's own
earlier verification of `chat_panel/mod.rs`'s real current imports).

- [ ] **Step 4: `input.rs` -- the early guard + the Tab-opens-picker check**

Find the existing:

```rust
        // InfoOverlay intercepts all keys (checked first, on top visually).
        // See `info_overlay.rs`'s `maybe_handle_info_overlay_key` doc.
        if self.maybe_handle_info_overlay_key(event, cx) {
            return;
        }
```

Add, right after it:

```rust

        // File picker key guard -- see `chat_panel/mod.rs`'s
        // `maybe_handle_file_picker_key` doc comment for why this is
        // mode-keyed rather than focus-keyed.
        if self.maybe_handle_file_picker_key(event, cx) {
            return;
        }
```

Find the existing chat-composer guard:

```rust
        if self.chat.composer_focused(window, cx) {
            return;
        }
```

Replace with:

```rust
        if self.chat.composer_focused(window, cx) {
            // Tab opens the file picker (the composer's own real focus
            // never changes -- see `maybe_handle_file_picker_key`'s own
            // doc comment). `TextInput` has no `Tab` binding of its own
            // (confirmed against `text_input/mod.rs`'s key-context
            // registration), so this is the only place Tab is ever
            // observed while the composer holds focus.
            if event.keystroke.key == "tab" {
                let cwd = self.cached_cwd.clone().unwrap_or_default();
                self.chat.open_file_picker_async(cwd);
                cx.notify();
            }
            return;
        }
```

- [ ] **Step 5: `poll.rs` -- drive `poll_file_scan`**

Find the existing `if this.poll_branch_scan() { should_notify = true; }` (M5c Task 4) and add,
right after it:

```rust
                    if this.chat.poll_file_scan() {
                        should_notify = true;
                    }
```

- [ ] **Step 6: `leader_dispatch.rs` -- auto-attach `AGENTS.md` on open**

Find the existing `LeaderAction::ToggleAiPanel` arm:

```rust
            LeaderAction::ToggleAiPanel => {
                self.chat.toggle(window, cx);
                // `toggle` only ever moves focus TO the composer (opening);
                // closing deliberately returns none, mirroring
                // `end_tab_rename`'s division of labor. This is the other
                // half: send focus back to the terminal right here rather
                // than waiting on render()'s guard, which can't tell "the
                // panel just closed" from "the composer still holds a stale
                // focus handle" -- gpui doesn't clear a `FocusHandle`'s
                // focused status just because its element left the tree.
                if !self.chat.is_visible() {
                    window.focus(&self.focus_handle);
                }
            }
```

Replace with:

```rust
            LeaderAction::ToggleAiPanel => {
                self.chat.toggle(window, cx);
                if self.chat.is_visible() {
                    // `init_default_files` is idempotent (checks `attached_
                    // files.contains` before adding), matching the wgpu
                    // build's own `open_panel_with_context`, which calls it
                    // unconditionally on every open -- safe to call here
                    // every time the panel opens, not just the first time.
                    if let Some(cwd) = self.cached_cwd.clone() {
                        self.chat.panel.init_default_files(&cwd);
                    }
                } else {
                    // `toggle` only ever moves focus TO the composer
                    // (opening); closing deliberately returns none,
                    // mirroring `end_tab_rename`'s division of labor. This
                    // is the other half: send focus back to the terminal
                    // right here rather than waiting on render()'s guard,
                    // which can't tell "the panel just closed" from "the
                    // composer still holds a stale focus handle" -- gpui
                    // doesn't clear a `FocusHandle`'s focused status just
                    // because its element left the tree.
                    window.focus(&self.focus_handle);
                }
            }
```

- [ ] **Step 7: `chat_panel/render.rs` -- attached-files chips + the in-flow picker list**

Replace `render_composer`:

```rust
fn render_composer(view: &ChatPanelView, colors: &ColorScheme) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .flex_shrink_0()
        .gap_1()
        .border_t_1()
        .border_color(to_rgba(colors.ui_border))
        .px_3()
        .py_2()
        .child(
            div()
                .flex()
                .h(px(28.0))
                .items_center()
                .font_family(font_state::font_family())
                .text_size(px(font_state::font_size()))
                .text_color(to_rgba(colors.foreground))
                .child(view.composer.clone()),
        )
        .child(
            div()
                .font_family(font_state::font_family())
                .text_size(px(font_state::font_size()))
                .text_color(to_rgba(colors.ui_muted))
                .child("Enter to send  ·  /clear /skills /mcp /model /agent  ·  /q to close"),
        )
}
```

with:

```rust
fn render_composer(view: &ChatPanelView, colors: &ColorScheme) -> impl IntoElement {
    let mut root = div()
        .flex()
        .flex_col()
        .flex_shrink_0()
        .gap_1()
        .border_t_1()
        .border_color(to_rgba(colors.ui_border))
        .px_3()
        .py_2();

    if !view.panel.attached_files.is_empty() {
        let mut chips = div().flex().flex_row().flex_wrap().gap_1();
        for path in &view.panel.attached_files {
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| path.to_string_lossy().into_owned());
            chips = chips.child(
                div()
                    .px_2()
                    .py_0()
                    .rounded_md()
                    .bg(to_rgba(colors.ui_surface_hover))
                    .text_color(to_rgba(colors.ui_muted))
                    .text_size(px(11.0))
                    .child(format!("\u{1F4CE} {name}")),
            );
        }
        root = root.child(chips);
    }

    if view.panel.file_picker_open {
        let filtered = view.panel.filtered_picker_items();
        let mut picker_list = div()
            .flex()
            .flex_col()
            .max_h(px(160.0))
            .overflow_y_scroll()
            .border_1()
            .border_color(to_rgba(colors.ui_border))
            .rounded_md();
        for (idx, path) in filtered.iter().enumerate() {
            let is_cursor = idx == view.panel.file_picker_cursor;
            let is_attached = view.panel.attached_files.contains(path);
            let label = path.to_string_lossy().into_owned();
            let mut row = div()
                .px_2()
                .py_1()
                .text_size(px(font_state::font_size()))
                .text_color(to_rgba(colors.foreground));
            if is_cursor {
                row = row.bg(to_rgba(colors.ui_surface_active));
            }
            let prefix = if is_attached { "\u{2713} " } else { "  " };
            picker_list = picker_list.child(row.child(format!("{prefix}{label}")));
        }
        root = root.child(picker_list);
    }

    root.child(
        div()
            .flex()
            .h(px(28.0))
            .items_center()
            .font_family(font_state::font_family())
            .text_size(px(font_state::font_size()))
            .text_color(to_rgba(colors.foreground))
            .child(view.composer.clone()),
    )
    .child(
        div()
            .font_family(font_state::font_family())
            .text_size(px(font_state::font_size()))
            .text_color(to_rgba(colors.ui_muted))
            .child("Enter to send  ·  Tab to attach files  ·  /clear /skills /mcp /model /agent  ·  /q to close"),
    )
}
```

`ChatPanel::filtered_picker_items(&self) -> Vec<&PathBuf>` -- confirm this exact return type
against `src/llm/chat_panel/picker.rs` before using `.iter().enumerate()` on it (it's already a
`Vec`, so `.iter()` borrows it -- no ownership issue calling it twice in the same function if
needed, though this step only calls it once).

- [ ] **Step 8: Build, test, verify**

Run: `cargo build 2>&1 | tail -100`, `cargo test --lib 2>&1 | tail -10` (234/234 unchanged),
`cargo fmt` then `cargo fmt --check` (clean), `cargo clippy --all-features -- -D warnings`
(clean), `./scripts/ci-local.sh` (exit 0).

Run `wc -l src/gpui_shell/chat_panel/mod.rs src/gpui_shell/chat_panel/render.rs
src/gpui_shell/chat_panel/file_picker.rs src/gpui_shell/input.rs src/gpui_shell/poll.rs
src/gpui_shell/leader_dispatch.rs` and note results. `chat_panel/render.rs` has grown across
Tasks 2 and 3 both -- if it's now over 400, extract `render_pill`/the picker-list/chip-row
building into a new file (e.g. `chat_panel/render_extras.rs`) rather than leaving it, following
the same pattern used throughout this whole migration; re-export so `render_chat_panel`'s own
call sites are unaffected.

Dogfood note: pending manual verification (spec's §7 "File attachment picker" checklist).

- [ ] **Step 9: Commit**

```bash
git add src/gpui_shell/chat_panel/mod.rs src/gpui_shell/chat_panel/render.rs src/gpui_shell/chat_panel/file_picker.rs src/gpui_shell/input.rs src/gpui_shell/poll.rs src/gpui_shell/leader_dispatch.rs
git commit -m "feat: Add the composer's Tab-triggered file attachment picker (M5b Task 3)."
```

---

## Exit Criteria

- `Leader a e`/`Leader a f` (and their palette entries) submit a real AI query built from the
  last 30 visible terminal lines, `Leader a f` including the real failed command + exit code when
  `ShellContext` has one.
- The chat panel's zero-state (no messages yet) shows two hoverable, clickable pills running the
  same two queries; the same pair reappears after an assistant reply completes.
- `Tab` in the composer opens a file picker (async scan, fuzzy-filtered by typing, Up/Down/Enter/
  Backspace/Escape all work); attached files show as chips above the input row; opening the panel
  auto-attaches `AGENTS.md` when present.
- `scripts/ci-local.sh` (including `cargo fmt --check`) is green and the full `cargo test --lib`
  suite passes after each task.
- This completes M5b. M5a (ACP + tool-calling) remains, to get its own design/plan under the same
  M5 milestone.
