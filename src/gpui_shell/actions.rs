// gpui chrome migration (M2 Task 5b): pane/tab lifecycle actions --
// splitting, closing, zooming, and leader-action dispatch. Split out of
// `mod.rs` for the 400-line convention.

use std::rc::Rc;

use gpui::{App, AppContext, Context, Focusable, Window};

use super::leader::LeaderAction;
use super::panes::{PaneForest, SplitDir};
use super::{mouse, rasterize, spawn_terminal, text_input, GpuiShellRoot};

impl GpuiShellRoot {
    /// Spawn a terminal for a new pane and split the focused one around it.
    /// The new pane's real size is whatever taffy gives it on the next frame
    /// (`pane_view::fit_terminal` resizes the PTY to match), so the spawn
    /// dimensions here are only a placeholder.
    pub(super) fn split_focused(&mut self, dir: SplitDir) {
        let (terminal, gate) = match spawn_terminal(80, 24, &self.config) {
            Ok(pair) => pair,
            Err(e) => {
                log::error!("gpui-shell: failed to spawn terminal for split: {e:#}");
                return;
            }
        };
        let terminal_id = self.next_terminal_id;
        self.next_terminal_id += 1;
        self.terminals.insert(terminal_id, terminal);
        self.wakeup_gates.insert(terminal_id, gate);
        let ws = self.workspaces.active_mut();
        let active = ws.tabs.active_index();
        ws.tab_panes[active].split(dir, terminal_id);
        // Splitting while zoomed would otherwise create a pane the user
        // can't see (the zoomed one still fills the window) and move focus
        // to it -- their next keystroke would go somewhere invisible.
        ws.zoomed_pane = None;
    }

    /// Close the focused pane and reap its terminal. A no-op when it's the
    /// tab's last pane (`PaneForest::close_focused` refuses that case --
    /// closing the last pane is closing the tab, which is Task 4's job).
    pub(super) fn close_focused_pane(&mut self, cx: &mut App) {
        let active = self.workspaces.active().tabs.active_index();
        let Some(closed) = self.workspaces.active_mut().tab_panes[active].close_focused() else {
            return;
        };
        // SIGHUP the shell before dropping our Rc<Terminal> (below).
        // `Drop for Pty` now runs the full `shutdown()` sequence itself, so
        // this is no longer load-bearing against the close()-vs-read()
        // deadlock it was originally added for. It is kept because it
        // signals the shell *before* the drop rather than during it, which
        // gives the shell a head start on exiting and keeps the drop's own
        // `reader_thread.join()` short -- that join runs on the main thread,
        // so any time it spends blocked is a frozen UI.
        if let Some(terminal) = self.terminals.get(&closed) {
            terminal.pty.request_exit();
        }
        self.reap_pane(closed, cx);
    }

    /// Auto-close a pane whose shell process has already exited on its own
    /// (typing `exit`, `Ctrl+D`, the shell crashing) -- detected via
    /// `PtyEvent::Exit` on `Pty::rx`, drained by the poll loop in `new()`.
    /// Mirrors the wgpu app's own `Mux::close_terminal` (src/app/mux/mod.rs)
    /// in full now that Task 4 gives us tab-closing machinery: multi-pane
    /// tabs just lose the one pane; a tab whose exited pane was its last
    /// one is closed entirely via `close_tab_at`, which quits the app
    /// outright if that was also the app's last tab (see its own doc
    /// comment) -- exactly `frame.rs`'s `if self.close_exited_terminals(..)
    /// { event_loop.exit(); }` behavior, just reached from gpui's
    /// `cx.quit()` instead of winit's `event_loop.exit()`.
    ///
    /// No `Pty::request_exit()` call for either branch, unlike
    /// `close_focused_pane`/`LeaderAction::CloseTab`: the child is already
    /// gone by the time this runs (that's how we heard about it), so the
    /// reader thread's blocking `read()` has already returned (EOF) rather
    /// than being outstanding -- none of the deadlock risk `request_exit`'s
    /// doc comment describes applies, and SIGHUP'ing an already-reaped pid
    /// risks hitting a since-reused pid for no benefit.
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

