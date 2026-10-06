//! Glyph atlas (Prompt 2.4): CPU-side packing + cache for the wgpu renderer.
//!
//! The atlas is a square RGBA texture (`ATLAS_SIZE` × `ATLAS_SIZE`, default
//! 2048) packed with a shelf algorithm. Glyphs are keyed by
//! `(char, bold, italic)`; rasterization itself is injected by the caller so
//! this module stays headless-testable (no font-kit/rusttype dependency in
//! unit tests — the renderer wires the real rasterizer).
//!
//! Layout: `pixels` is a flat RGBA buffer (`size * size * 4`); `insert`
//! copies the caller's bitmap into the packed rect and returns normalized UVs.
//! Upload to `wgpu::Texture` happens in `renderer.rs` via [`GlyphAtlas::pixels`]
//! + [`GlyphAtlas::take_dirty`].

use std::collections::HashMap;

/// Atlas edge length in pixels (Prompt 2.4 requirement: 2048×2048).
pub const ATLAS_SIZE: u32 = 2048;
/// Bytes per pixel (RGBA8).
pub const ATLAS_BPP: usize = 4;

/// What style variant a cached glyph was rasterized with.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GlyphKey {
    pub ch: char,
    pub bold: bool,
    pub italic: bool,
}

impl GlyphKey {
    pub fn new(ch: char, bold: bool, italic: bool) -> Self {
        Self { ch, bold, italic }
    }
}

/// Placement + metrics of one cached glyph.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GlyphEntry {
    /// Normalized UV rect `(u0, v0, u1, v1)`.
    pub uv: [f32; 4],
    /// Pixel rect `(x, y, w, h)` inside the atlas.
    pub rect: (u32, u32, u32, u32),
    /// Advance width in pixels (for future shaping; equals `w` for now).
    pub advance: u32,
}

impl GlyphEntry {
    /// Placeholder UV for a missing glyph (empty top-left texel).
    pub fn missing() -> Self {
        Self {
            uv: [0.0, 0.0, 0.0, 0.0],
            rect: (0, 0, 0, 0),
            advance: 0,
        }
    }
}

/// Atlas failures (user-actionable; surfaced as renderer errors upstream).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AtlasError {
    /// Glyph bitmap is larger than the whole atlas.
    #[error("glyph {0}x{1}px does not fit in a {2}px atlas")]
    TooLarge(u32, u32, u32),
    /// Atlas is full; caller should grow/clear or evict.
    #[error("glyph atlas is full")]
    Full,
    /// Bitmap length does not match `w*h` (or `w*h*4` for RGBA).
    #[error("glyph bitmap has wrong length")]
    BadBitmap,
}

/// Shelf-packed RGBA glyph atlas.
#[derive(Debug)]
pub struct GlyphAtlas {
    size: u32,
    pixels: Vec<u8>,
    entries: HashMap<GlyphKey, GlyphEntry>,
    cursor_x: u32,
    cursor_y: u32,
    row_height: u32,
    dirty: bool,
}

impl GlyphAtlas {
    /// Empty atlas with the default [`ATLAS_SIZE`].
    pub fn new() -> Self {
        Self::with_size(ATLAS_SIZE)
    }

    /// Empty atlas with a custom edge length (tests use small sizes).
    pub fn with_size(size: u32) -> Self {
        assert!(size > 0, "atlas size must be non-zero");
        Self {
            size,
            pixels: vec![0u8; size as usize * size as usize * ATLAS_BPP],
            entries: HashMap::new(),
            cursor_x: 0,
            cursor_y: 0,
            row_height: 0,
            dirty: false,
        }
    }

    /// Edge length in pixels.
    pub fn size(&self) -> u32 {
        self.size
    }

