//! Backend-neutral shelf allocator shared by the glow and wgpu atlases.

use kasumi_core::FontFaceId;

pub const ATLAS_SIZE: u32 = 2048;
const PADDING: u32 = 1;

/// Number of subpixel positions per axis: glyph outlines are rasterized on a
/// quarter-pixel grid.
pub const SUBPIXEL_STEPS: u8 = 4;

/// A quantized fractional device-space offset, in `1 / SUBPIXEL_STEPS` px.
///
/// Positioning a glyph quad on a whole device pixel is what keeps the atlas
/// sample 1:1 (and therefore crisp). The fractional part of the pen position
/// is folded into the rasterized outline instead, so the glyph still lands
/// where the layout put it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub(crate) struct Subpixel {
    pub x: u8,
    pub y: u8,
}

impl Subpixel {
    /// Splits a device-space coordinate into the whole pixel used for the quad
    /// origin and the quarter-pixel offset used for rasterization.
    ///
    /// Rounding up to the next whole pixel is folded into the returned whole
    /// coordinate, so the offset never equals `SUBPIXEL_STEPS`.
    pub fn split(device: f32) -> (i32, u8) {
        debug_assert!(device.is_finite(), "glyph position must be finite");
        let floor = device.floor();
        let mut step = ((device - floor) * f32::from(SUBPIXEL_STEPS)).round() as i32;
        let mut whole = floor as i32;
        if step >= i32::from(SUBPIXEL_STEPS) {
            step = 0;
            whole += 1;
        }
        (whole, step as u8)
    }

    /// Fractional offset in device pixels, with y pointing down.
    pub fn offset(self) -> (f32, f32) {
        let steps = f32::from(SUBPIXEL_STEPS);
        (f32::from(self.x) / steps, f32::from(self.y) / steps)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct GlyphKey {
    pub face: FontFaceId,
    pub glyph_id: u16,
    /// Device pixel size, quantized to 1/4 px.
    pub size_quarter: u32,
    pub subpixel: Subpixel,
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

impl AtlasEntry {
    /// Whole-pixel device-space origin of the glyph image.
    ///
    /// `whole_x` and `whole_y` are the values `Subpixel::split` returned for
    /// the pen position this entry was rasterized at. Both components are
    /// integers, which keeps the atlas sample 1:1 with the framebuffer.
    pub fn quad_origin(&self, whole_x: i32, whole_y: i32) -> (f32, f32) {
        (
            whole_x as f32 + self.left as f32,
            whole_y as f32 - self.top as f32,
        )
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subpixel_split_keeps_the_position_to_a_quarter_pixel() {
        let mut position = 0.0f32;
        for _ in 0..2000 {
            let (whole, step) = Subpixel::split(position);
            let error =
                (whole as f32 + f32::from(step) / f32::from(SUBPIXEL_STEPS) - position).abs();
            assert!(error <= 0.125 + f32::EPSILON, "{position}: {error}");
            assert!(step < SUBPIXEL_STEPS);
            position += 0.0137;
        }
    }

    #[test]
    fn subpixel_split_folds_rounded_up_pixels_into_the_origin() {
        assert_eq!(Subpixel::split(10.0), (10, 0));
        assert_eq!(Subpixel::split(10.13), (10, 1));
        assert_eq!(Subpixel::split(10.5), (10, 2));
        assert_eq!(Subpixel::split(10.99), (11, 0));
        assert_eq!(Subpixel::split(-0.13), (-1, 3));
    }

    #[test]
    fn subpixel_offsets_are_device_pixels_with_y_down() {
        let offset = Subpixel { x: 1, y: 3 }.offset();
        assert_eq!(offset, (0.25, 0.75));
    }

    #[test]
    fn glyph_quads_start_on_whole_pixels() {
        let entry = AtlasEntry {
            uv: [0.0; 4],
            width: 9,
            height: 12,
            left: -2,
            top: 8,
            is_color: false,
        };
        let (x, y) = entry.quad_origin(30, 40);
        assert_eq!((x, y), (28.0, 32.0));
        assert_eq!(x.fract(), 0.0);
        assert_eq!(y.fract(), 0.0);
    }
}
