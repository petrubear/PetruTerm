// gpui chrome migration (TD-GPUI-03 split): the glyph-draw pass, split out
// of `grid.rs` (itself split out of the single `rasterize.rs`, M1b) --
// `grid.rs` was still over the 400-line convention with this alone (its
// biggest phase, ~200 lines including its own doc comment) still inline.
// Pure code motion, no logic changed.

use cosmic_text::{Buffer, SwashCache, SwashContent};
use image::RgbaImage;

use crate::gpui_shell::font_state::PuaContext;

use super::colors::{blend_pixel, to_u8_rgba};
use super::grid::CellColorStyle;

/// Glyph-draw pass, hand-rolled instead of `buffer.draw(...)`.
///
/// `draw()` never exposes each glyph's (font_id, glyph_id), so it cannot
/// apply the Nerd Font PUA correction — and without that correction
/// cosmic-text hands back icon glyphs resolved against the WRONG face (or
/// `glyph_id == 0`), because Nerd Font patches routinely ship broken OS/2
/// Unicode-range bits and fontdb derives its coverage from exactly those
/// bits. See `font_state::PuaContext` and `font::shaper`'s `should_override`
/// block; this loop is otherwise a faithful reimplementation of cosmic-text
/// 0.18.2's own `Buffer::render` + `SwashCache::with_pixels` (same
/// `glyph.physical((0., run.line_y), 1.0)` call, same `placement.left` /
/// `-placement.top` blit origin).
///
/// Paint color comes from the glyph's OWN cell — the cell its first byte
/// belongs to — and is applied to every pixel of that glyph, exactly like
/// the wgpu renderer's per-glyph `ShapedGlyph::fg`
/// (`src/app/renderer/terminal.rs`). It must NOT be looked up per
/// destination pixel from that pixel's own column: a glyph's ink is free to
/// reach (or cross) its cell's edges, and powerline/Nerd Font separators are
/// drawn to fill their cell edge-to-edge by design, so after subpixel
/// positioning their outermost ink column lands one device pixel inside the
/// NEIGHBOURING cell. A per-pixel lookup then painted that column with the
/// neighbour's foreground — the dark segment text colour — putting a
/// one-pixel dark seam down the left edge of every powerline separator and
/// prompt cap. Ordinary letters carry side bearing and never reach a cell
/// edge, which is why only the icons showed it.
///
/// Colors are still fully decoupled from shaping: this is a post-shape
/// lookup keyed on the glyph, not an `Attrs` color.
#[allow(clippy::too_many_arguments)]
pub(super) fn draw_glyphs(
    buffer: &Buffer,
    grid_colors: &[Vec<CellColorStyle>],
    row_edges: &[u32],
    col_edges: &[u32],
    bitmap_width: u32,
    bitmap_height: u32,
    default_fg: [f32; 4],
    pua: &PuaContext<'_>,
    font_system: &mut cosmic_text::FontSystem,
    swash_cache: &mut SwashCache,
    image: &mut RgbaImage,
) {
    let mut byte_to_col: Vec<usize> = Vec::new();
    for run in buffer.layout_runs() {
        let row_idx = run.line_i;
        let Some(row_colors) = grid_colors.get(row_idx) else {
            continue;
        };
        // Clip each glyph to its own cell box vertically, matching
        // the wgpu renderer's `y0/y1` clamp. Nerd Font icons are
        // routinely taller than the terminal's line height, and
        // unclipped they overhang the cell — visibly, as a prompt
        // cap standing taller than the pill background beside it.
        let cell_top = row_edges[row_idx];
        let cell_bottom = row_edges[row_idx + 1];

        // Byte offset -> column (char index) within this row, built
        // once per row rather than rescanning per glyph. Only char
        // boundaries are written; `glyph.start` is always one.
        let run_len = run.text.len();
        byte_to_col.clear();
        byte_to_col.resize(run_len + 1, 0);
        let mut char_idx = 0usize;
        for (byte_idx, _) in run.text.char_indices() {
            byte_to_col[byte_idx] = char_idx;
            char_idx += 1;
        }
        byte_to_col[run_len] = char_idx;

        for glyph in run.glyphs {
            let start = glyph.start.min(run_len);
            let col_idx = byte_to_col[start];
            let ch = run
                .text
                .get(start..glyph.end.min(run_len))
                .and_then(|s| s.chars().next())
                .unwrap_or(' ');
            let glyph_fg = row_colors
                .get(col_idx)
                .map(|(fg, _, _)| *fg)
                .unwrap_or(default_fg);
            let [gr, gg, gb, _] = to_u8_rgba(glyph_fg);

            let physical_glyph = glyph.physical((0., run.line_y), 1.0);

            // Override when cosmic-text found no glyph at all, or
            // when a Nerd Font icon codepoint was routed to a face
            // outside the primary font's own family (a real
            // bold/italic face of the primary font is in
            // `primary_face_ids`, so it is never mistaken for a
            // fallback).
            let should_override = glyph.glyph_id == 0
                || (crate::font::shaper::is_pua(ch)
                    && !pua.primary_face_ids.contains(&glyph.font_id));

            let cache_key = if should_override {
                match pua.ft_cmap.and_then(|ft| ft.get_glyph_index(ch)) {
                    Some(real_id) => {
                        // Only (font_id, glyph_id) are replaced —
                        // size, subpixel offset, weight and flags
                        // still come from the real shaped glyph.
                        let (key, _, _) = cosmic_text::CacheKey::new(
                            pua.primary_font_id,
                            real_id as u16,
                            glyph.font_size,
                            (glyph.x.fract(), 0.0),
                            glyph.font_weight,
                            glyph.cache_key_flags,
                        );
                        key
                    }
                    // Not in the primary font's cmap either —
                    // keep whatever cosmic-text resolved.
                    None => physical_glyph.cache_key,
                }
            } else {
                physical_glyph.cache_key
            };

            let Some(image_data) = swash_cache.get_image(font_system, cache_key).as_ref() else {
                continue;
            };
            let placement = image_data.placement;
            if placement.width == 0 || placement.height == 0 {
                continue;
            }
            let is_color = match image_data.content {
                SwashContent::Mask => false,
                SwashContent::Color => true,
                // cosmic-text doesn't handle this either.
                SwashContent::SubpixelMask => continue,
            };

            // Horizontal blit position is grid-anchored to this
            // glyph's own column (`col_edges[col_idx]`), NOT
            // `physical_glyph.x` (cosmic-text's cumulative
            // shaped pen position) -- matches how the wgpu
            // renderer always positions a glyph at
            // `col * cell_width` (`ShapedGlyph::bearing_x` is a
            // small correction on top of that, never the primary
            // position; see `src/font/shaper.rs`). For ordinary
            // monospace glyphs the two coincide almost exactly
            // (confirmed: cosmic-text's own per-glyph advances
            // for the primary font already equal `cell_width`),
            // so this is a no-op there. It matters for any glyph
            // whose natural advance doesn't equal the grid's
            // per-column width -- concretely, color emoji: a
            // measured 😀 glyph advanced 15px against a 19.2px
            // (2-column) budget, and every character shaped
            // after it in the same run drifted right by the
            // 4.2px difference, compounding with each further
            // emoji on the line. `physical_glyph`'s subpixel
            // hinting (via `cache_key`) is left untouched --
            // that only affects which hinted bitmap variant gets
            // rasterized, not where it's drawn, and grid cell
            // edges are already whole-pixel (`col_edges: Vec<u32>`)
            // so re-deriving a cache key from them would only
            // throw away real subpixel hint quality for no gain.
            let Some(&col_x) = col_edges.get(col_idx) else {
                continue;
            };
            let glyph_x = col_x as i32 + placement.left;
            let glyph_y = physical_glyph.y - placement.top;

            for row in 0..placement.height {
                let py = glyph_y + row as i32;
                if py < 0 {
                    continue;
                }
                let py = py as u32;
                if py < cell_top || py >= cell_bottom || py >= bitmap_height {
                    continue;
                }
                for col in 0..placement.width {
                    let px_x = glyph_x + col as i32;
                    if px_x < 0 || px_x as u32 >= bitmap_width {
                        continue;
                    }
                    let i = (row * placement.width + col) as usize;
                    let (rgb, alpha) = if is_color {
                        // 32-bit RGBA bitmap (color emoji).
                        let Some(p) = image_data.data.get(i * 4..i * 4 + 4) else {
                            continue;
                        };
                        ([p[0], p[1], p[2]], p[3])
                    } else {
                        // 8-bit alpha mask: coverage only, the
                        // color is this glyph's own cell's fg.
                        let Some(&a) = image_data.data.get(i) else {
                            continue;
                        };
                        ([gr, gg, gb], a)
                    };
                    if alpha == 0 {
                        continue;
                    }
                    blend_pixel(image, px_x as u32, py, rgb, alpha);
                }
            }
        }
    }
}
