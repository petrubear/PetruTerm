#![allow(dead_code)]

/// Split direction for pane splits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SplitDir {
    Horizontal, // left | right
    Vertical,   // top / bottom
}

/// Vim-style direction for pane focus movement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusDir {
    Left,
    Right,
    Up,
    Down,
}

/// Pixel rectangle for a pane.
#[derive(Debug, Clone, Copy)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

/// Binary tree node for pane layout.
#[derive(Debug)]
pub enum PaneNode {
    Leaf {
        /// Terminal ID (maps to a Terminal instance in the pane manager).
        terminal_id: usize,
        /// Cached pixel rect (recomputed on resize).
        rect: Rect,
    },
    Split {
        dir: SplitDir,
        /// Split ratio: 0.0 = all left/top, 1.0 = all right/bottom.
        ratio: f32,
        left: Box<PaneNode>,
        right: Box<PaneNode>,
        /// Cached total rect for this node.
        rect: Rect,
    },
}

impl PaneNode {
    fn leaf(terminal_id: usize, rect: Rect) -> Self {
        PaneNode::Leaf { terminal_id, rect }
    }

    /// Recursively layout all nodes, setting cached rects.
    pub fn layout(&mut self, rect: Rect) {
        match self {
            PaneNode::Leaf { rect: r, .. } => *r = rect,
            PaneNode::Split { dir, ratio, left, right, rect: r } => {
                *r = rect;
                match dir {
                    SplitDir::Horizontal => {
                        let split_x = rect.x + rect.w * *ratio;
                        left.layout(Rect { x: rect.x, y: rect.y, w: split_x - rect.x, h: rect.h });
                        right.layout(Rect { x: split_x, y: rect.y, w: rect.x + rect.w - split_x, h: rect.h });
                    }
                    SplitDir::Vertical => {
                        let split_y = rect.y + rect.h * *ratio;
                        left.layout(Rect { x: rect.x, y: rect.y, w: rect.w, h: split_y - rect.y });
                        right.layout(Rect { x: rect.x, y: split_y, w: rect.w, h: rect.y + rect.h - split_y });
                    }
                }
            }
        }
    }

    /// Find the terminal_id whose rect contains point (px, py).
    pub fn hit_test(&self, px: f32, py: f32) -> Option<usize> {
        match self {
            PaneNode::Leaf { terminal_id, rect } => {
                if px >= rect.x && px < rect.x + rect.w && py >= rect.y && py < rect.y + rect.h {
                    Some(*terminal_id)
                } else {
                    None
                }
            }
            PaneNode::Split { left, right, .. } => {
                left.hit_test(px, py).or_else(|| right.hit_test(px, py))
            }
        }
    }

    /// Collect all leaf terminal IDs in order (left-to-right, top-to-bottom).
    pub fn leaf_ids(&self) -> Vec<usize> {
        match self {
            PaneNode::Leaf { terminal_id, .. } => vec![*terminal_id],
            PaneNode::Split { left, right, .. } => {
                let mut ids = left.leaf_ids();
                ids.extend(right.leaf_ids());
                ids
            }
        }
    }

    pub fn rect(&self) -> Rect {
        match self {
            PaneNode::Leaf { rect, .. } | PaneNode::Split { rect, .. } => *rect,
        }
    }
}

/// Manages the pane layout tree for a single tab.
pub struct PaneManager {
    pub root: PaneNode,
    pub focused_terminal: usize,
}

impl PaneManager {
    pub fn new(viewport: Rect, terminal_id: usize) -> Self {
        let root = PaneNode::leaf(terminal_id, viewport);
        Self { root, focused_terminal: terminal_id }
    }

    /// Relayout the tree to fill the given viewport.
    pub fn resize(&mut self, viewport: Rect) {
        self.root.layout(viewport);
    }

