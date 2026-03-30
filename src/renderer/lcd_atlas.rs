use anyhow::Result;
use std::collections::HashMap;

use crate::font::freetype_lcd::LcdAtlasEntry;

/// GPU atlas for LCD subpixel glyphs (3× horizontal resolution).
///
/// Glyphs are packed identically to GlyphAtlas but use a separate texture
/// so LCD rendering can use a dedicated shader pipeline.
pub struct LcdGlyphAtlas {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    sampler: wgpu::Sampler,
    pub width: u32,
    pub height: u32,
    cursor_x: u32,
    cursor_y: u32,
    shelf_height: u32,
    cache: HashMap<u64, LcdAtlasEntry>,
}

impl LcdGlyphAtlas {
    pub const SIZE: u32 = 2048;
    const PADDING: u32 = 1;

    pub fn new(device: &wgpu::Device) -> Self {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("LCD glyph atlas"),
            size: wgpu::Extent3d {
                width: Self::SIZE,
                height: Self::SIZE,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("LCD glyph atlas sampler"),
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

    pub fn get(&self, key: u64) -> Option<LcdAtlasEntry> {
        self.cache.get(&key).copied()
    }

    pub fn upload(
        &mut self,
        queue: &wgpu::Queue,
        key: u64,
        data: &[u8],
        width: u32,
        height: u32,
        bearing_x: i32,
        bearing_y: i32,
    ) -> Result<LcdAtlasEntry> {
        let w = width + Self::PADDING * 2;
        let h = height + Self::PADDING * 2;

        if self.cursor_x + w > self.width {
            self.cursor_y += self.shelf_height + Self::PADDING;
            self.cursor_x = Self::PADDING;
            self.shelf_height = 0;
        }

        anyhow::ensure!(
            self.cursor_y + h <= self.height,
            "LCD glyph atlas is full — increase LcdGlyphAtlas::SIZE"
        );

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
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );

        self.cursor_x += w;
        self.shelf_height = self.shelf_height.max(h);

        let uv = [
            x as f32 / self.width as f32,
            y as f32 / self.height as f32,
            (x + width) as f32 / self.width as f32,
            (y + height) as f32 / self.height as f32,
        ];

        let entry = LcdAtlasEntry {
            uv,
            width,
            height,
            bearing_x,
            bearing_y,
        };
        self.cache.insert(key, entry);
        Ok(entry)
    }

    pub fn texture_view(&self) -> &wgpu::TextureView {
        &self.view
    }

    pub fn sampler(&self) -> &wgpu::Sampler {
        &self.sampler
    }

    pub fn clear(&mut self, device: &wgpu::Device) {
        self.cursor_x = Self::PADDING;
        self.cursor_y = Self::PADDING;
        self.shelf_height = 0;
        self.cache.clear();
        self.texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("LCD glyph atlas"),
            size: wgpu::Extent3d {
                width: Self::SIZE,
                height: Self::SIZE,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        self.view = self.texture.create_view(&wgpu::TextureViewDescriptor::default());
    }
}
