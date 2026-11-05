//! Glyph rasterization through swash's scaler.

use swash::scale::image::Content;
use swash::scale::{Render, ScaleContext, Source, StrikeWith};
use swash::zeno::{Angle, Format, Transform, Vector};

use crate::font::FaceInstance;

/// A rasterized glyph image, in device pixels.
#[derive(Debug, Clone)]
pub struct RasterizedGlyph {
    pub width: u32,
    pub height: u32,
    /// Offset from the pen to the left edge of the image.
    pub left: i32,
    /// Offset from the baseline to the top edge of the image (positive up).
    pub top: i32,
    pub is_color: bool,
    /// Alpha mask when `is_color` is false, RGBA8 otherwise.
    pub data: Vec<u8>,
}

pub(crate) fn rasterize(
    context: &mut ScaleContext,
    instance: &FaceInstance,
    glyph_id: u16,
    pixel_size: f32,
    offset_x: f32,
    offset_y: f32,
) -> Option<RasterizedGlyph> {
    if pixel_size <= 0.0 {
        return None;
    }
    let font = instance.data.font();
    let synthesis = instance.synthesis;

    let mut builder = context.builder(font).size(pixel_size).hint(true);
    let variations = synthesis.variations().to_vec();
    if !variations.is_empty() {
        builder = builder.variations(variations);
    }
    let mut scaler = builder.build();

    // Trying the color sources first is harmless for outline fonts and picks
    // up color outlines and bitmap emoji when available.
    let sources = [
        Source::ColorOutline(0),
        Source::ColorBitmap(StrikeWith::BestFit),
        Source::Outline,
    ];
    let mut renderer = Render::new(&sources);
    renderer.format(Format::Alpha);
    // Swash's y axis points up while device offsets point down.
    renderer.offset(Vector::new(offset_x, -offset_y));
    if synthesis.embolden() {
        renderer.embolden((pixel_size * 0.025).clamp(0.3, 2.5));
    }
    if let Some(skew) = synthesis.skew() {
        renderer.transform(Some(Transform::skew(
            Angle::from_degrees(skew),
            Angle::from_degrees(0.0),
        )));
    }

    let image = renderer.render(&mut scaler, glyph_id)?;
    if image.placement.width == 0 || image.placement.height == 0 {
        return None;
    }
    let mut data = image.data;
    let is_color = matches!(image.content, Content::Color);
    if is_color {
        // Swash returns BGRA for color glyphs; the atlas is RGBA.
        for pixel in data.chunks_mut(4) {
            pixel.swap(0, 2);
        }
    }
    Some(RasterizedGlyph {
        width: image.placement.width,
        height: image.placement.height,
        left: image.placement.left,
        top: image.placement.top,
        is_color,
        data,
    })
}
