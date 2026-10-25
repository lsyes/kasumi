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

use crate::text::{TextEngine, TextSession};

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
    out: &'a mut DrawList,
    stats: PaintStats,
}

pub fn paint(
    tree: &Tree,
    texts: Arc<TextEngine>,
    interaction: InteractionState,
    viewport: Size,
    out: &mut DrawList,
) -> PaintStats {
    let mut painter = Painter {
        tree,
        texts,
        interaction,
        out,
        stats: PaintStats::default(),
    };
    let clip = Rect::new(kasumi_core::Point::ZERO, viewport);
    let mut text_session = painter.texts.session();
    free_paint_node(
        painter.tree,
        &mut text_session,
        painter.interaction,
        painter.out,
        &mut painter.stats,
        painter.tree.root(),
        clip,
        Vec2::ZERO,
        0,
    );
    painter.stats
}

#[allow(clippy::too_many_arguments)]
fn free_paint_node(
    tree: &Tree,
    texts: &mut TextSession<'_>,
    interaction: InteractionState,
    out: &mut DrawList,
    stats: &mut PaintStats,
    node_id: NodeId,
    clip: Rect,
    offset: Vec2,
    depth: usize,
) {
    if depth > 512 {
        return;
    }
    let Some(node) = tree.get(node_id) else {
        return;
    };
    let rect = node.rect.translate(offset);
    if node.style.hidden || rect.is_empty() {
        return;
    }

    let mut background = node.style.background;
    let mut border_color = node.style.border_color;
    let mut border_width = node.style.border_width;

    if node.kind == NodeKind::Button {
        let hovered = interaction.hovered == Some(node.id);
        let pressed = interaction.active == Some(node.id) && interaction.pointer_down;
        background = button_tint_with_progress(background, node.hover_progress, hovered, pressed);
    }

    if interaction.focused == Some(node.id) && node.style.focus_ring.a > 0.0 {
        border_color = node.style.focus_ring;
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
        paint_slider(node, rect, clip, out, stats);
    } else if node.kind == NodeKind::ProgressBar {
        paint_progress(node, rect, clip, out, stats);
    }

    if let Some(image) = node.image.as_ref() {
        paint_image(node, image, rect, clip, out, stats);
    }

    if let Some(content) = node.text.as_ref() {
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
        );
    }

    let child_clip = if node.style.clip {
        clip.intersect(rect)
    } else {
        clip
    };
    let child_offset = Vec2::new(
        offset.x
            - if node.style.scroll_x {
                node.scroll.offset.x
            } else {
                0.0
            },
        offset.y
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
            out,
            stats,
            *child,
            child_clip,
            child_offset,
            depth + 1,
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
            out,
            stats,
            *child,
            child_clip,
            child_offset,
            depth + 1,
        );
    }

    paint_scrollbars(node, rect, clip, interaction, out, stats);
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
    let color = content.style.color;
    let align = content.style.align;
    let vertical_offset = if center_vertical {
        ((content_rect.height() - layout.size.height) * 0.5).max(0.0)
    } else {
        0.0
    };
    let scroll_x = input.map(|input| input.scroll_x).unwrap_or(0.0);

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
            color: selection_color,
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
                color,
                clip: clip.intersect(content_rect),
            });
            stats.glyphs += 1;
        }
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
        opacity: 1.0,
        radius: node.style.radius,
        clip: clip.intersect(rect),
    });
    stats.images += 1;
}

fn paint_slider(
    node: &kasumi_tree::Node,
    rect: Rect,
    clip: Rect,
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
    let amount = ((node.slider_value - node.slider_min) / range).clamp(0.0, 1.0);
    let thumb_x = track_rect.left() + (track_rect.width() - 16.0) * amount;
    let accent = if node.style.border_color.a > 0.0 {
        node.style.border_color
    } else {
        node.style.focus_ring
    };

    let track_color = if node.style.background.a > 0.0 {
        node.style.background
    } else {
        Color::rgba(1.0, 1.0, 1.0, 0.10)
    };
    out.push_rect(RectPrimitive {
        rect: track_rect,
        color: track_color,
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
        color: accent,
        border_color: Color::TRANSPARENT,
        border_width: 0.0,
        radius: Corners::all(2.5),
        clip,
    });
    out.push_rect(RectPrimitive {
        rect: Rect::from_xywh(thumb_x, rect.center().y - 8.0, 16.0, 16.0),
        color: Color::WHITE,
        border_color: accent,
        border_width: 2.0,
        radius: Corners::all(8.0),
        clip,
    });
    stats.rects += 3;
}

