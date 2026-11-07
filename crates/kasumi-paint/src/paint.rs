//! The paint pass: retained nodes become an ordered draw list.
//!
//! The pass is intentionally GPU-agnostic. Rects and glyphs go into one list so
//! z-order is preserved; the renderer can still batch the whole list into one
//! instanced draw because a single shader handles both primitive kinds.

use std::sync::Arc;

use kasumi_core::{
    Color, Corners, CursorIcon, DrawList, Edges, GlyphPrimitive, Id, ImageFit, ImagePrimitive,
    Point, Rect, RectPrimitive, Size, Vec2,
};
use kasumi_tree::{NodeId, NodeKind, TextInputState, Tree};

use crate::editor::span_color;
use crate::text::{TextEngine, TextSession};

const SCROLLBAR_VISIBLE_MIN: f32 = 40.0;

#[derive(Debug, Default, Clone, Copy)]
pub struct InteractionState {
    pub hovered: Option<Id>,
    pub active: Option<Id>,
    pub focused: Option<Id>,
    pub pointer_down: bool,
    pub pointer: Point,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct PaintStats {
    pub rects: usize,
    pub glyphs: usize,
    pub images: usize,
    pub text_layouts: usize,
}

pub struct Painter<'a> {
    tree: &'a Tree,
    texts: Arc<TextEngine>,
    interaction: InteractionState,
    scrollbars: ScrollbarColors,
    out: &'a mut DrawList,
    stats: PaintStats,
}

/// Colors used by scrollbar rails and thumbs.
///
/// Kept as a small paint-level value so the paint crate does not depend on the
/// theme crate. Backends fill it from the Material color roles.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScrollbarColors {
    pub rail: Color,
    pub thumb: Color,
    pub thumb_hover: Color,
}

impl Default for ScrollbarColors {
    fn default() -> Self {
        Self {
            rail: Color::rgba(0.62, 0.62, 0.66, 0.20),
            thumb: Color::rgba(0.56, 0.56, 0.60, 0.58),
            thumb_hover: Color::rgba(0.38, 0.38, 0.42, 0.82),
        }
    }
}

pub fn paint(
    tree: &Tree,
    texts: Arc<TextEngine>,
    interaction: InteractionState,
    scrollbars: ScrollbarColors,
    viewport: Size,
    out: &mut DrawList,
) -> PaintStats {
    let mut painter = Painter {
        tree,
        texts,
        interaction,
        scrollbars,
        out,
        stats: PaintStats::default(),
    };
    let clip = Rect::new(kasumi_core::Point::ZERO, viewport);
    let mut text_session = painter.texts.session();
    free_paint_node(
        painter.tree,
        &mut text_session,
        painter.interaction,
        painter.scrollbars,
        painter.out,
        &mut painter.stats,
        painter.tree.root(),
        clip,
        Vec2::ZERO,
        0,
        1.0,
    );
    painter.stats
}

