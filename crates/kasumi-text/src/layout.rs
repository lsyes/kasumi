//! Line breaking, bidi reordering and construction of the public layout.
//!
//! Shaping produces logical-order clusters. This module chooses line breaks
//! and applies the UAX #9 rule L2 reordering at cluster granularity. Glyphs
//! inside a cluster stay in logical order, which is what complex-script mark
//! placement requires.

use kasumi_core::{FontFaceId, Size, TextAlign, TextStyle, TextWrap};
use unicode_linebreak::{BreakOpportunity, linebreaks};

use crate::font::FontStore;
use crate::shape::ShapedText;

/// One positioned glyph in a shaped run.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShapedGlyph {
    pub face: FontFaceId,
    pub glyph_id: u16,
    /// Font size in logical pixels.
    pub font_size: f32,
    /// Pen position relative to the text layout origin.
    pub x: f32,
    /// Baseline position relative to the text layout origin.
    pub baseline: f32,
}

/// A single laid-out line inside [`TextLayout`].
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

    pub(crate) fn blank(font_size: f32, line_height: f32) -> Self {
        let ascent = font_size * 0.8;
        let descent = font_size * 0.2;
        let baseline = (line_height + ascent - descent) * 0.5;
        Self {
            size: Size::new(0.0, line_height),
            glyphs: Vec::new(),
            lines: vec![ShapedLine {
                top: 0.0,
                baseline,
                height: line_height,
                width: 0.0,
                glyph_start: 0,
                glyph_end: 0,
            }],
            clusters: Vec::new(),
            font_size,
            line_height,
        }
    }

    /// Appends one paragraph layout below the existing content.
    pub(crate) fn append(&mut self, mut part: TextLayout) {
        let top_offset = self.size.height;
        let line_offset = self.lines.len();
        let glyph_offset = self.glyphs.len();
        for line in &mut part.lines {
            line.top += top_offset;
            line.baseline += top_offset;
            line.glyph_start += glyph_offset;
            line.glyph_end += glyph_offset;
        }
        for cluster in &mut part.clusters {
            cluster.line += line_offset;
        }
        // Glyphs were positioned relative to the paragraph origin. Shift
        // their baselines as well as the line metrics; otherwise explicit
        // newlines stack every paragraph on the first baseline.
        for glyph in &mut part.glyphs {
            glyph.baseline += top_offset;
        }
        self.glyphs.extend(part.glyphs);
        self.clusters.extend(part.clusters);
        self.lines.extend(part.lines);
        self.size.width = self.size.width.max(part.size.width);
        self.size.height = top_offset + part.size.height;
    }
}

fn clamp_boundary(text: &str, mut byte: usize) -> usize {
    byte = byte.min(text.len());
    while byte > 0 && !text.is_char_boundary(byte) {
        byte -= 1;
    }
    byte
}

/// Builds one paragraph. `base` is the byte offset of `text` in the complete
/// string, so cluster positions and caret mapping stay absolute.
pub(crate) fn build(
    fonts: &FontStore,
    text: &str,
    base: usize,
    style: &TextStyle,
    max_width: Option<f32>,
    raw: ShapedText,
) -> TextLayout {
    let font_size = style.size.max(1.0);
    let line_height = style.line_height.unwrap_or(font_size * 1.32).max(1.0);
    let spacing = if style.letter_spacing.is_finite() {
        style.letter_spacing
    } else {
        0.0
    };

    if raw.clusters.is_empty() {
        return TextLayout::blank(font_size, line_height);
    }

    let break_after = break_flags(text, base, style.wrap, &raw);

    let mut lines = Vec::new();
    let mut glyphs = Vec::with_capacity(raw.glyphs.len());
    let mut clusters = Vec::with_capacity(raw.clusters.len());
    let mut size = Size::ZERO;
    let mut line_top = 0.0;

    for line_range in split_lines(&raw, &break_after, spacing, max_width) {
        let width = display_width(&raw, line_range.clone(), spacing);

        let mut ascent: f32 = 0.0;
        let mut descent: f32 = 0.0;
        for cluster in &raw.clusters[line_range.clone()] {
            if let Some(face) = fonts.instance(cluster.face) {
                let metrics = face.data.metrics.scale(font_size);
                ascent = ascent.max(metrics.ascent);
                descent = descent.max(metrics.descent);
            }
        }
        if ascent <= 0.0 && descent <= 0.0 {
            ascent = font_size * 0.8;
            descent = font_size * 0.2;
        }
        let baseline = line_top + (line_height + ascent - descent) * 0.5;

        let glyph_start = glyphs.len();
        let order = visual_order(&raw, line_range.clone());
        let mut pen = 0.0;
        for index in order {
            let cluster = &raw.clusters[index];
            for glyph in &raw.glyphs[cluster.glyph_start..cluster.glyph_end] {
                glyphs.push(ShapedGlyph {
                    face: cluster.face,
                    glyph_id: glyph.id,
                    font_size,
                    x: pen + glyph.x,
                    // Swash positions marks with y pointing up; the layout
                    // and renderer use y pointing down.
                    baseline: baseline - glyph.y,
                });
            }
            clusters.push(TextCluster {
                start: cluster.start,
                end: cluster.end,
                x: pen,
                line: lines.len(),
            });
            pen += cluster.advance + cluster_spacing(cluster, spacing);
        }

        let glyph_end = glyphs.len();
        lines.push(ShapedLine {
            top: line_top,
            baseline,
            height: line_height,
            width,
            glyph_start,
            glyph_end,
        });
        // Trailing whitespace clusters still receive positions below for the
        // caret and selection logic, but they do not contribute to line width.
        size.width = size.width.max(width);
        line_top += line_height;
    }

    size.height = lines.len() as f32 * line_height;
    TextLayout {
        size,
        glyphs,
        lines,
        clusters,
        font_size,
        line_height,
    }
}

