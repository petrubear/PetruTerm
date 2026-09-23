// Turns `crate::llm::markdown::{AnnotatedLine, BlockKind, SpanKind}` -- an
// engine-agnostic annotation format built for the wgpu build's terminal-cell
// grid (`src/app/renderer/chat.rs`) -- into styled gpui text.
//
// Messages get *native gpui text layout*: real
// proportional-font wrapping, not the wgpu renderer's character-column math.
// `render.rs` calls `parse_markdown` with a very large width so its own
// char-count wrapping never fires; the `AnnotatedLine`s that come back are
// used here purely for *styling* (heading level, code block, bold/italic/
// code/syntax spans) while gpui's own flex/text layout does the actual
// wrapping.
//
// Byte vs. char offsets: `AnnotatedLine::spans` are `(start, end, SpanKind)`
// **character**-index ranges into `display` (correct for the wgpu build,
// which indexes terminal grid columns with them). gpui's
// `StyledText::with_highlights` wants **byte** ranges into the
// `SharedString` it wraps, so every span is remapped via `char_indices`
// below before it reaches gpui -- required for correctness on any message
// containing multi-byte UTF-8 (accents, CJK, emoji), not just an edge case.

use gpui::{div, prelude::*, px, Div, FontStyle, FontWeight, HighlightStyle, StyledText};

use crate::config::schema::ColorScheme;
use crate::llm::markdown::{AnnotatedLine, BlockKind, SpanKind, TokenKind};

use super::super::font_state;
use super::super::pane_view::to_rgba;

/// Render one already-parsed markdown line as a styled gpui block. Block
/// kind drives container styling (heading size/weight, code block
/// background+monospace, list indent); inline spans drive per-run
/// highlights within the line's own text.
pub fn render_line(line: &AnnotatedLine, colors: &ColorScheme) -> Div {
    let body = if line.display.is_empty() {
        // A bare empty `StyledText` has no intrinsic height, so a blank
        // paragraph line (markdown's own vertical spacing) would otherwise
        // collapse entirely. One cell-height's worth of blank div keeps
        // the paragraph break visible without depending on `StyledText` at
        // all.
        div().h(font_state::measured_cell_size().1)
    } else {
        let highlights: Vec<(std::ops::Range<usize>, HighlightStyle)> = line
            .spans
            .iter()
            .filter_map(|(start_char, end_char, kind)| {
                let (start, end) = char_range_to_byte_range(&line.display, *start_char, *end_char)?;
                Some((start..end, span_highlight(kind, colors)))
            })
            .collect();
        div().child(StyledText::new(line.display.clone()).with_highlights(highlights))
    };

    match &line.kind {
        BlockKind::Heading(level) => body
            .font_weight(FontWeight::BOLD)
            .text_size(heading_size(*level))
            .text_color(to_rgba(colors.foreground))
            .pt_2(),
        BlockKind::CodeBlock => body
            .font_family(font_state::font_family())
            .text_color(to_rgba(colors.foreground))
            .bg(to_rgba(colors.ui_surface)),
        BlockKind::ListItem { indent, .. } => body
            .text_color(to_rgba(colors.foreground))
            .whitespace_normal()
            .pl(px(12.0 * (*indent as f32 + 1.0))),
        BlockKind::Normal => body
            .text_color(to_rgba(colors.foreground))
            .whitespace_normal(),
    }
}

fn heading_size(level: u8) -> gpui::Pixels {
    match level {
        1 => px(20.0),
        2 => px(17.0),
        _ => px(15.0),
    }
}

fn span_highlight(kind: &SpanKind, colors: &ColorScheme) -> HighlightStyle {
    match kind {
        SpanKind::Bold => HighlightStyle {
            font_weight: Some(FontWeight::BOLD),
            ..Default::default()
        },
        SpanKind::Italic => HighlightStyle {
            font_style: Some(FontStyle::Italic),
            ..Default::default()
        },
        SpanKind::Code => HighlightStyle {
            color: Some(to_rgba(colors.ui_accent).into()),
            background_color: Some(to_rgba(colors.ui_surface).into()),
            ..Default::default()
        },
        SpanKind::Syntax(token) => HighlightStyle {
            color: Some(to_rgba(syntax_color(token, colors)).into()),
            ..Default::default()
        },
    }
}

fn syntax_color(token: &TokenKind, colors: &ColorScheme) -> [f32; 4] {
    match token {
        TokenKind::Keyword => colors.ui_accent,
        TokenKind::StringLit => colors.ui_success,
        TokenKind::Comment => colors.ui_muted,
        TokenKind::Number => colors.ui_accent,
        TokenKind::Operator => colors.foreground,
    }
}

/// Map a `(start_char, end_char)` character-offset span into `display`'s own
/// UTF-8 byte offsets. Returns `None` for a degenerate or unresolvable span
/// rather than panicking or slicing mid-codepoint -- malformed spans here
/// must not crash the shell, they're display-only annotations.
fn char_range_to_byte_range(
    display: &str,
    start_char: usize,
    end_char: usize,
) -> Option<(usize, usize)> {
    if end_char <= start_char {
        return None;
    }
    let mut start_byte = None;
    let mut end_byte = None;
    for (char_idx, (byte_idx, _)) in display.char_indices().enumerate() {
        if char_idx == start_char {
            start_byte = Some(byte_idx);
        }
        if char_idx == end_char {
            end_byte = Some(byte_idx);
        }
    }
    let start = start_byte?;
    // `end_char` landing exactly at (or past) the string's char count means
    // "through the end" -- char_indices() never yields an index for the
    // one-past-the-end position, so that case is resolved to `display.len()`
    // rather than treated as unresolvable.
    let end = end_byte.unwrap_or(display.len());
    if start >= end {
        None
    } else {
        Some((start, end))
    }
}