#[allow(clippy::too_many_arguments)]
fn free_paint_node(
    tree: &Tree,
    texts: &mut TextSession<'_>,
    interaction: InteractionState,
    scrollbars: ScrollbarColors,
    out: &mut DrawList,
    stats: &mut PaintStats,
    node_id: NodeId,
    clip: Rect,
    offset: Vec2,
    depth: usize,
    opacity: f32,
) {
    if depth > 512 {
        return;
    }
    let Some(node) = tree.get(node_id) else {
        return;
    };
    let base_rect = node.rect.translate(offset);
    if node.style.hidden || base_rect.is_empty() {
        return;
    }
    let inherited_opacity = opacity;
    let opacity = inherited_opacity * node.visual.opacity();
    if opacity <= 0.001 {
        return;
    }
    let scale = node.visual.scale().max(0.0);
    let translation = node.visual.translation();
    let translated_rect = transform_rect(base_rect, translation, scale, node.visual.origin());
    let rect = translated_rect;
    let local_translation = Vec2::new(
        translated_rect.x() - base_rect.x(),
        translated_rect.y() - base_rect.y(),
    );

    let mut background = node.style.background.fade(opacity);
    let mut border_color = node.style.border_color.fade(opacity);
    let mut border_width = node.style.border_width;

    if matches!(
        node.kind,
        NodeKind::Button | NodeKind::ComboBox | NodeKind::Checkbox | NodeKind::Radio
    ) && node.style.state_layer.a <= 0.0
    {
        let hovered = interaction.hovered == Some(node.id);
        let pressed = interaction.active == Some(node.id) && interaction.pointer_down;
        background = button_tint_with_progress(background, node.hover_progress, hovered, pressed);
    }

    // Material 3 state layer. The opacities are the normative interaction
    // values; themes customize component mappings, not these constants.
    if node.style.state_layer.a > 0.0 {
        let pressed = interaction.active == Some(node.id) && interaction.pointer_down;
        let focused = interaction.focused == Some(node.id);
        let mut strength = node.hover_progress.clamp(0.0, 1.0) * 0.08;
        if focused {
            strength = strength.max(0.10);
        }
        if pressed {
            strength = 0.10;
        }
        if strength > 0.0 {
            let layer = node.style.state_layer;
            let amount = (strength * layer.a).clamp(0.0, 1.0);
            background = if background.a <= 0.001 {
                layer.with_alpha(amount * opacity)
            } else {
                background.lerp(layer.fade(opacity), amount)
            };
        }
    }

    if interaction.focused == Some(node.id) && node.style.focus_ring.a > 0.0 {
        border_color = node.style.focus_ring.fade(opacity);
        border_width = border_width.max(2.0);
    }

    if background.a > 0.0 || (border_width > 0.0 && border_color.a > 0.0) {
        out.push_rect(RectPrimitive {
            rect,
            color: background,
            border_color,
            border_width,
            radius: node.style.radius,
            clip,
        });
        stats.rects += 1;
    }

    if node.kind == NodeKind::Slider {
        paint_slider(node, rect, clip, opacity, out, stats);
    } else if node.kind == NodeKind::ProgressBar {
        paint_progress(node, rect, clip, opacity, out, stats);
    } else if node.kind == NodeKind::Switch {
        paint_switch(node, rect, clip, opacity, out, stats);
    }

    if let Some(image) = node.image.as_ref() {
        paint_image(node, image, rect, clip, opacity, out, stats);
    }

    if let Some(content) = node.text.as_ref() {
        if let Some(editor) = node.editor.as_ref() {
            crate::editor::paint_editor(
                out,
                stats,
                texts,
                content,
                editor,
                Point::new(
                    rect.left() + node.style.padding.left + editor.gutter_width,
                    rect.top() + node.style.padding.top,
                ),
                Size::new(
                    (rect.width() - node.style.padding.horizontal() - editor.gutter_width).max(0.0),
                    (rect.height() - node.style.padding.vertical()).max(0.0),
                ),
                clip,
                opacity,
            );
        } else {
            let is_text_input = node.kind == NodeKind::TextInput;
            paint_text(
                texts,
                out,
                stats,
                content,
                rect,
                node.style.padding,
                node.text_shape_width.get(),
                clip,
                is_text_input,
                is_text_input && interaction.focused == Some(node.id),
                if is_text_input {
                    node.text_input.as_ref()
                } else {
                    None
                },
                node.style.focus_ring.fade(0.28),
                opacity,
            );
        }
    }

    let child_clip = if node.style.clip {
        clip.intersect(rect)
    } else {
        clip
    };
    let child_offset = Vec2::new(
        offset.x + local_translation.x
            - if node.style.scroll_x {
                node.scroll.offset.x
            } else {
                0.0
            },
        offset.y + local_translation.y
            - if node.style.scroll_y {
                node.scroll.offset.y
            } else {
                0.0
            },
    );
    for child in node.children.iter().filter(|child| {
        tree.get(**child)
            .is_none_or(|node| node.kind != NodeKind::Overlay)
    }) {
        free_paint_node(
            tree,
            texts,
            interaction,
            scrollbars,
            out,
            stats,
            *child,
            child_clip,
            child_offset,
            depth + 1,
            opacity,
        );
    }
    for child in node.children.iter().filter(|child| {
        tree.get(**child)
            .is_some_and(|node| node.kind == NodeKind::Overlay)
    }) {
        free_paint_node(
            tree,
            texts,
            interaction,
            scrollbars,
            out,
            stats,
            *child,
            child_clip,
            child_offset,
            depth + 1,
            opacity,
        );
    }

    paint_scrollbars(
        node,
        rect,
        clip,
        translated_rect,
        interaction,
        scrollbars,
        opacity,
        out,
        stats,
    );
}

