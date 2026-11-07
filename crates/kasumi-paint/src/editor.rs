//! Paint data resolved by the editor widget, replayed by the paint pass.
//!
//! Keeping this in `kasumi-tree` lets the paint crate stay theme-agnostic:
//! the widget turns theme tokens into colours and geometry, the paint layer
//! only emits primitives.

use kasumi_core::{
    Color, Corners, DrawList, GlyphPrimitive, Point, Rect, RectPrimitive, Size, TextStyle,
    TextSpan, Vec2,
};
use kasumi_text::{ShapedGlyph, TextLayout};
use kasumi_tree::EditorPaint;

use crate::text::TextSession;
use crate::PaintStats;

/// Resolves the colour of a glyph from a sorted list of byte-range spans.
///
/// Spans are half-open byte ranges. A glyph belongs to a span when its own
/// byte range intersects it; the last matching span wins, so an application
/// can overlay an error underline or a search hit on top of syntax colours.
/// Glyphs are stored in visual (bidi) order, which is why this uses their
/// explicit byte ranges instead of an index into the source string.
pub fn span_color(spans: &[TextSpan], glyph: &ShapedGlyph, base: Color) -> Color {
    let mut color = base;
    for span in spans {
        if span.start < glyph.end && span.end > glyph.start && span.color.a > 0.0 {
            color = span.color;
        }
    }
    color
}

/// Background rectangles for spans that request one, in layout-local
/// coordinates. Each visual line gets its own rect, like a selection.
pub fn span_backgrounds(
    layout: &TextLayout,
    spans: &[TextSpan],
    width: f32,
    align: TextStyle,
) -> Vec<(Rect, Color)> {
    let mut out = Vec::new();
    for span in spans {
        let Some(background) = span.background else {
            continue;
        };
        if background.a <= 0.0 || span.end <= span.start {
            continue;
        }
        for rect in layout.selection_rects(span.start, span.end, width, align.align) {
            out.push((rect, background));
        }
    }
    out
}

/// Underlines (thin rects at the line baseline) for spans that request one.
pub fn span_underlines(
    layout: &TextLayout,
    spans: &[TextSpan],
    width: f32,
    align: TextStyle,
) -> Vec<(Rect, Color)> {
    let mut out = Vec::new();
    for span in spans {
        let Some(underline) = span.underline else {
            continue;
        };
        if underline.a <= 0.0 || span.end <= span.start {
            continue;
        }
        for rect in layout.selection_rects(span.start, span.end, width, align.align) {
            out.push((
                Rect::from_xywh(
                    rect.left(),
                    rect.bottom() - 1.0,
                    rect.width(),
                    1.5,
                ),
                underline,
            ));
        }
    }
    out
}

/// Pushes one laid-out glyph with a resolved colour.
#[allow(clippy::too_many_arguments)]
fn push_glyph(
    out: &mut DrawList,
    stats: &mut PaintStats,
    glyph: &ShapedGlyph,
    x: f32,
    baseline: f32,
    color: Color,
    clip: Rect,
) {
    out.push_glyph(GlyphPrimitive {
        face: glyph.face,
        glyph_id: glyph.glyph_id,
        font_size: glyph.font_size,
        x,
        baseline,
        color,
        clip,
    });
    stats.glyphs += 1;
}

/// Pushes a flat rectangle. Used for selections, current-line bands, carets,
/// whitespace markers and gutter separators.
pub fn push_flat_rect(out: &mut DrawList, rect: Rect, color: Color, clip: Rect) {
    if color.a <= 0.0 {
        return;
    }
    out.push_rect(RectPrimitive {
        rect,
        color,
        border_color: Color::TRANSPARENT,
        border_width: 0.0,
        radius: Corners::ZERO,
        clip,
    });
}

/// Draws a small centred dot for a visible space, and an arrow-ish dash for a
/// tab. Both are ordinary rects; no new primitive is needed.
#[allow(clippy::too_many_arguments)]
pub fn push_whitespace_marker(
    out: &mut DrawList,
    stats: &mut PaintStats,
    x: f32,
    y: f32,
    width: f32,
    tab: bool,
    color: Color,
    clip: Rect,
) {
    if color.a <= 0.0 {
        return;
    }
    let (w, h, cx, cy) = if tab {
        (width.max(2.0) * 0.7, 1.5, x + width * 0.5, y)
    } else {
        (2.0, 2.0, x + width * 0.5, y)
    };
    push_flat_rect(
        out,
        Rect::from_xywh(cx - w * 0.5, cy - h * 0.5, w, h),
        color,
        clip,
    );
    stats.rects += 1;
}