fn cluster_spacing(cluster: &crate::shape::RawCluster, spacing: f32) -> f32 {
    if cluster.end > cluster.start {
        spacing
    } else {
        0.0
    }
}

fn break_flags(text: &str, base: usize, wrap: TextWrap, raw: &ShapedText) -> Vec<bool> {
    let mut flags = vec![false; raw.clusters.len()];
    match wrap {
        TextWrap::None => {
            if let Some(last) = flags.last_mut() {
                *last = true;
            }
            return flags;
        }
        TextWrap::Word => {}
    }
    let positions: Vec<usize> = linebreaks(text)
        .filter(|(_, opportunity)| {
            matches!(
                opportunity,
                BreakOpportunity::Allowed | BreakOpportunity::Mandatory
            )
        })
        .map(|(position, _)| base + position)
        .collect();
    for (index, cluster) in raw.clusters.iter().enumerate() {
        flags[index] = positions.binary_search(&cluster.end).is_ok();
    }
    if let Some(last) = flags.last_mut() {
        *last = true;
    }
    flags
}

fn split_lines(
    raw: &ShapedText,
    break_after: &[bool],
    spacing: f32,
    max_width: Option<f32>,
) -> Vec<std::ops::Range<usize>> {
    let count = raw.clusters.len();
    let mut lines = Vec::new();
    let mut start = 0;
    while start < count {
        let mut index = start;
        let mut width = 0.0;
        let mut last_break = None;
        while index < count {
            if break_after[index] && index + 1 > start {
                last_break = Some(index + 1);
            }
            let cluster = &raw.clusters[index];
            let next_width = width + cluster.advance + cluster_spacing(cluster, spacing);
            let Some(max_width) = max_width else {
                width = next_width;
                index += 1;
                continue;
            };
            if next_width > max_width && index > start {
                if let Some(end) = last_break
                    && end > start
                    && end <= index + 1
                {
                    index = end;
                    break;
                }
                break;
            }
            width = next_width;
            index += 1;
        }
        let end = if index == start {
            (start + 1).min(count)
        } else {
            index
        };
        lines.push(start..end);
        start = end;
    }
    if lines.is_empty() {
        lines.push(0..0);
    }
    lines
}

fn display_width(raw: &ShapedText, range: std::ops::Range<usize>, spacing: f32) -> f32 {
    let mut end = range.end;
    while end > range.start && raw.clusters[end - 1].whitespace {
        end -= 1;
    }
    raw.clusters[range.start..end]
        .iter()
        .map(|cluster| cluster.advance + cluster_spacing(cluster, spacing))
        .sum()
}

/// UAX #9 rule L2 at cluster granularity. The shaper documents that RTL runs
/// must be reversed by cluster, not by glyph.
fn visual_order(raw: &ShapedText, range: std::ops::Range<usize>) -> Vec<usize> {
    let mut order: Vec<usize> = (range.start..range.end).collect();
    let max_level = raw.clusters[range]
        .iter()
        .map(|cluster| cluster.level)
        .max()
        .unwrap_or(0);
    for level in (1..=max_level).rev() {
        let mut index = 0;
        while index < order.len() {
            if raw.clusters[order[index]].level >= level {
                let run_start = index;
                while index < order.len() && raw.clusters[order[index]].level >= level {
                    index += 1;
                }
                order[run_start..index].reverse();
            } else {
                index += 1;
            }
        }
    }
    order
}