#[allow(clippy::too_many_arguments)]
fn paint_text(
    texts: &mut TextSession<'_>,
    out: &mut DrawList,
    stats: &mut PaintStats,
    content: &kasumi_core::TextContent,
    rect: Rect,
    padding: Edges,
    shape_width: Option<f32>,
    clip: Rect,
    center_vertical: bool,
    caret: bool,
    input: Option<&TextInputState>,
    selection_color: Color,
    opacity: f32,
) {
    let content_rect = rect.inset(padding);
    if content_rect.width() <= 0.0 {
        return;
    }
    // A small tolerance prevents a line measured at exactly its own width
    // from wrapping because layout and paint rounded the same value
    // differently.
    let shape_width = shape_width.map(|width| width + 0.5);
    let layout = texts.layout(&content.text, &content.style, shape_width);
    stats.text_layouts += 1;
    let color = content.style.color.fade(opacity);
    let align = content.style.align;
    let vertical_offset = if center_vertical {
        ((content_rect.height() - layout.size.height) * 0.5).max(0.0)
    } else {
        0.0
    };
    let scroll_x = input.map(|input| input.scroll_x).unwrap_or(0.0);

    // Span backgrounds and underlines are painted behind the glyphs. They are
    // per visual line, so wrapped ranges do not produce one wide rectangle.
    for (rect, color) in crate::editor::span_backgrounds(
        &layout,
        &content.spans,
        content_rect.width(),
        content.style.clone(),
    ) {
        out.push_rect(RectPrimitive {
            rect: rect.translate(Vec2::new(
                content_rect.left() - scroll_x,
                content_rect.top() + vertical_offset,
            )),
            color: color.fade(opacity),
            border_color: Color::TRANSPARENT,
            border_width: 0.0,
            radius: Corners::ZERO,
            clip: clip.intersect(content_rect),
        });
        stats.rects += 1;
    }

    if caret
        && let Some(input) = input
        && input.paint_anchor != input.paint_cursor
    {
        let a = layout.caret_x(input.paint_anchor.min(input.paint_cursor)) - scroll_x;
        let b = layout.caret_x(input.paint_anchor.max(input.paint_cursor)) - scroll_x;
        let left = content_rect.left() + a.min(b);
        let right = content_rect.left() + a.max(b);
        let top = content_rect.top() + vertical_offset + 2.0;
        let height = (layout.line_height - 4.0).max(1.0);
        out.push_rect(RectPrimitive {
            rect: Rect::from_xywh(
                left.max(content_rect.left()),
                top,
                (right.min(content_rect.right()) - left.max(content_rect.left())).max(0.0),
                height,
            ),
            color: selection_color.fade(opacity),
            border_color: Color::TRANSPARENT,
            border_width: 0.0,
            radius: Corners::ZERO,
            clip: clip.intersect(content_rect),
        });
        stats.rects += 1;
    }

    for line in &layout.lines {
        let offset = layout.align_offset(line, content_rect.width(), align);
        for glyph in &layout.glyphs[line.glyph_start..line.glyph_end] {
            out.push_glyph(GlyphPrimitive {
                face: glyph.face,
                glyph_id: glyph.glyph_id,
                font_size: glyph.font_size,
                x: content_rect.left() + offset + glyph.x - scroll_x,
                baseline: content_rect.top() + vertical_offset + glyph.baseline,
                color: span_color(&content.spans, glyph, color).fade(opacity),
                clip: clip.intersect(content_rect),
            });
            stats.glyphs += 1;
        }
    }

    for (rect, color) in crate::editor::span_underlines(
        &layout,
        &content.spans,
        content_rect.width(),
        content.style.clone(),
    ) {
        out.push_rect(RectPrimitive {
            rect: rect.translate(Vec2::new(
                content_rect.left() - scroll_x,
                content_rect.top() + vertical_offset,
            )),
            color: color.fade(opacity),
            border_color: Color::TRANSPARENT,
            border_width: 0.0,
            radius: Corners::ZERO,
            clip: clip.intersect(content_rect),
        });
        stats.rects += 1;
    }

    if caret {
        let cursor = input
            .map(|input| input.paint_cursor)
            .unwrap_or(content.text.len());
        let x = content_rect.left() + layout.caret_x(cursor) - scroll_x;
        let top = content_rect.top() + vertical_offset;
        let height = layout.line_height;
        out.push_rect(RectPrimitive {
            rect: Rect::from_xywh(x, top + 2.0, 1.5, (height - 4.0).max(1.0)),
            color,
            border_color: Color::TRANSPARENT,
            border_width: 0.0,
            radius: kasumi_core::Corners::ZERO,
            clip: clip.intersect(content_rect),
        });
        stats.rects += 1;
    }
}

