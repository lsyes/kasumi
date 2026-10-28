//! Text shaping, layout and glyph rasterization.
//!
//! Font discovery and shaping use `cosmic-text`; rasterization uses `swash`,
//! following the same split as gpui's text system (Apache-2.0, see NOTICE).
//! The shaping cache is keyed by text, shaping attributes and available width;
//! glyph images are left to the renderer's atlas, which owns the GPU texture.

use std::collections::HashMap;
use std::hash::{BuildHasherDefault, Hash, Hasher};
use std::sync::{Arc, Mutex, MutexGuard};

use cosmic_text::{Attrs, Buffer, Family, FontSystem, Metrics, Shaping, Style as CosmicStyle};
use kasumi_core::{FontFaceId, FontFamily, Size, TextAlign, TextStyle, TextWrap};
use swash::scale::image::Content;
use swash::scale::{Render, ScaleContext, Source, StrikeWith};
use swash::zeno::{Format, Vector};

/// One positioned glyph in a shaped run.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShapedGlyph {
    pub face: FontFaceId,
    pub glyph_id: u16,
    pub font_size: f32,
    /// Pen position relative to the text layout origin.
    pub x: f32,
    /// Baseline position relative to the text layout origin.
    pub baseline: f32,
}

/// A single laid-out line inside `TextLayout`.
#[derive(Debug, Clone, PartialEq)]
pub struct ShapedLine {
    pub top: f32,
    pub baseline: f32,
    pub height: f32,
    pub width: f32,
    pub glyph_start: usize,
    pub glyph_end: usize,
}

/// A byte range and horizontal pen position used for caret and selection hit
/// testing. Whitespace clusters are retained even though they have no glyph.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextCluster {
    pub start: usize,
    pub end: usize,
    pub x: f32,
    pub line: usize,
}

/// Shaped text, independent of color and node position.
#[derive(Debug, Clone, PartialEq)]
pub struct TextLayout {
    pub size: Size,
    pub glyphs: Vec<ShapedGlyph>,
    pub lines: Vec<ShapedLine>,
    pub clusters: Vec<TextCluster>,
    pub font_size: f32,
    pub line_height: f32,
}

impl TextLayout {
    pub fn is_empty(&self) -> bool {
        self.size.width <= 0.0 || self.glyphs.is_empty()
    }

    /// Horizontal offset of a line inside a box of `width`.
    pub fn align_offset(&self, line: &ShapedLine, width: f32, align: TextAlign) -> f32 {
        let slack = (width - line.width).max(0.0);
        match align {
            TextAlign::Start => 0.0,
            TextAlign::Center => slack * 0.5,
            TextAlign::End => slack,
        }
    }

    /// Horizontal caret position for a UTF-8 byte offset.
    pub fn caret_x(&self, byte: usize) -> f32 {
        let mut x = 0.0;
        for cluster in &self.clusters {
            if byte <= cluster.start {
                return cluster.x;
            }
            if byte < cluster.end {
                return cluster.x;
            }
            x = cluster.x;
        }
        self.lines.first().map(|line| line.width).unwrap_or(x)
    }

    /// Nearest UTF-8 byte boundary for a horizontal position.
    pub fn byte_at_x(&self, text: &str, x: f32) -> usize {
        if self.clusters.is_empty() {
            return 0;
        }
        let mut following = vec![None; self.clusters.len()];
        let mut next_on_line: Vec<Option<usize>> = vec![None; self.lines.len()];
        for (index, cluster) in self.clusters.iter().enumerate().rev() {
            following[index] = next_on_line.get(cluster.line).copied().flatten();
            if let Some(slot) = next_on_line.get_mut(cluster.line) {
                *slot = Some(index);
            }
        }
        for (index, cluster) in self.clusters.iter().enumerate() {
            let next_x = following[index]
                .map(|next| self.clusters[next].x)
                .unwrap_or_else(|| {
                    self.lines
                        .get(cluster.line)
                        .map(|line| line.width)
                        .unwrap_or(cluster.x)
                });
            if x < (cluster.x + next_x) * 0.5 {
                return clamp_boundary(text, cluster.start);
            }
        }
        text.len()
    }
}

