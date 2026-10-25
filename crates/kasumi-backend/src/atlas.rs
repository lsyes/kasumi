//! Glyph atlas and shelf allocator.
//!
//! The atlas packs rasterized glyph masks and color emoji into one RGBA
//! texture so the whole frame can be drawn with a single sampler. The shelf
//! allocation and glyph-key scheme are adapted from gpui's glyph atlas
//! (Apache-2.0, see NOTICE).

use std::collections::HashMap;

use glow::HasContext;
use kasumi_core::{FontFaceId, ImageData};
use kasumi_paint::{RasterizedGlyph, TextEngine};

const ATLAS_SIZE: u32 = 2048;
const PADDING: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct GlyphKey {
    face: FontFaceId,
    glyph_id: u16,
    /// Device pixel size, quantized to 1/4 px.
    size_quarter: u32,
}

#[derive(Debug, Clone, Copy)]
pub struct AtlasEntry {
    pub uv: [f32; 4],
    pub width: u32,
    pub height: u32,
    pub left: i32,
    pub top: i32,
    pub is_color: bool,
}

pub struct GlyphAtlas {
    texture: glow::Texture,
    size: u32,
    x: u32,
    y: u32,
    row_height: u32,
    entries: HashMap<GlyphKey, Option<AtlasEntry>>,
    images: HashMap<u64, Option<AtlasEntry>>,
    warned_full: bool,
}

impl GlyphAtlas {
    pub fn new(gl: &glow::Context) -> anyhow::Result<Self> {
        let texture = create_texture(gl, ATLAS_SIZE)?;
        Ok(Self {
            texture,
            size: ATLAS_SIZE,
            x: 0,
            y: 0,
            row_height: 0,
            entries: HashMap::new(),
            images: HashMap::new(),
            warned_full: false,
        })
    }

    pub fn texture(&self) -> glow::Texture {
        self.texture
    }

    pub fn get_or_insert(
        &mut self,
        gl: &glow::Context,
        texts: &TextEngine,
        face: FontFaceId,
        glyph_id: u16,
        font_size: f32,
        scale: f32,
    ) -> Option<AtlasEntry> {
        let pixel_size = ((font_size * scale) * 4.0).round() / 4.0;
        if pixel_size < 1.0 {
            return None;
        }
        let key = GlyphKey {
            face,
            glyph_id,
            size_quarter: (pixel_size * 4.0).round() as u32,
        };
        if let Some(entry) = self.entries.get(&key) {
            return *entry;
        }

        let entry = texts
            .rasterize(face, glyph_id, pixel_size, 1.0)
            .and_then(|raster| self.insert(gl, raster));
        self.entries.insert(key, entry);
        entry
    }

    fn insert(&mut self, gl: &glow::Context, raster: RasterizedGlyph) -> Option<AtlasEntry> {
        let data = if raster.is_color {
            raster.data
        } else {
            let mut expanded = Vec::with_capacity((raster.width * raster.height * 4) as usize);
            for alpha in raster.data {
                expanded.extend_from_slice(&[alpha, alpha, alpha, 0xFF]);
            }
            expanded
        };
        self.insert_rgba(
            gl,
            raster.width,
            raster.height,
            &data,
            raster.is_color,
            raster.left,
            raster.top,
        )
    }

    /// Uploads an RGBA image once. Images are content-addressed by
    /// `ImageData::id`, so rebuilding a widget with the same pixels is cheap.
    pub fn get_or_insert_image(
        &mut self,
        gl: &glow::Context,
        image: &ImageData,
    ) -> Option<AtlasEntry> {
        if let Some(entry) = self.images.get(&image.id()) {
            return *entry;
        }
        let entry = self.insert_rgba(
            gl,
            image.width(),
            image.height(),
            image.pixels(),
            true,
            0,
            0,
        );
        self.images.insert(image.id(), entry);
        entry
    }

    #[allow(clippy::too_many_arguments)]
    fn insert_rgba(
        &mut self,
        gl: &glow::Context,
        width: u32,
        height: u32,
        data: &[u8],
        is_color: bool,
        left: i32,
        top: i32,
    ) -> Option<AtlasEntry> {
        let padded_width = width + PADDING * 2;
        let padded_height = height + PADDING * 2;
        if padded_width > self.size || padded_height > self.size {
            return None;
        }
        if self.x + padded_width > self.size {
            self.x = 0;
            self.y += self.row_height;
            self.row_height = 0;
        }
        if self.y + padded_height > self.size {
            // Rather than invalidating UVs already emitted into the current
            // draw list, drop the glyph and keep the existing atlas. 2048^2 is
            // enough for tens of thousands of distinct glyphs in practice.
            if !self.warned_full {
                self.warned_full = true;
                eprintln!("kasumi: glyph atlas full; new glyphs will be skipped");
            }
            return None;
        }

        let pixel_x = self.x + PADDING;
        let pixel_y = self.y + PADDING;
        unsafe {
            gl.bind_texture(glow::TEXTURE_2D, Some(self.texture));
            gl.pixel_store_i32(glow::UNPACK_ALIGNMENT, 1);
            gl.tex_sub_image_2d(
                glow::TEXTURE_2D,
                0,
                pixel_x as i32,
                pixel_y as i32,
                width as i32,
                height as i32,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                glow::PixelUnpackData::Slice(Some(data)),
            );
        }

        self.x += padded_width;
        self.row_height = self.row_height.max(padded_height);

        let size = self.size as f32;
        Some(AtlasEntry {
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
        })
    }
}

fn create_texture(gl: &glow::Context, size: u32) -> anyhow::Result<glow::Texture> {
    unsafe {
        let texture = gl
            .create_texture()
            .map_err(|error| anyhow::anyhow!("create atlas texture: {error}"))?;
        gl.bind_texture(glow::TEXTURE_2D, Some(texture));
        gl.tex_parameter_i32(
            glow::TEXTURE_2D,
            glow::TEXTURE_MIN_FILTER,
            glow::LINEAR as i32,
        );
        gl.tex_parameter_i32(
            glow::TEXTURE_2D,
            glow::TEXTURE_MAG_FILTER,
            glow::LINEAR as i32,
        );
        gl.tex_parameter_i32(
            glow::TEXTURE_2D,
            glow::TEXTURE_WRAP_S,
            glow::CLAMP_TO_EDGE as i32,
        );
        gl.tex_parameter_i32(
            glow::TEXTURE_2D,
            glow::TEXTURE_WRAP_T,
            glow::CLAMP_TO_EDGE as i32,
        );
        gl.tex_image_2d(
            glow::TEXTURE_2D,
            0,
            glow::RGBA8 as i32,
            size as i32,
            size as i32,
            0,
            glow::RGBA,
            glow::UNSIGNED_BYTE,
            glow::PixelUnpackData::Slice(None),
        );
        gl.bind_texture(glow::TEXTURE_2D, None);
        Ok(texture)
    }
}