fn paint_image(
    node: &kasumi_tree::Node,
    image: &Arc<kasumi_core::ImageData>,
    rect: Rect,
    clip: Rect,
    opacity: f32,
    out: &mut DrawList,
    stats: &mut PaintStats,
) {
    let image_size = Size::new(image.width() as f32, image.height() as f32);
    let fitted = match node.image_fit {
        ImageFit::Fill => rect,
        ImageFit::Original => Rect::new(
            Point::new(
                rect.center().x - image_size.width * 0.5,
                rect.center().y - image_size.height * 0.5,
            ),
            image_size,
        ),
        ImageFit::Contain | ImageFit::Cover => {
            let scale_x = rect.width() / image_size.width.max(1.0);
            let scale_y = rect.height() / image_size.height.max(1.0);
            let scale = if node.image_fit == ImageFit::Cover {
                scale_x.max(scale_y)
            } else {
                scale_x.min(scale_y)
            };
            let size = Size::new(image_size.width * scale, image_size.height * scale);
            Rect::new(
                Point::new(
                    rect.center().x - size.width * 0.5,
                    rect.center().y - size.height * 0.5,
                ),
                size,
            )
        }
    };
    out.push_image(ImagePrimitive {
        rect: fitted,
        image: Arc::clone(image),
        opacity,
        radius: node.style.radius,
        clip: clip.intersect(rect),
    });
    stats.images += 1;
}

fn paint_slider(
    node: &kasumi_tree::Node,
    rect: Rect,
    clip: Rect,
    opacity: f32,
    out: &mut DrawList,
    stats: &mut PaintStats,
) {
    let inset = 9.0;
    let track_rect = Rect::from_xywh(
        rect.left() + inset,
        rect.center().y - 2.5,
        (rect.width() - inset * 2.0).max(1.0),
        5.0,
    );
    let range = (node.slider_max - node.slider_min).max(f32::EPSILON);
    let amount = node
        .animations
        .get("slider")
        .map(|value| value.value().clamp(0.0, 1.0))
        .unwrap_or_else(|| ((node.slider_value - node.slider_min) / range).clamp(0.0, 1.0));
    let thumb_x = track_rect.left() + (track_rect.width() - 16.0) * amount;
    let accent = if node.style.border_color.a > 0.0 {
        node.style.border_color
    } else {
        node.style.focus_ring
    };

    let track_color = if node.style.text.color.a > 0.0 {
        node.style.text.color
    } else if node.style.background.a > 0.0 {
        node.style.background
    } else {
        Color::rgba(1.0, 1.0, 1.0, 0.10)
    };
    out.push_rect(RectPrimitive {
        rect: track_rect,
        color: track_color.fade(opacity),
        border_color: Color::TRANSPARENT,
        border_width: 0.0,
        radius: Corners::all(2.5),
        clip,
    });
    out.push_rect(RectPrimitive {
        rect: Rect::from_xywh(
            track_rect.left(),
            track_rect.top(),
            (thumb_x + 8.0 - track_rect.left()).max(0.0),
            track_rect.height(),
        ),
        color: accent.fade(opacity),
        border_color: Color::TRANSPARENT,
        border_width: 0.0,
        radius: Corners::all(2.5),
        clip,
    });
    out.push_rect(RectPrimitive {
        rect: Rect::from_xywh(thumb_x, rect.center().y - 9.0, 18.0, 18.0),
        color: accent.fade(opacity),
        border_color: Color::TRANSPARENT,
        border_width: 0.0,
        radius: Corners::all(9.0),
        clip,
    });
    stats.rects += 3;
}

