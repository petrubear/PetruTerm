// "Send message..." editor of the Agents list: an inline text field on the row, pinned to the
// agent process it was opened for. Sends as the user (see `agent_msg::send_as_user`); a refusal
// keeps the editor open and shows the reason under the row. Same shape as `agent_rename.rs`.

use gpui::{AppContext, Context, Entity, Focusable, Window};

use super::{text_input, GpuiShellRoot};
use crate::term::messaging::Outcome;

pub struct AgentCompose {
    pub terminal_id: usize,
    pub pid: u32,
    pub input: Entity<text_input::TextInput>,
    pub error: Option<String>,
}

impl GpuiShellRoot {
    pub(super) fn begin_agent_compose(
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
        self.agent_rename = None; // one editor at a time
        let colors = self.config.colors.clone();
        let who = row.custom_name.clone().unwrap_or_else(|| row.agent.clone());
        let input =
            cx.new(|cx| text_input::TextInput::new(cx, &colors, "", format!("message to {who}")));
        cx.subscribe(&input, move |this, input, event, cx| match event {
            text_input::TextInputEvent::Submit => {
                let text = input.read(cx).content().to_string();
                this.submit_agent_compose(&text, cx);
            }
            text_input::TextInputEvent::Cancel => this.end_agent_compose(cx),
        })
        .detach();
        input.focus_handle(cx).focus(window);
        self.agent_compose = Some(AgentCompose {
            terminal_id,
            pid: row.agent_pid,
            input,
            error: None,
        });
        cx.notify();
    }

    fn submit_agent_compose(&mut self, text: &str, cx: &mut Context<Self>) {
        let Some(terminal_id) = self.agent_compose.as_ref().map(|c| c.terminal_id) else {
            return;
        };
        match self.send_as_user(terminal_id, text) {
            Outcome::Delivered(_) => self.end_agent_compose(cx),
            other => {
                if let Some(c) = &mut self.agent_compose {
                    let line = other.cli_line();
                    c.error = Some(line.strip_prefix("error: ").unwrap_or(&line).to_string());
                }
                cx.notify();
            }
        }
    }

    /// Close the editor without sending; focus goes back to the terminal on the next frame.
    pub(super) fn end_agent_compose(&mut self, cx: &mut Context<Self>) {
        self.agent_compose = None;
        cx.notify();
    }
}
