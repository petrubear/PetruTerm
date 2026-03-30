use cosmic_text::CacheKey;
use std::collections::HashMap;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AtlasError {
    #[error("Glyph atlas is full")]
    Full,
    #[error("Atlas upload error: {0}")]
    Other(String),
}

/// Location of a glyph in the GPU atlas texture.
#[derive(Debug, Clone, Copy)]
pub struct AtlasEntry {
    /// UV rectangle [u_min, v_min, u_max, v_max] in [0.0, 1.0] space.
    pub uv: [f32; 4],
    /// Glyph width in pixels.
    pub width: u32,
    /// Glyph height in pixels.
    pub height: u32,
    /// Bearing X offset (pixels from cell left to glyph origin).
    pub bearing_x: i32,
    /// Bearing Y offset (pixels from cell top to glyph baseline).
    pub bearing_y: i32,
}

/// GPU glyph texture atlas.
///
/// Glyphs are packed into a single RGBA texture using a shelf-based algorithm.
/// New glyphs are rasterized by the font shaper and uploaded here on demand.
pub struct GlyphAtlas {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    sampler: wgpu::Sampler,
    pub width: u32,
    pub height: u32,
    cursor_x: u32,
    cursor_y: u32,
    shelf_height: u32,
    /// cosmic-text CacheKey → atlas location
    cache: HashMap<CacheKey, AtlasEntry>,
}

impl GlyphAtlas {
    /// Default atlas size: 2048×2048.
    pub const SIZE: u32 = 2048;
    const PADDING: u32 = 1;

    pub fn new(device: &wgpu::Device) -> Self {
        let texture = Self::create_texture(device);
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("glyph atlas sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        Self {
            texture,
            view,
            sampler,
            width: Self::SIZE,
            height: Self::SIZE,
            cursor_x: Self::PADDING,
            cursor_y: Self::PADDING,
            shelf_height: 0,
            cache: HashMap::new(),
        }
    }

    fn create_texture(device: &wgpu::Device) -> wgpu::Texture {
        device.create_texture(&wgpu::TextureDescriptor {
            label: Some("glyph atlas"),
            size: wgpu::Extent3d {
                width: Self::SIZE,
                height: Self::SIZE,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            // Non-sRGB: mask alpha values are linear coverage, not colors.
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        })
    }

    /// Look up a cached glyph.
    pub fn get(&self, key: &CacheKey) -> Option<AtlasEntry> {
        self.cache.get(key).copied()
    }

    /// Upload a rasterized glyph bitmap and cache its atlas location.
    ///
    /// `data` must be RGBA8 bytes of size `width × height × 4`.
    pub fn upload(
        &mut self,
        queue: &wgpu::Queue,
        key: CacheKey,
        data: &[u8],
        width: u32,
        height: u32,
        bearing_x: i32,
        bearing_y: i32,
    ) -> Result<AtlasEntry, AtlasError> {
        let w = width + Self::PADDING * 2;
        let h = height + Self::PADDING * 2;

        if self.cursor_x + w > self.width {
            self.cursor_y += self.shelf_height + Self::PADDING;
            self.cursor_x = Self::PADDING;
            self.shelf_height = 0;
        }

        if self.cursor_y + h > self.height {
            return Err(AtlasError::Full);
        }

        let x = self.cursor_x;
        let y = self.cursor_y;

        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.texture,
                mip_level: 0,
                origin: wgpu::Origin3d { x, y, z: 0 },
                aspect: wgpu::TextureAspect::All,
            },
            data,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(width * 4),
                rows_per_image: Some(height),
            },
            wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
        );

        self.cursor_x += w;
        self.shelf_height = self.shelf_height.max(h);

        let uv = [
            x as f32 / self.width as f32,
            y as f32 / self.height as f32,
            (x + width) as f32 / self.width as f32,
            (y + height) as f32 / self.height as f32,
        ];

        let entry = AtlasEntry { uv, width, height, bearing_x, bearing_y };
        self.cache.insert(key, entry);
        Ok(entry)
    }

    pub fn texture_view(&self) -> &wgpu::TextureView {
        &self.view
    }

    pub fn sampler(&self) -> &wgpu::Sampler {
        &self.sampler
    }

    /// Clear the atlas (e.g. on font config change).
    pub fn clear(&mut self, device: &wgpu::Device) {
        self.cursor_x = Self::PADDING;
        self.cursor_y = Self::PADDING;
        self.shelf_height = 0;
        self.cache.clear();
        self.texture = Self::create_texture(device);
        self.view = self.texture.create_view(&wgpu::TextureViewDescriptor::default());
    }
}
