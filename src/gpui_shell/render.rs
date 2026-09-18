// gpui chrome migration (M2 Task 5b): `impl Render for GpuiShellRoot`.
// Split out of `mod.rs` for the 400-line convention.

use std::rc::Rc;
use std::time::Duration;

use gpui::{
    div, ease_out_quint, prelude::*, px, Animation, AnimationExt as _, Context, Render, Window,
};

use super::pane_view::to_rgba;
use super::{
    ai_block, chat_panel, context_menu, info_overlay, palette, pane_view, render_callbacks,
    search_bar, status_bar, tabs, toast, GpuiShellRoot,
};

/// Duration of the drawer's opening grow animation (Step 3). Closing is
/// instant -- see this file's own `render()` doc comment on the animated
/// child for why gpui 0.2.2's `Animation`/`with_animation` only gets this
/// one direction for free.
const CHAT_PANEL_OPEN_ANIM: Duration = Duration::from_millis(180);

/// Outer window padding and the gap between floating "cards" (sidebar,
/// terminal, chat panel, status bar) -- the floating-card visual language
/// adopted in the visual-polish pass 2 rewrite (2026-09-17), matching the
/// approved mockup's own margin/gap rhythm. Before this pass every region
/// rendered edge-to-edge/full-bleed against the window and against each
/// other (confirmed wrong live, against a real screenshot vs. the mockup --
/// not merely under-styled).
///
/// `pub(super)` (not private): `render_sidebar.rs`'s own drag handle floats
/// over exactly this gap rather than adding its own width beside it -- see
/// that file's own doc comment on why.
pub(super) const CARD_GAP_PX: f32 = 10.0;

impl Render for GpuiShellRoot {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Runs before the focus-reclaim guard below: dispatching a
        // confirmed palette action (Task 3's `dispatch_palette_action`) can
        // itself change `self.palette.is_visible()` this same frame, and
        // the guard needs to see that change to correctly reclaim focus
        // for the terminal without a one-frame lag. `CommandPalette::
        // confirm()` (called from `palette_query`'s `cx.subscribe`
        // callback, `mod.rs`) already closes the palette itself before
        // this ever fires, so no explicit close call is needed here.
        if let Some(action) = self.pending_palette_action.take() {
            self.dispatch_palette_action(action, window, cx);
            window.focus(&self.focus_handle);
        }

        if let Some(text) = self.pending_send_to_chat.take() {
            if !self.chat.is_visible() {
                self.chat.toggle(window, cx);
            }
            self.chat.composer.update(cx, |input, cx| {
                input.set_content(&text, cx);
            });
        }

        self.flush_pending_agent_action(window, cx);
        self.flush_pending_pty_run();

        // Sync query state from the real `TextInput` widgets' live content
        // -- see `palette.rs`'s/`search_bar.rs`'s own doc comments on
        // `drive_palette_query`/`drive_search_query`.
        self.drive_palette_query(cx);
        self.drive_search_query(cx);

        // Search: run the query if dirty, scroll to the current match if
        // needed. See `search_bar.rs`'s own doc comment on `drive_search`
        // for the full reasoning (moved there to keep this file under the
        // 400-line convention).
        self.drive_search();

        // Skipped while a child owns focus. This runs every frame, and the
        // poll loop repaints at ~30Hz, so focusing unconditionally would tear
        // focus away from the rename editor ~30 times a second and make it
        // look like typing does nothing.
        //
        // Each composer's half of this guard is keyed on `composer_focused`
        // (`is_focused(window)`), NOT on `is_visible()`: the panel/block can
        // be open while the terminal holds focus (the user clicked back into
        // it), and a visibility-keyed guard would then rip focus away from
        // the terminal ~30 times a second. The `!is_visible() ||` half
        // handles the opposite gap: a composer-side `/q` or Enter-after-
        // `Done` close runs inside a `cx.subscribe` callback with no
        // `Window`, so it can't call `window.focus` -- `composer_focused`
        // would otherwise report stale "true" forever (gpui's `is_focused`
        // is just an id comparison; nothing clears it when the composer's
        // div leaves the tree). Safe to OR in: it only forces a refocus
        // while the composer is already hidden, a state in which it can
        // never legitimately hold focus.
        if self.tab_rename.is_none()
            && self.workspace_rename.is_none()
            && (!self.chat.is_visible() || !self.chat.composer_focused(window, cx))
            && (!self.ai_block.is_visible() || !self.ai_block.composer_focused(window, cx))
            && (!self.palette.visible || !self.palette_query_focused(window, cx))
        {
            window.focus(&self.focus_handle);
        }

