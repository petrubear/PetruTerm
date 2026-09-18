// gpui chrome migration (M2 Task 3, visual-polish pass 2026-09-17): the tab
// bar's own render function. Split out of the single `tabs.rs` (now
// `tabs/mod.rs`) once the polish pass's pill treatment pushed that file over
// the 400-line convention -- pure code motion for the surrounding types,
// only `render_tab_bar`'s own body actually changed shape.

use std::rc::Rc;

use gpui::{div, prelude::*, App, Div, MouseButton, MouseDownEvent, Window};

use crate::config::schema::ColorScheme;

use super::super::pane_view::to_rgba;
use super::{tab_display_label, TabManager};

/// Called with the clicked tab's index. A callback rather than a direct
/// `TabManager` mutation because the click also has to reach `GpuiShellRoot`
/// (switching tabs changes which pane tree renders, so the view has to be
/// notified) -- built from `Context::listener` at the call site.
pub type TabSelectCallback = Rc<dyn Fn(&usize, &mut Window, &mut App)>;

/// Called with the clicked tab's index and the right-click position --
/// triggers the tab color picker menu at that position for that tab.
///
/// `pub(crate)` (not `pub(super)`): this type is defined one module deeper
/// than the original flat `tabs.rs`, so `pub(super)` here would only reach
/// `tabs`, not `gpui_shell` where `render.rs`'s call site needs it --
/// `tabs/mod.rs`'s own re-export narrows this back down to `pub(super)`
/// (i.e. `gpui_shell`-visible), matching the original visibility exactly.
pub(crate) type TabRightClickCallback =
    Rc<dyn Fn(usize, gpui::Point<gpui::Pixels>, &mut Window, &mut App)>;

/// The tab bar row: now the header strip of the terminal "card" (visual-
/// polish pass 2, 2026-09-17 -- `render.rs`'s `terminal_card` wraps this bar
/// above the pane area, inside its own rounded/bordered frame, replacing
/// this bar's old role as a full-window-width bar sitting *above* the
/// sidebar too). Lighter treatment than pass 1's filled pill chips, matching
/// the approved mockup's minimal tab strip: no background of its own (the
/// card already has one), a soft highlight only behind the active label, an
/// accent-colored dot for the active tab, dimmed text for the rest, and a
/// bottom border separating the strip from the pane content below it (no
/// behavior changed, `on_select`/`on_right_click`/rename wiring is
/// untouched).
pub fn render_tab_bar(
    tabs: &TabManager,
    colors: &ColorScheme,
    on_select: TabSelectCallback,
    on_right_click: TabRightClickCallback,
    rename: Option<(usize, gpui::AnyElement)>,
) -> Div {
    let active_index = tabs.active_index();
    // `rename`'s element can't be cloned into every loop iteration
    // (`AnyElement` isn't `Clone`), and `.children()`'s closure must be
    // `FnMut` -- so it's built out here and `take()`n exactly once, on the
    // cell whose tab id matches (NOT on `is_active`: the rename is pinned to
    // a tab id precisely so it keeps rendering on the right cell even after
    // a tab switch moves `is_active` elsewhere).
    let mut rename = rename;
    let cells: Vec<_> = tabs
        .tabs()
        .iter()
        .enumerate()
        .map(|(idx, tab)| {
            let is_active = idx == active_index;
            let is_renaming = rename.as_ref().is_some_and(|(id, _)| *id == tab.id);
            let on_select = on_select.clone();
            let on_right_click = on_right_click.clone();
            let cell = div()
                .flex()
                .flex_row()
                .items_center()
                .gap_2()
                .px_2()
                .py_1()
                .rounded_md()
                .cursor_pointer()
                .on_mouse_down(MouseButton::Left, move |_: &MouseDownEvent, window, cx| {
                    on_select(&idx, window, cx)
                })
                .on_mouse_down(
                    MouseButton::Right,
                    move |event: &MouseDownEvent, window, cx| {
                        on_right_click(idx, event.position, window, cx)
                    },
                );
            // Accent dot: always shown for the active tab (its own custom
            // color, or the theme default); shown for an inactive tab only
            // when it has its own custom color, matching the wgpu build's
            // own underline (`src/app/renderer/overlay.rs`'s `tab_accent`/
            // `accent_color.is_some()` split) -- a color assigned via the
            // tab's right-click menu needs to stay visible at a glance
            // without switching to that tab (reported live: the dot-only
            // pass 1 rewrite dropped this, showing color for the active tab
            // alone), while a plain tab with no assigned color doesn't get
            // a decorative dot it never asked for.
            let show_dot = is_active || tab.accent_color.is_some();
            let cell = if show_dot {
                let dot_color = to_rgba(tab.accent_color.unwrap_or(colors.ui_accent));
                cell.child(
                    div()
                        .w(gpui::px(6.0))
                        .h(gpui::px(6.0))
                        .rounded_full()
                        .bg(dot_color),
                )
            } else {
                cell
            };
            let cell = if is_renaming {
                cell.child(rename.take().expect("checked is_some").1)
            } else {
                cell.child(
                    div()
                        .italic()
                        .child(tab_display_label(&tab.title, idx, is_active, None)),
                )
            };
            if is_active {
                cell.bg(to_rgba(colors.ui_surface_hover))
                    .text_color(to_rgba(colors.foreground))
            } else {
                cell.text_color(to_rgba(colors.ui_muted))
            }
        })
        .collect();
    let content_row = div()
        .flex()
        .flex_row()
        .items_center()
        .gap_1()
        // `px_2` here must equal `pane_view.rs`'s own leaf-wrapper `p_2` --
        // that's what puts the first pill's own left edge directly above the
        // terminal grid's own left edge (column 0), instead of the pill
        // sitting further right than the prompt text below it (a live
        // dogfood screenshot showed the mismatch directly). Nothing adds a
        // margin on top of this padding: see `divider` below for why the
        // border-bottom is a separate element instead of living here.
        .px_2()
        .py_1()
        .min_h(super::super::font_state::header_row_min_height())
        .flex_shrink_0()
        // Same fix as `status_bar::render_status_bar`: without an explicit
        // font, tab labels render in gpui's own default UI font instead of
        // the terminal grid's configured monospace face.
        .font_family(super::super::font_state::font_family())
        .text_size(gpui::px(super::super::font_state::font_size()))
        .children(cells);

    // A separate element, not `content_row`'s own `border_b_1`: the card
    // wrapping this bar rounds its own corners (`rounded_lg`, `render.rs`'s
    // `terminal_card`), and a border spanning the row's full width would
    // meet that curve at a sharp square notch (clipping only touches pixels
    // *within* the corner radius, and this row sits well below that band).
    // Insetting the border alone via `mx_2` lets it float clear of both
    // corners *without* also pushing `content_row`'s own pills off the grid
    // alignment above -- margin and padding on the very same element would
    // have compounded instead.
    let divider = div().mx_2().h(gpui::px(1.0)).bg(to_rgba(colors.ui_border));

    div()
        .flex()
        .flex_col()
        .flex_shrink_0()
        .child(content_row)
        .child(divider)
}
