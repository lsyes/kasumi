//! Backend-neutral shelf allocator shared by the glow and wgpu atlases.

use kasumi_core::FontFaceId;

pub const ATLAS_SIZE: u32 = 2048;
const PADDING: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct GlyphKey {
    pub face: FontFaceId,
    pub glyph_id: u16,
    /// Device pixel size, quantized to 1/4 px.
    pub size_quarter: u32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AtlasEntry {
    pub uv: [f32; 4],
    pub width: u32,
    pub height: u32,
    pub left: i32,
    pub top: i32,
    pub is_color: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct AtlasRegion {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
    pub entry: AtlasEntry,
}

#[derive(Debug, Default)]
pub struct ShelfAllocator {
    x: u32,
    y: u32,
    row_height: u32,
    warned_full: bool,
}

impl ShelfAllocator {
    pub fn allocate(
        &mut self,
        width: u32,
        height: u32,
        is_color: bool,
        left: i32,
        top: i32,
    ) -> Option<AtlasRegion> {
        let padded_width = width + PADDING * 2;
        let padded_height = height + PADDING * 2;
        if padded_width > ATLAS_SIZE || padded_height > ATLAS_SIZE {
            return None;
        }
        if self.x + padded_width > ATLAS_SIZE {
            self.x = 0;
            self.y += self.row_height;
            self.row_height = 0;
        }
        if self.y + padded_height > ATLAS_SIZE {
            if !self.warned_full {
                self.warned_full = true;
                eprintln!("kasumi: glyph atlas full; new glyphs will be skipped");
            }
            return None;
        }

        let pixel_x = self.x + PADDING;
        let pixel_y = self.y + PADDING;
        self.x += padded_width;
        self.row_height = self.row_height.max(padded_height);

        let size = ATLAS_SIZE as f32;
        Some(AtlasRegion {
            x: pixel_x,
            y: pixel_y,
            width,
            height,
            entry: AtlasEntry {
                uv: [
                    pixel_x as f32 / size,
                    pixel_y as f32 / size,
                    width as f32 / size,
                    height as f32 / size,
                ],
                width,
                height,
                left,
                top,
                is_color,
            },
        })
    }
}

pub fn expand_mask(raster: &kasumi_paint::RasterizedGlyph) -> Vec<u8> {
    let expected_rgba = raster.width as usize * raster.height as usize * 4;
    if raster.is_color && raster.data.len() >= expected_rgba {
        return raster.data[..expected_rgba].to_vec();
    }
    let mut expanded = Vec::with_capacity(expected_rgba);
    for &alpha in raster
        .data
        .iter()
        .take((raster.width * raster.height) as usize)
    {
        expanded.extend_from_slice(&[alpha, alpha, alpha, 0xFF]);
    }
    expanded
}
