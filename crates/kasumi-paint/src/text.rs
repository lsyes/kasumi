//! Text service wrapper.
//!
//! Shaping, layout and rasterization live in `kasumi-text`. This module keeps
//! the engine behind one mutex so the runtime and renderer can share it, and
//! exposes a frame-scoped session that paints many text nodes with a single
//! lock acquisition.

use std::sync::{Arc, Mutex, MutexGuard};

use kasumi_core::{FontFaceId, Size, TextStyle};

use kasumi_text::TextSystem;
pub use kasumi_text::{RasterizedGlyph, ShapedGlyph, ShapedLine, TextCluster, TextLayout};

/// Shared text service.
pub struct TextEngine {
    inner: Mutex<TextSystem>,
}

pub(crate) struct TextSession<'a> {
    inner: MutexGuard<'a, TextSystem>,
}

impl TextEngine {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(TextSystem::new()),
        }
    }

    /// Shapes `text`, returning a cached immutable layout.
    pub fn layout(&self, text: &str, style: &TextStyle, max_width: Option<f32>) -> Arc<TextLayout> {
        self.session().layout(text, style, max_width)
    }

    pub(crate) fn session(&self) -> TextSession<'_> {
        TextSession { inner: self.lock() }
    }

    fn lock(&self) -> MutexGuard<'_, TextSystem> {
        self.inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    pub fn measure(&self, text: &str, style: &TextStyle, max_width: Option<f32>) -> Size {
        self.layout(text, style, max_width).size
    }

    /// Number of distinct font instances registered so far.
    pub fn face_count(&self) -> usize {
        self.lock().face_count()
    }

    /// Rasterizes one glyph at `font_size * scale` device pixels.
    pub fn rasterize(
        &self,
        face: FontFaceId,
        glyph_id: u16,
        font_size: f32,
        scale: f32,
    ) -> Option<RasterizedGlyph> {
        self.rasterize_offset(face, glyph_id, font_size, scale, 0.0, 0.0)
    }

    /// Rasterizes one glyph with a fractional device-space offset.
    pub fn rasterize_offset(
        &self,
        face: FontFaceId,
        glyph_id: u16,
        font_size: f32,
        scale: f32,
        offset_x: f32,
        offset_y: f32,
    ) -> Option<RasterizedGlyph> {
        self.lock()
            .rasterize_offset(face, glyph_id, font_size, scale, offset_x, offset_y)
    }
}

impl Default for TextEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl kasumi_layout::TextMeasure for TextEngine {
    fn measure_text(&self, text: &str, style: &TextStyle, max_width: Option<f32>) -> Size {
        self.measure(text, style, max_width)
    }
}

impl TextSession<'_> {
    pub(crate) fn layout(
        &mut self,
        text: &str,
        style: &TextStyle,
        max_width: Option<f32>,
    ) -> Arc<TextLayout> {
        self.inner.layout(text, style, max_width)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shapes_and_rasterizes_latin() {
        let engine = TextEngine::new();
        let style = TextStyle::default().size(18.0);
        let layout = engine.layout("Hello, Kasumi", &style, None);
        assert!(!layout.glyphs.is_empty(), "no glyphs produced");
        assert!(layout.size.width > 20.0);
        assert!(layout.size.height >= 18.0);

        let glyph = layout.glyphs[0];
        let raster = engine
            .rasterize(glyph.face, glyph.glyph_id, glyph.font_size, 1.0)
            .expect("failed to rasterize glyph");
        assert!(raster.width > 0 && raster.height > 0);
        if !raster.is_color {
            assert_eq!(raster.data.len(), (raster.width * raster.height) as usize);
        }
    }

    #[test]
    fn cache_hits_return_same_arc() {
        let engine = TextEngine::new();
        let style = TextStyle::default().size(14.0);
        let a = engine.layout("cache me", &style, Some(200.0));
        let b = engine.layout("cache me", &style, Some(200.0));
        assert!(Arc::ptr_eq(&a, &b));
    }
}
