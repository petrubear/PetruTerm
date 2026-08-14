#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UploadRange {
    pub start: usize,
    pub end: usize,
}

pub fn upload_ranges_bytes(ranges: &[UploadRange], element_size: usize) -> usize {
    ranges.iter().fold(0usize, |total, range| {
        total.saturating_add(
            range
                .end
                .saturating_sub(range.start)
                .saturating_mul(element_size),
        )
    })
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct TerminalUploadAccounting {
    pub full_bytes: usize,
    pub full_ranges: usize,
    pub incremental_bytes: usize,
    pub incremental_ranges: usize,
}

pub fn account_terminal_uploads(
    terminal_vertices: usize,
    lcd_vertices: usize,
    terminal_ranges: &[UploadRange],
    lcd_ranges: &[UploadRange],
    element_size: usize,
    lcd_enabled: bool,
) -> TerminalUploadAccounting {
    let full_bytes = terminal_vertices
        .saturating_add(lcd_enabled.then_some(lcd_vertices).unwrap_or(0))
        .saturating_mul(element_size);
    let full_ranges =
        usize::from(terminal_vertices > 0) + usize::from(lcd_enabled && lcd_vertices > 0);
    let incremental_bytes = upload_ranges_bytes(terminal_ranges, element_size).saturating_add(
        lcd_enabled
            .then(|| upload_ranges_bytes(lcd_ranges, element_size))
            .unwrap_or(0),
    );
    let incremental_ranges = terminal_ranges.len() + usize::from(lcd_enabled) * lcd_ranges.len();
    TerminalUploadAccounting {
        full_bytes,
        full_ranges,
        incremental_bytes,
        incremental_ranges,
    }
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
    use super::{account_terminal_uploads, merge_upload_ranges, upload_ranges_bytes, UploadRange};

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

    #[test]
    fn upload_ranges_bytes_accounts_for_merged_ranges() {
        let ranges = [
            UploadRange { start: 0, end: 4 },
            UploadRange { start: 8, end: 10 },
        ];

        assert_eq!(upload_ranges_bytes(&ranges, 80), 480);
    }

    #[test]
    fn terminal_upload_accounting_respects_lcd_configuration() {
        let terminal = [UploadRange { start: 0, end: 4 }];
        let lcd = [UploadRange { start: 8, end: 10 }];

        assert_eq!(
            account_terminal_uploads(10, 10, &terminal, &lcd, 8, false),
            super::TerminalUploadAccounting {
                full_bytes: 80,
                full_ranges: 1,
                incremental_bytes: 32,
                incremental_ranges: 1,
            }
        );
        assert_eq!(
            account_terminal_uploads(10, 10, &terminal, &lcd, 8, true),
            super::TerminalUploadAccounting {
                full_bytes: 160,
                full_ranges: 2,
                incremental_bytes: 48,
                incremental_ranges: 2,
            }
        );
    }
}
