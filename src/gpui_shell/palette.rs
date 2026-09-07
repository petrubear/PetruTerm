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
    div, prelude::*, px, rgba, App, Context, Entity, Focusable, FontWeight, KeyDownEvent,
    MouseButton, MouseDownEvent, Window,
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
