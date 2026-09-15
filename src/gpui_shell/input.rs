// gpui chrome migration (M2 Task 5b): keyboard input handling --
// `arrow_key_to_focus_dir` and `GpuiShellRoot::on_key_down`. Split out of
// `mod.rs` for the 400-line convention.

use gpui::{Context, Focusable, KeyDownEvent, Window};

use super::leader::LeaderAction;
use super::{panes, GpuiShellRoot};

/// Maps a gpui named-key string to the resize direction it drives under
/// `Leader Option+Arrow` / resize-mode continuation. gpui's own arrow-key
/// strings ("left"/"right"/"up"/"down", see `key_map::translate_key`), not
/// winit's `NamedKey::Arrow*` variants -- different event model, see this
/// module's own doc comment on why gpui_shell can't import winit at all.
pub(super) fn arrow_key_to_focus_dir(key: &str) -> Option<panes::FocusDir> {
    match key {
        "left" => Some(panes::FocusDir::Left),
        "right" => Some(panes::FocusDir::Right),
        "up" => Some(panes::FocusDir::Up),
        "down" => Some(panes::FocusDir::Down),
        _ => None,
    }
}

impl GpuiShellRoot {
    pub(super) fn on_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Palette key guard -- see `palette.rs`'s `maybe_handle_palette_key`.
        if self.maybe_handle_palette_key(event, window, cx) {
            return;
        }

        // Search bar key guard -- see `search_bar.rs`'s `maybe_handle_search_key`.
        if self.maybe_handle_search_key(event, window, cx) {
            return;
        }

        // InfoOverlay intercepts all keys (checked first, on top visually).
        // See `info_overlay.rs`'s `maybe_handle_info_overlay_key` doc.
        if self.maybe_handle_info_overlay_key(event, cx) {
            return;
        }

        // File picker key guard -- see `chat_panel/mod.rs`'s
        // `maybe_handle_file_picker_key` doc comment for why this is
        // mode-keyed rather than focus-keyed.
        if self.maybe_handle_file_picker_key(event, cx) {
            return;
        }

        // Inline-action confirm card's own key guard -- see
        // `standalone_keys.rs`'s `maybe_handle_confirm_action_key` doc
        // comment.
        if self.maybe_handle_confirm_action_key(event, cx) {
            return;
        }

        // ACP write/run confirm card's own key guard -- see `standalone_keys.rs`'s
        // `maybe_handle_awaiting_confirm_key` doc comment.
        if self.maybe_handle_awaiting_confirm_key(event, cx) {
            return;
        }

        // While the rename `TextInput` genuinely holds focus, every key
        // belongs to it, full stop -- checked before anything else,
        // including the leader-key arm below (renaming while chording the
        // leader key makes no sense, and letting the leader arm run first
        // would swallow keys the input never gets a chance to see). gpui
        // dispatches bound actions (the input's own key bindings, scoped to
        // its `"TextInput"` context) along the focus→root path BEFORE bubble
        // listeners like this one run, so those still reach the input
        // normally. What this guard stops is the *fall-through* case: plain
        // printable characters have no action binding (they route through
        // IME instead), so they always reach the tail of this function,
        // which unconditionally writes to the focused terminal's PTY. Without
        // this early return, every typed character during a rename would
        // land in the input AND get written to the live shell behind it.
        //
        // Keyed on `is_focused`, not merely `tab_rename.is_some()`: a rename
        // being open and the input actually owning focus are different
        // things -- clicking the terminal, the status bar, or blank tab-bar
        // space moves focus to `GpuiShellRoot`'s own root div (every
        // `track_focus`'d element auto-focuses itself on a mouse-down inside
        // its hitbox), and `tab_rename` stays `Some` the whole time (only
        // Enter/Escape/a different tab's click clear it). Guarding on
        // `is_some()` alone would freeze every key in the whole app the
        // moment that happened, with no route back except `Cmd+Q` -- this
        // guard exists to stop keystrokes reaching the PTY while the field
        // owns them, not to hold the app hostage once focus has moved on.
        if let Some((_, input)) = &self.tab_rename {
            if input.focus_handle(cx).is_focused(window) {
                return;
            }
        }

        // Same guard, same reasoning, for the workspace-rename editor.
        if let Some((_, input)) = &self.workspace_rename {
            if input.focus_handle(cx).is_focused(window) {
                return;
            }
        }

        // Same guard, same reasoning, for the chat composer (M3b): while it
        // holds focus every key is either one of `TextInput`'s own bound
        // actions (dispatched before this bubble listener runs) or a
        // printable character routed through IME -- neither should also
        // fall through to the PTY write at the end of this function. Keyed
        // on `composer_focused` (== `is_focused(window)`), NOT on
        // `self.chat.is_visible()`: the panel can be open while the
        // terminal holds focus (the user clicked back into it), and a
        // visibility-keyed guard would swallow every terminal keystroke in
        // that state -- exactly M3a's shipped Critical, reproduced here if
        // this guard checked the wrong thing.
        if self.chat.composer_focused(window, cx) {
            // Tab opens the file picker (the composer's own real focus
            // never changes -- see `maybe_handle_file_picker_key`'s own
            // doc comment). `TextInput` has no `Tab` binding of its own
            // (confirmed against `text_input/mod.rs`'s key-context
            // registration), so this is the only place Tab is ever
            // observed while the composer holds focus.
            if event.keystroke.key == "tab" {
                let cwd = self.cached_cwd.clone().unwrap_or_default();
                self.chat.open_file_picker_async(cwd);
                cx.notify();
            }
            return;
        }

