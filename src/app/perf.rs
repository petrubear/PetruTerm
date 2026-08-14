#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct FrameMetrics {
    pub pty_terminals: usize,
    pub dirty_rows: usize,
    pub rebuilt_rows: usize,
    pub upload_ranges: usize,
    pub upload_bytes: usize,
    pub wakeups: usize,
}

impl FrameMetrics {
    pub(crate) fn reset(&mut self) {
        *self = Self::default();
    }

    pub(crate) fn record_upload(&mut self, bytes: usize, ranges: usize) {
        self.upload_bytes = self.upload_bytes.saturating_add(bytes);
        self.upload_ranges = self.upload_ranges.saturating_add(ranges);
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

        assert_eq!(metrics.upload_bytes, 192);
        assert_eq!(metrics.upload_ranges, 3);

        metrics.reset();
        assert_eq!(metrics, FrameMetrics::default());
    }
}
