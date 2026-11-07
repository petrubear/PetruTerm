// Inline rename editor for a row of the Agents list (same pattern as `rename.rs`), except that a
// refused name keeps the editor open and shows why under the row. The name is pinned to the
// terminal id and agent pid the editor was opened for.

use gpui::{AppContext, Context, Entity, Focusable, Window};

use super::{text_input, GpuiShellRoot};

pub struct AgentRename {
    pub terminal_id: usize,
    pub pid: u32,
    pub input: Entity<text_input::TextInput>,
    pub error: Option<String>,
}

impl GpuiShellRoot {
    pub(super) fn begin_agent_rename(
        &mut self,
        terminal_id: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(row) = self
            .agent_rows()
            .into_iter()
            .find(|r| r.terminal_id == terminal_id)
        else {
            return;
        };
        self.agent_compose = None; // one editor at a time
        let colors = self.config.colors.clone();
        let current = row.custom_name.clone().unwrap_or_default();
        let input = cx.new(|cx| text_input::TextInput::new(cx, &colors, current, "agent name"));
        cx.subscribe(&input, move |this, input, event, cx| match event {
            text_input::TextInputEvent::Submit => {
                let text = input.read(cx).content().to_string();
                this.submit_agent_rename(&text, cx);
            }
            text_input::TextInputEvent::Cancel => this.end_agent_rename(cx),
        })
        .detach();
        input.focus_handle(cx).focus(window);
        self.agent_rename = Some(AgentRename {
            terminal_id,
            pid: row.agent_pid,
            input,
            error: None,
        });
        cx.notify();
    }

    fn submit_agent_rename(&mut self, text: &str, cx: &mut Context<Self>) {
        let Some((terminal_id, pid)) = self.agent_rename.as_ref().map(|r| (r.terminal_id, r.pid))
        else {
            return;
        };
        let rows = self.agent_rows();
        let locate = |t: usize| {
            rows.iter()
                .find(|r| r.terminal_id == t)
                .map(|r| format!("{} / {}", r.workspace_name, r.tab_title))
                .unwrap_or_else(|| "another agent".to_string())
        };
        match self.agent_names.set(terminal_id, pid, text, locate) {
            Ok(()) => self.end_agent_rename(cx),
            Err(e) => {
                if let Some(r) = &mut self.agent_rename {
                    r.error = Some(e.to_string());
                }
                cx.notify();
            }
        }
    }

    /// Close the editor without saving. Like `end_workspace_rename`, it does not move focus:
    /// `render()`'s own guard hands it back to the terminal on the next frame.
    pub(super) fn end_agent_rename(&mut self, cx: &mut Context<Self>) {
        self.agent_rename = None;
        cx.notify();
    }
}
