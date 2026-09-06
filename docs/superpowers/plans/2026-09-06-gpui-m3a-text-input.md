# M3a — Text Input Primitive Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a reusable, IME-capable text-input primitive in `src/gpui_shell/text_input/`, and prove it end-to-end by closing out M2's deferred `RenameTab`.

**Architecture:** Port gpui 0.2.2's own reference `TextInput` (`examples/input.rs`, 746 lines) as a real gpui `Entity` implementing `EntityInputHandler` — deliberately departing from `gpui_shell`'s usual "plain state struct + free `render_*` function" pattern, because focus handling and IME registration both require an entity. Adapted in four ways: theme colors come from `ColorScheme` rather than hardcoded literals; all key bindings are scoped to a `"TextInput"` key context so they cannot hijack terminal keys; the entity emits `Submit`/`Cancel` events so parents can react; and the pure string helpers become free functions so they are unit-testable without a gpui context.

**Tech Stack:** Rust 2021, gpui 0.2.2, unicode-segmentation 1.13.

**Spec:** `docs/superpowers/specs/2026-09-06-gpui-m3-sidebars-design.md` (§2 M3a, §3.1)

## Global Constraints

- Module files stay under 400 lines (`AGENTS.md`); split when exceeded.
- Tests cover **logic only**. No tests for painting, layout, or hit-testing — those are dogfooded, because GPU windows cannot be captured from the agent sandbox.
- `bash scripts/ci-local.sh` must exit 0 after every task.
- `RUSTFLAGS="-D warnings" cargo build --bin gpui-petruterm --bin petruterm` must be clean (both binaries — shared code must not regress the shipping wgpu app).
- `cargo fmt --check` clean.
- Commit format: `type: Message.` — type one of `feat`/`fix`/`chore`/`refactor`.
- Dependencies: stable published releases only, never prerelease or git-tip.
- Baseline at plan time: **212 lib tests passing**, HEAD `8653d87`, branch `worktree-gpui-migration`. Do not create a worktree; one exists.

---

## File Structure

| File | Responsibility |
|---|---|
| `src/gpui_shell/text_input/mod.rs` (new) | `TextInput` entity: state, editing actions, `EntityInputHandler` (IME), `Render`, `Focusable`, `TextInputEvent`, `register_key_bindings`, and the free string helpers + their tests |
| `src/gpui_shell/text_input/element.rs` (new) | `TextElement` + `PrepaintState`: the custom `Element` that shapes the line and paints selection, text, and cursor |
| `src/gpui_shell/mod.rs` (modify) | declare `pub mod text_input;`; add the `tab_rename` field |
| `src/gpui_shell/actions.rs` (modify) | replace `RenameTab`'s logged no-op with a real implementation |
| `src/gpui_shell/render.rs` (modify) | fix the unconditional focus steal; render the rename input in the tab bar |
| `src/gpui_shell/tabs.rs` (modify) | let the tab bar host the rename element in place of the active tab's label |
| `src/bin/gpui_petruterm.rs` (modify) | call `text_input::register_key_bindings(cx)` |
| `Cargo.toml` (modify) | add `unicode-segmentation = "1.13"` |

---

### Task 1: The `TextInput` primitive

**Files:**
- Create: `src/gpui_shell/text_input/mod.rs`
- Create: `src/gpui_shell/text_input/element.rs`
- Modify: `Cargo.toml` (add one dependency)
- Modify: `src/gpui_shell/mod.rs` (add `pub mod text_input;` to the existing module-declaration block)

