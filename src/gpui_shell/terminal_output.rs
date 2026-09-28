// Per-terminal exit-code tracking and final-output caching, so ACP's
// `terminal/output` and `terminal/wait_for_exit` can answer after a pane
// has closed. Used by `acp_bridge.rs`.

use crate::term::Terminal;

use super::GpuiShellRoot;

/// Cap on `terminal_exit_codes`/`terminal_final_output`'s size, shared via
/// `closed_terminal_order` (mirrors `Mux::MAX_CLOSED_TERMINALS`). Past it,
/// the oldest closed terminal's entries are evicted from both maps.
const MAX_CLOSED_TERMINALS: usize = 64;

impl GpuiShellRoot {
    /// Every visible row of `terminal_id`'s grid if it's still alive,
    /// trimmed of trailing empty lines; the cached final output if it has
    /// already been reaped; empty string if neither.
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
        self.retain_closed_terminal(terminal_id);
    }

    /// Capture `terminal_id`'s final grid text just before its last
    /// `Rc<Terminal>` is dropped. Called from `actions.rs`'s own pane-
    /// removal sites, right before `self.terminals.remove(...)`.
    pub(super) fn record_terminal_final_output(&mut self, terminal_id: usize) {
        if let Some(terminal) = self.terminals.get(&terminal_id) {
            let text = full_grid_text(terminal);
            self.terminal_final_output.insert(terminal_id, text);
            self.retain_closed_terminal(terminal_id);
        }
    }

    /// Register a closed terminal id for bounded FIFO retention of its exit
    /// code/output, evicting the oldest once `MAX_CLOSED_TERMINALS` is
    /// exceeded. Mirrors `Mux::retain_closed_terminal`. `record_terminal_
    /// exit_code` and `record_terminal_final_output` fire independently
    /// (from `poll.rs` and `actions.rs` respectively), so this only pushes
    /// `terminal_id` once, whichever call reaches it first.
    fn retain_closed_terminal(&mut self, terminal_id: usize) {
        evict_oldest_closed_terminals(
            &mut self.closed_terminal_order,
            &mut self.terminal_exit_codes,
            &mut self.terminal_final_output,
            terminal_id,
        );
    }
}

/// Pure FIFO-eviction step used by `GpuiShellRoot::retain_closed_terminal`,
/// factored out so it's testable without constructing a live `GpuiShellRoot`.
fn evict_oldest_closed_terminals(
    closed_terminal_order: &mut std::collections::VecDeque<usize>,
    terminal_exit_codes: &mut std::collections::HashMap<usize, i32>,
    terminal_final_output: &mut std::collections::HashMap<usize, String>,
    terminal_id: usize,
) {
    if !closed_terminal_order.contains(&terminal_id) {
        closed_terminal_order.push_back(terminal_id);
    }
    while closed_terminal_order.len() > MAX_CLOSED_TERMINALS {
        if let Some(oldest) = closed_terminal_order.pop_front() {
            terminal_exit_codes.remove(&oldest);
            terminal_final_output.remove(&oldest);
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
    use super::evict_oldest_closed_terminals;
    use std::collections::{HashMap, VecDeque};

    /// Exercises the real eviction function -- no live `Terminal`/
    /// `GpuiShellRoot` needed, matching this project's own "no PTY in unit
    /// tests" convention.
    #[test]
    fn eviction_keeps_maps_at_the_cap() {
        let mut order: VecDeque<usize> = VecDeque::new();
        let mut exit_codes: HashMap<usize, i32> = HashMap::new();
        let mut final_output: HashMap<usize, String> = HashMap::new();

        for i in 0..super::MAX_CLOSED_TERMINALS + 2 {
            exit_codes.insert(i, i as i32);
            final_output.insert(i, format!("output-{i}"));
            evict_oldest_closed_terminals(&mut order, &mut exit_codes, &mut final_output, i);
        }

        assert_eq!(order.len(), super::MAX_CLOSED_TERMINALS);
        assert_eq!(exit_codes.len(), super::MAX_CLOSED_TERMINALS);
        assert_eq!(final_output.len(), super::MAX_CLOSED_TERMINALS);
    }

    /// Regression check for AUDIT-BUG-08: eviction must drop the oldest
    /// entry, not an arbitrary one picked by `HashMap` iteration order.
    #[test]
    fn eviction_drops_the_oldest_entry_first() {
        let mut order: VecDeque<usize> = VecDeque::new();
        let mut exit_codes: HashMap<usize, i32> = HashMap::new();
        let mut final_output: HashMap<usize, String> = HashMap::new();

        for i in 0..super::MAX_CLOSED_TERMINALS {
            exit_codes.insert(i, i as i32);
            final_output.insert(i, format!("output-{i}"));
            evict_oldest_closed_terminals(&mut order, &mut exit_codes, &mut final_output, i);
        }
        // One more closed terminal past the cap: id 0 (the oldest) must be
        // the one evicted, and every id in between must survive untouched.
        let new_id = super::MAX_CLOSED_TERMINALS;
        exit_codes.insert(new_id, new_id as i32);
        final_output.insert(new_id, format!("output-{new_id}"));
        evict_oldest_closed_terminals(&mut order, &mut exit_codes, &mut final_output, new_id);

        assert!(!exit_codes.contains_key(&0));
        assert!(!final_output.contains_key(&0));
        for i in 1..=new_id {
            assert!(exit_codes.contains_key(&i));
            assert!(final_output.contains_key(&i));
        }
    }

    /// Recording the same terminal id twice (once from `poll.rs`'s exit-code
    /// path, once from `actions.rs`'s final-output path) must not push a
    /// duplicate into the FIFO order or double-count against the cap.
    #[test]
    fn recording_the_same_terminal_twice_does_not_duplicate_the_order_entry() {
        let mut order: VecDeque<usize> = VecDeque::new();
        let mut exit_codes: HashMap<usize, i32> = HashMap::new();
        let mut final_output: HashMap<usize, String> = HashMap::new();

        evict_oldest_closed_terminals(&mut order, &mut exit_codes, &mut final_output, 42);
        evict_oldest_closed_terminals(&mut order, &mut exit_codes, &mut final_output, 42);

        assert_eq!(order.len(), 1);
    }
}
