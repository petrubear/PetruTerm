# M2 — Core Chrome Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Bring `gpui-petruterm` to daily-use parity with `petruterm`'s tabs, pane splits/resize/zoom, and status bar — the surfaces the parent spec's M2 line calls out as needed "before the branch is dogfoodable at all."

**Architecture:** Port `TabManager`/`Tab`/`tab_display_label` (`src/ui/tabs.rs`) and `StatusBar`/`StatusBarSegment`/`StatusBar::build` (`src/ui/status_bar.rs`) verbatim — both are pure data/functions with zero engine coupling. Port `PaneNode`/`PaneManager`'s tree-mutation *algorithms* (split/close/focus_dir/adjust_ratio/drag_split_ratio) as-is, but replace their manual `Rect` computation with gpui's native taffy flexbox (`flex_row()`/`flex_col()` + `flex_basis(relative(ratio))`) — a new `PaneTree` type without the `rect` field, with a small per-frame "last painted bounds" cache (keyed by leaf terminal id and by separator node id) that `render()` populates as it walks the tree, replacing the old cached-`rect`-on-every-node design. Port the leader-key chorded/timed dispatch state machine (`leader_active`/`leader_deadline`/`leader_prefix`/`resize_mode`) into `GpuiShellRoot`, since gpui's own declarative keymap doesn't express a timeout+context shape (confirmed against gpui 0.2.2 source). Port the git-branch async fetch using the same `tokio::spawn` + static-bridge pattern this branch already established for config hot-reload.

**Tech Stack:** gpui 0.2.2 (taffy-backed flex layout), alacritty_terminal 0.25 (unchanged), Lua config DSL (unchanged — `config::keybind_view::leader_bindings_view` ported as-is).

**Spec:** `docs/superpowers/specs/2026-08-30-gpui-chrome-migration-design.md`, `## M2 — Core Chrome: Design` section.

## Global Constraints