    /// Split the focused pane using a caller-supplied terminal ID (from Mux.next_terminal_id).
    pub fn split(&mut self, dir: SplitDir, new_id: usize) {
        let focused = self.focused_terminal;
        split_node(&mut self.root, focused, dir, new_id);
        self.root.layout(self.root.rect());
        self.focused_terminal = new_id;
    }

    /// Close the focused pane. Returns the terminal ID that was closed (if any).
    pub fn close_focused(&mut self) -> Option<usize> {
        let closed = self.focused_terminal;
        let leaf_ids = self.root.leaf_ids();
        if leaf_ids.len() <= 1 {
            return None; // Can't close the last pane.
        }

        // Find the sibling to promote; focus moves to nearest leaf.
        if remove_leaf(&mut self.root, closed) {
            self.root.layout(self.root.rect());
            // Move focus to first remaining leaf.
            self.focused_terminal = self.root.leaf_ids()[0];
            Some(closed)
        } else {
            None
        }
    }

    pub fn focus_at(&mut self, px: f32, py: f32) {
        if let Some(id) = self.root.hit_test(px, py) {
            self.focused_terminal = id;
        }
    }

    pub fn rect_of(&self, terminal_id: usize) -> Option<Rect> {
        find_rect(&self.root, terminal_id)
    }
}

fn split_node(node: &mut PaneNode, target: usize, dir: SplitDir, new_id: usize) -> bool {
    match node {
        PaneNode::Leaf { terminal_id, rect } if *terminal_id == target => {
            let old_rect = *rect;
            let old_id = *terminal_id;
            *node = PaneNode::Split {
                dir,
                ratio: 0.5,
                left: Box::new(PaneNode::leaf(old_id, old_rect)),
                right: Box::new(PaneNode::leaf(new_id, old_rect)),
                rect: old_rect,
            };
            true
        }
        PaneNode::Leaf { .. } => false,
        PaneNode::Split { left, right, .. } => {
            split_node(left, target, dir, new_id) || split_node(right, target, dir, new_id)
        }
    }
}

fn remove_leaf(node: &mut PaneNode, target: usize) -> bool {
    match node {
        PaneNode::Leaf { .. } => false,
        PaneNode::Split { left, right, rect, .. } => {
            let left_is_target = matches!(left.as_ref(), PaneNode::Leaf { terminal_id, .. } if *terminal_id == target);
            let right_is_target = matches!(right.as_ref(), PaneNode::Leaf { terminal_id, .. } if *terminal_id == target);
            let old_rect = *rect;

            if left_is_target {
                // Promote right child.
                let mut new_node = std::mem::replace(right.as_mut(), PaneNode::leaf(0, old_rect));
                new_node.layout(old_rect);
                *node = new_node;
                return true;
            }
            if right_is_target {
                let mut new_node = std::mem::replace(left.as_mut(), PaneNode::leaf(0, old_rect));
                new_node.layout(old_rect);
                *node = new_node;
                return true;
            }
            remove_leaf(left, target) || remove_leaf(right, target)
        }
    }
}

fn find_rect(node: &PaneNode, target: usize) -> Option<Rect> {
    match node {
        PaneNode::Leaf { terminal_id, rect } if *terminal_id == target => Some(*rect),
        PaneNode::Leaf { .. } => None,
        PaneNode::Split { left, right, .. } => {
            find_rect(left, target).or_else(|| find_rect(right, target))
        }
    }
}

// ── Multi-pane rendering helpers ─────────────────────────────────────────────

/// Rendering info for a single leaf pane, relative to the terminal viewport origin.
#[derive(Debug, Clone, Copy)]
pub struct PaneInfo {
    pub terminal_id: usize,
    /// Column offset from the viewport left edge (in cell units).
    pub col_offset: usize,
    /// Row offset from the viewport top edge (in cell units).
    pub row_offset: usize,
    /// Width of this pane in terminal columns.
    pub cols: usize,
    /// Height of this pane in terminal rows.
    pub rows: usize,
    /// Whether this pane currently has keyboard focus.
    pub focused: bool,
}

