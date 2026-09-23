// `RectCache` and the pixel-rect-driven pane operations (`focus_dir`,
// `adjust_ratio`, `drag_separator`).

use super::{FocusDir, PaneForest, PaneTree, SplitDir};

/// Last-painted pixel bounds for every leaf and every separator in one
/// tab's pane tree, refreshed every frame as render() walks the tree (Task
/// 3) -- the taffy-flex replacement for PaneNode's old cached `rect` field.
/// Keyed the same way mouse.rs's CLICK_STATE is (plain integer keys, no
/// lifetime issues from holding gpui types across frames).
#[derive(Default)]
pub struct RectCache {
    /// terminal_id -> last-painted bounds.
    pub leaves: std::collections::HashMap<usize, gpui::Bounds<gpui::Pixels>>,
    /// Split node_id -> last-painted bounds of THE WHOLE SPLIT CONTAINER
    /// (both children plus the divider between them), not the thin divider's
    /// own rect.
    ///
    /// `drag_split_ratio` divides by this rect's extent in the resize axis to
    /// turn a pointer position into a 0..1 ratio; the divider's own ~6px
    /// width in that denominator would make every pointer more than a few
    /// pixels away clamp straight to 0.1 or 0.9. The flex walk
    /// (`pane_view::render_split`) populates this from the union of the
    /// container's three children, which tile it exactly.
    pub separators: std::collections::HashMap<u32, gpui::Bounds<gpui::Pixels>>,
}

impl PaneForest {
    /// Move focus to the nearest pane in `dir`, using each leaf's last-
    /// painted center point from `rects`. Does nothing if there's no pane
    /// in that direction, or if `rects` has no entry yet for the focused
    /// leaf (first frame before any paint has happened).
    pub fn focus_dir(&mut self, dir: FocusDir, rects: &RectCache) {
        let Some(focused_bounds) = rects.leaves.get(&self.focused_terminal) else {
            return;
        };
        let fc_x = f32::from(focused_bounds.origin.x) + f32::from(focused_bounds.size.width) * 0.5;
        let fc_y = f32::from(focused_bounds.origin.y) + f32::from(focused_bounds.size.height) * 0.5;

        let mut best_id: Option<usize> = None;
        let mut best_dist = f32::MAX;
        for id in self.root.leaf_ids() {
            if id == self.focused_terminal {
                continue;
            }
            let Some(bounds) = rects.leaves.get(&id) else {
                continue;
            };
            let cx = f32::from(bounds.origin.x) + f32::from(bounds.size.width) * 0.5;
            let cy = f32::from(bounds.origin.y) + f32::from(bounds.size.height) * 0.5;
            let in_dir = match dir {
                FocusDir::Left => cx < fc_x,
                FocusDir::Right => cx > fc_x,
                FocusDir::Up => cy < fc_y,
                FocusDir::Down => cy > fc_y,
            };
            if !in_dir {
                continue;
            }
            let dist = (cx - fc_x).powi(2) + (cy - fc_y).powi(2);
            if dist < best_dist {
                best_dist = dist;
                best_id = Some(id);
            }
        }
        if let Some(id) = best_id {
            self.focused_terminal = id;
        }
    }

    /// Adjust the ratio of the closest ancestor Split in `dir`'s axis by
    /// `delta` (always positive; sign is inferred from `dir`, matching
    /// src/ui/panes.rs's adjust_ratio exactly -- ported as-is, this part
    /// needs no rect at all).
    pub fn adjust_ratio(&mut self, dir: FocusDir, delta: f32) {
        let target_dir = match dir {
            FocusDir::Left | FocusDir::Right => SplitDir::Horizontal,
            FocusDir::Up | FocusDir::Down => SplitDir::Vertical,
        };
        let signed = match dir {
            FocusDir::Right | FocusDir::Down => delta,
            FocusDir::Left | FocusDir::Up => -delta,
        };
        adjust_parent_split(&mut self.root, self.focused_terminal, target_dir, signed);
    }

    /// Drag the separator owned by the Split with `node_id` to the current
    /// mouse position, using that SPLIT's own last-painted bounds from
    /// `rects` (the RectCache-based replacement for src/ui/panes.rs's `rect`
    /// field on the Split node itself, which held exactly the same thing --
    /// see `RectCache::separators`' doc comment).
    pub fn drag_separator(&mut self, node_id: u32, mouse_x: f32, mouse_y: f32, rects: &RectCache) {
        let Some(bounds) = rects.separators.get(&node_id) else {
            return;
        };
        let x = f32::from(bounds.origin.x);
        let y = f32::from(bounds.origin.y);
        let w = f32::from(bounds.size.width).max(1.0);
        let h = f32::from(bounds.size.height).max(1.0);
        drag_split_ratio(&mut self.root, node_id, mouse_x, mouse_y, x, y, w, h);
    }
}

fn contains_leaf(node: &PaneTree, target: usize) -> bool {
    match node {
        PaneTree::Leaf { terminal_id } => *terminal_id == target,
        PaneTree::Split { left, right, .. } => {
            contains_leaf(left, target) || contains_leaf(right, target)
        }
    }
}

