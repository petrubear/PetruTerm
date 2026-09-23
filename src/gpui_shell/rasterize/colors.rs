// Pure color/geometry helpers `rasterize_grid` builds on -- cell color
// resolution, search highlighting, shaping attrs, and pixel blending.

use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::vte::ansi::Color as AnsiColor;
use cosmic_text::{Attrs, Family, FontFeatures, Style, Weight};
use image::RgbaImage;

use crate::config::schema::ColorScheme;

/// Highlight colors for search matches -- Dracula bg/yellow/orange, ported
/// verbatim from the wgpu build's own `SEARCH_MATCH_FG`/`SEARCH_MATCH_BG`/
/// `SEARCH_CURRENT_BG` (`src/app/mux/mod.rs`), converted from that file's
/// 0-255 `AnsiColor::Spec(Rgb {..})` literals to this file's own `[f32; 4]`
/// (0.0-1.0) color space -- same values, same visual result.
#[allow(clippy::eq_op)]
pub(super) const SEARCH_MATCH_FG: [f32; 4] = [40.0 / 255.0, 42.0 / 255.0, 54.0 / 255.0, 1.0];
#[allow(clippy::eq_op)]
pub(super) const SEARCH_MATCH_BG: [f32; 4] = [241.0 / 255.0, 250.0 / 255.0, 140.0 / 255.0, 1.0];
#[allow(clippy::eq_op)]
pub(super) const SEARCH_CURRENT_BG: [f32; 4] = [255.0 / 255.0, 184.0 / 255.0, 108.0 / 255.0, 1.0];

/// Return overridden (fg, bg) colors if (grid_line, col) falls inside any
/// search match -- ported from the wgpu build's own `search_highlight_at`
/// (`src/app/mux/mod.rs`), same pre-built per-line index for O(1) line
/// lookup + O(matches_on_line) range check (TD-PERF-22), just returning
/// this file's `[f32; 4]` colors instead of `AnsiColor`.
pub(super) fn search_highlight_at(
    grid_line: i32,
    col: usize,
    idx: &rustc_hash::FxHashMap<i32, Vec<(usize, usize, bool)>>,
) -> Option<([f32; 4], [f32; 4])> {
    for &(start, end, is_current) in idx.get(&grid_line)? {
        if col >= start && col < end {
            let bg = if is_current {
                SEARCH_CURRENT_BG
            } else {
                SEARCH_MATCH_BG
            };
            return Some((SEARCH_MATCH_FG, bg));
        }
    }
    None
}

/// Resolve one cell's (fg, bg) into real theme colors, applying inverse-video
/// and selection-highlight swaps in that order — ported from
/// `src/app/mux/mod.rs`'s row-building loop, which already solves exactly
/// this for the wgpu renderer.
pub(super) fn resolve_cell_colors(
    fg: AnsiColor,
    bg: AnsiColor,
    flags: Flags,
    in_selection: bool,
    scheme: &ColorScheme,
) -> ([f32; 4], [f32; 4]) {
    let (fg, bg) = if flags.contains(Flags::INVERSE) {
        (bg, fg)
    } else {
        (fg, bg)
    };
    let (fg, bg) = (
        crate::term::color::resolve_color(fg, scheme),
        crate::term::color::resolve_color(bg, scheme),
    );
    if in_selection {
        (bg, fg)
    } else {
        (fg, bg)
    }
}

/// Attrs for a shaping span, keyed ONLY on (bold, italic) — deliberately no
/// `.color(...)`. Color is applied per-pixel post-shape (see the `draw`
/// callback in `rasterize_grid`), so it must never appear in `Attrs` here:
/// that would let cosmic-text bake a color into `color_opt`, defeating the
/// whole point of decoupling color from shaping.
pub(super) fn attrs_for<'a>(
    (bold, italic): (bool, bool),
    family: &'a str,
    font_features: &FontFeatures,
) -> Attrs<'a> {
    let mut attrs = Attrs::new()
        .family(Family::Name(family))
        .font_features(font_features.clone());
    if bold {
        attrs = attrs.weight(Weight::BOLD);
    }
    if italic {
        attrs = attrs.style(Style::Italic);
    }
    attrs
}

/// Source-over composite of one antialiased glyph pixel onto whatever is
/// already in the bitmap (its cell's resolved background, or nothing).
///
/// Blending (rather than overwriting) keeps antialiased edges over a
/// non-default cell background from fringing. On a fully transparent
/// destination this is equivalent to `put_pixel`.
pub(super) fn blend_pixel(img: &mut RgbaImage, x: u32, y: u32, src: [u8; 3], src_a: u8) {
    let dst = img.get_pixel(x, y).0;
    if dst[3] == 0 || src_a == 255 {
        img.put_pixel(
            x,
            y,
            image::Rgba([src[0], src[1], src[2], src_a.max(dst[3])]),
        );
        return;
    }
    let sa = f32::from(src_a) / 255.0;
    let da = f32::from(dst[3]) / 255.0;
    let out_a = sa + da * (1.0 - sa);
    if out_a <= 0.0 {
        return;
    }
    let mut out = [0u8; 4];
    for i in 0..3 {
        let blended = (f32::from(src[i]) * sa + f32::from(dst[i]) * da * (1.0 - sa)) / out_a;
        out[i] = blended.round().clamp(0.0, 255.0) as u8;
    }
    out[3] = (out_a * 255.0).round().clamp(0.0, 255.0) as u8;
    img.put_pixel(x, y, image::Rgba(out));
}

