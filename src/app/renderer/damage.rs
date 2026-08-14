#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[allow(dead_code)]
pub(crate) struct RowRange {
    pub(crate) start: usize,
    pub(crate) end: usize,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct DirtyRows {
    rows: Vec<usize>,
    full: bool,
    full_count: usize,
}

impl DirtyRows {
    pub(crate) fn mark(&mut self, row: usize) {
        if !self.full && !self.rows.contains(&row) {
            self.rows.push(row);
        }
    }

    #[allow(dead_code)]
    pub(crate) fn mark_range(&mut self, start: usize, end: usize) {
        for row in start..end {
            self.mark(row);
        }
    }

    pub(crate) fn mark_all(&mut self, row_count: usize) {
        self.rows.clear();
        self.full = true;
        self.full_count = row_count;
    }

    pub(crate) fn is_dirty(&self, row: usize) -> bool {
        self.full || self.rows.contains(&row)
    }

    #[allow(dead_code)]
    pub(crate) fn is_full(&self) -> bool {
        self.full
    }

    pub(crate) fn len(&self) -> usize {
        if self.full {
            self.full_count
        } else {
            self.rows.len()
        }
    }

    #[allow(dead_code)]
    pub(crate) fn ranges(&self, row_count: usize) -> Vec<RowRange> {
        if self.full {
            return if row_count == 0 {
                Vec::new()
            } else {
                vec![RowRange {
                    start: 0,
                    end: row_count,
                }]
            };
        }

        let mut rows: Vec<usize> = self
            .rows
            .iter()
            .copied()
            .filter(|row| *row < row_count)
            .collect();
        rows.sort_unstable();
        rows.dedup();

        let mut ranges: Vec<RowRange> = Vec::new();
        for row in rows {
            if let Some(last) = ranges.last_mut() {
                if row <= last.end {
                    last.end = last.end.max(row + 1);
                    continue;
                }
            }
            ranges.push(RowRange {
                start: row,
                end: row + 1,
            });
        }
        ranges
    }

    pub(crate) fn clear(&mut self) {
        self.rows.clear();
        self.full = false;
        self.full_count = 0;
    }

    pub(crate) fn full_rebuild(row_count: usize) -> Self {
        let mut rows = Self::default();
        rows.mark_all(row_count);
        rows
    }
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum FullRebuildTrigger {
    TerminalResize,
    PaneGeometryChange,
    FontMetricRefresh,
    ThemeColorChange,
    AtlasGenerationChange,
    MissingRowCache,
    RowSlotCapacityOverflow,
    InvalidGpuUploadRange,
}

#[allow(dead_code)]
pub(crate) fn rows_for_full_rebuild(_trigger: FullRebuildTrigger, row_count: usize) -> DirtyRows {
    DirtyRows::full_rebuild(row_count)
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct BuildDamage {
    pub(crate) full_rebuild: bool,
    pub(crate) rows: DirtyRows,
}

pub(crate) fn request_full_rebuild(
    pending: &mut Option<FullRebuildTrigger>,
    trigger: FullRebuildTrigger,
) {
    *pending = Some(trigger);
}

pub(crate) fn take_build_damage(
    pending: &mut Option<FullRebuildTrigger>,
    capacity_overflow: Option<FullRebuildTrigger>,
    dirty_rows: &DirtyRows,
    row_count: usize,
    cache_complete: bool,
) -> BuildDamage {
    let invalidation = capacity_overflow.or_else(|| pending.take());
    let rows = if let Some(trigger) = invalidation {
        rows_for_full_rebuild(trigger, row_count)
    } else if cache_complete {
        dirty_rows.clone()
    } else {
        rows_for_full_rebuild(FullRebuildTrigger::MissingRowCache, row_count)
    };
    BuildDamage {
        full_rebuild: rows.is_full(),
        rows,
    }
}

#[derive(Debug, Default)]
pub(crate) struct RowRevisionMap {
    revisions: Vec<u64>,
    next: u64,
}

impl RowRevisionMap {
    pub(crate) fn mark(&mut self, row: usize) {
        self.ensure_len(row + 1);
        self.next = self.next.wrapping_add(1);
        self.revisions[row] = self.next;
    }

    #[allow(dead_code)]
    pub(crate) fn mark_all(&mut self, row_count: usize) {
        self.ensure_len(row_count);
        for row in 0..row_count {
            self.next = self.next.wrapping_add(1);
            self.revisions[row] = self.next;
        }
    }

    pub(crate) fn revision(&self, row: usize) -> u64 {
        self.revisions.get(row).copied().unwrap_or(0)
    }

    fn ensure_len(&mut self, len: usize) {
        if self.revisions.len() < len {
            self.revisions.resize(len, 0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        request_full_rebuild, take_build_damage, DirtyRows, FullRebuildTrigger, RowRange,
        RowRevisionMap,
    };

    #[test]
    fn dirty_rows_merge_and_sort_ranges() {
        let mut rows = DirtyRows::default();
        rows.mark(5);
        rows.mark_range(1, 3);
        rows.mark(3);
        rows.mark(4);

        assert_eq!(rows.ranges(8), vec![RowRange { start: 1, end: 6 }]);
    }

    #[test]
    fn full_damage_covers_requested_rows() {
        let mut rows = DirtyRows::default();
        rows.mark_all(4);

        assert!(rows.is_full());
        assert!((0..4).all(|row| rows.is_dirty(row)));
    }

    #[test]
    fn row_revisions_increase_only_for_marked_rows() {
        let mut revisions = RowRevisionMap::default();
        revisions.mark(2);
        let first = revisions.revision(2);
        revisions.mark(4);

        assert_eq!(revisions.revision(2), first);
        assert!(revisions.revision(4) > first);
    }

    #[test]
    fn production_build_contract_consumes_every_full_rebuild_trigger() {
        let triggers = [
            FullRebuildTrigger::TerminalResize,
            FullRebuildTrigger::PaneGeometryChange,
            FullRebuildTrigger::FontMetricRefresh,
            FullRebuildTrigger::ThemeColorChange,
            FullRebuildTrigger::AtlasGenerationChange,
            FullRebuildTrigger::MissingRowCache,
            FullRebuildTrigger::RowSlotCapacityOverflow,
            FullRebuildTrigger::InvalidGpuUploadRange,
        ];

        for trigger in triggers {
            let mut pending = None;
            request_full_rebuild(&mut pending, trigger);
            let damage = take_build_damage(&mut pending, None, &DirtyRows::default(), 6, true);
            assert!(damage.full_rebuild);
            assert!(pending.is_none());
            assert_eq!(damage.rows.len(), 6);
            assert!((0..6).all(|row| damage.rows.is_dirty(row)));
        }
    }

    #[test]
    fn production_build_contract_falls_back_for_missing_cache_and_capacity_overflow() {
        let dirty = DirtyRows::default();
        let missing = take_build_damage(&mut None, None, &dirty, 5, false);
        assert!(missing.full_rebuild);
        assert!((0..5).all(|row| missing.rows.is_dirty(row)));

        let overflow = take_build_damage(
            &mut None,
            Some(FullRebuildTrigger::RowSlotCapacityOverflow),
            &dirty,
            5,
            true,
        );
        assert!(overflow.full_rebuild);
        assert!((0..5).all(|row| overflow.rows.is_dirty(row)));
    }
}
