// gpui chrome migration (M0 spike): paints one `term::Terminal`'s grid as a
// custom gpui `Element`.
//
// gpui's native text-shaping/painting facilities (`window.text_system()`,
// used here originally) do not produce ligatures at this gpui version, per
// real dogfood testing (not a config mistake — a confirmed limitation).
// Fallback: shape + rasterize the grid with cosmic-text (which already
// renders ligatures correctly in this project's existing wgpu renderer, see
// src/font/shaper.rs) into an RGBA bitmap, and paint that bitmap into gpui
// via `Window::paint_image`.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use cosmic_text::{
    Attrs, Buffer, Color as CosmicColor, Family, FeatureTag, FontFeatures, FontSystem, Metrics,
    Shaping, SwashCache, Wrap,
};
use gpui::{
    App, Bounds, Corners, Element, ElementId, GlobalElementId, InspectorElementId, IntoElement,
    LayoutId, Pixels, RenderImage, Style, Window, fill, point, size,
};
use image::{Frame, RgbaImage};

use crate::config::Config;
use crate::term::Terminal;

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
    static FONT_SYSTEM: RefCell<(FontSystem, String)> = RefCell::new({
        // `Config::default()`'s bare font family ("JetBrainsMono Nerd Font
        // Mono") is a code-level fallback, not what's actually configured —
        // the real app loads the user's Lua config (`config/default/*.lua`,
        // MonoLisaCode Nerd Font) at startup, which this M0 spike never
        // does. Override just the family/feature fields explicitly instead.
        let mut config = Config::default();
        config.font.family = "MonoLisaCode Nerd Font".into();
        let (font_system, actual_family, _face_id, _path, _face_index) =
            crate::font::loader::build_font_system(&config.font)
                .expect("load configured font for terminal ligature rendering");
        (font_system, actual_family)
    });
    static SWASH_CACHE: RefCell<SwashCache> = RefCell::new(SwashCache::new());
}

const FONT_SIZE: f32 = 14.0;
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
            let font_size_px = FONT_SIZE * scale;

            let mut rgba_image = RgbaImage::new(bitmap_width, bitmap_height);

            FONT_SYSTEM.with_borrow_mut(|(font_system, actual_family)| {
                SWASH_CACHE.with_borrow_mut(|swash_cache| {
                    let metrics = Metrics::new(font_size_px, cell_h_px);
                    let mut buffer = Buffer::new(font_system, metrics);
                    let mut buffer = buffer.borrow_with(font_system);
                    buffer.set_size(
                        Some(bitmap_width as f32),
                        Some(bitmap_height as f32),
                    );
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

            // gpui's sprite atlas stores images as BGRA with straight alpha
            // (verified against gpui 0.2.2's `elements/img.rs` decode path,
            // which does the same swap for plain RGBA-decoded images).
            for pixel in rgba_image.chunks_exact_mut(4) {
                pixel.swap(0, 2);
            }

            let render_image = RenderImage::new(smallvec::smallvec![Frame::new(rgba_image)]);
            let _ = window.paint_image(bounds, Corners::default(), Arc::new(render_image), 0, false);
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
