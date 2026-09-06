// gpui chrome migration (M2 Task 5b): keyboard input handling --
// `arrow_key_to_focus_dir` and `GpuiShellRoot::on_key_down`. Split out of
// `mod.rs` for the 400-line convention.

use gpui::{Context, Focusable, KeyDownEvent, Window};

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
                    let active = self.tabs.active_index();
                    self.tab_panes[active].adjust_ratio(dir, 0.05);
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

            // Leader + Option + Arrow → resize (TD-042 parity).
            if event.keystroke.modifiers.alt {
                if let Some(dir) = arrow_key_to_focus_dir(&event.keystroke.key) {
                    let active = self.tabs.active_index();
                    self.tab_panes[active].adjust_ratio(dir, 0.05);
                    self.resize_mode = true; // stay in resize mode for subsequent arrows
                    cx.notify();
                    return;
                }
            }

            // Leader + 1-9 → select tab by index (hardcoded, like Cmd+1-9).
            if let Ok(n) = event.keystroke.key.parse::<usize>() {
                if (1..=9).contains(&n) {
                    self.tabs.switch_to_index(n - 1);
                    cx.notify();
                    return;
                }
            }

            // Data-driven dispatch for this milestone's ten actions
            // (c/&/n/b/,/%/"/x/z/h/j/k/l, per config/default/keybinds.lua).
            if let Some(action) = self.leader_map.get(event.keystroke.key.as_str()).copied() {
                self.dispatch_leader_action(action, window, cx);
            }
            return;
        }

        // ── Cmd+1-9 — switch to tab N (standard macOS pattern) ───────────
        if event.keystroke.modifiers.platform {
            if let Ok(n) = event.keystroke.key.parse::<usize>() {
                if (1..=9).contains(&n) {
                    self.tabs.switch_to_index(n - 1);
                    cx.notify();
                    return;
                }
            }
        }

        let active_tid = self.tab_panes[self.tabs.active_index()].focused_terminal;
        let Some(terminal) = self.terminals.get(&active_tid) else {
            return;
        };
        // Any keystroke -- paste included -- snaps the view back to the
        // live edge, matching the wgpu app's own key handler
        // (src/app/input/mod.rs's scroll_to_bottom() call before every key
        // write). alacritty's grid deliberately pins a scrolled view even
        // as new output arrives, so without this a key press while
        // scrolled back leaves its own output landing off-screen.
        // `cx.notify()` here, not just below: a swallowed key (an unbound
        // Cmd-combo, e.g.) reaches neither this function's other `notify()`
        // calls, but scroll_to_bottom() already ran unconditionally above
        // -- without this, the view would jump to the bottom in Terminal
        // state but not on screen until the poll loop's own next incidental
        // repaint (up to 530ms later, the blink toggle).
        terminal.scroll_to_bottom();
        cx.notify();

        // Cmd+V paste. `key_map::translate_key` never sees this: gpui only
        // populates `key_char` when cmd is NOT held (see its own doc
        // comment), and there's no gpui keybinding action claiming Cmd+V
        // either, so it falls through as an unbound cmd-combo. Ported from
        // the wgpu app's own paste path (`frame.rs`'s `flush_pending_paste`)
        // minus its background-thread dance: that existed to keep arboard's
        // clipboard read off the main thread (TD-PERF-15), a cost gpui's own
        // `cx.read_from_clipboard()` doesn't have (a direct, already
        // in-process platform call).
        if event.keystroke.modifiers.platform && event.keystroke.key == "v" {
            if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
                if terminal.bracketed_paste_mode() {
                    let mut data = b"\x1b[200~".to_vec();
                    data.extend_from_slice(text.as_bytes());
                    data.extend_from_slice(b"\x1b[201~");
                    terminal.write_input(&data);
                } else {
                    terminal.write_input(text.as_bytes());
                }
                cx.notify();
            }
            return;
        }

        let mode = terminal.with_term(|term| *term.mode());
        if let Some(bytes) = super::key_map::translate_key(
            &event.keystroke,
            mode,
            self.config.keyboard.option_as_meta,
        ) {
            terminal.write_input(&bytes);
            cx.notify();
        }
    }
}
