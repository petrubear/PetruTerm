// gpui chrome migration (M0 foundation spike + M1a foundation fixes): paints
// one `term::Terminal`'s grid as a custom gpui `Element`.
//
// gpui's native text-shaping/painting facilities (`window.text_system()`,
// used here originally) do not produce ligatures at this gpui version, per
// real dogfood testing (not a config mistake — a confirmed limitation).
// Fallback: shape + rasterize the grid with cosmic-text (which already
// renders ligatures correctly in this project's existing wgpu renderer, see
// src/font/shaper.rs) into an RGBA bitmap, and paint that bitmap into gpui
// via `Window::paint_image`.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::rc::Rc;
use std::sync::Arc;

use cosmic_text::{
    Attrs, Buffer, Color as CosmicColor, Family, FeatureTag, FontFeatures, FontSystem, Metrics,
    Shaping, SwashCache, Wrap,
};
use gpui::{
    fill, point, px, size, App, Bounds, Corners, Element, ElementId, GlobalElementId,
    InspectorElementId, IntoElement, LayoutId, Pixels, RenderImage, Style, Window,
};
use image::{Frame, RgbaImage};

use crate::config::schema::FontConfig;
use crate::term::Terminal;

/// Set once from `main()`, before any window or `TerminalGridElement` exists.
/// `FONT_SYSTEM`'s thread-local initializer reads from this instead of a
/// hardcoded literal — see `set_font_config`.
static FONT_CONFIG: std::sync::OnceLock<FontConfig> = std::sync::OnceLock::new();

/// Must be called exactly once, before the first paint (i.e. before
/// `cx.open_window(...)` in `main()`). Panics if called twice.
pub fn set_font_config(font_config: FontConfig) {
    FONT_CONFIG
        .set(font_config)
        .expect("set_font_config called more than once");
}

/// Rebuild `FONT_SYSTEM`'s cached font/family/size/line-height for a new
/// config, on hot-reload. Unlike `set_font_config` (set-once, for startup),
/// this may be called repeatedly. Also recomputes `CELL_SIZE` (font size and
/// line height feed cell geometry, so a reload that changes either must
/// recompute it too — this is the single source of truth `measured_cell_size`
/// and `spawn_terminal` both read).
///
/// Takes `&mut App` to explicitly free the outgoing frames' GPU sprite-atlas
/// entries: `LAST_IMAGE`'s entries are keyed on content hash, not font
/// identity, so every cached frame is stale the moment the font changes —
/// but gpui's atlas is insert-only (see `LAST_IMAGE`'s doc comment), so
/// merely dropping the `Arc<RenderImage>` handles (`cache.clear()`, the
/// original approach) would free the Rust-side handle while leaking the
/// underlying GPU texture, one per terminal per reload — the exact bug class
/// M0 fixed for the steady-state repaint path, reopened here. `App::drop_image`
/// prunes the entry from every window's atlas, matching what `paint()`'s own
/// per-frame eviction already does for the steady-state case.
pub fn reload_font_config(font_config: FontConfig, cx: &mut App) {
    let (new_font_system, new_family, _face_id, _path, _face_index) =
        match crate::font::loader::build_font_system(&font_config) {
            Ok(v) => v,
            Err(e) => {
                log::error!("gpui-shell: failed to reload font on config change: {e:#}");
                return;
            }
        };
    let mut font_system = new_font_system;
    let cell_size = compute_cell_size(
        &mut font_system,
        &new_family,
        font_config.size,
        font_config.line_height,
    );
    FONT_SYSTEM.with_borrow_mut(|state| {
        state.font_system = font_system;
        state.family = new_family;
        state.size = font_config.size;
        state.line_height = font_config.line_height;
    });
    CELL_SIZE.set(cell_size);

    let evicted: Vec<Arc<RenderImage>> =
        LAST_IMAGE.with_borrow_mut(|cache| cache.drain().map(|(_, frame)| frame.image).collect());
    for image in evicted {
        cx.drop_image(image, None);
    }
}

