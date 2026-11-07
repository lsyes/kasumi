//! Text service wrapper and the retained-to-draw-list paint pass.

#![forbid(unsafe_code)]

mod editor;
mod paint;
mod text;

pub use editor::{
    push_flat_rect, push_whitespace_marker, span_backgrounds, span_color, span_underlines,
};
pub use paint::{
    InteractionState, PaintStats, Painter, ScrollbarColors, button_tint, cursor_for, paint,
    transform_rect,
};
pub use text::{
    BracketKind, BracketMatch, CaretPosition, RasterizedGlyph, ShapedGlyph, ShapedLine,
    TextAnalysis, TextCluster, TextEngine, TextLayout, TextPosition, byte_at_column,
    column_of_byte, expand_tabs, line_end, line_start, next_grapheme_boundary,
    prev_grapheme_boundary, word_range_at,
};
