//! An experimental cross-platform text backend for Kasumi.
//!
//! `kasumi-text` replaces the cosmic-text layer that the framework originally
//! used. It is intentionally a small, UI-oriented engine rather than a
//! general-purpose typesetter:
//!
//! - `fontdb` discovers platform fonts and resolves family/weight/style;
//! - Unicode analysis and bidi levels come from `swash` and `unicode-bidi`;
//! - shaping is done by swash's cluster shaper with per-cluster fallback;
//! - line breaking uses UAX #14 and UAX #9 rule L2 reordering;
//! - rasterization uses swash, including color outlines and bitmap emoji.
//!
//! The engine takes `&mut self` for layout and rasterization. The caller owns
//! synchronization; `kasumi-paint` wraps it in a mutex and exposes a paint
//! session so a whole frame shares one lock.
//!
//! # Scope
//!
//! This round implements what the framework needs: horizontal text, word
//! wrapping, explicit line breaks, generic and named families, CJK and emoji
//! fallback, mixed-direction runs, glyph masks, color glyphs and the
//! caret/selection cluster map. Vertical writing, font collections selected
//! by PostScript name, and a convenient single-file font registration API are
//! future work; `with_database` already accepts a caller-built font database.
//!
//! # Performance
//!
//! No hand-written assembly is used yet. The hot paths are swash's shaper and
//! rasterizer, both already optimized; the parts this crate owns are the
//! allocation-light analysis, fallback and line-breaking passes. The
//! `examples/bench.rs` harness tracks the cost of cache hits, cache misses,
//! fallback-heavy paragraphs and rasterization so a future assembly feature
//! can be justified against measurements instead of speculation.

#![forbid(unsafe_code)]

mod analysis;
mod cache;
mod font;
mod layout;
mod raster;
mod shape;

use std::sync::Arc;

use kasumi_core::{FontFaceId, Size, TextStyle, TextWrap};
use swash::scale::ScaleContext;
use swash::shape::ShapeContext;

use crate::cache::LayoutCache;
use crate::font::FontStore;
use crate::shape::ShapedText;

pub use analysis::{
    BracketKind, BracketMatch, TextAnalysis, byte_at_column, column_of_byte, expand_tabs, line_end,
    line_start, next_grapheme_boundary, prev_grapheme_boundary, word_range_at,
};
pub use layout::{CaretPosition, ShapedGlyph, ShapedLine, TextCluster, TextLayout, TextPosition};
pub use raster::RasterizedGlyph;

/// Shared text engine: font database, shaper caches, rasterizer and layout
/// cache. All methods that shape or rasterize take `&mut self`.
pub struct TextSystem {
    fonts: FontStore,
    shaper: ShapeContext,
    scaler: ScaleContext,
    cache: LayoutCache,
}

impl TextSystem {
    /// Discovers system fonts and creates an empty layout/shaping cache.
    pub fn new() -> Self {
        Self::with_store(FontStore::new())
    }

    /// Creates an engine from a caller-built font database. Useful for tests,
    /// embedded fonts, or platforms where the application supplies its own
    /// font list.
    pub fn with_database(database: fontdb::Database) -> Self {
        Self::with_store(FontStore::from_db(database))
    }

    fn with_store(fonts: FontStore) -> Self {
        Self {
            fonts,
            shaper: ShapeContext::new(),
            scaler: ScaleContext::new(),
            cache: LayoutCache::default(),
        }
    }

    /// Shapes `text`, returning a cached immutable layout.
    pub fn layout(
        &mut self,
        text: &str,
        style: &TextStyle,
        max_width: Option<f32>,
    ) -> Arc<TextLayout> {
        let width = if style.wrap == TextWrap::Word {
            max_width.map(|width| width.max(1.0))
        } else {
            None
        };
        if let Some(layout) = self.cache.get(text, style, width) {
            return layout;
        }
        let layout = Arc::new(self.shape_text(text, style, width));
        self.cache.insert(text, style, width, layout.clone());
        layout
    }

    /// Measures a text run without exposing the layout.
    pub fn measure(&mut self, text: &str, style: &TextStyle, max_width: Option<f32>) -> Size {
        self.layout(text, style, max_width).size
    }

    /// Number of distinct font instances registered so far. Instances are
    /// created lazily, on first use.
    pub fn face_count(&self) -> usize {
        self.fonts.face_count()
    }

