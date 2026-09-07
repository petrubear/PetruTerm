// gpui chrome migration (M2): walks one tab's `PaneTree` (see `panes.rs`)
// into nested taffy flex `div()`s and renders each leaf's live terminal.
//
// This is the piece that replaces the wgpu app's manual recursive rect
// subdivision (`src/ui/panes.rs`'s `PaneNode::layout`): a `Split` becomes a
// flex row/column whose first child carries `flex_basis(relative(ratio))`,
// and taffy computes every pixel rect from there. Anything that still needs
// real pixel geometry after the fact -- `focus_dir`'s center-point search,
// `drag_separator`'s ratio math, and each terminal's own PTY winsize --
// reads it back out of `RectCache`, populated here from `Div::
// on_children_prepainted` as the tree is laid out.
//
// Two deliberate design choices worth knowing before editing:
//
// 1. `RectCache.separators[node_id]` holds the SPLIT CONTAINER's bounds, not
//    the thin separator strip's own. `panes.rs`'s `drag_split_ratio` computes
//    `(mouse_x - sep_x) / sep_w`; with the separator's own ~6px width in the
//    denominator that formula is nonsense (any pointer more than a few px
//    from the strip clamps straight to 0.1/0.9). The container's origin and
//    width in the resize axis are what make it a real 0..1 ratio across the
//    whole split. The container's own bounds aren't handed to
//    `on_children_prepainted` directly (it reports the CHILDREN's bounds), so
//    they're reconstructed as the union of the three children -- which tile
//    the container exactly, in both axes, under flexbox's default
//    `align-items: stretch`.
//
// 2. The split's children are `flex_basis(relative(ratio)) + flex_shrink_0`
//    (first) and `flex_1` (second) -- deliberately NOT two complementary
//    `flex_basis(ratio)` / `flex_basis(1.0 - ratio)` children. Two
//    complementary bases plus a fixed-width separator sum to 100% + 6px, so
//    the row overflows its container by the separator's width every frame.
//    Letting the second child simply take the remainder is exact.

use std::collections::HashMap;
use std::rc::Rc;

use gpui::{div, prelude::*, px, relative, App, Bounds, Div, Pixels, Point, Rgba, Window};

use crate::config::schema::ColorScheme;
use crate::term::Terminal;
use crate::ui::search_bar::SearchMatch;

use super::mouse::OnFocusCallback;
use super::panes::{PaneTree, RectCache, SplitDir};
use super::separator::SeparatorElement;
use super::terminal_element::TerminalGridElement;

// Re-exported so `render.rs`'s existing `pane_view::is_dragging_separator()`
// call site keeps working unchanged -- the function itself moved to
// `separator.rs` (M4b Task 3 review) alongside the drag state it reads.
pub(super) use super::separator::is_dragging_separator;

/// Thickness of the separator strip between two panes -- the wider grab
/// room around the thin painted line (`separator.rs`'s own
/// `SEPARATOR_LINE_PX`), matching the wgpu app's own ±8px separator hit
/// tolerance (`src/app/mod.rs`'s `separator_at_pixel`) -- a literal 1px hit
/// target is unusable with a trackpad.
const SEPARATOR_PX: f32 = 6.0;

/// Called with a leaf's terminal id when that pane is clicked -- the
/// multi-pane counterpart of `mouse::OnFocusCallback`, which the per-leaf
/// closure built in `render_leaf` closes over its own id to produce.
pub(super) type PaneFocusCallback = Rc<dyn Fn(usize, &mut Window, &mut App)>;

/// Called with `(split node_id, mouse position)` on every move of an
/// in-progress separator drag.
pub(super) type SeparatorDragCallback = Rc<dyn Fn(u32, Point<Pixels>, &mut Window, &mut App)>;

/// `ColorScheme`'s `[f32; 4]` RGBA -> gpui's own color type. Shared with
/// `mod.rs`'s tab bar so the chrome and the pane tree read the same theme
/// tokens the same way.
pub(super) fn to_rgba(color: [f32; 4]) -> Rgba {
    Rgba {
        r: color[0],
        g: color[1],
        b: color[2],
        a: color[3],
    }
}

/// Everything the tree walk needs, bundled so the recursive helpers don't
/// take a dozen positional arguments. Borrowed for the duration of one
/// `render()` call; the per-element closures built from it capture only
/// cheap clones (`Rc<Terminal>`, `Rc<RefCell<RectCache>>`, callback `Rc`s).
pub(super) struct PaneRenderCx<'a> {
    pub terminals: &'a HashMap<usize, Rc<Terminal>>,
    /// The active tab's `PaneForest::focused_terminal` -- drives each leaf's
    /// solid-vs-hollow cursor.
    pub focused: usize,
    pub colors: &'a ColorScheme,
    pub cell_width: Pixels,
    pub cell_height: Pixels,
    pub cursor_blink_on: bool,
    /// `config.scrollback_lines`, needed by the per-leaf PTY resize below.
    pub scrollback: usize,
    pub rects: Rc<std::cell::RefCell<RectCache>>,
    pub on_focus: PaneFocusCallback,
    pub on_drag: SeparatorDragCallback,
    /// Active search matches for the focused pane, threaded to whichever
    /// leaf's `terminal_id == focused` (M4b Task 3) -- every other leaf
    /// gets `None`.
    pub search: Option<(Vec<SearchMatch>, usize)>,
}

/// Walk `node` into a nested flex tree. The returned `Div` carries no
/// main-axis sizing of its own -- the caller decides (the root gets
/// `size_full()`, a split's children get `flex_basis`/`flex_1`).
pub(super) fn render_pane_tree(node: &PaneTree, ctx: &PaneRenderCx) -> Div {
    match node {
        PaneTree::Leaf { terminal_id } => render_leaf(*terminal_id, ctx),
        PaneTree::Split {
            node_id,
            dir,
            ratio,
            left,
            right,
        } => render_split(*node_id, *dir, *ratio, left, right, ctx),
    }
}

