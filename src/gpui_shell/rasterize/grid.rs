// gpui chrome migration (TD-GPUI-03 split): `rasterize_grid` itself --
// cosmic-text rasterization of the terminal grid into an RGBA bitmap, using
// the frame cache (`cache.rs`) to skip re-rasterizing/re-uploading when the
// grid hasn't changed. Split out of the single `rasterize.rs` (M1b) for the
// 400-line convention. `rasterize_grid`'s own body is broken into three
// private helpers along already-documented phase boundaries the original
// function's own comments called out: `fill_cell_backgrounds` and
// `build_shaping_spans` stay here, `draw_glyphs` (the biggest phase) is
// `glyphs.rs` -- this file was still over 400 lines with it inline. Pure
// code motion throughout, no logic changed, same computation order, same
// parameters threaded through by reference.

use std::cell::RefCell;
use std::hash::{Hash, Hasher};
use std::rc::Rc;
use std::sync::Arc;

use alacritty_terminal::selection::SelectionRange;
use alacritty_terminal::term::cell::Flags;

use crate::ui::search_bar::SearchMatch;
use cosmic_text::{Attrs, Buffer, Family, Shaping, SwashCache, Wrap};
use gpui::{Pixels, RenderImage, Window};
use image::{Frame, RgbaImage};

use crate::config::schema::ColorScheme;
use crate::font::shaper::CellStyle;
use crate::term::Terminal;

use super::cache::{CachedFrame, LAST_IMAGE};
use super::colors::{resolve_cell_colors, search_highlight_at, to_u8_rgba};
use super::glyphs::draw_glyphs;
use super::spans::build_shaping_spans;

/// Per-cell resolved (fg, bg, style) — one entry per character in a row's
/// shaped string, indices aligned with `grid_rows[row].chars()`.
pub(super) type CellColorStyle = ([f32; 4], [f32; 4], CellStyle);

thread_local! {
    static SWASH_CACHE: RefCell<SwashCache> = RefCell::new(SwashCache::new());
}

