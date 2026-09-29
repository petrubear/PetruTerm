// The tab bar's render function.

use std::rc::Rc;

use gpui::{div, prelude::*, App, Context, Div, MouseButton, MouseDownEvent, Rgba, Window};

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
/// `pub(crate)` (not `pub(super)`): `pub(super)` here would only reach
/// `tabs`, not `gpui_shell`; `tabs/mod.rs`'s re-export narrows it back to
/// `pub(super)`.
pub(crate) type TabRightClickCallback =
    Rc<dyn Fn(usize, gpui::Point<gpui::Pixels>, &mut Window, &mut App)>;

/// gpui's own drag-and-drop payload for a tab pill being dragged out of its
/// slot -- carries the dragged tab's stable id, matched against `on_drop`'s
/// type parameter on each gap (`Interactivity::on_drag`/`on_drop` pair up by
/// payload type, not by any registration the caller manages). Not the same
/// mechanism `resize_handle.rs`/`separator.rs` use (a thread-local drag
/// state read inside a custom `Element`'s `paint`): those exist only
/// because plain `on_mouse_move`/`on_mouse_up` are hover-gated on the
/// element under the pointer. gpui's `on_drag`/`on_drop` are a dedicated,
/// non-hover-gated subsystem built for exactly this case (already used
/// elsewhere in this file's sibling `render_callbacks.rs` for whole-window
/// OS file drops).
#[derive(Clone, Copy)]
struct DraggedTab {
    tab_id: usize,
}

/// Called with the dragged tab's id and the target gap (0 = before the
/// first tab, `tab_count()` = after the last) -- mirrors `TabSelectCallback`
/// in going through `GpuiShellRoot` rather than mutating `TabManager`
/// directly, since a reorder also needs `cx.notify()`.
pub type TabReorderCallback = Rc<dyn Fn(usize, usize, &mut Window, &mut App)>;

/// Layout footprint of each gap between (and around) tab pills. This is the
/// *only* source of inter-pill spacing (`content_row` has no `gap_1()` of
/// its own -- the two were stacking, roughly quadrupling the space between
/// tabs), so it stays thin: the bar reads as tight as a plain flex gap.
const TAB_GAP_WIDTH_PX: f32 = 4.0;

/// Width of a gap's invisible drop hit area. Wider than its footprint (see
/// `tab_gap`) because a 4px-wide target is too small for a pointer to land
/// on mid-drag. Overlaps ~4px into each neighbouring pill's 8px padding.
const TAB_GAP_HIT_WIDTH_PX: f32 = 12.0;

/// Height of a gap's invisible drop hit area: the whole row, as tall as
/// `header_row_min_height()`, while its footprint stays text-height.
fn tab_gap_hit_height_px() -> f32 {
    super::super::font_state::header_row_min_height().into()
}

/// The floating preview gpui renders under the cursor while a tab is being
/// dragged -- a plain view entity, per `Interactivity::on_drag`'s own
/// `constructor` contract.
struct TabDragPreview {
    label: String,
    bg: Rgba,
    fg: Rgba,
}

impl gpui::Render for TabDragPreview {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .px_2()
            .py_1()
            .rounded_md()
            .bg(self.bg)
            .text_color(self.fg)
            .child(self.label.clone())
    }
}

/// One drop target in the gap before/after/between tab pills, landing a
/// dropped tab at `to_gap` (see `TabManager::move_tab`'s own doc comment
/// for what "gap" means). Highlighted via `drag_over` only while a
/// `DraggedTab` is actually being dragged over it.
fn tab_gap(
    to_gap: usize,
    colors: &ColorScheme,
    on_reorder: TabReorderCallback,
) -> gpui::AnyElement {
    let highlight = to_rgba(colors.ui_accent);
    let footprint_h = super::super::font_state::font_size();
    let hit_h = tab_gap_hit_height_px();
    // The pill cells' own height is content-driven, so the gap's footprint
    // height stays at the text's own size (never the row's tallest child).
    // The hit area is larger than the footprint in both axes; negative
    // margins cancel the overhang so it costs no layout space and simply
    // overlaps the neighbouring pills' padding. Its bounds never change
    // while dragging (only the color does), so the hover can't flicker from
    // the target moving under the pointer. The highlight is `drag_over` on
    // this same element rather than `group_drag_over` on an inner child:
    // gpui only guarantees a hitbox for elements with `drag_over` styles,
    // so an inner child styled by group never lit up.
    let overhang_x = (TAB_GAP_HIT_WIDTH_PX - TAB_GAP_WIDTH_PX) / 2.0;
    let overhang_y = (hit_h - footprint_h) / 2.0;
    div()
        .w(gpui::px(TAB_GAP_HIT_WIDTH_PX))
        .h(gpui::px(hit_h))
        .mx(gpui::px(-overhang_x))
        .my(gpui::px(-overhang_y))
        .rounded_full()
        .drag_over::<DraggedTab>(move |style, _dragged, _window, _cx| style.bg(highlight))
        .on_drop::<DraggedTab>(move |dragged, window, cx| {
            on_reorder(dragged.tab_id, to_gap, window, cx)
        })
        .into_any_element()
}

