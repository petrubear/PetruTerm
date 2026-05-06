use crate::term::Osc133Marker;

/// A completed or in-progress command block detected via OSC 133.
///
/// Rows are stored as "absolute from top of buffer":
///   absolute_row = history_size + grid_cursor_line
/// This value is stable across scrolling. To convert back to a viewport row:
///   viewport_row = absolute_row - history_size + display_offset
#[derive(Debug, Clone)]
pub struct Block {
    pub id: usize,
    /// Row where the prompt started (OSC 133 A).
    pub prompt_row: i64,
    /// Row where command output starts (OSC 133 B or C).
    pub output_start: i64,
    /// Row where output ended (OSC 133 D). None while still streaming.
    pub output_end: Option<i64>,
    pub exit_code: Option<i32>,
    /// Text of the command line captured at CommandStart.
    pub command_text: String,
}

pub struct BlockManager {
    /// Completed blocks, oldest first.
    pub blocks: Vec<Block>,
    /// Block currently accumulating (between A and D).
    current: Option<Block>,
    next_id: usize,
}

impl BlockManager {
    pub fn new() -> Self {
        Self {
            blocks: Vec::new(),
            current: None,
            next_id: 0,
        }
    }

    /// Update block state from an OSC 133 marker.
    /// `absolute_row` = `history_size + cursor_viewport_row - display_offset` at event time.
    /// `command_text` is the text of the current row; only used at CommandStart.
    pub fn on_marker(&mut self, marker: Osc133Marker, absolute_row: i64, command_text: String) {
        match marker {
            Osc133Marker::PromptStart => {
                // Close any uncompleted block from the previous command.
                if let Some(mut prev) = self.current.take() {
                    if prev.output_end.is_none() {
                        prev.output_end = Some(absolute_row);
                    }
                    self.blocks.push(prev);
                }
                self.current = Some(Block {
                    id: self.next_id,
                    prompt_row: absolute_row,
                    output_start: absolute_row,
                    output_end: None,
                    exit_code: None,
                    command_text: String::new(),
                });
                self.next_id += 1;
            }
            Osc133Marker::CommandStart(_) | Osc133Marker::OutputStart => {
                if let Some(ref mut block) = self.current {
                    block.output_start = absolute_row;
                    if !command_text.is_empty() {
                        block.command_text = command_text;
                    }
                }
            }
            Osc133Marker::CommandEnd(exit_code) => {
                if let Some(mut block) = self.current.take() {
                    block.output_end = Some(absolute_row);
                    block.exit_code = Some(exit_code);
                    self.blocks.push(block);
                }
            }
        }
    }

    /// Returns completed blocks whose row range overlaps the current viewport.
    /// `history_size` and `display_offset` are from the terminal grid at render time.
    pub fn blocks_in_viewport(
        &self,
        history_size: usize,
        display_offset: usize,
        rows: usize,
    ) -> Vec<&Block> {
        let h = history_size as i64;
        let d = display_offset as i64;
        let r = rows as i64;
        self.blocks
            .iter()
            .filter(|block| {
                let Some(output_end) = block.output_end else {
                    return false;
                };
                // Convert to viewport rows.
                let start_vp = block.prompt_row - h + d;
                let end_vp = output_end - h + d;
                // Visible if the block's range overlaps [0, rows).
                end_vp >= 0 && start_vp < r
            })
            .collect()
    }

    /// Remove blocks that have scrolled entirely out of the scrollback buffer.
    /// Call periodically to avoid unbounded growth.
    #[allow(dead_code)]
    pub fn evict_old(&mut self, history_size: usize) {
        let h = history_size as i64;
        self.blocks.retain(|b| {
            b.output_end
                .is_none_or(|end| end - h > -(history_size as i64))
        });
    }

    /// Return the completed block whose row range contains `absolute_row`, if any.
    /// Searches newest-first so that when two blocks share a boundary row (D and A
    /// fire at the same cursor position), the newer block wins.
    pub fn block_at_absolute_row(&self, absolute_row: i64) -> Option<&Block> {
        self.blocks.iter().rev().find(|b| {
            let Some(end) = b.output_end else {
                return false;
            };
            absolute_row >= b.prompt_row && absolute_row <= end
        })
    }

    /// Find a completed block by id.
    pub fn find_block_by_id(&self, id: usize) -> Option<&Block> {
        self.blocks.iter().find(|b| b.id == id)
    }


}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::term::Osc133Marker;

    fn mgr() -> BlockManager {
        BlockManager::new()
    }

    #[test]
    fn complete_block_lifecycle() {
        let mut m = mgr();
        m.on_marker(Osc133Marker::PromptStart, 100, String::new());
        m.on_marker(Osc133Marker::CommandStart("ls -la".into()), 100, String::new());
        m.on_marker(Osc133Marker::CommandEnd(0), 115, String::new());
        assert_eq!(m.blocks.len(), 1);
        let b = &m.blocks[0];
        assert_eq!(b.prompt_row, 100);
        assert_eq!(b.output_start, 100);
        assert_eq!(b.output_end, Some(115));
        assert_eq!(b.exit_code, Some(0));
        assert_eq!(b.command_text, "ls -la");
    }

    #[test]
    fn second_prompt_closes_uncompleted_block() {
        let mut m = mgr();
        m.on_marker(Osc133Marker::PromptStart, 50, String::new());
        // No CommandEnd — next prompt arrives
        m.on_marker(Osc133Marker::PromptStart, 80, String::new());
        assert_eq!(m.blocks.len(), 1);
        assert_eq!(m.blocks[0].output_end, Some(80));
        assert!(m.current.is_some());
    }

    #[test]
    fn blocks_in_viewport_filters_correctly() {
        let mut m = mgr();
        // history=100, rows=24 → viewport is [0,24) = absolute [100,124)
        // Block at abs 90-105: overlaps viewport
        m.on_marker(Osc133Marker::PromptStart, 90, String::new());
        m.on_marker(Osc133Marker::CommandEnd(0), 105, String::new());
        // Block at abs 50-60: entirely in history, not visible
        m.on_marker(Osc133Marker::PromptStart, 50, String::new());
        m.on_marker(Osc133Marker::CommandEnd(1), 60, String::new());

        let visible = m.blocks_in_viewport(100, 0, 24);
        assert_eq!(visible.len(), 1);
        assert_eq!(visible[0].prompt_row, 90);
    }

    #[test]
    fn incomplete_block_not_in_viewport() {
        let mut m = mgr();
        m.on_marker(Osc133Marker::PromptStart, 100, String::new());
        m.on_marker(Osc133Marker::CommandStart("cmd".into()), 100, String::new());
        // No CommandEnd
        let visible = m.blocks_in_viewport(90, 0, 24);
        assert_eq!(visible.len(), 0);
    }

    #[test]
    fn exit_code_nonzero() {
        let mut m = mgr();
        m.on_marker(Osc133Marker::PromptStart, 100, String::new());
        m.on_marker(Osc133Marker::CommandEnd(127), 110, String::new());
        assert_eq!(m.blocks[0].exit_code, Some(127));
    }
}
