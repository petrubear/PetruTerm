// The command palette's render tree and its Up/Down key guard. State
// (`CommandPalette`) is reused from `crate::ui::palette`. Enter/Escape route
// through `palette_query`'s `Submit`/`Cancel` actions and the `cx.subscribe`
// callback in `construct.rs`.

use gpui::{
    div, prelude::*, px, rgba, App, Context, Entity, Focusable, FontWeight, KeyDownEvent,
    MouseButton, MouseDownEvent, Window,
};

use crate::config::schema::ColorScheme;
use crate::ui::palette::CommandPalette;

use super::font_state;
use super::pane_view::to_rgba;
use super::text_input::TextInput;
use super::GpuiShellRoot;

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

    /// Sync `CommandPalette`'s query/results from `palette_query`'s real,
    /// live `TextInput` content. `CommandPalette::type_char`/`backspace`
    /// are the wgpu build's own character-at-a-time input model; nothing
    /// in `gpui_shell` ever calls either, since real keystrokes go through
    /// `TextInput`'s own cursor-based editing instead -- without this,
    /// `self.palette.query` never changes and the results list never
    /// filters. Checked once per `render()` call (`render.rs`'s own top),
    /// the same "driver, not an event handler" shape `drive_search`
    /// already uses.
    pub(super) fn drive_palette_query(&mut self, cx: &App) {
        if !self.palette.visible {
            return;
        }
        let content = self.palette_query.read(cx).content();
        if content != self.palette.query {
            self.palette.set_query(content.to_string());
        }
    }

    /// The palette's own key guard, called from `input.rs`'s `on_key_down`
    /// as its very first statement. Returns `true` if the key was consumed (the
    /// query field held focus), telling the caller to `return` early.
    ///
    /// The palette's query field intercepts Up/Down directly. Enter/Escape
    /// are NOT handled here -- `TextInput`'s own `"TextInput"`-scoped key
    /// bindings (`text_input/mod.rs`) consume those as its own
    /// `Submit`/`Cancel` actions before this bubble listener ever sees
    /// them; `construct.rs`'s `cx.subscribe` callback on `palette_query` is where
    /// this struct reacts to them instead. Keyed on real focus, not
    /// `self.palette.visible`: unlike `InfoOverlay`, the palette's query
    /// field is a genuine focus-grabbing `TextInput`, so it follows the
    /// same guard shape every other `TextInput` consumer in this codebase
    /// uses.
    pub(super) fn maybe_handle_palette_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.palette_query_focused(window, cx) {
            return false;
        }
        match event.keystroke.key.as_str() {
            "down" => self.palette.select_down(),
            "up" => self.palette.select_up(),
            _ => {}
        }
        cx.notify();
        true
    }
}