**Interfaces:**
- Consumes: `crate::config::schema::ColorScheme` (fields `foreground`, `ui_muted`, `ui_accent`, `ui_surface_active` — all `[f32; 4]`, all verified present at `src/config/schema.rs:291-303`); `super::pane_view::to_rgba([f32; 4]) -> gpui::Rgba` (the existing shared color converter every other `gpui_shell` render helper uses).
- Produces, for Task 2 and for M3b:
  - `TextInput::new(cx: &mut Context<Self>, colors: &ColorScheme, initial: impl Into<SharedString>, placeholder: impl Into<SharedString>) -> Self`
  - `TextInput::content(&self) -> &str`
  - `TextInput::set_content(&mut self, text: impl Into<SharedString>, cx: &mut Context<Self>)` — replaces content and moves the cursor to the end
  - `enum TextInputEvent { Submit, Cancel }`, with `impl EventEmitter<TextInputEvent> for TextInput`
  - `pub fn register_key_bindings(cx: &mut App)`
  - Free helpers (pure, no gpui): `offset_from_utf16(&str, usize) -> usize`, `offset_to_utf16(&str, usize) -> usize`, `previous_boundary(&str, usize) -> usize`, `next_boundary(&str, usize) -> usize`

**Reference:** `~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/gpui-0.2.2/examples/input.rs`. Read it in full before starting. You are porting it, not inventing it — keep its structure, method names, and IME logic. Port only lines 1–607; the file's `main()`, `InputExample`, and window setup (608–746) are demo scaffolding and must not be copied.

- [ ] **Step 1: Add the dependency**

`unicode-segmentation` is already in `Cargo.lock` at `1.13.3` (transitively, via `cosmic-text`), so this pins nothing new. Add to `Cargo.toml`'s `[dependencies]`, in the position that keeps the section's existing ordering:

```toml
unicode-segmentation = "1.13"
```

Verify it resolves to the version already locked, rather than pulling a new one:

```bash
cargo tree -i unicode-segmentation --depth 0
```
Expected: `unicode-segmentation v1.13.3`

- [ ] **Step 2: Write the failing tests for the pure string helpers**

These four helpers are free functions taking `&str` — **not** methods on `TextInput` as in the gpui example. That is a deliberate adaptation: methods would need a constructed entity, which needs an `App` context, which this project's convention ("tests for logic only") does not build test harnesses for. As free functions they are directly testable.

Create `src/gpui_shell/text_input/mod.rs` containing only this test module for now:

```rust
#[cfg(test)]
mod helper_tests {
    use super::{next_boundary, offset_from_utf16, offset_to_utf16, previous_boundary};

    #[test]
    fn utf16_offsets_round_trip_through_multibyte_text() {
        // "é" is 2 UTF-8 bytes / 1 UTF-16 unit; "日" is 3 / 1; "😀" is 4 / 2.
        let s = "aé日😀b";
        // Byte offsets of each char boundary: a=0, é=1, 日=3, 😀=6, b=10, end=11.
        for &utf8 in &[0usize, 1, 3, 6, 10, 11] {
            let utf16 = offset_to_utf16(s, utf8);
            assert_eq!(
                offset_from_utf16(s, utf16),
                utf8,
                "round trip failed at utf8 offset {utf8}"
            );
        }
        // The emoji really does occupy 2 UTF-16 units, so the mapping is not identity.
        assert_eq!(offset_to_utf16(s, 10), 5);
    }

    #[test]
    fn boundaries_step_over_whole_grapheme_clusters() {
        // A ZWJ family emoji is one grapheme but many bytes -- stepping by
        // char or by byte would land inside it and corrupt the string.
        let s = "a👨‍👩‍👧b";
        let after_a = next_boundary(s, 0);
        assert_eq!(after_a, 1);
        let after_family = next_boundary(s, after_a);
        assert_eq!(&s[after_a..after_family], "👨‍👩‍👧");
        assert_eq!(previous_boundary(s, after_family), after_a);
    }

    #[test]
    fn boundaries_clamp_at_the_ends() {
        let s = "abc";
        assert_eq!(previous_boundary(s, 0), 0);
        assert_eq!(next_boundary(s, s.len()), s.len());
    }
}
```

- [ ] **Step 3: Run the tests to verify they fail**

Run: `cargo test --lib text_input 2>&1 | tail -20`
Expected: FAIL — compile error, `cannot find function offset_to_utf16 in this scope` (and the other three).