    /// Number of cached glyphs.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// `true` when no glyph is cached.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Raw RGBA pixels (row-major) for `queue.write_texture` uploads.
    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }

    /// Lookup without inserting.
    pub fn get(&self, key: &GlyphKey) -> Option<&GlyphEntry> {
        self.entries.get(key)
    }

    /// `true` after any insert/clear until [`GlyphAtlas::take_dirty`] runs.
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// Consume the dirty flag (renderer calls this after uploading).
    pub fn take_dirty(&mut self) -> bool {
        std::mem::take(&mut self.dirty)
    }

    /// Fraction of texels covered by packed rows (0.0–1.0).
    pub fn utilization(&self) -> f32 {
        if self.size == 0 {
            return 0.0;
        }
        let used_h = self.cursor_y + self.row_height;
        (used_h as f32 / self.size as f32).clamp(0.0, 1.0)
    }

    /// Insert a glyph bitmap.
    ///
    /// `bitmap` is either grayscale (`w*h` bytes, expanded to white RGBA
    /// with the byte as alpha) or RGBA (`w*h*4` bytes, copied verbatim).
    /// Zero-sized glyphs (e.g. space) are cached as UV-only entries.
    pub fn insert(
        &mut self,
        key: GlyphKey,
        bitmap: &[u8],
        width: u32,
        height: u32,
    ) -> Result<GlyphEntry, AtlasError> {
        if let Some(entry) = self.entries.get(&key) {
            return Ok(*entry);
        }
        if width == 0 || height == 0 {
            let entry = GlyphEntry {
                uv: [0.0, 0.0, 0.0, 0.0],
                rect: (0, 0, width, height),
                advance: width,
            };
            self.entries.insert(key, entry);
            return Ok(entry);
        }
        if width > self.size || height > self.size {
            return Err(AtlasError::TooLarge(width, height, self.size));
        }
        let gray_len = (width * height) as usize;
        let rgba_len = gray_len * ATLAS_BPP;
        let is_gray = bitmap.len() == gray_len;
        let is_rgba = bitmap.len() == rgba_len;
        if !is_gray && !is_rgba {
            return Err(AtlasError::BadBitmap);
        }

        let (x, y) = self.pack(width, height).ok_or(AtlasError::Full)?;
        self.blit(x, y, bitmap, width, height, is_gray);
        let inv = self.size as f32;
        let entry = GlyphEntry {
            uv: [
                x as f32 / inv,
                y as f32 / inv,
                (x + width) as f32 / inv,
                (y + height) as f32 / inv,
            ],
            rect: (x, y, width, height),
            advance: width,
        };
        self.entries.insert(key, entry);
        self.dirty = true;
        Ok(entry)
    }

    /// Get-or-rasterize: returns the cached entry or rasterizes via
    /// `rasterize` (which yields `(bitmap, w, h)`) and inserts it.
    pub fn ensure_glyph(
        &mut self,
        key: GlyphKey,
        rasterize: impl FnOnce() -> (Vec<u8>, u32, u32),
    ) -> Result<GlyphEntry, AtlasError> {
        if let Some(entry) = self.entries.get(&key) {
            return Ok(*entry);
        }
        let (bitmap, w, h) = rasterize();
        self.insert(key, &bitmap, w, h)
    }

    /// Drop every glyph and zero the texture (e.g. on font-size change).
    pub fn clear(&mut self) {
        self.entries.clear();
        self.pixels.fill(0);
        self.cursor_x = 0;
        self.cursor_y = 0;
        self.row_height = 0;
        self.dirty = true;
    }

    // -- internals ---------------------------------------------------------

    fn pack(&mut self, w: u32, h: u32) -> Option<(u32, u32)> {
        if self.cursor_x + w > self.size {
            self.cursor_x = 0;
            self.cursor_y += self.row_height;
            self.row_height = 0;
        }
        if self.cursor_y + h > self.size {
            return None;
        }
        let pos = (self.cursor_x, self.cursor_y);
        self.cursor_x += w;
        self.row_height = self.row_height.max(h);
        Some(pos)
    }

    fn blit(&mut self, x: u32, y: u32, bitmap: &[u8], w: u32, h: u32, is_gray: bool) {
        let stride = self.size as usize * ATLAS_BPP;
        for row in 0..h as usize {
            for col in 0..w as usize {
                let dst = (y as usize + row) * stride + (x as usize + col) * ATLAS_BPP;
                if is_gray {
                    let a = bitmap[row * w as usize + col];
                    self.pixels[dst] = 255;
                    self.pixels[dst + 1] = 255;
                    self.pixels[dst + 2] = 255;
                    self.pixels[dst + 3] = a;
                } else {
                    let src = (row * w as usize + col) * ATLAS_BPP;
                    self.pixels[dst..dst + ATLAS_BPP]
                        .copy_from_slice(&bitmap[src..src + ATLAS_BPP]);
                }
            }
        }
    }
}