    /// Close the tab at `tab_idx` (not necessarily the active one -- a
    /// background tab's last pane can exit while a different tab is
    /// focused) and reap every leaf terminal it owned. Quits the whole app
    /// via `cx.quit()` instead when `tab_idx` is the app's only remaining
    /// tab: gpui_shell's `render()` indexes `self.tab_panes[active_index]`
    /// unconditionally, so leaving zero tabs open is not a state this app
    /// can render at all -- matching the wgpu app's own behavior for the
    /// equivalent situation (`frame.rs`'s `if self.close_exited_terminals(
    /// exited) { event_loop.exit(); }`, reached when `Mux::close_terminal`
    /// closes a tab and none remain), and matching ordinary terminal
    /// emulators generally (closing your only tab closes the window).
    ///
    /// `signal_shells`: `true` sends every leaf's shell a SIGHUP first (the
    /// user explicitly closing a tab whose shells may still be alive,
    /// `LeaderAction::CloseTab`'s own prior behavior); `false` skips it
    /// (`on_terminal_exited`, whose sole leaf is already known dead).
    /// Returns whether a tab was actually closed (false only if `tab_idx`
    /// didn't name a real tab -- quitting the app counts as "closed").
    /// `ws_idx` names which workspace's tab list `tab_idx` indexes into,
    /// since callers now span workspaces via `on_terminal_exited`.
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

    /// Shared teardown for a terminal id that a `PaneForest` has just
    /// dropped from its tree (either call site above) -- keeps the two from
    /// drifting out of sync on which bookkeeping needs updating.
    pub(super) fn reap_pane(&mut self, terminal_id: usize, cx: &mut App) {
        // Both of these are keyed on the `Rc<Terminal>`'s heap address, not
        // on `terminal_id`, so they must be evicted while we still hold the
        // `Rc` -- `self.terminals.remove` below drops the last handle, after
        // which the address is gone (and reusable by a later pane).
        //
        // Missing these was an unbounded leak of one full-grid GPU texture
        // per closed pane: see `rasterize::evict_terminal`'s doc comment.
        if let Some(terminal) = self.terminals.get(&terminal_id) {
            let terminal_key = Rc::as_ptr(terminal) as usize;
            rasterize::evict_terminal(terminal_key, cx);
            mouse::forget_terminal(terminal_key);
        }
        self.terminals.remove(&terminal_id);
        self.wakeup_gates.remove(&terminal_id);
        self.rect_cache.borrow_mut().leaves.remove(&terminal_id);
        // terminal_id is globally unique, so at most one workspace can have
        // it zoomed -- checking all of them (cheap; there are at most a
        // handful) is simpler than threading a workspace index through
        // every caller of `reap_pane` just for this.
        for ws in self.workspaces.workspaces_mut() {
            if ws.zoomed_pane == Some(terminal_id) {
                ws.zoomed_pane = None;
            }
        }
    }

    /// Zoom the focused pane to fill the window, or unzoom if it already is.
    /// Zooming a tab that only has one pane is meaningless, so it's ignored.
    pub(super) fn toggle_zoom(&mut self) {
        let ws = self.workspaces.active_mut();
        let active = ws.tabs.active_index();
        let focused = ws.tab_panes[active].focused_terminal;
        ws.zoomed_pane = match ws.zoomed_pane {
            Some(id) if id == focused => None,
            _ if ws.tab_panes[active].root.leaf_count() > 1 => Some(focused),
            _ => None,
        };
    }