fn clamp_boundary(text: &str, mut byte: usize) -> usize {
    byte = byte.min(text.len());
    while byte > 0 && !text.is_char_boundary(byte) {
        byte -= 1;
    }
    byte
}

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

struct FaceInfo {
    id: fontdb::ID,
    weight: u16,
}

#[derive(Debug, Clone)]
struct CachedLayout {
    text: Arc<str>,
    style: TextStyle,
    width: Option<f32>,
    layout: Arc<TextLayout>,
}

/// A fast, non-cryptographic hasher for shaping cache keys. Collisions are
/// resolved by comparing the full text and style in `LayoutCache::get`.
#[derive(Clone, Copy)]
struct TextHasher(u64);

impl Default for TextHasher {
    fn default() -> Self {
        Self(0xcbf2_9ce4_8422_2325)
    }
}

impl Hasher for TextHasher {
    fn finish(&self) -> u64 {
        self.0
    }

    fn write(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            self.0 ^= u64::from(byte);
            self.0 = self.0.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
}

type LayoutBuckets = HashMap<u64, Vec<CachedLayout>, BuildHasherDefault<TextHasher>>;

#[derive(Default)]
struct LayoutCache {
    buckets: LayoutBuckets,
    len: usize,
}

impl LayoutCache {
    const LIMIT: usize = 2048;

    fn hash(text: &str, style: &TextStyle, width: Option<f32>) -> u64 {
        let mut hasher = TextHasher::default();
        text.hash(&mut hasher);
        style.family.hash(&mut hasher);
        style.weight.hash(&mut hasher);
        style.italic.hash(&mut hasher);
        style.size.to_bits().hash(&mut hasher);
        style.line_height.map(f32::to_bits).hash(&mut hasher);
        style.letter_spacing.to_bits().hash(&mut hasher);
        style.wrap.hash(&mut hasher);
        width.map(f32::to_bits).hash(&mut hasher);
        hasher.finish()
    }

    fn get(
        &self,
        hash: u64,
        text: &str,
        style: &TextStyle,
        width: Option<f32>,
    ) -> Option<Arc<TextLayout>> {
        let bucket = self.buckets.get(&hash)?;
        bucket.iter().find_map(|entry| {
            if entry.text.as_ref() == text
                && shaping_eq(&entry.style, style)
                && entry.width.map(f32::to_bits) == width.map(f32::to_bits)
            {
                Some(entry.layout.clone())
            } else {
                None
            }
        })
    }

    fn insert(
        &mut self,
        hash: u64,
        text: &str,
        style: &TextStyle,
        width: Option<f32>,
        layout: Arc<TextLayout>,
    ) {
        if self.len >= Self::LIMIT {
            // A wholesale clear is predictable and keeps the cache free of
            // per-entry bookkeeping. Text caches fill slowly in normal UIs.
            self.buckets.clear();
            self.len = 0;
        }
        let bucket = self.buckets.entry(hash).or_default();
        if bucket.len() >= 8 {
            bucket.remove(0);
            self.len -= 1;
        }
        bucket.push(CachedLayout {
            text: Arc::from(text),
            style: style.clone(),
            width,
            layout,
        });
        self.len += 1;
    }
}

fn shaping_eq(a: &TextStyle, b: &TextStyle) -> bool {
    a.family == b.family
        && a.weight == b.weight
        && a.italic == b.italic
        && a.size.to_bits() == b.size.to_bits()
        && a.line_height.map(f32::to_bits) == b.line_height.map(f32::to_bits)
        && a.letter_spacing.to_bits() == b.letter_spacing.to_bits()
        && a.wrap == b.wrap
}

struct Inner {
    font_system: FontSystem,
    scale_context: ScaleContext,
    faces: Vec<FaceInfo>,
    face_ids: HashMap<(fontdb::ID, u16), FontFaceId>,
    cache: LayoutCache,
}

/// Shared text service. All methods take `&self`; mutation is guarded by a
/// mutex so the runtime and the renderer can share one engine.
pub struct TextEngine {
    inner: Mutex<Inner>,
}

pub(crate) struct TextSession<'a> {
    inner: MutexGuard<'a, Inner>,
}

