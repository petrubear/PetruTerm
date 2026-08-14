use crate::renderer::cell::CellVertex;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct RowSlot {
    pub(crate) start: usize,
    pub(crate) capacity: usize,
    pub(crate) len: usize,
    pub(crate) lcd_len: usize,
}

#[derive(Debug, Eq, PartialEq)]
pub(crate) enum RowWriteError {
    InvalidRow,
    CapacityExceeded {
        row: usize,
        capacity: usize,
        actual: usize,
    },
    StorageTooSmall {
        required: usize,
        actual: usize,
    },
    GeometryOverflow,
}

impl std::fmt::Display for RowWriteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidRow => write!(f, "row is outside the terminal layout"),
            Self::CapacityExceeded {
                row,
                capacity,
                actual,
            } => write!(
                f,
                "row {row} has {actual} instances but its slot capacity is {capacity}"
            ),
            Self::StorageTooSmall { required, actual } => {
                write!(
                    f,
                    "terminal storage needs {required} slots but has {actual}"
                )
            }
            Self::GeometryOverflow => write!(f, "terminal row geometry overflows"),
        }
    }
}

impl std::error::Error for RowWriteError {}

#[derive(Debug)]
pub(crate) struct TerminalInstanceLayout {
    #[allow(dead_code)]
    pub(crate) terminal_id: usize,
    pub(crate) columns: usize,
    pub(crate) rows: usize,
    pub(crate) col_offset: usize,
    pub(crate) row_offset: usize,
    pub(crate) row_stride: usize,
    pub(crate) slots: Vec<RowSlot>,
}

impl TerminalInstanceLayout {
    pub(crate) fn rebuild(
        terminal_id: usize,
        columns: usize,
        rows: usize,
        col_offset: usize,
        row_offset: usize,
        row_stride: usize,
    ) -> Self {
        let row_stride = row_stride.max(1);
        let slots = (0..rows)
            .map(|row| RowSlot {
                start: row.saturating_mul(row_stride),
                capacity: row_stride,
                len: 0,
                lcd_len: 0,
            })
            .collect();
        Self {
            terminal_id,
            columns,
            rows,
            col_offset,
            row_offset,
            row_stride,
            slots,
        }
    }

    #[allow(dead_code)]
    pub(crate) fn row_slot(&self, row: usize) -> Option<RowSlot> {
        self.slots.get(row).copied()
    }

    pub(crate) fn storage_len(&self) -> usize {
        self.rows.saturating_mul(self.row_stride)
    }

    #[allow(dead_code)]
    pub(crate) fn matches_geometry(
        &self,
        columns: usize,
        rows: usize,
        col_offset: usize,
        row_offset: usize,
        row_stride: usize,
    ) -> bool {
        self.columns == columns
            && self.rows == rows
            && self.col_offset == col_offset
            && self.row_offset == row_offset
            && self.row_stride >= row_stride.max(1)
    }

    pub(crate) fn set_base(&mut self, base: usize) {
        for (row, slot) in self.slots.iter_mut().enumerate() {
            slot.start = base.saturating_add(row.saturating_mul(self.row_stride));
        }
    }

    pub(crate) fn write_row(
        &mut self,
        row: usize,
        vertices: &[CellVertex],
        storage: &mut [CellVertex],
    ) -> Result<RowSlot, RowWriteError> {
        self.write_row_internal(row, vertices, storage, true)
    }

    pub(crate) fn write_lcd_row(
        &mut self,
        row: usize,
        vertices: &[CellVertex],
        storage: &mut [CellVertex],
    ) -> Result<RowSlot, RowWriteError> {
        self.write_row_internal(row, vertices, storage, false)
    }

