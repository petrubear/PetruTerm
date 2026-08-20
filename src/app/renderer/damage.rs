// Row-range coalescing already lives in `merge_upload_ranges`
// (src/renderer/upload.rs), which is what the live GPU upload path actually
// uses. DirtyRows only needs O(1) membership tracking, not a second
// sort-and-coalesce implementation.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct DirtyRows {
    rows: std::collections::HashSet<usize>,
    full: bool,
    full_count: usize,
}

impl DirtyRows {
    pub(crate) fn mark(&mut self, row: usize) {
        if !self.full {
            self.rows.insert(row);
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
    SurfaceReconfiguration,
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

#[derive(Debug, Default)]
pub(crate) struct RenderBuildState {
    pending_full_rebuild: Option<FullRebuildTrigger>,
    pending_terminal_rebuilds: std::collections::HashMap<usize, FullRebuildTrigger>,
}

impl RenderBuildState {
    /// Record a full rebuild request for the next production terminal build.
    pub(crate) fn request_full_rebuild(&mut self, trigger: FullRebuildTrigger) {
        self.pending_full_rebuild = Some(trigger);
    }

    /// Record a full rebuild request deferred to one production terminal build.
    pub(crate) fn request_terminal_full_rebuild(
        &mut self,
        terminal_id: usize,
        trigger: FullRebuildTrigger,
    ) {
        self.pending_terminal_rebuilds.insert(terminal_id, trigger);
    }

    pub(crate) fn clear_terminal_rebuild(&mut self, terminal_id: usize) {
        self.pending_terminal_rebuilds.remove(&terminal_id);
    }

    pub(crate) fn clear_all_rebuilds(&mut self) {
        self.pending_full_rebuild = None;
        self.pending_terminal_rebuilds.clear();
    }

    /// Resolve the damage consumed directly by `RenderContext::build_instances`.
    ///
    /// A pending whole-frame trigger is only *peeked* here, not consumed: a
    /// frame with multiple terminals (split panes, multiple tabs' hidden
    /// panes) must apply it to every terminal it builds this frame, not just
    /// the first one processed. Call `clear_pending_full_rebuild` once after
    /// every terminal for the frame has gone through this method.
    pub(crate) fn resolve_terminal_build(
        &mut self,
        terminal_id: usize,
        dirty_rows: &DirtyRows,
        row_count: usize,
        cache_complete: bool,
    ) -> BuildDamage {
        let invalidation = self
            .pending_terminal_rebuilds
            .remove(&terminal_id)
            .or(self.pending_full_rebuild);
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

    /// Clear the pending whole-frame rebuild trigger. Call once per frame,
    /// after every terminal built that frame has consumed it via
    /// `resolve_terminal_build`. Per-terminal triggers are untouched — they
    /// stay pending for a terminal that wasn't built this frame (e.g. a
    /// hidden pane) until it actually gets built.
    pub(crate) fn clear_pending_full_rebuild(&mut self) {
        self.pending_full_rebuild = None;
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
    use super::{DirtyRows, FullRebuildTrigger, RenderBuildState, RowRevisionMap};

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
            FullRebuildTrigger::SurfaceReconfiguration,
        ];

        for trigger in triggers {
            let mut state = RenderBuildState::default();
            state.request_full_rebuild(trigger);
            let damage = state.resolve_terminal_build(0, &DirtyRows::default(), 6, true);
            assert!(damage.full_rebuild);
            assert_eq!(damage.rows.len(), 6);
            assert!((0..6).all(|row| damage.rows.is_dirty(row)));
            // Still pending within the same frame — a second terminal built
            // before the frame ends must see it too.
            let same_frame = state.resolve_terminal_build(1, &DirtyRows::default(), 6, true);
            assert!(same_frame.full_rebuild);
            state.clear_pending_full_rebuild();
            let next = state.resolve_terminal_build(0, &DirtyRows::default(), 6, true);
            assert!(!next.full_rebuild);
        }
    }

    #[test]
    fn pending_full_rebuild_applies_to_every_terminal_built_in_the_frame() {
        let mut state = RenderBuildState::default();
        state.request_full_rebuild(FullRebuildTrigger::ThemeColorChange);

        // A frame with a split pane builds terminal 0 then terminal 1 before
        // the frame ends. Both must see the pending full rebuild — not just
        // whichever terminal happened to be built first.
        let first = state.resolve_terminal_build(0, &DirtyRows::default(), 4, true);
        let second = state.resolve_terminal_build(1, &DirtyRows::default(), 4, true);
        assert!(first.full_rebuild);
        assert!(second.full_rebuild);

        state.clear_pending_full_rebuild();
        let after_frame = state.resolve_terminal_build(0, &DirtyRows::default(), 4, true);
        assert!(!after_frame.full_rebuild);
    }

    #[test]
    fn production_build_contract_falls_back_for_missing_cache_and_capacity_overflow() {
        let dirty = DirtyRows::default();
        let mut state = RenderBuildState::default();
        let missing = state.resolve_terminal_build(0, &dirty, 5, false);
        assert!(missing.full_rebuild);
        assert!((0..5).all(|row| missing.rows.is_dirty(row)));

        state.request_terminal_full_rebuild(0, FullRebuildTrigger::RowSlotCapacityOverflow);
        let overflow = state.resolve_terminal_build(0, &dirty, 5, true);
        assert!(overflow.full_rebuild);
        assert!((0..5).all(|row| overflow.rows.is_dirty(row)));
    }

    #[test]
    fn capacity_overflow_is_deferred_to_the_originating_terminal() {
        let dirty = DirtyRows::default();
        let mut state = RenderBuildState::default();
        state.request_terminal_full_rebuild(7, FullRebuildTrigger::RowSlotCapacityOverflow);

        let other = state.resolve_terminal_build(3, &dirty, 4, true);
        assert!(!other.full_rebuild);
        assert_eq!(other.rows.len(), 0);

        let originating = state.resolve_terminal_build(7, &dirty, 4, true);
        assert!(originating.full_rebuild);
        assert!((0..4).all(|row| originating.rows.is_dirty(row)));

        let next = state.resolve_terminal_build(7, &dirty, 4, true);
        assert!(!next.full_rebuild);
    }

    #[test]
    fn deferred_terminal_rebuilds_are_removed_when_state_is_cleared() {
        let mut state = RenderBuildState::default();
        state.request_terminal_full_rebuild(7, FullRebuildTrigger::RowSlotCapacityOverflow);
        state.clear_terminal_rebuild(7);
        let damage = state.resolve_terminal_build(7, &DirtyRows::default(), 4, true);
        assert!(!damage.full_rebuild);

        state.request_full_rebuild(FullRebuildTrigger::SurfaceReconfiguration);
        state.request_terminal_full_rebuild(7, FullRebuildTrigger::RowSlotCapacityOverflow);
        state.clear_all_rebuilds();
        let damage = state.resolve_terminal_build(7, &DirtyRows::default(), 4, true);
        assert!(!damage.full_rebuild);
    }
}
