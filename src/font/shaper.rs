use std::cell::RefCell;
use std::rc::Rc;

use cosmic_text::{
    fontdb, Attrs, AttrsList, Buffer, BufferLine, CacheKey, CacheKeyFlags, Family, FontSystem,
    LayoutGlyph, Metrics, Shaping, SwashCache,
};

use crate::config::schema::FontConfig;
use crate::font::freetype_lcd::{FreeTypeLcdRasterizer, LcdAtlasEntry};
use crate::renderer::atlas::{AtlasEntry, GlyphAtlas};
use crate::renderer::lcd_atlas::LcdGlyphAtlas;

// ── PUA detection ─────────────────────────────────────────────────────────────

/// Returns true for codepoints in the Unicode Private Use Areas or symbol ranges
/// used by Nerd Fonts that are not reliably declared in font OS/2 coverage bits.
///
/// BMP PUA (0xE000–0xF8FF) covers all Nerd Font icon blocks: Devicons, Font Awesome,
/// Font Logotypes, Seti-UI, Weather Icons, Powerline symbols, etc.
/// The supplementary PUA planes cover emoji-style icons in some icon fonts.
/// The additional symbol codepoints (Power, Octicons, Arrows) live outside PUA but
/// are commonly patched into Nerd Fonts and also lack OS/2 coverage bits.
#[inline]
fn is_pua(ch: char) -> bool {
    let c = ch as u32;
    matches!(c,
        0xE000..=0xF8FF   |  // BMP PUA — all Nerd Font icon blocks (Devicons, FA, Seti, etc.)
        0xF0000..=0xFFFFF |  // Supplementary PUA-A
        0x100000..=0x10FFFF| // Supplementary PUA-B
        0x23FB..=0x23FE |    // IEC Power Symbols
        0x2B58 |             // Heavy Circle (power symbol variant)
        0x2665 | 0x26A1 |    // Octicons heart / lightning
        0x2190..=0x2199 |    // Arrows block
        0x2714 | 0x2716 | 0x2728 | 0x2764 | // Heavy check/cross/sparkles/heart
        0x2B06..=0x2B07      // Up/down arrows
    )
}

#[inline]
fn should_use_lcd(cache_key: CacheKey, primary_font_id: fontdb::ID, ch: char) -> bool {
    cache_key.font_id == primary_font_id && ch.is_ascii()
}

/// Build an AttrsList where PUA codepoints get an explicit span forcing the
/// user's own font. Without this, cosmic-text may route PUA to a fallback that
/// doesn't exist (no system fonts loaded) and return glyph_id=0.
fn build_attr_list<'a>(text: &str, default_attrs: &'a Attrs<'a>, family: &'a str) -> AttrsList {
    let mut attr_list = AttrsList::new(default_attrs);
    let pua_attrs = default_attrs.clone().family(Family::Name(family));
    let mut byte_idx = 0;
    for ch in text.chars() {
        let ch_len = ch.len_utf8();
        if is_pua(ch) {
            attr_list.add_span(byte_idx..byte_idx + ch_len, &pua_attrs);
        }
        byte_idx += ch_len;
    }
    attr_list
}

// ── FreeType cmap lookup ──────────────────────────────────────────────────────

/// Minimal FreeType handle used only for direct cmap glyph-index lookup.
///
/// fontdb determines font coverage from the OS/2 Unicode Range bits. Nerd Font
/// patchers often don't set the PUA bit (0xE000-0xF8FF), so fontdb reports no
/// coverage and cosmic-text returns glyph_id=0 for PUA characters even when the
/// font actually has those glyphs.
///
/// FreeType's FT_Get_Char_Index reads the cmap directly, bypassing the OS/2
/// check. When cosmic-text gives us glyph_id=0 for a PUA char, we use this to
/// get the real glyph_id and construct the correct CacheKey so swash can
/// rasterize the actual icon.
struct FreeTypeCmapLookup {
    library: freetype::freetype::FT_Library,
    face: freetype::freetype::FT_Face,
}

