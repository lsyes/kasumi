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

use crate::atlas_shelf::{ATLAS_SIZE, AtlasEntry, GlyphKey, ShelfAllocator, expand_mask};

pub struct GlyphAtlas {
    texture: glow::Texture,
    entries: HashMap<GlyphKey, Option<AtlasEntry>>,
    images: HashMap<u64, Option<AtlasEntry>>,
    shelf: ShelfAllocator,
}

impl GlyphAtlas {
    pub fn new(gl: &glow::Context) -> anyhow::Result<Self> {
        let texture = create_texture(gl, ATLAS_SIZE)?;
        Ok(Self {
            texture,
            entries: HashMap::new(),
            images: HashMap::new(),
            shelf: ShelfAllocator::default(),
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
        let data = expand_mask(&raster);
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
        let region = self.shelf.allocate(width, height, is_color, left, top)?;
        unsafe {
            gl.bind_texture(glow::TEXTURE_2D, Some(self.texture));
            gl.pixel_store_i32(glow::UNPACK_ALIGNMENT, 1);
            gl.tex_sub_image_2d(
                glow::TEXTURE_2D,
                0,
                region.x as i32,
                region.y as i32,
                region.width as i32,
                region.height as i32,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                glow::PixelUnpackData::Slice(Some(data)),
            );
        }

        Some(region.entry)
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