/// `FontSystem` + the real config values that feed shaping/metrics, cached
/// together so a config reload (`reload_font_config`) has one place to
/// update all of them consistently.
struct FontState {
    font_system: FontSystem,
    family: String,
    /// Font size in points — real `config.font.size`, not a hardcoded
    /// constant (see `reload_font_config`'s doc comment for why this and
    /// `line_height` must live beside `font_system`/`family`).
    size: f32,
    /// Line height multiplier — real `config.font.line_height`.
    line_height: f32,
}

// `FontSystem::new()` only does a bare system-font scan — it will NOT find
// MonoLisaCode Nerd Font unless that exact family is fully OS-registered,
// and will silently fall back to some default monospace with no ligature
// GSUB rules (renders fine, never ligates, regardless of feature flags).
// The existing wgpu renderer never hits this: `font::loader::build_font_system`
// resolves the configured family via `FontLocator` and explicitly
// `db.load_font_file(...)`s it, then reports the font's ACTUAL internal
// family name from fontdb (which can differ from the config string) — reuse
// that exact, proven path instead of guessing a bare `Family::Name(...)`.
//
// `SwashCache` accumulates a rasterization cache. Both are meant to be
// created once and shared. `TerminalGridElement` is rebuilt every frame, so
// these live in thread-local storage instead (gpui's paint phase runs on the
// main thread, so a plain `RefCell` is enough here).
thread_local! {
    static FONT_SYSTEM: RefCell<FontState> = RefCell::new({
        let font_config = FONT_CONFIG
            .get()
            .expect("set_font_config must be called before the first paint");
        let (font_system, family, _face_id, _path, _face_index) =
            crate::font::loader::build_font_system(font_config)
                .expect("load configured font for terminal ligature rendering");
        FontState {
            font_system,
            family,
            size: font_config.size,
            line_height: font_config.line_height,
        }
    });
    static SWASH_CACHE: RefCell<SwashCache> = RefCell::new(SwashCache::new());

    // Caches the last rasterized frame per terminal (keyed by `Rc<Terminal>`'s
    // heap address — stable for the terminal's lifetime, and distinct per
    // split pane). Without this, `paint()` (driven by the ~30Hz repaint poll
    // in `gpui_shell/mod.rs`) would mint a brand-new `RenderImage` — and
    // therefore a brand-new globally-unique `ImageId` (gpui 0.2.2's
    // `assets.rs` static counter) — on every single paint, and gpui's Metal
    // sprite atlas is insert-only: nothing prunes an entry except an
    // explicit `window.drop_image(...)` call. That leaked one full-grid GPU
    // texture per paint, unbounded (measured: 598MB -> 2.52GB RSS in 17s,
    // idle, one terminal). Caching the previous frame and explicitly
    // dropping it before inserting the next bounds the atlas to one live
    // texture per terminal, and skipping the rasterize+upload entirely when
    // the grid content hasn't changed also avoids needless GPU uploads while
    // idle.
    static LAST_IMAGE: RefCell<HashMap<usize, CachedFrame>> = RefCell::new(HashMap::new());

    // Real cell width/height for the current font/size/line-height, computed
    // once here (and again in `reload_font_config` on a config change) —
    // the single source of truth both `measured_cell_size` (render path) and
    // `spawn_terminal` (PTY winsize) read, so the two can never disagree.
    static CELL_SIZE: Cell<(Pixels, Pixels)> = Cell::new(FONT_SYSTEM.with_borrow_mut(|state| {
        compute_cell_size(&mut state.font_system, &state.family, state.size, state.line_height)
    }));
}

struct CachedFrame {
    /// Hash of the shaped row text plus the bitmap's pixel dimensions —
    /// covers both content changes (typing, scrolling) and resize/rescale
    /// (cell size or window scale factor changing bitmap resolution).
    content_hash: u64,
    image: Arc<RenderImage>,
}

