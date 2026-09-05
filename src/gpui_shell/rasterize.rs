// gpui chrome migration (M1b): ANSI color resolution + cosmic-text
// rasterization of the terminal grid into an RGBA bitmap, plus the GPU
// sprite-atlas frame cache that avoids re-rasterizing/re-uploading when the
// grid hasn't changed. Split out of `terminal_element.rs` (M1a's own
// whole-branch review flagged the file as over the project's 400-line
// convention; this is the single biggest addition this plan makes to it).

use std::cell::RefCell;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::rc::Rc;
use std::sync::Arc;

use alacritty_terminal::selection::SelectionRange;
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::vte::ansi::Color as AnsiColor;
use cosmic_text::{
    Attrs, Buffer, Family, FeatureTag, FontFeatures, Shaping, Style, SwashCache, SwashContent,
    Weight, Wrap,
};
use gpui::{App, Pixels, RenderImage, Window};
use image::{Frame, RgbaImage};

use crate::config::schema::ColorScheme;
use crate::font::shaper::CellStyle;
use crate::term::Terminal;

/// Per-cell resolved (fg, bg, style) — one entry per character in a row's
/// shaped string, indices aligned with `grid_rows[row].chars()`.
type CellColorStyle = ([f32; 4], [f32; 4], CellStyle);

