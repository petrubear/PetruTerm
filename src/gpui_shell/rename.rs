// gpui chrome migration (M3c post-Task-4 split): the tab-rename and
// workspace-rename inline editor flows. Split out of `actions.rs` for the
// 400-line convention -- `actions.rs` grew past 400 lines once M3c's three
// workspace tasks landed on top of it. Pure code motion: no logic changed.

use gpui::{AppContext, Context, Focusable, Window};

use super::{text_input, GpuiShellRoot};

impl GpuiShellRoot {
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
    /// the next render hits the `if self.tab_rename.is_none()` guard and
    /// returns focus to the terminal on its own, which also keeps exactly one
    /// place deciding who owns focus.
    pub(super) fn end_tab_rename(&mut self, cx: &mut Context<Self>) {
        self.tab_rename = None;
        cx.notify();
    }

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
    /// with no `Window`, and `render()`'s own guard reclaims focus for the
    /// terminal on the very next frame.
    pub(super) fn end_workspace_rename(&mut self, cx: &mut Context<Self>) {
        self.workspace_rename = None;
        cx.notify();
    }
}