/// Shape a sample string and read its advance width to get the real cell
/// width/height for `family` at `size`/`line_height` — the same technique
/// `font::shaper::TextShaper::measure_cell()`'s fallback branch uses, ported
/// here rather than importing that (wgpu-atlas-coupled) type. Pure function
/// so both the initial `CELL_SIZE` computation and `reload_font_config` share
/// one implementation.
fn compute_cell_size(
    font_system: &mut FontSystem,
    family: &str,
    size: f32,
    line_height: f32,
) -> (Pixels, Pixels) {
    let metrics = Metrics::new(size, size * line_height);
    let mut buffer = Buffer::new(font_system, metrics);
    let mut buffer = buffer.borrow_with(font_system);
    buffer.set_size(Some(1000.0), Some(1000.0));

    let attrs = Attrs::new().family(Family::Name(family));
    // 16 `M`s, matching TextShaper::measure_cell's own sample — wide
    // enough for a stable average, short enough to stay off any line-wrap
    // boundary at this buffer width.
    buffer.set_text("MMMMMMMMMMMMMMMM", &attrs, Shaping::Advanced, None);
    buffer.shape_until_scroll(true);

    let run_width = buffer
        .layout_runs()
        .next()
        .map(|run| run.line_w)
        .unwrap_or(size * 0.6 * 16.0);
    let cell_width = (run_width / 16.0).max(1.0);
    let cell_height = metrics.line_height.max(1.0);

    (px(cell_width), px(cell_height))
}

/// Real cell width/height for the current config, cached in `CELL_SIZE` —
/// the same value `spawn_terminal` uses for the PTY winsize, so the painted
/// grid and the PTY's advertised cell geometry never disagree.
pub fn measured_cell_size() -> (Pixels, Pixels) {
    CELL_SIZE.get()
}

const TEXT_COLOR: CosmicColor = CosmicColor::rgb(0xf8, 0xf8, 0xf2);

pub struct TerminalGridElement {
    pub terminal: Rc<Terminal>,
    pub cell_width: Pixels,
    pub cell_height: Pixels,
}