- [ ] **Step 4: Implement the pure helpers**

Add above the test module in `src/gpui_shell/text_input/mod.rs`. Bodies are the gpui example's `offset_from_utf16`/`offset_to_utf16`/`previous_boundary`/`next_boundary` (`examples/input.rs:197-248`), with `self.content` replaced by the `content` parameter:

```rust
use unicode_segmentation::UnicodeSegmentation;

/// Byte offset for a UTF-16 offset. macOS's IME speaks UTF-16, Rust strings
/// are UTF-8, so every `EntityInputHandler` boundary crosses this conversion.
fn offset_from_utf16(content: &str, offset: usize) -> usize {
    let mut utf8_offset = 0;
    let mut utf16_count = 0;
    for ch in content.chars() {
        if utf16_count >= offset {
            break;
        }
        utf16_count += ch.len_utf16();
        utf8_offset += ch.len_utf8();
    }
    utf8_offset
}

/// UTF-16 offset for a byte offset -- the inverse of `offset_from_utf16`.
fn offset_to_utf16(content: &str, offset: usize) -> usize {
    let mut utf16_offset = 0;
    let mut utf8_count = 0;
    for ch in content.chars() {
        if utf8_count >= offset {
            break;
        }
        utf8_count += ch.len_utf8();
        utf16_offset += ch.len_utf16();
    }
    utf16_offset
}

/// Previous grapheme-cluster boundary. Grapheme, not char: a ZWJ emoji
/// sequence is several chars but one cursor stop, and stepping by char
/// would leave the cursor inside it.
fn previous_boundary(content: &str, offset: usize) -> usize {
    content
        .grapheme_indices(true)
        .rev()
        .find_map(|(idx, _)| (idx < offset).then_some(idx))
        .unwrap_or(0)
}

/// Next grapheme-cluster boundary. See `previous_boundary`.
fn next_boundary(content: &str, offset: usize) -> usize {
    content
        .grapheme_indices(true)
        .find_map(|(idx, _)| (idx > offset).then_some(idx))
        .unwrap_or(content.len())
}
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test --lib text_input 2>&1 | tail -20`
Expected: PASS, 3 passed.

- [ ] **Step 6: Port the `TextInput` entity**

Add to `src/gpui_shell/text_input/mod.rs`, above the tests. Port `examples/input.rs:33-259` (struct + editing methods) and `261-386` (`EntityInputHandler`), with these adaptations — each is required, none is optional:

1. **Colors from the theme.** The struct carries resolved colors instead of the example's hardcoded `hsla(0., 0., 0., 0.2)` / `gpui::blue()` / `rgba(0x3311ff30)`.
2. **The four helpers are free functions** (Step 4) — call them as `previous_boundary(&self.content, offset)`, etc.
3. **Add `Submit`/`Cancel`** actions and the matching event emissions. The example has no notion of committing or abandoning an edit; every consumer in this migration needs both.
4. **Add `content()` / `set_content()`** accessors, so a parent can seed the buffer and read the result.

