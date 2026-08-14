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

pub(crate) fn classify_frame_scenario(
    requested: FrameScenario,
    active_panes: usize,
    has_data: bool,
    data_events: usize,
    data_bytes: usize,
    pending_events: usize,
) -> FrameScenario {
    if matches!(requested, FrameScenario::Resize | FrameScenario::Scroll) {
        return requested;
    }
    if active_panes > 1 {
        return FrameScenario::MultiPane;
    }
    if has_data {
        if data_events > 2 || data_bytes >= 4096 || pending_events > 0 {
            FrameScenario::PtyOutput
        } else {
            FrameScenario::Interactive
        }
    } else {
        requested
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct FrameMetrics {
    pub scenario: FrameScenario,
    pub pty_terminals: usize,
    pub pty_events: usize,
    pub pty_bytes: usize,
    pub pty_pending_events: usize,
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
    pub event_loop_iterations: usize,
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
    use super::{classify_frame_scenario, FrameMetrics, FrameScenario};

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
        let labels = [
            (FrameScenario::Idle, "idle"),
            (FrameScenario::Interactive, "interactive"),
            (FrameScenario::PtyOutput, "pty_output"),
            (FrameScenario::Scroll, "scroll"),
            (FrameScenario::Resize, "resize"),
            (FrameScenario::MultiPane, "multi_pane"),
        ];
        for (scenario, label) in labels {
            assert_eq!(scenario.label(), label);
        }
    }

    #[test]
    fn scenario_classification_uses_event_volume_and_preserves_workload_labels() {
        assert_eq!(
            classify_frame_scenario(FrameScenario::Idle, 1, true, 1, 1, 0),
            FrameScenario::Interactive
        );
        assert_eq!(
            classify_frame_scenario(FrameScenario::Interactive, 1, true, 3, 0, 0),
            FrameScenario::PtyOutput
        );
        assert_eq!(
            classify_frame_scenario(FrameScenario::Idle, 1, true, 1, 1, 1),
            FrameScenario::PtyOutput
        );
        assert_eq!(
            classify_frame_scenario(FrameScenario::Interactive, 2, true, 1, 1, 0),
            FrameScenario::MultiPane
        );
        assert_eq!(
            classify_frame_scenario(FrameScenario::Resize, 2, true, 10, 10, 10),
            FrameScenario::Resize
        );
        assert_eq!(
            classify_frame_scenario(FrameScenario::Scroll, 2, true, 10, 10, 10),
            FrameScenario::Scroll
        );
    }

    #[test]
    fn large_single_read_is_output_not_interactive_echo() {
        assert_eq!(
            classify_frame_scenario(FrameScenario::Interactive, 1, true, 1, 64 * 1024, 0),
            FrameScenario::PtyOutput
        );
    }
}
