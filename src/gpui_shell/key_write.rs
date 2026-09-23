// The tail of `on_key_down`: paste, snippet Tab-expand, and the final PTY
// key write.

use gpui::{Context, KeyDownEvent};

use super::GpuiShellRoot;

impl GpuiShellRoot {
    /// Snap the active terminal's scroll to the live edge, then route the
    /// key: Cmd+V paste, snippet Tab-expand, or the plain PTY write via
    /// `key_map::translate_key`. Called from `on_key_down`'s tail, once
    /// every earlier guard (palette/search/overlays/composers/leader/
    /// standalone combos) has already returned.
    pub(super) fn write_key_to_terminal(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) {
        let active_ws = self.workspaces.active();
        let active_tid = active_ws.tab_panes[active_ws.tabs.active_index()].focused_terminal;
        {
            let Some(terminal) = self.terminals.get(&active_tid) else {
                return;
            };
            // Any keystroke -- paste included -- snaps the view back to the
            // live edge, matching the wgpu app's own key handler. alacritty's
            // grid pins a scrolled view even as new output arrives, so without
            // this a key press while scrolled back leaves its output off-screen.
            terminal.scroll_to_bottom();
            cx.notify();
        }

        // Cmd+V paste. `key_map::translate_key` never sees this: gpui only
        // populates `key_char` when cmd is NOT held, and there's no gpui
        // keybinding action claiming Cmd+V either, so it falls through as an
        // unbound cmd-combo. Ported from the wgpu app's own paste path minus
        // background-thread dance: gpui's `cx.read_from_clipboard()` has no
        // such overhead (direct platform call).
        if event.keystroke.modifiers.platform && event.keystroke.key == "v" {
            if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
                self.paste_text_to_active_terminal(&text);
                cx.notify();
            }
            return;
        }

        if self.maybe_expand_snippet_tab(event, active_tid, cx) {
            return;
        }

        let Some(terminal) = self.terminals.get(&active_tid) else {
            return;
        };
        let mode = terminal.with_term(|term| *term.mode());
        if let Some(bytes) = super::key_map::translate_key(
            &event.keystroke,
            mode,
            self.config.keyboard.option_as_meta,
        ) {
            terminal.write_input(&bytes);
            self.track_snippet_key(event);
            cx.notify();
        }
    }
}
