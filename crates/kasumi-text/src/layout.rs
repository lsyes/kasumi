//! Line breaking, bidi reordering and construction of the public layout.
//!
//! Shaping produces logical-order clusters. This module chooses line breaks
//! and applies the UAX #9 rule L2 reordering at cluster granularity. Glyphs
//! inside a cluster stay in logical order, which is what complex-script mark
//! placement requires.

use kasumi_core::{FontFaceId, Rect, Size, TextAlign, TextStyle, TextWrap};
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
    /// Byte range of the cluster this glyph belongs to. Syntax highlighting
    /// uses it to pick a colour without relying on glyph order (glyphs are
    /// stored in visual order after bidi reordering).
    pub start: usize,
    pub end: usize,
}

/// A single laid-out line inside [`TextLayout`].
///
/// `byte_start..byte_end` covers the text that produced the line, including
/// trailing whitespace but excluding the explicit newline that ended a hard
/// line. A soft-wrapped line therefore starts at the byte after the previous
/// line's last cluster, and `logical_line` is shared by all of its pieces.
#[derive(Debug, Clone, PartialEq)]
pub struct ShapedLine {
    pub top: f32,
    pub baseline: f32,
    pub height: f32,
    pub width: f32,
    pub glyph_start: usize,
    pub glyph_end: usize,
    pub byte_start: usize,
    pub byte_end: usize,
    /// Index into [`TextLayout::logical_line_starts`].
    pub logical_line: usize,
}

/// A byte range and horizontal pen position used for caret and selection hit
/// testing. Whitespace clusters are retained even though they have no glyph.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextCluster {
    pub start: usize,
    pub end: usize,
    /// Horizontal pen position of the cluster's leading edge.
    pub x: f32,
    /// Visual line index.
    pub line: usize,
    /// Advance of the cluster in logical pixels, including letter spacing.
    pub advance: f32,
    /// Bidi embedding level (odd levels are RTL).
    pub level: u8,
    /// True when the cluster contains only whitespace.
    pub whitespace: bool,
    /// Byte offset of this cluster within its logical line.
    pub column_byte: usize,
}

/// A resolved caret position, in layout-local coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CaretPosition {
    /// The (possibly clamped) UTF-8 byte offset of the caret.
    pub byte: usize,
    /// Visual line containing the caret.
    pub line: usize,
    /// Horizontal offset from the layout origin.
    pub x: f32,
    /// Top of the caret's line, relative to the layout origin.
    pub y: f32,
    /// Height of the caret's line.
    pub height: f32,
}