/// Ported from src/ui/panes.rs's adjust_parent_split as-is (no rect
/// involved in this one at all).
fn adjust_parent_split(
    node: &mut PaneTree,
    target: usize,
    target_dir: SplitDir,
    delta: f32,
) -> bool {
    match node {
        PaneTree::Leaf { .. } => false,
        PaneTree::Split {
            dir,
            ratio,
            left,
            right,
            ..
        } => {
            let in_left = contains_leaf(left, target);
            if !in_left && !contains_leaf(right, target) {
                return false;
            }
            let child_found = if in_left {
                adjust_parent_split(left, target, target_dir, delta)
            } else {
                adjust_parent_split(right, target, target_dir, delta)
            };
            if child_found {
                return true;
            }
            if *dir == target_dir {
                *ratio = (*ratio + delta).clamp(0.1, 0.9);
                return true;
            }
            false
        }
    }
}

/// Ported from src/ui/panes.rs's drag_split_ratio, with the split's own
/// rect passed in explicitly (x, y, w, h) instead of read from a `rect`
/// field on the node itself. (`sep_*` here is the SPLIT's rect -- the one
/// the separator divides -- not the divider strip's own; see
/// `RectCache::separators`.)
#[allow(clippy::too_many_arguments)]
fn drag_split_ratio(
    node: &mut PaneTree,
    target_id: u32,
    mouse_x: f32,
    mouse_y: f32,
    sep_x: f32,
    sep_y: f32,
    sep_w: f32,
    sep_h: f32,
) -> bool {
    match node {
        PaneTree::Leaf { .. } => false,
        PaneTree::Split {
            node_id,
            dir,
            ratio,
            left,
            right,
        } => {
            if *node_id == target_id {
                let new_ratio = match dir {
                    SplitDir::Horizontal => (mouse_x - sep_x) / sep_w,
                    SplitDir::Vertical => (mouse_y - sep_y) / sep_h,
                };
                *ratio = new_ratio.clamp(0.1, 0.9);
                return true;
            }
            drag_split_ratio(
                left, target_id, mouse_x, mouse_y, sep_x, sep_y, sep_w, sep_h,
            ) || drag_split_ratio(
                right, target_id, mouse_x, mouse_y, sep_x, sep_y, sep_w, sep_h,
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn root_node_id(forest: &PaneForest) -> u32 {
        match &forest.root {
            PaneTree::Split { node_id, .. } => *node_id,
            PaneTree::Leaf { .. } => panic!("root is not a split"),
        }
    }

    fn root_ratio(forest: &PaneForest) -> f32 {
        match &forest.root {
            PaneTree::Split { ratio, .. } => *ratio,
            PaneTree::Leaf { .. } => panic!("root is not a split"),
        }
    }

    /// A 900x565 split container at (0, 36) -- the real geometry a 900x600
    /// window produces below the tab bar.
    fn split_bounds() -> gpui::Bounds<gpui::Pixels> {
        gpui::Bounds {
            origin: gpui::point(gpui::px(0.0), gpui::px(36.0)),
            size: gpui::size(gpui::px(900.0), gpui::px(565.0)),
        }
    }

    #[test]
    fn drag_separator_tracks_the_pointer_across_the_whole_split() {
        // Pins what `RectCache::separators` must hold: the SPLIT CONTAINER's
        // bounds. Populated with the 6px divider's own rect instead, every
        // position below would divide by ~6 and clamp to 0.9.
        let mut forest = PaneForest::new(1);
        forest.split(SplitDir::Horizontal, 2);
        let node_id = root_node_id(&forest);
        let mut rects = RectCache::default();
        rects.separators.insert(node_id, split_bounds());

        forest.drag_separator(node_id, 225.0, 300.0, &rects);
        assert!((root_ratio(&forest) - 0.25).abs() < 0.01);
        forest.drag_separator(node_id, 675.0, 300.0, &rects);
        assert!((root_ratio(&forest) - 0.75).abs() < 0.01);
        // Past the ends, the ratio clamps rather than inverting the panes.
        forest.drag_separator(node_id, -50.0, 300.0, &rects);
        assert!((root_ratio(&forest) - 0.1).abs() < f32::EPSILON);
        forest.drag_separator(node_id, 5000.0, 300.0, &rects);
        assert!((root_ratio(&forest) - 0.9).abs() < f32::EPSILON);
    }

    #[test]
    fn vertical_drag_uses_the_y_axis_and_the_container_origin() {
        let mut forest = PaneForest::new(1);
        forest.split(SplitDir::Vertical, 2);
        let node_id = root_node_id(&forest);
        let mut rects = RectCache::default();
        rects.separators.insert(node_id, split_bounds());

        // y = 36 + 565/4 -> a quarter of the way down the container, NOT of
        // the window (the container starts below the tab bar).
        forest.drag_separator(node_id, 400.0, 36.0 + 141.25, &rects);
        assert!((root_ratio(&forest) - 0.25).abs() < 0.01);
    }

    #[test]
    fn drag_separator_without_a_cached_rect_is_a_no_op() {
        // First frame: nothing has been painted yet, so there is no geometry
        // to compute a ratio from.
        let mut forest = PaneForest::new(1);
        forest.split(SplitDir::Horizontal, 2);
        let node_id = root_node_id(&forest);
        forest.drag_separator(node_id, 225.0, 300.0, &RectCache::default());
        assert!((root_ratio(&forest) - 0.5).abs() < f32::EPSILON);
    }
}