fn paint_progress(
    node: &kasumi_tree::Node,
    rect: Rect,
    clip: Rect,
    out: &mut DrawList,
    stats: &mut PaintStats,
) {
    let amount = node.slider_value.clamp(0.0, 1.0);
    out.push_rect(RectPrimitive {
        rect: Rect::from_xywh(
            rect.left(),
            rect.top(),
            rect.width() * amount,
            rect.height(),
        ),
        color: node.style.border_color,
        border_color: Color::TRANSPARENT,
        border_width: 0.0,
        radius: node.style.radius,
        clip,
    });
    stats.rects += 1;
}

fn paint_scrollbars(
    node: &kasumi_tree::Node,
    rect: Rect,
    clip: Rect,
    interaction: InteractionState,
    out: &mut DrawList,
    stats: &mut PaintStats,
) {
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
            .clamp(32.0, track.height());
        let t = (node.scroll.offset.y / range.max(f32::EPSILON)).clamp(0.0, 1.0);
        let rail = Rect::from_xywh(track.left() + 2.0, track.top(), 8.0, track.height());
        let hovered = track.contains(pointer);
        let thumb = Rect::from_xywh(
            rail.left() + 1.0,
            track.top() + (track.height() - thumb_height) * t,
            6.0,
            thumb_height,
        );
        out.push_rect(RectPrimitive {
            rect: track,
            color: Color::rgba(
                0.62,
                0.62,
                0.66,
                if hovered || active { 0.32 } else { 0.20 },
            ),
            border_color: Color::TRANSPARENT,
            border_width: 0.0,
            radius: Corners::all(3.5),
            clip,
        });
        out.push_rect(RectPrimitive {
            rect: thumb,
            color: if hovered || active {
                Color::rgba(0.38, 0.38, 0.42, 0.82)
            } else {
                Color::rgba(0.56, 0.56, 0.60, 0.58)
            },
            border_color: Color::TRANSPARENT,
            border_width: 0.0,
            radius: Corners::all(2.0),
            clip,
        });
        stats.rects += 2;
    }
    if node.style.scroll_x && node.scroll.content_size.width > rect.width() + 0.5 {
        let track = Rect::from_xywh(
            rect.left() + 4.0,
            rect.bottom() - 12.0,
            (rect.width() - 8.0).max(1.0),
            12.0,
        );
        let range = node.scroll.content_size.width - rect.width();
        let thumb_width = (track.width() * rect.width() / node.scroll.content_size.width)
            .clamp(32.0, track.width());
        let t = (node.scroll.offset.x / range.max(f32::EPSILON)).clamp(0.0, 1.0);
        let rail = Rect::from_xywh(track.left(), track.top() + 2.0, track.width(), 8.0);
        let hovered = track.contains(pointer);
        let thumb = Rect::from_xywh(
            track.left() + (track.width() - thumb_width) * t,
            rail.top() + 1.0,
            thumb_width,
            6.0,
        );
        out.push_rect(RectPrimitive {
            rect: track,
            color: Color::rgba(
                0.62,
                0.62,
                0.66,
                if hovered || active { 0.32 } else { 0.20 },
            ),
            border_color: Color::TRANSPARENT,
            border_width: 0.0,
            radius: Corners::all(3.5),
            clip,
        });
        out.push_rect(RectPrimitive {
            rect: thumb,
            color: if hovered || active {
                Color::rgba(0.38, 0.38, 0.42, 0.82)
            } else {
                Color::rgba(0.56, 0.56, 0.60, 0.58)
            },
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

#[cfg(test)]
mod tests {
    use kasumi_core::{Id, Size};
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
}

fn button_tint_with_progress(base: Color, progress: f32, hovered: bool, pressed: bool) -> Color {
    let progress = if hovered || pressed {
        progress.clamp(0.0, 1.0)
    } else {
        progress.clamp(0.0, 1.0)
    };
    let amount = progress * 0.10;
    if pressed {
        base.darken(0.1)
    } else {
        base.lighten(amount)
    }
}
