//! The output of the paint pass: a flat, ordered list of primitives.
//!
//! One list preserves z-order across different primitive types, so the
//! renderer can upload the whole scene as a single instance buffer and issue
//! one draw call. Scene construction owns no GPU resources.

use crate::color::Color;
use crate::geometry::{Corners, Rect};
use crate::image::ImageData;
use crate::text::FontFaceId;
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq)]
pub struct RectPrimitive {
    pub rect: Rect,
    pub color: Color,
    pub border_color: Color,
    pub border_width: f32,
    pub radius: Corners,
    pub clip: Rect,
}

/// A glyph positioned by the paint pass. The atlas UV and final pixel rect are
/// resolved by the renderer, because both depend on the device scale factor and
/// on glyphs that were already cached.
#[derive(Debug, Clone, PartialEq)]
pub struct GlyphPrimitive {
    pub face: FontFaceId,
    pub glyph_id: u16,
    /// Font size in logical pixels.
    pub font_size: f32,
    /// Pen x relative to the window origin.
    pub x: f32,
    /// Baseline y relative to the window origin.
    pub baseline: f32,
    pub color: Color,
    pub clip: Rect,
}

/// A textured RGBA image. The backend uploads `image` once and reuses it across
/// frames using its content-derived id.
#[derive(Debug, Clone, PartialEq)]
pub struct ImagePrimitive {
    pub rect: Rect,
    pub image: Arc<ImageData>,
    pub opacity: f32,
    pub radius: Corners,
    pub clip: Rect,
}

#[derive(Debug, Clone, PartialEq)]
pub enum DrawItem {
    Rect(RectPrimitive),
    Glyph(GlyphPrimitive),
    Image(ImagePrimitive),
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct DrawList {
    pub items: Vec<DrawItem>,
}

impl DrawList {
    pub fn clear(&mut self) {
        self.items.clear();
    }

    pub fn push_rect(&mut self, rect: RectPrimitive) {
        if rect.rect.is_empty() || rect.color.a <= 0.0 {
            return;
        }
        self.items.push(DrawItem::Rect(rect));
    }

    pub fn push_glyph(&mut self, glyph: GlyphPrimitive) {
        if glyph.color.a <= 0.0 {
            return;
        }
        self.items.push(DrawItem::Glyph(glyph));
    }

    pub fn push_image(&mut self, image: ImagePrimitive) {
        if image.rect.is_empty() || image.opacity <= 0.0 {
            return;
        }
        self.items.push(DrawItem::Image(image));
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn rect_count(&self) -> usize {
        self.items
            .iter()
            .filter(|item| matches!(item, DrawItem::Rect(_)))
            .count()
    }

    pub fn glyph_count(&self) -> usize {
        self.items
            .iter()
            .filter(|item| matches!(item, DrawItem::Glyph(_)))
            .count()
    }

    pub fn image_count(&self) -> usize {
        self.items
            .iter()
            .filter(|item| matches!(item, DrawItem::Image(_)))
            .count()
    }
}