impl Default for GlyphAtlas {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gray(w: u32, h: u32, v: u8) -> Vec<u8> {
        vec![v; (w * h) as usize]
    }

    #[test]
    fn insert_and_cache_hit() {
        let mut atlas = GlyphAtlas::with_size(64);
        let key = GlyphKey::new('A', false, false);
        let first = atlas.insert(key, &gray(8, 12, 200), 8, 12).unwrap();
        let second = atlas.insert(key, &gray(8, 12, 7), 8, 12).unwrap();
        assert_eq!(first, second, "second insert is a cache hit");
        assert_eq!(atlas.len(), 1);
        assert!(atlas.is_dirty());
        assert!(atlas.take_dirty());
        assert!(!atlas.is_dirty());
    }

    #[test]
    fn styles_are_distinct_keys() {
        let mut atlas = GlyphAtlas::with_size(64);
        atlas
            .insert(GlyphKey::new('A', false, false), &gray(8, 8, 1), 8, 8)
            .unwrap();
        atlas
            .insert(GlyphKey::new('A', true, false), &gray(8, 8, 1), 8, 8)
            .unwrap();
        assert_eq!(atlas.len(), 2);
    }

    #[test]
    fn shelf_packing_wraps_rows() {
        let mut atlas = GlyphAtlas::with_size(16);
        let a = atlas
            .insert(GlyphKey::new('a', false, false), &gray(8, 8, 1), 8, 8)
            .unwrap();
        let b = atlas
            .insert(GlyphKey::new('b', false, false), &gray(8, 8, 1), 8, 8)
            .unwrap();
        let c = atlas
            .insert(GlyphKey::new('c', false, false), &gray(8, 8, 1), 8, 8)
            .unwrap();
        assert_eq!(a.rect, (0, 0, 8, 8));
        assert_eq!(b.rect, (8, 0, 8, 8));
        assert_eq!(c.rect, (0, 8, 8, 8), "wrapped to the next shelf");
    }

    #[test]
    fn full_and_too_large_are_reported() {
        let mut atlas = GlyphAtlas::with_size(8);
        atlas
            .insert(GlyphKey::new('x', false, false), &gray(8, 8, 1), 8, 8)
            .unwrap();
        assert_eq!(
            atlas.insert(GlyphKey::new('y', false, false), &gray(8, 8, 1), 8, 8),
            Err(AtlasError::Full)
        );
        assert!(matches!(
            atlas.insert(GlyphKey::new('z', false, false), &gray(9, 9, 1), 9, 9),
            Err(AtlasError::TooLarge(..))
        ));
        assert!(atlas
            .insert(GlyphKey::new('w', false, false), &[1, 2, 3], 8, 8)
            .is_err());
    }

    #[test]
    fn clear_resets_packing_and_pixels() {
        let mut atlas = GlyphAtlas::with_size(16);
        atlas
            .insert(GlyphKey::new('a', false, false), &gray(8, 8, 255), 8, 8)
            .unwrap();
        atlas.clear();
        assert!(atlas.is_empty());
        assert!(atlas.pixels().iter().all(|b| *b == 0));
        let entry = atlas
            .insert(GlyphKey::new('b', false, false), &gray(8, 8, 1), 8, 8)
            .unwrap();
        assert_eq!(entry.rect, (0, 0, 8, 8));
    }

    #[test]
    fn space_is_uv_only_and_utilization_grows() {
        let mut atlas = GlyphAtlas::with_size(32);
        assert_eq!(atlas.utilization(), 0.0);
        atlas
            .insert(GlyphKey::new(' ', false, false), &[], 0, 0)
            .unwrap();
        assert_eq!(atlas.len(), 1);
        atlas
            .insert(GlyphKey::new('M', false, false), &gray(16, 16, 1), 16, 16)
            .unwrap();
        assert!(atlas.utilization() > 0.0);
    }
}
