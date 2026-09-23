// Pane-split tree (`PaneTree`/`PaneForest` and their split/close mutation
// logic), ported from src/ui/panes.rs's algorithms. The tree stores no rects:
// render() walks it into nested taffy flex divs, and anything needing a
// leaf's or separator's pixel rect (focus_dir, adjust_ratio,
// drag_split_ratio) reads it from `RectCache` (`geometry.rs`), populated by
// each frame's paint pass.

mod geometry;

pub use geometry::RectCache;

use std::sync::atomic::{AtomicU32, Ordering};

static NEXT_NODE_ID: AtomicU32 = AtomicU32::new(1);

pub(crate) fn next_node_id() -> u32 {
    NEXT_NODE_ID.fetch_add(1, Ordering::Relaxed)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SplitDir {
    Horizontal, // left | right
    Vertical,   // top / bottom
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusDir {
    Left,
    Right,
    Up,
    Down,
}

/// Binary tree node for pane layout -- see this module's doc comment for
/// why there is no `rect` field (unlike src/ui/panes.rs's PaneNode).
#[derive(Debug)]
pub enum PaneTree {
    Leaf {
        terminal_id: usize,
    },
    Split {
        /// Stable ID assigned at creation -- survives tree restructuring.
        /// Used to key RectCache entries and to find this exact node again
        /// during a drag, immune to concurrent splits/closes elsewhere in
        /// the tree.
        node_id: u32,
        dir: SplitDir,
        /// 0.0 = all left/top, 1.0 = all right/bottom.
        ratio: f32,
        left: Box<PaneTree>,
        right: Box<PaneTree>,
    },
}

impl PaneTree {
    fn leaf(terminal_id: usize) -> Self {
        PaneTree::Leaf { terminal_id }
    }

    pub fn leaf_ids(&self) -> Vec<usize> {
        match self {
            PaneTree::Leaf { terminal_id } => vec![*terminal_id],
            PaneTree::Split { left, right, .. } => {
                let mut ids = left.leaf_ids();
                ids.extend(right.leaf_ids());
                ids
            }
        }
    }

    pub fn leaf_count(&self) -> usize {
        match self {
            PaneTree::Leaf { .. } => 1,
            PaneTree::Split { left, right, .. } => left.leaf_count() + right.leaf_count(),
        }
    }
}

fn split_node(node: &mut PaneTree, target: usize, dir: SplitDir, new_id: usize) -> bool {
    match node {
        PaneTree::Leaf { terminal_id } if *terminal_id == target => {
            let old_id = *terminal_id;
            *node = PaneTree::Split {
                node_id: next_node_id(),
                dir,
                ratio: 0.5,
                left: Box::new(PaneTree::leaf(old_id)),
                right: Box::new(PaneTree::leaf(new_id)),
            };
            true
        }
        PaneTree::Leaf { .. } => false,
        PaneTree::Split { left, right, .. } => {
            split_node(left, target, dir, new_id) || split_node(right, target, dir, new_id)
        }
    }
}

fn remove_leaf(node: &mut PaneTree, target: usize) -> bool {
    match node {
        PaneTree::Leaf { .. } => false,
        PaneTree::Split { left, right, .. } => {
            let left_is_target =
                matches!(left.as_ref(), PaneTree::Leaf { terminal_id } if *terminal_id == target);
            let right_is_target =
                matches!(right.as_ref(), PaneTree::Leaf { terminal_id } if *terminal_id == target);

            if left_is_target {
                let new_node = std::mem::replace(right.as_mut(), PaneTree::leaf(0));
                *node = new_node;
                return true;
            }
            if right_is_target {
                let new_node = std::mem::replace(left.as_mut(), PaneTree::leaf(0));
                *node = new_node;
                return true;
            }
            remove_leaf(left, target) || remove_leaf(right, target)
        }
    }
}

/// One pane-split tree for one tab -- ported from src/ui/panes.rs's
/// PaneManager, minus the viewport/Rect-resize responsibility (taffy owns
/// that now, via render()).
pub struct PaneForest {
    pub root: PaneTree,
    pub focused_terminal: usize,
}

impl PaneForest {
    pub fn new(terminal_id: usize) -> Self {
        Self {
            root: PaneTree::leaf(terminal_id),
            focused_terminal: terminal_id,
        }
    }

    /// Split the focused pane using a caller-supplied terminal ID.
    pub fn split(&mut self, dir: SplitDir, new_id: usize) {
        let focused = self.focused_terminal;
        split_node(&mut self.root, focused, dir, new_id);
        self.focused_terminal = new_id;
    }

    /// Close the focused pane. Returns the terminal ID that was closed, or
    /// None if it was the last pane (caller must close the whole tab instead).
    pub fn close_focused(&mut self) -> Option<usize> {
        let closed = self.focused_terminal;
        if self.root.leaf_count() <= 1 {
            return None;
        }
        if remove_leaf(&mut self.root, closed) {
            self.focused_terminal = self.root.leaf_ids()[0];
            Some(closed)
        } else {
            None
        }
    }

    /// Close a specific pane by terminal_id (e.g. after its shell process
    /// exits). Returns false (does nothing) if it's the only pane.
    pub fn close_specific(&mut self, terminal_id: usize) -> bool {
        if self.root.leaf_count() <= 1 {
            return false;
        }
        if remove_leaf(&mut self.root, terminal_id) {
            if self.focused_terminal == terminal_id {
                self.focused_terminal = self.root.leaf_ids()[0];
            }
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_replaces_focused_leaf_with_a_split_node() {
        let mut forest = PaneForest::new(1);
        forest.split(SplitDir::Horizontal, 2);
        assert_eq!(forest.focused_terminal, 2);
        let mut ids = forest.root.leaf_ids();
        ids.sort();
        assert_eq!(ids, vec![1, 2]);
        assert_eq!(forest.root.leaf_count(), 2);
    }

    #[test]
    fn close_focused_promotes_the_sibling() {
        let mut forest = PaneForest::new(1);
        forest.split(SplitDir::Horizontal, 2); // focus now on 2
        let closed = forest.close_focused();
        assert_eq!(closed, Some(2));
        assert_eq!(forest.root.leaf_ids(), vec![1]);
        assert_eq!(forest.focused_terminal, 1);
    }

    #[test]
    fn close_focused_refuses_to_close_the_last_pane() {
        let mut forest = PaneForest::new(1);
        assert_eq!(forest.close_focused(), None);
        assert_eq!(forest.root.leaf_ids(), vec![1]);
    }

    #[test]
    fn close_specific_moves_focus_off_the_closed_pane() {
        let mut forest = PaneForest::new(1);
        forest.split(SplitDir::Horizontal, 2);
        forest.split(SplitDir::Vertical, 3); // splits pane 2 (focused)
        assert!(forest.close_specific(2));
        let mut ids = forest.root.leaf_ids();
        ids.sort();
        assert_eq!(ids, vec![1, 3]);
    }

    #[test]
    fn nested_split_produces_three_leaves() {
        let mut forest = PaneForest::new(1);
        forest.split(SplitDir::Horizontal, 2);
        forest.split(SplitDir::Vertical, 3); // splits pane 2
        assert_eq!(forest.root.leaf_count(), 3);
    }
}