    /// Rasterizes one glyph at `font_size * scale` device pixels.
    ///
    /// The renderer includes the scale factor in its atlas key; this method
    /// itself performs no image caching.
    pub fn rasterize(
        &mut self,
        face: FontFaceId,
        glyph_id: u16,
        font_size: f32,
        scale: f32,
    ) -> Option<RasterizedGlyph> {
        self.rasterize_offset(face, glyph_id, font_size, scale, 0.0, 0.0)
    }

    /// Rasterizes one glyph with a fractional device-space offset.
    ///
    /// The offset is measured in device pixels with y pointing down, and is
    /// usually in `0..1`. It shifts the outline inside its pixel grid, which
    /// lets renderers keep glyph quads on whole device pixels without losing
    /// the fractional pen position.
    pub fn rasterize_offset(
        &mut self,
        face: FontFaceId,
        glyph_id: u16,
        font_size: f32,
        scale: f32,
        offset_x: f32,
        offset_y: f32,
    ) -> Option<RasterizedGlyph> {
        if !font_size.is_finite()
            || !scale.is_finite()
            || font_size <= 0.0
            || scale <= 0.0
            || !offset_x.is_finite()
            || !offset_y.is_finite()
        {
            return None;
        }
        let Self { fonts, scaler, .. } = self;
        let instance = fonts.instance(face)?;
        raster::rasterize(
            scaler,
            instance,
            glyph_id,
            font_size * scale,
            offset_x,
            offset_y,
        )
    }

    fn shape_text(&mut self, text: &str, style: &TextStyle, width: Option<f32>) -> TextLayout {
        let primary = self
            .fonts
            .primary(&style.family, style.weight.0, style.italic);
        let font_size = style.size.max(1.0);
        let line_height = style.line_height.unwrap_or(font_size * 1.32).max(1.0);

        let mut combined: Option<TextLayout> = None;
        let mut base = 0;
        for (logical_line, paragraph) in text.split('\n').enumerate() {
            let visible = paragraph.strip_suffix('\r').unwrap_or(paragraph);
            let mut part = if let Some(primary) = primary {
                let mut raw = ShapedText::default();
                shape::shape_paragraph(
                    &mut self.fonts,
                    &mut self.shaper,
                    visible,
                    base,
                    primary,
                    style,
                    &mut raw,
                );
                layout::build(&self.fonts, visible, base, style, width, raw)
            } else {
                TextLayout::blank(font_size, line_height)
            };
            for line in &mut part.lines {
                line.logical_line = logical_line;
                line.byte_start = line.byte_start.max(base);
                line.byte_end = line.byte_end.clamp(base, base + paragraph.len());
            }
            if let Some(combined) = &mut combined {
                combined.append(part);
            } else {
                combined = Some(part);
            }
            base += paragraph.len() + 1;
        }
        let mut layout = combined.unwrap_or_else(|| TextLayout::blank(font_size, line_height));
        layout.logical_line_starts = text
            .split('\n')
            .scan(0usize, |offset, paragraph| {
                let start = *offset;
                *offset += paragraph.len() + 1;
                Some(start)
            })
            .collect();
        layout
    }
}

impl Default for TextSystem {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kasumi_core::{FontFamily, FontWeight, TextAlign};

    fn engine() -> TextSystem {
        TextSystem::new()
    }

    fn style(size: f32) -> TextStyle {
        TextStyle::default().size(size)
    }

    #[test]
    fn shapes_latin_and_respects_metrics() {
        let mut engine = engine();
        let layout = engine.layout("Hello, Kasumi", &style(18.0), None);
        assert!(!layout.glyphs.is_empty(), "no glyphs produced");
        assert!(layout.lines.len() == 1);
        assert!(layout.size.width > 20.0);
        assert!(layout.size.height >= 18.0);
        assert!(layout.line_height > 0.0);
        assert!(layout.glyphs.iter().all(|glyph| glyph.glyph_id != 0));
        assert!(
            layout
                .clusters
                .iter()
                .all(|cluster| cluster.end > cluster.start)
        );
    }

    #[test]
    fn cache_hits_return_same_arc() {
        let mut engine = engine();
        let style = style(14.0);
        let a = engine.layout("cache me", &style, Some(200.0));
        let b = engine.layout("cache me", &style, Some(200.0));
        assert!(Arc::ptr_eq(&a, &b));
        let c = engine.layout("cache me", &style, Some(201.0));
        assert!(!Arc::ptr_eq(&a, &c));
    }