```rust
use std::ops::Range;

use gpui::{
    actions, div, prelude::*, App, Bounds, ClipboardItem, Context, CursorStyle, EntityInputHandler,
    EventEmitter, FocusHandle, Focusable, KeyBinding, MouseButton, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, Pixels, Point, Rgba, ShapedLine, SharedString, UTF16Selection, Window,
};

use crate::config::schema::ColorScheme;

use super::pane_view::to_rgba;

mod element;
use element::TextElement;

actions!(
    text_input,
    [
        Backspace, Delete, Left, Right, SelectLeft, SelectRight, SelectAll, Home, End,
        ShowCharacterPalette, Paste, Cut, Copy, Submit, Cancel,
    ]
);

/// Emitted so a parent can react to the edit finishing. The example this is
/// ported from has no equivalent -- it is a standalone demo with nothing to
/// report to -- but every consumer here (tab rename, chat input, file-picker
/// query) needs to know when the user committed or abandoned the edit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TextInputEvent {
    Submit,
    Cancel,
}

impl EventEmitter<TextInputEvent> for TextInput {}

/// A single-line editable text field: real cursor, selection, clipboard, and
/// IME composition (marked-range) support.
///
/// A gpui `Entity` rather than this module's usual "plain struct + free
/// `render_*` function" shape, because `window.handle_input` needs a
/// `FocusHandle` and an `EntityInputHandler` implementor -- neither of which a
/// free function can provide. See the M3 design's §3.1.
pub struct TextInput {
    focus_handle: FocusHandle,
    content: SharedString,
    placeholder: SharedString,
    selected_range: Range<usize>,
    selection_reversed: bool,
    /// The IME's in-progress composition, underlined while active.
    marked_range: Option<Range<usize>>,
    last_layout: Option<ShapedLine>,
    last_bounds: Option<Bounds<Pixels>>,
    is_selecting: bool,
    text_color: Rgba,
    placeholder_color: Rgba,
    cursor_color: Rgba,
    selection_color: Rgba,
}

impl TextInput {
    pub fn new(
        cx: &mut Context<Self>,
        colors: &ColorScheme,
        initial: impl Into<SharedString>,
        placeholder: impl Into<SharedString>,
    ) -> Self {
        let content: SharedString = initial.into();
        let end = content.len();
        Self {
            focus_handle: cx.focus_handle(),
            content,
            placeholder: placeholder.into(),
            // Seeded with the cursor at the end, which is what every consumer
            // wants when opening an editor over existing text.
            selected_range: end..end,
            selection_reversed: false,
            marked_range: None,
            last_layout: None,
            last_bounds: None,
            is_selecting: false,
            text_color: to_rgba(colors.foreground),
            placeholder_color: to_rgba(colors.ui_muted),
            cursor_color: to_rgba(colors.ui_accent),
            selection_color: to_rgba(colors.ui_surface_active),
        }
    }

    pub fn content(&self) -> &str {
        &self.content
    }

    pub fn set_content(&mut self, text: impl Into<SharedString>, cx: &mut Context<Self>) {
        self.content = text.into();
        let end = self.content.len();
        self.selected_range = end..end;
        self.marked_range = None;
        cx.notify();
    }

    fn submit(&mut self, _: &Submit, _: &mut Window, cx: &mut Context<Self>) {
        cx.emit(TextInputEvent::Submit);
    }

    fn cancel(&mut self, _: &Cancel, _: &mut Window, cx: &mut Context<Self>) {
        cx.emit(TextInputEvent::Cancel);
    }
}
```

Then port, unchanged in behavior from the example: `left`, `right`, `select_left`, `select_right`, `select_all`, `home`, `end`, `backspace`, `delete`, `on_mouse_down`, `on_mouse_up`, `on_mouse_move`, `show_character_palette`, `paste`, `copy`, `cut`, `move_to`, `cursor_offset`, `index_for_mouse_position`, `select_to`, `range_to_utf16`, `range_from_utf16`, and the whole `impl EntityInputHandler for TextInput`. Keep `reset()` only if you use it; drop it otherwise rather than carrying dead code.

`element::TextElement` and the fields it reads (`content`, `placeholder`, `selected_range`, `marked_range`, `last_layout`, `last_bounds`, and the four colors) are in a child module, so the private fields are visible to it without any `pub` — do not widen them.

- [ ] **Step 7: Port the paint element**

Create `src/gpui_shell/text_input/element.rs` from `examples/input.rs:388-563`, with the hardcoded colors replaced by the entity's fields. Header comment in the house style (see `src/gpui_shell/status_bar/render.rs` for the pattern), noting it is a port of gpui's own example.