        // Same guard, same reasoning, for the inline AI block's composer
        // (M3b Task 3) -- keyed on `composer_focused` (== `is_focused
        // (window)`), never on `self.ai_block.is_visible()`, per `ai_block.
        // rs`'s own doc comment (point 1). Checked as its own arm rather
        // than folded into the chat-composer check above: the two composers
        // are independent entities, and only one can hold focus at a time,
        // but either one holding it must short-circuit here the same way.
        if self.ai_block.composer_focused(window, cx) {
            return;
        }

        // Same guard, same reasoning, for the workspace sidebar (M3d) --
        // keyed on `is_focused(window)`, never on `self.sidebar.is_
        // visible()`: the drawer can be open while the terminal holds focus
        // (the user clicked back into it), and a visibility-keyed guard
        // here would swallow every terminal keystroke in that state -- the
        // exact M3a-class Critical this project has now avoided three times
        // over by keying every guard like it on real focus. See
        // `sidebar_nav.rs`'s own doc comment on `handle_sidebar_focused_key`
        // for the rest of the reasoning (moved there to keep this file under
        // the 400-line convention).
        if self.sidebar_focus_handle.is_focused(window) {
            self.handle_sidebar_focused_key(event, window, cx);
            return;
        }

        self.cursor_blink_on = true;
        self.cursor_last_blink = std::time::Instant::now();

        // ── Pane-resize mode continuation — Leader Option+Arrow started it
        // (below); while it's active, subsequent Option+Arrow presses keep
        // resizing without another leader press. Checked first, like
        // `src/app/input/mod.rs`'s own top-of-function resize_mode guard:
        // gpui has no `ModifiersChanged`-equivalent hook wired into
        // `on_key_down` here, so mode exit is inferred from the next
        // keystroke instead of Option's key-up -- the first key that isn't
        // an Option-held arrow clears it.
        if self.resize_mode {
            if event.keystroke.modifiers.alt {
                if let Some(dir) = arrow_key_to_focus_dir(&event.keystroke.key) {
                    let active = self.workspaces.active().tabs.active_index();
                    self.workspaces.active_mut().tab_panes[active].adjust_ratio(dir, 0.05);
                    cx.notify();
                    return;
                }
            }
            self.resize_mode = false;
        }

        // ── Leader key activation ────────────────────────────────────────
        // Leader-deadline expiry piggybacks on the 33ms poll loop (`new()`'s
        // `cx.spawn` block) -- this branch only ever SETS leader_active/
        // leader_deadline, never expires them (a key press always means the
        // deadline hasn't fired yet, since the poll loop would have cleared
        // leader_active first if it had).
        if !self.leader_active
            && event.keystroke.modifiers.control
            && !event.keystroke.modifiers.shift
            && !event.keystroke.modifiers.platform
            && event.keystroke.key == self.config.leader.key
        {
            self.leader_active = true;
            self.leader_deadline = Some(
                std::time::Instant::now()
                    + std::time::Duration::from_millis(self.config.leader.timeout_ms),
            );
            cx.notify(); // leader-active indicator (future status bar) needs to see this
            return;
        }