impl FreeTypeCmapLookup {
    fn new(font_path: &std::path::Path, font_size: f32) -> Option<Self> {
        use freetype::freetype as ft;

        let mut library: ft::FT_Library = std::ptr::null_mut();
        let err = unsafe { ft::FT_Init_FreeType(&mut library) };
        if err != 0 || library.is_null() {
            log::warn!("PUA lookup: FT_Init_FreeType failed ({err})");
            return None;
        }

        let path_str = match font_path.to_str() {
            Some(s) => s,
            None => {
                unsafe { ft::FT_Done_FreeType(library) };
                return None;
            }
        };
        let c_path = match std::ffi::CString::new(path_str) {
            Ok(s) => s,
            Err(_) => {
                unsafe { ft::FT_Done_FreeType(library) };
                return None;
            }
        };

        let mut face: ft::FT_Face = std::ptr::null_mut();
        let err = unsafe { ft::FT_New_Face(library, c_path.as_ptr(), 0, &mut face) };
        if err != 0 || face.is_null() {
            unsafe { ft::FT_Done_FreeType(library) };
            log::warn!("PUA lookup: FT_New_Face failed ({err})");
            return None;
        }

        let err = unsafe { ft::FT_Set_Char_Size(face, 0, (font_size * 64.0) as ft::FT_F26Dot6, 72, 72) };
        if err != 0 {
            unsafe {
                ft::FT_Done_Face(face);
                ft::FT_Done_FreeType(library);
            }
            log::warn!("PUA lookup: FT_Set_Char_Size failed ({err})");
            return None;
        }

        log::debug!("FreeType cmap lookup ready for PUA glyph resolution.");
        Some(Self { library, face })
    }

    /// Returns the glyph index for `ch` from the font's cmap, or None if the
    /// character is not in the font.
    fn get_glyph_index(&self, ch: char) -> Option<u32> {
        use freetype::freetype as ft;
        let idx = unsafe { ft::FT_Get_Char_Index(self.face, ch as ft::FT_ULong) };
        if idx == 0 || idx > u16::MAX as u32 {
            None
        } else {
            Some(idx as u32)
        }
    }

    fn cell_metrics(&self) -> Option<(f32, f32)> {
        use freetype::freetype as ft;

        let size_metrics = unsafe {
            let size = (*self.face).size;
            if size.is_null() {
                return None;
            }
            (*size).metrics
        };

        let mut width = 0.0f32;
        for codepoint in 32u32..128u32 {
            let glyph_idx = unsafe { ft::FT_Get_Char_Index(self.face, codepoint as ft::FT_ULong) };
            if glyph_idx == 0 {
                continue;
            }

            let err = unsafe { ft::FT_Load_Glyph(self.face, glyph_idx, ft::FT_LOAD_DEFAULT as ft::FT_Int32) };
            if err != 0 {
                continue;
            }

            let advance = unsafe { (*(*self.face).glyph).metrics.horiAdvance } as f32 / 64.0;
            width = width.max(advance);
        }

        if width <= 0.0 {
            return None;
        }

        let height = size_metrics.height as f32 / 64.0;
        Some((width.round(), height.round()))
    }
}

impl Drop for FreeTypeCmapLookup {
    fn drop(&mut self) {
        use freetype::freetype as ft;
        unsafe {
            if !self.face.is_null() {
                ft::FT_Done_Face(self.face);
            }
            if !self.library.is_null() {
                ft::FT_Done_FreeType(self.library);
            }
        }
    }
}

// ── Public types ──────────────────────────────────────────────────────────────

/// A shaped text run ready for rendering.
#[derive(Debug, Clone)]
pub struct ShapedRun {
    pub glyphs: Vec<ShapedGlyph>,
    pub ascent: f32,
    pub line_height: f32,
}

#[derive(Debug, Clone)]
pub struct ShapedGlyph {
    pub col: usize,
    pub span: usize,
    pub ch: char,
    pub cache_key: CacheKey,
    pub advance: f32,
    pub bearing_x: f32,
    pub bearing_y: f32,
    pub fg: [f32; 4],
    pub bg: [f32; 4],
}

// ── TextShaper ────────────────────────────────────────────────────────────────

pub struct TextShaper {
    pub font_system: FontSystem,
    pub swash_cache: SwashCache,
    pub metrics: Metrics,
    pub cell_width: f32,
    pub cell_height: f32,
    shape_buf: Buffer,
    pub lcd_rasterizer: Option<FreeTypeLcdRasterizer>,
    pub lcd_atlas: Option<Rc<RefCell<LcdGlyphAtlas>>>,
    /// Queried family name (internal to the font file, may differ from config).
    family: String,
    /// fontdb face ID for the loaded font — used when overriding PUA glyph_ids.
    font_id: fontdb::ID,
    /// FreeType cmap lookup — always initialized (not just for LCD) to resolve
    /// PUA glyph_ids that cosmic-text can't find via fontdb coverage.
    ft_cmap: Option<FreeTypeCmapLookup>,
}

