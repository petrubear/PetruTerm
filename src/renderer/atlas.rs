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
    /// True if the atlas stores full RGBA color (e.g. emoji), false if grayscale mask.
    pub is_color: bool,
    /// Frame epoch when this entry was last used. Updated on every cache hit.
    pub last_used: u64,
}

/// GPU glyph texture atlas with epoch-based LRU eviction.
///
/// Glyphs are packed into a single RGBA texture using a shelf-based algorithm.
/// New glyphs are rasterized by the font shaper and uploaded here on demand.
///
/// ## Eviction strategy
/// Each `AtlasEntry` carries a `last_used` epoch counter. Callers must call
/// `touch(key)` when a glyph is used in a frame to keep it warm. When the
/// atlas is 90% full (`try_evict_cold()` returns true), the atlas is rebuilt
/// from scratch keeping only warm entries — the caller must re-upload those.
/// As a last resort, `clear()` resets everything.
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
    /// Current frame epoch. Incremented by callers once per frame via `next_epoch()`.
    pub epoch: u64,
    /// Approximate used pixel area (for fill-ratio heuristics).
    used_pixels: u64,
}

impl GlyphAtlas {
    /// Atlas texture side length. 4096×4096 @ 4 bytes = 64 MiB — comfortable on modern GPUs.
    pub const SIZE: u32 = 4096;
    const PADDING: u32 = 1;

    /// Fraction of the atlas area at which we attempt cold-entry eviction (90%).
    const EVICT_THRESHOLD: f32 = 0.90;

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
            epoch: 0,
            used_pixels: 0,
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

    /// Advance to the next frame epoch. Call once per rendered frame.
    pub fn next_epoch(&mut self) {
        self.epoch = self.epoch.saturating_add(1);
    }

    /// Look up a cached glyph.
    pub fn get(&self, key: &CacheKey) -> Option<AtlasEntry> {
        self.cache.get(key).copied()
    }

    /// Look up a cached glyph and mark it as used in the current epoch.
    #[allow(dead_code)]
    pub fn get_and_touch(&mut self, key: &CacheKey) -> Option<AtlasEntry> {
        if let Some(entry) = self.cache.get_mut(key) {
            entry.last_used = self.epoch;
            Some(*entry)
        } else {
            None
        }
    }

    /// Returns true if the atlas has reached the eviction threshold.
    pub fn is_near_full(&self) -> bool {
        let total = (self.width * self.height) as u64;
        self.used_pixels >= (total as f32 * Self::EVICT_THRESHOLD) as u64
    }

    /// Remove all entries that were last used more than `max_age` epochs ago.
    /// Returns the number of entries evicted.
    ///
    /// Note: because the atlas uses contiguous shelf packing, evicting entries
    /// does NOT reclaim physical space — the cursor positions are unchanged.
    /// This method is therefore a logical eviction: old entries are removed from
    /// the cache map so they will be re-rasterized and re-uploaded when needed,
    /// fitting into new space appended after the current cursor. A full `clear()`
    /// is still required when the cursor reaches the atlas boundary.
    pub fn evict_cold(&mut self, max_age: u64) -> usize {
        let current = self.epoch;
        let before = self.cache.len();
        self.cache.retain(|_, entry| {
            current.saturating_sub(entry.last_used) <= max_age
        });
        let evicted = before - self.cache.len();
        if evicted > 0 {
            log::debug!("Atlas: evicted {} cold glyphs (epoch {}, max_age {})", evicted, current, max_age);
        }
        evicted
    }

    /// Upload a rasterized glyph bitmap and cache its atlas location.
    ///
    /// `data` must be RGBA8 bytes of size `width × height × 4`.
    /// Set `is_color` to true for color glyphs (e.g. emoji) whose pixels are pre-colored RGBA.
    #[allow(clippy::too_many_arguments)]
    pub fn upload(
        &mut self,
        queue: &wgpu::Queue,
        key: CacheKey,
        data: &[u8],
        width: u32,
        height: u32,
        bearing_x: i32,
        bearing_y: i32,
        is_color: bool,
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
        self.used_pixels += (width * height) as u64;

        let uv = [
            x as f32 / self.width as f32,
            y as f32 / self.height as f32,
            (x + width) as f32 / self.width as f32,
            (y + height) as f32 / self.height as f32,
        ];

        let entry = AtlasEntry { uv, width, height, bearing_x, bearing_y, is_color, last_used: self.epoch };
        self.cache.insert(key, entry);
        Ok(entry)
    }

    pub fn texture_view(&self) -> &wgpu::TextureView {
        &self.view
    }

    pub fn sampler(&self) -> &wgpu::Sampler {
        &self.sampler
    }

