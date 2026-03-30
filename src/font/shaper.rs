use std::cell::RefCell;
use std::rc::Rc;

use cosmic_text::{
    Attrs, AttrsList, Buffer, BufferLine, CacheKey, CacheKeyFlags, Family, FontSystem, LayoutGlyph,
    Metrics, Shaping, SwashCache,
};

use crate::config::schema::FontConfig;
use crate::font::freetype_lcd::{FreeTypeLcdRasterizer, LcdAtlasEntry};
use crate::renderer::atlas::{AtlasEntry, GlyphAtlas};
use crate::renderer::lcd_atlas::LcdGlyphAtlas;

/// A shaped text run ready for rendering.
#[derive(Debug, Clone)]
pub struct ShapedRun {
    pub glyphs: Vec<ShapedGlyph>,
    pub ascent: f32,
    pub line_height: f32,
}

#[derive(Debug, Clone)]
pub struct ShapedGlyph {
    /// Column index in the terminal grid.
    pub col: usize,
    /// Number of terminal columns this glyph covers (>1 for ligatures / wide chars).
    pub span: usize,
    /// The UTF-8 character for this glyph (for LCD AA rasterization).
    pub ch: char,
    /// cosmic-text cache key for atlas lookup / rasterization.
    pub cache_key: CacheKey,
    /// X advance in pixels.
    pub advance: f32,
    /// X bearing within the cell.
    pub bearing_x: f32,
    /// Y bearing (baseline offset).
    pub bearing_y: f32,
    /// Foreground RGBA.
    pub fg: [f32; 4],
    /// Background RGBA.
    pub bg: [f32; 4],
}

/// Text shaper using cosmic-text + HarfBuzz.
pub struct TextShaper {
    pub font_system: FontSystem,
    pub swash_cache: SwashCache,
    pub metrics: Metrics,
    pub cell_width: f32,
    pub cell_height: f32,
    /// Reusable shaping buffer — avoids a Buffer allocation on every shape_line call.
    shape_buf: Buffer,
    /// FreeType LCD rasterizer (holds Rc clone of lcd_atlas), available when lcd_antialiasing is enabled.
    pub lcd_rasterizer: Option<FreeTypeLcdRasterizer>,
    /// LCD glyph atlas (Rc clone, shared with GpuRenderer via set_lcd_atlas).
    pub lcd_atlas: Option<Rc<RefCell<LcdGlyphAtlas>>>,
}

unsafe impl Send for TextShaper {}
unsafe impl Sync for TextShaper {}

impl TextShaper {
    pub fn new(
        device: &wgpu::Device,
        font_system: FontSystem,
        font_config: &FontConfig,
        lcd_atlas: Option<Rc<RefCell<LcdGlyphAtlas>>>,
    ) -> Self {
        let line_height = font_config.size * font_config.line_height;
        let metrics = Metrics::new(font_config.size, line_height);

        // Create the reusable buffer before moving font_system into the struct.
        let mut font_system = font_system;
        let shape_buf = Buffer::new(&mut font_system, metrics);

        // Create LCD rasterizer using the provided atlas (from GpuRenderer)
        let lcd_rasterizer = if font_config.lcd_antialiasing {
            if let Some(atlas) = &lcd_atlas {
                match FreeTypeLcdRasterizer::new(device, font_config, Rc::clone(atlas)) {
                    Ok(r) => {
                        log::info!("LCD subpixel AA enabled via FreeType");
                        Some(r)
                    }
                    Err(e) => {
                        log::warn!(
                            "Failed to initialize FreeType LCD rasterizer: {e}. LCD AA disabled."
                        );
                        None
                    }
                }
            } else {
                log::warn!("LCD AA enabled but no atlas provided. LCD AA disabled.");
                None
            }
        } else {
            None
        };

        let mut shaper = Self {
            font_system,
            swash_cache: SwashCache::new(),
            metrics,
            cell_width: font_config.size * 0.6,
            cell_height: line_height,
            shape_buf,
            lcd_rasterizer,
            lcd_atlas,
        };

        shaper.measure_cell(font_config);
        shaper
    }

    /// Measure the cell dimensions by shaping the reference character "M".
    fn measure_cell(&mut self, font_config: &FontConfig) {
        let attrs = Self::make_attrs(font_config);
        let mut buffer = Buffer::new(&mut self.font_system, self.metrics);
        buffer.set_size(&mut self.font_system, Some(1000.0), Some(1000.0));

        let attr_list = AttrsList::new(&attrs);
        buffer.lines = vec![BufferLine::new(
            "M",
            cosmic_text::LineEnding::None,
            attr_list,
            Shaping::Advanced,
        )];
        buffer.shape_until_scroll(&mut self.font_system, false);

        for run in buffer.layout_runs() {
            if let Some(glyph) = run.glyphs.first() {
                // Round to integer physical pixels so every column boundary
                // lands on an exact pixel, preventing sub-pixel seams between
                // adjacent cell background rects.
                self.cell_width = glyph.w.round();
            }
            self.cell_height = run.line_height.round();
            break;
        }

        log::info!(
            "Cell size: {:.1}x{:.1}px (font: '{}' {}pt)",
            self.cell_width,
            self.cell_height,
            font_config.family,
            font_config.size
        );
    }

