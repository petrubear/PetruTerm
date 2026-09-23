// The right-click context menu's state and action dispatch. Reuses the pure
// data types `crate::ui::context_menu::{ContextAction, ContextMenuItem}`,
// but not `ContextMenu` itself (built around the wgpu build's cell
// hit-testing): this file's `ContextMenu` is visible + a pixel position +
// the item list, rendered as clickable `div()`s.

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
    pub visible: bool,
    pub position: Point<Pixels>,
    pub items: Vec<ContextMenuItem>,
}

impl ContextMenu {
    pub fn close(&mut self) {
        self.visible = false;
    }
}

/// Called with the click's real window-space pixel position on a right
/// mouse-down over the terminal grid. No terminal id: Copy/Paste/Clear all
/// operate on the globally-active terminal, the same scoping the wgpu build's own
/// `Mux::active_terminal()`-based menu already uses.
pub(super) type RightClickCallback = Rc<dyn Fn(Point<Pixels>, usize, usize, &mut Window, &mut App)>;

/// Called when a menu row is clicked, with that row's own `ContextAction`.
pub type ContextActionCallback =
    Rc<dyn Fn(&crate::ui::context_menu::ContextAction, &mut Window, &mut App)>;

/// Called when a click lands outside the menu (`on_mouse_down_out`).
pub type ContextMenuCloseCallback = Rc<dyn Fn(&mut Window, &mut App)>;

/// Register the terminal grid's own right-click handler -- a new,
/// independent `window.on_mouse_event` registration alongside (not
/// replacing) `mouse::register_mouse_handlers`'s existing left-click/drag/
/// scroll handling in `terminal_element.rs`'s `paint()`.
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

/// Build the context menu's `div()` tree: a small popup positioned at
/// `menu.position`, one clickable row per item, closing on any click
/// outside itself. `on_mouse_down_out` fires during the CAPTURE phase and
/// does NOT call `cx.stop_propagation()`, so the outside click that closed
/// this menu still reaches whatever it actually landed on (the terminal, a
/// different tab) afterward -- unlike the palette or
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
    /// Run one confirmed context-menu action. CopyLastCommand is never
    /// built by the gpui menu; Separator/Label are filtered out at render.
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
            ContextAction::CopyBlockOutput(tid, bid) => {
                if let Some(text) = self.block_output_text(tid, bid) {
                    if !text.is_empty() {
                        cx.write_to_clipboard(gpui::ClipboardItem::new_string(text));
                    }
                }
            }
            ContextAction::ReRunCommand(cmd) if !cmd.is_empty() => {
                let active_ws = self.workspaces.active();
                let active_tid =
                    active_ws.tab_panes[active_ws.tabs.active_index()].focused_terminal;
                if let Some(terminal) = self.terminals.get(&active_tid) {
                    terminal.write_input(format!("{cmd}\n").as_bytes());
                }
            }
            ContextAction::SendToChat => {
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
            ContextAction::OpenLink(url) => {
                let open_arg =
                    if url.starts_with('/') || url.starts_with("./") || url.starts_with("../") {
                        crate::app::hover_link::path_for_open(&url).to_string()
                    } else {
                        url
                    };
                std::thread::spawn(move || {
                    let _ = std::process::Command::new("open").arg(&open_arg).spawn();
                });
            }
            ContextAction::CopyLink(url) => {
                cx.write_to_clipboard(gpui::ClipboardItem::new_string(url));
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
