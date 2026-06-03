//! Glyph atlas — rasterizes font glyphs and packs them into a GPU texture.
//!
//! Uses `fontdue` to rasterize individual glyphs, then packs them into
//! a single texture atlas. Each glyph's UV coordinates are cached for
//! fast lookup during rendering.

use fontdue::{Font, FontSettings};
use std::collections::HashMap;

/// Default embedded font (will be replaced with configurable fonts).
const DEFAULT_FONT_BYTES: &[u8] = include_bytes!("../../assets/fonts/JetBrainsMono-Regular.ttf");

/// A single glyph's position in the atlas texture.
#[derive(Debug, Clone, Copy)]
pub struct GlyphInfo {
    /// X offset in the atlas (pixels).
    pub atlas_x: u32,
    /// Y offset in the atlas (pixels).
    pub atlas_y: u32,
    /// Width of the glyph bitmap.
    pub width: u32,
    /// Height of the glyph bitmap.
    pub height: u32,
    /// Horizontal offset for rendering.
    pub x_offset: f32,
    /// Vertical offset for rendering.
    pub y_offset: f32,
    /// Advance width (how far to move cursor after this glyph).
    pub advance: f32,
}

/// Manages glyph rasterization and atlas packing.
pub struct GlyphAtlas {
    font: Font,
    font_size: f32,
    /// Cached glyph info indexed by character.
    cache: HashMap<char, GlyphInfo>,
    /// Atlas pixel data (single channel, grayscale).
    atlas_data: Vec<u8>,
    /// Atlas width in pixels.
    pub atlas_width: u32,
    /// Atlas height in pixels.
    pub atlas_height: u32,
    /// Next free X position in current row.
    cursor_x: u32,
    /// Current row Y position.
    cursor_y: u32,
    /// Tallest glyph in current row.
    row_height: u32,
    /// Cell dimensions derived from font metrics.
    pub cell_width: f32,
    pub cell_height: f32,
    /// Whether the atlas texture needs re-uploading to GPU.
    pub dirty: bool,
    /// Font ascent (baseline distance from top of cell).
    pub ascent: f32,
}

impl GlyphAtlas {
    /// Create a new glyph atlas with the given font size.
    pub fn new(font_size: f32) -> Self {
        let font = Font::from_bytes(DEFAULT_FONT_BYTES, FontSettings::default())
            .expect("Failed to load embedded font");

        let atlas_width = 1024;
        let atlas_height = 1024;
        let atlas_data = vec![0u8; (atlas_width * atlas_height) as usize];

        // Calculate cell dimensions from a reference character
        let metrics = font.metrics('M', font_size);
        let cell_width = metrics.advance_width.ceil();
        let line_metrics = font.horizontal_line_metrics(font_size);
        let cell_height = if let Some(lm) = line_metrics {
            (lm.ascent - lm.descent + lm.line_gap).ceil()
        } else {
            font_size * 1.4
        };

        let ascent = if let Some(lm) = line_metrics {
            lm.ascent
        } else {
            font_size
        };

        Self {
            font,
            font_size,
            cache: HashMap::new(),
            atlas_data,
            atlas_width,
            atlas_height,
            cursor_x: 1, // 1px padding from edge
            cursor_y: 1,
            row_height: 0,
            cell_width,
            cell_height,
            dirty: true,
            ascent,
        }
    }

    /// Get glyph info for a character, rasterizing and caching if needed.
    pub fn get_glyph(&mut self, ch: char) -> GlyphInfo {
        if let Some(info) = self.cache.get(&ch) {
            return *info;
        }

        self.rasterize_glyph(ch)
    }

    /// Rasterize a single glyph and pack it into the atlas.
    fn rasterize_glyph(&mut self, ch: char) -> GlyphInfo {
        let (metrics, bitmap) = self.font.rasterize(ch, self.font_size);

        let glyph_w = metrics.width as u32;
        let glyph_h = metrics.height as u32;

        // Check if we need to wrap to next row
        if self.cursor_x + glyph_w + 1 > self.atlas_width {
            self.cursor_x = 1;
            self.cursor_y += self.row_height + 1;
            self.row_height = 0;
        }

        // If atlas is full, return a blank glyph (shouldn't happen with 1024x1024)
        if self.cursor_y + glyph_h + 1 > self.atlas_height {
            tracing::warn!("Glyph atlas is full, cannot fit character: {:?}", ch);
            return GlyphInfo {
                atlas_x: 0,
                atlas_y: 0,
                width: 0,
                height: 0,
                x_offset: 0.0,
                y_offset: 0.0,
                advance: self.cell_width,
            };
        }

        // Copy bitmap into atlas
        for row in 0..glyph_h {
            for col in 0..glyph_w {
                let src_idx = (row * glyph_w + col) as usize;
                let dst_idx =
                    ((self.cursor_y + row) * self.atlas_width + self.cursor_x + col) as usize;
                if src_idx < bitmap.len() && dst_idx < self.atlas_data.len() {
                    self.atlas_data[dst_idx] = bitmap[src_idx];
                }
            }
        }

        let info = GlyphInfo {
            atlas_x: self.cursor_x,
            atlas_y: self.cursor_y,
            width: glyph_w,
            height: glyph_h,
            x_offset: metrics.xmin as f32,
            // y_offset = distance from TOP of cell to TOP of glyph bitmap
            // ascent = baseline position from top of cell
            // ymin = distance from baseline to bottom of glyph (negative for descenders)
            // So top of glyph = ascent - (ymin + height)
            y_offset: self.ascent - (metrics.ymin as f32 + glyph_h as f32),
            advance: metrics.advance_width,
        };

        // Advance cursor
        self.cursor_x += glyph_w + 1; // 1px padding
        self.row_height = self.row_height.max(glyph_h);

        self.cache.insert(ch, info);
        self.dirty = true;

        info
    }

    /// Get the raw atlas pixel data.
    pub fn atlas_data(&self) -> &[u8] {
        &self.atlas_data
    }

    /// Mark atlas as uploaded (not dirty).
    pub fn mark_clean(&mut self) {
        self.dirty = false;
    }

    /// Pre-rasterize ASCII printable range for fast startup.
    pub fn preload_ascii(&mut self) {
        for ch in ' '..='~' {
            self.get_glyph(ch);
        }
    }
}