    fn write_row_internal(
        &mut self,
        row: usize,
        vertices: &[CellVertex],
        storage: &mut [CellVertex],
        update_len: bool,
    ) -> Result<RowSlot, RowWriteError> {
        let Some(slot) = self.slots.get_mut(row) else {
            return Err(RowWriteError::InvalidRow);
        };
        if vertices.len() > slot.capacity {
            return Err(RowWriteError::CapacityExceeded {
                row,
                capacity: slot.capacity,
                actual: vertices.len(),
            });
        }
        let required = slot.start.saturating_add(slot.capacity);
        if required > storage.len() {
            return Err(RowWriteError::StorageTooSmall {
                required,
                actual: storage.len(),
            });
        }

        let global_row = self
            .row_offset
            .checked_add(row)
            .ok_or(RowWriteError::GeometryOverflow)?;
        for (index, vertex) in vertices.iter().enumerate() {
            let mut global = *vertex;
            global.grid_pos[0] += self.col_offset as f32;
            global.grid_pos[1] += global_row as f32;
            storage[slot.start + index] = global;
        }
        for index in vertices.len()..slot.capacity {
            storage[slot.start + index] = bytemuck::Zeroable::zeroed();
        }
        if update_len {
            slot.len = vertices.len();
        } else {
            slot.lcd_len = vertices.len();
        }
        Ok(*slot)
    }
}

/// Compare the complete contents of persistent row storage.
///
/// The comparison is deliberately slot-based: row slots include transparent
/// padding, which is part of the observable GPU state even when a row has fewer
/// visible instances than its capacity.
#[allow(dead_code)]
pub(crate) fn row_storage_equivalent(
    full: &[CellVertex],
    incremental: &[CellVertex],
    slots: &[RowSlot],
) -> bool {
    if full.len() != incremental.len() {
        return false;
    }
    slots.iter().all(|slot| {
        let Some(end) = slot.start.checked_add(slot.capacity) else {
            return false;
        };
        end <= full.len()
            && end <= incremental.len()
            && bytemuck::cast_slice::<CellVertex, u8>(&full[slot.start..end])
                == bytemuck::cast_slice::<CellVertex, u8>(&incremental[slot.start..end])
    })
}

#[cfg(test)]
mod tests {
    use super::{row_storage_equivalent, TerminalInstanceLayout};
    use crate::renderer::cell::CellVertex;
    use bytemuck::Zeroable;

    #[test]
    fn row_slots_are_non_overlapping_and_bounded() {
        let layout = TerminalInstanceLayout::rebuild(7, 80, 24, 0, 0, 160);
        let first = layout.row_slot(0).unwrap();
        let second = layout.row_slot(1).unwrap();

        assert_eq!(first.start + first.capacity, second.start);
        assert_eq!(layout.rows, 24);
    }

    #[test]
    fn write_row_offsets_vertices_and_clears_padding() {
        let mut layout = TerminalInstanceLayout::rebuild(7, 4, 2, 3, 5, 4);
        let local = CellVertex {
            grid_pos: [1.0, 0.0],
            ..CellVertex::zeroed()
        };
        let mut storage = vec![
            CellVertex {
                bg: [1.0; 4],
                ..CellVertex::zeroed()
            };
            layout.storage_len()
        ];

        let slot = layout.write_row(0, &[local], &mut storage).unwrap();

        assert_eq!(slot.len, 1);
        assert_eq!(storage[slot.start].grid_pos, [4.0, 5.0]);
        assert_eq!(storage[slot.start + 1].grid_pos, [0.0, 0.0]);
        assert_eq!(storage[slot.start + 1].glyph_size, [0.0, 0.0]);
    }

    #[test]
    fn layout_geometry_changes_require_rebuild() {
        let layout = TerminalInstanceLayout::rebuild(7, 80, 24, 0, 0, 160);

        assert!(layout.matches_geometry(80, 24, 0, 0, 160));
        assert!(layout.matches_geometry(80, 24, 0, 0, 80));
        assert!(!layout.matches_geometry(81, 24, 0, 0, 160));
        assert!(!layout.matches_geometry(80, 24, 1, 0, 160));
    }

    #[test]
    fn lcd_slots_track_their_own_lengths() {
        let mut layout = TerminalInstanceLayout::rebuild(7, 4, 1, 0, 0, 4);
        let mut storage = vec![CellVertex::zeroed(); layout.storage_len()];

        let slot = layout
            .write_lcd_row(0, &[CellVertex::zeroed()], &mut storage)
            .unwrap();

        assert_eq!(slot.len, 0);
        assert_eq!(slot.lcd_len, 1);
        assert_eq!(layout.row_slot(0).unwrap().lcd_len, 1);
    }