/// Paints a multi-line editor's text layer: current-line band, selection,
/// span backgrounds, visible whitespace, glyphs with per-span colours,
/// underlines, bracket highlights and the caret.
///
/// All coordinates are window-space; `origin` is the top-left of the text
/// content (after padding and the gutter), and `scroll` is the retained
/// scroll offset. Only the visible lines are drawn.
#[allow(clippy::too_many_arguments)]
pub(crate) fn paint_editor(
    out: &mut DrawList,
    stats: &mut PaintStats,
    texts: &mut TextSession<'_>,
    content: &kasumi_core::TextContent,
    editor: &EditorPaint,
    origin: Point,
    viewport: Size,
    clip: Rect,
    opacity: f32,
) {
    let width = viewport.width.max(1.0);
    // Unconditionally pass the available width: `TextSystem::layout` ignores
    // it for `TextWrap::None`, so this keeps hit testing and painting on the
    // exact same layout whether or not soft wrapping is enabled.
    let layout = texts.layout(&content.text, &content.style, Some(width));
    stats.text_layouts += 1;
    let style = &content.style;
    let (scroll_x, scroll_y) = (editor.scroll.0, editor.scroll.1);
    let base_color = style.color.fade(opacity);
    let text_clip = clip.intersect(Rect::new(origin, viewport));
    let line_height = layout.line_height;

    // Gutter: background, separator and one right-aligned number per logical
    // line. Soft-wrapped continuation lines stay blank, like a code editor.
    if editor.gutter_width > 0.0 {
        let gutter_rect = Rect::from_xywh(
            origin.x - editor.gutter_width,
            origin.y,
            editor.gutter_width,
            viewport.height,
        );
        let gutter_clip = clip.intersect(gutter_rect);
        push_flat_rect(
            out,
            gutter_rect,
            editor.gutter_background.fade(opacity),
            gutter_clip,
        );
        stats.rects += 1;
        push_flat_rect(
            out,
            Rect::from_xywh(origin.x - 1.0, origin.y, 1.0, viewport.height),
            editor.guide_color.fade(opacity),
            gutter_clip,
        );
        stats.rects += 1;
        if editor.show_line_numbers {
            let number_style = TextStyle {
                size: (style.size * 0.85).max(8.0),
                line_height: Some(line_height),
                color: editor.gutter_color,
                family: style.family.clone(),
                ..TextStyle::default()
            };
            let mut last_logical = None;
            for line in &layout.lines {
                let top = origin.y + line.top - scroll_y;
                if top + line.height < clip.top() || top > clip.bottom() {
                    continue;
                }
                if last_logical == Some(line.logical_line) {
                    continue;
                }
                last_logical = Some(line.logical_line);
                let label = (line.logical_line + 1).to_string();
                let numbers = texts.layout(&label, &number_style, None);
                stats.text_layouts += 1;
                let x = origin.x - 10.0 - numbers.size.width;
                for glyph in &numbers.glyphs {
                    let color = if line.logical_line == editor.current_line {
                        editor.current_line_color
                    } else {
                        editor.gutter_color
                    };
                    out.push_glyph(GlyphPrimitive {
                        face: glyph.face,
                        glyph_id: glyph.glyph_id,
                        font_size: glyph.font_size,
                        x: x + glyph.x,
                        baseline: top + (line_height + numbers.font_size * 0.35) * 0.5,
                        color: color.fade(opacity),
                        clip: gutter_clip,
                    });
                    stats.glyphs += 1;
                }
            }
        }
    }

    // Current-logical-line band behind every visual line of that source line.
    if editor.highlight_current_line && editor.current_line_background.a > 0.0 {
        for line in &layout.lines {
            if line.logical_line != editor.current_line {
                continue;
            }
            let top = origin.y + line.top - scroll_y;
            if top + line.height < clip.top() || top > clip.bottom() {
                continue;
            }
            push_flat_rect(
                out,
                Rect::from_xywh(origin.x, top, width, line.height),
                editor.current_line_background.fade(opacity),
                text_clip,
            );
            stats.rects += 1;
        }
    }

    // Selection rectangles, one per visual line.
    let (sel_start, sel_end) = editor.selection;
    if sel_start != sel_end {
        for rect in layout.selection_rects(sel_start, sel_end, width, style.align) {
            push_flat_rect(
                out,
                rect.translate(Vec2::new(
                    origin.x - scroll_x,
                    origin.y - scroll_y,
                )),
                editor.selection_color.fade(opacity),
                text_clip,
            );
            stats.rects += 1;
        }
    }

    // Span backgrounds (token highlights, search hits, diagnostics).
    for (rect, color) in span_backgrounds(&layout, &content.spans, width, style.clone()) {
        push_flat_rect(
            out,
            rect.translate(Vec2::new(origin.x - scroll_x, origin.y - scroll_y)),
            color.fade(opacity),
            text_clip,
        );
        stats.rects += 1;
    }

    // Visible whitespace markers and glyphs.
    for (line_index, line) in layout.lines.iter().enumerate() {
        let top = origin.y + line.top - scroll_y;
        if top + line.height < clip.top() || top > clip.bottom() {
            continue;
        }
        let offset = layout.align_offset(line, width, style.align);
        for glyph in &layout.glyphs[line.glyph_start..line.glyph_end] {
            let x = origin.x + offset + glyph.x - scroll_x;
            let baseline = origin.y + line.baseline - scroll_y;
            let color = span_color(&content.spans, glyph, base_color).fade(opacity);
            push_glyph(out, stats, glyph, x, baseline, color, text_clip);
        }
        if editor.show_whitespace {
            for cluster in layout.logical_clusters(line_index) {
                if !cluster.whitespace || cluster.advance <= 0.0 {
                    continue;
                }
                let text = &content.text;
                let slice = text.get(cluster.start..cluster.end).unwrap_or("");
                if slice == "\n" || slice == "\r" || slice.is_empty() {
                    continue;
                }
                let is_tab = slice.contains('\t');
                let width = cluster.advance;
                push_whitespace_marker(
                    out,
                    stats,
                    origin.x + offset + cluster.x - scroll_x,
                    origin.y + line.baseline - scroll_y - line_height * 0.28,
                    width,
                    is_tab,
                    editor.whitespace_color.fade(opacity),
                    text_clip,
                );
            }
        }
    }

    // Span underlines (errors, links, …).
    for (rect, color) in span_underlines(&layout, &content.spans, width, style.clone()) {
        push_flat_rect(
            out,
            rect.translate(Vec2::new(origin.x - scroll_x, origin.y - scroll_y)),
            color.fade(opacity),
            text_clip,
        );
        stats.rects += 1;
    }

    // Matching bracket highlight: a thin outline-like band behind the glyph.
    for byte in &editor.bracket_highlights {
        let caret = layout.caret_at(*byte);
        let line = &layout.lines[caret.line];
        let offset = layout.align_offset(line, width, style.align);
        let next = layout.caret_at(*byte + 1);
        let x = origin.x + offset + caret.x - scroll_x;
        let w = (next.x - caret.x).max(2.0);
        push_flat_rect(
            out,
            Rect::from_xywh(x, origin.y + line.top - scroll_y, w, line.height),
            editor.bracket_color.fade(opacity * 0.18),
            text_clip,
        );
        stats.rects += 1;
    }

    // Caret.
    if editor.show_caret {
        let rect = layout.caret_rect(editor.caret, width, style.align);
        push_flat_rect(
            out,
            rect.translate(Vec2::new(origin.x - scroll_x, origin.y - scroll_y)),
            editor.caret_color.fade(opacity),
            text_clip,
        );
        stats.rects += 1;
    }

    // Optional vertical indentation/line-length guide.
    if editor.guide_column > 0 && editor.guide_color.a > 0.0 {
        let x = origin.x + editor.guide_column as f32 * line_height * 0.6 - scroll_x;
        push_flat_rect(
            out,
            Rect::from_xywh(x, origin.y, 1.0, viewport.height),
            editor.guide_color.fade(opacity),
            text_clip,
        );
        stats.rects += 1;
    }
}