        let active_index = self.workspaces.active().tabs.active_index();
        let (cell_width, cell_height) = super::font_state::measured_cell_size();

        // A zoomed pane that no longer belongs to the active tab (tab switch,
        // pane closed) has to be dropped before it's used, mirroring the wgpu
        // app's own "zoomed pane no longer in active tab -- clear zoom" guard
        // in src/app/frame.rs.
        if let Some(id) = self.workspaces.active().zoomed_pane {
            if !self.workspaces.active().tab_panes[active_index]
                .root
                .leaf_ids()
                .contains(&id)
            {
                self.workspaces.active_mut().zoomed_pane = None;
            }
        }

        // Drop last frame's geometry before this frame's prepaint pass
        // repopulates it: a pane that just closed, or one belonging to a tab
        // that's no longer active, must not keep answering focus_dir's
        // nearest-neighbour search with a rect it no longer occupies.
        {
            let mut rects = self.rect_cache.borrow_mut();
            rects.leaves.clear();
            rects.separators.clear();
        }

        // See `render_callbacks.rs`'s own doc comment for why these five
        // are built in one place (moved there to keep this file under the
        // 400-line convention) and why each holds a WEAK handle.
        let (
            on_focus,
            on_drag,
            on_right_click,
            on_context_action,
            on_close_context_menu,
            on_fix_last_error,
            on_explain_last_output,
        ) = render_callbacks::build_frame_callbacks(cx);

        let on_tab_right_click = render_callbacks::build_tab_right_click_callback(cx);

        // Focus-border color: the active tab's own accent if it has one set,
        // else the theme's accent -- mirrors the wgpu app's own
        // `build_focus_border` (src/app/frame.rs), which resolves the same
        // fallback. `pane_count` collapses to 1 while zoomed (matching the
        // wgpu app's `pane_infos.len()` there, since only one pane is ever
        // visible), so the border check below stays a single `> 1` test
        // instead of a separate zoomed-vs-not branch.
        let focus_accent = self
            .workspaces
            .active()
            .tabs
            .active_tab()
            .and_then(|t| t.accent_color)
            .unwrap_or(self.config.colors.ui_accent);
        let pane_count = if self.workspaces.active().zoomed_pane.is_some() {
            1
        } else {
            self.workspaces.active().tab_panes[active_index]
                .root
                .leaf_count()
        };

        let pane_ctx = pane_view::PaneRenderCx {
            terminals: &self.terminals,
            focused: self.workspaces.active().tab_panes[active_index].focused_terminal,
            colors: &self.config.colors,
            cell_width,
            cell_height,
            cursor_blink_on: self.cursor_blink_on,
            scrollback: self.config.scrollback_lines as usize,
            rects: self.rect_cache.clone(),
            on_focus,
            on_drag,
            search: self
                .search_bar
                .visible
                .then(|| (self.search_bar.matches.clone(), self.search_bar.current)),
            on_right_click,
            focus_accent,
            pane_count,
        };
        let panes = match self.workspaces.active().zoomed_pane {
            Some(terminal_id) => pane_view::render_leaf(terminal_id, &pane_ctx),
            None => pane_view::render_pane_tree(
                &self.workspaces.active().tab_panes[active_index].root,
                &pane_ctx,
            ),
        };