/// Resolve one cell's (fg, bg) into real theme colors, applying inverse-video
/// and selection-highlight swaps in that order — ported from
/// `src/app/mux/mod.rs`'s row-building loop, which already solves exactly
/// this for the wgpu renderer.
fn resolve_cell_colors(
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

thread_local! {
    static SWASH_CACHE: RefCell<SwashCache> = RefCell::new(SwashCache::new());

    // Caches the last rasterized frame per terminal (keyed by `Rc<Terminal>`'s
    // heap address — stable for the terminal's lifetime, and distinct per
    // split pane). Without this, `rasterize_grid` (driven by the ~30Hz
    // repaint poll in `gpui_shell/mod.rs`) would mint a brand-new
    // `RenderImage` — and therefore a brand-new globally-unique `ImageId`
    // (gpui 0.2.2's `assets.rs` static counter) — on every single paint, and
    // gpui's Metal sprite atlas is insert-only: nothing prunes an entry
    // except an explicit `window.drop_image(...)` call. That leaked one
    // full-grid GPU texture per paint, unbounded (measured: 598MB -> 2.52GB
    // RSS in 17s, idle, one terminal — the M0 leak). Caching the previous
    // frame and explicitly dropping it before inserting the next bounds the
    // atlas to one live texture per terminal, and skipping the
    // rasterize+upload entirely when the grid content hasn't changed also
    // avoids needless GPU uploads while idle.
    static LAST_IMAGE: RefCell<HashMap<usize, CachedFrame>> = RefCell::new(HashMap::new());
}

struct CachedFrame {
    /// Hash of the shaped row text, per-cell colors/style, and the bitmap's
    /// pixel dimensions — covers content changes (typing, scrolling, color
    /// changes, selection changes) and resize/rescale (cell size or window
    /// scale factor changing bitmap resolution).
    content_hash: u64,
    image: Arc<RenderImage>,
}

/// Drop every cached frame's GPU sprite-atlas entry across all windows —
/// called by `font_state::reload_font_config` when the font changes, since
/// every cached frame is stale the moment the font changes (keyed on content
/// hash, not font identity) and `cache.clear()` alone would free only the
/// Rust-side `Arc<RenderImage>` handles while leaking the underlying GPU
/// texture (gpui's atlas is insert-only — see `LAST_IMAGE`'s doc comment).
pub fn evict_all(cx: &mut App) {
    let evicted: Vec<Arc<RenderImage>> =
        LAST_IMAGE.with_borrow_mut(|cache| cache.drain().map(|(_, frame)| frame.image).collect());
    for image in evicted {
        cx.drop_image(image, None);
    }
}

/// Drop ONE terminal's cached frame and its GPU sprite-atlas entry, for a
/// pane that is going away.
///
/// `evict_all` (font reload) and the same-key replacement inside
/// `rasterize_grid` were the only two things that ever pruned `LAST_IMAGE`,
/// and neither fires when a pane closes — so every closed pane used to
/// strand one full-grid texture in gpui's insert-only Metal atlas for the
/// rest of the session. That is the M0 leak this cache exists to prevent
/// (see `LAST_IMAGE`'s doc comment), just at pane granularity instead of
/// per-paint: unreachable until M2 made panes and tabs closable at all.
///
/// `terminal_key` is the `Rc<Terminal>` heap address, so the caller MUST
/// call this while it still holds that `Rc` — once the last handle drops,
/// the address is gone and can be recycled by a later pane, which would
/// leave this entry stranded and hand the new pane a dead one.
pub fn evict_terminal(terminal_key: usize, cx: &mut App) {
    let evicted = LAST_IMAGE.with_borrow_mut(|cache| cache.remove(&terminal_key));
    if let Some(frame) = evicted {
        cx.drop_image(frame.image, None);
    }
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
            let Some(row) = viewport_row(cell.point.line.0, display_offset) else {
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
        for (row_idx, row_colors) in grid_colors.iter().enumerate() {
            let (py0, py1) = (row_edges[row_idx], row_edges[row_idx + 1]);
            for (col_idx, (_, bg, _)) in row_colors.iter().enumerate() {
                if *bg == colors.background {
                    continue; // default background already covers this pixel via the base fill in terminal_element.rs
                }
                let (px0, px1) = (col_edges[col_idx], col_edges[col_idx + 1]);
                let pixel = image::Rgba(to_u8_rgba(*bg));
                for y in py0..py1 {
                    for x in px0..px1 {
                        rgba_image.put_pixel(x, y, pixel);
                    }
                }
            }
        }

        crate::gpui_shell::font_state::with_font_system(|font_system, actual_family, pua| {
            SWASH_CACHE.with_borrow_mut(|swash_cache| {
                let metrics = cosmic_text::Metrics::new(font_size_px, cell_h_px);
                let mut buffer = Buffer::new(font_system, metrics);

                // MonoLisa gates its `->`/`==`/`!=`/`>=`-style ligatures
                // behind OpenType Character Variants, NOT calt/liga/ss0x
                // (confirmed via MonoLisa's own specimen page: "Arrows
                // (cv08)", "Equal combinations (cv09)").
                let mut font_features = FontFeatures::new();
                for tag in [
                    b"cv01", b"cv02", b"cv03", b"cv04", b"cv05", b"cv06", b"cv07", b"cv08", b"cv09",
                ] {
                    font_features.enable(FeatureTag::new(tag));
                }

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
                                    attrs_for(prev_key, actual_family, &font_features),
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
                        &font_features,
                    ),
                ));

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

                // Glyph-draw pass, hand-rolled instead of `buffer.draw(...)`.
                //
                // `draw()` never exposes each glyph's (font_id, glyph_id), so
                // it cannot apply the Nerd Font PUA correction — and without
                // that correction cosmic-text hands back icon glyphs resolved
                // against the WRONG face (or `glyph_id == 0`), because Nerd
                // Font patches routinely ship broken OS/2 Unicode-range bits
                // and fontdb derives its coverage from exactly those bits.
                // See `font_state::PuaContext` and `font::shaper`'s
                // `should_override` block; this loop is otherwise a faithful
                // reimplementation of cosmic-text 0.18.2's own
                // `Buffer::render` + `SwashCache::with_pixels` (same
                // `glyph.physical((0., run.line_y), 1.0)` call, same
                // `placement.left` / `-placement.top` blit origin).
                //
                // Paint color comes from the glyph's OWN cell — the cell its
                // first byte belongs to — and is applied to every pixel of
                // that glyph, exactly like the wgpu renderer's per-glyph
                // `ShapedGlyph::fg` (`src/app/renderer/terminal.rs`). It must
                // NOT be looked up per destination pixel from that pixel's own
                // column: a glyph's ink is free to reach (or cross) its cell's
                // edges, and powerline/Nerd Font separators are drawn to fill
                // their cell edge-to-edge by design, so after subpixel
                // positioning their outermost ink column lands one device pixel
                // inside the NEIGHBOURING cell. A per-pixel lookup then painted
                // that column with the neighbour's foreground — the dark
                // segment text colour — putting a one-pixel dark seam down the
                // left edge of every powerline separator and prompt cap.
                // Ordinary letters carry side bearing and never reach a cell
                // edge, which is why only the icons showed it.
                //
                // Colors are still fully decoupled from shaping: this is a
                // post-shape lookup keyed on the glyph, not an `Attrs` color.
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
                            .unwrap_or(colors.foreground);
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

                        let Some(image) = swash_cache.get_image(font_system, cache_key).as_ref()
                        else {
                            continue;
                        };
                        let placement = image.placement;
                        if placement.width == 0 || placement.height == 0 {
                            continue;
                        }
                        let is_color = match image.content {
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
                                    let Some(p) = image.data.get(i * 4..i * 4 + 4) else {
                                        continue;
                                    };
                                    ([p[0], p[1], p[2]], p[3])
                                } else {
                                    // 8-bit alpha mask: coverage only, the
                                    // color is this glyph's own cell's fg.
                                    let Some(&a) = image.data.get(i) else {
                                        continue;
                                    };
                                    ([gr, gg, gb], a)
                                };
                                if alpha == 0 {
                                    continue;
                                }
                                blend_pixel(&mut rgba_image, px_x as u32, py, rgb, alpha);
                            }
                        }
                    }
                }
            });
        });

        // gpui's sprite atlas stores images as BGRA with straight alpha
        // (verified against gpui 0.2.2's `elements/img.rs` decode path,
        // which does the same swap for plain RGBA-decoded images).
        for pixel in rgba_image.chunks_exact_mut(4) {
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

/// Attrs for a shaping span, keyed ONLY on (bold, italic) — deliberately no
/// `.color(...)`. Color is applied per-pixel post-shape (see the `draw`
/// callback in `rasterize_grid`), so it must never appear in `Attrs` here:
/// that would let cosmic-text bake a color into `color_opt`, defeating the
/// whole point of decoupling color from shaping.
fn attrs_for<'a>(
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
/// Overwriting instead — the previous behaviour — dropped the cell background
/// on every partially-covered edge pixel, so antialiased text over a non-default
/// background (powerline pill segments, selection highlight) got a fringe of
/// semi-transparent pixels that composited against the window's base fill
/// rather than against their own cell. On a fully transparent destination this
/// is exactly equivalent to the old `put_pixel`.
fn blend_pixel(img: &mut RgbaImage, x: u32, y: u32, src: [u8; 3], src_a: u8) {
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
fn viewport_row(line: i32, display_offset: usize) -> Option<usize> {
    let row = line + display_offset as i32;
    (row >= 0).then_some(row as usize)
}

fn to_u8_rgba(c: [f32; 4]) -> [u8; 4] {
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