- `scripts/ci-local.sh` is the real gate; stable-only dependencies, exact-pinned, no new deps needed.
- Don't break the existing `petruterm` (wgpu) binary.
- `gpui_shell` must never import winit.
- No GPU/rendering/layout/mouse-pixel-math test harness — dogfood only. Unit tests only for pure logic: `PaneTree`'s tree-mutation algorithms, `drag_split_ratio`'s ratio-from-position math, `tab_display_label`'s formatting (already tested in `src/ui/tabs.rs`, port the tests too), `StatusBar::build`'s segment assembly, the git-branch fetch's TTL/stuck-recovery decision logic (fake clock inputs).
- Module files stay under 400 lines; split when exceeded (matches this branch's established convention).
- Every dogfood step stops and asks the user before that task's commit.
- `Terminal::cols`/`rows` are `Cell<u16>` (Task 3 of M1b) — read via `.get()`. `Terminal::resize(&self, cols, rows, scrollback, cell_width, cell_height)` takes `&self`, already callable through `Rc<Terminal>`.
- `spawn_terminal(cols, rows, config) -> anyhow::Result<(Rc<Terminal>, Arc<WakeupGate>)>` (`gpui_shell/mod.rs`) is the existing terminal-spawn primitive; reuse it for new tabs/panes.
- The existing 33ms poll loop in `GpuiShellRoot::new` (`cx.spawn` + `cx.background_executor().timer(...)`) already drives cursor blink; leader-deadline expiry and git-branch-fetch polling both piggyback on it rather than adding new timers.

---

### Task 1: Pane tree — port algorithms onto a rect-cache instead of cached `Rect`

**Files:**
- Create: `src/gpui_shell/panes.rs`
- Test: `src/gpui_shell/panes.rs` (`#[cfg(test)] mod tests`)

**Interfaces:**
- Produces: `PaneTree` (enum: `Leaf { terminal_id: usize }` / `Split { node_id: u32, dir: SplitDir, ratio: f32, left: Box<PaneTree>, right: Box<PaneTree> }`), `SplitDir { Horizontal, Vertical }`, `FocusDir { Left, Right, Up, Down }` — all `pub`. `PaneForest` (one `PaneTree` + `focused_terminal: usize` per tab, mirrors `PaneManager`): `new(terminal_id) -> Self`, `split(&mut self, dir, new_id)`, `close_focused(&mut self) -> Option<usize>`, `close_specific(&mut self, terminal_id) -> bool`, `leaf_ids(&self) -> Vec<usize>`, `leaf_count(&self) -> usize`.
- Consumes (Task 3 wires this): a per-tab `RectCache` — see Step 4.

- [ ] **Step 1: Port the tree types and pure mutation algorithms**

Copy `src/ui/panes.rs`'s `SplitDir`, `FocusDir`, `next_node_id`/`NEXT_NODE_ID` verbatim. Define `PaneTree` as the `Rect`-free version of `PaneNode`:

```rust
// gpui chrome migration (M2): pane-split tree. Ported from src/ui/panes.rs's
// PaneNode/PaneManager -- the tree-mutation ALGORITHMS (split/close/
// focus_dir/adjust_ratio/drag_split_ratio) are ported as-is, since they're
// real domain logic, not rendering code. What does NOT port is the tree's
// own cached `Rect` field and PaneNode::layout's manual recursive rect
// subdivision: gpui's Div is backed by a real taffy flexbox engine, so
// render() walks this tree into nested flex divs (Split -> div().flex_row()/
// .flex_col() with flex_basis(relative(ratio)) on each child) and lets
// taffy compute rects instead. Anything that still needs a leaf's or a
// separator's pixel rect (focus_dir's center-point search, adjust_ratio,
// drag_split_ratio) reads it from RectCache (Step 4), populated by each
// frame's own paint pass, not from a field on the tree.

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
```

Port `split_node`/`remove_leaf` from `src/ui/panes.rs:279-327`, adapted to the `Rect`-free `PaneTree` (drop every `rect`/`old_rect` reference; `remove_leaf`'s `std::mem::replace(right.as_mut(), PaneNode::leaf(0, old_rect))` becomes `std::mem::replace(right.as_mut(), PaneTree::leaf(0))`, and drop the `new_node.layout(old_rect)` calls entirely -- there is no layout step here anymore, taffy does it at render time):

```rust
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
```

- [ ] **Step 2: `PaneForest` (the `Rect`-free `PaneManager`)**

```rust
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
```

- [ ] **Step 2b: Write and run the tree-mutation tests**

Port `src/ui/panes.rs`'s implicit behavior into explicit tests (the original file had none for these -- writing them now, TDD, since they're exactly the "business logic" the parent spec calls in-scope):

```rust
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
```

Run: `cargo test --lib gpui_shell::panes:: 2>&1 | tail -20` -- expect 5/5 passing.

- [ ] **Step 3: `focus_dir`/`adjust_ratio`/`drag_separator`, backed by `RectCache`**

These three need real pixel geometry (a leaf's or a separator's last-painted rect) that no longer lives on the tree. Define `RectCache` here too, since `PaneForest`'s methods consume it directly:

```rust
/// Last-painted pixel bounds for every leaf and every separator in one
/// tab's pane tree, refreshed every frame as render() walks the tree (Task
/// 3) -- the taffy-flex replacement for PaneNode's old cached `rect` field.
/// Keyed the same way mouse.rs's CLICK_STATE is (plain integer keys, no
/// lifetime issues from holding gpui types across frames).
#[derive(Default)]
pub struct RectCache {
    /// terminal_id -> last-painted bounds.
    pub leaves: std::collections::HashMap<usize, gpui::Bounds<gpui::Pixels>>,
    /// Split node_id -> last-painted bounds of ITS SEPARATOR (not the whole
    /// split's bounds -- just the thin divider div's own rect, since that's
    /// what drag_separator's mouse-position math needs).
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
    /// mouse position, using that separator's own last-painted bounds from
    /// `rects` (the RectCache-based replacement for src/ui/panes.rs's
    /// `rect` field on the Split node itself).
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
fn adjust_parent_split(node: &mut PaneTree, target: usize, target_dir: SplitDir, delta: f32) -> bool {
    match node {
        PaneTree::Leaf { .. } => false,
        PaneTree::Split { dir, ratio, left, right, .. } => {
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

/// Ported from src/ui/panes.rs's drag_split_ratio, with the separator's
/// rect passed in explicitly (x, y, w, h) instead of read from a `rect`
/// field on the node itself.
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
        PaneTree::Split { node_id, dir, ratio, left, right } => {
            if *node_id == target_id {
                let new_ratio = match dir {
                    SplitDir::Horizontal => (mouse_x - sep_x) / sep_w,
                    SplitDir::Vertical => (mouse_y - sep_y) / sep_h,
                };
                *ratio = new_ratio.clamp(0.1, 0.9);
                return true;
            }
            drag_split_ratio(left, target_id, mouse_x, mouse_y, sep_x, sep_y, sep_w, sep_h)
                || drag_split_ratio(right, target_id, mouse_x, mouse_y, sep_x, sep_y, sep_w, sep_h)
        }
    }
}
```

Note on the `RectCache`-vs-`rect`-field design: the *drag* case is slightly different from the original wgpu design. `src/ui/panes.rs`'s `drag_split_ratio` reads the Split node's own cached `rect` (the whole split's bounds, both children combined) and computes the ratio from `(mouse_x - rect.x) / rect.w`. The gpui version above instead reads the SEPARATOR DIV's own bounds -- deliberately, since with a real flex layout there's no single "combined rect" cached anywhere convenient, but the separator div's own position IS exactly the ratio boundary. If a review finds the arithmetic doesn't produce the same result (e.g. the separator's `w`/`h` in the resize axis is near-zero, being a thin 1-cell-wide divider, which would make `sep_w`/`sep_h` in the WRONG axis meaningless) -- this needs the PARENT split's bounds, not the separator's own thin one, in the axis perpendicular to the divider's own thinness. Concretely: for a `Horizontal` split (left|right, vertical divider), the ratio needs `sep_x` (the divider's X position, correct) and the SPLIT's overall width (not the 1-cell-wide divider's own width). Task 3, which actually builds the div tree, must pass the PARENT split container's bounds for the axis being resized, not the thin separator's own bounds in that axis -- read Task 3's own guidance on this before wiring `RectCache.separators` up; this task only needs the function shape (`drag_separator(node_id, mouse_x, mouse_y, &RectCache)`) to be right, the exact cache contents are Task 3's responsibility to populate correctly.

- [ ] **Step 4: Build, verify the full gate**

```bash
cargo build --bin gpui-petruterm 2>&1
bash scripts/ci-local.sh
RUSTFLAGS="-D warnings" cargo build --bin gpui-petruterm --bin petruterm 2>&1 | tail -60
cargo test 2>&1 | tail -20
```

No dogfood step for this task alone -- `panes.rs` isn't wired into `render()` yet (Task 3 does that); this task's only observable surface is its own unit tests.