/// The tab bar row: the header strip of the terminal "card" (`render.rs`'s
/// `terminal_card`), with no background of its own. Every tab is a pill:
/// the active one gets `ui_surface_hover` (or a tint of its custom accent),
/// inactive ones get `ui_surface`. Custom-colored tabs use their accent as
/// text color and show an accent dot; the active tab always shows a dot.
/// A bottom divider separates the strip from the pane content.
pub fn render_tab_bar(
    tabs: &TabManager,
    colors: &ColorScheme,
    on_select: TabSelectCallback,
    on_right_click: TabRightClickCallback,
    on_reorder: TabReorderCallback,
    rename: Option<(usize, gpui::AnyElement)>,
) -> Div {
    let active_index = tabs.active_index();
    // `rename`'s element can't be cloned into every loop iteration
    // (`AnyElement` isn't `Clone`), so it's taken exactly once, on the cell
    // whose tab id matches (NOT on `is_active`: the rename is pinned to a
    // tab id precisely so it keeps rendering on the right cell even after a
    // tab switch moves `is_active` elsewhere). A plain `for` loop (not
    // `.map().collect()`) since each iteration now pushes a gap *and* a
    // cell, one item each, interleaved.
    let mut rename = rename;
    let tab_count = tabs.tab_count();
    let mut cells: Vec<gpui::AnyElement> = Vec::with_capacity(tab_count * 2 + 1);
    if tab_count > 0 {
        cells.push(tab_gap(0, colors, on_reorder.clone()));
    }
    for (idx, tab) in tabs.tabs().iter().enumerate() {
        let is_active = idx == active_index;
        let is_renaming = rename.as_ref().is_some_and(|(id, _)| *id == tab.id);
        let on_select = on_select.clone();
        let on_right_click = on_right_click.clone();
        // Hoisted above the cell's own construction (rather than applied at
        // the end, as before) so the same colors can also style the
        // floating drag preview below.
        let (bg, fg) = if is_active {
            // Status-bar-style pill: a custom-colored tab gets its accent
            // scaled down by `ACTIVE_TAB_TINT_FACTOR`. A plain tab with no
            // custom color keeps the neutral highlight instead of every
            // default active tab turning theme-accent purple.
            match tab.accent_color {
                // Title text matches the dot's own color, same as a
                // custom-colored tab already gets a tinted pill instead
                // of the plain default -- only for tabs the user has
                // actually assigned a color to, so a default tab's
                // title doesn't turn theme-accent purple on its own.
                Some(accent) => (
                    tab_pill_tint(accent[0], accent[1], accent[2]),
                    to_rgba(accent),
                ),
                None => (to_rgba(colors.ui_surface_hover), to_rgba(colors.foreground)),
            }
        } else {
            // Inactive tabs get a dimmer pill, matching the status
            // bar's always-filled segments.
            let fg = match tab.accent_color {
                Some(accent) => to_rgba(accent),
                None => to_rgba(colors.ui_muted),
            };
            (to_rgba(colors.ui_surface), fg)
        };
        let tab_id = tab.id;
        let preview_label = tab_display_label(&tab.title, idx, is_active, None);
        let cell = div()
            .id(("tab", tab_id))
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
            )
            .on_drag(
                DraggedTab { tab_id },
                move |_dragged, _position, _window, cx| {
                    cx.new(|_| TabDragPreview {
                        label: preview_label.clone(),
                        bg,
                        fg,
                    })
                },
            );
        // Accent dot: always shown for the active tab (its own custom
        // color, or the theme default); shown for an inactive tab only
        // when it has its own custom color, matching the wgpu build's
        // own underline (`src/app/renderer/overlay.rs`'s `tab_accent`/
        // `accent_color.is_some()` split) -- a color assigned via the
        // tab's right-click menu needs to stay visible at a glance
        // without switching to that tab, while a plain tab with no
        // assigned color doesn't get a decorative dot it never asked for.
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
        cells.push(cell.bg(bg).text_color(fg).into_any_element());
        cells.push(tab_gap(idx + 1, colors, on_reorder.clone()));
    }
    let content_row = div()
        .flex()
        .flex_row()
        .items_center()
        // No `.gap_1()` here: `tab_gap` now provides 100% of the space
        // between (and around) pills itself -- the two were stacking,
        // roughly quadrupling the visible space between tabs.
        // `px_2` here must equal `pane_view.rs`'s own leaf-wrapper `p_2` --
        // that's what puts the first pill's own left edge directly above the
        // terminal grid's own left edge (column 0), instead of the pill
        // sitting further right than the prompt text below it. Nothing adds a
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

/// Flat RGB scale-down of the accent, same factor as the status bar's
/// `darken(_, 0.15)` segment backgrounds.
const ACTIVE_TAB_TINT_FACTOR: f32 = 0.15;

fn tab_pill_tint(r: f32, g: f32, b: f32) -> Rgba {
    Rgba {
        r: r * ACTIVE_TAB_TINT_FACTOR,
        g: g * ACTIVE_TAB_TINT_FACTOR,
        b: b * ACTIVE_TAB_TINT_FACTOR,
        a: 1.0,
    }
}
