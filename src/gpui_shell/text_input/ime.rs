// `TextInput`'s IME support -- `EntityInputHandler`, the UTF-16/grapheme-
// boundary helpers it relies on (macOS's IME speaks UTF-16, Rust strings are
// UTF-8), and their unit tests. Ported unchanged in behavior from gpui
// 0.2.2's own examples/input.rs -- see `mod.rs`'s header for the
// adaptations.

use std::ops::Range;

use gpui::{Bounds, Context, EntityInputHandler, Pixels, UTF16Selection, Window};
use unicode_segmentation::UnicodeSegmentation;

use super::TextInput;

impl TextInput {
    fn range_to_utf16(&self, range: &Range<usize>) -> Range<usize> {
        offset_to_utf16(&self.content, range.start)..offset_to_utf16(&self.content, range.end)
    }

    fn range_from_utf16(&self, range_utf16: &Range<usize>) -> Range<usize> {
        offset_from_utf16(&self.content, range_utf16.start)
            ..offset_from_utf16(&self.content, range_utf16.end)
    }
}

impl EntityInputHandler for TextInput {
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        actual_range: &mut Option<Range<usize>>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<String> {
        let range = self.range_from_utf16(&range_utf16);
        actual_range.replace(self.range_to_utf16(&range));
        Some(self.content[range].to_string())
    }

    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(UTF16Selection {
            range: self.range_to_utf16(&self.selected_range),
            reversed: self.selection_reversed,
        })
    }

    fn marked_text_range(
        &self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Range<usize>> {
        self.marked_range
            .as_ref()
            .map(|range| self.range_to_utf16(range))
    }

    fn unmark_text(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        self.marked_range = None;
    }

    fn replace_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = range_utf16
            .as_ref()
            .map(|range_utf16| self.range_from_utf16(range_utf16))
            .or(self.marked_range.clone())
            .unwrap_or(self.selected_range.clone());

        self.content =
            (self.content[0..range.start].to_owned() + new_text + &self.content[range.end..])
                .into();
        self.selected_range = range.start + new_text.len()..range.start + new_text.len();
        self.marked_range.take();
        cx.notify();
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        new_selected_range_utf16: Option<Range<usize>>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = range_utf16
            .as_ref()
            .map(|range_utf16| self.range_from_utf16(range_utf16))
            .or(self.marked_range.clone())
            .unwrap_or(self.selected_range.clone());

        self.content =
            (self.content[0..range.start].to_owned() + new_text + &self.content[range.end..])
                .into();
        if !new_text.is_empty() {
            self.marked_range = Some(range.start..range.start + new_text.len());
        } else {
            self.marked_range = None;
        }
        self.selected_range = new_selected_range_utf16
            .as_ref()
            .map(|range_utf16| self.range_from_utf16(range_utf16))
            .map(|new_range| new_range.start + range.start..new_range.end + range.end)
            .unwrap_or_else(|| range.start + new_text.len()..range.start + new_text.len());

        cx.notify();
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        bounds: Bounds<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let last_layout = self.last_layout.as_ref()?;
        let range = self.range_from_utf16(&range_utf16);
        Some(Bounds::from_corners(
            gpui::point(
                bounds.left() + last_layout.x_for_index(range.start),
                bounds.top(),
            ),
            gpui::point(
                bounds.left() + last_layout.x_for_index(range.end),
                bounds.bottom(),
            ),
        ))
    }

    fn character_index_for_point(
        &mut self,
        point: gpui::Point<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<usize> {
        let line_point = self.last_bounds?.localize(&point)?;
        let last_layout = self.last_layout.as_ref()?;

        assert_eq!(last_layout.text, self.content);
        let utf8_index = last_layout.index_for_x(point.x - line_point.x)?;
        Some(offset_to_utf16(&self.content, utf8_index))
    }
}

/// Byte offset for a UTF-16 offset. macOS's IME speaks UTF-16, Rust strings
/// are UTF-8, so every `EntityInputHandler` boundary crosses this conversion.
fn offset_from_utf16(content: &str, offset: usize) -> usize {
    let mut utf8_offset = 0;
    let mut utf16_count = 0;
    for ch in content.chars() {
        if utf16_count >= offset {
            break;
        }
        utf16_count += ch.len_utf16();
        utf8_offset += ch.len_utf8();
    }
    utf8_offset
}

/// UTF-16 offset for a byte offset -- the inverse of `offset_from_utf16`.
fn offset_to_utf16(content: &str, offset: usize) -> usize {
    let mut utf16_offset = 0;
    let mut utf8_count = 0;
    for ch in content.chars() {
        if utf8_count >= offset {
            break;
        }
        utf8_count += ch.len_utf8();
        utf16_offset += ch.len_utf16();
    }
    utf16_offset
}

/// Previous grapheme-cluster boundary. Grapheme, not char: a ZWJ emoji
/// sequence is several chars but one cursor stop, and stepping by char
/// would leave the cursor inside it.
pub(super) fn previous_boundary(content: &str, offset: usize) -> usize {
    content
        .grapheme_indices(true)
        .rev()
        .find_map(|(idx, _)| (idx < offset).then_some(idx))
        .unwrap_or(0)
}

/// Next grapheme-cluster boundary. See `previous_boundary`.
pub(super) fn next_boundary(content: &str, offset: usize) -> usize {
    content
        .grapheme_indices(true)
        .find_map(|(idx, _)| (idx > offset).then_some(idx))
        .unwrap_or(content.len())
}

#[cfg(test)]
mod helper_tests {
    use super::{next_boundary, offset_from_utf16, offset_to_utf16, previous_boundary};

    #[test]
    fn utf16_offsets_round_trip_through_multibyte_text() {
        // "é" is 2 UTF-8 bytes / 1 UTF-16 unit; "日" is 3 / 1; "😀" is 4 / 2.
        let s = "aé日😀b";
        // Byte offsets of each char boundary: a=0, é=1, 日=3, 😀=6, b=10, end=11.
        for &utf8 in &[0usize, 1, 3, 6, 10, 11] {
            let utf16 = offset_to_utf16(s, utf8);
            assert_eq!(
                offset_from_utf16(s, utf16),
                utf8,
                "round trip failed at utf8 offset {utf8}"
            );
        }
        // The emoji really does occupy 2 UTF-16 units, so the mapping is not identity.
        assert_eq!(offset_to_utf16(s, 10), 5);
    }

    #[test]
    fn boundaries_step_over_whole_grapheme_clusters() {
        // A ZWJ family emoji is one grapheme but many bytes -- stepping by
        // char or by byte would land inside it and corrupt the string.
        let s = "a👨‍👩‍👧b";
        let after_a = next_boundary(s, 0);
        assert_eq!(after_a, 1);
        let after_family = next_boundary(s, after_a);
        assert_eq!(&s[after_a..after_family], "👨‍👩‍👧");
        assert_eq!(previous_boundary(s, after_family), after_a);
    }

    #[test]
    fn boundaries_clamp_at_the_ends() {
        let s = "abc";
        assert_eq!(previous_boundary(s, 0), 0);
        assert_eq!(next_boundary(s, s.len()), s.len());
    }
}