impl IntoElement for TerminalGridElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for TerminalGridElement {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let cols = self.terminal.cols as f32;
        let rows = self.terminal.rows as f32;
        let mut style = Style::default();
        style.size.width = (self.cell_width * cols).into();
        style.size.height = (self.cell_height * rows).into();
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        _window: &mut Window,
        _cx: &mut App,
    ) -> Self::PrepaintState {
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        _prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        _cx: &mut App,
    ) {
        // Background.
        window.paint_quad(fill(bounds, gpui::rgb(0x1e1f29)));

        self.terminal.with_term(|term| {
            let content = term.renderable_content();
            let cols = self.terminal.cols as usize;
            let rows = self.terminal.rows as usize;

            // Ligatures (e.g. `->`, `==`) only exist as a shaper decision
            // across ADJACENT characters in one shaped run — shaping one
            // character at a time (the earlier approach) can never produce
            // them, regardless of font. Build one string per row instead and
            // shape/paint it as a single run, so adjacent glyphs the font
            // defines a ligature for are actually shaped together.
            let mut grid_rows: Vec<String> = vec![String::new(); rows];
            for cell in content.display_iter {
                let row = cell.point.line.0;
                if row < 0 {
                    continue;
                }
                let row = row as usize;
                let col = cell.point.column.0;
                if row >= rows || col >= cols {
                    continue;
                }
                let row_text = &mut grid_rows[row];
                while row_text.chars().count() < col {
                    row_text.push(' ');
                }
                row_text.push(cell.c);
            }

            if cols == 0 || rows == 0 {
                return;
            }

            // Rasterize at physical (device) pixel resolution so text stays
            // crisp on HiDPI/Retina displays — `paint_image` stretches the
            // bitmap to fill `bounds` scaled to device pixels internally, so
            // rendering 1:1 at that resolution avoids blur from upscaling.
            let scale = window.scale_factor();
            let cell_w_px = f32::from(self.cell_width) * scale;
            let cell_h_px = f32::from(self.cell_height) * scale;
            let bitmap_width = ((cell_w_px * cols as f32).round() as u32).max(1);
            let bitmap_height = ((cell_h_px * rows as f32).round() as u32).max(1);
            let font_size = FONT_SYSTEM.with_borrow(|state| state.size);
            let font_size_px = font_size * scale;

            let content_hash = {
                let mut hasher = std::collections::hash_map::DefaultHasher::new();
                grid_rows.hash(&mut hasher);
                bitmap_width.hash(&mut hasher);
                bitmap_height.hash(&mut hasher);
                hasher.finish()
            };
            let terminal_key = Rc::as_ptr(&self.terminal) as usize;

            let cached = LAST_IMAGE.with_borrow(|cache| {
                cache.get(&terminal_key).and_then(|frame| {
                    (frame.content_hash == content_hash).then(|| frame.image.clone())
                })
            });

            let render_image = if let Some(image) = cached {
                // Grid unchanged since the last paint — reuse the cached
                // texture, skipping cosmic-text shaping/rasterization and
                // the GPU upload entirely.
                image
            } else {
                let mut rgba_image = RgbaImage::new(bitmap_width, bitmap_height);

                FONT_SYSTEM.with_borrow_mut(|state| {
                    let font_system = &mut state.font_system;
                    let actual_family = &state.family;
                    SWASH_CACHE.with_borrow_mut(|swash_cache| {
                        let metrics = Metrics::new(font_size_px, cell_h_px);
                        let mut buffer = Buffer::new(font_system, metrics);
                        let mut buffer = buffer.borrow_with(font_system);
                        buffer.set_size(Some(bitmap_width as f32), Some(bitmap_height as f32));
                        buffer.set_wrap(Wrap::None);

                        // MonoLisa gates its `->`/`==`/`!=`/`>=`-style ligatures
                        // behind OpenType Character Variants, NOT calt/liga/ss0x
                        // (confirmed via MonoLisa's own specimen page: "Arrows
                        // (cv08)", "Equal combinations (cv09)" — exactly the
                        // glyphs tested here). calt/liga alone (the earlier
                        // attempt, matching the existing wgpu renderer's proven
                        // call) is why every prior try failed — this is a
                        // MonoLisa-specific feature family the existing renderer
                        // never needed to request explicitly for other fonts.
                        let mut font_features = FontFeatures::new();
                        for tag in [
                            b"cv01", b"cv02", b"cv03", b"cv04", b"cv05", b"cv06", b"cv07", b"cv08",
                            b"cv09",
                        ] {
                            font_features.enable(FeatureTag::new(tag));
                        }
                        let attrs = Attrs::new()
                            .family(Family::Name(actual_family.as_str()))
                            .font_features(font_features);

                        let text = grid_rows.join("\n");
                        buffer.set_text(&text, &attrs, Shaping::Advanced, None);
                        buffer.shape_until_scroll(true);

                        buffer.draw(swash_cache, TEXT_COLOR, |x, y, w, h, color| {
                            if color.a() == 0 {
                                return;
                            }
                            let pixel = image::Rgba([color.r(), color.g(), color.b(), color.a()]);
                            // The glyph-rasterization path always calls back
                            // with w == h == 1 (verified against cosmic-text
                            // 0.18.2's `LegacyRenderer::glyph`), but handle the
                            // general rect case for correctness regardless.
                            for off_y in 0..h {
                                let py = y + off_y as i32;
                                if py < 0 || py as u32 >= bitmap_height {
                                    continue;
                                }
                                for off_x in 0..w {
                                    let px_x = x + off_x as i32;
                                    if px_x < 0 || px_x as u32 >= bitmap_width {
                                        continue;
                                    }
                                    rgba_image.put_pixel(px_x as u32, py as u32, pixel);
                                }
                            }
                        });
                    });
                });

                // gpui's sprite atlas stores images as BGRA with straight
                // alpha (verified against gpui 0.2.2's `elements/img.rs`
                // decode path, which does the same swap for plain
                // RGBA-decoded images).
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
                // Explicitly free the previous frame's sprite-atlas entry —
                // gpui's atlas is insert-only otherwise, which is the root
                // cause of the leak this cache fixes.
                if let Some(old_frame) = old {
                    let _ = window.drop_image(old_frame.image);
                }

                image
            };

            let _ = window.paint_image(bounds, Corners::default(), render_image, 0, false);
        });

        // Cursor.
        let cursor = self.terminal.cursor_info();
        if cursor.visible {
            let cursor_origin = point(
                bounds.origin.x + self.cell_width * (cursor.col as f32),
                bounds.origin.y + self.cell_height * (cursor.row as f32),
            );
            window.paint_quad(fill(
                Bounds {
                    origin: cursor_origin,
                    size: size(self.cell_width, self.cell_height),
                },
                gpui::rgba(0xf8f8f280),
            ));
        }
    }
}
