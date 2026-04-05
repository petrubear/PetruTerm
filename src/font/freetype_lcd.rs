use freetype::freetype as ft;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Mutex;

use crate::config::schema::FontConfig;
use crate::renderer::lcd_atlas::LcdGlyphAtlas;

#[derive(Debug, Clone, Copy)]
pub struct LcdAtlasEntry {
    pub uv: [f32; 4],
    pub width: u32,
    pub height: u32,
    pub bearing_x: i32,
    pub bearing_y: i32,
}

fn srgb8_to_linear(c: u8) -> f32 {
    let c = c as f32 / 255.0;
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

fn linear8_to_srgb(c: f32) -> u8 {
    let c = c.max(0.0).min(1.0);
    if c <= 0.0031308 {
        (c * 12.92 * 255.0).round() as u8
    } else {
        ((1.055 * c.powf(1.0 / 2.4) - 0.055) * 255.0).round() as u8
    }
}

pub struct FreeTypeLcdRasterizer {
    library: ft::FT_Library,
    face: ft::FT_Face,
    cache: Mutex<HashMap<u64, LcdAtlasEntry>>,
    lcd_atlas: Rc<RefCell<LcdGlyphAtlas>>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LcdPixelMode {
    Horizontal,
    Vertical,
}

impl FreeTypeLcdRasterizer {
    pub fn new(
        _device: &wgpu::Device,
        font_config: &FontConfig,
        lcd_atlas: Rc<RefCell<LcdGlyphAtlas>>,
    ) -> anyhow::Result<Self> {
        let mut library: ft::FT_Library = std::ptr::null_mut();
        let err = unsafe { ft::FT_Init_FreeType(&mut library) };
        if err != 0 || library.is_null() {
            return Err(anyhow::anyhow!("FT_Init_FreeType failed: {err}"));
        }

        let face = if let Some(ref font_path) = font_config.font_path {
            Self::load_face_from_file(library, font_path)?
        } else {
            unsafe { ft::FT_Done_FreeType(library) };
            return Err(anyhow::anyhow!(
                "Font '{}' could not be located for LCD AA. Set lcd_antialiasing=false or ensure the font is installed.",
                font_config.family
            ));
        };

        if face.is_null() {
            unsafe { ft::FT_Done_FreeType(library) };
            return Err(anyhow::anyhow!("Failed to load font face for LCD AA"));
        }

        unsafe {
            let err =
                ft::FT_Set_Char_Size(face, 0, (font_config.size * 64.0) as ft::FT_F26Dot6, 0, 0);
            if err != 0 {
                ft::FT_Done_Face(face);
                ft::FT_Done_FreeType(library);
                return Err(anyhow::anyhow!("FT_Set_Char_Size failed: {err}"));
            }
        }

        Ok(Self {
            library,
            face,
            cache: Mutex::new(HashMap::new()),
            lcd_atlas,
        })
    }

    fn load_face_from_file(
        library: ft::FT_Library,
        font_path: &std::path::Path,
    ) -> anyhow::Result<ft::FT_Face> {
        use std::ffi::CString;

        let path_str = font_path
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("Font path is not valid UTF-8: {:?}", font_path))?;
        let c_path = CString::new(path_str)
            .map_err(|e| anyhow::anyhow!("Font path contains null byte: {e}"))?;

        let mut face: ft::FT_Face = std::ptr::null_mut();
        let err = unsafe { ft::FT_New_Face(library, c_path.as_ptr(), 0, &mut face) };
        if err != 0 {
            anyhow::bail!("FT_New_Face failed for {:?}: {err}", font_path);
        }
        Ok(face)
    }

    pub fn rasterize(&mut self, glyph_id: u32, queue: &wgpu::Queue) -> Option<LcdAtlasEntry> {
        let cache_key = glyph_id as u64;

        if let Some(cached) = self.cache.lock().unwrap().get(&cache_key) {
            return Some(*cached);
        }

        if let Some(entry) = self.lcd_atlas.borrow().get(cache_key) {
            self.cache.lock().unwrap().insert(cache_key, entry);
            return Some(entry);
        }

        unsafe {
            let err = ft::FT_Load_Glyph(
                self.face,
                glyph_id as ft::FT_UInt,
                ft::FT_LOAD_FORCE_AUTOHINT as ft::FT_Int32,
            );
            if err != 0 {
                log::warn!("FT_Load_Glyph failed for gid {}: {}", glyph_id, err);
                return None;
            }
        }

        let slot = unsafe { (*self.face).glyph };
        unsafe {
            let err = ft::FT_Render_Glyph(slot, ft::FT_Render_Mode::FT_RENDER_MODE_LCD);
            if err != 0 {
                log::warn!("FT_Render_Glyph failed for gid {}: {}", glyph_id, err);
                return None;
            }
        }

        let bitmap = unsafe { (*slot).bitmap };

        let width = bitmap.width;
        let height = bitmap.rows;
        let pitch = bitmap.pitch as i32;

        if width == 0 || height == 0 {
            return None;
        }

        let lcd_width = width / 3;
        let pixel_mode = bitmap.pixel_mode;

        if pixel_mode != ft::FT_Pixel_Mode::FT_PIXEL_MODE_LCD as ft::FT_Pixel_Mode as u8 {
            log::debug!(
                "Glyph {} rendered in pixel_mode {} (not LCD) — falling back to greyscale",
                glyph_id,
                pixel_mode
            );
            return None;
        }

        let rgba = unsafe { self.deinterleave_lcd(bitmap, lcd_width, height, pitch) };

        let bearing_x = unsafe { (*slot).bitmap_left as i32 };
        let bearing_y = unsafe { (*slot).bitmap_top as i32 };

        let entry = match self.lcd_atlas.borrow_mut().upload(
            queue, cache_key, &rgba, lcd_width, height, bearing_x, bearing_y,
        ) {
            Ok(e) => e,
            Err(e) => {
                log::warn!("LCD atlas upload failed: {}", e);
                return None;
            }
        };

        self.cache.lock().unwrap().insert(cache_key, entry);
        Some(entry)
    }

    pub fn rasterize_char(&mut self, c: char, queue: &wgpu::Queue) -> Option<LcdAtlasEntry> {
        let glyph_id = self.get_glyph_index(c)?;
        self.rasterize(glyph_id, queue)
    }

    unsafe fn deinterleave_lcd(
        &self,
        bitmap: ft::FT_Bitmap,
        width: u32,
        height: u32,
        pitch: i32,
    ) -> Vec<u8> {
        let buffer = bitmap.buffer;
        if buffer.is_null() {
            return vec![0u8; width as usize * height as usize * 4];
        }

        let mut rgba = vec![0u8; width as usize * height as usize * 4];

        for y in 0..height {
            for x in 0..width {
                let dest = ((y * width + x) * 4) as usize;
                let src_offset = ((y as isize) * (pitch as isize) + (x as isize) * 3) as usize;

                // Raw FreeType coverage for R, G, B subpixels.
                rgba[dest] = *buffer.offset(src_offset as isize);
                rgba[dest + 1] = *buffer.offset(src_offset as isize + 1);
                rgba[dest + 2] = *buffer.offset(src_offset as isize + 2);
                rgba[dest + 3] = 255;
            }
        }

        rgba
    }

    pub fn get_glyph_index(&self, c: char) -> Option<u32> {
        let char_code = c as ft::FT_ULong;
        let idx = unsafe { ft::FT_Get_Char_Index(self.face, char_code) };
        if idx == 0 {
            None
        } else {
            Some(idx)
        }
    }
}

impl Drop for FreeTypeLcdRasterizer {
    fn drop(&mut self) {
        if !self.face.is_null() {
            unsafe { ft::FT_Done_Face(self.face) };
        }
        if !self.library.is_null() {
            unsafe { ft::FT_Done_FreeType(self.library) };
        }
    }
}