    #[test]
    fn full_and_incremental_row_storage_are_equivalent() {
        let mut full_layout = TerminalInstanceLayout::rebuild(7, 4, 4, 2, 3, 4);
        let mut incremental_layout = TerminalInstanceLayout::rebuild(7, 4, 4, 2, 3, 4);
        let mut full_terminal = vec![CellVertex::zeroed(); full_layout.storage_len()];
        let mut full_lcd = vec![CellVertex::zeroed(); full_layout.storage_len()];
        let mut incremental_terminal = vec![CellVertex::zeroed(); incremental_layout.storage_len()];
        let mut incremental_lcd = vec![CellVertex::zeroed(); incremental_layout.storage_len()];

        let old_rows = |row: usize| {
            (0..row % 3 + 1)
                .map(|col| CellVertex {
                    grid_pos: [col as f32, row as f32],
                    glyph_size: [1.0 + row as f32, 1.0],
                    ..CellVertex::zeroed()
                })
                .collect::<Vec<_>>()
        };
        let updated_rows = |row: usize| {
            (0..3)
                .map(|col| CellVertex {
                    grid_pos: [col as f32, row as f32],
                    glyph_size: [10.0 + row as f32, 2.0],
                    ..CellVertex::zeroed()
                })
                .collect::<Vec<_>>()
        };

        // Establish the old incremental frame once. Clean rows must survive this
        // frame while the full reference is rebuilt from the updated terminal.
        for row in 0..4 {
            let vertices = old_rows(row);
            incremental_layout
                .write_row(row, &vertices, &mut incremental_terminal)
                .unwrap();
            incremental_layout
                .write_lcd_row(row, &vertices, &mut incremental_lcd)
                .unwrap();
            incremental_layout
                .write_lcd_row(row, &vertices, &mut incremental_lcd)
                .unwrap();
        }

        // The full path rebuilds every row, while the incremental path only
        // rewrites the two damaged rows through the production slot writers.
        for row in 0..4 {
            let vertices = if row == 1 || row == 3 {
                updated_rows(row)
            } else {
                old_rows(row)
            };
            full_layout
                .write_row(row, &vertices, &mut full_terminal)
                .unwrap();
            full_layout
                .write_lcd_row(row, &vertices, &mut full_lcd)
                .unwrap();
        }
        for row in [1, 3] {
            incremental_layout
                .write_row(row, &updated_rows(row), &mut incremental_terminal)
                .unwrap();
            incremental_layout
                .write_lcd_row(row, &updated_rows(row), &mut incremental_lcd)
                .unwrap();
        }

        assert!(row_storage_equivalent(
            &full_terminal,
            &incremental_terminal,
            &full_layout.slots
        ));
        assert!(row_storage_equivalent(
            &full_lcd,
            &incremental_lcd,
            &full_layout.slots
        ));
        assert_eq!(
            full_layout.slots.iter().map(|slot| slot.len).sum::<usize>(),
            incremental_layout
                .slots
                .iter()
                .map(|slot| slot.len)
                .sum::<usize>()
        );
        assert_eq!(
            full_layout
                .slots
                .iter()
                .map(|slot| slot.lcd_len)
                .sum::<usize>(),
            incremental_layout
                .slots
                .iter()
                .map(|slot| slot.lcd_len)
                .sum::<usize>()
        );
        for slot in &incremental_layout.slots {
            let terminal_padding =
                &incremental_terminal[slot.start + slot.len..slot.start + slot.capacity];
            let lcd_padding =
                &incremental_lcd[slot.start + slot.lcd_len..slot.start + slot.capacity];
            assert!(terminal_padding.iter().all(|vertex| {
                bytemuck::bytes_of(vertex) == bytemuck::bytes_of(&CellVertex::zeroed())
            }));
            assert!(lcd_padding.iter().all(|vertex| {
                bytemuck::bytes_of(vertex) == bytemuck::bytes_of(&CellVertex::zeroed())
            }));
        }
    }
}
