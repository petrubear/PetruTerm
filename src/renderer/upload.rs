#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UploadRange {
    pub start: usize,
    pub end: usize,
}

pub fn merge_upload_ranges(ranges: &mut [UploadRange]) -> Vec<UploadRange> {
    ranges.sort_unstable_by_key(|range| (range.start, range.end));
    let mut merged: Vec<UploadRange> = Vec::with_capacity(ranges.len());
    for range in ranges
        .iter()
        .copied()
        .filter(|range| range.start < range.end)
    {
        if let Some(current) = merged.last_mut() {
            if range.start <= current.end {
                current.end = current.end.max(range.end);
                continue;
            }
        }
        merged.push(range);
    }
    merged
}

#[cfg(test)]
mod tests {
    use super::{merge_upload_ranges, UploadRange};

    #[test]
    fn merge_upload_ranges_coalesces_adjacent_and_overlapping_ranges() {
        let mut input = vec![
            UploadRange { start: 8, end: 12 },
            UploadRange { start: 0, end: 4 },
            UploadRange { start: 4, end: 8 },
            UploadRange { start: 10, end: 14 },
        ];

        assert_eq!(
            merge_upload_ranges(&mut input),
            vec![UploadRange { start: 0, end: 14 }]
        );
    }

    #[test]
    fn merge_upload_ranges_discards_empty_ranges() {
        let mut input = vec![
            UploadRange { start: 3, end: 3 },
            UploadRange { start: 8, end: 4 },
        ];

        assert!(merge_upload_ranges(&mut input).is_empty());
    }

    #[test]
    fn merge_upload_ranges_keeps_scattered_ranges_separate() {
        let mut input = vec![
            UploadRange { start: 12, end: 16 },
            UploadRange { start: 0, end: 2 },
            UploadRange { start: 6, end: 9 },
        ];

        assert_eq!(
            merge_upload_ranges(&mut input),
            vec![
                UploadRange { start: 0, end: 2 },
                UploadRange { start: 6, end: 9 },
                UploadRange { start: 12, end: 16 },
            ]
        );
    }
}