impl TextEngine {
    pub fn new() -> Self {
        let mut font_system = FontSystem::new();
        configure_generic_families(&mut font_system);
        Self {
            inner: Mutex::new(Inner {
                font_system,
                scale_context: ScaleContext::new(),
                faces: Vec::new(),
                face_ids: HashMap::new(),
                cache: LayoutCache::default(),
            }),
        }
    }

    /// Shapes `text`, returning a cached immutable layout.
    pub fn layout(&self, text: &str, style: &TextStyle, max_width: Option<f32>) -> Arc<TextLayout> {
        self.session().layout(text, style, max_width)
    }

    pub(crate) fn session(&self) -> TextSession<'_> {
        TextSession { inner: self.lock() }
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    pub fn measure(&self, text: &str, style: &TextStyle, max_width: Option<f32>) -> Size {
        self.layout(text, style, max_width).size
    }

    pub fn face_count(&self) -> usize {
        self.lock().faces.len()
    }

    /// Rasterizes one glyph at `font_size * scale` device pixels.
    ///
    /// The renderer includes the scale factor in its atlas key; this method
    /// itself performs no image caching.
    pub fn rasterize(
        &self,
        face: FontFaceId,
        glyph_id: u16,
        font_size: f32,
        scale: f32,
    ) -> Option<RasterizedGlyph> {
        if font_size <= 0.0 || scale <= 0.0 {
            return None;
        }
        self.lock().rasterize(face, glyph_id, font_size * scale)
    }
}