        let on_select_tab: tabs::TabSelectCallback =
            Rc::new(cx.listener(|this, idx: &usize, _window, cx| {
                // Clicking a DIFFERENT tab dismisses an in-progress rename
                // rather than leaving it open on a tab the user has
                // navigated away from. Without this the app reaches a dead
                // end: `on_key_down`'s rename guard blocks every keyboard
                // route back, and this listener is the only way to get from
                // a background tab to the one being renamed.
                //
                // Guarded on the clicked tab's id, not merely
                // `tab_rename.is_some()`: the renaming tab's cell is padded
                // (`tabs.rs`'s `.px_2().py_1()`) around a `size_full()`
                // `TextInput`, so its hitbox is bigger than the field's --
                // clicking that padding (a natural "put the cursor at the
                // start" miss) still lands on THIS listener for the SAME
                // tab, and clicking a tab must never dismiss a rename open
                // on itself. Copy the id out before touching `this.tabs`
                // (the borrows can't overlap otherwise).
                //
                // This makes the outcome correct by construction, independent
                // of `TextInput::on_mouse_down`'s `cx.stop_propagation()`
                // (`text_input/edit.rs`) -- that call still matters (it's
                // what lets a click inside the field reach the field at all
                // instead of also being treated as a tab click), but it is
                // now defense in depth here, not the only thing standing
                // between an in-editor click and a discarded rename.
                //
                // Dismiss discards the edit rather than committing it: an
                // explicit click elsewhere is not a confirmation, and a
                // silent rename to a half-typed string is the same class of
                // surprise as the wrong-tab commit this pinning already
                // fixed. Re-renaming is cheap; an unwanted rename is not.
                let rename_id = this.tab_rename.as_ref().map(|(id, _)| *id);
                let clicked_id = this.workspaces.active().tabs.tabs().get(*idx).map(|t| t.id);
                if rename_id.is_some() && rename_id != clicked_id {
                    this.end_tab_rename(cx);
                }
                if this.workspaces.active_mut().tabs.switch_to_index(*idx) {
                    cx.notify();
                }
            }));
        let rename = self
            .tab_rename
            .as_ref()
            .map(|(id, input)| (*id, input.clone().into_any_element()));
        let tab_bar = tabs::render_tab_bar(
            &self.workspaces.active().tabs,
            &self.config.colors,
            on_select_tab,
            on_tab_right_click,
            rename,
        );

        // Status bar row -- built from the poll-loop-refreshed cwd/git-branch/
        // exit-code state above plus this frame's leader/zoom state, same
        // inputs `StatusBar::build` takes in the wgpu app's own render path
        // (`src/app/frame.rs`). `leader_resize_mode` here is `resize_mode`
        // (set by a completed `Leader Option+Arrow`) OR a live separator
        // drag, since gpui_shell has no `ModifiersChanged`-equivalent hook
        // to ask "is Option currently held" outside of a keystroke (see
        // `on_key_down`'s own doc comment on that gap).
        let status_bar_row = self.config.status_bar.enabled.then(|| {
            let leader_resize_mode = self.resize_mode || pane_view::is_dragging_separator();
            let sb_colors = self.config.colors.status_bar_colors();
            let bar = status_bar::StatusBar::build(
                self.leader_active,
                leader_resize_mode,
                &self.config.leader.key,
                self.cached_cwd.as_deref(),
                self.git_branch.cache.as_deref(),
                self.exit_code.cache,
                self.workspaces.active().zoomed_pane.is_some(),
                self.config.status_bar.style.clone(),
                self.battery.cache,
                &sb_colors,
            );
            status_bar::render_status_bar(&bar, &sb_colors)
        });

        // Middle row: the pane tree plus, when open, the chat panel drawer --
        // flex siblings in a row (this div's default flex direction), NOT a
        // manually computed viewport split (§3.3: no `resize_terminals_
        // for_panel` port). `pane_view.rs`'s `fit_terminal`/
        // `on_children_prepainted` already resize each PTY to whatever box
        // taffy hands it, so the terminal reflowing when the drawer opens or
        // closes is just a consequence of this layout, not code this task
        // has to write.
        //
        // The drawer only gets an ANIMATED width on open: gpui 0.2.2's
        // `AnimationElement` (`with_animation`) restarts its clock from
        // `Instant::now()` the first time a given element id is laid out
        // after not appearing in the previous frame (`Window::
        // with_element_state` drops per-id state for ids not touched last
        // frame), which is exactly "the drawer just (re)appeared" -- so
        // wrapping it here gives a real grow-in every time it opens. There
        // is no equivalent for closing: this element is removed from the
        // tree the instant `visible` flips false (`when` below), so there is
        // no frame in which a shrinking width could be painted. Ship it
        // unanimated on close rather than hand-rolling a tween in the poll
        // loop to keep a "closing" copy of this div alive across frames.
        //
        // Populate the markdown wrapped-line cache for settled messages
        // BEFORE the read-only `render_chat_panel` call below reads it
        // (`ChatPanelView::sync_markdown_cache`'s own doc comment has the
        // full reasoning) -- hoisted out of the `.when(...)` closure below
        // so it runs against a plain `&mut self.chat`, not a value the
        // closure would otherwise have to capture mutably alongside
        // `&self.config` immutably.
        if self.chat.is_visible() {
            self.chat.sync_markdown_cache();
        }

