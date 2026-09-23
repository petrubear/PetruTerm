// Builds the whole grid's cosmic-text shaping spans.

use cosmic_text::{Attrs, FontFeatures};

use crate::font::shaper::CellStyle;

use super::colors::attrs_for;
use super::grid::CellColorStyle;

/// Build the whole grid's shaping spans -- one span per contiguous run of
/// matching (bold, italic), joined by '\n' at row boundaries. See
/// `rasterize_grid`'s own call site comment for why the whole grid is one
/// multi-line buffer rather than one buffer per row.
pub(super) fn build_shaping_spans<'a>(
    grid_rows: &[String],
    grid_colors: &[Vec<CellColorStyle>],
    rows: usize,
    actual_family: &'a str,
    font_features: &FontFeatures,
) -> Vec<(String, Attrs<'a>)> {
    let mut spans: Vec<(String, Attrs)> = Vec::new();
    let mut span_text = String::new();
    let mut span_key: Option<(bool, bool)> = None;
    for (row_idx, row_text) in grid_rows.iter().enumerate() {
        let chars: Vec<char> = row_text.chars().collect();
        let row_colors = &grid_colors[row_idx];
        for (i, ch) in chars.iter().enumerate() {
            let style = row_colors
                .get(i)
                .map(|(_, _, s)| *s)
                .unwrap_or(CellStyle::NORMAL);
            let key = (style.bold, style.italic);
            if let Some(prev_key) = span_key {
                if prev_key != key {
                    spans.push((
                        std::mem::take(&mut span_text),
                        attrs_for(prev_key, actual_family, font_features),
                    ));
                }
            }
            span_key = Some(key);
            span_text.push(*ch);
        }
        if row_idx + 1 < rows {
            span_text.push('\n');
        }
    }
    spans.push((
        span_text,
        attrs_for(
            span_key.unwrap_or((false, false)),
            actual_family,
            font_features,
        ),
    ));
    spans
}