Key points to preserve exactly, because they are subtle:
- `prepaint` builds three `TextRun`s when `marked_range` is `Some`, so the IME's in-progress composition renders underlined. Filter out zero-length runs (`.filter(|run| run.len > 0)`) — a zero-length run panics the shaper.
- `paint` calls `window.handle_input(&focus_handle, ElementInputHandler::new(bounds, self.input.clone()), cx)`. **This is the line that makes IME work at all**; without it the entity's `EntityInputHandler` impl is never consulted.
- `paint` writes `last_layout`/`last_bounds` back onto the entity at the end. Mouse hit-testing reads them, so a click before the first paint is a no-op rather than a panic.
- The cursor quad is painted only when `focus_handle.is_focused(window)`.

- [ ] **Step 8: Implement `Render`, `Focusable`, and the key bindings**

Back in `mod.rs`. Port `examples/input.rs:565-607`, with the binding registration added:

```rust
/// Register this primitive's key bindings. Call once, from `main()`.
///
/// Every binding is scoped to the `"TextInput"` key context. This is NOT
/// optional and is the one place this port must not follow gpui's example,
/// which passes `None` (global) because its demo window contains nothing but
/// an input. Global bindings here would capture `backspace`, `left`, `right`,
/// `home`, `end` and the clipboard chords application-wide -- so they would
/// stop reaching the PTY, breaking ordinary typing in the terminal.
pub fn register_key_bindings(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("backspace", Backspace, Some("TextInput")),
        KeyBinding::new("delete", Delete, Some("TextInput")),
        KeyBinding::new("left", Left, Some("TextInput")),
        KeyBinding::new("right", Right, Some("TextInput")),
        KeyBinding::new("shift-left", SelectLeft, Some("TextInput")),
        KeyBinding::new("shift-right", SelectRight, Some("TextInput")),
        KeyBinding::new("cmd-a", SelectAll, Some("TextInput")),
        KeyBinding::new("home", Home, Some("TextInput")),
        KeyBinding::new("end", End, Some("TextInput")),
        KeyBinding::new("cmd-v", Paste, Some("TextInput")),
        KeyBinding::new("cmd-x", Cut, Some("TextInput")),
        KeyBinding::new("cmd-c", Copy, Some("TextInput")),
        KeyBinding::new("ctrl-cmd-space", ShowCharacterPalette, Some("TextInput")),
        KeyBinding::new("enter", Submit, Some("TextInput")),
        KeyBinding::new("escape", Cancel, Some("TextInput")),
    ]);
}

impl Focusable for TextInput {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for TextInput {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .size_full()
            // Must match the context string in `register_key_bindings`.
            .key_context("TextInput")
            .track_focus(&self.focus_handle(cx))
            .cursor(CursorStyle::IBeam)
            .on_action(cx.listener(Self::backspace))
            .on_action(cx.listener(Self::delete))
            .on_action(cx.listener(Self::left))
            .on_action(cx.listener(Self::right))
            .on_action(cx.listener(Self::select_left))
            .on_action(cx.listener(Self::select_right))
            .on_action(cx.listener(Self::select_all))
            .on_action(cx.listener(Self::home))
            .on_action(cx.listener(Self::end))
            .on_action(cx.listener(Self::show_character_palette))
            .on_action(cx.listener(Self::paste))
            .on_action(cx.listener(Self::cut))
            .on_action(cx.listener(Self::copy))
            .on_action(cx.listener(Self::submit))
            .on_action(cx.listener(Self::cancel))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_move(cx.listener(Self::on_mouse_move))
            .child(TextElement { input: cx.entity() })
    }
}
```

Note the example's own `div()` sets `bg`, `line_height`, `text_size` and wraps the element in a padded box. Do **not** copy that — sizing and background belong to whatever hosts the input, and the tab bar (Task 2) already has its own. Keep this render minimal.

- [ ] **Step 9: Declare the module**

In `src/gpui_shell/mod.rs`'s existing module-declaration block, keeping alphabetical order:

```rust
pub mod text_input;
```

- [ ] **Step 10: Run the full gate**

```bash
cargo build --bin gpui-petruterm --bin petruterm 2>&1 | tail -20
bash scripts/ci-local.sh 2>&1 | tail -30
RUSTFLAGS="-D warnings" cargo build --bin gpui-petruterm --bin petruterm 2>&1 | tail -20
cargo fmt --check
wc -l src/gpui_shell/text_input/*.rs
```
Expected: **215 lib tests passing** (212 baseline + 3 new), everything else clean, both new files under 400 lines.