/// A separator line between two adjacent panes.
#[derive(Debug, Clone, Copy)]
pub struct PaneSeparator {
    /// `true` = vertical divider (left|right split); `false` = horizontal (top/bottom).
    pub vertical: bool,
    /// For vertical: column where the divider sits. For horizontal: first column.
    pub col: usize,
    /// For horizontal: row where the divider sits. For vertical: first row.
    pub row: usize,
    /// Extent: rows (vertical) or columns (horizontal).
    pub length: usize,
}

impl PaneManager {
    /// Collect info for all leaf panes, including their viewport-relative cell offsets.
    pub fn pane_infos(&self, viewport: Rect, cell_w: f32, cell_h: f32) -> Vec<PaneInfo> {
        let mut result = Vec::new();
        collect_leaf_infos_impl(&self.root, viewport, cell_w, cell_h, self.focused_terminal, &mut result);
        result
    }

    /// Collect separator lines between panes (one per internal Split node).
    pub fn pane_separators(&self, viewport: Rect, cell_w: f32, cell_h: f32) -> Vec<PaneSeparator> {
        let mut result = Vec::new();
        collect_separators_impl(&self.root, viewport, cell_w, cell_h, &mut result);
        result
    }
}

fn collect_leaf_infos_impl(
    node: &PaneNode,
    viewport: Rect,
    cell_w: f32,
    cell_h: f32,
    focused: usize,
    result: &mut Vec<PaneInfo>,
) {
    match node {
        PaneNode::Leaf { terminal_id, rect } => {
            let col_offset = ((rect.x - viewport.x) / cell_w).round() as usize;
            let row_offset = ((rect.y - viewport.y) / cell_h).round() as usize;
            let cols = (rect.w / cell_w).floor() as usize;
            let rows = (rect.h / cell_h).floor() as usize;
            result.push(PaneInfo {
                terminal_id: *terminal_id,
                col_offset, row_offset,
                cols: cols.max(1),
                rows: rows.max(1),
                focused: *terminal_id == focused,
            });
        }
        PaneNode::Split { left, right, .. } => {
            collect_leaf_infos_impl(left, viewport, cell_w, cell_h, focused, result);
            collect_leaf_infos_impl(right, viewport, cell_w, cell_h, focused, result);
        }
    }
}

fn collect_separators_impl(
    node: &PaneNode,
    viewport: Rect,
    cell_w: f32,
    cell_h: f32,
    result: &mut Vec<PaneSeparator>,
) {
    match node {
        PaneNode::Leaf { .. } => {}
        PaneNode::Split { dir, left, right, rect, .. } => {
            let rect = *rect;
            match dir {
                SplitDir::Horizontal => {
                    // Vertical separator at the right edge of the left child.
                    let left_rect = left.rect();
                    let sep_col = ((left_rect.x + left_rect.w - viewport.x) / cell_w).round() as usize;
                    let sep_row = ((rect.y - viewport.y) / cell_h).round() as usize;
                    let length = (rect.h / cell_h).floor() as usize;
                    result.push(PaneSeparator { vertical: true, col: sep_col, row: sep_row, length });
                }
                SplitDir::Vertical => {
                    // Horizontal separator at the bottom edge of the top child.
                    let left_rect = left.rect();
                    let sep_row = ((left_rect.y + left_rect.h - viewport.y) / cell_h).round() as usize;
                    let sep_col = ((rect.x - viewport.x) / cell_w).round() as usize;
                    let length = (rect.w / cell_w).floor() as usize;
                    result.push(PaneSeparator { vertical: false, col: sep_col, row: sep_row, length });
                }
            }
            collect_separators_impl(left, viewport, cell_w, cell_h, result);
            collect_separators_impl(right, viewport, cell_w, cell_h, result);
        }
    }
}