    #[test]
    fn tracking_is_measured_in_logical_pixels() {
        let mut engine = engine();
        let base = TextStyle::default().size(20.0);
        let tracked = TextStyle::default().size(20.0).letter_spacing(2.0);
        let plain = engine.layout("tracking", &base, None);
        let spaced = engine.layout("tracking", &tracked, None);
        assert!(
            spaced.size.width > plain.size.width + 10.0,
            "{} vs {}",
            spaced.size.width,
            plain.size.width
        );
    }

    #[test]
    fn wraps_at_width() {
        let mut engine = engine();
        let style = style(16.0);
        let wide = engine.layout("one two three four five six", &style, None);
        let narrow = engine.layout("one two three four five six", &style, Some(80.0));
        assert!(narrow.lines.len() > wide.lines.len());
        assert!(narrow.size.height > wide.size.height);
        assert!(narrow.lines.iter().all(|line| line.width <= 80.5));
    }

    #[test]
    fn explicit_newlines_create_blank_lines() {
        let mut engine = engine();
        let style = style(16.0);
        let layout = engine.layout("a\n\nb", &style, None);
        assert_eq!(layout.lines.len(), 3);
        let empty = engine.layout("", &style, None);
        assert_eq!(empty.lines.len(), 1);
        assert_eq!(empty.size.height, empty.line_height);
    }

    #[test]
    fn explicit_newlines_shift_glyph_baselines() {
        let mut engine = engine();
        let style = style(16.0);
        let text = "view(ui) rebuilds the description;\nthe tree, layout and state persist.";
        let layout = engine.layout(text, &style, None);
        assert_eq!(layout.lines.len(), 2);
        let first = &layout.lines[0];
        let second = &layout.lines[1];
        assert!(second.top > first.top);
        assert!(second.baseline > first.baseline);
        let first_baseline = layout.glyphs[first.glyph_start].baseline;
        let second_baseline = layout.glyphs[second.glyph_start].baseline;
        assert!(
            second_baseline > first_baseline + 1.0,
            "second paragraph glyphs were not shifted: {first_baseline} vs {second_baseline}"
        );
    }

    #[test]
    fn byte_hit_testing_handles_boundaries_and_word_wrap() {
        let mut engine = engine();
        let style = style(16.0);
        let layout = engine.layout("one two three", &style, None);
        assert_eq!(layout.byte_at_x("one two three", -10.0), 0);
        assert_eq!(
            layout.byte_at_x("one two three", 10_000.0),
            "one two three".len()
        );
        let wrapped = engine.layout("one two three four", &style, Some(50.0));
        assert!(wrapped.lines.len() > 1);
        let second_line_start = wrapped
            .clusters
            .iter()
            .find(|cluster| cluster.line == 1)
            .expect("second line has no clusters")
            .start;
        assert!(second_line_start > 0);
    }

    #[test]
    fn cjk_punctuation_uses_the_cjk_fallback() {
        let mut engine = engine();
        let layout = engine.layout("中文：。", &style(18.0), None);
        assert!(layout.glyphs.iter().all(|glyph| glyph.glyph_id != 0));
        let faces: std::collections::HashSet<_> =
            layout.glyphs.iter().map(|glyph| glyph.face).collect();
        assert_eq!(
            faces.len(),
            1,
            "CJK punctuation did not stay on the CJK fallback face"
        );
    }

    #[test]
    fn carriage_returns_keep_absolute_offsets() {
        let mut engine = engine();
        let text = "a\r\nb";
        let layout = engine.layout(text, &style(16.0), None);
        assert_eq!(layout.lines.len(), 2);
        let second = layout
            .clusters
            .iter()
            .find(|cluster| cluster.line == 1)
            .expect("no cluster on the second line");
        assert_eq!(second.start, 3);
        assert_eq!(second.end, 4);
    }

    #[test]
    fn cjk_fallback_produces_glyphs() {
        let mut engine = engine();
        let layout = engine.layout("Hello 世界", &style(18.0), None);
        assert!(layout.glyphs.len() > 5);
        assert!(
            layout.glyphs.iter().all(|glyph| glyph.glyph_id != 0),
            "missing glyphs: {:?}",
            layout
                .glyphs
                .iter()
                .filter(|glyph| glyph.glyph_id == 0)
                .count()
        );
        assert!(
            engine.face_count() >= 2,
            "CJK text did not select a fallback face"
        );
    }