`-D warnings` will flag `TextInput` as never constructed until Task 2 uses it. Resolve that by finishing Task 2 in the same session — do **not** add `#[allow(dead_code)]` to paper over it. If you must commit Task 1 separately, say so in your report rather than silently suppressing.

- [ ] **Step 11: Commit**

```bash
git add Cargo.toml Cargo.lock src/gpui_shell/text_input src/gpui_shell/mod.rs
git commit -m "feat: Add an IME-capable text-input primitive.

Ports gpui 0.2.2's own examples/input.rs TextInput as a real Entity
implementing EntityInputHandler -- the first surface in gpui_shell that
needs focus and IME registration, neither of which the module's usual
plain-struct-plus-free-render-function shape can provide.

Four deliberate adaptations: colors resolve from ColorScheme instead of
the example's hardcoded literals; every key binding is scoped to a
\"TextInput\" key context, because the example's global bindings would
capture backspace/arrows/clipboard chords application-wide and stop them
reaching the PTY; Submit/Cancel events let a parent observe the edit
finishing; and the UTF-16 and grapheme-boundary helpers are free
functions over &str so they unit-test without a gpui context."
```

---

### Task 2: Wire `RenameTab`, the primitive's first consumer

**Files:**
- Modify: `src/gpui_shell/mod.rs` (one field on `GpuiShellRoot`)
- Modify: `src/gpui_shell/actions.rs:223-228` (replace the logged no-op)
- Modify: `src/gpui_shell/render.rs:13` (the focus fix) and its tab-bar call
- Modify: `src/gpui_shell/tabs.rs` (`render_tab_bar` hosts the rename element)
- Modify: `src/bin/gpui_petruterm.rs` (register bindings)

**Interfaces:**
- Consumes, from Task 1: `TextInput::new`, `TextInput::content`, `TextInputEvent::{Submit, Cancel}`, `text_input::register_key_bindings`.
- Consumes, already present: `TabManager::rename_active(&mut self, title: impl Into<String>)` (`tabs.rs`); `TabManager::active_index()`; `TabManager::active_tab() -> Option<&Tab>` (`Tab` has a `title: String`).
- Produces: `GpuiShellRoot.tab_rename: Option<Entity<TextInput>>` — `Some` exactly while a rename is in progress.

- [ ] **Step 1: Add the field**

In `src/gpui_shell/mod.rs`'s `GpuiShellRoot` struct, after the `leader_map` field:

```rust
    /// The in-progress tab rename, `Some` only while `Leader ,` is being
    /// answered. Owning it here (rather than inside `TabManager`) keeps the
    /// tab data model free of gpui types, the same separation `StatusBar` and
    /// `PaneForest` already keep.
    tab_rename: Option<gpui::Entity<text_input::TextInput>>,
```

Initialize it in `GpuiShellRoot::new`, alongside the other field initializers:

```rust
            tab_rename: None,
```

- [ ] **Step 2: Implement the action**

In `src/gpui_shell/actions.rs`, replace the whole `LeaderAction::RenameTab` arm (currently lines 223-228, the `log::info!` no-op, together with the three-line comment above it at 219-222 that calls the deferral deliberate) with:

```rust
            LeaderAction::RenameTab => self.begin_tab_rename(cx),
```

Then add these two methods to the same `impl GpuiShellRoot` block:

