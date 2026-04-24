#![allow(dead_code)]

use std::sync::atomic::{AtomicU32, Ordering};

static NEXT_NODE_ID: AtomicU32 = AtomicU32::new(1);

fn next_node_id() -> u32 {
    NEXT_NODE_ID.fetch_add(1, Ordering::Relaxed)
}

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
        /// Stable ID assigned at creation — survives layout recalculations.
        node_id: u32,
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
            PaneNode::Split {
                dir,
                ratio,
                left,
                right,
                rect: r,
                ..
            } => {
                *r = rect;
                match dir {
                    SplitDir::Horizontal => {
                        let split_x = rect.x + rect.w * *ratio;
                        left.layout(Rect {
                            x: rect.x,
                            y: rect.y,
                            w: split_x - rect.x,
                            h: rect.h,
                        });
                        right.layout(Rect {
                            x: split_x,
                            y: rect.y,
                            w: rect.x + rect.w - split_x,
                            h: rect.h,
                        });
                    }
                    SplitDir::Vertical => {
                        let split_y = rect.y + rect.h * *ratio;
                        left.layout(Rect {
                            x: rect.x,
                            y: rect.y,
                            w: rect.w,
                            h: split_y - rect.y,
                        });
                        right.layout(Rect {
                            x: rect.x,
                            y: split_y,
                            w: rect.w,
                            h: rect.y + rect.h - split_y,
                        });
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

    /// Count leaf panes recursively.
    pub fn leaf_count(&self) -> usize {
        match self {
            PaneNode::Leaf { .. } => 1,
            PaneNode::Split { left, right, .. } => left.leaf_count() + right.leaf_count(),
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
        Self {
            root,
            focused_terminal: terminal_id,
        }
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

    /// Close a specific pane by terminal_id (e.g. after the shell process exits).
    /// Returns true if the pane was found and removed.
    /// If there is only one pane, returns false (caller must close the whole tab).
    pub fn close_specific(&mut self, terminal_id: usize) -> bool {
        let leaf_ids = self.root.leaf_ids();
        if leaf_ids.len() <= 1 {
            return false;
        }
        if remove_leaf(&mut self.root, terminal_id) {
            self.root.layout(self.root.rect());
            if self.focused_terminal == terminal_id {
                self.focused_terminal = self.root.leaf_ids()[0];
            }
            true
        } else {
            false
        }
    }

    pub fn focus_at(&mut self, px: f32, py: f32) {
        if let Some(id) = self.root.hit_test(px, py) {
            self.focused_terminal = id;
        }
    }

    /// Move focus to the nearest pane in `dir` using center-point geometry.
    /// Does nothing if there is no pane in that direction.
    pub fn focus_dir(&mut self, dir: FocusDir) {
        let focused_rect = match self.rect_of(self.focused_terminal) {
            Some(r) => r,
            None => return,
        };
        let fc_x = focused_rect.x + focused_rect.w * 0.5;
        let fc_y = focused_rect.y + focused_rect.h * 0.5;

        let leaves = self.root.leaf_ids();
        let mut best_id: Option<usize> = None;
        let mut best_dist = f32::MAX;

        for id in leaves {
            if id == self.focused_terminal {
                continue;
            }
            let rect = match find_rect(&self.root, id) {
                Some(r) => r,
                None => continue,
            };
            let cx = rect.x + rect.w * 0.5;
            let cy = rect.y + rect.h * 0.5;

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
                node_id: next_node_id(),
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
        PaneNode::Split {
            left, right, rect, ..
        } => {
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
    /// Column offset from the viewport left edge (in cell units), after separator inset.
    pub col_offset: usize,
    /// Row offset from the viewport top edge (in cell units), after separator inset.
    pub row_offset: usize,
    /// Width of this pane in terminal columns (after separator inset).
    pub cols: usize,
    /// Height of this pane in terminal rows (after separator inset).
    pub rows: usize,
    /// Whether this pane currently has keyboard focus.
    pub focused: bool,
    /// Raw pixel rect of this pane in the same coordinate space as the viewport rect.
    /// Use this (not col/row_offset) to align with separator lines.
    pub pane_rect: crate::ui::Rect,
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
    /// Stable ID of the Split node that owns this separator.
    pub node_id: u32,
}

impl PaneManager {
    /// Collect info for all leaf panes, including their viewport-relative cell offsets.
    /// Each pane insets its usable area by 1 cell on sides adjacent to a separator so that
    /// content never renders flush against the divider line.
    pub fn pane_infos(&self, viewport: Rect, cell_w: f32, cell_h: f32) -> Vec<PaneInfo> {
        let mut result = Vec::new();
        collect_leaf_infos_impl(
            &self.root,
            viewport,
            cell_w,
            cell_h,
            self.focused_terminal,
            PanePad::default(),
            &mut result,
        );
        result
    }

    /// Collect separator lines between panes (one per internal Split node).
    pub fn pane_separators(&self, viewport: Rect, cell_w: f32, cell_h: f32) -> Vec<PaneSeparator> {
        let mut result = Vec::new();
        collect_separators_impl(&self.root, viewport, cell_w, cell_h, &mut result);
        result
    }
}

/// 1-cell inset flags: which sides of a pane border an adjacent separator.
#[derive(Clone, Copy, Default)]
struct PanePad {
    left: bool,
    right: bool,
    top: bool,
    bottom: bool,
}

fn collect_leaf_infos_impl(
    node: &PaneNode,
    viewport: Rect,
    cell_w: f32,
    cell_h: f32,
    focused: usize,
    pad: PanePad,
    result: &mut Vec<PaneInfo>,
) {
    match node {
        PaneNode::Leaf { terminal_id, rect } => {
            let pl = pad.left as usize;
            let pr = pad.right as usize;
            let pt = pad.top as usize;
            let pb = pad.bottom as usize;
            let col_offset = ((rect.x - viewport.x) / cell_w).round() as usize + pl;
            let row_offset = ((rect.y - viewport.y) / cell_h).round() as usize + pt;
            let cols = ((rect.w / cell_w).floor() as usize).saturating_sub(pl + pr);
            let rows = ((rect.h / cell_h).floor() as usize).saturating_sub(pt + pb);
            // Snap pane_rect edges to the cell grid so the focus border aligns exactly
            // with separator lines, which use the same round() formula.
            let snap_x = |px: f32| viewport.x + ((px - viewport.x) / cell_w).round() * cell_w;
            let snap_y = |py: f32| viewport.y + ((py - viewport.y) / cell_h).round() * cell_h;
            let sx = snap_x(rect.x);
            let sy = snap_y(rect.y);
            let sx2 = snap_x(rect.x + rect.w);
            let sy2 = snap_y(rect.y + rect.h);
            result.push(PaneInfo {
                terminal_id: *terminal_id,
                col_offset,
                row_offset,
                cols: cols.max(1),
                rows: rows.max(1),
                focused: *terminal_id == focused,
                pane_rect: crate::ui::Rect { x: sx, y: sy, w: sx2 - sx, h: sy2 - sy },
            });
        }
        PaneNode::Split {
            dir, left, right, ..
        } => match dir {
            SplitDir::Horizontal => {
                collect_leaf_infos_impl(
                    left,
                    viewport,
                    cell_w,
                    cell_h,
                    focused,
                    PanePad { right: true, ..pad },
                    result,
                );
                collect_leaf_infos_impl(
                    right,
                    viewport,
                    cell_w,
                    cell_h,
                    focused,
                    PanePad { left: true, ..pad },
                    result,
                );
            }
            SplitDir::Vertical => {
                collect_leaf_infos_impl(
                    left,
                    viewport,
                    cell_w,
                    cell_h,
                    focused,
                    PanePad {
                        bottom: true,
                        ..pad
                    },
                    result,
                );
                collect_leaf_infos_impl(
                    right,
                    viewport,
                    cell_w,
                    cell_h,
                    focused,
                    PanePad { top: true, ..pad },
                    result,
                );
            }
        },
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
        PaneNode::Split {
            node_id,
            dir,
            left,
            right,
            rect,
            ..
        } => {
            let rect = *rect;
            let nid = *node_id;
            match dir {
                SplitDir::Horizontal => {
                    // Vertical separator at the right edge of the left child.
                    let left_rect = left.rect();
                    let sep_col =
                        ((left_rect.x + left_rect.w - viewport.x) / cell_w).round() as usize;
                    let sep_row = ((rect.y - viewport.y) / cell_h).round() as usize;
                    let length = (rect.h / cell_h).floor() as usize;
                    result.push(PaneSeparator {
                        vertical: true,
                        col: sep_col,
                        row: sep_row,
                        length,
                        node_id: nid,
                    });
                }
                SplitDir::Vertical => {
                    // Horizontal separator at the bottom edge of the top child.
                    let left_rect = left.rect();
                    let sep_row =
                        ((left_rect.y + left_rect.h - viewport.y) / cell_h).round() as usize;
                    let sep_col = ((rect.x - viewport.x) / cell_w).round() as usize;
                    let length = (rect.w / cell_w).floor() as usize;
                    result.push(PaneSeparator {
                        vertical: false,
                        col: sep_col,
                        row: sep_row,
                        length,
                        node_id: nid,
                    });
                }
            }
            collect_separators_impl(left, viewport, cell_w, cell_h, result);
            collect_separators_impl(right, viewport, cell_w, cell_h, result);
        }
    }
}

// ── Pane resize helpers ───────────────────────────────────────────────────────

impl PaneManager {
    /// Adjust the ratio of the closest ancestor Split in `dir`'s axis.
    /// `delta` is always positive; sign is inferred from `dir`:
    ///   Right/Down → +delta (separator moves right/down).
    ///   Left/Up    → -delta (separator moves left/up).
    pub fn adjust_ratio(&mut self, focused_id: usize, dir: FocusDir, delta: f32) {
        let target_dir = match dir {
            FocusDir::Left | FocusDir::Right => SplitDir::Horizontal,
            FocusDir::Up | FocusDir::Down => SplitDir::Vertical,
        };
        let signed = match dir {
            FocusDir::Right | FocusDir::Down => delta,
            FocusDir::Left | FocusDir::Up => -delta,
        };
        if adjust_parent_split(&mut self.root, focused_id, target_dir, signed) {
            let r = self.root.rect();
            self.root.layout(r);
        }
    }

    /// Drag the separator owned by the Split with `node_id` to the current mouse position.
    pub fn drag_separator(&mut self, node_id: u32, mouse_x: f32, mouse_y: f32) {
        if drag_split_ratio(&mut self.root, node_id, mouse_x, mouse_y) {
            let r = self.root.rect();
            self.root.layout(r);
        }
    }
}

/// Returns true if `node`'s subtree contains the leaf with `target` id.
fn contains_leaf(node: &PaneNode, target: usize) -> bool {
    match node {
        PaneNode::Leaf { terminal_id, .. } => *terminal_id == target,
        PaneNode::Split { left, right, .. } => {
            contains_leaf(left, target) || contains_leaf(right, target)
        }
    }
}

/// Walk the tree and adjust the ratio of the nearest ancestor Split of `target`
/// whose direction matches `target_dir`. Returns true if found and adjusted.
fn adjust_parent_split(
    node: &mut PaneNode,
    target: usize,
    target_dir: SplitDir,
    delta: f32,
) -> bool {
    match node {
        PaneNode::Leaf { .. } => false,
        PaneNode::Split {
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
            // Prefer a deeper (closer) match first.
            let child_found = if in_left {
                adjust_parent_split(left, target, target_dir, delta)
            } else {
                adjust_parent_split(right, target, target_dir, delta)
            };
            if child_found {
                return true;
            }
            // No closer match — try this node.
            if *dir == target_dir {
                *ratio = (*ratio + delta).clamp(0.1, 0.9);
                return true;
            }
            false
        }
    }
}

/// Walk the tree, find the Split with `target_id`, and recompute its ratio so
/// the divider tracks the mouse position. Uses stable node_id — immune to layout changes.
fn drag_split_ratio(node: &mut PaneNode, target_id: u32, mouse_x: f32, mouse_y: f32) -> bool {
    match node {
        PaneNode::Leaf { .. } => false,
        PaneNode::Split {
            node_id,
            dir,
            ratio,
            left,
            right,
            rect,
        } => {
            if *node_id == target_id {
                let new_ratio = match dir {
                    SplitDir::Horizontal => (mouse_x - rect.x) / rect.w,
                    SplitDir::Vertical => (mouse_y - rect.y) / rect.h,
                };
                *ratio = new_ratio.clamp(0.1, 0.9);
                return true;
            }
            drag_split_ratio(left, target_id, mouse_x, mouse_y)
                || drag_split_ratio(right, target_id, mouse_x, mouse_y)
        }
    }
}