/// Paints a Material 3 switch from the geometry the widget resolved.
///
/// The track is centred in the node, the state layer sits under the thumb, and
/// the icons are drawn as two small rects (a check mark and an X) so the paint
/// layer stays free of a glyph atlas dependency for control chrome.
fn paint_switch(
    node: &kasumi_tree::Node,
    rect: Rect,
    clip: Rect,
    opacity: f32,
    out: &mut DrawList,
    stats: &mut PaintStats,
) {
    let Some(switch) = node.switch else {
        return;
    };
    let (track_width, track_height) = switch.track_size;
    let track = Rect::from_xywh(
        rect.left() + (rect.width() - track_width) / 2.0,
        rect.top() + (rect.height() - track_height) / 2.0,
        track_width,
        track_height,
    );
    let progress = switch.progress.clamp(0.0, 1.0);
    let thumb_center = (
        track.left() + switch.thumb_center.0,
        track.top() + switch.thumb_center.1,
    );

    // Material paints the state layer above the track and below the thumb so
    // the thumb keeps its full contrast.
    if switch.state_layer.a > 0.001 {
        let diameter = 40.0;
        out.push_rect(RectPrimitive {
            rect: Rect::from_xywh(
                thumb_center.0 - diameter / 2.0,
                thumb_center.1 - diameter / 2.0,
                diameter,
                diameter,
            ),
            color: switch.state_layer.fade(opacity),
            border_color: Color::TRANSPARENT,
            border_width: 0.0,
            radius: Corners::all(diameter / 2.0),
            clip,
        });
        stats.rects += 1;
    }

    out.push_rect(RectPrimitive {
        rect: track,
        color: switch.track.fade(opacity),
        border_color: switch.track_border.fade(opacity),
        border_width: if switch.track_border.a > 0.01 {
            2.0
        } else {
            0.0
        },
        radius: Corners::all(track_height / 2.0),
        clip,
    });
    stats.rects += 1;

    // Icons are painted before the thumb so the 16 dp glyph sits inside it.
    if switch.icons {
        let (glyph, alpha) = if progress >= 0.5 {
            ("check", progress)
        } else {
            ("close", 1.0 - progress)
        };
        if alpha > 0.02 {
            paint_switch_icon(
                out,
                clip,
                glyph,
                thumb_center,
                switch.icon_color.fade(opacity * alpha),
                &mut stats.rects,
            );
        }
    }

    let thumb = switch.thumb.max(1.0);
    out.push_rect(RectPrimitive {
        rect: Rect::from_xywh(
            thumb_center.0 - thumb / 2.0,
            thumb_center.1 - thumb / 2.0,
            thumb,
            thumb,
        ),
        color: switch.thumb_color.fade(opacity),
        border_color: Color::TRANSPARENT,
        border_width: 0.0,
        radius: Corners::all(thumb / 2.0),
        clip,
    });
    stats.rects += 1;
}