        // ── Leader key dispatch ──────────────────────────────────────────
        if self.leader_active {
            self.leader_active = false;
            self.leader_deadline = None;

            // 'a'-prefix continuation: a prior keypress (below) set
            // `leader_prefix` to `Some('a')` and re-armed the leader
            // timeout; this keypress is the second key of that two-key
            // chord. Checked before every other leader-dispatch arm, the
            // same position the wgpu build's own `leader_prefix` check
            // occupies (`src/app/input/mod.rs:314`).
            if let Some(prefix) = self.leader_prefix.take() {
                if prefix == 'a' {
                    match event.keystroke.key.as_str() {
                        "a" => self.dispatch_leader_action(LeaderAction::ToggleAiPanel, window, cx),
                        "e" => {
                            self.dispatch_leader_action(LeaderAction::ExplainLastOutput, window, cx)
                        }
                        "f" => self.dispatch_leader_action(LeaderAction::FixLastError, window, cx),
                        "z" => self.dispatch_leader_action(LeaderAction::UndoLastWrite, window, cx),
                        // `c` (ClearAiContext) stays out of scope -- ACP/
                        // tool-calling-dependent, per this milestone's own
                        // spec §9. Dropped, matching the wgpu build's own
                        // `_ => {}` fallthrough.
                        _ => {}
                    }
                }
                if prefix == 'e' && event.keystroke.key == "e" {
                    self.dispatch_leader_action(LeaderAction::ToggleWorkspaceSidebar, window, cx);
                }
                if prefix == 'W' {
                    let action = match event.keystroke.key.as_str() {
                        "&" => Some(LeaderAction::CloseWorkspace),
                        "," => Some(LeaderAction::RenameWorkspace),
                        "j" => Some(LeaderAction::NextWorkspace),
                        "k" => Some(LeaderAction::PrevWorkspace),
                        _ => None,
                    };
                    if let Some(action) = action {
                        self.dispatch_leader_action(action, window, cx);
                    }
                }
                return;
            }

            // Leader + Option + Arrow → resize (TD-042 parity).
            if event.keystroke.modifiers.alt {
                if let Some(dir) = arrow_key_to_focus_dir(&event.keystroke.key) {
                    let active = self.workspaces.active().tabs.active_index();
                    self.workspaces.active_mut().tab_panes[active].adjust_ratio(dir, 0.05);
                    self.resize_mode = true; // stay in resize mode for subsequent arrows
                    cx.notify();
                    return;
                }
            }

            // Leader + 1-9 → select tab by index (hardcoded, like Cmd+1-9).
            if let Ok(n) = event.keystroke.key.parse::<usize>() {
                if (1..=9).contains(&n) {
                    self.workspaces.active_mut().tabs.switch_to_index(n - 1);
                    cx.notify();
                    return;
                }
            }

            // Leader + a → enter the AI sub-prefix: re-arm the leader
            // timeout and wait for the second key. Only "a" (below) is
            // wired to anything this milestone.
            if event.keystroke.key == "a" {
                self.leader_active = true;
                self.leader_prefix = Some('a');
                self.leader_deadline = Some(
                    std::time::Instant::now()
                        + std::time::Duration::from_millis(self.config.leader.timeout_ms),
                );
                cx.notify();
                return;
            }

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

            // Data-driven dispatch for this milestone's ten actions
            // (c/&/n/b/,/%/"/x/z/h/j/k/l, per config/default/keybinds.lua).
            if let Some(action) = self.leader_map.get(event.keystroke.key.as_str()).copied() {
                self.dispatch_leader_action(action, window, cx);
            }
            return;
        }

        // ── Ctrl+Space — toggle the inline AI block ──────────────────────
        // Independent of leader state, like the wgpu build's own check
        // (`src/app/input/mod.rs:449-458`): a standalone combo, not a leader
        // chord, so it's checked here rather than folded into the leader
        // dispatch above -- reached only once neither `leader_active` branch
        // above already returned. Only reachable at all when neither
        // composer holds focus (both guards at the top of this function
        // already returned otherwise), so this can't be swallowed by a
        // focused text field's own key bindings. See `standalone_keys.rs`'s
        // own doc comment on `toggle_ai_block` for the rest of the
        // reasoning (moved there to keep this file under the 400-line
        // convention).
        if event.keystroke.modifiers.control
            && !event.keystroke.modifiers.shift
            && !event.keystroke.modifiers.platform
            && !event.keystroke.modifiers.alt
            && event.keystroke.key == "space"
        {
            self.toggle_ai_block(window, cx);
            return;
        }

        // ── Cmd+F — toggle the in-terminal search bar ────────────────────
        // Standalone combo, not a leader chord, same shape as `Ctrl+Space`
        // above. See `standalone_keys.rs`'s own doc comment on
        // `toggle_search_bar` for the full reasoning.
        if event.keystroke.modifiers.platform
            && !event.keystroke.modifiers.shift
            && !event.keystroke.modifiers.control
            && !event.keystroke.modifiers.alt
            && event.keystroke.key == "f"
        {
            self.toggle_search_bar(window, cx);
            return;
        }

        // ── Cmd+K — clear screen + scrollback ─────────────────────────────
        // Standalone combo, not a leader chord, same shape as `Cmd+F`
        // above. See `standalone_keys.rs`'s own doc comment on
        // `clear_focused_terminal` for the full reasoning.
        if event.keystroke.modifiers.platform
            && !event.keystroke.modifiers.shift
            && !event.keystroke.modifiers.control
            && !event.keystroke.modifiers.alt
            && event.keystroke.key == "k"
        {
            self.clear_focused_terminal(cx);
            return;
        }

        // ── Cmd+1-9 — switch to tab N (standard macOS pattern) ───────────
        if event.keystroke.modifiers.platform {
            if let Ok(n) = event.keystroke.key.parse::<usize>() {
                if (1..=9).contains(&n) {
                    self.workspaces.active_mut().tabs.switch_to_index(n - 1);
                    cx.notify();
                    return;
                }
            }
        }

        // Palette/search/overlays/composers/sidebar/leader/standalone combos
        // above all either handled the key or fell through deliberately;
        // what's left is the plain terminal path (paste, snippet-Tab-expand,
        // PTY write) -- see `key_write.rs`'s own doc comment for why that
        // tail lives in its own file.
        self.write_key_to_terminal(event, cx);
    }
}