/// Rasterize `terminal`'s current grid into an RGBA bitmap and upload it as
/// a gpui `RenderImage`, or return the cached one from the last paint if
/// nothing that affects the bitmap has changed. `colors` is the resolved
/// theme (`config.colors`).
pub fn rasterize_grid(
    terminal: &Rc<Terminal>,
    cell_width: Pixels,
    cell_height: Pixels,
    scale: f32,
    colors: &ColorScheme,
    window: &mut Window,
    search: Option<(&[SearchMatch], usize)>,
) -> Option<Arc<RenderImage>> {
    terminal.with_term(|term| {
        let content = term.renderable_content();
        let cols = terminal.cols.get() as usize;
        let rows = terminal.rows.get() as usize;
        if cols == 0 || rows == 0 {
            return None;
        }

        let sel_range: Option<SelectionRange> =
            term.selection.as_ref().and_then(|s| s.to_range(term));

        // Build a line-indexed search map once — O(matches) — so the
        // per-cell lookup below is O(1) (TD-PERF-22, ported from the wgpu
        // build's own `collect_grid_cells`). Keyed on buffer-space grid
        // line (matching `SearchMatch::grid_line`'s own semantics
        // directly, the same space `cell.point.line.0` below is in before
        // any viewport conversion) — no coordinate translation needed.
        let search_idx: rustc_hash::FxHashMap<i32, Vec<(usize, usize, bool)>> =
            if let Some((matches, current_idx)) = search {
                let mut idx: rustc_hash::FxHashMap<i32, Vec<(usize, usize, bool)>> =
                    rustc_hash::FxHashMap::default();
                for (i, m) in matches.iter().enumerate() {
                    idx.entry(m.grid_line).or_default().push((
                        m.col,
                        m.col + m.len,
                        i == current_idx,
                    ));
                }
                idx
            } else {
                rustc_hash::FxHashMap::default()
            };

        // See `viewport_row`'s doc comment: `display_iter`'s cell line
        // numbers are buffer-space, not viewport-space, whenever
        // `display_offset > 0` -- using them as row indices directly (this
        // file's previous behaviour) drops the scrolled-back view's topmost
        // rows and misfiles the rest, leaving the bottom of the screen
        // blank. Invisible at `display_offset == 0`, the only case this
        // file was exercised under before scrolling existed (Task 6).
        let display_offset = content.display_offset;

        // Ligatures (e.g. `->`, `==`) only exist as a shaper decision across
        // ADJACENT characters in one shaped run, so text is still built one
        // string per row. Colors/style are collected in parallel, one entry
        // per character in that row's string (indices stay aligned with
        // `grid_rows[row].chars()`).
        let mut grid_rows: Vec<String> = vec![String::new(); rows];
        let mut grid_colors: Vec<Vec<CellColorStyle>> =
            (0..rows).map(|_| Vec::with_capacity(cols)).collect();
        for cell in content.display_iter {
            let Some(row) = super::colors::viewport_row(cell.point.line.0, display_offset) else {
                continue;
            };
            let col = cell.point.column.0;
            if row >= rows || col >= cols {
                continue;
            }
            let row_text = &mut grid_rows[row];
            let row_colors = &mut grid_colors[row];
            while row_text.chars().count() < col {
                row_text.push(' ');
                row_colors.push((colors.foreground, colors.background, CellStyle::NORMAL));
            }
            row_text.push(cell.c);
            let in_selection = sel_range.is_some_and(|range| {
                if range.is_block {
                    cell.point.line >= range.start.line
                        && cell.point.line <= range.end.line
                        && cell.point.column >= range.start.column
                        && cell.point.column <= range.end.column
                } else {
                    cell.point >= range.start && cell.point <= range.end
                }
            });
            let (fg, bg) = resolve_cell_colors(cell.fg, cell.bg, cell.flags, in_selection, colors);
            // Search highlight overrides selection, not the reverse --
            // matches the wgpu build's own priority order in
            // `collect_grid_cells`.
            let (fg, bg) =
                search_highlight_at(cell.point.line.0, col, &search_idx).unwrap_or((fg, bg));
            let style = CellStyle {
                bold: cell.flags.contains(Flags::BOLD),
                italic: cell.flags.contains(Flags::ITALIC),
            };
            row_colors.push((fg, bg, style));
        }

        let cell_w_px = f32::from(cell_width) * scale;
        let cell_h_px = f32::from(cell_height) * scale;
        // Must exactly match the destination rect `terminal_element.rs`'s
        // `paint()` passes to `Window::paint_image` for this bitmap. Since
        // M2 (`TerminalGridElement::request_layout` now requests
        // `relative(1.0)` -- a pane's element fills whatever size the flex
        // tree gives it, not a fixed `cell_width * cols` -- that call site
        // no longer uses its own full `bounds` as the image's destination
        // rect; it explicitly clamps to `cell_width * cols` x
        // `cell_height * rows` (the same values this function computes) so
        // the two always agree regardless of leftover fractional-cell space
        // in the pane's actual layout rect. `paint_image` then scales
        // WHATEVER destination rect it's given by `scale_factor` and rounds
        // with `.ceil()` (`bounds.scale(scale_factor).map_size(|s| s.ceil())`
        // -- verified against gpui 0.2.2's `window.rs`/`geometry.rs`); this
        // file mirrors that exact rounding order (multiply-by-scale, THEN
        // `.ceil()`, not the reverse) so the two computations can never
        // disagree by even one device pixel -- a real disagreement here
        // (found and fixed once already, in M1b) forces the GPU to stretch
        // the uploaded texture to fit a differently-sized target rect,
        // which blurs/smears hardest exactly at sharp color edges (glyph
        // boundaries, cell/pill transitions) while staying invisible in
        // large solid-color regions.
        let bitmap_width = ((f32::from(cell_width) * cols as f32 * scale).ceil() as u32).max(1);
        let bitmap_height = ((f32::from(cell_height) * rows as f32 * scale).ceil() as u32).max(1);
        let font_size_px = crate::gpui_shell::font_state::font_size() * scale;

        let content_hash = {
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            grid_rows.hash(&mut hasher);
            for row in &grid_colors {
                for (fg, bg, style) in row {
                    fg.map(|c| c.to_bits()).hash(&mut hasher);
                    bg.map(|c| c.to_bits()).hash(&mut hasher);
                    style.bold.hash(&mut hasher);
                    style.italic.hash(&mut hasher);
                }
            }
            bitmap_width.hash(&mut hasher);
            bitmap_height.hash(&mut hasher);
            hasher.finish()
        };
        let terminal_key = Rc::as_ptr(terminal) as usize;

        let cached = LAST_IMAGE.with_borrow(|cache| {
            cache
                .get(&terminal_key)
                .and_then(|frame| (frame.content_hash == content_hash).then(|| frame.image.clone()))
        });
        if let Some(image) = cached {
            // Grid unchanged since the last paint — reuse the cached
            // texture, skipping cosmic-text shaping/rasterization and the
            // GPU upload entirely.
            return Some(image);
        }

        let mut rgba_image = RgbaImage::new(bitmap_width, bitmap_height);

        // Shared column/row pixel-boundary tables: each edge is computed
        // exactly once and reused by the two cells that meet there, so
        // adjacent cells' fills are guaranteed contiguous (no 1px gap or
        // overlap from rounding `col_idx * cell_w_px` and
        // `(col_idx + 1) * cell_w_px` independently per cell, which doesn't
        // generally satisfy round((n+1)*w) == round(n*w) + round(w)).
        let col_edges: Vec<u32> = (0..=cols)
            .map(|i| ((i as f32 * cell_w_px).round() as u32).min(bitmap_width))
            .collect();
        let row_edges: Vec<u32> = (0..=rows)
            .map(|i| ((i as f32 * cell_h_px).round() as u32).min(bitmap_height))
            .collect();

        // Per-cell background: filled before glyphs are drawn, so text paints
        // on top of its own cell's resolved background.
        fill_cell_backgrounds(
            &mut rgba_image,
            &grid_colors,
            &row_edges,
            &col_edges,
            colors.background,
        );

        let font_features = crate::gpui_shell::font_state::font_features();
        crate::gpui_shell::font_state::with_font_system(|font_system, actual_family, pua| {
            SWASH_CACHE.with_borrow_mut(|swash_cache| {
                let metrics = cosmic_text::Metrics::new(font_size_px, cell_h_px);
                let mut buffer = Buffer::new(font_system, metrics);

                // Whole grid, ONE multi-line buffer: rows are joined with
                // '\n' into a single span list and shaped/drawn in one pass
                // (`set_rich_text` + `shape_until_scroll` + `draw`, each
                // called exactly once), matching the pre-M1b structure this
                // file replaced (`grid_rows.join("\n")` into one buffer).
                // Shaping each row as its own independent single-line buffer
                // in a loop — the previous structure here — was the actual
                // regression that produced fragmented/split Nerd Font icon
                // glyphs: resetting the buffer to a fresh single line 24
                // times and manually offsetting each row's glyph Y by
                // `row_idx * cell_height` doesn't necessarily line up with
                // cosmic-text's own ascent/descent/baseline placement for a
                // "fresh single line", which clips or misplaces glyphs that
                // sit close to the edges of their nominal cell height (Nerd
                // Font icons especially). Shaping the whole grid as one
                // buffer lets cosmic-text compute line positions itself, the
                // same way the pre-M1b code did.
                //
                // Spans still split ONLY on (bold, italic), never on color —
                // color must never fragment shaping (see `attrs_for`'s doc
                // comment). A span may contain an embedded '\n' where a row
                // boundary falls inside a run of matching (bold, italic);
                // that's fine, it just forces a line break there like any
                // other newline in the text — it does not need to end the
                // span.
                let spans = build_shaping_spans(
                    &grid_rows,
                    &grid_colors,
                    rows,
                    actual_family,
                    &font_features,
                );

                let default_attrs = Attrs::new().family(Family::Name(actual_family));
                let rich_spans: Vec<(&str, Attrs)> =
                    spans.iter().map(|(t, a)| (t.as_str(), a.clone())).collect();
                {
                    // Scoped so the `&mut FontSystem` reborrow ends here: the
                    // glyph-draw pass below needs `font_system` again (to
                    // rasterize glyphs through `SwashCache`), which it can't
                    // have while a `BorrowedWithFontSystem` is alive.
                    let mut shaping_buffer = buffer.borrow_with(font_system);
                    shaping_buffer.set_size(Some(bitmap_width as f32), Some(bitmap_height as f32));
                    shaping_buffer.set_wrap(Wrap::None);
                    shaping_buffer.set_rich_text(
                        rich_spans,
                        &default_attrs,
                        Shaping::Advanced,
                        None,
                    );
                    shaping_buffer.shape_until_scroll(true);
                }

                draw_glyphs(
                    &buffer,
                    &grid_colors,
                    &row_edges,
                    &col_edges,
                    bitmap_width,
                    bitmap_height,
                    colors.foreground,
                    &pua,
                    font_system,
                    swash_cache,
                    &mut rgba_image,
                );
            });
        });

        // gpui's sprite atlas stores images as BGRA with straight alpha
        // (verified against gpui 0.2.2's `elements/img.rs` decode path,
        // which does the same swap for plain RGBA-decoded images).
        for pixel in rgba_image.as_chunks_mut::<4>().0 {
            pixel.swap(0, 2);
        }

        let image = Arc::new(RenderImage::new(smallvec::smallvec![Frame::new(
            rgba_image
        )]));

        let old = LAST_IMAGE.with_borrow_mut(|cache| {
            cache.insert(
                terminal_key,
                CachedFrame {
                    content_hash,
                    image: image.clone(),
                },
            )
        });
        // Explicitly free the previous frame's sprite-atlas entry — gpui's
        // atlas is insert-only otherwise, which is the root cause of the M0
        // leak this cache fixes.
        if let Some(old_frame) = old {
            let _ = window.drop_image(old_frame.image);
        }

        Some(image)
    })
}

/// Per-cell background fill, before glyphs are drawn on top -- its own
/// phase in the original `rasterize_grid` (see the call site's own doc
/// comment there for why background must be filled before shaping/drawing).
fn fill_cell_backgrounds(
    image: &mut RgbaImage,
    grid_colors: &[Vec<CellColorStyle>],
    row_edges: &[u32],
    col_edges: &[u32],
    default_bg: [f32; 4],
) {
    for (row_idx, row_colors) in grid_colors.iter().enumerate() {
        let (py0, py1) = (row_edges[row_idx], row_edges[row_idx + 1]);
        for (col_idx, (_, bg, _)) in row_colors.iter().enumerate() {
            if *bg == default_bg {
                continue; // default background already covers this pixel via the base fill in terminal_element.rs
            }
            let (px0, px1) = (col_edges[col_idx], col_edges[col_idx + 1]);
            let pixel = image::Rgba(to_u8_rgba(*bg));
            for y in py0..py1 {
                for x in px0..px1 {
                    image.put_pixel(x, y, pixel);
                }
            }
        }
    }
}