/// Convert a `display_iter` cell's buffer-space line number to the
/// viewport-relative row it belongs to, or `None` if it falls above the
/// current viewport (only possible when `line + display_offset < 0`, which
/// doesn't happen for cells `display_iter` actually yields, but the guard is
/// cheap and matches the original defensive check this replaces).
///
/// `Grid::display_iter` (alacritty_terminal 0.25.1) starts at
/// `Line(-(display_offset) - 1)` and yields lines
/// `-display_offset ..= -display_offset + screen_lines - 1` -- i.e. buffer
/// line 0 is `display_offset` rows down from the top of the viewport, not
/// the top row itself, whenever the view is scrolled back at all. This is
/// alacritty's own canonical buffer-to-viewport transform (see
/// `Term::point_to_viewport`, same formula) -- not a convention invented
/// here.
pub(super) fn viewport_row(line: i32, display_offset: usize) -> Option<usize> {
    let row = line + display_offset as i32;
    (row >= 0).then_some(row as usize)
}

pub(super) fn to_u8_rgba(c: [f32; 4]) -> [u8; 4] {
    [
        (c[0].clamp(0.0, 1.0) * 255.0).round() as u8,
        (c[1].clamp(0.0, 1.0) * 255.0).round() as u8,
        (c[2].clamp(0.0, 1.0) * 255.0).round() as u8,
        (c[3].clamp(0.0, 1.0) * 255.0).round() as u8,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use alacritty_terminal::vte::ansi::NamedColor;

    // 24-row terminal, scrolled back 10 lines -- the exact fixture the fix
    // was hand-verified against (`display_iter` yields buffer lines
    // -10..=13 inclusive; the old code discarded lines -10..-1 and
    // misfiled 0..13 as if they were viewport rows 0..13, instead of their
    // real 10..23).
    #[test]
    fn viewport_row_maps_buffer_range_onto_full_viewport_with_no_gaps() {
        let display_offset = 10;
        let mapped: Vec<usize> = (-10..=13)
            .map(|line| viewport_row(line, display_offset).unwrap())
            .collect();
        let expected: Vec<usize> = (0..24).collect();
        assert_eq!(mapped, expected);
    }

    #[test]
    fn viewport_row_is_identity_when_not_scrolled() {
        for line in 0..24 {
            assert_eq!(viewport_row(line, 0), Some(line as usize));
        }
    }

    #[test]
    fn viewport_row_none_above_the_viewport() {
        // Below the range `display_iter` ever actually yields in practice
        // (`viewport_row`'s own doc comment), but the guard must still
        // hold: a buffer line further back than `display_offset` maps
        // above row 0.
        assert_eq!(viewport_row(-11, 10), None);
    }

    fn test_scheme() -> ColorScheme {
        let mut scheme = ColorScheme {
            foreground: [1.0, 1.0, 1.0, 1.0],
            background: [0.0, 0.0, 0.0, 1.0],
            cursor_bg: [1.0, 1.0, 1.0, 1.0],
            cursor_fg: [0.0, 0.0, 0.0, 1.0],
            cursor_border: [1.0, 1.0, 1.0, 1.0],
            selection_bg: [0.5, 0.5, 0.5, 1.0],
            selection_fg: [1.0, 1.0, 1.0, 1.0],
            ansi: [[0.0; 4]; 8],
            brights: [[0.0; 4]; 8],
            ui_accent: [0.0; 4],
            ui_surface: [0.0; 4],
            ui_surface_active: [0.0; 4],
            ui_surface_hover: [0.0; 4],
            ui_muted: [0.0; 4],
            ui_success: [0.0; 4],
            ui_overlay: [0.0; 4],
            ui_border: [0.0; 4],
        };
        scheme.ansi[1] = [0.8, 0.0, 0.0, 1.0]; // red
        scheme
    }

    #[test]
    fn plain_cell_resolves_fg_bg_unswapped() {
        let scheme = test_scheme();
        let (fg, bg) = resolve_cell_colors(
            AnsiColor::Named(NamedColor::Foreground),
            AnsiColor::Named(NamedColor::Background),
            Flags::empty(),
            false,
            &scheme,
        );
        assert_eq!(fg, scheme.foreground);
        assert_eq!(bg, scheme.background);
    }

    #[test]
    fn inverse_flag_swaps_fg_bg() {
        let scheme = test_scheme();
        let (fg, bg) = resolve_cell_colors(
            AnsiColor::Named(NamedColor::Foreground),
            AnsiColor::Named(NamedColor::Background),
            Flags::INVERSE,
            false,
            &scheme,
        );
        assert_eq!(fg, scheme.background);
        assert_eq!(bg, scheme.foreground);
    }

    #[test]
    fn selection_swaps_fg_bg_after_inverse() {
        let scheme = test_scheme();
        let (fg, bg) = resolve_cell_colors(
            AnsiColor::Named(NamedColor::Red),
            AnsiColor::Named(NamedColor::Background),
            Flags::empty(),
            true,
            &scheme,
        );
        // Selection swap applies to the post-inverse (fg, bg): here inverse
        // didn't fire, so plain fg=red/bg=background gets swapped to
        // fg=background/bg=red.
        assert_eq!(fg, scheme.background);
        assert_eq!(bg, scheme.ansi[1]);
    }

    #[test]
    fn inverse_and_selection_compose() {
        let scheme = test_scheme();
        // Inverse swaps fg/bg first (fg=background, bg=red), then selection
        // swaps again (fg=red, bg=background) -- two swaps cancel out.
        let (fg, bg) = resolve_cell_colors(
            AnsiColor::Named(NamedColor::Red),
            AnsiColor::Named(NamedColor::Background),
            Flags::INVERSE,
            true,
            &scheme,
        );
        assert_eq!(fg, scheme.ansi[1]);
        assert_eq!(bg, scheme.background);
    }
}
