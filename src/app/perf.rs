#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum FrameScenario {
    #[default]
    Idle,
    Interactive,
    PtyOutput,
    Scroll,
    Resize,
    MultiPane,
}

impl FrameScenario {
    pub(crate) const fn label(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Interactive => "interactive",
            Self::PtyOutput => "pty_output",
            Self::Scroll => "scroll",
            Self::Resize => "resize",
            Self::MultiPane => "multi_pane",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct FrameMetrics {
    pub scenario: FrameScenario,
    pub pty_terminals: usize,
    pub dirty_rows: usize,
    pub rebuilt_rows: usize,
    pub upload_ranges: usize,
    pub upload_bytes: usize,
    pub full_upload_ranges: usize,
    pub full_upload_bytes: usize,
    pub incremental_upload_ranges: usize,
    pub incremental_upload_bytes: usize,
    pub wakeups: usize,
    pub redraws: usize,
}

impl FrameMetrics {
    pub(crate) fn reset(&mut self) {
        *self = Self::default();
    }

    pub(crate) fn record_upload(&mut self, bytes: usize, ranges: usize) {
        self.upload_bytes = self.upload_bytes.saturating_add(bytes);
        self.upload_ranges = self.upload_ranges.saturating_add(ranges);
    }

    pub(crate) fn record_terminal_uploads(
        &mut self,
        full_bytes: usize,
        full_ranges: usize,
        incremental_bytes: usize,
        incremental_ranges: usize,
    ) {
        self.full_upload_bytes = self.full_upload_bytes.saturating_add(full_bytes);
        self.full_upload_ranges = self.full_upload_ranges.saturating_add(full_ranges);
        self.incremental_upload_bytes = self
            .incremental_upload_bytes
            .saturating_add(incremental_bytes);
        self.incremental_upload_ranges = self
            .incremental_upload_ranges
            .saturating_add(incremental_ranges);
    }
}

#[cfg(test)]
mod tests {
    use super::FrameMetrics;

    #[test]
    fn upload_metrics_accumulate_and_reset() {
        let mut metrics = FrameMetrics::default();
        metrics.record_upload(128, 2);
        metrics.record_upload(64, 1);
        metrics.record_terminal_uploads(1024, 2, 192, 3);

        assert_eq!(metrics.upload_bytes, 192);
        assert_eq!(metrics.upload_ranges, 3);
        assert_eq!(metrics.full_upload_bytes, 1024);
        assert_eq!(metrics.full_upload_ranges, 2);
        assert_eq!(metrics.incremental_upload_bytes, 192);
        assert_eq!(metrics.incremental_upload_ranges, 3);

        metrics.reset();
        assert_eq!(metrics, FrameMetrics::default());
    }

    #[test]
    fn scenario_labels_are_stable_debug_labels() {
        assert_eq!(FrameScenario::Idle.label(), "idle");
        assert_eq!(FrameScenario::PtyOutput.label(), "pty_output");
        assert_eq!(FrameScenario::MultiPane.label(), "multi_pane");
    }
}
