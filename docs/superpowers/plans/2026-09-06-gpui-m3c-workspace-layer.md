# gpui M3c: Workspace Layer + Workspace Sidebar Drawer Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Give `gpui_shell::GpuiShellRoot` a `Workspace` layer (each workspace owning its own tabs+panes+zoom state) and an animated, VSCode-style workspace sidebar drawer that lists/creates/switches/closes/renames them — closing out M3c of the gpui chrome migration.

**Architecture:** A new `WorkspaceManager` (pure data, no gpui deps) replaces `GpuiShellRoot`'s flat `tabs`/`tab_panes`/`zoomed_pane` fields with `Vec<Workspace>` + an active index, mirroring the wgpu build's `Mux` workspace CRUD conceptually (not its archive-on-switch shape — see `workspace.rs`'s own doc comment). Every existing call site that read those three fields is rewired through `self.workspaces.active()`/`active_mut()`. A `sidebar/` module (state + `div()` tree, same "plain struct + free `render_*` function" shape as `status_bar/`/`panes/`) renders the list as a flex sibling of the pane tree, animated the same way `chat_panel`'s drawer already is.

**Tech Stack:** Rust, gpui 0.2.2 (existing `gpui_shell` conventions only — no new crates).

**Spec:** `docs/superpowers/specs/2026-09-06-gpui-m3-sidebars-design.md` (§3.1 text-input as workspace-rename's consumer, §3.4 animated drawers, §3.6 workspace layer, §4 file structure, §5 keybinds, §6 testing, §7 exit criteria, §8.1 risk).

## Global Constraints

- 400-line module limit (split further if a task's file grows past it).
- Tests for logic only — no painting/layout/hit-testing tests (those are dogfooded). GPU windows cannot be captured from the agent sandbox.
- `scripts/ci-local.sh` must stay green after every task.
- Commit format: `type: Message.` per `AGENTS.md` (types: `feat`/`fix`/`chore`/`refactor`).
- **M3c scope is "Workspace layer + Workspace sidebar drawer (Workspaces section only)"** — MCP/Skills/Steering sections and `InfoOverlay` are M3d, out of scope here. Workspace *persistence* (save/load snapshots — `Leader W s`/`Leader W L` in the wgpu build) is likewise out of scope: the M3 design's exit criteria (§7) name list/create/switch/rename/close, never save-to-disk. Do not add it.
- **Key/focus guards key on real focus (`is_focused(window)`), never on visibility/open state** — the hard-won M3a/M3b rule (see `render.rs`'s existing combined guard and its extensive doc comment). Every new focusable element this plan adds (the workspace-rename `TextInput`) must be guarded the same way.
- `terminals`/`wakeup_gates` stay flat on `GpuiShellRoot`, keyed by terminal id (ids are globally unique across every workspace; the poll loop wants one map to walk) — per §3.6. Do not move them onto `Workspace`.
- No new Lua config surface: the wgpu reference (`src/app/input/mod.rs`) hardcodes every workspace-key dispatch directly (not through `config.keys`/`petruterm.action`), same as its own `Leader z` (zoom). `gpui_shell`'s existing convention for a single-key action hardcoded on the wgpu side is to *seed* it into `leader::build_leader_map`'s table (see that function's existing `"z"` entry), not to add a literal `if key == "..."` branch — follow that convention for the new single-key actions (`w`, `s`); the two-key `Leader W <x>` and `Leader e e` chords need literal match arms in `input.rs`'s prefix-continuation block, exactly like the existing `'a'`-prefix's children. **Do not touch `config/default/keybinds.lua` or `src/config/lua.rs`'s `action` table in this plan.**

---

## Task 1: `WorkspaceManager` data model

**Files:**
- Create: `src/gpui_shell/workspace.rs`
- Modify: `src/gpui_shell/mod.rs:9-26` (add `mod workspace;` to the module list, alphabetically after `pub mod text_input;`)

**Interfaces:**
- Produces (used by every later task):
  - `pub struct Workspace { pub id: usize, pub name: String, pub tabs: TabManager, pub tab_panes: Vec<PaneForest>, pub zoomed_pane: Option<usize> }`
  - `pub struct WorkspaceManager` with: `new() -> Self`, `new_workspace(&mut self, name: impl Into<String>) -> usize`, `close_workspace(&mut self, id: usize) -> Option<Workspace>`, `switch_to_index(&mut self, idx: usize) -> bool`, `next_workspace(&mut self)`, `prev_workspace(&mut self)`, `rename_workspace(&mut self, id: usize, name: impl Into<String>) -> bool`, `active(&self) -> &Workspace`, `active_mut(&mut self) -> &mut Workspace`, `active_id(&self) -> usize`, `active_index(&self) -> usize`, `workspaces(&self) -> &[Workspace]`, `workspaces_mut(&mut self) -> &mut [Workspace]`, `workspace_mut(&mut self, idx: usize) -> Option<&mut Workspace>`, `len(&self) -> usize`.

- [ ] **Step 1: Write `src/gpui_shell/workspace.rs`**

```rust
// gpui chrome migration (M3c Task 1): the workspace data model --
// `Workspace` (one named group of tabs+panes) and `WorkspaceManager` (the
// ordered list of them, plus which one is active).
//
// Mirrors the wgpu build's `Mux` workspace CRUD (`src/app/mux/workspace.rs`)
// conceptually -- same operations (new/close/switch/rename/next/prev) -- but
// NOT its "active fields direct on Mux + Vec<WorkspaceData> archive" shape.
// `Mux` keeps the active workspace's `tabs`/`panes` as bare fields on itself
// and `mem::take`s them into an archive `Vec` on switch, because it grew
// workspaces onto a struct that started single-workspace and had to stay
// source-compatible with every existing `mux.tabs`/`mux.panes` call site.
// `gpui_shell` has no such legacy: `Workspace` just owns its `TabManager` +
// `Vec<PaneForest>` + zoom state directly, `WorkspaceManager` holds
// `Vec<Workspace>` + an active index, and switching is nothing more than
// pointing the index elsewhere -- no archive, no `mem::take`.
//
// `active` is tracked by INDEX into `workspaces`, not by id -- the same
// choice `TabManager` already made (`tabs.rs`), and for the same reason:
// display order IS index order (the sidebar lists `workspaces()` top to
// bottom), so switching by click needs `switch_to_index` anyway. Every
// mutation below shifts `active` using the exact fixed logic
// `TabManager::close_tab`'s own doc comment explains -- M2 already paid for
// finding that bug once for tabs; this module's tests are the same
// regression coverage one level up, per the M3 design's §3.6 explicit
// requirement.

use super::panes::PaneForest;
use super::tabs::TabManager;

/// One named group of tabs+panes+zoom-state.
pub struct Workspace {
    pub id: usize,
    pub name: String,
    pub tabs: TabManager,
    pub tab_panes: Vec<PaneForest>,
    /// Render-time zoom filter (see `GpuiShellRoot`'s own field of the same
    /// name, before Task 2): workspace-scoped, since a zoomed terminal id
    /// only makes sense against ITS workspace's `tab_panes`.
    pub zoomed_pane: Option<usize>,
}

impl Workspace {
    fn new(id: usize, name: impl Into<String>) -> Self {
        Self {
            id,
            name: name.into(),
            tabs: TabManager::new(),
            tab_panes: Vec::new(),
            zoomed_pane: None,
        }
    }
}

/// Manages the ordered list of workspaces. Never empty once `new_workspace`
/// has been called at least once -- `GpuiShellRoot::new` (Task 2) calls it
/// immediately, same invariant `TabManager` relies on callers upholding for
/// tabs (`GpuiShellRoot::new` calls `tabs.new_tab` right after
/// `TabManager::new`).
pub struct WorkspaceManager {
    workspaces: Vec<Workspace>,
    active: usize,
    next_id: usize,
}

impl WorkspaceManager {
    pub fn new() -> Self {
        Self {
            workspaces: Vec::new(),
            active: 0,
            next_id: 0,
        }
    }

    /// Create a new, empty workspace and make it active. Returns its id.
    /// Caller is responsible for giving it an initial tab+pane, same
    /// division of labor `TabManager::new_tab`/`GpuiShellRoot::new` already
    /// have for tabs.
    pub fn new_workspace(&mut self, name: impl Into<String>) -> usize {
        let id = self.next_id;
        self.next_id += 1;
        self.workspaces.push(Workspace::new(id, name));
        self.active = self.workspaces.len() - 1;
        id
    }

    /// Close the workspace with the given id. Refuses (returns `None`) if
    /// it's the only one left -- a workspace-of-zero is not a state
    /// anything here can render. On success, returns the removed
    /// `Workspace` so the caller can walk its `tab_panes` and reap every
    /// terminal it owned (mirrors `Mux::cmd_close_workspace_id`'s own
    /// `closed_ids` collection, just handed back instead of pushed to a
    /// shared queue -- `gpui_shell` has no equivalent queue).
    ///
    /// Active-index shift on removal uses the exact fixed logic
    /// `TabManager::close_tab` documents: removing an element before
    /// `active` must shift `active` down with it, not just clamp it.
    pub fn close_workspace(&mut self, id: usize) -> Option<Workspace> {
        if self.workspaces.len() <= 1 {
            return None;
        }
        let pos = self.workspaces.iter().position(|w| w.id == id)?;
        let removed = self.workspaces.remove(pos);
        if pos < self.active {
            self.active -= 1;
        } else {
            self.active = self.active.min(self.workspaces.len() - 1);
        }
        Some(removed)
    }

    /// Switch to the workspace at the given display index. Returns whether
    /// it existed.
    pub fn switch_to_index(&mut self, idx: usize) -> bool {
        if idx < self.workspaces.len() {
            self.active = idx;
            true
        } else {
            false
        }
    }

    /// Switch to the next workspace (wraps around). No-op with one workspace.
    pub fn next_workspace(&mut self) {
        if !self.workspaces.is_empty() {
            self.active = (self.active + 1) % self.workspaces.len();
        }
    }

    /// Switch to the previous workspace (wraps around). No-op with one workspace.
    pub fn prev_workspace(&mut self) {
        if !self.workspaces.is_empty() {
            self.active = (self.active + self.workspaces.len() - 1) % self.workspaces.len();
        }
    }

    /// Rename the workspace with the given id, wherever it sits and
    /// regardless of which workspace is active -- same reasoning as
    /// `TabManager::rename_tab`: the rename editor is pinned to an id at
    /// the moment it opens, and a workspace switch mid-edit must not
    /// redirect the commit.
    pub fn rename_workspace(&mut self, id: usize, name: impl Into<String>) -> bool {
        let Some(w) = self.workspaces.iter_mut().find(|w| w.id == id) else {
            return false;
        };
        w.name = name.into();
        true
    }

    pub fn active(&self) -> &Workspace {
        &self.workspaces[self.active]
    }

    pub fn active_mut(&mut self) -> &mut Workspace {
        &mut self.workspaces[self.active]
    }

    pub fn active_id(&self) -> usize {
        self.workspaces[self.active].id
    }

    pub fn active_index(&self) -> usize {
        self.active
    }

    pub fn workspaces(&self) -> &[Workspace] {
        &self.workspaces
    }

    pub fn workspaces_mut(&mut self) -> &mut [Workspace] {
        &mut self.workspaces
    }

    pub fn workspace_mut(&mut self, idx: usize) -> Option<&mut Workspace> {
        self.workspaces.get_mut(idx)
    }

    pub fn len(&self) -> usize {
        self.workspaces.len()
    }
}

impl Default for WorkspaceManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_workspace_creates_and_activates_it() {
        let mut mgr = WorkspaceManager::new();
        let a = mgr.new_workspace("a");
        assert_eq!(mgr.active_id(), a);
        assert_eq!(mgr.active_index(), 0);
        assert_eq!(mgr.len(), 1);

        let b = mgr.new_workspace("b");
        assert_eq!(mgr.active_id(), b);
        assert_eq!(mgr.active_index(), 1);
        assert_eq!(mgr.len(), 2);
    }

    #[test]
    fn closing_a_background_workspace_before_active_keeps_the_same_workspace_active() {
        let mut mgr = WorkspaceManager::new();
        let a = mgr.new_workspace("a");
        let _b = mgr.new_workspace("b");
        let _c = mgr.new_workspace("c");
        let _d = mgr.new_workspace("d");
        // 4 workspaces: a b c d, active on d. Move active to b (index 1)
        // before closing a (index 0) -- the exact shape that silently
        // mis-clamped for TabManager before its own fix: `pos (0) < active
        // (1)`, nowhere near the last index, so a bare `.min(len-1)` clamp
        // is a no-op and would leave `active` pointing at whatever now
        // occupies index 1 (c, after removal) instead of following b to 0.
        mgr.switch_to_index(1);
        assert_eq!(mgr.active().name, "b");

        assert!(mgr.close_workspace(a).is_some());

        assert_eq!(mgr.active_index(), 0);
        assert_eq!(mgr.active().name, "b");
        assert_eq!(
            mgr.workspaces()
                .iter()
                .map(|w| w.name.as_str())
                .collect::<Vec<_>>(),
            vec!["b", "c", "d"]
        );
    }

    #[test]
    fn closing_a_background_workspace_after_active_leaves_active_index_unchanged() {
        let mut mgr = WorkspaceManager::new();
        let _a = mgr.new_workspace("a");
        let _b = mgr.new_workspace("b");
        let c = mgr.new_workspace("c");
        mgr.switch_to_index(0);
        assert_eq!(mgr.active().name, "a");

        assert!(mgr.close_workspace(c).is_some());

        assert_eq!(mgr.active_index(), 0);
        assert_eq!(mgr.active().name, "a");
    }

    #[test]
    fn closing_the_active_workspace_clamps_to_the_new_last_index() {
        let mut mgr = WorkspaceManager::new();
        let _a = mgr.new_workspace("a");
        let b = mgr.new_workspace("b");
        assert_eq!(mgr.active_index(), 1);

        let removed = mgr
            .close_workspace(b)
            .expect("two workspaces, closing one is fine");
        assert_eq!(removed.name, "b");
        assert_eq!(mgr.active_index(), 0);
        assert_eq!(mgr.active().name, "a");
    }

    #[test]
    fn closing_the_only_remaining_workspace_is_refused() {
        let mut mgr = WorkspaceManager::new();
        let a = mgr.new_workspace("a");
        assert!(mgr.close_workspace(a).is_none());
        assert_eq!(mgr.len(), 1);
    }

    #[test]
    fn closing_an_unknown_workspace_id_is_refused() {
        let mut mgr = WorkspaceManager::new();
        mgr.new_workspace("a");
        mgr.new_workspace("b");
        assert!(mgr.close_workspace(999).is_none());
        assert_eq!(mgr.len(), 2);
    }

    #[test]
    fn rename_workspace_by_id_renames_a_non_active_workspace_and_leaves_active_alone() {
        let mut mgr = WorkspaceManager::new();
        let a = mgr.new_workspace("a");
        let _b = mgr.new_workspace("b");
        assert_eq!(mgr.active().name, "b");

        assert!(mgr.rename_workspace(a, "notes"));

        assert_eq!(mgr.workspaces()[0].name, "notes");
        assert_eq!(mgr.active().name, "b");
    }

    #[test]
    fn rename_workspace_with_unknown_id_returns_false() {
        let mut mgr = WorkspaceManager::new();
        let _a = mgr.new_workspace("a");
        assert!(!mgr.rename_workspace(999, "notes"));
    }

    #[test]
    fn next_and_prev_workspace_wrap_around() {
        let mut mgr = WorkspaceManager::new();
        mgr.new_workspace("a");
        mgr.new_workspace("b");
        mgr.new_workspace("c");
        mgr.switch_to_index(2); // "c"

        mgr.next_workspace();
        assert_eq!(mgr.active_index(), 0); // wrapped to "a"

        mgr.prev_workspace();
        assert_eq!(mgr.active_index(), 2); // wrapped back to "c"
    }

    #[test]
    fn switch_to_index_out_of_bounds_is_refused_and_leaves_active_unchanged() {
        let mut mgr = WorkspaceManager::new();
        mgr.new_workspace("a");
        assert!(!mgr.switch_to_index(5));
        assert_eq!(mgr.active_index(), 0);
    }

    #[test]
    fn workspace_mut_returns_none_out_of_bounds() {
        let mut mgr = WorkspaceManager::new();
        mgr.new_workspace("a");
        assert!(mgr.workspace_mut(5).is_none());
        assert!(mgr.workspace_mut(0).is_some());
    }
}
```

- [ ] **Step 2: Register the module**

In `src/gpui_shell/mod.rs`, add `mod workspace;` to the `mod`/`pub mod` list (line ~26, right after `pub mod text_input;` — alphabetical order: `terminal_element` < `text_input` < `workspace`).

- [ ] **Step 3: Run the new tests**

Run: `cargo test --lib gpui_shell::workspace:: -- --nocapture`
Expected: all 12 tests pass (the module isn't used anywhere yet, so expect an "unused" warning on the whole file — harmless, Task 2 wires it in).

- [ ] **Step 4: Run full suite + ci-local**

Run: `cargo test --lib` (expect prior count + 12 new, all passing) then `./scripts/ci-local.sh` (must stay green).

- [ ] **Step 5: Commit**

```bash
git add src/gpui_shell/workspace.rs src/gpui_shell/mod.rs
git commit -m "feat: Add the WorkspaceManager data model (M3c Task 1)."
```

---

## Task 2: Wire `WorkspaceManager` into `GpuiShellRoot`

**Purpose:** Pure structural refactor. Replaces `GpuiShellRoot`'s `tabs`/`tab_panes`/`zoomed_pane` fields with one `workspaces: workspace::WorkspaceManager` field and rewires every call site. **No new keybinds, no new user-visible behavior** — with exactly one workspace (the only state reachable after this task, since Task 3 adds the only way to create a second one), every code path must behave byte-for-byte identically to before. The `on_terminal_exited`/`close_tab_at` rewrite below also makes those two paths correct for a *second* workspace even though one can't exist yet — doing that now avoids touching these exact lines twice across two tasks.

**Files:**
- Modify: `src/gpui_shell/mod.rs` (struct fields + constructor)
- Modify: `src/gpui_shell/actions.rs` (every method)
- Modify: `src/gpui_shell/render.rs` (render, closures, pane_ctx, status bar, tab bar)
- Modify: `src/gpui_shell/poll.rs` (one call site)
- Modify: `src/gpui_shell/input.rs` (four call sites)
- Modify: `src/gpui_shell/ai_block.rs` (one call site)

**Interfaces:**
- Consumes: `workspace::WorkspaceManager` and `workspace::Workspace` from Task 1, unchanged.
- Produces: `GpuiShellRoot::workspaces: workspace::WorkspaceManager` (private field), `pub(super) fn close_tab_at(&mut self, ws_idx: usize, tab_idx: usize, signal_shells: bool, cx: &mut Context<Self>) -> bool` (signature GAINS `ws_idx` — Task 3/4 pass `self.workspaces.active_index()` at every call site added later), `pub(super) fn close_workspace_at(&mut self, ws_idx: usize, signal_shells: bool, cx: &mut Context<Self>) -> bool` (new — Task 3's `CloseWorkspace` and Task 4's sidebar "x" button call this).

- [ ] **Step 1: `mod.rs` — struct fields**

In the `GpuiShellRoot` struct definition, delete these three fields and their doc comments:

```rust
    pub tabs: tabs::TabManager,
    /// Index-aligned with `tabs`'s tab list -- one PaneForest per tab,
    /// mirroring Mux.panes: Vec<PaneManager> in the wgpu app exactly.
    tab_panes: Vec<PaneForest>,
```

and (further down, after `rect_cache`):

```rust
    /// Render-time zoom filter: when `Some(terminal_id)`, that pane is drawn
    /// alone, filling the whole content area, and the tab's pane tree is not
    /// walked at all. Deliberately never written into `PaneTree`/
    /// `PaneForest` itself -- same design as the wgpu app's own zoom
    /// (`src/app/frame.rs`, which swaps in a single full-viewport `PaneInfo`
    /// instead of mutating the tree), so unzooming is just dropping this.
    zoomed_pane: Option<usize>,
```

Replace both with one field, inserted where `tabs` was (top of the struct, right after the doc comment block that currently precedes `pub tabs`):

```rust
    /// One workspace per named group of tabs+panes+zoom-state (M3c) --
    /// mirrors `Mux`'s workspace layer (`src/app/mux/mod.rs`) conceptually;
    /// see `workspace.rs`'s own doc comment for why the on-disk shape
    /// differs. `terminals`/`wakeup_gates` below stay flat, keyed by
    /// terminal id, since ids are already globally unique across every
    /// workspace and the poll loop wants one map to walk (design doc §3.6).
    workspaces: workspace::WorkspaceManager,
```

- [ ] **Step 2: `mod.rs` — constructor**

Replace:

```rust
        let mut tabs = tabs::TabManager::new();
        tabs.new_tab("zsh");
```

with:

```rust
        let mut workspaces = workspace::WorkspaceManager::new();
        workspaces.new_workspace("ws1");
        workspaces.active_mut().tabs.new_tab("zsh");
```

Replace the `Self { ... }` struct literal's `tabs,` and `tab_panes: vec![PaneForest::new(terminal_id)],` lines with:

```rust
            workspaces,
```

placed where `tabs,` was. Add, right after (still inside the constructor body, before the `Self { ... }` literal):

```rust
        workspaces.active_mut().tab_panes.push(PaneForest::new(terminal_id));
```

Delete the `zoomed_pane: None,` line from the struct literal (no replacement — it now lives on each `Workspace`, defaulted by `Workspace::new` in Task 1).

- [ ] **Step 3: `actions.rs` — `split_focused`**

Replace:

```rust
        let active = self.tabs.active_index();
        self.tab_panes[active].split(dir, terminal_id);
        // Splitting while zoomed would otherwise create a pane the user
        // can't see (the zoomed one still fills the window) and move focus
        // to it -- their next keystroke would go somewhere invisible.
        self.zoomed_pane = None;
```

with:

```rust
        let ws = self.workspaces.active_mut();
        let active = ws.tabs.active_index();
        ws.tab_panes[active].split(dir, terminal_id);
        // Splitting while zoomed would otherwise create a pane the user
        // can't see (the zoomed one still fills the window) and move focus
        // to it -- their next keystroke would go somewhere invisible.
        ws.zoomed_pane = None;
```

- [ ] **Step 4: `actions.rs` — `close_focused_pane`**

Replace:

```rust
        let active = self.tabs.active_index();
        let Some(closed) = self.tab_panes[active].close_focused() else {
            return;
        };
```

with:

```rust
        let active = self.workspaces.active().tabs.active_index();
        let Some(closed) = self.workspaces.active_mut().tab_panes[active].close_focused() else {
            return;
        };
```

- [ ] **Step 5: `actions.rs` — `on_terminal_exited`**

The poll loop drains `PtyEvent::Exit` from the flat `self.terminals` map (`poll.rs`), so the exited terminal can belong to ANY workspace, not just the active one — replace the whole function body:

```rust
    pub(super) fn on_terminal_exited(&mut self, terminal_id: usize, cx: &mut Context<Self>) {
        let mut found = None;
        for (ws_idx, ws) in self.workspaces.workspaces().iter().enumerate() {
            if let Some(tab_idx) = ws
                .tab_panes
                .iter()
                .position(|p| p.root.leaf_ids().contains(&terminal_id))
            {
                found = Some((ws_idx, tab_idx));
                break;
            }
        }
        let Some((ws_idx, tab_idx)) = found else {
            return;
        };
        let closed_here = self
            .workspaces
            .workspace_mut(ws_idx)
            .is_some_and(|w| w.tab_panes[tab_idx].close_specific(terminal_id));
        if closed_here {
            self.reap_pane(terminal_id, cx);
            return;
        }
        // close_specific only refuses when this was the tab's last pane --
        // close_tab_at's own leaf loop will then find exactly one leaf
        // (terminal_id itself), so signal_shells: false is always correct
        // here, never a guess.
        self.close_tab_at(ws_idx, tab_idx, false, cx);
    }
```

(Keep the function's existing doc comment above it unchanged.)

- [ ] **Step 6: `actions.rs` — `close_tab_at` + new `close_workspace_at`**

Replace the whole `close_tab_at` function body (keep its existing doc comment, but note in it that `ws_idx` is new — see the note below the code) with:

```rust
    pub(super) fn close_tab_at(
        &mut self,
        ws_idx: usize,
        tab_idx: usize,
        signal_shells: bool,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(tab_count) = self
            .workspaces
            .workspace_mut(ws_idx)
            .map(|w| w.tabs.tab_count())
        else {
            return false;
        };
        if tab_count <= 1 {
            // A workspace can't render with zero tabs (`render()` indexes
            // its active tab's pane tree unconditionally) -- so closing a
            // workspace's last tab closes the WORKSPACE, unless it's also
            // the app's last workspace, in which case there is nowhere left
            // to fall back to and the whole app quits (unchanged from this
            // function's pre-M3c behavior for the single-workspace case).
            if self.workspaces.len() <= 1 {
                cx.quit();
                return true;
            }
            return self.close_workspace_at(ws_idx, signal_shells, cx);
        }
        let Some(tab_id) = self
            .workspaces
            .workspace_mut(ws_idx)
            .and_then(|w| w.tabs.tabs().get(tab_idx).map(|t| t.id))
        else {
            return false;
        };
        self.workspaces
            .workspace_mut(ws_idx)
            .expect("checked above")
            .tabs
            .close_tab(tab_id);
        // A rename pinned to the tab being closed would otherwise survive as
        // a live `TextInput` entity with no cell left to render it into.
        if self
            .tab_rename
            .as_ref()
            .is_some_and(|(id, _)| *id == tab_id)
        {
            self.tab_rename = None;
        }
        let removed_forest = self.workspaces.workspace_mut(ws_idx).and_then(|w| {
            if tab_idx < w.tab_panes.len() {
                Some(w.tab_panes.remove(tab_idx))
            } else {
                None
            }
        });
        if let Some(forest) = removed_forest {
            for id in forest.root.leaf_ids() {
                if signal_shells {
                    if let Some(terminal) = self.terminals.get(&id) {
                        terminal.pty.request_exit();
                    }
                }
                self.reap_pane(id, cx);
            }
        }
        true
    }

    /// Close the workspace at `ws_idx` entirely (every tab, every pane).
    /// Refuses (returns `false`) if it's the app's only workspace or
    /// `ws_idx` doesn't name a real one -- `close_tab_at` above is the only
    /// caller until Task 3 adds `LeaderAction::CloseWorkspace` and Task 4
    /// adds the sidebar's "x" button, both of which call this directly.
    pub(super) fn close_workspace_at(
        &mut self,
        ws_idx: usize,
        signal_shells: bool,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(id) = self.workspaces.workspaces().get(ws_idx).map(|w| w.id) else {
            return false;
        };
        let Some(removed) = self.workspaces.close_workspace(id) else {
            return false;
        };
        // The active workspace may have changed as a side effect of the
        // removal (`WorkspaceManager::close_workspace` shifts `active`) --
        // any tab rename in flight was pinned to a tab id scoped to the
        // WORKSPACE being left, and each workspace's `TabManager` has its
        // own independent id counter starting at 0, so that id could
        // collide with an unrelated tab in whatever workspace is now
        // active. Unconditionally dropping it here (rather than trying to
        // match it against the removed workspace) is the only safe option.
        self.tab_rename = None;
        for forest in &removed.tab_panes {
            for leaf_id in forest.root.leaf_ids() {
                if signal_shells {
                    if let Some(terminal) = self.terminals.get(&leaf_id) {
                        terminal.pty.request_exit();
                    }
                }
                self.reap_pane(leaf_id, cx);
            }
        }
        true
    }
```

Note for the doc comment above `close_tab_at` (the existing one, currently starting "Close the tab at `tab_idx` (not necessarily the active one..."): append one sentence noting `ws_idx` names which workspace's tab list `tab_idx` indexes into, since callers now span workspaces via `on_terminal_exited`.

- [ ] **Step 7: `actions.rs` — `reap_pane`**

Replace the tail:

```rust
        if self.zoomed_pane == Some(terminal_id) {
            self.zoomed_pane = None;
        }
```

with:

```rust
        // terminal_id is globally unique, so at most one workspace can have
        // it zoomed -- checking all of them (cheap; there are at most a
        // handful) is simpler than threading a workspace index through
        // every caller of `reap_pane` just for this.
        for ws in self.workspaces.workspaces_mut() {
            if ws.zoomed_pane == Some(terminal_id) {
                ws.zoomed_pane = None;
            }
        }
```

- [ ] **Step 8: `actions.rs` — `toggle_zoom`**

Replace:

```rust
        let active = self.tabs.active_index();
        let focused = self.tab_panes[active].focused_terminal;
        self.zoomed_pane = match self.zoomed_pane {
            Some(id) if id == focused => None,
            _ if self.tab_panes[active].root.leaf_count() > 1 => Some(focused),
            _ => None,
        };
```

with:

```rust
        let ws = self.workspaces.active_mut();
        let active = ws.tabs.active_index();
        let focused = ws.tab_panes[active].focused_terminal;
        ws.zoomed_pane = match ws.zoomed_pane {
            Some(id) if id == focused => None,
            _ if ws.tab_panes[active].root.leaf_count() > 1 => Some(focused),
            _ => None,
        };
```

- [ ] **Step 9: `actions.rs` — `dispatch_leader_action`'s `NewTab`/`CloseTab`/`NextTab`/`PrevTab`/`FocusPane` arms**

Replace the `NewTab` arm's body from `self.tabs.new_tab("zsh");` through `self.zoomed_pane = None;` with:

```rust
                let ws = self.workspaces.active_mut();
                ws.tabs.new_tab("zsh");
                ws.tab_panes.push(PaneForest::new(terminal_id));
                // Same reasoning as `split_focused`: a zoomed pane from the
                // tab being left would otherwise linger, filling the window
                // even after the new tab (which has nothing zoomed) becomes
                // active.
                ws.zoomed_pane = None;
```

Replace the `CloseTab` arm:

```rust
                self.close_tab_at(self.tabs.active_index(), true, cx);
```

with:

```rust
                let ws_idx = self.workspaces.active_index();
                let tab_idx = self.workspaces.active().tabs.active_index();
                self.close_tab_at(ws_idx, tab_idx, true, cx);
```

Replace `LeaderAction::NextTab => self.tabs.next_tab(),` and `LeaderAction::PrevTab => self.tabs.prev_tab(),` with:

```rust
            LeaderAction::NextTab => self.workspaces.active_mut().tabs.next_tab(),
            LeaderAction::PrevTab => self.workspaces.active_mut().tabs.prev_tab(),
```

Replace the `FocusPane(dir)` arm's body:

```rust
                let active = self.tabs.active_index();
                // Clone the Rc first, same reason as `on_drag` in render():
                // `focus_dir` needs `&mut self.tab_panes[..]` and
                // `&self.rect_cache`'s contents at once, which a single
                // `self.` borrow of both fields can't express.
                let rects = self.rect_cache.clone();
                let rects = rects.borrow();
                self.tab_panes[active].focus_dir(dir, &rects);
```

with:

```rust
                let active = self.workspaces.active().tabs.active_index();
                // Clone the Rc first, same reason as `on_drag` in render():
                // `focus_dir` needs `&mut self.workspaces.active_mut().
                // tab_panes[..]` and `&self.rect_cache`'s contents at once,
                // which a single `self.` borrow of both fields can't
                // express.
                let rects = self.rect_cache.clone();
                let rects = rects.borrow();
                self.workspaces.active_mut().tab_panes[active].focus_dir(dir, &rects);
```

- [ ] **Step 10: `actions.rs` — `begin_tab_rename`**

Replace:

```rust
        let Some((tab_id, title)) = self.tabs.active_tab().map(|t| (t.id, t.title.clone())) else {
            return;
        };
```

with:

```rust
        let Some((tab_id, title)) = self
            .workspaces
            .active()
            .tabs
            .active_tab()
            .map(|t| (t.id, t.title.clone()))
        else {
            return;
        };
```

and, inside the `cx.subscribe` closure, replace `this.tabs.rename_tab(tab_id, name);` with `this.workspaces.active_mut().tabs.rename_tab(tab_id, name);` (this is a pre-existing, not-yet-reachable simplification: until Task 3 lands, only one workspace ever exists, so "active" here can only ever mean the workspace the rename started in).

- [ ] **Step 11: `render.rs`**

Replace `let active_index = self.tabs.active_index();` with `let active_index = self.workspaces.active().tabs.active_index();`.

Replace:

```rust
        if let Some(id) = self.zoomed_pane {
            if !self.tab_panes[active_index].root.leaf_ids().contains(&id) {
                self.zoomed_pane = None;
            }
        }
```

with:

```rust
        if let Some(id) = self.workspaces.active().zoomed_pane {
            if !self.workspaces.active().tab_panes[active_index]
                .root
                .leaf_ids()
                .contains(&id)
            {
                self.workspaces.active_mut().zoomed_pane = None;
            }
        }
```

In the `on_focus` closure, replace:

```rust
                    let active = root.tabs.active_index();
                    if root.tab_panes[active].focused_terminal != terminal_id {
                        root.tab_panes[active].focused_terminal = terminal_id;
                        cx.notify();
                    }
```

with:

```rust
                    let ws = root.workspaces.active_mut();
                    let active = ws.tabs.active_index();
                    if ws.tab_panes[active].focused_terminal != terminal_id {
                        ws.tab_panes[active].focused_terminal = terminal_id;
                        cx.notify();
                    }
```

In the `on_drag` closure, replace:

```rust
                        let rects = root.rect_cache.clone();
                        let rects = rects.borrow();
                        let active = root.tabs.active_index();
                        root.tab_panes[active].drag_separator(
                            node_id,
                            f32::from(position.x),
                            f32::from(position.y),
                            &rects,
                        );
```

with:

```rust
                        let rects = root.rect_cache.clone();
                        let rects = rects.borrow();
                        let active = root.workspaces.active().tabs.active_index();
                        root.workspaces.active_mut().tab_panes[active].drag_separator(
                            node_id,
                            f32::from(position.x),
                            f32::from(position.y),
                            &rects,
                        );
```

Replace `pane_ctx`'s `focused: self.tab_panes[active_index].focused_terminal,` with `focused: self.workspaces.active().tab_panes[active_index].focused_terminal,`.

Replace:

```rust
        let panes = match self.zoomed_pane {
            Some(terminal_id) => pane_view::render_leaf(terminal_id, &pane_ctx),
            None => pane_view::render_pane_tree(&self.tab_panes[active_index].root, &pane_ctx),
        };
```

with:

```rust
        let panes = match self.workspaces.active().zoomed_pane {
            Some(terminal_id) => pane_view::render_leaf(terminal_id, &pane_ctx),
            None => pane_view::render_pane_tree(
                &self.workspaces.active().tab_panes[active_index].root,
                &pane_ctx,
            ),
        };
```

In the `on_select_tab` listener, replace:

```rust
                let clicked_id = this.tabs.tabs().get(*idx).map(|t| t.id);
                if rename_id.is_some() && rename_id != clicked_id {
                    this.end_tab_rename(cx);
                }
                if this.tabs.switch_to_index(*idx) {
                    cx.notify();
                }
```

with:

```rust
                let clicked_id = this.workspaces.active().tabs.tabs().get(*idx).map(|t| t.id);
                if rename_id.is_some() && rename_id != clicked_id {
                    this.end_tab_rename(cx);
                }
                if this.workspaces.active_mut().tabs.switch_to_index(*idx) {
                    cx.notify();
                }
```

Replace `tabs::render_tab_bar(&self.tabs, &self.config.colors, on_select_tab, rename);` with `tabs::render_tab_bar(&self.workspaces.active().tabs, &self.config.colors, on_select_tab, rename);`.

Replace the `status_bar::StatusBar::build(...)` call's `self.zoomed_pane.is_some(),` argument with `self.workspaces.active().zoomed_pane.is_some(),`.

- [ ] **Step 12: `poll.rs`**

Replace:

```rust
                    let active = this.tabs.active_index();
                    let active_tid = this.tab_panes[active].focused_terminal;
```

with:

```rust
                    let active = this.workspaces.active().tabs.active_index();
                    let active_tid = this.workspaces.active().tab_panes[active].focused_terminal;
```

- [ ] **Step 13: `input.rs`**

There are four call sites. Replace each:

1. (resize-mode continuation, inside `if self.resize_mode {`):
```rust
                    let active = self.tabs.active_index();
                    self.tab_panes[active].adjust_ratio(dir, 0.05);
```
→
```rust
                    let active = self.workspaces.active().tabs.active_index();
                    self.workspaces.active_mut().tab_panes[active].adjust_ratio(dir, 0.05);
```

2. (`Leader + Option + Arrow` resize start): identical replacement as above (same two lines appear a second time, inside the `if self.leader_active {` block).

3. (`Leader + 1-9`): replace `self.tabs.switch_to_index(n - 1);` with `self.workspaces.active_mut().tabs.switch_to_index(n - 1);` (appears twice: once under leader dispatch, once under the `Cmd+1-9` branch — replace both).

4. (tail, PTY-write path): replace:
```rust
        let active_tid = self.tab_panes[self.tabs.active_index()].focused_terminal;
```
with:
```rust
        let active_ws = self.workspaces.active();
        let active_tid = active_ws.tab_panes[active_ws.tabs.active_index()].focused_terminal;
```

- [ ] **Step 14: `ai_block.rs`**

Replace:

```rust
            let active = self.tabs.active_index();
            let active_tid = self.tab_panes[active].focused_terminal;
```

with:

```rust
            let active_ws = self.workspaces.active();
            let active_tid = active_ws.tab_panes[active_ws.tabs.active_index()].focused_terminal;
```

- [ ] **Step 15: Build, test, dogfood-equivalence check**

Run: `cargo build 2>&1 | tail -80` — fix any borrow-checker error by re-reading the exact surrounding code (every replacement above was written to avoid holding two conflicting `&mut self` borrows across statements; if the compiler disagrees at some site, split the offending expression into its own `let` binding the same way `close_tab_at`'s rewrite above does, rather than restructuring the surrounding function).

Run: `cargo test --lib 2>&1 | tail -40` — expected count is Task 1's baseline + 12 (workspace.rs's tests, now actually exercised via the wired-in code) with zero failures and zero behavior change in any existing test (tabs/panes/leader tests must all still pass unmodified).

Run: `./scripts/ci-local.sh` — must stay green.

Dogfood manually (single window, single workspace — this task adds no way to create a second one): open the app, create tabs (`Leader c`), split panes (`Leader %`/`Leader "`), close a pane/tab, zoom (`Leader z`), rename a tab (`Leader ,`), switch tabs by number (`Cmd+1`), resize a pane (`Leader Option+Arrow`), let a shell `exit` on its own. Every one of these must behave exactly as it did before this task — there is no new capability to check yet.

- [ ] **Step 16: Commit**

```bash
git add src/gpui_shell/mod.rs src/gpui_shell/actions.rs src/gpui_shell/render.rs src/gpui_shell/poll.rs src/gpui_shell/input.rs src/gpui_shell/ai_block.rs
git commit -m "refactor: Wire WorkspaceManager into GpuiShellRoot (M3c Task 2)."
```

---

## Task 3: Workspace CRUD keybinds (new / close / switch)

**Purpose:** Make a second (and third, ...) workspace actually reachable: `Leader w` (new), `Leader W &` (close active), `Leader W j` / `Leader W k` (next/prev). Each workspace gets its own independent tab set and starts with one `zsh` tab, exactly like `GpuiShellRoot::new`'s bootstrap. Rename (`Leader W ,`) and the sidebar toggle are Task 4's job — see that task's header for why they're bundled with the sidebar instead of here.

**Files:**
- Modify: `src/gpui_shell/leader.rs` (new `LeaderAction` variants + seed)
- Modify: `src/gpui_shell/input.rs` (`W`-prefix entry + continuation)
- Modify: `src/gpui_shell/actions.rs` (`dispatch_leader_action` arms + three new wrapper methods)

**Interfaces:**
- Consumes: `WorkspaceManager` from Tasks 1-2 unchanged; `close_workspace_at` from Task 2.
- Produces: `LeaderAction::{NewWorkspace, CloseWorkspace, NextWorkspace, PrevWorkspace}`; `pub(super) fn switch_workspace_to_index(&mut self, idx: usize) -> bool`, `pub(super) fn next_workspace(&mut self)`, `pub(super) fn prev_workspace(&mut self)` on `GpuiShellRoot` — Task 4's sidebar click handler calls `switch_workspace_to_index` (never `self.workspaces.switch_to_index` directly).

- [ ] **Step 1: `leader.rs` — new variants**

Add four variants to the `LeaderAction` enum (after `ToggleAiPanel`):

```rust
    NewWorkspace,
    CloseWorkspace,
    NextWorkspace,
    PrevWorkspace,
```

Add matching arms to `TryFrom<&str>` (after the `"ToggleAiPanel"` arm):

```rust
            "NewWorkspace" => Ok(LeaderAction::NewWorkspace),
            "CloseWorkspace" => Ok(LeaderAction::CloseWorkspace),
            "NextWorkspace" => Ok(LeaderAction::NextWorkspace),
            "PrevWorkspace" => Ok(LeaderAction::PrevWorkspace),
```

Seed `"w"` into the map, right after the existing `"z"` seed in `build_leader_map` (same `.entry().or_insert()` shape, same reasoning — see that function's doc comment: the wgpu build hardcodes `Leader w` as a literal key check too, not through its own config table):

```rust
    map.entry("w".to_string())
        .or_insert(LeaderAction::NewWorkspace);
```

- [ ] **Step 2: `leader.rs` — update existing tests**

`unparseable_action_strings_are_skipped` currently asserts `map.len() == 1` (only `"z"` seeded); update to `map.len() == 2` (now `"z"` and `"w"` are both always-seeded defaults) — the comment `// just the seeded "z"` becomes `// just the seeded "z" and "w"`.

`build_leader_map_matches_default_keybinds_lua` currently ends with `assert_eq!(map.get("z"), Some(&LeaderAction::ZoomPane));`; add right after it:

```rust
        // Seeded even though it's absent from the input bindings, same as "z".
        assert_eq!(map.get("w"), Some(&LeaderAction::NewWorkspace));
```

Extend `parses_all_eleven_action_strings` (rename it `parses_all_action_strings` — it now covers more than eleven) with four more assertions, following the existing pattern:

```rust
        assert_eq!(
            LeaderAction::try_from("NewWorkspace"),
            Ok(LeaderAction::NewWorkspace)
        );
        assert_eq!(
            LeaderAction::try_from("CloseWorkspace"),
            Ok(LeaderAction::CloseWorkspace)
        );
        assert_eq!(
            LeaderAction::try_from("NextWorkspace"),
            Ok(LeaderAction::NextWorkspace)
        );
        assert_eq!(
            LeaderAction::try_from("PrevWorkspace"),
            Ok(LeaderAction::PrevWorkspace)
        );
```

(keep these before the final `assert_eq!(LeaderAction::try_from("NotAnAction"), Err(()));` line).

- [ ] **Step 3: Run leader.rs's tests**

Run: `cargo test --lib gpui_shell::leader:: -- --nocapture`
Expected: all pass, including the two updated assertions.

- [ ] **Step 4: `input.rs` — enter the `W` prefix**

Right after the existing block that enters the `'a'` prefix (`if event.keystroke.key == "a" { self.leader_active = true; self.leader_prefix = Some('a'); ... return; }`), add:

```rust
            // Leader + Shift+W → enter the workspace sub-prefix. gpui
            // reports a shift-held ASCII-lowercase-producing key with its
            // UNSHIFTED key string and `modifiers.shift = true` (verified
            // against gpui 0.2.2's `parse_keystroke`,
            // `platform/mac/events.rs`) -- so this is "w" + shift, not "W".
            // Checked here, before the plain `leader_map` lookup below
            // (which matches "w" too, for `Leader w` with no shift), so the
            // two never collide.
            if event.keystroke.key == "w" && event.keystroke.modifiers.shift {
                self.leader_active = true;
                self.leader_prefix = Some('W');
                self.leader_deadline = Some(
                    std::time::Instant::now()
                        + std::time::Duration::from_millis(self.config.leader.timeout_ms),
                );
                cx.notify();
                return;
            }
```

- [ ] **Step 5: `input.rs` — `W`-prefix continuation**

Inside the `if let Some(prefix) = self.leader_prefix.take() { ... }` block, after the existing `if prefix == 'a' && event.keystroke.key == "a" { ... }` arm and before its trailing `return;`, add:

```rust
                if prefix == 'W' {
                    let action = match event.keystroke.key.as_str() {
                        "&" => Some(LeaderAction::CloseWorkspace),
                        "j" => Some(LeaderAction::NextWorkspace),
                        "k" => Some(LeaderAction::PrevWorkspace),
                        _ => None,
                    };
                    if let Some(action) = action {
                        self.dispatch_leader_action(action, window, cx);
                    }
                }
```

(The existing comment above the `'a'` arm — "Every other `a`-prefix subkey ... is simply dropped" — still applies unchanged to `'W'`'s unrecognized subkeys via the same trailing `return;`.)

- [ ] **Step 6: `actions.rs` — new `dispatch_leader_action` arms + wrapper methods**

Add four arms to `dispatch_leader_action`'s `match action { ... }`, after the existing `LeaderAction::ToggleAiPanel => { ... }` arm:

```rust
            LeaderAction::NewWorkspace => {
                let name = format!("ws{}", self.workspaces.len() + 1);
                let (terminal, gate) = match spawn_terminal(80, 24, &self.config) {
                    Ok(pair) => pair,
                    Err(e) => {
                        log::error!(
                            "gpui-shell: failed to spawn terminal for new workspace: {e:#}"
                        );
                        return;
                    }
                };
                let terminal_id = self.next_terminal_id;
                self.next_terminal_id += 1;
                self.terminals.insert(terminal_id, terminal);
                self.wakeup_gates.insert(terminal_id, gate);
                self.workspaces.new_workspace(name);
                self.workspaces.active_mut().tabs.new_tab("zsh");
                self.workspaces
                    .active_mut()
                    .tab_panes
                    .push(PaneForest::new(terminal_id));
                self.tab_rename = None;
            }
            LeaderAction::CloseWorkspace => {
                let ws_idx = self.workspaces.active_index();
                self.close_workspace_at(ws_idx, true, cx);
            }
            LeaderAction::NextWorkspace => self.next_workspace(),
            LeaderAction::PrevWorkspace => self.prev_workspace(),
```

Add three new methods at the end of `impl GpuiShellRoot` in `actions.rs` (after `end_tab_rename`):

```rust
    /// Switch to the workspace at `idx`. The only path any workspace switch
    /// (keyboard here, a sidebar row click in Task 4) should go through --
    /// centralizes clearing `tab_rename`, which every switch must do: a tab
    /// id is only unique WITHIN its own workspace's `TabManager` (each has
    /// its own counter starting at 0), so a rename left open across a
    /// workspace switch could commit onto an unrelated tab that happens to
    /// share the same numeric id in the newly active workspace.
    pub(super) fn switch_workspace_to_index(&mut self, idx: usize) -> bool {
        let switched = self.workspaces.switch_to_index(idx);
        if switched {
            self.tab_rename = None;
        }
        switched
    }

    /// See `switch_workspace_to_index`'s doc comment for why `tab_rename`
    /// is cleared here too.
    pub(super) fn next_workspace(&mut self) {
        self.workspaces.next_workspace();
        self.tab_rename = None;
    }

    /// See `switch_workspace_to_index`'s doc comment.
    pub(super) fn prev_workspace(&mut self) {
        self.workspaces.prev_workspace();
        self.tab_rename = None;
    }
```

- [ ] **Step 7: Build, test, dogfood**

Run: `cargo build 2>&1 | tail -60`, then `cargo test --lib 2>&1 | tail -40` (all pass, no new tests expected beyond Step 2/3's leader.rs updates — this task's new behavior is keyboard/render-driven, not unit-testable per this project's "no painting/hit-testing tests" convention), then `./scripts/ci-local.sh`.

Dogfood: `Leader w` twice (now 3 workspaces: the initial one + 2 new, each showing just a fresh `zsh` tab — the ORIGINAL workspace's tabs/panes/terminal must NOT be visible or affected). `Leader W j` / `Leader W k` cycle through all three, each one's own tabs/panes reappearing exactly as left (make a visible change in each — e.g. run `echo 1`/`echo 2`/`echo 3` in each — and confirm switching back shows the right one, unscrolled). `Leader W &` closes the active workspace and falls back to a neighbor; closing down to one workspace, `Leader W &` again is a no-op (nothing closes, no crash). Create a workspace, split a pane in it, then `Leader W &`: confirm both terminals in that workspace's split are gone (check `ps`/`jobs` isn't needed — just confirm no zombie terminal remains visible after switching through the remaining workspaces).

- [ ] **Step 8: Commit**

```bash
git add src/gpui_shell/leader.rs src/gpui_shell/input.rs src/gpui_shell/actions.rs
git commit -m "feat: Add workspace create/close/switch keybinds (M3c Task 3)."
```

---

## Task 4: Workspace sidebar drawer (list, switch, create, close, rename)

**Purpose:** The VSCode-style animated drawer this milestone is named for. Lists every workspace with the active one highlighted, click-to-switch, a header "+" to create one, a per-row "x" to close one, and `Leader W ,` to rename the active one inline (auto-opening the drawer if it's closed, so the edit is never invisible). `Leader s` and `Leader e e` both toggle it, matching the wgpu build's own alias (`src/app/input/mod.rs`).

**Files:**
- Create: `src/gpui_shell/sidebar/mod.rs`
- Create: `src/gpui_shell/sidebar/render.rs`
- Modify: `src/gpui_shell/mod.rs` (module registration, `sidebar` + `workspace_rename` fields, constructor)
- Modify: `src/gpui_shell/leader.rs` (`ToggleWorkspaceSidebar`, `RenameWorkspace` variants + `"s"` seed)
- Modify: `src/gpui_shell/input.rs` (`e`-prefix entry/continuation, `W`-prefix `","` arm, top-of-function `workspace_rename` focus guard)
- Modify: `src/gpui_shell/actions.rs` (`ToggleWorkspaceSidebar`/`RenameWorkspace` arms, `begin_workspace_rename`/`end_workspace_rename`)
- Modify: `src/gpui_shell/render.rs` (focus-reclaim guard, sidebar as a flex sibling of `pane_area`)

**Interfaces:**
- Consumes: `WorkspaceManager` (Tasks 1-2), `switch_workspace_to_index` (Task 3), `text_input::TextInput`/`TextInputEvent` (M3a).
- Produces: `sidebar::WorkspaceSidebar { is_visible(&self) -> bool, toggle(&mut self), show(&mut self) }`; `sidebar::render::render_workspace_sidebar(...) -> Div`.

- [ ] **Step 1: Write `src/gpui_shell/sidebar/mod.rs`**

```rust
// gpui chrome migration (M3c Task 4): the workspace sidebar drawer's
// visibility state. Mirrors `chat_panel`'s own `visible: bool` +
// `toggle`/`is_visible` shape (`chat_panel/mod.rs`) -- this drawer has no
// streaming state or composer of its own (M3c scope is "Workspaces section
// only", per the M3 design's milestone table; MCP/Skills/Steering +
// `InfoOverlay` are M3d), so there is nothing else to hold here yet.

pub mod render;

#[derive(Default)]
pub struct WorkspaceSidebar {
    visible: bool,
}

impl WorkspaceSidebar {
    pub fn is_visible(&self) -> bool {
        self.visible
    }

    pub fn toggle(&mut self) {
        self.visible = !self.visible;
    }

    /// Force the drawer open -- `begin_workspace_rename` calls this so the
    /// rename editor (rendered inline in the sidebar row, same as a tab
    /// rename renders inline in the tab bar) is never focused while
    /// invisible.
    pub fn show(&mut self) {
        self.visible = true;
    }
}
```

- [ ] **Step 2: Write `src/gpui_shell/sidebar/render.rs`**

```rust
// gpui chrome migration (M3c Task 4): the workspace sidebar's `div()` tree
// -- one clickable row per workspace, a header "+" new-workspace
// affordance, and a per-row "x" close affordance. Same "flat clickable
// cell, closure built at the call site" shape as `tabs::render_tab_bar`
// (`tabs.rs`), laid out as a column instead of a row.

use std::rc::Rc;

use gpui::{div, prelude::*, px, App, Div, MouseButton, MouseDownEvent, Window};

use crate::config::schema::ColorScheme;

use super::super::font_state;
use super::super::pane_view::to_rgba;
use super::super::workspace::WorkspaceManager;

pub const SIDEBAR_WIDTH_PX: f32 = 220.0;

pub type WorkspaceSelectCallback = Rc<dyn Fn(&usize, &mut Window, &mut App)>;
pub type WorkspaceNewCallback = Rc<dyn Fn(&(), &mut Window, &mut App)>;
pub type WorkspaceCloseCallback = Rc<dyn Fn(&usize, &mut Window, &mut App)>;

/// `rename`: `(workspace_id, editor)` -- same "pinned to an id, taken at
/// most once, placed on the matching row" shape as `render_tab_bar`'s own
/// `rename` parameter (`tabs.rs`).
pub fn render_workspace_sidebar(
    workspaces: &WorkspaceManager,
    colors: &ColorScheme,
    on_select: WorkspaceSelectCallback,
    on_new: WorkspaceNewCallback,
    on_close: WorkspaceCloseCallback,
    rename: Option<(usize, gpui::AnyElement)>,
) -> Div {
    let active_index = workspaces.active_index();
    let mut rename = rename;
    let rows: Vec<_> = workspaces
        .workspaces()
        .iter()
        .enumerate()
        .map(|(idx, ws)| {
            let is_active = idx == active_index;
            let is_renaming = rename.as_ref().is_some_and(|(id, _)| *id == ws.id);
            let select = on_select.clone();
            let close = on_close.clone();
            let ws_id = ws.id;
            let label = format!("{}  ({} tabs)", ws.name, ws.tabs.tab_count());
            let row = div()
                .flex()
                .flex_row()
                .items_center()
                .justify_between()
                .px_2()
                .py_1()
                .cursor_pointer()
                .on_mouse_down(MouseButton::Left, move |_: &MouseDownEvent, window, cx| {
                    select(&idx, window, cx)
                })
                .when(is_renaming, |el| {
                    el.child(rename.take().expect("checked is_some").1)
                })
                .when(!is_renaming, |el| el.child(label))
                .child(
                    div()
                        .cursor_pointer()
                        .px_1()
                        .child("x")
                        .on_mouse_down(MouseButton::Left, move |_: &MouseDownEvent, window, cx| {
                            close(&ws_id, window, cx)
                        }),
                );
            if is_active {
                row.bg(to_rgba(colors.ui_surface_active))
                    .text_color(to_rgba(colors.foreground))
            } else {
                row.text_color(to_rgba(colors.ui_muted))
            }
        })
        .collect();

    div()
        .flex()
        .flex_col()
        .flex_shrink_0()
        .h_full()
        .w(px(SIDEBAR_WIDTH_PX))
        .bg(to_rgba(colors.ui_surface))
        .border_r_1()
        .border_color(to_rgba(colors.ui_border))
        .font_family(font_state::font_family())
        .text_size(px(font_state::font_size()))
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .justify_between()
                .px_2()
                .py_2()
                .border_b_1()
                .border_color(to_rgba(colors.ui_border))
                .child("Workspaces")
                .child(
                    div()
                        .cursor_pointer()
                        .px_1()
                        .child("+")
                        .on_mouse_down(MouseButton::Left, move |_: &MouseDownEvent, window, cx| {
                            on_new(&(), window, cx)
                        }),
                ),
        )
        .children(rows)
}
```

- [ ] **Step 3: `mod.rs` — register the module, add fields**

Add `mod sidebar;` to the module list, alphabetically after `pub mod status_bar;` and before `pub mod tabs;`.

Add two fields to `GpuiShellRoot`, near `tab_rename` (after it):

```rust
    /// The in-progress workspace rename, `Some` only while `Leader W ,` is
    /// being answered. Unlike `tab_rename`, a workspace id is globally
    /// unique (one shared counter on `WorkspaceManager`, not one per
    /// workspace) so no cross-workspace collision risk exists here -- see
    /// `switch_workspace_to_index`'s doc comment (`actions.rs`) for why
    /// `tab_rename` doesn't get the same guarantee.
    workspace_rename: Option<(usize, gpui::Entity<text_input::TextInput>)>,
    /// The workspace sidebar drawer -- one global drawer on the LEFT,
    /// mirroring `chat`'s drawer on the right (`render.rs`'s `middle_row`).
    sidebar: sidebar::WorkspaceSidebar,
```

In the constructor's `Self { ... }` literal, add `workspace_rename: None,` (next to `tab_rename: None,`) and `sidebar: sidebar::WorkspaceSidebar::default(),`.

- [ ] **Step 4: `leader.rs` — new variants + seed**

Add two more variants to `LeaderAction` (after `PrevWorkspace`):

```rust
    RenameWorkspace,
    ToggleWorkspaceSidebar,
```

Add matching `TryFrom<&str>` arms:

```rust
            "RenameWorkspace" => Ok(LeaderAction::RenameWorkspace),
            "ToggleWorkspaceSidebar" => Ok(LeaderAction::ToggleWorkspaceSidebar),
```

Seed `"s"`, right after the `"w"` seed added in Task 3:

```rust
    map.entry("s".to_string())
        .or_insert(LeaderAction::ToggleWorkspaceSidebar);
```

Update `unparseable_action_strings_are_skipped`'s expected `map.len()` from `2` to `3` (now `"z"`, `"w"`, `"s"`), and its comment. Add to `build_leader_map_matches_default_keybinds_lua`, after the `"w"` assertion added in Task 3:

```rust
        assert_eq!(map.get("s"), Some(&LeaderAction::ToggleWorkspaceSidebar));
```

Add to `parses_all_action_strings`:

```rust
        assert_eq!(
            LeaderAction::try_from("RenameWorkspace"),
            Ok(LeaderAction::RenameWorkspace)
        );
        assert_eq!(
            LeaderAction::try_from("ToggleWorkspaceSidebar"),
            Ok(LeaderAction::ToggleWorkspaceSidebar)
        );
```

- [ ] **Step 5: Run leader.rs's tests**

Run: `cargo test --lib gpui_shell::leader:: -- --nocapture` — all pass.

- [ ] **Step 6: `input.rs` — top-of-function `workspace_rename` guard**

Right after the existing `tab_rename` guard block (`if let Some((_, input)) = &self.tab_rename { if input.focus_handle(cx).is_focused(window) { return; } }`), add the identical shape for `workspace_rename`:

```rust
        // Same guard, same reasoning, for the workspace-rename editor.
        if let Some((_, input)) = &self.workspace_rename {
            if input.focus_handle(cx).is_focused(window) {
                return;
            }
        }
```

- [ ] **Step 7: `input.rs` — `e`-prefix entry + continuation, `W`-prefix `","` arm**

In the prefix-entry block added in Task 3 (right after the `if event.keystroke.key == "w" && event.keystroke.modifiers.shift { ... }` block), add:

```rust
            // Leader + e → enter the explorer/sidebar sub-prefix (only "e"
            // is wired to anything: `Leader e e` toggles the sidebar,
            // matching the wgpu build's own alias for `Leader s`).
            if event.keystroke.key == "e" {
                self.leader_active = true;
                self.leader_prefix = Some('e');
                self.leader_deadline = Some(
                    std::time::Instant::now()
                        + std::time::Duration::from_millis(self.config.leader.timeout_ms),
                );
                cx.notify();
                return;
            }
```

In the prefix-continuation block, add an `'e'` arm alongside the existing `'a'`/`'W'` ones:

```rust
                if prefix == 'e' && event.keystroke.key == "e" {
                    self.dispatch_leader_action(LeaderAction::ToggleWorkspaceSidebar, window, cx);
                }
```

Extend the `'W'`-prefix match added in Task 3 with the rename key:

```rust
                        "," => Some(LeaderAction::RenameWorkspace),
```

(placed alongside the existing `"&"`/`"j"`/`"k"` arms).

- [ ] **Step 8: `actions.rs` — new `dispatch_leader_action` arms**

Add two arms, after the `PrevWorkspace` arm from Task 3:

```rust
            LeaderAction::RenameWorkspace => self.begin_workspace_rename(window, cx),
            LeaderAction::ToggleWorkspaceSidebar => self.sidebar.toggle(),
```

- [ ] **Step 9: `actions.rs` — `begin_workspace_rename`/`end_workspace_rename`**

Add at the end of `impl GpuiShellRoot` (after `end_tab_rename`):

```rust
    /// Open an editable field over the active workspace's sidebar row,
    /// seeded with its current name and focused so the next keystroke goes
    /// to it. Forces the sidebar open first (`sidebar.show()`) so the
    /// editor -- rendered inline in that row, same as a tab rename renders
    /// inline in the tab bar -- is never focused while invisible.
    pub(super) fn begin_workspace_rename(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.sidebar.show();
        let ws_id = self.workspaces.active_id();
        let name = self.workspaces.active().name.clone();
        let colors = self.config.colors.clone();
        let input = cx.new(|cx| text_input::TextInput::new(cx, &colors, name, "workspace name"));

        cx.subscribe(&input, move |this, input, event, cx| {
            match event {
                text_input::TextInputEvent::Submit => {
                    let name = input.read(cx).content().trim().to_string();
                    if !name.is_empty() {
                        this.workspaces.rename_workspace(ws_id, name);
                    }
                }
                text_input::TextInputEvent::Cancel => {}
            }
            this.end_workspace_rename(cx);
        })
        .detach();

        input.focus_handle(cx).focus(window);
        self.workspace_rename = Some((ws_id, input));
        cx.notify();
    }

    /// Close the rename editor. Deliberately does NOT focus anything, same
    /// reasoning as `end_tab_rename`: reached from a `cx.subscribe` closure
    /// with no `Window`, and `render()`'s own guard (Step 10) reclaims
    /// focus for the terminal on the very next frame.
    pub(super) fn end_workspace_rename(&mut self, cx: &mut Context<Self>) {
        self.workspace_rename = None;
        cx.notify();
    }
```

- [ ] **Step 10: `render.rs` — focus-reclaim guard + sidebar layout**

Extend the combined focus guard's condition:

```rust
        if self.tab_rename.is_none()
            && self.workspace_rename.is_none()
            && (!self.chat.is_visible() || !self.chat.composer_focused(window, cx))
            && (!self.ai_block.is_visible() || !self.ai_block.composer_focused(window, cx))
        {
            window.focus(&self.focus_handle);
        }
```

Add `use super::leader::LeaderAction;` and `use super::sidebar;` to `render.rs`'s imports (alongside the existing `use super::{ai_block, chat_panel, pane_view, status_bar, tabs, GpuiShellRoot};`).

Build the sidebar's callbacks and rename element right before `middle_row` is constructed (same place `on_select_tab`/`rename` for the tab bar are built), and wrap the sidebar as the FIRST child of `middle_row` (left of `pane_area`, animated the same way `chat_panel`'s drawer already is on the right):

```rust
        let on_select_workspace: sidebar::render::WorkspaceSelectCallback =
            Rc::new(cx.listener(|this, idx: &usize, _window, cx| {
                if this.switch_workspace_to_index(*idx) {
                    cx.notify();
                }
            }));
        let on_new_workspace: sidebar::render::WorkspaceNewCallback =
            Rc::new(cx.listener(|this, _: &(), window, cx| {
                this.dispatch_leader_action(LeaderAction::NewWorkspace, window, cx);
            }));
        let on_close_workspace: sidebar::render::WorkspaceCloseCallback =
            Rc::new(cx.listener(|this, id: &usize, _window, cx| {
                if let Some(idx) = this.workspaces.workspaces().iter().position(|w| w.id == *id) {
                    this.close_workspace_at(idx, true, cx);
                    cx.notify();
                }
            }));
        let workspace_rename_element = self
            .workspace_rename
            .as_ref()
            .map(|(id, input)| (*id, input.clone().into_any_element()));
```

Replace the `middle_row` definition:

```rust
        let middle_row = div()
            .flex()
            .flex_1()
            .min_h_0()
            // `min_h_0`: a flex item's automatic minimum size is its content
            // size, so without this the pane row refuses to shrink below the
            // terminal grid it contains and pushes the tab bar off-screen on
            // a small window.
            .child(pane_area)
            .when(self.chat.is_visible(), |el| {
```

with:

```rust
        const SIDEBAR_OPEN_ANIM: Duration = Duration::from_millis(180);
        let middle_row = div()
            .flex()
            .flex_1()
            .min_h_0()
            // `min_h_0`: a flex item's automatic minimum size is its content
            // size, so without this the pane row refuses to shrink below the
            // terminal grid it contains and pushes the tab bar off-screen on
            // a small window.
            .when(self.sidebar.is_visible(), |el| {
                let bar = sidebar::render::render_workspace_sidebar(
                    &self.workspaces,
                    &self.config.colors,
                    on_select_workspace,
                    on_new_workspace,
                    on_close_workspace,
                    workspace_rename_element,
                );
                el.child(bar.with_animation(
                    "workspace-sidebar-drawer",
                    Animation::new(SIDEBAR_OPEN_ANIM).with_easing(ease_out_quint()),
                    |bar, delta| bar.w(px(sidebar::render::SIDEBAR_WIDTH_PX * delta)),
                ))
            })
            .child(pane_area)
            .when(self.chat.is_visible(), |el| {
```

(`CHAT_PANEL_OPEN_ANIM`'s own `const` declaration stays where it is at module scope; `SIDEBAR_OPEN_ANIM` here is a local `const` inside `render()` purely to keep this diff minimal — if `cargo fmt`/clippy objects to a local animation-duration const specifically, hoist it to module scope next to `CHAT_PANEL_OPEN_ANIM` instead, same value.)

- [ ] **Step 11: Build, test, dogfood**

Run: `cargo build 2>&1 | tail -60`, then `cargo test --lib 2>&1 | tail -40`, then `./scripts/ci-local.sh`.

Dogfood, in order:
1. `Leader s` opens the sidebar (animated grow-in from the left); `Leader s` again closes it. `Leader e e` does the same (both keys toggle the identical drawer).
2. With the sidebar open: click "+" — a new `wsN` workspace appears in the list and becomes active (highlighted), its own fresh terminal visible in the pane area.
3. Click a different row — switches to it, panes reflow, the row highlight follows.
4. Click "x" on a non-active row — that workspace disappears from the list, its terminal(s) are gone, the active workspace is unaffected.
5. Click "x" on the LAST remaining row — refused (still one workspace left, nothing happens, no crash).
6. `Leader W ,` while the sidebar is CLOSED — the sidebar opens automatically and the active row's name becomes an editable field with a cursor; type a name, Enter — commits, sidebar stays open, row shows the new name. Escape instead — cancels, name unchanged.
7. Open a tab rename (`Leader ,`) on a tab in workspace A, then click a DIFFERENT workspace's row in the sidebar without confirming — the tab-rename editor must vanish (not survive, not commit) and the app's keyboard must NOT be frozen afterward (type in the newly active workspace's terminal to confirm).

- [ ] **Step 12: Update `AGENTS.md`'s keybind table**

Add rows for `Leader w`, `Leader s`, `Leader W &`, `Leader W ,`, `Leader W j/k` to the Keybinds table in `AGENTS.md`, matching the existing row format (`| `Leader x` | Description |`). Place them near the existing tab/pane rows. This directly serves the M3 design's own §5 callout ("Reconcile `AGENTS.md`'s table against the code's actual bindings") for the subset this plan adds — the rest of that reconciliation (checking every OTHER existing row against the real code) is explicitly M3d's job per that same sentence; do not expand scope here.

- [ ] **Step 13: Commit**

```bash
git add src/gpui_shell/sidebar/ src/gpui_shell/mod.rs src/gpui_shell/leader.rs src/gpui_shell/input.rs src/gpui_shell/actions.rs src/gpui_shell/render.rs AGENTS.md
git commit -m "feat: Add the animated workspace sidebar drawer (M3c Task 4)."
```

---

## Exit Criteria (from the M3 design's §7, the M3c slice of it)

- The workspace sidebar opens as an animated drawer (`Leader s` or `Leader e e`), lists every workspace, and supports create (`Leader w` or the "+" button), switch (click a row, or `Leader W j`/`Leader W k`), rename (`Leader W ,`, inline in the row), and close (`Leader W &` or a row's "x" button) — with the terminal pane area reflowing automatically on open/close (a consequence of `middle_row`'s flex layout, not code this plan writes).
- Every existing M0-M2 keybind (tabs, splits, pane focus/zoom/close) and M3a/M3b surface (tab rename, chat panel, inline AI block) continues to work identically inside EVERY workspace, not just the first one.
- `scripts/ci-local.sh` is green and the full `cargo test --lib` suite passes after each task.