- [ ] **Step 5: Commit**

```bash
git add src/gpui_shell/panes.rs
git commit -m "[gpui-migration] feat: Port pane-split tree algorithms (M2).

PaneTree/PaneForest port src/ui/panes.rs's PaneNode/PaneManager
tree-mutation algorithms (split/close/focus_dir/adjust_ratio/
drag_split_ratio) as-is -- real domain logic, kept unchanged. What
does NOT port is the tree's own cached Rect field and PaneNode::
layout's manual recursive subdivision: gpui's Div is backed by a
real taffy flexbox engine, so a later task walks this tree into
nested flex divs and lets taffy compute rects instead. Anything
that still needs pixel geometry (focus_dir, adjust_ratio via its
target lookup, drag_separator) reads it from a new RectCache
populated by each frame's own paint pass.

5 new tests cover the tree-mutation algorithms directly (not
covered by tests in the original wgpu file). Not yet wired into
render() -- no dogfood surface yet, that's the next task.

scripts/ci-local.sh clean."
```

---

### Task 2: Tabs — `TabManager` port + tab bar UI, `GpuiShellRoot` restructured to be tab-indexed

**Files:**
- Create: `src/gpui_shell/tabs.rs`
- Modify: `src/gpui_shell/mod.rs` (`GpuiShellRoot` restructured; `SplitDemo`/`on_split_demo` retired)