    /// Execute one resolved leader-key action (`on_key_down`'s leader
    /// dispatch branch). See `leader::LeaderAction`'s doc comment for why
    /// the set stops at these eleven variants.
    pub(super) fn dispatch_leader_action(
        &mut self,
        action: LeaderAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match action {
            LeaderAction::NewTab => {
                let (terminal, gate) = match spawn_terminal(80, 24, &self.config) {
                    Ok(pair) => pair,
                    Err(e) => {
                        log::error!("gpui-shell: failed to spawn terminal for new tab: {e:#}");
                        return;
                    }
                };
                let terminal_id = self.next_terminal_id;
                self.next_terminal_id += 1;
                self.terminals.insert(terminal_id, terminal);
                self.wakeup_gates.insert(terminal_id, gate);
                let ws = self.workspaces.active_mut();
                ws.tabs.new_tab("zsh");
                ws.tab_panes.push(PaneForest::new(terminal_id));
                // Same reasoning as `split_focused`: a zoomed pane from the
                // tab being left would otherwise linger, filling the window
                // even after the new tab (which has nothing zoomed) becomes
                // active.
                ws.zoomed_pane = None;
            }
            LeaderAction::CloseTab => {
                // Mirrors `Mux::cmd_close_tab` (src/app/mux/mod.rs:792-807),
                // via the shared `close_tab_at` helper (also used by
                // `on_terminal_exited` for the "shell exited as a tab's
                // last pane" case) so the two close paths can't drift
                // apart. `signal_shells: true` since this tab's shells may
                // still be alive (the user is closing it explicitly, not
                // reacting to an exit already observed).
                let ws_idx = self.workspaces.active_index();
                let tab_idx = self.workspaces.active().tabs.active_index();
                self.close_tab_at(ws_idx, tab_idx, true, cx);
            }
            LeaderAction::NextTab => self.workspaces.active_mut().tabs.next_tab(),
            LeaderAction::PrevTab => self.workspaces.active_mut().tabs.prev_tab(),
            LeaderAction::RenameTab => self.begin_tab_rename(window, cx),
            LeaderAction::SplitHorizontal => self.split_focused(SplitDir::Horizontal),
            LeaderAction::SplitVertical => self.split_focused(SplitDir::Vertical),
            LeaderAction::ClosePane => self.close_focused_pane(cx),
            LeaderAction::ZoomPane => self.toggle_zoom(),
            LeaderAction::FocusPane(dir) => {
                let active = self.workspaces.active().tabs.active_index();
                // Clone the Rc first, same reason as `on_drag` in render():
                // `focus_dir` needs `&mut self.workspaces.active_mut().
                // tab_panes[..]` and `&self.rect_cache`'s contents at once,
                // which a single `self.` borrow of both fields can't
                // express.
                let rects = self.rect_cache.clone();
                let rects = rects.borrow();
                self.workspaces.active_mut().tab_panes[active].focus_dir(dir, &rects);
            }
            LeaderAction::ToggleAiPanel => {
                self.chat.toggle(window, cx);
                // `toggle` only ever moves focus TO the composer (opening);
                // closing deliberately returns none, mirroring
                // `end_tab_rename`'s division of labor. This is the other
                // half: send focus back to the terminal right here rather
                // than waiting on render()'s guard, which can't tell "the
                // panel just closed" from "the composer still holds a stale
                // focus handle" -- gpui doesn't clear a `FocusHandle`'s
                // focused status just because its element left the tree.
                if !self.chat.is_visible() {
                    window.focus(&self.focus_handle);
                }
            }
        }
        cx.notify();
    }

    /// Open an editable field over the active tab's label, seeded with its
    /// current title and focused so the next keystroke goes to it.
    ///
    /// Pinned to the active tab's **id** at the moment the rename starts, not
    /// to "whichever tab is active" -- the active tab can change while the
    /// editor is still open (`Cmd+2`, `Leader n`, a tab click), and the
    /// commit below must land on the tab the user actually opened the editor
    /// for, not whatever happens to be active when Enter is pressed.
    pub(super) fn begin_tab_rename(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((tab_id, title)) = self
            .workspaces
            .active()
            .tabs
            .active_tab()
            .map(|t| (t.id, t.title.clone()))
        else {
            return;
        };
        let colors = self.config.colors.clone();
        let input = cx.new(|cx| text_input::TextInput::new(cx, &colors, title, "tab name"));

        // Subscribe before storing: the parent owns the outcome, so Enter and
        // Escape resolve here rather than inside the primitive, which has no
        // idea what is being renamed.
        cx.subscribe(&input, move |this, input, event, cx| {
            match event {
                text_input::TextInputEvent::Submit => {
                    let name = input.read(cx).content().trim().to_string();
                    // An all-whitespace name would render as a blank pill with
                    // no way to tell which tab it is; treat it as a cancel.
                    if !name.is_empty() {
                        this.workspaces.active_mut().tabs.rename_tab(tab_id, name);
                    }
                }
                text_input::TextInputEvent::Cancel => {}
            }
            this.end_tab_rename(cx);
        })
        .detach();

        input.focus_handle(cx).focus(window);
        self.tab_rename = Some((tab_id, input));
        cx.notify();
    }

    /// Close the rename editor. Deliberately does NOT focus anything: it is
    /// reached from a `cx.subscribe` closure, which is handed no `Window`,
    /// and `FocusHandle::focus` needs one. Clearing the field is enough --
    /// the next render hits Step 3's `if self.tab_rename.is_none()` guard and
    /// returns focus to the terminal on its own, which also keeps exactly one
    /// place deciding who owns focus.
    pub(super) fn end_tab_rename(&mut self, cx: &mut Context<Self>) {
        self.tab_rename = None;
        cx.notify();
    }
}