    #[test]
    fn emoji_fallback_and_color_rasterization() {
        let mut engine = engine();
        let layout = engine.layout("hi 🌍", &style(20.0), None);
        let emoji = layout
            .glyphs
            .iter()
            .find(|glyph| glyph.glyph_id != 0 && glyph.face != layout.glyphs[0].face)
            .copied();
        let Some(emoji) = emoji else {
            // A system without an emoji font may render a fallback glyph.
            return;
        };
        let raster = engine
            .rasterize(emoji.face, emoji.glyph_id, emoji.font_size, 1.0)
            .expect("emoji did not rasterize");
        assert!(raster.width > 0 && raster.height > 0);
        if raster.is_color {
            assert_eq!(
                raster.data.len(),
                (raster.width * raster.height * 4) as usize
            );
        }
    }

    #[test]
    fn rtl_text_lays_out_and_reorders_visually() {
        let mut engine = engine();
        let layout = engine.layout("مرحبا بالعالم", &style(18.0), None);
        assert!(!layout.glyphs.is_empty());
        assert!(layout.size.width > 0.0);
        // Visual order starts at the left edge and increases.
        let mut x = f32::NEG_INFINITY;
        for glyph in &layout.glyphs {
            assert!(glyph.x >= x - 0.01, "glyph x went backwards");
            x = glyph.x;
        }
    }

    #[test]
    fn rtl_marks_overlay_their_base() {
        let mut engine = engine();
        let style = style(40.0);
        let layout = engine.layout("بَ", &style, None);
        let [base, mark, ..] = layout.glyphs.as_slice() else {
            // Some fonts expose this combination as a single composed glyph.
            return;
        };
        assert!(base.x.abs() < 0.01, "base pen moved: {}", base.x);
        assert!(
            mark.x > base.x,
            "RTL mark was not shifted back over its base: base.x={} mark.x={}",
            base.x,
            mark.x
        );
        assert!(
            mark.x < layout.size.width,
            "RTL mark fell outside the cluster: {} >= {}",
            mark.x,
            layout.size.width
        );
    }

    #[test]
    fn mixed_direction_runs_reorder_on_one_line() {
        let mut engine = engine();
        let layout = engine.layout("abc مرحبا 123", &style(18.0), None);
        assert!(!layout.glyphs.is_empty());
        assert_eq!(layout.lines.len(), 1);
        let mut x = f32::NEG_INFINITY;
        for glyph in &layout.glyphs {
            assert!(glyph.x >= x - 0.01, "glyph x went backwards");
            x = glyph.x;
        }
        let first = layout.clusters.iter().map(|cluster| cluster.start).min();
        assert_eq!(first, Some(0));
        assert!(layout.clusters.iter().any(|cluster| cluster.start >= 4));
    }

    #[test]
    fn named_and_monospace_families_shape() {
        let mut engine = engine();
        for family in [
            FontFamily::Monospace,
            FontFamily::Serif,
            FontFamily::Named("Noto Sans CJK SC".into()),
        ] {
            let layout = engine.layout("Kasumi 123", &style(16.0).family(family.clone()), None);
            assert!(!layout.glyphs.is_empty(), "{family}");
        }
        let bold = engine.layout(
            "Bold",
            &TextStyle::default().size(16.0).weight(FontWeight::BOLD),
            None,
        );
        assert!(!bold.glyphs.is_empty());
    }

    #[test]
    fn glyphs_carry_byte_ranges_for_highlighting() {
        let mut engine = engine();
        let text = "let x = 1;";
        let layout = engine.layout(text, &style(16.0).wrap(TextWrap::None), None);
        assert!(!layout.glyphs.is_empty());
        for glyph in &layout.glyphs {
            assert!(glyph.end > glyph.start, "glyph lost its byte range");
            assert!(glyph.end <= text.len());
            let slice = &text[glyph.start..glyph.end];
            assert!(!slice.is_empty());
        }
        let covered: Vec<usize> = layout
            .glyphs
            .iter()
            .flat_map(|glyph| glyph.start..glyph.end)
            .collect();
        assert!(
            covered.contains(&0) && covered.contains(&2),
            "glyph byte ranges lost"
        );
        assert!(
            layout
                .glyphs
                .iter()
                .any(|glyph| &text[glyph.start..glyph.end] == "l")
        );
    }

