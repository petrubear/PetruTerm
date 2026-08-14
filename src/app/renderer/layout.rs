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

#[cfg(test)]
mod tests {
    use super::TerminalInstanceLayout;
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
}
