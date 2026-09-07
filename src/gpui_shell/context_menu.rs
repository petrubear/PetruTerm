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

use std::rc::Rc;

use gpui::{
    div, prelude::*, px, App, Bounds, Context, DispatchPhase, MouseButton, MouseDownEvent, Pixels,
    Point, Window,
};

use crate::config::schema::ColorScheme;

use super::pane_view::to_rgba;

use crate::ui::context_menu::{ContextAction, ContextMenuItem};

use super::GpuiShellRoot;

/// The right-click menu's own state. `position` is a real window-relative
/// pixel point (the click that opened it), not a terminal-cell coordinate.
#[derive(Default)]
pub struct ContextMenu {
    #[allow(dead_code)]
    pub visible: bool,
    #[allow(dead_code)]
    pub position: Point<Pixels>,
    #[allow(dead_code)]
    pub items: Vec<ContextMenuItem>,
}

impl ContextMenu {
    #[allow(dead_code)]
    pub fn close(&mut self) {
        self.visible = false;
    }
}

/// Called with the click's real window-space pixel position on a right
/// mouse-down over the terminal grid. No terminal id: Copy/Paste/Clear all
/// operate on the globally-active terminal (this file's own doc comment
/// has the reasoning), the same scoping the wgpu build's own
/// `Mux::active_terminal()`-based menu already uses.
pub(super) type RightClickCallback = Rc<dyn Fn(Point<Pixels>, &mut Window, &mut App)>;

/// Called when a menu row is clicked, with that row's own `ContextAction`.
pub type ContextActionCallback =
    Rc<dyn Fn(&crate::ui::context_menu::ContextAction, &mut Window, &mut App)>;

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

impl GpuiShellRoot {
    /// Run one confirmed context-menu action. `ContextAction`'s other
    /// variants (`SendToChat`, `CopyLastCommand`, `OpenLink`, `CopyLink`,
    /// `CopyBlockOutput`, `ReRunCommand`, `Separator`, `Label`) are never
    /// constructed by this milestone's own item lists (Task 2/3) -- no
    /// arm needed for them here, `_ => {}` covers anything unreachable in
    /// practice the same way M4a's palette dispatch does.
    #[allow(dead_code)]
    pub(super) fn dispatch_context_action(
        &mut self,
        action: ContextAction,
        cx: &mut Context<Self>,
    ) {
        self.context_menu.close();
        match action {
            ContextAction::Copy => {
                let active_ws = self.workspaces.active();
                let active_tid =
                    active_ws.tab_panes[active_ws.tabs.active_index()].focused_terminal;
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

    pub(super) fn clear_active_terminal(&mut self) {
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