/// Draws the check mark or the X inside a switch thumb from thin bars.
fn paint_switch_icon(
    out: &mut DrawList,
    clip: Rect,
    glyph: &str,
    center: (f32, f32),
    color: Color,
    rects: &mut usize,
) {
    // Two 2 dp strokes approximating the 16 dp Material glyphs. Rotation is not
    // available on rect primitives, so the strokes are axis-aligned segments
    // that read as a check or a cross at control size.
    let stroke = 2.0;
    let arm = 3.4;
    let bars: [(f32, f32, f32, f32); 2] = if glyph == "check" {
        [
            (center.0 - arm, center.1, stroke, arm * 1.6),
            (
                center.0 + arm * 0.4,
                center.1 - arm * 0.6,
                stroke,
                arm * 2.6,
            ),
        ]
    } else {
        [
            (center.0 - arm, center.1 - arm, arm * 2.0, stroke),
            (center.0 - arm, center.1 + arm - stroke, arm * 2.0, stroke),
        ]
    };
    for (x, y, width, height) in bars {
        out.push_rect(RectPrimitive {
            rect: Rect::from_xywh(x, y, width, height),
            color,
            border_color: Color::TRANSPARENT,
            border_width: 0.0,
            radius: Corners::all(stroke / 2.0),
            clip,
        });
        *rects += 1;
    }
}

fn paint_progress(
    node: &kasumi_tree::Node,
    rect: Rect,
    clip: Rect,
    opacity: f32,
    out: &mut DrawList,
    stats: &mut PaintStats,
) {
    let amount = node
        .animations
        .get("progress")
        .map(|value| value.value().clamp(0.0, 1.0))
        .unwrap_or_else(|| node.slider_value.clamp(0.0, 1.0));
    out.push_rect(RectPrimitive {
        rect: Rect::from_xywh(
            rect.left(),
            rect.top(),
            rect.width() * amount,
            rect.height(),
        ),
        color: node.style.border_color.fade(opacity),
        border_color: Color::TRANSPARENT,
        border_width: 0.0,
        radius: node.style.radius,
        clip,
    });
    stats.rects += 1;
}

#[allow(clippy::too_many_arguments)]
fn paint_scrollbars(
    node: &kasumi_tree::Node,
    rect: Rect,
    clip: Rect,
    translated_rect: Rect,
    interaction: InteractionState,
    scrollbars: ScrollbarColors,
    opacity: f32,
    out: &mut DrawList,
    stats: &mut PaintStats,
) {
    if !node.show_scrollbars {
        return;
    }
    let pointer = interaction.pointer;
    let active = interaction.active == Some(node.id) && interaction.pointer_down;
    if node.style.scroll_y && node.scroll.content_size.height > rect.height() + 0.5 {
        let track = Rect::from_xywh(
            rect.right() - 12.0,
            rect.top() + 4.0,
            12.0,
            (rect.height() - 8.0).max(1.0),
        );
        let range = node.scroll.content_size.height - rect.height();
        let thumb_height = (track.height() * rect.height() / node.scroll.content_size.height)
            .clamp(32.0_f32.min(track.height()), track.height());
        let t = (node.scroll.offset.y / range.max(f32::EPSILON)).clamp(0.0, 1.0);
        let hovered = track.contains(pointer);
        let track = Rect::from_xywh(
            translated_rect.right() - 12.0,
            translated_rect.top() + 4.0,
            12.0,
            (translated_rect.height() - 8.0).max(1.0),
        );
        let rail = Rect::from_xywh(track.left() + 2.0, track.top(), 8.0, track.height());
        let thumb = Rect::from_xywh(
            rail.left() + 1.0,
            track.top() + (track.height() - thumb_height) * t,
            6.0,
            thumb_height,
        );
        out.push_rect(RectPrimitive {
            rect: track,
            color: scrollbars.rail.fade(opacity),
            border_color: Color::TRANSPARENT,
            border_width: 0.0,
            radius: Corners::all(3.5),
            clip,
        });
        out.push_rect(RectPrimitive {
            rect: thumb,
            color: if hovered || active {
                scrollbars.thumb_hover
            } else {
                scrollbars.thumb
            }
            .fade(opacity),
            border_color: Color::TRANSPARENT,
            border_width: 0.0,
            radius: Corners::all(2.0),
            clip,
        });
        stats.rects += 2;
    }
    if node.style.scroll_x
        && node.scroll.content_size.width > rect.width() + 0.5
        && rect.width() >= SCROLLBAR_VISIBLE_MIN
    {
        let track = Rect::from_xywh(
            rect.left() + 4.0,
            rect.bottom() - 12.0,
            (rect.width() - 8.0).max(1.0),
            12.0,
        );
        let range = node.scroll.content_size.width - rect.width();
        let thumb_width = (track.width() * rect.width() / node.scroll.content_size.width)
            .clamp(32.0_f32.min(track.width()), track.width());
        let t = (node.scroll.offset.x / range.max(f32::EPSILON)).clamp(0.0, 1.0);
        let hovered = track.contains(pointer);
        let track = Rect::from_xywh(
            translated_rect.left() + 4.0,
            translated_rect.bottom() - 12.0,
            (translated_rect.width() - 8.0).max(1.0),
            12.0,
        );
        let rail = Rect::from_xywh(track.left(), track.top() + 2.0, track.width(), 8.0);
        let thumb = Rect::from_xywh(
            track.left() + (track.width() - thumb_width) * t,
            rail.top() + 1.0,
            thumb_width,
            6.0,
        );
        out.push_rect(RectPrimitive {
            rect: track,
            color: scrollbars.rail.fade(opacity),
            border_color: Color::TRANSPARENT,
            border_width: 0.0,
            radius: Corners::all(3.5),
            clip,
        });
        out.push_rect(RectPrimitive {
            rect: thumb,
            color: if hovered || active {
                scrollbars.thumb_hover
            } else {
                scrollbars.thumb
            }
            .fade(opacity),
            border_color: Color::TRANSPARENT,
            border_width: 0.0,
            radius: Corners::all(2.0),
            clip,
        });
        stats.rects += 2;
    }
}

