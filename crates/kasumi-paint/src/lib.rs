//! Text service wrapper and the retained-to-draw-list paint pass.

#![forbid(unsafe_code)]

mod paint;
mod text;

pub use paint::{
    InteractionState, PaintStats, Painter, ScrollbarColors, button_tint, cursor_for, paint,
    transform_rect,
};
pub use text::{RasterizedGlyph, ShapedGlyph, ShapedLine, TextCluster, TextEngine, TextLayout};