/// A source position expressed as a logical line and a byte column.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TextPosition {
    pub byte: usize,
    pub line: usize,
    pub column: usize,
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
    /// Byte offset at which each logical (source) line starts. Always non-empty
    /// and always begins with 0, including for an empty string.
    pub logical_line_starts: Vec<usize>,
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
    ///
    /// This is the single-line compatibility wrapper; use [`Self::caret_at`]
    /// whenever more than one visual line is possible.
    pub fn caret_x(&self, byte: usize) -> f32 {
        self.caret_x_on_line(0, byte)
    }

    /// Resolves a UTF-8 byte offset into a caret position with line geometry.
    ///
    /// The visual line is selected from the line byte ranges first, so an
    /// offset on the second line never snaps to a cluster on the first.Within
    /// a line the clusters are walked in logical (byte) order; offsets inside
    /// a cluster snap to its leading edge, which keeps ligatures and combining
    /// sequences as one caret stop.
    pub fn caret_at(&self, byte: usize) -> CaretPosition {
        if self.lines.is_empty() {
            return CaretPosition {
                byte,
                line: 0,
                x: 0.0,
                y: 0.0,
                height: self.line_height,
            };
        }
        let mut line_index = 0usize;
        for (index, line) in self.lines.iter().enumerate() {
            if byte < line.byte_start {
                break;
            }
            line_index = index;
            if byte < line.byte_end || byte == line.byte_start {
                break;
            }
        }
        let line = &self.lines[line_index];
        CaretPosition {
            byte,
            line: line_index,
            x: self.caret_x_on_line(line_index, byte),
            y: line.top,
            height: line.height,
        }
    }

    /// Horizontal caret position for `byte`, constrained to one visual line.
    fn caret_x_on_line(&self, line: usize, byte: usize) -> f32 {
        let end_x = self.line_end_x(line);
        let clusters = self.logical_clusters(line);
        for cluster in clusters.iter() {
            if byte <= cluster.start {
                return cluster.x;
            }
            if byte < cluster.end {
                return cluster.x;
            }
        }
        // Past the last cluster: use its trailing pen position (which includes
        // whitespace) rather than just the cluster's leading x.
        clusters
            .last()
            .map(|cluster| cluster.x + cluster.advance)
            .unwrap_or(end_x.max(self.lines.get(line).map(|l| l.width).unwrap_or(0.0)))
    }

    /// Rightmost pen position on a visual line, including trailing whitespace.
    fn line_end_x(&self, line: usize) -> f32 {
        self.clusters
            .iter()
            .filter(|cluster| cluster.line == line)
            .map(|cluster| cluster.x + cluster.advance)
            .fold(0.0f32, f32::max)
    }

    /// Nearest UTF-8 byte boundary for a horizontal position.
    ///
    /// Compatibility wrapper for single-line layouts; prefer
    /// [`Self::byte_at_point`] with a `y` coordinate.
    pub fn byte_at_x(&self, text: &str, x: f32) -> usize {
        self.byte_at_point(text, x, 0.0)
    }

    /// Nearest caret byte offset for a point in layout-local coordinates.
    ///
    /// The `y` coordinate selects the visual line; `x` is compared against the
    /// midpoints between clusters on that line. Points beyond the last cluster
    /// snap to the end of that line, not to the end of the whole document.
    pub fn byte_at_point(&self, text: &str, x: f32, y: f32) -> usize {
        if self.lines.is_empty() {
            return 0;
        }
        let line_index = self
            .lines
            .iter()
            .enumerate()
            .find(|(_, line)| y < line.top + line.height)
            .map(|(index, _)| index)
            .unwrap_or(self.lines.len() - 1);

        let mut on_line: Vec<&TextCluster> = self
            .clusters
            .iter()
            .filter(|cluster| cluster.line == line_index)
            .collect();
        on_line.sort_by_key(|cluster| cluster.start);

        for (index, cluster) in on_line.iter().enumerate() {
            let next_x = on_line
                .get(index + 1)
                .map(|next| next.x)
                .unwrap_or_else(|| {
                    self.lines
                        .get(line_index)
                        .map(|line| line.width.max(cluster.x))
                        .unwrap_or(cluster.x)
                });
            if x < (cluster.x + next_x) * 0.5 {
                return clamp_boundary(text, cluster.start);
            }
        }
        let end = self
            .lines
            .get(line_index)
            .map(|line| line.byte_end)
            .unwrap_or(text.len());
        clamp_boundary(text, end.min(text.len()))
    }

    /// Visual line index containing `byte`.
    pub fn line_of_byte(&self, byte: usize) -> usize {
        self.caret_at(byte).line
    }

    /// Logical (source) line index containing `byte`.
    pub fn logical_line_of_byte(&self, byte: usize) -> usize {
        match self.logical_line_starts.binary_search(&byte) {
            Ok(index) => index,
            Err(index) => index.saturating_sub(1),
        }
    }

    /// Number of logical lines in the source text.
    pub fn logical_line_count(&self) -> usize {
        self.logical_line_starts.len()
    }

    /// Byte range of a logical line, excluding its trailing newline.
    pub fn logical_line_range(&self, line: usize) -> std::ops::Range<usize> {
        let start = self
            .logical_line_starts
            .get(line)
            .copied()
            .unwrap_or(usize::MAX);
        if start == usize::MAX {
            return 0..0;
        }
        let end = self
            .logical_line_starts
            .get(line + 1)
            .copied()
            .map(|next| next.saturating_sub(1))
            .unwrap_or_else(|| {
                self.lines
                    .last()
                    .map(|l| l.byte_end.max(start))
                    .unwrap_or(start)
            });
        start..end.max(start)
    }

    /// Byte range covered by a visual line.
    pub fn line_byte_range(&self, line: usize) -> std::ops::Range<usize> {
        self.lines
            .get(line)
            .map(|line| line.byte_start..line.byte_end)
            .unwrap_or(0..0)
    }

    /// Source position of a byte offset. `column` counts UTF-8 bytes from the
    /// start of the logical line, which is the natural unit for an editor that
    /// stores byte offsets.
    pub fn position_at(&self, byte: usize) -> TextPosition {
        let line = self.logical_line_of_byte(byte);
        let start = self.logical_line_starts.get(line).copied().unwrap_or(0);
        TextPosition {
            byte,
            line,
            column: byte.saturating_sub(start),
        }
    }

    /// Byte offset for a logical line and byte column, clamped to that line.
    pub fn byte_at_position(&self, line: usize, column: usize) -> usize {
        let range = self.logical_line_range(line);
        if range.start == usize::MAX {
            return 0;
        }
        range.start + column.min(range.end.saturating_sub(range.start))
    }

    /// Byte offset of the start of a visual line.
    pub fn visual_line_start(&self, line: usize) -> usize {
        self.lines.get(line).map(|l| l.byte_start).unwrap_or(0)
    }

    /// Byte offset of the end of a visual line.
    pub fn visual_line_end(&self, line: usize) -> usize {
        self.lines.get(line).map(|l| l.byte_end).unwrap_or(0)
    }

    /// Character (Unicode scalar) index of a UTF-8 byte offset.
    pub fn char_index_of_byte(text: &str, byte: usize) -> usize {
        text[..clamp_boundary(text, byte)].chars().count()
    }

    /// UTF-8 byte offset of a character index, clamped to `text.len()`.
    pub fn byte_of_char_index(text: &str, index: usize) -> usize {
        text.char_indices()
            .nth(index)
            .map(|(byte, _)| byte)
            .unwrap_or(text.len())
    }

    /// One rectangle per visual line covered by a selection, in layout-local
    /// coordinates. `width`/`align` reproduce the paint pass's per-line offset.
    pub fn selection_rects(
        &self,
        start: usize,
        end: usize,
        width: f32,
        align: TextAlign,
    ) -> Vec<Rect> {
        let (start, end) = (start.min(end), start.max(end));
        if start == end {
            return Vec::new();
        }
        let mut rects = Vec::new();
        for (index, line) in self.lines.iter().enumerate() {
            let line_start = line.byte_start.max(start);
            let line_end = line.byte_end.min(end);
            if line_start >= line_end {
                continue;
            }
            let offset = self.align_offset(line, width, align);
            let a = self.caret_x_on_line(index, line_start);
            let b = self.caret_x_on_line(index, line_end);
            let left = offset + a.min(b);
            let right = offset + a.max(b);
            if right <= left {
                continue;
            }
            rects.push(Rect::from_xywh(
                left,
                line.top + 2.0,
                right - left,
                (line.height - 4.0).max(1.0),
            ));
            let _ = index;
        }
        rects
    }

    /// Caret rectangle in layout-local coordinates.
    pub fn caret_rect(&self, byte: usize, width: f32, align: TextAlign) -> Rect {
        let caret = self.caret_at(byte);
        let offset = self
            .lines
            .get(caret.line)
            .map(|line| self.align_offset(line, width, align))
            .unwrap_or(0.0);
        Rect::from_xywh(offset + caret.x, caret.y, 1.5, caret.height)
    }

    /// Logical-order view of the clusters on one visual line. The public
    /// `clusters` vector is in visual (bidi-reordered) order; editors that walk
    /// source text should use this instead.
    pub fn logical_clusters(&self, line: usize) -> Vec<TextCluster> {
        let mut result: Vec<TextCluster> = self
            .clusters
            .iter()
            .filter(|cluster| cluster.line == line)
            .copied()
            .collect();
        result.sort_by_key(|cluster| cluster.start);
        result
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
                byte_start: 0,
                byte_end: 0,
                logical_line: 0,
            }],
            clusters: Vec::new(),
            font_size,
            line_height,
            logical_line_starts: vec![0],
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
    let paragraph_end = base + text.len();

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

        let byte_start = raw
            .clusters
            .get(line_range.start)
            .map(|cluster| cluster.start)
            .unwrap_or(base);
        let byte_end = line_range
            .end
            .checked_sub(1)
            .and_then(|index| raw.clusters.get(index))
            .map(|cluster| cluster.end.max(byte_start))
            .unwrap_or(byte_start);

        let glyph_start = glyphs.len();
        let order = visual_order(&raw, line_range.clone());
        let mut pen = 0.0;
        for index in order {
            let cluster = &raw.clusters[index];
            let advance = cluster.advance + cluster_spacing(cluster, spacing);
            for glyph in &raw.glyphs[cluster.glyph_start..cluster.glyph_end] {
                glyphs.push(ShapedGlyph {
                    face: cluster.face,
                    glyph_id: glyph.id,
                    font_size,
                    x: pen + glyph.x,
                    // Swash positions marks with y pointing up; the layout
                    // and renderer use y pointing down.
                    baseline: baseline - glyph.y,
                    start: cluster.start,
                    end: cluster.end,
                });
            }
            clusters.push(TextCluster {
                start: cluster.start,
                end: cluster.end,
                x: pen,
                line: lines.len(),
                advance,
                level: cluster.level,
                whitespace: cluster.whitespace,
                column_byte: cluster.start.saturating_sub(base),
            });
            pen += advance;
        }

        let glyph_end = glyphs.len();
        lines.push(ShapedLine {
            top: line_top,
            baseline,
            height: line_height,
            width,
            glyph_start,
            glyph_end,
            byte_start,
            byte_end: byte_end.min(paragraph_end),
            logical_line: 0,
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
        // Filled in by `TextSystem::shape_text`, which knows all paragraphs.
        logical_line_starts: vec![base],
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