```rust
    /// Open an editable field over the active tab's label, seeded with its
    /// current title and focused so the next keystroke goes to it.
    pub(super) fn begin_tab_rename(&mut self, cx: &mut Context<Self>) {
        let Some(title) = self.tabs.active_tab().map(|t| t.title.clone()) else {
            return;
        };
        let colors = self.config.colors.clone();
        let input = cx.new(|cx| text_input::TextInput::new(cx, &colors, title, "tab name"));

        // Subscribe before storing: the parent owns the outcome, so Enter and
        // Escape resolve here rather than inside the primitive, which has no
        // idea what is being renamed.
        cx.subscribe(&input, |this, input, event, cx| {
            match event {
                text_input::TextInputEvent::Submit => {
                    let name = input.read(cx).content().trim().to_string();
                    // An all-whitespace name would render as a blank pill with
                    // no way to tell which tab it is; treat it as a cancel.
                    if !name.is_empty() {
                        this.tabs.rename_active(name);
                    }
                }
                text_input::TextInputEvent::Cancel => {}
            }
            this.end_tab_rename(cx);
        })
        .detach();

        input.focus_handle(cx).focus(cx);
        self.tab_rename = Some(input);
        cx.notify();
    }

    /// Close the rename editor and hand focus back to the terminal.
    pub(super) fn end_tab_rename(&mut self, cx: &mut Context<Self>) {
        self.tab_rename = None;
        self.focus_handle.focus(cx);
        cx.notify();
    }
```

`cx.subscribe`'s closure receives `(&mut GpuiShellRoot, Entity<TextInput>, &TextInputEvent, &mut Context<GpuiShellRoot>)`. Add `use gpui::Focusable;` to `actions.rs` if `focus_handle(cx)` does not already resolve there.

- [ ] **Step 3: Fix the focus steal**

`src/gpui_shell/render.rs:13` currently reads `window.focus(&self.focus_handle);` and runs on **every** frame. The poll loop calls `cx.notify()` at ~30Hz, so as written it would rip focus back from the rename field roughly thirty times a second, and typing would appear to do nothing at all. Replace it with:

```rust
        // Skipped while a child owns focus. This runs every frame, and the
        // poll loop repaints at ~30Hz, so focusing unconditionally would tear
        // focus away from the rename editor ~30 times a second and make it
        // look like typing does nothing.
        if self.tab_rename.is_none() {
            window.focus(&self.focus_handle);
        }
```

- [ ] **Step 4: Host the editor in the tab bar**

`render_tab_bar` in `src/gpui_shell/tabs.rs` currently always calls `tab_display_label(&tab.title, idx, is_active, None)` — so that function's `rename_input` branch is presently unreachable. Give the bar an optional element to substitute for the active tab's label.

Change the signature to take one more parameter, and hand the element to the active tab's cell:

```rust
pub fn render_tab_bar(
    tabs: &TabManager,
    colors: &ColorScheme,
    on_select: TabSelectCallback,
    rename_editor: Option<gpui::AnyElement>,
) -> Div {
```

Inside the per-tab closure, the active tab renders the editor when one is supplied, and its label otherwise. Because `rename_editor` is an `AnyElement` and cannot be cloned into every iteration, take it out of an `Option` held outside the loop:

```rust
    let mut rename_editor = rename_editor;
    // ... inside the map over tabs, for the cell's child:
    let cell = if is_active && rename_editor.is_some() {
        cell.child(rename_editor.take().expect("checked is_some"))
    } else {
        cell.child(tab_display_label(&tab.title, idx, is_active, None))
    };
```

Since `.children()`'s closure must be `FnMut`, build the children into a `Vec` first if the borrow checker objects to `take()` inside it.

At the call site in `src/gpui_shell/render.rs`, pass the editor:

```rust
        let rename_editor = self
            .tab_rename
            .as_ref()
            .map(|input| input.clone().into_any_element());
        let tab_bar = tabs::render_tab_bar(&self.tabs, &self.config.colors, on_select_tab, rename_editor);
```

- [ ] **Step 5: Register the key bindings**

In `src/bin/gpui_petruterm.rs`, inside the `Application::new().run(...)` closure, beside the existing `cx.bind_keys([KeyBinding::new("cmd-q", Quit, None)]);`:

```rust
    petruterm::gpui_shell::text_input::register_key_bindings(cx);
```

- [ ] **Step 6: Build and run the full gate**