// SAFETY: TextShaper owns a FreeType library + face handle (via FreeTypeCmapLookup)
// and a FreeType-backed LCD rasterizer. FreeType's FT_Library is not thread-safe —
// concurrent use from multiple threads would be UB. However, TextShaper lives
// exclusively on the main (render) thread and is never aliased concurrently:
//   • It is stored inside RenderContext which is owned by App (main thread only).
//   • No Arc<TextShaper> or shared reference crosses thread boundaries.
//   • It is only moved across threads (e.g. into a tokio::spawn) via ownership
//     transfer, never while any other thread holds a reference.
// Given this single-owner invariant, moving TextShaper between threads is sound.
// Sync is intentionally NOT implemented: a shared &TextShaper from multiple threads
// could lead to concurrent FreeType calls which would be unsound.
unsafe impl Send for TextShaper {}

impl TextShaper {
    pub fn new(
        device: &wgpu::Device,
        font_system: FontSystem,
        actual_family: String,
        font_id: fontdb::ID,
        font_path: std::path::PathBuf,
        font_config: &FontConfig,
        lcd_atlas: Option<Rc<RefCell<LcdGlyphAtlas>>>,
    ) -> Self {
        let line_height = font_config.size * font_config.line_height;
        let metrics = Metrics::new(font_config.size, line_height);

        let mut font_system = font_system;
        let shape_buf = Buffer::new(&mut font_system, metrics);

        let lcd_rasterizer = if font_config.lcd_antialiasing {
            if let Some(atlas) = &lcd_atlas {
                match FreeTypeLcdRasterizer::new(device, font_config, Rc::clone(atlas)) {
                    Ok(r) => {
                        log::info!("LCD subpixel AA enabled via FreeType");
                        Some(r)
                    }
                    Err(e) => {
                        log::warn!("Failed to initialize FreeType LCD rasterizer: {e}. LCD AA disabled.");
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

        let ft_cmap = FreeTypeCmapLookup::new(&font_path, font_config.size);
        if ft_cmap.is_none() {
            log::warn!("FreeType cmap lookup unavailable — Nerd Font PUA icons may not render.");
        }

        let mut shaper = Self {
            font_system,
            swash_cache: SwashCache::new(),
            metrics,
            cell_width: font_config.size * 0.6,
            cell_height: line_height,
            shape_buf,
            lcd_rasterizer,
            lcd_atlas,
            family: actual_family,
            font_id,
            ft_cmap,
        };

        shaper.measure_cell(font_config);
        shaper
    }

    fn measure_cell(&mut self, font_config: &FontConfig) {
        if let Some((width, height)) = self.ft_cmap.as_ref().and_then(|ft| ft.cell_metrics()) {
            self.cell_width = width;
            self.cell_height = height.max((font_config.size * font_config.line_height).round());
            log::info!(
                "Cell size from FreeType: {:.1}x{:.1}px (font: '{}' {}pt, family: '{}')",
                self.cell_width,
                self.cell_height,
                font_config.family,
                font_config.size,
                self.family,
            );
            return;
        }

        let attrs = Self::make_attrs(&self.family, font_config);
        let mut buffer = Buffer::new(&mut self.font_system, self.metrics);
        buffer.set_size(&mut self.font_system, Some(1000.0), Some(1000.0));

        let attr_list = AttrsList::new(&attrs);
        let sample = "MMMMMMMMMMMMMMMM";
        buffer.lines = vec![BufferLine::new(
            sample,
            cosmic_text::LineEnding::None,
            attr_list,
            Shaping::Advanced,
        )];
        buffer.shape_until_scroll(&mut self.font_system, false);

        for run in buffer.layout_runs() {
            if run.glyphs.len() >= 2 {
                let mut total_advance = 0.0f32;
                let mut count = 0usize;
                for pair in run.glyphs.windows(2) {
                    total_advance += pair[1].x - pair[0].x;
                    count += 1;
                }
                if count > 0 {
                    self.cell_width = (total_advance / count as f32).round();
                }
            } else if let Some(glyph) = run.glyphs.first() {
                self.cell_width = glyph.w.round();
            }
            self.cell_height = run.line_height.round();
            break;
        }

        log::info!(
            "Cell size: {:.1}x{:.1}px (font: '{}' {}pt, family: '{}')",
            self.cell_width,
            self.cell_height,
            font_config.family,
            font_config.size,
            self.family,
        );
    }

    pub fn shape_line(
        &mut self,
        text: &str,
        colors: &[([f32; 4], [f32; 4])],
        font_config: &FontConfig,
    ) -> ShapedRun {
        let attrs = Self::make_attrs(&self.family, font_config);
        let attr_list = build_attr_list(text, &attrs, &self.family);

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
                let tlen = text.len();
                let start = glyph.start.min(tlen);
                let end = glyph.end.min(tlen);
                let col = text[..start].chars().count();
                let span = text[start..end].chars().count().max(1);
                let ch = text[start..end].chars().next().unwrap_or(' ');
                
                log::debug!("Shaping char '{}' (U+{:04X}), font_id: {:?}, glyph_id: {}", ch, ch as u32, glyph.font_id, glyph.glyph_id);

                let (fg, bg) = colors
                    .get(col)
                    .copied()
                    .unwrap_or(([1.0; 4], [0.0, 0.0, 0.0, 1.0]));

                // Use FreeType to resolve glyph IDs for Nerd Font symbols. 
                // Many Nerd Font patchers don't set the OS/2 Unicode Range bits, causing 
                // cosmic-text to return glyph_id=0 or fall back to system fonts (like Noto) 
                // that don't have the icon.
                //
                // We override the glyph if:
                // 1. cosmic-text returned glyph_id=0 (not found)
                // 2. The character is a Nerd Font symbol AND cosmic-text routed it to 
                //    a DIFFERENT font (fallback).
                let should_override = glyph.glyph_id == 0 || (is_pua(ch) && glyph.font_id != self.font_id);

                let cache_key = if should_override {
                    if let Some(real_id) = self.ft_cmap.as_ref().and_then(|ft| ft.get_glyph_index(ch)) {
                        log::debug!("Overriding glyph {} -> ID {}", ch, real_id);
                        let (key, _, _) = CacheKey::new(
                            self.font_id,
                            real_id as u16,
                            font_config.size,
                            (glyph.x.fract(), 0.0),
                            glyph.font_weight,
                            CacheKeyFlags::empty(),
                        );
                        key
                    } else {
                        log::debug!("No override for {} (not in cmap)", ch);
                        // Truly not in the font — use original key (will render .notdef or blank).
                        glyph_to_cache_key(glyph, font_config.size)
                    }
                } else {
                    glyph_to_cache_key(glyph, font_config.size)
                };

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

        ShapedRun { glyphs, ascent, line_height }
    }

    pub fn rasterize_to_atlas(
        &mut self,
        cache_key: CacheKey,
        atlas: &mut GlyphAtlas,
        queue: &wgpu::Queue,
    ) -> Result<AtlasEntry, crate::renderer::atlas::AtlasError> {
        if let Some(entry) = atlas.get(&cache_key) {
            return Ok(entry);
        }

        let image = self
            .swash_cache
            .get_image_uncached(&mut self.font_system, cache_key)
            .ok_or_else(|| {
                crate::renderer::atlas::AtlasError::Other(
                    "Swash failed to rasterize glyph".into(),
                )
            })?;

        let width = image.placement.width;
        let height = image.placement.height;

        if width == 0 || height == 0 {
            return Ok(AtlasEntry { uv: [0.0; 4], width: 0, height: 0, bearing_x: 0, bearing_y: 0, is_color: false, last_used: atlas.epoch });
        }

        let is_color = matches!(image.content, cosmic_text::SwashContent::Color);
        let rgba: Vec<u8> = match image.content {
            cosmic_text::SwashContent::Mask => {
                image.data.iter().flat_map(|&a| [a, a, a, 255u8]).collect()
            }
            cosmic_text::SwashContent::Color => image.data.to_vec(),
            cosmic_text::SwashContent::SubpixelMask => {
                image.data.iter().flat_map(|&a| [a, a, a, 255u8]).collect()
            }
        };

        atlas.upload(queue, cache_key, &rgba, width, height, image.placement.left, image.placement.top, is_color)
    }

    pub fn rasterize_lcd_to_atlas(
        &mut self,
        cache_key: CacheKey,
        ch: char,
        queue: &wgpu::Queue,
    ) -> Option<LcdAtlasEntry> {
        if !should_use_lcd(cache_key, self.font_id, ch) {
            return None;
        }

        let rasterizer = self.lcd_rasterizer.as_mut()?;
        rasterizer.rasterize(cache_key.glyph_id as u32, queue)
    }

    fn make_attrs<'a>(family: &'a str, _font_config: &FontConfig) -> Attrs<'a> {
        Attrs::new().family(Family::Name(family))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cosmic_text::Attrs;

    #[test]
    fn test_is_pua() {
        // ── BMP PUA (0xE000–0xF8FF) — covers all Nerd Font icon blocks ─────────
        assert!(is_pua('\u{e0a0}')); // Powerline branch icon
        assert!(is_pua('\u{f418}')); // Nerd Font git-branch
        assert!(is_pua('\u{e000}')); // BMP PUA lower bound
        assert!(is_pua('\u{f8ff}')); // BMP PUA upper bound

        // Blocks previously listed as separate ranges (all within BMP PUA).
        // Verifies the consolidation doesn't break coverage — TD-OP-02.
        assert!(is_pua('\u{e700}')); // Devicons lower bound
        assert!(is_pua('\u{e7c5}')); // Devicons upper bound
        assert!(is_pua('\u{f000}')); // Font Awesome lower bound
        assert!(is_pua('\u{f2e0}')); // Font Awesome upper bound
        assert!(is_pua('\u{e200}')); // Font Logotypes lower bound
        assert!(is_pua('\u{e2a9}')); // Font Logotypes upper bound
        assert!(is_pua('\u{e5fa}')); // Seti-UI lower bound
        assert!(is_pua('\u{e62b}')); // Seti-UI upper bound
        assert!(is_pua('\u{e300}')); // Weather lower bound
        assert!(is_pua('\u{e3e3}')); // Weather upper bound

        // ── Supplementary PUA planes ──────────────────────────────────────────
        assert!(is_pua('\u{F0000}')); // PUA-A lower bound
        assert!(is_pua('\u{FFFFF}')); // PUA-A upper bound

        // ── Symbol codepoints outside PUA (also patched by Nerd Fonts) ────────
        assert!(is_pua('\u{23fb}')); // IEC Power Symbol
        assert!(is_pua('\u{23fe}')); // IEC Power Symbol upper bound
        assert!(is_pua('\u{2b58}')); // Heavy circle / power variant
        assert!(is_pua('\u{2665}')); // Octicons heart ♥
        assert!(is_pua('\u{26a1}')); // Lightning bolt ⚡
        assert!(is_pua('\u{2190}')); // Arrow left ←
        assert!(is_pua('\u{2199}')); // Arrow lower-left ↙
        assert!(is_pua('\u{2714}')); // Heavy check ✔
        assert!(is_pua('\u{2716}')); // Heavy cross ✖
        assert!(is_pua('\u{2728}')); // Sparkles ✨
        assert!(is_pua('\u{2764}')); // Heavy heart ❤
        assert!(is_pua('\u{2b06}')); // Up arrow ⬆
        assert!(is_pua('\u{2b07}')); // Down arrow ⬇

        // ── Ordinary text — must NOT be flagged as PUA ────────────────────────
        assert!(!is_pua('A'));
        assert!(!is_pua(' '));
        assert!(!is_pua('α')); // U+03B1 — Greek letter, outside all ranges
        assert!(!is_pua('€')); // U+20AC — currency symbol
        assert!(!is_pua('\u{D7FF}')); // Highest non-surrogate BMP char below PUA
        assert!(!is_pua('ñ')); // U+00F1 — Latin extended, outside all ranges
        // Note: arrows 0x2190–0x2199 ARE in the override list (Nerd Font patches);
        // U+2192 → IS flagged intentionally.
        assert!(is_pua('→')); // U+2192 — in the patched arrows range
    }

    #[test]
    fn test_build_attr_list() {
        let family = "TestFont";
        let default_attrs = Attrs::new();
        let text = "A \u{e0a0} B";
        let list = build_attr_list(text, &default_attrs, family);
        
        // "A " is 2 bytes, "\u{e0a0}" is 3 bytes, " B" is 2 bytes
        // Total bytes: 7
        
        // We can't easily inspect the spans in AttrsList without shaping,
        // but we can at least verify it doesn't panic and the logic runs.
        assert_eq!(text.len(), 7);
    }

    #[test]
    fn test_should_use_lcd_only_for_ascii_in_primary_font() {
        let font_id = fontdb::ID::dummy();
        let (ascii_key, _, _) = CacheKey::new(
            font_id,
            b'A' as u16,
            16.0,
            (0.0, 0.0),
            fontdb::Weight::NORMAL,
            CacheKeyFlags::empty(),
        );
        let (symbol_key, _, _) = CacheKey::new(
            font_id,
            0x21E1,
            16.0,
            (0.0, 0.0),
            fontdb::Weight::NORMAL,
            CacheKeyFlags::empty(),
        );
        assert!(should_use_lcd(ascii_key, font_id, 'A'));
        assert!(!should_use_lcd(symbol_key, font_id, '⇡'));
    }
}

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
