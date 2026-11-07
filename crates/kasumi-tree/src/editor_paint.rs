//! Resolved paint data for a multi-line [`TextEditor`](crate::NodeKind::TextEditor).
//!
//! The widget resolves theme colours and geometry once per frame and stores
//! them on the node; the paint layer only replays them. This keeps
//! `kasumi-paint` independent of the theme crate and `kasumi-tree` free of
//! dependencies beyond `kasumi-core`.

use kasumi_core::{Color, TextSpan};

/// Everything the paint pass needs to draw one frame of a code editor.
#[derive(Debug, Clone, PartialEq)]
pub struct EditorPaint {
    /// Width of the line-number gutter in logical pixels.
    pub gutter_width: f32,
    /// Colour of the gutter background.
    pub gutter_background: Color,
    /// Colour of line numbers.
    pub gutter_color: Color,
    /// Colour of the current line's number and its highlight band.
    pub current_line_color: Color,
    /// Colour of the current-line band (a translucent primary tint).
    pub current_line_background: Color,
    /// Selection highlight colour.
    pub selection_color: Color,
    /// Caret colour.
    pub caret_color: Color,
    /// Colour used for visible whitespace glyphs.
    pub whitespace_color: Color,
    /// Colour for matching-bracket highlights.
    pub bracket_color: Color,
    /// Colour for the right-hand guide at `guide_column`, if any.
    pub guide_color: Color,
    /// Draw a vertical guide at this character column (0 disables it).
    pub guide_column: usize,
    /// Colour of the editor surface.
    pub background: Color,
    /// Colour of the editor border.
    pub border_color: Color,
    /// Number of spaces a tab represents.
    pub tab_width: usize,
    /// Replace tabs with spaces when editing.
    pub insert_spaces: bool,
    /// Show dots/arrows for spaces and tabs.
    pub show_whitespace: bool,
    /// Highlight trailing whitespace.
    pub highlight_trailing_whitespace: bool,
    /// Draw a band behind the current logical line.
    pub highlight_current_line: bool,
    /// Show line numbers in the gutter.
    pub show_line_numbers: bool,
    /// Byte offsets of matching brackets to highlight, if any.
    pub bracket_highlights: Vec<usize>,
    /// Colour ranges for syntax highlighting, stored on the node's
    /// `TextContent` because the paint pass reads them there.
    pub spans: Vec<TextSpan>,
    /// Caret byte offset in the painted (IME-adjusted) string.
    pub caret: usize,
    /// Selection endpoints in the painted string.
    pub selection: (usize, usize),
    /// Horizontal and vertical scroll offsets in logical pixels.
    pub scroll: (f32, f32),
    /// Logical line number under the caret, for the gutter highlight.
    pub current_line: usize,
    /// Whether the caret should be visible (editor focused).
    pub show_caret: bool,
    /// Whether the editor accepts input.
    pub enabled: bool,
}

impl Default for EditorPaint {
    fn default() -> Self {
        Self {
            gutter_width: 0.0,
            gutter_background: Color::TRANSPARENT,
            gutter_color: Color::rgba(0.5, 0.5, 0.5, 1.0),
            current_line_color: Color::WHITE,
            current_line_background: Color::TRANSPARENT,
            selection_color: Color::rgba(0.3, 0.5, 1.0, 0.28),
            caret_color: Color::WHITE,
            whitespace_color: Color::rgba(0.5, 0.5, 0.5, 0.4),
            bracket_color: Color::rgba(0.4, 0.8, 1.0, 0.9),
            guide_color: Color::rgba(0.5, 0.5, 0.5, 0.2),
            guide_column: 0,
            background: Color::TRANSPARENT,
            border_color: Color::TRANSPARENT,
            tab_width: 4,
            insert_spaces: true,
            show_whitespace: false,
            highlight_trailing_whitespace: false,
            highlight_current_line: true,
            show_line_numbers: true,
            bracket_highlights: Vec::new(),
            spans: Vec::new(),
            caret: 0,
            selection: (0, 0),
            scroll: (0.0, 0.0),
            current_line: 0,
            show_caret: false,
            enabled: true,
        }
    }
}