```bash
cargo build --bin gpui-petruterm --bin petruterm 2>&1 | tail -20
bash scripts/ci-local.sh 2>&1 | tail -30
RUSTFLAGS="-D warnings" cargo build --bin gpui-petruterm --bin petruterm 2>&1 | tail -20
cargo fmt --check
```
Expected: **215 lib tests passing**, everything clean, and no dead-code warning for `TextInput` now that it is constructed.

- [ ] **Step 7: Dogfood checkpoint — STOP and ask the user to confirm**

Ask the user to run `cargo run --bin gpui-petruterm` and check:
1. `Ctrl+F` then `,` opens an editable field over the active tab's label, pre-filled with its current name and with a visible cursor.
2. Typing edits it; `left`/`right`/`home`/`end` move the cursor; `backspace`/`delete` work; `shift-left`/`shift-right` select; `cmd-a` selects all; `cmd-c`/`cmd-x`/`cmd-v` copy/cut/paste.
3. `Enter` commits — the tab shows the new name. `Escape` abandons — the tab keeps its old name.
4. After either, typing goes to the **terminal** again (this is the focus hand-back; if the terminal seems dead afterwards, `end_tab_rename` is not restoring focus).
5. A rename to only spaces leaves the old name rather than blanking the tab.
6. **IME**, the reason for this port: switch to a non-ASCII input method (macOS: an IME such as Japanese or Pinyin) and type into the field — in-progress composition should appear underlined and commit correctly. Also try a dead key (e.g. `⌥e` then `e` → `é`).
7. Terminal keys are unaffected while **not** renaming: `backspace`, arrows, `home`/`end`, and `cmd-v` all still behave normally in the shell. (This is what the `"TextInput"` binding scope protects; if any of them broke, the bindings leaked globally.)

- [ ] **Step 8: Commit**

```bash
git add src/gpui_shell src/bin/gpui_petruterm.rs
git commit -m "feat: Implement tab rename with the new text-input primitive.

Closes the RenameTab no-op M2 deferred for lack of a text-input
primitive. Leader , opens an editable field over the active tab's label;
Enter commits, Escape abandons, whitespace-only is treated as a cancel.

Also fixes a focus bug this is the first code to expose: render() called
window.focus() unconditionally every frame, and the poll loop repaints at
~30Hz, so any focusable child would have had focus torn away ~30 times a
second."
```

---

## Self-Review

**Spec coverage.** §3.1 requires a reusable primitive ported from gpui's example, as an `Entity` implementing `EntityInputHandler`, keeping IME marked-range, selection, cut/copy/paste, and word boundaries — Task 1 Steps 6-8, with the boundary helpers in Step 4. It requires the departure from the free-function pattern to be deliberate and documented — Task 1 Step 6's doc comment. It names the consumers (chat input, workspace rename, file-picker query, and `RenameTab`); only `RenameTab` is in M3a, the rest arrive in M3b/M3c, and the produced interface is stated in Task 1's Interfaces block so those milestones can consume it. §2 requires M3a to close out `RenameTab` — Task 2.

**Placeholders.** None: every code step carries the actual code, the two commands whose output matters state their expected output, and the dogfood checklist enumerates concrete steps rather than "verify it works".

**Type consistency.** `TextInput::new(cx, colors, initial, placeholder)` is defined in Task 1 and called in Task 2 Step 2 with exactly that shape. `TextInputEvent::{Submit, Cancel}` is defined in Task 1 and matched exhaustively in Task 2. `content()` returns `&str`, and Task 2 calls `.trim().to_string()` on it. `register_key_bindings(cx: &mut App)` is defined in Task 1 Step 8 and called in Task 2 Step 5. `rename_active` and `active_tab` are pre-existing and were verified against `tabs.rs` before writing.

**One risk flagged for the executor.** Task 2 Step 4's `AnyElement` cannot be cloned, and `.children()` takes an `FnMut` closure — the `take()` may need the children built into a `Vec` first. The step says so rather than pretending the first formulation compiles.