    /// Shape a line of terminal text into glyph runs.
    ///
    /// `text` — UTF-8 string of the terminal line.
    /// `colors` — per-column (fg, bg) RGBA pairs.
    pub fn shape_line(
        &mut self,
        text: &str,
        colors: &[([f32; 4], [f32; 4])],
        font_config: &FontConfig,
    ) -> ShapedRun {
        let attrs = Self::make_attrs(font_config);
        let attr_list = AttrsList::new(&attrs);

        // Reuse the stored buffer: replace lines and re-shape in-place.
        // This avoids a Buffer heap allocation on every call (~5 000–7 000/s at 60 fps).
        self.shape_buf
            .set_size(&mut self.font_system, None, Some(self.cell_height));
        self.shape_buf.lines = vec![BufferLine::new(
            text,
            cosmic_text::LineEnding::None,
            attr_list,
            Shaping::Advanced,
        )];
        self.shape_buf
            .shape_until_scroll(&mut self.font_system, false);

        let mut glyphs = Vec::new();
        let mut ascent = 0.0f32;
        let mut line_height = self.cell_height;

        for run in self.shape_buf.layout_runs() {
            ascent = run.line_y;
            line_height = run.line_height;

            for glyph in run.glyphs {
                // Use the cluster's start byte index to determine the correct column.
                // glyph.x / cell_width can be unreliable for ligatures because
                // cosmic-text may report the position at the last char of the cluster.
                let tlen = text.len();
                let start = glyph.start.min(tlen);
                let end = glyph.end.min(tlen);
                let col = text[..start].chars().count();
                // span = number of terminal columns the cluster occupies (>=1).
                let span = text[start..end].chars().count().max(1);

                // Extract the first character from the glyph cluster for LCD AA.
                let ch = text[start..end].chars().next().unwrap_or(' ');

                let (fg, bg) = colors
                    .get(col)
                    .copied()
                    .unwrap_or(([1.0; 4], [0.0, 0.0, 0.0, 1.0]));

                let cache_key = glyph_to_cache_key(glyph, font_config.size);

                glyphs.push(ShapedGlyph {
                    col,
                    span,
                    ch,
                    cache_key,
                    advance: glyph.w,
                    bearing_x: glyph.x - (col as f32 * self.cell_width),
                    bearing_y: run.line_y,
                    fg,
                    bg,
                });
            }
        }

        ShapedRun {
            glyphs,
            ascent,
            line_height,
        }
    }

    /// Rasterize a glyph via swash and upload it to the GPU atlas.
    /// Returns the atlas entry, or None if the glyph has no visual representation.
    pub fn rasterize_to_atlas(
        &mut self,
        cache_key: CacheKey,
        atlas: &mut GlyphAtlas,
        queue: &wgpu::Queue,
    ) -> Option<AtlasEntry> {
        if let Some(entry) = atlas.get(&cache_key) {
            return Some(entry);
        }

        let image = self
            .swash_cache
            .get_image_uncached(&mut self.font_system, cache_key)?;

        let width = image.placement.width;
        let height = image.placement.height;

        if width == 0 || height == 0 {
            return None;
        }

        // Convert swash image content to RGBA8.
        // Store coverage in R channel — the shader reads `.r` as the alpha mask.
        // Color glyphs (emoji) are stored as full RGBA with R=255, so `.r` = 1.0
        // and the fg/bg mix will render the glyph color via the atlas directly.
        let rgba: Vec<u8> = match image.content {
            cosmic_text::SwashContent::Mask => {
                // Grayscale mask: replicate coverage into all channels, full alpha.
                image.data.iter().flat_map(|&a| [a, a, a, 255u8]).collect()
            }
            cosmic_text::SwashContent::Color => image.data.to_vec(),
            cosmic_text::SwashContent::SubpixelMask => {
                // Treat subpixel as grayscale for now.
                image.data.iter().flat_map(|&a| [a, a, a, 255u8]).collect()
            }
        };

        atlas
            .upload(
                queue,
                cache_key,
                &rgba,
                width,
                height,
                image.placement.left,
                image.placement.top,
            )
            .ok()
    }

    /// Rasterize a character using the FreeType LCD rasterizer and upload to the LCD atlas.
    /// Returns the LCD atlas entry, or None if the character has no LCD representation.
    pub fn rasterize_lcd_to_atlas(
        &mut self,
        c: char,
        queue: &wgpu::Queue,
    ) -> Option<LcdAtlasEntry> {
        let rasterizer = self.lcd_rasterizer.as_mut()?;
        rasterizer.rasterize_char(c, queue)
    }

    fn make_attrs(font_config: &FontConfig) -> Attrs<'_> {
        Attrs::new().family(Family::Name(&font_config.family))
    }
}

/// Build a cosmic-text CacheKey from a layout glyph.
fn glyph_to_cache_key(glyph: &LayoutGlyph, font_size: f32) -> CacheKey {
    let (key, _, _) = CacheKey::new(
        glyph.font_id,
        glyph.glyph_id,
        font_size,
        (glyph.x.fract(), 0.0),
        glyph.font_weight,
        CacheKeyFlags::empty(),
    );
    key
}