pub fn cursor_for(tree: &Tree, hovered: Option<Id>) -> CursorIcon {
    hovered
        .and_then(|id| tree.node_by_id(id))
        .map(|node| node.style.cursor)
        .unwrap_or(CursorIcon::Default)
}

/// Blend helper used by themes and widgets.
pub fn button_tint(base: Color, hovered: bool, pressed: bool) -> Color {
    if pressed {
        base.darken(0.1)
    } else if hovered {
        base.lighten(0.08)
    } else {
        base
    }
}

fn button_tint_with_progress(base: Color, progress: f32, _hovered: bool, pressed: bool) -> Color {
    let progress = progress.clamp(0.0, 1.0);
    let amount = progress * 0.10;
    let lightness = base.r.max(base.g).max(base.b);
    if pressed {
        base.darken(0.1)
    } else if (0.72..=0.98).contains(&lightness) {
        base.darken(amount)
    } else {
        base.lighten(amount)
    }
}

/// Applies a visual transform as one shared affine operation. This helper is
/// used by paint and tests so nested translate/scale handling cannot drift.
pub fn transform_rect(
    rect: Rect,
    translation: Vec2,
    scale: f32,
    origin: Option<(f32, f32)>,
) -> Rect {
    if (scale - 1.0).abs() <= 0.0001 && translation == Vec2::ZERO {
        return rect;
    }
    let (origin_x, origin_y) = origin.unwrap_or((0.5, 0.5));
    let pivot = Point::new(
        rect.left() + rect.width() * origin_x,
        rect.top() + rect.height() * origin_y,
    );
    Rect::from_xywh(
        pivot.x + (rect.left() - pivot.x) * scale + translation.x,
        pivot.y + (rect.top() - pivot.y) * scale + translation.y,
        rect.width() * scale.max(0.0),
        rect.height() * scale.max(0.0),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use kasumi_core::{Id, Size, Style, TextStyle};
    use kasumi_tree::{NodeKind, Sense, Tree};

    #[test]
    fn hover_transition_is_continuous() {
        let mut tree = Tree::new();
        tree.begin_frame(Size::new(100.0, 100.0));
        let id = Id::from_raw(7);
        let node = tree.ensure(id, NodeKind::Button);
        tree.set_sense(node, Sense::click());
        tree.declare_child(tree.root(), node);
        tree.end_frame();
        assert!(tree.tick_hover(Some(id), false, 1.0 / 60.0));
        let progress = tree.node_by_id(id).unwrap().hover_progress;
        assert!(progress > 0.0 && progress < 1.0);
        assert!(tree.tick_hover(None, false, 1.0 / 60.0));
        assert!(tree.node_by_id(id).unwrap().hover_progress < progress);
    }

    #[test]
    fn hovered_menu_item_background_is_visibly_darker() {
        let theme = kasumi_core::Color::hex("#ffffff");
        let base = theme.lerp(kasumi_core::Color::hex("#ececf0"), 0.45);
        let unhighlighted = button_tint_with_progress(base, 0.0, false, false);
        let highlighted = button_tint_with_progress(base, 1.0, true, false);
        assert!(highlighted.r < unhighlighted.r);
        assert!(highlighted.g < unhighlighted.g);
        assert!(highlighted.b < unhighlighted.b);
    }

    #[test]
    fn transform_rect_keeps_the_origin_fixed() {
        let rect = Rect::from_xywh(10.0, 20.0, 100.0, 50.0);
        let transformed = transform_rect(rect, Vec2::new(5.0, -3.0), 1.5, Some((0.5, 0.5)));
        assert_eq!(transformed.center(), Point::new(65.0, 42.0));
        assert_eq!(transformed.size, Size::new(150.0, 75.0));
    }

    #[test]
    fn short_scrollbar_tracks_do_not_panic() {
        let mut tree = Tree::new();
        tree.begin_frame(Size::new(100.0, 100.0));
        let id = Id::from_raw(7);
        let node = tree.ensure(id, NodeKind::Container);
        let style = Style {
            scroll_y: true,
            clip: true,
            ..Style::default()
        };
        tree.set_style(node, style);
        tree.declare_child(tree.root(), node);
        tree.end_frame();
        let mut engine = kasumi_layout::LayoutEngine::new();
        struct Measure;
        impl kasumi_layout::TextMeasure for Measure {
            fn measure_text(
                &self,
                _text: &str,
                _style: &TextStyle,
                _max_width: Option<f32>,
            ) -> Size {
                Size::ZERO
            }
        }
        engine.compute(&mut tree, Size::new(100.0, 100.0), &Measure);
        if let Some(node) = tree.get_mut(node) {
            node.rect = Rect::from_xywh(0.0, 0.0, 40.0, 2.0);
            node.scroll.content_size = Size::new(40.0, 100.0);
        }
        let texts = Arc::new(crate::TextEngine::new());
        let mut draw = kasumi_core::DrawList::default();
        paint(
            &tree,
            texts,
            InteractionState::default(),
            ScrollbarColors::default(),
            Size::new(100.0, 100.0),
            &mut draw,
        );
    }

    #[test]
    fn hidden_scrollbars_emit_no_rail_or_thumb() {
        let mut tree = Tree::new();
        tree.begin_frame(Size::new(100.0, 100.0));
        let id = Id::from_raw(8);
        let node = tree.ensure(id, NodeKind::Container);
        tree.set_style(
            node,
            Style {
                scroll_y: true,
                clip: true,
                ..Style::default()
            },
        );
        tree.declare_child(tree.root(), node);
        tree.end_frame();
        if let Some(node) = tree.get_mut(node) {
            node.rect = Rect::from_xywh(0.0, 0.0, 80.0, 80.0);
            node.scroll.content_size = Size::new(80.0, 160.0);
            node.show_scrollbars = false;
        }
        let texts = Arc::new(crate::TextEngine::new());
        let mut draw = kasumi_core::DrawList::default();
        paint(
            &tree,
            texts,
            InteractionState::default(),
            ScrollbarColors::default(),
            Size::new(100.0, 100.0),
            &mut draw,
        );
        assert!(draw.is_empty());
    }
}