impl TextSession<'_> {
    pub(crate) fn layout(
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
        let hash = LayoutCache::hash(text, style, width);
        if let Some(layout) = self.inner.cache.get(hash, text, style, width) {
            return layout;
        }
        let layout = Arc::new(self.inner.shape(text, style, width));
        self.inner
            .cache
            .insert(hash, text, style, width, layout.clone());
        layout
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

impl Inner {
    fn face_id(&mut self, id: fontdb::ID, weight: u16) -> FontFaceId {
        if let Some(face) = self.face_ids.get(&(id, weight)) {
            return *face;
        }
        let face = FontFaceId(self.faces.len() as u64);
        self.faces.push(FaceInfo { id, weight });
        self.face_ids.insert((id, weight), face);
        face
    }

    fn shape(&mut self, text: &str, style: &TextStyle, max_width: Option<f32>) -> TextLayout {
        let line_height = style.line_height.unwrap_or(style.size * 1.32).max(1.0);
        let metrics = Metrics::new(style.size.max(1.0), line_height);
        let mut buffer = Buffer::new(&mut self.font_system, metrics);
        buffer.set_size(max_width, None);

        let family = match &style.family {
            FontFamily::SansSerif => Family::SansSerif,
            FontFamily::Serif => Family::Serif,
            FontFamily::Monospace => Family::Monospace,
            FontFamily::Named(name) => Family::Name(name.as_str()),
        };
        let attrs = Attrs::new()
            .family(family)
            .weight(fontdb::Weight(style.weight.0))
            .style(if style.italic {
                CosmicStyle::Italic
            } else {
                CosmicStyle::Normal
            })
            .letter_spacing(style.letter_spacing);
        buffer.set_text(text, &attrs, Shaping::Advanced, None);
        buffer.shape_until_scroll(&mut self.font_system, false);

        let mut glyphs = Vec::new();
        let mut lines = Vec::new();
        let mut clusters = Vec::new();
        let mut width: f32 = 0.0;
        let mut height: f32 = 0.0;

        for (line_index, run) in buffer.layout_runs().enumerate() {
            let glyph_start = glyphs.len();
            for glyph in run.glyphs {
                let x = glyph.x + glyph.x_offset * glyph.font_size;
                clusters.push(TextCluster {
                    start: glyph.start,
                    end: glyph.end,
                    x,
                    line: line_index,
                });
                if glyph.glyph_id == 0 {
                    continue;
                }
                let cluster = &run.text[glyph.start..glyph.end];
                if cluster.chars().all(char::is_whitespace) {
                    continue;
                }
                let face = self.face_id(glyph.font_id, glyph.font_weight.0);
                let baseline = run.line_y + glyph.y - glyph.y_offset * glyph.font_size;
                glyphs.push(ShapedGlyph {
                    face,
                    glyph_id: glyph.glyph_id,
                    font_size: glyph.font_size,
                    x,
                    baseline,
                });
            }
            let glyph_end = glyphs.len();
            width = width.max(run.line_w);
            height = height.max(run.line_top + run.line_height);
            lines.push(ShapedLine {
                top: run.line_top,
                baseline: run.line_y,
                height: run.line_height,
                width: run.line_w,
                glyph_start,
                glyph_end,
            });
        }

        if lines.is_empty() {
            height = line_height;
        }

        TextLayout {
            size: Size::new(width, height),
            glyphs,
            lines,
            clusters,
            font_size: style.size,
            line_height,
        }
    }

    fn rasterize(
        &mut self,
        face: FontFaceId,
        glyph_id: u16,
        pixel_size: f32,
    ) -> Option<RasterizedGlyph> {
        let info = self.faces.get(face.0 as usize)?;
        let font = self
            .font_system
            .get_font(info.id, fontdb::Weight(info.weight))?;
        let font_ref = font.as_swash();
        let mut scaler = self
            .scale_context
            .builder(font_ref)
            .size(pixel_size)
            .hint(true)
            .build();

        // Trying the color sources first is harmless for outline fonts and
        // picks up color outlines and bitmap emoji when available.
        let sources = [
            Source::ColorOutline(0),
            Source::ColorBitmap(StrikeWith::BestFit),
            Source::Outline,
        ];
        let mut renderer = Render::new(&sources);
        renderer.format(Format::Alpha).offset(Vector::new(0.0, 0.0));
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
}

fn configure_generic_families(font_system: &mut FontSystem) {
    let available: std::collections::HashSet<String> = font_system
        .db()
        .faces()
        .filter_map(|face| face.families.first().map(|(name, _)| name.clone()))
        .collect();

    fn pick(available: &std::collections::HashSet<String>, candidates: &[&str]) -> Option<String> {
        candidates
            .iter()
            .find(|candidate| available.contains(**candidate))
            .map(|candidate| (*candidate).to_owned())
    }

    if let Some(family) = pick(
        &available,
        &[
            "Inter",
            "Noto Sans",
            "DejaVu Sans",
            "Liberation Sans",
            "Cantarell",
            "Ubuntu",
            "Open Sans",
        ],
    ) {
        font_system.db_mut().set_sans_serif_family(family);
    }
    if let Some(family) = pick(
        &available,
        &["Noto Serif", "DejaVu Serif", "Liberation Serif", "Georgia"],
    ) {
        font_system.db_mut().set_serif_family(family);
    }
    if let Some(family) = pick(
        &available,
        &[
            "JetBrains Mono",
            "Noto Sans Mono",
            "DejaVu Sans Mono",
            "Liberation Mono",
            "Cascadia Code",
        ],
    ) {
        font_system.db_mut().set_monospace_family(family);
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

    #[test]
    fn wraps_at_width() {
        let engine = TextEngine::new();
        let style = TextStyle::default().size(16.0);
        let wide = engine.layout("one two three four five six", &style, None);
        let narrow = engine.layout("one two three four five six", &style, Some(80.0));
        assert!(narrow.lines.len() > wide.lines.len());
        assert!(narrow.size.height > wide.size.height);
    }

    #[test]
    fn byte_hit_testing_handles_boundaries_and_word_wrap() {
        let engine = TextEngine::new();
        let style = TextStyle::default().size(16.0);
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
}