**Interfaces:**
- Consumes: `panes::{PaneForest, RectCache}` (Task 1), `spawn_terminal` (already exists).
- Produces: `tabs::{Tab, TabManager, tab_display_label, TAB_LABEL_MAX_CHARS}` (ported verbatim). `GpuiShellRoot` gains `tabs: TabManager`, `tab_panes: Vec<PaneForest>` (index-aligned with `TabManager`'s tab list, mirroring `Mux.panes: Vec<PaneManager>`), `rect_cache: RectCache`. `active_terminal: usize` is retired (replaced by `self.tab_panes[self.tabs.active_index()].focused_terminal`).

- [ ] **Step 1: Port `tabs.rs` verbatim**

Copy `src/ui/tabs.rs` in full (`tab_display_label`, `Tab`, `TabManager`, and its existing `#[cfg(test)] mod tab_label_tests`) into `src/gpui_shell/tabs.rs` unchanged except the module doc comment:

```rust
// gpui chrome migration (M2): tab list. Ported from src/ui/tabs.rs
// verbatim -- pure data + one pure string-formatting function, zero I/O,
// zero rendering coupling. See tab_display_label's own doc comment for why
// it's the single source of truth both the tab-bar Render impl (this
// module, added below) and any future hit-testing must use.
```

(then the rest of the file, byte-for-byte identical to `src/ui/tabs.rs:1-175`).

- [ ] **Step 2: Run the ported tests**

```bash
cargo test --lib gpui_shell::tabs:: 2>&1 | tail -20
```
Expected: 1/1 passing (`label_format_and_truncation`).

- [ ] **Step 3: Restructure `GpuiShellRoot` to be tab-indexed**

Read the CURRENT `src/gpui_shell/mod.rs` in full before editing -- this brief describes the shape of the change, not a byte-for-byte diff, since Task 1 (already landed) didn't touch this file and the exact surrounding code (poll loop, `on_key_down`, `spawn_config_watcher` wiring) must be read fresh.

Replace `terminals: Vec<Rc<Terminal>>` + `active_terminal: usize` with:

```rust
pub struct GpuiShellRoot {
    pub tabs: tabs::TabManager,
    /// Index-aligned with `tabs`'s tab list -- one PaneForest per tab,
    /// mirroring Mux.panes: Vec<PaneManager> in the wgpu app exactly.
    tab_panes: Vec<panes::PaneForest>,
    /// terminal_id -> live Terminal handle. A tab's PaneForest only stores
    /// usize ids (matching src/ui/panes.rs's own design); this map is
    /// where the actual Rc<Terminal> lives, looked up by id wherever a
    /// leaf's real terminal is needed (paint, key routing, resize).
    terminals: std::collections::HashMap<usize, Rc<Terminal>>,
    next_terminal_id: usize,
    focus_handle: FocusHandle,
    config: Config,
    wakeup_gates: std::collections::HashMap<usize, Arc<WakeupGate>>,
    cursor_blink_on: bool,
    cursor_last_blink: std::time::Instant,
    rect_cache: panes::RectCache,
}
```

`GpuiShellRoot::new` changes its initial-terminal setup to: `spawn_terminal(80, 24, &config)`, assign it `id = 0` (the first `next_terminal_id`), `tabs.new_tab("zsh")`, push `PaneForest::new(0)` onto `tab_panes`, insert into the `terminals`/`wakeup_gates` maps keyed by `0`.

Every place the OLD code read `self.terminals.get(self.active_terminal)` (the key-down handler, the poll loop's `wakeup_gates.iter()`) now needs: `let active_tid = self.tab_panes[self.tabs.active_index()].focused_terminal;` then `self.terminals.get(&active_tid)`. The poll loop's `wakeup_gates.iter().any(|g| g.take_pending())` becomes `self.wakeup_gates.values().any(|g| g.take_pending())` (checking every live terminal across every tab, not just the active one, matching the wgpu app's own all-terminals-get-PTY-output-processed-regardless-of-focus behavior).

`SplitDemo`/`on_split_demo`/the `ctrl-f %` binding in `src/bin/gpui_petruterm.rs` are retired entirely -- Task 3 replaces this proof-of-concept with the real `cmd_split` (bound to `Leader %`/`"` via Task 4's leader dispatch, not a standalone demo action). Delete `actions!(gpui_shell_spike, [SplitDemo])`, `on_split_demo`, and the `cx.bind_keys([KeyBinding::new("ctrl-f %", SplitDemo, None)])` line in `main()`.

`render()`'s children-building logic changes substantially in Task 3 (walking the active tab's `PaneForest` instead of a flat `Vec<Rc<Terminal>>`) -- for THIS task, leave `render()` rendering only `self.terminals[&self.tab_panes[self.tabs.active_index()].focused_terminal]` as a single `TerminalGridElement` filling the window (i.e. exactly today's single-terminal behavior, just sourced through the new tab/pane-indexed fields instead of the old flat ones) plus a placeholder tab-bar `div()` row above it showing each tab's `tab_display_label` text (no click handling yet -- that's also Task 3, once the real render-tree structure exists to attach it to). This keeps Task 2 in a working, dogfoodable state without needing Task 3's full flex-tree walk yet.

- [ ] **Step 4: Build, fix errors, verify the full gate**

```bash
cargo build --bin gpui-petruterm 2>&1
bash scripts/ci-local.sh
RUSTFLAGS="-D warnings" cargo build --bin gpui-petruterm --bin petruterm 2>&1 | tail -60
cargo test 2>&1 | tail -20
```

- [ ] **Step 5: Dogfood checkpoint — STOP and ask the user to confirm**

Ask the user to run `cargo run --bin gpui-petruterm` and confirm:
1. The terminal still works exactly as before (typing, colors, cursor, selection, scrollback all unaffected -- this task only touched *how* the single terminal is addressed internally, not its behavior).
2. A tab bar row is visible at the top showing one tab (even though there's no way to create a second one yet).

- [ ] **Step 6: Commit**

```bash
git add src/gpui_shell/tabs.rs src/gpui_shell/mod.rs src/bin/gpui_petruterm.rs
git commit -m "[gpui-migration] feat: Port TabManager, restructure GpuiShellRoot to be tab-indexed (M2).

tabs.rs ports src/ui/tabs.rs's TabManager/Tab/tab_display_label
verbatim -- pure data + one pure string-formatting function.

GpuiShellRoot's flat terminals: Vec<Rc<Terminal>> + active_terminal
index is replaced with a tabs: TabManager + tab_panes:
Vec<PaneForest> (Task 1) structure, index-aligned exactly like the
wgpu app's TabManager + Mux.panes: Vec<PaneManager>. The M0-era
SplitDemo proof-of-concept action is retired -- its job (proving a
second live terminal works) is superseded by the real pane-split
machinery this milestone adds.

Render still shows a single terminal (the active tab's focused
pane) plus a placeholder tab bar row -- full multi-pane rendering
and tab-bar interactivity are the next task.

scripts/ci-local.sh clean. Dogfooded: single-terminal behavior
unaffected, tab bar renders."
```

---

### Task 3: Multi-pane rendering via taffy flex, separator drag, zoom, click-to-focus/click-to-switch-tab

**Files:**
- Modify: `src/gpui_shell/mod.rs` (`render()` walks the pane tree; tab bar gains click handling)
- Modify: `src/gpui_shell/terminal_element.rs` (if `TerminalGridElement` needs a `pane_bounds_out` hook — see Step 2)
- Create: `src/gpui_shell/pane_view.rs` (the `PaneTree` → nested flex-`div()` walk, separator rendering/drag)

**Interfaces:**
- Consumes: `panes::{PaneTree, PaneForest, RectCache, SplitDir}` (Task 1), `tabs::TabManager` (Task 2).
- Produces: `pane_view::render_pane_tree(tree: &PaneTree, terminals: &HashMap<usize, Rc<Terminal>>, focused: usize, on_focus_leaf: impl Fn(usize) -> OnFocusCallback, rects_out: &mut RectCache, ...) -> impl IntoElement` (exact signature is this task's own design work — the shape above is a starting point, not a contract to copy verbatim; read `mouse.rs`'s existing `register_mouse_handlers`/`OnFocusCallback` pattern before finalizing it, since separator drag reuses that same `window.on_mouse_event`-based approach).

This task is the plan's most design-heavy piece — like M1b's Task 6 (scrollbar thumb drag), it is described here, not handed over as literal code, because the exact shape depends on gpui APIs this branch hasn't exercised yet (nested flex construction) and on reading Task 1/2's real landed code rather than a snapshot. Read `src/gpui_shell/mouse.rs` and `src/gpui_shell/terminal_element.rs` in full before starting.

- [ ] **Step 1: Walk `PaneTree` into nested flex `div()`s**

For each `PaneTree::Split { dir, ratio, left, right, .. }`: a `div()` with `.flex()` and `.flex_row()` (Horizontal) or `.flex_col()` (Vertical), containing three children in order: `left`'s own rendered element wrapped in `div().flex_basis(relative(*ratio)).flex_shrink(0.)`, a separator `div()` (Step 3), `right`'s own rendered element wrapped in `div().flex_basis(relative(1.0 - *ratio)).flex_shrink(0.)`. Verify `relative(f32)` and `flex_basis`/`flex_shrink`'s exact import paths and signatures against gpui 0.2.2's real source (`~/.cargo/git/checkouts/zed-*/*/crates/gpui/src/styled.rs` and wherever `relative`/`Length` are defined) before using them — do not assume the names above are exactly right, they're this brief's best-effort based on earlier research, not a verified API surface.

For each `PaneTree::Leaf { terminal_id }`: a `TerminalGridElement` (existing) for that terminal, wrapped in `div().flex_1()` so it fills whatever space its parent flex container gives it, with an `on_focus` callback that sets `self.tab_panes[active_tab].focused_terminal = terminal_id` (mirroring `PaneManager::focus_at`'s job, but now driven by gpui's own click dispatch on the wrapping div rather than a manual hit-test) instead of the old `active_terminal = idx` — same underlying mechanism M1b's `on_focus` callback already established, redirected to write into the pane tree's `focused_terminal` field instead of a flat index.

- [ ] **Step 2: Populate `RectCache` as the tree is walked**

Each leaf's wrapping `div()` and each separator `div()` need their OWN painted bounds recorded into `rect_cache` for `focus_dir`/`adjust_ratio`/`drag_separator` (Task 1) to consume next frame. gpui elements don't expose "my own bounds" to an ordinary `Render`-tree closure the way a custom `Element` impl's `paint()` does (`TerminalGridElement` gets `bounds: Bounds<Pixels>` as a `paint()` argument; a plain `div()` built via the `IntoElement` DSL doesn't hand that back to the closure that constructed it). Investigate `.child()`/`.children()` combined with a custom small wrapper `Element` (similar in spirit to `TerminalGridElement` but trivial — just records its own `bounds` into a shared `Rc<RefCell<RectCache>>` in `paint()` and renders its single child) if plain `div()` doesn't expose this some other way (e.g. an `on_children_prepainted`-style hook — check gpui 0.2.2's `Div`/`Interactivity` source for anything along these lines before building a custom wrapper element from scratch). This is the one piece of this task most likely to need real exploration against gpui's actual source rather than an assumption from this brief.

For the separator specifically: per Task 1's Step 3 note, the ratio-drag math needs the SPLIT's overall bounds in the resize axis, not the thin separator's own bounds in that axis — so `RectCache.separators[node_id]` should be populated with the *parent split container's* bounds (available from the `div().flex_row()/.flex_col()` wrapping both children, which is exactly the element whose `paint()` this task is already instrumenting for children), not the 1-cell separator `div()`'s own tiny bounds. Confirm this produces correct drag behavior in Step 6's dogfood check specifically (drag a separator between two UNEQUAL-size panes and confirm the ratio tracks the mouse smoothly across the whole range, not just near the separator's own thin strip).

- [ ] **Step 3: Separator `div()` — visual + drag**

A 1px-wide (vertical split) or 1px-tall (horizontal split) `div()` with a subtle background color (reuse `colors.ui_border` or similar from `ColorScheme` — check `src/config/schema.rs`'s `ColorScheme` fields for the right one), `.cursor_col_resize()` (Horizontal split) or `.cursor_row_resize()` (Vertical split) for the hover affordance. Mouse-down/move/up on this div drives `PaneForest::drag_separator` — reuse `mouse.rs`'s established `window.on_mouse_event` pattern (this needs `Window`, available in `paint()` context; if the separator is a plain `div()` rather than a custom `Element`, check whether `.on_mouse_down()`/`.on_mouse_move()` div-builder methods exist as an alternative to the raw `window.on_mouse_event` this branch has used everywhere so far — either is acceptable, prefer whichever is more idiomatic once you've read gpui's `Div`/`Interactivity` source).

- [ ] **Step 4: Zoom — render-time filter, not tree mutation**

`GpuiShellRoot` gains `zoomed_pane: Option<usize>`. In `render()`, when `Some(id)` and `id` is a leaf of the active tab's tree: render ONLY that terminal's `TerminalGridElement` (full window, no flex-tree walk at all) instead of calling `pane_view::render_pane_tree`. This exactly mirrors `src/app/frame.rs:696-715`'s "swap in one full-viewport PaneInfo instead of the real list" design — zoom is never written into `PaneTree`/`PaneForest` itself.

- [ ] **Step 5: Tab bar click handling**

Each tab's `div()` (Task 2's placeholder) gains `.on_mouse_down(MouseButton::Left, ...)` calling `self.tabs.switch_to_index(idx)` + `cx.notify()`. Active tab gets a background fill (`colors.ui_surface_active` or equivalent) + bottom border accent (`tabs.active_accent(colors.ui_accent)`); inactive tabs get dimmed text (`colors.ui_muted`) — matching the wgpu app's actual flat-rect-plus-underline visual (verified in the design doc, not the stale "pill" memory).

- [ ] **Step 6: Build, fix errors, verify the full gate**

```bash
cargo build --bin gpui-petruterm 2>&1
bash scripts/ci-local.sh
RUSTFLAGS="-D warnings" cargo build --bin gpui-petruterm --bin petruterm 2>&1 | tail -60
cargo test 2>&1 | tail -20
```

- [ ] **Step 7: Dogfood checkpoint — STOP and ask the user to confirm**

No leader keybinds exist yet (Task 4) to trigger split/close/zoom from the keyboard — this task's own dogfood needs a temporary way to exercise it. Add a throwaway debug keybind (same shape as the retired `SplitDemo`, but calling the REAL `cmd_split`/pane-tree code this task built) if there's no other way to trigger a split yet, OR coordinate with Task 4 landing first if that's more practical — controller's call at execution time, record the reasoning in the ledger either way. Ask the user to confirm:
1. Splitting produces two independently-live, correctly-sized terminals side by side (or stacked).
2. Dragging the separator resizes both panes smoothly, tracking the mouse across the full range.
3. Clicking into a pane focuses it (cursor becomes solid Block there, hollow in the other).
4. Zooming a pane fills the window; unzooming restores the split view.
5. Clicking a tab switches to it.

- [ ] **Step 8: Commit**

(Commit message left to the controller/implementer at execution time, once the real diff shape is known — this task's design uncertainty is too high to pre-write an accurate summary now.)

---

### Task 4: Leader-key chorded dispatch

**Files:**
- Create: `src/gpui_shell/leader.rs`
- Modify: `src/gpui_shell/mod.rs` (`GpuiShellRoot` gains leader state; `on_key_down` gains leader activation/dispatch; poll loop gains deadline-expiry check)

**Interfaces:**
- Consumes: `config::keybind_view::leader_bindings_view(config) -> LeaderBindingsView { leader_key: String, bindings: Vec<KeyBinding> }` (already exists, engine-agnostic, `src/config/keybind_view.rs:9-19`; each `KeyBinding` has `.key: String`/`.action: String`). `panes::{PaneForest, FocusDir, SplitDir}` (Task 1), `tabs::TabManager` (Task 2), `pane_view`'s zoom/split/close entry points (Task 3).
- Produces: a `LeaderAction` enum covering exactly this milestone's scope (`NewTab`, `CloseTab`, `NextTab`, `PrevTab`, `RenameTab`, `SplitHorizontal`, `SplitVertical`, `ClosePane`, `ZoomPane`, `FocusPane(FocusDir)`) — a deliberately narrower type than the wgpu app's full `Action` enum (`src/ui/palette/actions.rs`, which also covers AI/workspace/sidebar actions this milestone doesn't touch); `GpuiShellRoot` gains `leader_active: bool`, `leader_deadline: Option<std::time::Instant>`, `resize_mode: bool`, `leader_map: HashMap<String, LeaderAction>` (built once from `leader_bindings_view` in `new()`, mapping `.key` strings to parsed `LeaderAction`s — a small `TryFrom<&str>` for `LeaderAction`, following the shape of `src/ui/palette/actions.rs`'s existing `Action` string parsing at `:77-95` for exactly the ten variants above).

- [ ] **Step 1: Port the state machine's activation + expiry**

Read `src/app/input/mod.rs:196-448` in full (already excerpted in the design research, but read the live file — line numbers may have drifted) before starting; this step ports its SHAPE, not a literal diff (this is genuinely different code: a different key-event type, no `render_ctx`/`wakeup_proxy`/`Action` variants outside this milestone's ten).

Leader activation, in `on_key_down` (before the existing Cmd+V/translate_key logic):

```rust
// Leader-deadline expiry piggybacks on the existing 33ms poll loop
// (same pattern M1b's cursor blink used) -- see the poll-loop change
// in Step 3, not here; on_key_down only ever SETS leader_active/
// leader_deadline, never expires them (a key press always means the
// deadline hasn't fired yet, by definition, since the poll loop
// would have cleared leader_active first if it had).
if !self.leader_active {
    if event.keystroke.modifiers.control
        && !event.keystroke.modifiers.shift
        && !event.keystroke.modifiers.platform
        && event.keystroke.key == self.config.leader.key
    {
        self.leader_active = true;
        self.leader_deadline = Some(
            std::time::Instant::now()
                + std::time::Duration::from_millis(1000), // match config.leader.timeout_ms if that field exists -- check src/config/schema.rs's LeaderConfig
        );
        cx.notify(); // status bar's leader indicator needs to see this
        return;
    }
}
```

Verify `config.leader`'s exact field names (`key`, and whatever the timeout field is called — `src/app/input/mod.rs` referenced `self.leader_timeout_ms` as a field on the wgpu app's own `Input` struct, not necessarily 1:1 with `LeaderConfig`'s own field name; check `src/config/schema.rs`) before using them.

- [ ] **Step 2: Single-key dispatch for this milestone's ten actions**

Still in `on_key_down`, when `self.leader_active` is true (the modifier-key-passthrough guard from `src/app/input/mod.rs:265-277` — Shift/Alt/Control/Super presses alone must not consume the leader — ports as-is):

```rust
if self.leader_active {
    if matches!(event.keystroke.key.as_str(), "shift" | "alt" | "control" | "platform" | "function") {
        return; // bare modifier key -- don't consume the leader (verify these are gpui's actual key-name strings for modifier-only presses, not winit's NamedKey variants -- different event model)
    }
    self.leader_active = false;
    self.leader_deadline = None;

    // Leader+Option+Arrow -> resize (check event.keystroke.modifiers.alt).
    // ... (port src/app/input/mod.rs:282-310's shape, calling
    // self.tab_panes[active].adjust_ratio(dir, 0.05) + self.resize_mode = true)

    // Leader+1-9 -> switch tab by index (hardcoded, matches src/app/input/mod.rs:424-430).
    if let Ok(n) = event.keystroke.key.parse::<usize>() {
        if (1..=9).contains(&n) {
            self.tabs.switch_to_index(n - 1);
            cx.notify();
            return;
        }
    }

    // Data-driven dispatch for c/&/n/b/,/%/"/x/z/h/j/k/l via self.leader_map.
    if let Some(action) = self.leader_map.get(event.keystroke.key.as_str()) {
        self.dispatch_leader_action(action.clone(), window, cx);
    }
    return;
}
```

`dispatch_leader_action` matches each `LeaderAction` variant to the real call: `NewTab` -> spawn a terminal via `spawn_terminal`, `self.tabs.new_tab(...)`, push a new `PaneForest`; `CloseTab` -> mirror `Mux::cmd_close_tab` (`src/app/mux/mod.rs:792-807`) — remove from `TabManager`, drop every leaf terminal of that tab's `PaneForest` from the `terminals`/`wakeup_gates` maps, remove the `PaneForest`; `NextTab`/`PrevTab` -> `self.tabs.next_tab()`/`.prev_tab()`; `SplitHorizontal`/`SplitVertical` -> spawn + `PaneForest::split` (mirroring `cmd_split`'s "spawn before mutating the tree" safety property); `ClosePane` -> `PaneForest::close_focused` + drop the closed terminal; `ZoomPane` -> toggle `self.zoomed_pane` by identity, matching `cmd_toggle_zoom_pane`'s exact logic; `FocusPane(dir)` -> `PaneForest::focus_dir(dir, &self.rect_cache)`. `RenameTab` needs a rename-input UI flow this milestone doesn't otherwise build (the wgpu app's version is a modal text-input prompt, `src/app/ui/mod.rs:1623-1630`+) — scope it down to the simplest thing that satisfies "Leader , renames the tab": read a single line from... actually, given no command-palette/modal-input infra exists in gpui_shell yet (M4), the controller should rule on the simplest workable M2 scope for this one action specifically (e.g. cycle through a small set of preset names, or skip real rename input and leave the tab titled by its shell name until M4's modal-input infra exists) — record the ruling in the ledger.

- [ ] **Step 3: Deadline expiry in the poll loop**

In the existing 33ms poll loop (`GpuiShellRoot::new`'s `cx.spawn` block), alongside the blink-toggle check:

```rust
if this.leader_active {
    if let Some(deadline) = this.leader_deadline {
        if std::time::Instant::now() >= deadline {
            this.leader_active = false;
            this.leader_deadline = None;
            should_notify = true; // status bar's leader indicator needs to clear
        }
    }
}
```

- [ ] **Step 4: `Cmd+1-9`**

In `on_key_down`, alongside the existing Cmd+V check: `if event.keystroke.modifiers.platform { if let Ok(n) = event.keystroke.key.parse::<usize>() { if (1..=9).contains(&n) { self.tabs.switch_to_index(n - 1); cx.notify(); return; } } }`.

- [ ] **Step 5: Build, fix errors, verify the full gate**

```bash
cargo build --bin gpui-petruterm 2>&1
bash scripts/ci-local.sh
RUSTFLAGS="-D warnings" cargo build --bin gpui-petruterm --bin petruterm 2>&1 | tail -60
cargo test 2>&1 | tail -20
```

- [ ] **Step 6: Dogfood checkpoint — STOP and ask the user to confirm**

Ask the user to run `cargo run --bin gpui-petruterm` and check every keybind this task adds: `Leader c` (new tab), `Leader &` (close tab), `Leader n`/`b` (next/prev tab), `Leader %`/`"` (split horizontal/vertical), `Leader x` (close pane), `Leader z` (zoom), `Leader h/j/k/l` (focus direction), `Leader Option+arrows` (resize, and confirm it stays active for repeated arrow presses without re-pressing leader), `Cmd+1-9` (switch tab by index). Also confirm the leader timeout actually expires (press `Ctrl+F`, wait over a second, press `c` — should type a literal `c` into the terminal, not open a new tab).

- [ ] **Step 7: Commit**

(Commit message left to the controller/implementer at execution time.)

---

### Task 5: Status bar

**Files:**
- Create: `src/gpui_shell/status_bar.rs`
- Modify: `src/gpui_shell/mod.rs` (`render()` gains the status bar row; poll loop gains git-branch polling + exit-code file check)

**Interfaces:**
- Consumes: `Terminal::child_pid: u32` (already exists), `crate::term::process_cwd(pid) -> Option<PathBuf>` (already exists, engine-agnostic), `crate::llm::shell_context::ShellContext` (already exists, engine-agnostic mtime-gated JSON read), `config::schema::{StatusBarColors, StatusBarStyle}` (already exist).
- Produces: `status_bar::{StatusBar, StatusBarSegment, SegmentKind}` (ported verbatim from `src/ui/status_bar.rs`), a git-branch async bridge following the `PENDING_CONFIG_RELOAD`/`CONFIG_CHANGED` static-pair pattern already in `gpui_shell/mod.rs:82-128`.

- [ ] **Step 1: Port `StatusBar`/`StatusBarSegment`/`StatusBar::build` verbatim**

Copy `src/ui/status_bar.rs` in full into `src/gpui_shell/status_bar.rs` unchanged (the `click_kind`/`left_sep_width`/`right_sep_width` pixel-math methods can be dropped — Step 3 below uses real interactive `div()`s per segment instead, per the design doc's "same simplification the tab bar gets" note; `truncate_path`/`format_time` port unchanged).

- [ ] **Step 2: Git-branch async bridge**

```rust
// Mirrors gpui_shell/mod.rs's PENDING_CONFIG_RELOAD/CONFIG_CHANGED bridge
// (that file's own doc comment explains why: gpui 0.2.2 has no
// spawn_blocking-style bridge from BackgroundExecutor to drive a true
// cross-thread wake, so a background thread/tokio task writes into a
// static slot and the existing 33ms poll loop reads it).
static PENDING_GIT_BRANCH: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);
static GIT_BRANCH_READY: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Ported from src/app/ui/git.rs's fetch_git_branch as-is -- runs `git
/// branch --show-current`, then (if dirty_check) `git status --porcelain`
/// and appends a `*` suffix if dirty.
async fn fetch_git_branch(cwd: &std::path::Path, dirty_check: bool) -> String {
    // ... identical body to src/app/ui/git.rs:168-202
}

/// Call once per poll-loop tick with the currently-focused terminal's cwd.
/// Spawns a fresh tokio fetch when the cwd changed or the TTL (15s) has
/// expired and no fetch is already in flight; drains any completed result.
/// Ported policy from src/app/ui/git.rs's poll_git_branch (TTL,
/// cwd-changed check, 30s stuck-in-flight recovery) -- re-plumbed onto
/// this file's own static bridge instead of a channel + tokio_rt field on
/// a winit-coupled struct.
pub fn poll_git_branch(state: &mut GitBranchState, cwd: Option<&std::path::Path>) -> bool {
    // state: { cache: Option<String>, cwd: Option<PathBuf>, fetched_at: Option<Instant>,
    //          in_flight: bool, spawn_time: Option<Instant> } -- a small struct living on
    // GpuiShellRoot (or inside this module behind a thread_local, controller's call --
    // GpuiShellRoot is simpler and matches how cursor_blink_on/cursor_last_blink already
    // live directly on the struct rather than a thread_local).
}
```

The exact `GitBranchState` field placement (on `GpuiShellRoot` directly, vs. a separate struct field) and the tokio runtime handle to spawn onto (check whether `gpui_shell` already has one reachable — `spawn_config_watcher` uses `std::thread::spawn` for its blocking watch loop, not tokio; this is the first place gpui_shell needs an actual tokio task, so confirm whether the binary already initializes a tokio runtime anywhere reachable, e.g. via `#[tokio::main]` on `main()` or a lazily-constructed `tokio::runtime::Runtime` — read `src/bin/gpui_petruterm.rs` before deciding) is this task's own design call; record the reasoning in the ledger.

- [ ] **Step 3: Status bar `Render` — one `div()` row, one `div()` per segment**

Each `StatusBarSegment` becomes a `div()` with its `fg`/`bg` and text; git-branch and exit-code segments get `.on_mouse_down(...)` (git-branch: no-op for now, since the branch picker is command-palette-scoped, M4 — or open nothing yet; exit-code: same, the exit-info context menu is also out of scope here). This replaces `click_kind`'s manual pixel math entirely, matching the design doc.

- [ ] **Step 4: Exit-code + CWD wiring**

CWD: call `crate::term::process_cwd(pid)` for the active tab's focused terminal's `child_pid` on tab-switch/focus-change (mirroring `refresh_status_cache`'s call-site pattern, not every frame). Exit code: read the existing mtime-gated JSON file via `crate::llm::shell_context::ShellContext` in the poll loop, same cadence as the git-branch poll.

- [ ] **Step 5: Build, verify the full gate**

```bash
cargo build --bin gpui-petruterm 2>&1
bash scripts/ci-local.sh
RUSTFLAGS="-D warnings" cargo build --bin gpui-petruterm --bin petruterm 2>&1 | tail -60
cargo test 2>&1 | tail -20
```

- [ ] **Step 6: Dogfood checkpoint — STOP and ask the user to confirm**

Ask the user to run `cargo run --bin gpui-petruterm` and confirm: the status bar shows CWD (changes when `cd`-ing), git branch (in a git repo, updates within ~15s of a branch change), exit code (only after a nonzero-exit command, clears on success), and the clock (updates each minute). Also confirm the leader/resize indicator segment reflects `Leader` being held (from Task 4).

- [ ] **Step 7: Commit**

(Commit message left to the controller/implementer at execution time.)

---

### Task 6: Whole-branch review, keybind regression pass

Once Tasks 1-5 are individually reviewed and dogfooded: dispatch the final whole-branch review (most capable model) over the full M2 diff, base = the commit before Task 1 started, head = current. Before that, manually exercise every keybind this milestone touches against AGENTS.md's own table (`Leader c/&/n/b/,`, `Leader %/"`, `Leader x/z`, `Leader h/j/k/l`, `Leader Option+arrows`, `Cmd+1-9`) as a regression checklist, per the parent spec's own "Keybind regression checklist" testing requirement. Fix findings the same way M1b's fix loop worked (R≤3 resume implementer/controller-direct fix, R≥4 escalate model); re-review scoped fixes before commit.

## M2 Exit Criteria

Tabs (create/close/switch/rename/reorder-N/A), panes (split/close/zoom/focus-direction/resize-drag), and the
status bar (cwd/git/exit-code/time) all work and are dogfood-confirmed together — a user can open several
tabs, split panes within a tab, resize/zoom/refocus them, and read live status at the bottom, entirely via
the leader-key/Cmd bindings AGENTS.md documents. `scripts/ci-local.sh` passes clean after every task. New
files (`panes.rs`, `tabs.rs`, `pane_view.rs`, `leader.rs`, `status_bar.rs`) each own one clear
responsibility and stay under the 400-line convention (split further if a task's real diff exceeds it).
M2's completion unblocks M3 (sidebars + AI panel), not yet designed.