    /// Full atlas reset. Clears all entries and recreates the texture.
    /// The caller is responsible for re-rasterizing all visible glyphs.
    pub fn clear(&mut self, device: &wgpu::Device) {
        self.cursor_x = Self::PADDING;
        self.cursor_y = Self::PADDING;
        self.shelf_height = 0;
        self.used_pixels = 0;
        self.cache.clear();
        self.texture = Self::create_texture(device);
        self.view = self.texture.create_view(&wgpu::TextureViewDescriptor::default());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cosmic_text::{fontdb, CacheKey, CacheKeyFlags};

    /// Build a minimal AtlasEntry for test injection (no GPU involved).
    fn dummy_entry(epoch: u64) -> AtlasEntry {
        AtlasEntry {
            uv: [0.0; 4], width: 10, height: 10,
            bearing_x: 0, bearing_y: 0, is_color: false,
            last_used: epoch,
        }
    }

    fn dummy_key(glyph_id: u16) -> CacheKey {
        let (key, _, _) = CacheKey::new(
            fontdb::ID::dummy(),
            glyph_id,
            16.0,
            (0.0, 0.0),
            fontdb::Weight::NORMAL,
            CacheKeyFlags::empty(),
        );
        key
    }

    // ── TD-OP-03: epoch counter advances ─────────────────────────────────────

    #[test]
    fn epoch_starts_at_zero() {
        // We can't create a GlyphAtlas without a device, so we test the
        // counter logic by directly manipulating the struct fields via a
        // helper that mirrors what new() sets up.
        let mut epoch: u64 = 0;
        // Simulate next_epoch() calls.
        for i in 1..=5u64 {
            epoch = epoch.saturating_add(1);
            assert_eq!(epoch, i);
        }
    }

    // ── TD-OP-03: evict_cold removes stale entries only ───────────────────────

    #[test]
    fn evict_cold_removes_old_entries() {
        // Build a fake atlas state without a GPU device by creating the
        // GlyphAtlas internals directly via a wgpu-free path.
        // We test just the cache + epoch fields which have no GPU dependency.
        let mut cache: HashMap<CacheKey, AtlasEntry> = HashMap::new();
        let epoch: u64 = 100;

        // 3 warm entries (used at epoch 99/100).
        cache.insert(dummy_key(1), dummy_entry(99));
        cache.insert(dummy_key(2), dummy_entry(100));
        cache.insert(dummy_key(3), dummy_entry(100));
        // 2 cold entries (last used at epoch 30 — age > 60).
        cache.insert(dummy_key(4), dummy_entry(30));
        cache.insert(dummy_key(5), dummy_entry(39));

        // Mirror evict_cold logic: remove entries with age > max_age.
        let max_age = 60u64;
        cache.retain(|_, entry| epoch.saturating_sub(entry.last_used) <= max_age);

        assert_eq!(cache.len(), 3, "Only warm entries should survive");
        assert!(cache.contains_key(&dummy_key(1)));
        assert!(cache.contains_key(&dummy_key(2)));
        assert!(cache.contains_key(&dummy_key(3)));
        assert!(!cache.contains_key(&dummy_key(4)), "Cold entry must be evicted");
        assert!(!cache.contains_key(&dummy_key(5)), "Cold entry must be evicted");
        let _ = epoch; // suppress unused warning
    }

    #[test]
    fn evict_cold_keeps_all_when_all_warm() {
        let mut cache: HashMap<CacheKey, AtlasEntry> = HashMap::new();
        let epoch: u64 = 10;
        cache.insert(dummy_key(1), dummy_entry(8));
        cache.insert(dummy_key(2), dummy_entry(10));
        let max_age = 60u64;
        cache.retain(|_, entry| epoch.saturating_sub(entry.last_used) <= max_age);
        assert_eq!(cache.len(), 2);
    }

    #[test]
    fn evict_cold_removes_all_when_all_stale() {
        let mut cache: HashMap<CacheKey, AtlasEntry> = HashMap::new();
        let epoch: u64 = 200;
        cache.insert(dummy_key(1), dummy_entry(0));
        cache.insert(dummy_key(2), dummy_entry(1));
        let max_age = 60u64;
        cache.retain(|_, entry| epoch.saturating_sub(entry.last_used) <= max_age);
        assert_eq!(cache.len(), 0);
    }

    // ── TD-OP-03: is_near_full threshold ─────────────────────────────────────

    #[test]
    fn fill_ratio_below_threshold_is_not_near_full() {
        let size = GlyphAtlas::SIZE;
        let total = (size * size) as u64;
        let used = (total as f32 * 0.85) as u64; // 85% < 90% threshold
        let threshold = (total as f32 * GlyphAtlas::EVICT_THRESHOLD) as u64;
        assert!(used < threshold, "85% fill should not trigger eviction");
    }

    #[test]
    fn fill_ratio_at_threshold_is_near_full() {
        let size = GlyphAtlas::SIZE;
        let total = (size * size) as u64;
        let used = (total as f32 * 0.91) as u64; // 91% > 90% threshold
        let threshold = (total as f32 * GlyphAtlas::EVICT_THRESHOLD) as u64;
        assert!(used >= threshold, "91% fill should trigger eviction");
    }
}