    #[test]
    fn logical_lines_and_visual_lines_are_both_available() {
        let mut engine = engine();
        let text = "alpha\nbeta\ngamma";
        let layout = engine.layout(text, &style(16.0).wrap(TextWrap::None), None);
        assert_eq!(layout.lines.len(), 3);
        assert_eq!(layout.logical_line_count(), 3);
        assert_eq!(layout.logical_line_starts, vec![0, 6, 11]);
        assert_eq!(layout.logical_line_range(1), 6..10);
        for (index, line) in layout.lines.iter().enumerate() {
            assert_eq!(line.logical_line, index);
            assert!(line.byte_end >= line.byte_start);
        }
        assert_eq!(layout.position_at(7).line, 1);
        assert_eq!(layout.position_at(7).column, 1);
        assert_eq!(layout.byte_at_position(1, 3), 9);
    }

    #[test]
    fn caret_and_hit_testing_are_line_aware() {
        let mut engine = engine();
        let text = "first line\nsecond line\nthird";
        let layout = engine.layout(text, &style(16.0).wrap(TextWrap::None), None);
        assert_eq!(layout.lines.len(), 3);
        let first_end = layout.visual_line_end(0);
        let second_start = layout.visual_line_start(1);
        assert_eq!(&text[first_end..second_start], "\n");
        let caret = layout.caret_at(second_start + 2);
        assert_eq!(caret.line, 1);
        assert!(caret.y > layout.caret_at(0).y);
        // A click on the second line must not snap back to the first.
        let byte = layout.byte_at_point(text, caret.x + 1.0, caret.y + 4.0);
        assert!(byte >= second_start && byte <= layout.visual_line_end(1));
        assert_eq!(layout.line_of_byte(second_start + 2), 1);
    }

    #[test]
    fn soft_wrapped_lines_share_a_logical_line() {
        let mut engine = engine();
        let text = "one two three four five six seven eight";
        let layout = engine.layout(text, &style(16.0), Some(90.0));
        assert!(layout.lines.len() > 1);
        assert_eq!(layout.logical_line_count(), 1);
        assert!(layout.lines.iter().all(|line| line.logical_line == 0));
        assert_eq!(layout.logical_line_range(0), 0..text.len());
        // Selection ranges from the whole document produce one rect per line.
        let rects = layout.selection_rects(0, text.len(), 90.0, TextAlign::Start);
        assert_eq!(rects.len(), layout.lines.len());
        assert!(rects.windows(2).all(|pair| pair[1].y() > pair[0].y()));
    }

    #[test]
    fn clusters_expose_geometry_and_logical_order() {
        let mut engine = engine();
        let text = "abc def";
        let layout = engine.layout(text, &style(16.0).wrap(TextWrap::None), None);
        let logical = layout.logical_clusters(0);
        assert_eq!(logical.len(), layout.clusters.len());
        assert!(logical.windows(2).all(|pair| pair[0].start < pair[1].start));
        assert!(logical.iter().all(|cluster| cluster.advance > 0.0));
        let space = logical.iter().find(|c| &text[c.start..c.end] == " ");
        assert!(space.is_some_and(|cluster| cluster.whitespace));
    }

    #[test]
    fn char_index_helpers_round_trip() {
        let text = "aé中😀z";
        for (index, (byte, _)) in text.char_indices().enumerate() {
            assert_eq!(TextLayout::char_index_of_byte(text, byte), index);
            assert_eq!(TextLayout::byte_of_char_index(text, index), byte);
        }
        assert_eq!(
            TextLayout::byte_of_char_index(text, text.chars().count()),
            text.len()
        );
    }

    #[test]
    fn rasterizes_glyphs_with_offsets() {
        let mut engine = engine();
        let layout = engine.layout("Wg", &style(20.0), None);
        let glyph = layout.glyphs[0];
        let base = engine
            .rasterize(glyph.face, glyph.glyph_id, glyph.font_size, 2.0)
            .expect("rasterize failed");
        assert!(base.width > 0 && base.height > 0);
        let shifted = engine
            .rasterize_offset(glyph.face, glyph.glyph_id, glyph.font_size, 2.0, 0.25, 0.5)
            .expect("offset rasterize failed");
        assert!(shifted.width > 0 && shifted.height > 0);
        assert!(
            engine
                .rasterize(glyph.face, glyph.glyph_id, 0.0, 1.0)
                .is_none()
        );
    }
}