/// One leaf: the terminal's own grid element, wrapped in a div whose
/// `on_children_prepainted` records the grid's painted bounds (and keeps the
/// PTY's winsize in step with them).
///
/// `TerminalGridElement` requests `relative(1.)` in both axes, so the bounds
/// reported here for its (single) child are exactly this wrapper's content
/// box -- i.e. the recorded rect is simultaneously "the space this pane was
/// given" and "the rect this pane paints and hit-tests in", which is what
/// keeps `focus_dir`'s geometry and `mouse.rs`'s own `bounds` in agreement.
pub(super) fn render_leaf(terminal_id: usize, ctx: &PaneRenderCx) -> Div {
    let Some(terminal) = ctx.terminals.get(&terminal_id).cloned() else {
        // A leaf whose terminal was already reaped. Nothing to paint; the
        // tree mutation that removes it happens on the next input event.
        return div();
    };

    let rects = ctx.rects.clone();
    let resize_target = terminal.clone();
    let cell_width = ctx.cell_width;
    let cell_height = ctx.cell_height;
    let scrollback = ctx.scrollback;

    let focus_cb = ctx.on_focus.clone();
    let on_focus: OnFocusCallback = Rc::new(move |window, cx| focus_cb(terminal_id, window, cx));

    div()
        .flex()
        .size_full()
        .on_children_prepainted(move |bounds, _window, _cx| {
            let Some(bounds) = bounds.first().copied() else {
                return;
            };
            rects.borrow_mut().leaves.insert(terminal_id, bounds);
            fit_terminal(&resize_target, bounds, cell_width, cell_height, scrollback);
        })
        .child(TerminalGridElement {
            terminal,
            cell_width,
            cell_height,
            colors: ctx.colors.clone(),
            is_active: terminal_id == ctx.focused,
            cursor_blink_on: ctx.cursor_blink_on,
            on_focus,
            search: if terminal_id == ctx.focused {
                ctx.search.clone()
            } else {
                None
            },
        })
}

fn render_split(
    node_id: u32,
    dir: SplitDir,
    ratio: f32,
    left: &PaneTree,
    right: &PaneTree,
    ctx: &PaneRenderCx,
) -> Div {
    let rects = ctx.rects.clone();
    let container = div()
        .flex()
        .size_full()
        // See this module's doc comment (1): the union of the three children
        // IS the container's content box, and it's the container -- not the
        // thin strip -- that `drag_split_ratio` needs.
        .on_children_prepainted(move |bounds, _window, _cx| {
            let Some(union) = bounds
                .iter()
                .copied()
                .reduce(|acc, b| acc.union(&b))
                .filter(|b| b.size.width > px(0.0) && b.size.height > px(0.0))
            else {
                return;
            };
            rects.borrow_mut().separators.insert(node_id, union);
        })
        .child(
            render_pane_tree(left, ctx)
                .flex_basis(relative(ratio))
                .flex_shrink_0(),
        )
        .child(separator(node_id, dir, ctx))
        // See this module's doc comment (2): the remainder, not a second
        // explicit basis -- otherwise the row overflows by the separator's
        // own width.
        .child(render_pane_tree(right, ctx).flex_1());

    match dir {
        SplitDir::Horizontal => container.flex_row(),
        SplitDir::Vertical => container.flex_col(),
    }
}

/// The grab strip between two panes: a fixed-size, non-shrinking flex child
/// carrying the resize cursor affordance, wrapping the element that paints
/// the divider line and registers the drag handlers.
fn separator(node_id: u32, dir: SplitDir, ctx: &PaneRenderCx) -> Div {
    let element = SeparatorElement {
        node_id,
        dir,
        color: to_rgba(ctx.colors.ui_border),
        on_drag: ctx.on_drag.clone(),
    };
    let strip = div().flex_shrink_0().child(element);
    match dir {
        SplitDir::Horizontal => strip.w(px(SEPARATOR_PX)).h_full().cursor_col_resize(),
        SplitDir::Vertical => strip.h(px(SEPARATOR_PX)).w_full().cursor_row_resize(),
    }
}

/// Resize `terminal`'s grid + PTY to the cell count that fits `bounds`.
///
/// Every pane's size is decided by taffy, so this is the only place a
/// terminal ever learns how big it actually is -- it covers a split, a
/// separator drag, a zoom toggle and a plain window resize alike. A no-op
/// unless the whole-cell count actually changed (`Terminal::resize` locks the
/// grid and issues a `TIOCSWINSZ`, so it must not run every frame).
fn fit_terminal(
    terminal: &Terminal,
    bounds: Bounds<Pixels>,
    cell_width: Pixels,
    cell_height: Pixels,
    scrollback: usize,
) {
    if bounds.size.width <= px(0.0) || bounds.size.height <= px(0.0) {
        return; // not laid out yet -- don't collapse the PTY to 1x1
    }
    let cols = (f32::from(bounds.size.width) / f32::from(cell_width)).floor();
    let rows = (f32::from(bounds.size.height) / f32::from(cell_height)).floor();
    let cols = (cols.max(1.0) as u16).max(1);
    let rows = (rows.max(1.0) as u16).max(1);
    if cols == terminal.cols.get() && rows == terminal.rows.get() {
        return;
    }
    terminal.resize(
        cols,
        rows,
        scrollback,
        f32::from(cell_width).round().max(1.0) as u16,
        f32::from(cell_height).round().max(1.0) as u16,
    );
}
