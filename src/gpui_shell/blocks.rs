// gpui chrome migration (M5c Task 1): command-block state and the
// output-text helper. `crate::term::{Block, BlockManager}` (src/term/
// blocks.rs) is already engine-agnostic -- ported from Mux's own
// block_output_text (src/app/mux/mod.rs:583-604) with the Mux lookup
// dropped in favor of taking a `&Terminal` directly.
//
// `GpuiShellRoot`'s own `block_managers` sidecar (mod.rs) exists because
// `Terminal.block_manager` itself has no interior mutability -- see this
// plan's own Global Constraints for why reusing that field directly
// doesn't compile through `Rc<Terminal>`.

use crate::term::{BlockManager, Terminal};

use super::GpuiShellRoot;

impl GpuiShellRoot {
    /// The captured output text of one completed command block, or `None`
    /// if the block doesn't exist or is still streaming (`output_end` is
    /// `None`).
    pub(super) fn block_output_text(&self, terminal_id: usize, block_id: usize) -> Option<String> {
        let terminal = self.terminals.get(&terminal_id)?;
        let manager = self.block_managers.get(&terminal_id)?;
        block_output_text_for(terminal, manager, block_id)
    }
}

fn block_output_text_for(
    terminal: &Terminal,
    manager: &BlockManager,
    block_id: usize,
) -> Option<String> {
    let block = manager.find_block_by_id(block_id)?;
    let output_end = block.output_end?;
    let output_start = block.output_start;

    Some(terminal.with_term(|term| {
        use alacritty_terminal::grid::Dimensions;
        use alacritty_terminal::index::{Column, Line};
        let history_size = term.grid().history_size() as i64;
        let cols = term.columns();
        let mut lines = Vec::new();

        for abs_row in output_start..=output_end {
            let grid_idx = (abs_row - history_size) as i32;
            let mut text = String::new();
            for col in 0..cols {
                let cell = &term.grid()[Line(grid_idx)][Column(col)];
                text.push(if cell.c == '\0' { ' ' } else { cell.c });
            }
            lines.push(text.trim_end().to_string());
        }
        lines.join("\n")
    }))
}

/// One grid read serving both this task's own block-detection and Task
/// 3's link detection: `row`'s visible text (ported from `Mux::viewport_
/// row_text`, `src/app/mux/mod.rs:558-578`, adapted to take `&Terminal`
/// directly) AND that row's "absolute row from top of buffer" (the same
/// coordinate space `Block::prompt_row`/`output_start`/`output_end` use,
/// per `src/term/blocks.rs`'s own doc comment: `absolute_row = history_
/// size + grid_cursor_line`, solved here for an arbitrary clicked `row`
/// instead of the cursor's own line).
pub(super) fn row_text_and_absolute_row(terminal: &Terminal, row: usize) -> (String, i64) {
    terminal.with_term(|term| {
        use alacritty_terminal::grid::Dimensions;
        use alacritty_terminal::index::{Column, Line};
        let cols = term.columns();
        let screen_rows = term.screen_lines() as i32;
        let display_offset = term.grid().display_offset() as i32;
        let history_size = term.grid().history_size() as i64;
        let grid_line = row as i32 - display_offset;
        let absolute_row = history_size + grid_line as i64;
        // Negative `grid_line` is a valid, reachable case (a visible row
        // showing scrolled-back history, per `Line`'s own negative-index
        // support into `Grid`'s history) -- only the upper bound is a real
        // out-of-range guard, matching `viewport_row_text`'s own real
        // guard exactly (`src/app/mux/mod.rs:569`).
        if grid_line >= screen_rows {
            return (String::new(), absolute_row);
        }
        let mut text = String::with_capacity(cols);
        for col in 0..cols {
            let cell = &term.grid()[Line(grid_line)][Column(col)];
            text.push(if cell.c == '\0' { ' ' } else { cell.c });
        }
        (text.trim_end().to_string(), absolute_row)
    })
}
