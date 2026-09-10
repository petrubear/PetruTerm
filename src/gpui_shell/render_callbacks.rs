// gpui chrome migration (M4c Task 2 review): the five per-frame `Rc`
// callbacks `render()` hands down into the pane tree and the context
// menu. Split out of `render.rs` for the 400-line convention -- this
// block was self-contained (its only input is `cx`, its only output is
// the five callbacks) and grew `render.rs` to 439 lines once the M4c
// Task 2 context-menu callbacks (`on_right_click`, `on_context_action`,
// `on_close_context_menu`) landed alongside the pre-existing
// `on_focus`/`on_drag` pair.
//
// Every closure below holds a WEAK handle (`cx.entity().downgrade()`),
// exactly what `Context::listener` does internally and for the same
// reason: these closures outlive `render()` (owned by the element tree,
// run during event dispatch), so a strong `Entity<Self>` parked in one
// would keep the view alive past window close.

use std::rc::Rc;

use gpui::Context;

use super::{context_menu, pane_view, GpuiShellRoot};

pub(super) fn build_frame_callbacks(
    cx: &mut Context<GpuiShellRoot>,
) -> (
    pane_view::PaneFocusCallback,
    pane_view::SeparatorDragCallback,
    context_menu::RightClickCallback,
    context_menu::ContextActionCallback,
    context_menu::ContextMenuCloseCallback,
) {
    let view = cx.entity().downgrade();

    let focus_view = view.clone();
    let on_focus: pane_view::PaneFocusCallback = Rc::new(move |terminal_id, _window, cx| {
        focus_view
            .update(cx, |root, cx| {
                let ws = root.workspaces.active_mut();
                let active = ws.tabs.active_index();
                if ws.tab_panes[active].focused_terminal != terminal_id {
                    ws.tab_panes[active].focused_terminal = terminal_id;
                    cx.notify();
                }
            })
            .ok();
    });

    let drag_view = view.clone();
    let on_drag: pane_view::SeparatorDragCallback =
        Rc::new(move |node_id, position, _window, cx| {
            drag_view
                .update(cx, |root, cx| {
                    // Clone the Rc first: `drag_separator` needs `&mut
                    // self.tab_panes[..]` and `&self.rect_cache`'s
                    // contents at once, which a single `root.` borrow of
                    // both fields can't express.
                    let rects = root.rect_cache.clone();
                    let rects = rects.borrow();
                    let active = root.workspaces.active().tabs.active_index();
                    root.workspaces.active_mut().tab_panes[active].drag_separator(
                        node_id,
                        f32::from(position.x),
                        f32::from(position.y),
                        &rects,
                    );
                    cx.notify();
                })
                .ok();
        });

    let right_click_view = view.clone();
    let on_right_click: context_menu::RightClickCallback =
        Rc::new(move |position, col, row, _window, cx| {
            right_click_view
                .update(cx, |root, cx| {
                    let active_ws = root.workspaces.active();
                    let active_tid =
                        active_ws.tab_panes[active_ws.tabs.active_index()].focused_terminal;

                    let link = root.terminals.get(&active_tid).and_then(|terminal| {
                        let (row_text, _) = super::blocks::row_text_and_absolute_row(terminal, row);
                        crate::app::hover_link::scan_link_at(&row_text, col)
                    });

                    if let Some((_, _, _, text)) = link {
                        root.context_menu.position = position;
                        root.context_menu.items = vec![
                            crate::ui::context_menu::ContextMenuItem {
                                label: "Open Link".to_string(),
                                keybind: None,
                                action: crate::ui::context_menu::ContextAction::OpenLink(
                                    text.clone(),
                                ),
                                swatch_color: None,
                            },
                            crate::ui::context_menu::ContextMenuItem {
                                label: "Copy Link".to_string(),
                                keybind: None,
                                action: crate::ui::context_menu::ContextAction::CopyLink(text),
                                swatch_color: None,
                            },
                        ];
                        root.context_menu.visible = true;
                        cx.notify();
                        return;
                    }

                    root.context_menu.position = position;

                    let block = root.terminals.get(&active_tid).and_then(|terminal| {
                        let (_, absolute_row) =
                            super::blocks::row_text_and_absolute_row(terminal, row);
                        root.block_managers
                            .get(&active_tid)
                            .and_then(|m| m.block_at_absolute_row(absolute_row))
                            .map(|b| (b.id, b.command_text.clone()))
                    });

                    let mut items = vec![
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

                    if let Some((block_id, command_text)) = block {
                        items.push(crate::ui::context_menu::ContextMenuItem {
                            label: "Copy Output".to_string(),
                            keybind: Some("Leader y".to_string()),
                            action: crate::ui::context_menu::ContextAction::CopyBlockOutput(
                                active_tid, block_id,
                            ),
                            swatch_color: None,
                        });
                        items.push(crate::ui::context_menu::ContextMenuItem {
                            label: "Re-run Command".to_string(),
                            keybind: Some("Leader r".to_string()),
                            action: crate::ui::context_menu::ContextAction::ReRunCommand(
                                command_text,
                            ),
                            swatch_color: None,
                        });
                    }

                    let has_selection = root
                        .terminals
                        .get(&active_tid)
                        .and_then(|t| t.selection_text())
                        .is_some();
                    if has_selection {
                        items.push(crate::ui::context_menu::ContextMenuItem {
                            label: "Send to Chat".to_string(),
                            keybind: None,
                            action: crate::ui::context_menu::ContextAction::SendToChat,
                            swatch_color: None,
                        });
                    }

                    root.context_menu.items = items;
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

    let close_menu_view = view;
    let on_close_context_menu: context_menu::ContextMenuCloseCallback =
        Rc::new(move |_window, cx| {
            close_menu_view
                .update(cx, |root, cx| {
                    root.context_menu.close();
                    cx.notify();
                })
                .ok();
        });

    (
        on_focus,
        on_drag,
        on_right_click,
        on_context_action,
        on_close_context_menu,
    )
}
