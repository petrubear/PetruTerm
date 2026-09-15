// gpui chrome migration (M5b Task 1): "explain last output" / "fix last
// error" -- the two AI-query builders `Leader a e`/`Leader a f`, the
// command palette, and (Task 2) the suggestion pills all funnel into.
//
// Both real call sites (input.rs's 'a'-prefix leader continuation,
// palette_dispatch.rs's dispatch_palette_action, and Task 2's pill
// on_mouse_down callbacks) already have a real `&mut Window` in hand --
// unlike M5c's `SendToChat`, neither method here needs the deferred
// pending_*-drained-at-render() pattern.

use gpui::{Context, Window};

use crate::llm::shell_context::ShellContext;
use crate::term::Terminal;

use super::GpuiShellRoot;

impl GpuiShellRoot {
    /// `Leader a e` / palette "Explain Last Output" / the zero-state and
    /// post-response "Explain command"/"Explain more" pills (Task 2).
    pub(super) fn explain_last_output(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let output = self.last_terminal_lines(30);
        if output.is_empty() {
            return;
        }
        let query = format!("Explain this terminal output:\n```\n{output}\n```");
        self.run_ai_query(query, window, cx);
    }

    /// `Leader a f` / palette "Fix Last Error" / the zero-state and
    /// post-response "Fix last error" pills (Task 2).
    pub(super) fn fix_last_error(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let output = self.last_terminal_lines(30);
        let ctx = ShellContext::load();
        let query = match &ctx {
            Some(c) if !c.last_command.is_empty() => format!(
                "The command `{}` failed (exit code {}). Output:\n```\n{output}\n```\nHow do I \
                 fix this?",
                c.last_command, c.last_exit_code
            ),
            _ => format!("This command failed. Output:\n```\n{output}\n```\nHow do I fix this?"),
        };
        self.run_ai_query(query, window, cx);
    }

    /// Shared tail: open the panel if needed, drive `panel.input` (NOT the
    /// composer's own `TextInput` -- `submit()` reads from `panel.input`,
    /// confirmed against `stream.rs`'s own `handle_chat_composer_submit`),
    /// clear the composer's visible text to keep the real widget in sync
    /// (a discrepancy the wgpu build doesn't have, since it has no
    /// separate composer widget), then submit.
    pub(super) fn run_ai_query(
        &mut self,
        query: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.chat.is_visible() {
            self.chat.toggle(window, cx);
        }
        self.chat.panel.set_input(query);
        self.chat
            .composer
            .update(cx, |input, cx| input.set_content("", cx));
        self.chat.submit(&self.tokio_rt, cx);
    }

    /// Execute one confirmed inline agent action (`ChatPanel::resolve_
    /// action_yes`'s own return value, drained here from `render()`'s top
    /// -- see `mod.rs`'s new `pending_agent_action` field). Ported from
    /// `flush_pending_agent_action` (`src/app/frame.rs:259-316`).
    pub(super) fn flush_pending_agent_action(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        use crate::llm::agent_action::AgentAction;
        use crate::llm::ChatMessage;
        let Some(action) = self.pending_agent_action.take() else {
            return;
        };
        match action {
            AgentAction::RunCommand { cmd, .. } => {
                let note = format!("Running: `{cmd}`");
                let active_ws = self.workspaces.active();
                let active_tid =
                    active_ws.tab_panes[active_ws.tabs.active_index()].focused_terminal;
                if let Some(terminal) = self.terminals.get(&active_tid) {
                    let mut data = cmd.into_bytes();
                    data.push(b'\n');
                    terminal.write_input(&data);
                }
                self.chat.panel.messages.push(ChatMessage::assistant(note));
                cx.notify();
            }
            AgentAction::OpenFile { path } => {
                let cwd = self.cached_cwd.clone().unwrap_or_default();
                let abs = cwd.join(&path);
                let p = if abs.exists() {
                    abs.to_string_lossy().into_owned()
                } else {
                    path.clone()
                };
                let _ = std::process::Command::new("open").arg(&p).spawn();
                let note = format!("Opening: `{path}`");
                self.chat.panel.messages.push(ChatMessage::assistant(note));
                cx.notify();
            }
            AgentAction::ExplainOutput { last_n_lines } => {
                let output = self.last_terminal_lines(last_n_lines);
                if output.is_empty() {
                    return;
                }
                let query = format!("Explain this terminal output:\n```\n{output}\n```");
                self.run_ai_query(query, window, cx);
            }
        }
    }

    /// Read the bottom `n` visible terminal rows of the focused pane,
    /// joined with `\n`, trimmed. Ported from `Mux::last_terminal_lines`
    /// (`src/app/mux/mod.rs:611-627`), adapted to read the focused
    /// `Terminal` directly (same adaptation style M5c's `blocks.rs::
    /// row_text_and_absolute_row` already used for a sibling grid read).
    pub(super) fn last_terminal_lines(&self, n: usize) -> String {
        let active_ws = self.workspaces.active();
        let active_tid = active_ws.tab_panes[active_ws.tabs.active_index()].focused_terminal;
        let Some(terminal) = self.terminals.get(&active_tid) else {
            return String::new();
        };
        last_terminal_lines_for(terminal, n)
    }
}

fn last_terminal_lines_for(terminal: &Terminal, n: usize) -> String {
    terminal.with_term(|term| {
        use alacritty_terminal::grid::Dimensions;
        use alacritty_terminal::index::{Column, Line};
        let rows = term.screen_lines();
        let cols = term.columns();
        let start = rows.saturating_sub(n);
        let mut lines = Vec::new();
        for row in start..rows {
            let mut text = String::new();
            for col in 0..cols {
                let cell = &term.grid()[Line(row as i32)][Column(col)];
                text.push(if cell.c == '\0' { ' ' } else { cell.c });
            }
            lines.push(text.trim_end().to_string());
        }
        lines.join("\n").trim().to_string()
    })
}
