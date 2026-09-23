// The tab bar's render function.

use std::rc::Rc;

use gpui::{div, prelude::*, App, Div, MouseButton, MouseDownEvent, Rgba, Window};

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
            if is_active {
                // Status-bar-style pill: a custom-colored tab gets a
                // darkened tint of its own accent. Clamped in HSL space
                // (fixed lightness, hue/saturation kept) rather than a flat
                // `RGB * factor` scale-down -- the theme's tab swatches
                // (`brights[1..7]`, dracula-pro.lua) are all very light
                // pastels, so scaling their RGB toward black shrinks every
                // channel by the same proportion and the results collapse
                // into near-identical dark blobs.
                // Pinning lightness instead keeps each swatch's actual hue
                // doing the differentiating work. A plain tab with no
                // custom color keeps the neutral highlight instead of every
                // default active tab turning theme-accent purple.
                let (bg, fg) = match tab.accent_color {
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
                };
                cell.bg(bg).text_color(fg)
            } else {
                // Inactive tabs get a dimmer pill, matching the status
                // bar's always-filled segments.
                let fg = match tab.accent_color {
                    Some(accent) => to_rgba(accent),
                    None => to_rgba(colors.ui_muted),
                };
                cell.bg(to_rgba(colors.ui_surface)).text_color(fg)
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

/// Fixed lightness an active custom-colored tab's pill is tinted to,
/// regardless of the source swatch's own lightness -- see
/// `render_tab_bar`'s call site for why a flat RGB scale-down isn't enough.
const ACTIVE_TAB_TINT_LIGHTNESS: f32 = 0.16;

/// Recolor `(r, g, b)` (0.0-1.0 each) to `ACTIVE_TAB_TINT_LIGHTNESS`,
/// keeping its hue and saturation. A minimal local RGB<->HSL round trip
/// (gpui has no public conversion for this) -- standard formulas, e.g.
/// https://www.w3.org/TR/css-color-3/#hsl-color.
fn tab_pill_tint(r: f32, g: f32, b: f32) -> Rgba {
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) / 2.0;
    let (h, s) = if (max - min).abs() < f32::EPSILON {
        (0.0, 0.0)
    } else {
        let d = max - min;
        let s = if l > 0.5 {
            d / (2.0 - max - min)
        } else {
            d / (max + min)
        };
        let h = if max == r {
            (g - b) / d + if g < b { 6.0 } else { 0.0 }
        } else if max == g {
            (b - r) / d + 2.0
        } else {
            (r - g) / d + 4.0
        };
        (h / 6.0, s)
    };

    fn hue_to_channel(p: f32, q: f32, mut t: f32) -> f32 {
        if t < 0.0 {
            t += 1.0;
        }
        if t > 1.0 {
            t -= 1.0;
        }
        if t < 1.0 / 6.0 {
            return p + (q - p) * 6.0 * t;
        }
        if t < 0.5 {
            return q;
        }
        if t < 2.0 / 3.0 {
            return p + (q - p) * (2.0 / 3.0 - t) * 6.0;
        }
        p
    }

    let l = ACTIVE_TAB_TINT_LIGHTNESS;
    let (r, g, b) = if s == 0.0 {
        (l, l, l)
    } else {
        let q = if l < 0.5 {
            l * (1.0 + s)
        } else {
            l + s - l * s
        };
        let p = 2.0 * l - q;
        (
            hue_to_channel(p, q, h + 1.0 / 3.0),
            hue_to_channel(p, q, h),
            hue_to_channel(p, q, h - 1.0 / 3.0),
        )
    };
    Rgba { r, g, b, a: 1.0 }
}
