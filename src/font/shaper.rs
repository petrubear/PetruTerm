use std::cell::RefCell;
use std::collections::HashSet;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::num::NonZeroUsize;
use std::rc::Rc;

use lru::LruCache;

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

/// Returns `true` if `text` contains any byte that can participate in common
/// font ligatures (`calt`/`liga` features). Used to gate the ASCII fast path:
/// if none of these bytes are present, HarfBuzz cannot produce a different result
/// than a direct glyph-per-codepoint lookup, so we can skip it entirely.
#[inline]
fn has_ligature_chars(text: &str) -> bool {
    text.bytes().any(|b| matches!(b, b'=' | b'<' | b'>' | b'-' | b'|' | b'+' | b'*' | b'/' | b'~' | b'!' | b':' | b'.'))
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
    fn new(font_path: &std::path::Path, face_index: u32, font_size: f32) -> Option<Self> {
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
        let err = unsafe { ft::FT_New_Face(library, c_path.as_ptr(), face_index as ft::FT_Long, &mut face) };
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
    #[allow(dead_code)]
    pub line_height: f32,
}

#[derive(Debug, Clone)]
pub struct ShapedGlyph {
    pub col: usize,
    #[allow(dead_code)]
    pub span: usize,
    pub ch: char,
    pub cache_key: CacheKey,
    #[allow(dead_code)]
    pub advance: f32,
    #[allow(dead_code)]
    pub bearing_x: f32,
    #[allow(dead_code)]
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
    /// Reusable byte-offset → char-index map. Resized on demand; shrinks when capacity > 4x need.
    byte_to_col_buf: Vec<usize>,
    pub lcd_rasterizer: Option<FreeTypeLcdRasterizer>,
    pub lcd_atlas: Option<Rc<RefCell<LcdGlyphAtlas>>>,
    /// Queried family name (internal to the font file, may differ from config).
    family: String,
    /// fontdb face ID for the regular weight face that was loaded into FreeType.
    /// Used for CacheKey construction in PUA overrides and for the LCD filter.
    primary_font_id: fontdb::ID,
    /// All fontdb face IDs that belong to the primary font family (regular, bold,
    /// italic, etc.). A glyph is a true fallback only if its font_id is NOT in
    /// this set. Using the full family set avoids false-positive PUA overrides
    /// when cosmic-text picks a bold/italic variant of the same font.
    primary_face_ids: HashSet<fontdb::ID>,
    /// FreeType cmap lookup — always initialized (not just for LCD) to resolve
    /// PUA glyph_ids that cosmic-text can't find via fontdb coverage.
    ft_cmap: Option<FreeTypeCmapLookup>,
    /// Per-run shape cache: key is (xxhash(text_bytes), font_size_bits).
    /// Stores pre-shaped `ShapedRun`s so that common words (`fn`, `let`, etc.)
    /// hit this cache instead of re-entering HarfBuzz. Capped at 1024 entries
    /// with LRU eviction — evicts the least-recently-used entry instead of
    /// clearing everything (TD-MEM-05).
    word_cache: LruCache<(u64, u32), ShapedRun>,
    /// Direct cmap glyph-ID cache for the ASCII fast path.
    /// Maps ASCII codepoint → glyph_id (0 = not in font / fast-path unavailable).
    ascii_glyph_cache: [u32; 128],
    /// Whether the ASCII fast path has been initialized.
    ascii_glyph_cache_ready: bool,
}


impl TextShaper {
    pub fn new(
        device: Option<&wgpu::Device>,
        font_system: FontSystem,
        actual_family: String,
        font_id: fontdb::ID,
        font_path: std::path::PathBuf,
        face_index: u32,
        font_config: &FontConfig,
        lcd_atlas: Option<Rc<RefCell<LcdGlyphAtlas>>>,
    ) -> Self {
        let line_height = font_config.size * font_config.line_height;
        let metrics = Metrics::new(font_config.size, line_height);

        let mut font_system = font_system;
        let shape_buf = Buffer::new(&mut font_system, metrics);

        let lcd_rasterizer = if font_config.lcd_antialiasing {
            if let (Some(device), Some(atlas)) = (device, &lcd_atlas) {
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
                log::warn!("LCD AA enabled but no device/atlas provided. LCD AA disabled.");
                None
            }
        } else {
            None
        };

        let ft_cmap = FreeTypeCmapLookup::new(&font_path, face_index, font_config.size);
        if ft_cmap.is_none() {
            log::warn!("FreeType cmap lookup unavailable — Nerd Font PUA icons may not render.");
        }

        // Collect all face IDs that belong to the primary font family (regular, bold,
        // italic, bold-italic, etc.). This prevents false-positive PUA overrides when
        // cosmic-text picks a different weight/style of the same font.
        let primary_face_ids: HashSet<fontdb::ID> = {
            // The face we loaded (font_id) tells us the canonical family name as stored
            // in fontdb. We look up that name and collect all faces sharing it.
            let canonical_family = font_system
                .db()
                .face(font_id)
                .and_then(|f| f.families.first())
                .map(|(name, _)| name.clone())
                .unwrap_or_else(|| actual_family.clone());

            font_system
                .db()
                .faces()
                .filter(|face| {
                    face.families
                        .iter()
                        .any(|(name, _)| name.eq_ignore_ascii_case(&canonical_family))
                })
                .map(|face| face.id)
                .collect()
        };

        log::debug!(
            "Primary family '{}' covers {} fontdb face ID(s)",
            actual_family,
            primary_face_ids.len()
        );

        let mut shaper = Self {
            font_system,
            swash_cache: SwashCache::new(),
            metrics,
            cell_width: font_config.size * 0.6,
            cell_height: line_height,
            shape_buf,
            byte_to_col_buf: Vec::new(),
            lcd_rasterizer,
            lcd_atlas,
            family: actual_family,
            primary_font_id: font_id,
            primary_face_ids,
            ft_cmap,
            word_cache: LruCache::new(NonZeroUsize::new(1024).unwrap()),
            ascii_glyph_cache: [0u32; 128],
            ascii_glyph_cache_ready: false,
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

        if let Some(run) = buffer.layout_runs().next() {
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

    /// Populate `ascii_glyph_cache` using the FreeType cmap for printable ASCII.
    /// Called lazily on first use so the constructor stays fast.
    fn init_ascii_glyph_cache(&mut self) {
        self.ascii_glyph_cache_ready = true;
        let Some(ft) = self.ft_cmap.as_ref() else { return };
        for cp in 0x20u32..=0x7Eu32 {
            let ch = char::from(cp as u8);
            if let Some(id) = ft.get_glyph_index(ch) {
                self.ascii_glyph_cache[cp as usize] = id;
            }
        }
    }

    /// Try the ASCII fast path: bypass HarfBuzz entirely.
    ///
    /// Returns `Some(ShapedRun)` when:
    /// - `text` is pure ASCII
    /// - none of the bytes can form ligatures (so HarfBuzz would give the same result)
    /// - the FreeType cmap has glyph IDs for every character
    ///
    /// Returns `None` to signal "fall through to HarfBuzz".
    fn try_ascii_fast_path(
        &mut self,
        text: &str,
        colors: &[([f32; 4], [f32; 4])],
        font_config: &FontConfig,
    ) -> Option<ShapedRun> {
        if !text.is_ascii() || has_ligature_chars(text) {
            return None;
        }
        if !self.ascii_glyph_cache_ready {
            self.init_ascii_glyph_cache();
        }
        // Require ft_cmap to be present (if it's None, glyph IDs would all be 0).
        self.ft_cmap.as_ref()?;

        let font_size = font_config.size;
        let ascent = self.metrics.line_height * 0.8; // approximate; matches HarfBuzz closely
        let line_height = self.cell_height;

        let mut glyphs = Vec::with_capacity(text.len());

        for (col, ch) in text.chars().enumerate() {
            let cp = ch as u32;
            // Fast path only covers printable ASCII (0x20..=0x7E); for others fall back.
            if !(0x20..=0x7E).contains(&cp) {
                return None;
            }
            let glyph_id = self.ascii_glyph_cache[cp as usize];
            if glyph_id == 0 {
                return None; // character missing in cmap — fall back
            }

            let (key, _, _) = CacheKey::new(
                self.primary_font_id,
                glyph_id as u16,
                font_size,
                (0.0, 0.0), // no sub-pixel x offset in fast path
                fontdb::Weight::NORMAL,
                CacheKeyFlags::empty(),
            );

            let (fg, bg) = colors
                .get(col)
                .copied()
                .unwrap_or(([1.0; 4], [0.0, 0.0, 0.0, 1.0]));

            glyphs.push(ShapedGlyph {
                col,
                span: 1,
                ch,
                cache_key: key,
                advance: self.cell_width,
                bearing_x: 0.0,
                bearing_y: ascent,
                fg,
                bg,
            });
        }

        Some(ShapedRun { glyphs, ascent, line_height })
    }

    /// Compute a cheap hash for a word (or short text run) for `word_cache`.
    fn word_hash(text: &str, font_size_bits: u32) -> (u64, u32) {
        let mut h = DefaultHasher::new();
        text.hash(&mut h);
        (h.finish(), font_size_bits)
    }

    /// Insert a shaped run into `word_cache`. LRU eviction handles capacity automatically.
    fn word_cache_insert(&mut self, text: &str, font_size_bits: u32, run: ShapedRun) {
        let key = Self::word_hash(text, font_size_bits);
        self.word_cache.put(key, run);
    }

    pub fn shape_line(
        &mut self,
        text: &str,
        colors: &[([f32; 4], [f32; 4])],
        font_config: &FontConfig,
    ) -> ShapedRun {
        #[cfg(feature = "profiling")]
        let _span = tracing::info_span!("shape_line", len = text.len()).entered();

        // ── ASCII fast path ───────────────────────────────────────────────────
        // For pure-ASCII text with no ligature-trigger characters, skip HarfBuzz
        // entirely: look up glyph IDs directly from the FreeType cmap. Benchmarks
        // show ~3–4× speedup for typical terminal output (the common case).
        if let Some(run) = self.try_ascii_fast_path(text, colors, font_config) {
            return run;
        }

        // ── Per-word shape cache (slow path only) ─────────────────────────────
        // For lines that DO go through HarfBuzz, we check a word-level cache so
        // that repeated words (`fn`, `let`, `pub`, …) pay only one HarfBuzz call
        // across many rows. The cache stores geometry (glyph IDs + positions)
        // without colors; colors are re-applied on each hit.
        //
        // This is only attempted when the text is pure ASCII (but has ligature
        // chars) — non-ASCII lines are too diverse for high hit rates.
        if text.is_ascii() {
            if let Some(run) = self.try_word_cached_shape(text, colors, font_config) {
                return run;
            }
        }

        self.shape_line_harfbuzz(text, colors, font_config)
    }

    /// Word-level shape cache for ASCII-but-with-ligature-chars lines.
    ///
    /// Splits the line by spaces, shapes each token individually through HarfBuzz
    /// (with caching), then stitches the results into a single `ShapedRun`.
    ///
    /// Returns `None` if any token fails to shape cleanly (fall through to full-line HarfBuzz).
    fn try_word_cached_shape(
        &mut self,
        text: &str,
        colors: &[([f32; 4], [f32; 4])],
        font_config: &FontConfig,
    ) -> Option<ShapedRun> {
        let font_size_bits = font_config.size.to_bits();
        let cell_height = self.cell_height;

        // Collect token ranges: (col_start, &str) pairs.
        // We split by spaces so each token is a contiguous run of non-space chars.
        // Space cells are skipped (they produce no visible glyph; bg is handled elsewhere).
        let mut tokens: Vec<(usize, &str)> = Vec::new();
        let mut col = 0usize;
        for word in text.split(' ') {
            if !word.is_empty() {
                tokens.push((col, word));
            }
            col += word.len() + 1; // +1 for the space separator
        }

        // Check if all tokens are cached before doing any mutation.
        let all_cached = tokens.iter().all(|(_, word)| {
            self.word_cache.contains(&Self::word_hash(word, font_size_bits))
        });

        let mut all_glyphs: Vec<ShapedGlyph> = Vec::with_capacity(text.len());
        let mut ascent = 0.0f32;

        if all_cached {
            // Pure cache-hit path: assemble from cache only.
            for (col_offset, word) in &tokens {
                let key = Self::word_hash(word, font_size_bits);
                let cached = self.word_cache.get(&key)?;
                ascent = cached.ascent;
                for g in &cached.glyphs {
                    let abs_col = col_offset + g.col;
                    let (fg, bg) = colors
                        .get(abs_col)
                        .copied()
                        .unwrap_or(([1.0; 4], [0.0, 0.0, 0.0, 1.0]));
                    all_glyphs.push(ShapedGlyph {
                        col: abs_col,
                        span: g.span,
                        ch: g.ch,
                        cache_key: g.cache_key,
                        advance: g.advance,
                        bearing_x: g.bearing_x,
                        bearing_y: g.bearing_y,
                        fg,
                        bg,
                    });
                }
            }
            return Some(ShapedRun { glyphs: all_glyphs, ascent, line_height: cell_height });
        }

        // Mixed path: shape uncached tokens, use cache for the rest.
        // We need to collect what to insert after borrowing self mutably.
        let mut to_insert: Vec<(String, ShapedRun)> = Vec::new();

        for (col_offset, word) in &tokens {
            let key = Self::word_hash(word, font_size_bits);
            if let Some(cached) = self.word_cache.get(&key) {
                ascent = cached.ascent;
                for g in &cached.glyphs {
                    let abs_col = col_offset + g.col;
                    let (fg, bg) = colors
                        .get(abs_col)
                        .copied()
                        .unwrap_or(([1.0; 4], [0.0, 0.0, 0.0, 1.0]));
                    all_glyphs.push(ShapedGlyph {
                        col: abs_col,
                        span: g.span,
                        ch: g.ch,
                        cache_key: g.cache_key,
                        advance: g.advance,
                        bearing_x: g.bearing_x,
                        bearing_y: g.bearing_y,
                        fg,
                        bg,
                    });
                }
            } else {
                // Shape this word individually through HarfBuzz.
                // Use uniform dummy colors for caching (colors don't affect glyph geometry).
                let dummy_colors: Vec<([f32; 4], [f32; 4])> = vec![([1.0; 4], [0.0, 0.0, 0.0, 1.0]); word.len()];
                let word_run = self.shape_word_harfbuzz(word, &dummy_colors, font_config);
                ascent = word_run.ascent;

                // Apply real colors and adjusted col offsets.
                for g in &word_run.glyphs {
                    let abs_col = col_offset + g.col;
                    let (fg, bg) = colors
                        .get(abs_col)
                        .copied()
                        .unwrap_or(([1.0; 4], [0.0, 0.0, 0.0, 1.0]));
                    all_glyphs.push(ShapedGlyph {
                        col: abs_col,
                        span: g.span,
                        ch: g.ch,
                        cache_key: g.cache_key,
                        advance: g.advance,
                        bearing_x: g.bearing_x,
                        bearing_y: g.bearing_y,
                        fg,
                        bg,
                    });
                }
                to_insert.push((word.to_string(), word_run));
            }
        }

        // Insert newly shaped words into cache.
        for (word, run) in to_insert {
            self.word_cache_insert(&word, font_size_bits, run);
        }

        Some(ShapedRun { glyphs: all_glyphs, ascent, line_height: cell_height })
    }

    /// Shape a single word (no spaces) through HarfBuzz. Used by `try_word_cached_shape`.
    fn shape_word_harfbuzz(
        &mut self,
        word: &str,
        colors: &[([f32; 4], [f32; 4])],
        font_config: &FontConfig,
    ) -> ShapedRun {
        self.shape_line_harfbuzz(word, colors, font_config)
    }

    /// Full HarfBuzz shaping path for a single text run.
    fn shape_line_harfbuzz(
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

        // Precompute byte-offset → char-index map once (O(n)) to avoid O(n²) per-glyph scans.
        // Reuse the buffer across cache-miss calls. Shrink if capacity is >4x the current need
        // (e.g. after a very long line is no longer being shaped) to avoid retaining large allocations.
        let n = text.len();
        self.byte_to_col_buf.resize(n + 1, 0);
        if self.byte_to_col_buf.capacity() > (n + 1).max(256) * 4 {
            self.byte_to_col_buf.shrink_to((n + 1) * 2);
        }
        let mut char_idx = 0usize;
        for (byte_idx, _) in text.char_indices() {
            self.byte_to_col_buf[byte_idx] = char_idx;
            char_idx += 1;
        }
        self.byte_to_col_buf[n] = char_idx;
        let byte_to_col = &self.byte_to_col_buf;

        for run in self.shape_buf.layout_runs() {
            ascent = run.line_y;
            line_height = run.line_height;

            for glyph in run.glyphs {
                let tlen = text.len();
                let start = glyph.start.min(tlen);
                let end = glyph.end.min(tlen);
                let col = byte_to_col[start];
                let span = (byte_to_col[end] - col).max(1);
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
                let should_override = glyph.glyph_id == 0
                    || (is_pua(ch) && !self.primary_face_ids.contains(&glyph.font_id));

                let cache_key = if should_override {
                    if let Some(real_id) = self.ft_cmap.as_ref().and_then(|ft| ft.get_glyph_index(ch)) {
                        log::debug!("Overriding glyph {} -> ID {}", ch, real_id);
                        let (key, _, _) = CacheKey::new(
                            self.primary_font_id,
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
        if let Some(entry) = atlas.get_and_touch(&cache_key) {
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

    /// Clear the LCD rasterizer's local glyph cache.
    ///
    /// Must be called after `LcdGlyphAtlas::clear()` to prevent the rasterizer
    /// from returning cached UVs that now point into a destroyed/empty texture.
    pub fn clear_lcd_rasterizer_cache(&mut self) {
        if let Some(r) = self.lcd_rasterizer.as_mut() {
            r.clear_local_cache();
        }
    }

    pub fn rasterize_lcd_to_atlas(
        &mut self,
        cache_key: CacheKey,
        ch: char,
        queue: &wgpu::Queue,
    ) -> Option<LcdAtlasEntry> {
        if !should_use_lcd(cache_key, self.primary_font_id, ch) {
            return None;
        }

        let rasterizer = self.lcd_rasterizer.as_mut()?;
        rasterizer.rasterize(cache_key.glyph_id as u32, queue)
    }

    fn make_attrs<'a>(family: &'a str, _font_config: &FontConfig) -> Attrs<'a> {
        Attrs::new().family(Family::Name(family))
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
        let _list = build_attr_list(text, &default_attrs, family);
        
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