        // The pane area is `.relative()` so the inline AI block (M3b Task 3)
        // can anchor an `.absolute().bottom_0()` overlay to it -- a `div()`
        // overlay over the focused pane's own area, not the wgpu build's
        // bottom-`AI_BLOCK_ROWS`-of-the-grid pixel math (`chat.rs:1430-
        // 1567`), which has no equivalent once cells aren't hand-shaped
        // quads. `ai_block.rs`'s doc comment covers the guard/streaming/
        // error-recovery side of this surface; this is only the layout half.
        let pane_area = div()
            .relative()
            .flex()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .child(panes)
            .when(self.ai_block.is_visible(), |el| {
                el.child(ai_block::render_ai_block(
                    &self.ai_block,
                    &self.config.colors,
                ))
            })
            .when(self.search_bar.visible, |el| {
                el.child(search_bar::render_search_bar(
                    &self.search_bar,
                    &self.search_query,
                    &self.config.colors,
                ))
            });

        // Terminal "card": tab strip + pane area as one floating panel with
        // its own rounded/bordered frame -- part of the floating-card visual
        // language below, replacing the tab bar's old role as a bar spanning
        // the *entire* window width (even over the sidebar).
        let terminal_card = div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .rounded_lg()
            .overflow_hidden()
            .border_1()
            .border_color(to_rgba(self.config.colors.ui_border))
            .bg(to_rgba(self.config.colors.ui_surface))
            .child(tab_bar)
            .child(pane_area);

        let middle_row = div()
            .flex()
            .flex_1()
            .min_h_0()
            .gap(px(CARD_GAP_PX))
            // `min_h_0`: a flex item's automatic minimum size is its content
            // size, so without this the pane row refuses to shrink below the
            // terminal grid it contains and pushes the tab bar off-screen on
            // a small window.
            .when(self.sidebar.is_visible(), |el| {
                el.child(self.render_sidebar_drawer(cx))
            })
            .child(terminal_card)
            .when(self.chat.is_visible(), |el| {
                let panel = chat_panel::render_chat_panel(
                    &self.chat,
                    &self.config.llm,
                    &self.config.colors,
                    on_fix_last_error,
                    on_explain_last_output,
                );
                el.child(panel.with_animation(
                    "chat-panel-drawer",
                    Animation::new(CHAT_PANEL_OPEN_ANIM).with_easing(ease_out_quint()),
                    |panel, delta| panel.w(px(chat_panel::PANEL_WIDTH_PX * delta)),
                ))
            });

        div()
            .track_focus(&self.focus_handle)
            .on_key_down(cx.listener(Self::on_key_down))
            .relative()
            .flex()
            .flex_col()
            .size_full()
            .gap(px(CARD_GAP_PX))
            .p(px(CARD_GAP_PX))
            .bg(to_rgba(self.config.colors.background))
            .child(middle_row)
            .when_some(status_bar_row, |el, bar| el.child(bar))
            .when(self.info_overlay.is_visible(), |el| {
                el.child(info_overlay::render_info_overlay(
                    &self.info_overlay,
                    &self.config.colors,
                ))
            })
            .when(self.palette.visible, |el| {
                el.child(palette::render_command_palette(
                    &self.palette,
                    &self.palette_query,
                    &self.config.colors,
                ))
            })
            .when(self.context_menu.visible, |el| {
                el.child(context_menu::render_context_menu(
                    &self.context_menu,
                    &self.config.colors,
                    on_context_action,
                    on_close_context_menu,
                ))
            })
            .when_some(self.toast.clone(), |el, (msg, _)| {
                el.child(toast::render_toast(&msg, &self.config.colors))
            })
    }
}
