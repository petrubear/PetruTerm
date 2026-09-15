// gpui chrome migration (M5a Task 2): per-terminal exit-code tracking and
// final-output caching -- neither exists in `gpui_shell` today (`poll.rs`
// already drains `PtyEvent::Exit(code)` but discards `code`; a reaped
// terminal's grid is simply gone). Built for ACP's own `terminal/output`/
// `terminal/wait_for_exit` (Task 4), which need to answer "what did this
// pane print" and "did it exit, with what code" even after the pane
// itself has closed -- mirrors `Mux::{terminal_output_text,
// terminal_exit_code}` (`src/app/mux/mod.rs:637-673`) exactly.

use crate::term::Terminal;

use super::GpuiShellRoot;

/// Cap on `terminal_final_output`'s size -- oldest entry evicted past
/// this, same bounded-cache shape `Mux::retain_closed_terminal` already
/// established for its own identical map.
const MAX_FINAL_OUTPUT_ENTRIES: usize = 64;

impl GpuiShellRoot {
    /// Every visible row of `terminal_id`'s grid if it's still alive,
    /// trimmed of trailing empty lines; the cached final output if it has
    /// already been reaped; empty string if neither.
    #[allow(dead_code)]
    pub(super) fn terminal_output_text(&self, terminal_id: usize) -> String {
        let Some(terminal) = self.terminals.get(&terminal_id) else {
            return self
                .terminal_final_output
                .get(&terminal_id)
                .cloned()
                .unwrap_or_default();
        };
        full_grid_text(terminal)
    }

    /// `None` while `terminal_id` is still alive; its cached exit code
    /// once it has been reaped.
    #[allow(dead_code)]
    pub(super) fn terminal_exit_code(&self, terminal_id: usize) -> Option<i32> {
        if self.terminals.contains_key(&terminal_id) {
            return None;
        }
        self.terminal_exit_codes.get(&terminal_id).copied()
    }

    /// Capture `terminal_id`'s exit code just before `poll.rs` reaps it.
    /// Called from `poll.rs`'s own `PtyEvent::Exit(code)` arm.
    pub(super) fn record_terminal_exit_code(&mut self, terminal_id: usize, code: i32) {
        self.terminal_exit_codes.insert(terminal_id, code);
    }

    /// Capture `terminal_id`'s final grid text just before its last
    /// `Rc<Terminal>` is dropped. Called from `actions.rs`'s own pane-
    /// removal sites, right before `self.terminals.remove(...)`.
    pub(super) fn record_terminal_final_output(&mut self, terminal_id: usize) {
        if let Some(terminal) = self.terminals.get(&terminal_id) {
            let text = full_grid_text(terminal);
            if self.terminal_final_output.len() >= MAX_FINAL_OUTPUT_ENTRIES {
                if let Some(&oldest) = self.terminal_final_output.keys().next() {
                    self.terminal_final_output.remove(&oldest);
                }
            }
            self.terminal_final_output.insert(terminal_id, text);
        }
    }
}

fn full_grid_text(terminal: &Terminal) -> String {
    terminal.with_term(|term| {
        use alacritty_terminal::grid::Dimensions;
        use alacritty_terminal::index::{Column, Line};
        let rows = term.screen_lines();
        let cols = term.columns();
        let mut lines: Vec<String> = (0..rows)
            .map(|row| {
                let mut text = String::new();
                for col in 0..cols {
                    let cell = &term.grid()[Line(row as i32)][Column(col)];
                    text.push(if cell.c == '\0' { ' ' } else { cell.c });
                }
                text.trim_end().to_string()
            })
            .collect();
        while lines.last().is_some_and(|l| l.is_empty()) {
            lines.pop();
        }
        lines.join("\n")
    })
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    /// Pure logic check of the eviction rule `record_terminal_final_
    /// output` uses -- no live `Terminal`/`GpuiShellRoot` needed, matching
    /// this project's own "no PTY in unit tests" convention.
    #[test]
    fn eviction_keeps_map_at_the_cap() {
        const CAP: usize = 3;
        let mut map: HashMap<usize, String> = HashMap::new();
        for i in 0..5 {
            if map.len() >= CAP {
                if let Some(&oldest) = map.keys().next() {
                    map.remove(&oldest);
                }
            }
            map.insert(i, format!("output-{i}"));
        }
        assert_eq!(map.len(), CAP);
    }
}
