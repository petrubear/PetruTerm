// gpui chrome migration (M3a Task 1): `TextInput`, a single-line editable
// text field with real cursor, selection, clipboard, and IME composition
// (marked-range) support.
//
// Ports gpui 0.2.2's own `examples/input.rs` `TextInput` almost verbatim --
// see that file for the canonical reference this was built from. Four
// deliberate adaptations from the example: colors resolve from
// `ColorScheme` instead of hardcoded literals; every key binding is scoped
// to a `"TextInput"` key context (the example uses global bindings, which
// would capture backspace/arrows/clipboard chords application-wide and stop
// them reaching the PTY); `Submit`/`Cancel` events let a parent observe the
// edit finishing; and the UTF-16 and grapheme-boundary helpers are free
// functions over `&str` so they unit-test without a gpui context.
//
// Split under the 400-line convention: this file keeps the `TextInput`
// struct, its constructor/accessors, the actions/key-binding registration,
// and `Render`/`Focusable`; `edit` holds the editing and mouse methods;
// `ime` holds `EntityInputHandler`, the UTF-16/grapheme helpers, and their
// tests; `element` (unchanged by this split) holds the paint-time
// `TextElement`.

use std::ops::Range;

use gpui::{
    actions, div, prelude::*, App, Bounds, Context, CursorStyle, EventEmitter, FocusHandle,
    Focusable, KeyBinding, MouseButton, Pixels, Rgba, ShapedLine, SharedString, Window,
};

use crate::config::schema::ColorScheme;

use super::pane_view::to_rgba;

mod edit;
mod element;
mod ime;
use element::TextElement;

actions!(
    text_input,
    [
        Backspace,
        Delete,
        Left,
        Right,
        SelectLeft,
        SelectRight,
        SelectAll,
        Home,
        End,
        ShowCharacterPalette,
        Paste,
        Cut,
        Copy,
        Submit,
        Cancel,
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
