//! A multi-line code/text editor bound to `State<String>`.
//!
//! `TextEditor` is the framework's IDE-grade editor primitive. It builds on the
//! same retained [`TextInputState`] and [`TextEdit`] pipeline as
//! [`TextInput`](super::TextInput), but adds:
//!
//! - logical/visual line aware hit testing via `kasumi-text`'s `TextLayout`;
//! - a line-number gutter, current-line band, per-line selection rectangles;
//! - two-axis scrolling that follows the caret;
//! - UAX #29 grapheme/word boundaries, bracket matching and tab stops;
//! - auto-indent, bracket auto-pairing, line duplication/movement/commenting;
//! - undo/redo, IME pre-edit, clipboard and a built-in context menu;
//! - pluggable syntax highlighting through [`Highlighter`].
//!
//! The authoritative value stays in the application's `State<String>`; the
//! widget keeps only transient selection, scroll and undo state on the tree
//! node. Highlighting is recomputed per frame from the current value and does
//! not affect shaping, so the layout cache stays effective.

use std::sync::Arc;

use kasumi_core::{
    Align, Color, Corners, CursorIcon, Edges, Id, Length, Rect, Style, TextAlign, TextStyle,
    TextWrap,
};
use kasumi_paint::TextAnalysis;
use kasumi_state::State;
use kasumi_tree::{EditorPaint, NodeKind, Sense, TextInputState};

use crate::response::{Response, ResponseFlags};
use crate::runtime::ClipboardAction;
use crate::ui::{Ui, Widget};

use super::editor_ops::{EditorConfig, apply_edits, bracket_highlights, follow_caret};
use super::highlight::{Highlighter, SyntaxColors, SyntaxHighlighter};
use super::text_input::text_menu;

/// A multi-line editor bound to `State<String>`.
#[derive(Clone)]
pub struct TextEditor {
    value: State<String>,
    placeholder: String,
    width: Length,
    height: f32,
    font_size: Option<f32>,
    line_height: Option<f32>,
    enabled: bool,
    context_menu: bool,
    highlighter: Option<Arc<dyn Highlighter>>,
    tab_width: usize,
    insert_spaces: bool,
    show_line_numbers: bool,
    show_whitespace: bool,
    highlight_current_line: bool,
    highlight_trailing_whitespace: bool,
    wrap: bool,
    guide_column: usize,
    auto_indent: bool,
    line_comment: Option<String>,
    monospace: bool,
    selection: Option<(usize, usize)>,
}

impl TextEditor {
    pub fn new(value: &State<String>) -> Self {
        Self {
            value: value.clone(),
            placeholder: String::new(),
            width: Length::Percent(100.0),
            height: 320.0,
            font_size: None,
            line_height: None,
            enabled: true,
            context_menu: true,
            highlighter: None,
            tab_width: 4,
            insert_spaces: true,
            show_line_numbers: true,
            show_whitespace: false,
            highlight_current_line: true,
            highlight_trailing_whitespace: false,
            wrap: false,
            guide_column: 0,
            auto_indent: true,
            line_comment: None,
            monospace: true,
            selection: None,
        }
    }

    pub fn placeholder(mut self, text: impl Into<String>) -> Self {
        self.placeholder = text.into();
        self
    }

    pub fn width(mut self, width: impl Into<Length>) -> Self {
        self.width = width.into();
        self
    }

    pub fn height(mut self, height: f32) -> Self {
        self.height = height;
        self
    }

    pub fn font_size(mut self, size: f32) -> Self {
        self.font_size = Some(size);
        self
    }

    /// Explicit line height; defaults to `font_size * 1.5`.
    pub fn line_height(mut self, height: f32) -> Self {
        self.line_height = Some(height);
        self
    }

    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Enables the built-in Cut/Copy/Paste/Select All/Undo/Redo menu.
    pub fn context_menu(mut self, enabled: bool) -> Self {
        self.context_menu = enabled;
        self
    }

    /// Installs a syntax highlighter. The built-in [`SyntaxHighlighter`]
    /// covers Rust, C, Python, JSON and shell; any [`Highlighter`] works.
    /// The highlighter's line-comment prefix is used by Ctrl+/ unless
    /// [`Self::line_comment`] overrides it.
    pub fn highlighter(mut self, highlighter: impl Highlighter + 'static) -> Self {
        self.line_comment = highlighter.line_comment().map(str::to_owned);
        self.highlighter = Some(Arc::new(highlighter));
        self
    }

    /// Enables the built-in Rust highlighter with the given palette.
    pub fn rust_syntax(mut self, colors: SyntaxColors) -> Self {
        self.highlighter = Some(Arc::new(SyntaxHighlighter::rust(colors)));
        self.line_comment = Some("//".to_owned());
        self
    }

    /// Removes syntax highlighting (plain text).
    pub fn plain(mut self) -> Self {
        self.highlighter = None;
        self.line_comment = None;
        self
    }

    pub fn tab_width(mut self, width: usize) -> Self {
        self.tab_width = width.max(1);
        self
    }

    /// When true (default) pressing Tab inserts spaces; otherwise a tab char.
    pub fn insert_spaces(mut self, value: bool) -> Self {
        self.insert_spaces = value;
        self
    }

    pub fn line_numbers(mut self, show: bool) -> Self {
        self.show_line_numbers = show;
        self
    }

    pub fn show_whitespace(mut self, show: bool) -> Self {
        self.show_whitespace = show;
        self
    }

    pub fn highlight_current_line(mut self, show: bool) -> Self {
        self.highlight_current_line = show;
        self
    }

    pub fn highlight_trailing_whitespace(mut self, show: bool) -> Self {
        self.highlight_trailing_whitespace = show;
        self
    }

    /// Soft-wrap long lines. Off by default, like a source editor.
    pub fn wrap(mut self, wrap: bool) -> Self {
        self.wrap = wrap;
        self
    }

    /// Draws a vertical guide at this character column (0 disables it).
    pub fn guide_column(mut self, column: usize) -> Self {
        self.guide_column = column;
        self
    }

    pub fn auto_indent(mut self, enabled: bool) -> Self {
        self.auto_indent = enabled;
        self
    }

    /// Line-comment prefix used by Ctrl+/; inferred from the highlighter.
    pub fn line_comment(mut self, prefix: impl Into<String>) -> Self {
        self.line_comment = Some(prefix.into());
        self
    }

    pub fn monospace(mut self, value: bool) -> Self {
        self.monospace = value;
        self
    }

    /// Programmatically places the caret (and optionally selects) in byte
    /// offsets for the next render.
    ///
    /// This is the entry point for go-to-definition, search results and
    /// completion acceptance: the widget clamps both offsets to the current
    /// text and scrolls the caret into view. Pass the same offset twice to
    /// place a caret without a selection.
    pub fn selection(mut self, anchor: usize, cursor: usize) -> Self {
        self.selection = Some((anchor, cursor));
        self
    }
}

/// Status information for the current frame, returned alongside the response.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EditorStatus {
    pub line: usize,
    pub column: usize,
    pub line_count: usize,
    pub selection_len: usize,
    pub changed: bool,
    /// Caret offset as a UTF-8 byte index into the bound value.
    pub byte: usize,
    /// Selection as UTF-8 byte offsets, `None` when the selection is empty.
    pub selection: Option<(usize, usize)>,
    /// Caret rectangle in window coordinates, for popups anchored to the caret.
    pub caret_rect: Option<kasumi_core::Rect>,
    /// Byte offset under the pointer, when the editor is hovered. Used by IDE
    /// applications for hover documentation and symbol highlighting.
    pub hover_byte: Option<usize>,
    /// Pointer position relative to the editor node, for positioning tooltips.
    pub hover_point: Option<kasumi_core::Point>,
}

impl Widget for TextEditor {
    type Output = (Response, EditorStatus);

    fn ui(self, ui: &mut Ui) -> (Response, EditorStatus) {
        let theme = ui.theme().clone();
        let id: Id = ui.next_id("text editor");
        let node = ui.cx.tree.ensure(id, NodeKind::TextEditor);
        ui.declare_child(ui.node, node);

        let focused = ui.cx.focused == Some(id) && self.enabled;
        let mut value = self.value.get();
        let mut editor = ui
            .cx
            .tree
            .get_mut(node)
            .and_then(|n| n.text_input.take())
            .unwrap_or_default();
        if !focused {
            editor.preedit.clear();
            editor.preedit_cursor = None;
        }
        if editor.last_value.as_deref() != Some(value.as_str()) {
            editor.anchor = clamp(&value, editor.anchor);
            editor.cursor = clamp(&value, editor.cursor);
            editor.preedit.clear();
            editor.preedit_cursor = None;
        }

        let font_size = self.font_size.unwrap_or(theme.typography.body_medium.size);
        let line_height = self.line_height.unwrap_or((font_size * 1.5).max(12.0));

        // BUG FIX: `str::lines()` under-counts when the value ends with '\n'
        // (e.g. "a\nb\n" is 3 logical lines, not 2). Count newlines directly.
        let logical_lines = value.bytes().filter(|&b| b == b'\n').count() + 1;
        let gutter_width = if self.show_line_numbers {
            16.0 + (logical_lines.to_string().len() as f32) * font_size * 0.62
        } else {
            0.0
        };
        let rect_width = resolve_length(self.width, ui.cx.input.viewport.width);
        let viewport_h = self.height.max(1.0);

        let config = EditorConfig {
            tab_width: self.tab_width,
            insert_spaces: self.insert_spaces,
            auto_indent: self.auto_indent,
            line_comment: self.line_comment.clone(),
            family: editor_family(&self),
            font_size,
            line_height,
            viewport_height: viewport_h,
            gutter_width,
            viewport_width: rect_width,
            wrap: self.wrap,
        };

        let outcome = apply_edits(ui, &mut value, &mut editor, &config, self.enabled);
        let wrote_selection = self.selection.is_some();
        if let Some((anchor, cursor)) = self.selection {
            editor.anchor = clamp(&value, anchor);
            editor.cursor = clamp(&value, cursor);
            editor.goal_column = 0.0;
            editor.preedit.clear();
            editor.preedit_cursor = None;
        }
        if outcome.changed {
            self.value.set(value.clone());
            // BUG FIX: any edit terminates an in-flight composition. Keeping
            // the stale pre-edit would splice it again on the next frame.
            editor.preedit.clear();
            editor.preedit_cursor = None;
        }
        editor.anchor = clamp(&value, editor.anchor);
        editor.cursor = clamp(&value, editor.cursor);

        // IME pre-edit: splice the composition string into the painted text.
        let mut display = value.clone();
        let mut paint_caret = editor.cursor;
        let mut paint_anchor = editor.anchor;
        if focused && !editor.preedit.is_empty() {
            let start = clamp(&value, editor.anchor.min(editor.cursor));
            let end = clamp(&value, editor.anchor.max(editor.cursor));
            display.replace_range(start..end, "");
            display.insert_str(start, &editor.preedit);
            let cursor_in_preedit = editor
                .preedit_cursor
                .map(|(cursor, _)| cursor)
                .map(|c| clamp(&editor.preedit, c))
                .unwrap_or(editor.preedit.len());
            paint_caret = start + cursor_in_preedit;
            // BUG FIX: collapse the selection during composition. The
            // pre-edit itself is rendered specially; showing the prefix of
            // the composition string as a "selection" is wrong.
            paint_anchor = paint_caret;
        }

        // Syntax highlighting.
        let mut spans = self
            .highlighter
            .as_ref()
            .map(|h| h.highlight(&display))
            .unwrap_or_default();
        if self.highlight_trailing_whitespace {
            add_trailing_whitespace_spans(&display, &mut spans, theme.colors.error.fade(0.35));
        }
        let analysis = TextAnalysis::new(&display);

        // BUG FIX: when wrap is disabled, do not cap the layout width;
        // otherwise overlong lines get truncated horizontally and the
        // horizontal scroll range never grows.
        let content_width = (rect_width - gutter_width).max(1.0);
        let max_width = if self.wrap { Some(content_width) } else { None };
        let layout = ui.texts().layout(
            &display,
            &text_style(&self, font_size, line_height),
            max_width,
        );

        // Publish the new content size *before* reading scroll, so any
        // clamping done inside the tree is visible to us.
        let content_size =
            kasumi_core::Size::new(layout.size.width + gutter_width, layout.size.height);
        ui.cx.tree.set_scroll_content(node, content_size);

        let mut scroll = ui
            .cx
            .tree
            .get(node)
            .map(|n| n.scroll.offset)
            .unwrap_or_default();
        editor.scroll_x = scroll.x;
        editor.scroll_y = scroll.y;

        // BUG FIX: only follow the caret when the edit engine requests it.
        // Previously this ran on every focused frame, which yanked the
        // viewport back to the caret and defeated manual scrolling.
        let followed = outcome.follow_caret || wrote_selection;
        if followed {
            follow_caret(ui, &display, &mut editor, &config, paint_caret);
            scroll = kasumi_core::Point::new(editor.scroll_x, editor.scroll_y);
        }

        // BUG FIX: clamp against the (possibly shrunk) content size so we
        // never scroll past the new maximum.
        let viewport = kasumi_core::Size::new(rect_width, viewport_h);
        let max_x = (content_size.width - viewport.width).max(0.0);
        let max_y = (content_size.height - viewport.height).max(0.0);
        scroll.x = scroll.x.clamp(0.0, max_x);
        scroll.y = scroll.y.clamp(0.0, max_y);

        // BUG FIX: never overwrite `scroll.target` unconditionally — that
        // cancels in-flight wheel/animation scrolls. Only sync it when we
        // actually followed the caret; otherwise just clamp the pending
        // target to the new range.
        if let Some(n) = ui.cx.tree.get_mut(node) {
            n.scroll.offset = scroll;
            if followed {
                n.scroll.target = scroll;
            } else {
                n.scroll.target.x = n.scroll.target.x.clamp(0.0, max_x);
                n.scroll.target.y = n.scroll.target.y.clamp(0.0, max_y);
            }
        }
        editor.scroll_x = scroll.x;
        editor.scroll_y = scroll.y;

        let (scroll_x, scroll_y) = (scroll.x, scroll.y);
        let caret_line = layout.logical_line_of_byte(paint_caret);
        let selection = (paint_anchor.min(paint_caret), paint_anchor.max(paint_caret));
        let highlights = bracket_highlights(&display, paint_caret);

        let paint = EditorPaint {
            gutter_width,
            gutter_background: theme.colors.surface_container_low,
            gutter_color: theme.colors.on_surface_variant.fade(0.7),
            current_line_color: theme.colors.primary,
            current_line_background: theme.colors.primary.fade(0.07),
            selection_color: theme.colors.primary.fade(0.28),
            caret_color: theme.colors.primary,
            whitespace_color: theme.colors.outline.fade(0.55),
            bracket_color: theme.colors.tertiary,
            guide_color: theme.colors.outline_variant.fade(0.4),
            guide_column: self.guide_column,
            background: theme.colors.surface_container_lowest,
            border_color: theme.colors.outline_variant,
            tab_width: self.tab_width,
            insert_spaces: self.insert_spaces,
            show_whitespace: self.show_whitespace,
            highlight_trailing_whitespace: self.highlight_trailing_whitespace,
            highlight_current_line: self.highlight_current_line,
            show_line_numbers: self.show_line_numbers,
            bracket_highlights: highlights,
            spans: spans.clone(),
            caret: paint_caret,
            selection,
            scroll: (scroll_x, scroll_y),
            current_line: caret_line,
            show_caret: focused,
            enabled: self.enabled,
        };

        let mut text_style = text_style(&self, font_size, line_height);
        text_style.color = if !self.enabled {
            theme.colors.on_surface.fade(theme.state.disabled_content)
        } else if display.is_empty() && !focused {
            theme.colors.on_surface_variant
        } else {
            theme.colors.on_surface
        };
        let display_text = if display.is_empty() && !focused && !self.placeholder.is_empty() {
            self.placeholder.clone()
        } else {
            display.clone()
        };

        let style = Style {
            width: self.width,
            height: Length::Px(self.height),
            min_height: Length::Px(self.height),
            padding: Edges::all(0.0),
            align_items: Align::Start,
            background: theme.colors.surface_container_lowest,
            border_color: theme.colors.outline_variant,
            border_width: 1.0,
            radius: Corners::all(theme.shape.extra_small),
            cursor: if self.enabled {
                CursorIcon::Text
            } else {
                CursorIcon::NotAllowed
            },
            focus_ring: theme.colors.primary,
            align_self: Align::Stretch,
            clip: true,
            scroll_x: true,
            scroll_y: true,
            text: text_style.clone(),
            ..Style::default()
        };

        editor.paint_anchor = paint_anchor;
        editor.paint_cursor = paint_caret;
        editor.last_value = Some(value);
        ui.set_style_on(node, style);
        ui.cx.tree.set_enabled(node, self.enabled);
        ui.set_sense_on(
            node,
            if self.enabled {
                Sense::click().union(Sense::DRAG)
            } else {
                Sense::NONE
            },
        );
        ui.set_text_on_with_spans(node, display_text, text_style, spans);
        if let Some(n) = ui.cx.tree.get_mut(node) {
            n.text_input = Some(editor);
            n.editor = Some(paint);
            n.changed = outcome.changed;
        }

        let mut response = ui.response_of(id);
        if outcome.changed {
            response.insert_flag(ResponseFlags::CHANGED);
        }
        if self.context_menu
            && let Some(selection) = ui.context_menu(&response, text_menu(response.id()))
        {
            match selection.index() {
                Some(0) => queue_edit(ui, id, kasumi_tree::TextEdit::Cut),
                Some(1) => queue_edit(ui, id, kasumi_tree::TextEdit::Copy),
                Some(2) => {
                    ui.cx.clipboard_action = Some(ClipboardAction::ReadText);
                    ui.cx.clipboard_target = Some(id);
                    ui.request_repaint();
                }
                Some(4) => queue_edit(ui, id, kasumi_tree::TextEdit::SelectAll),
                Some(5) => queue_edit(ui, id, kasumi_tree::TextEdit::Undo),
                Some(6) => queue_edit(ui, id, kasumi_tree::TextEdit::Redo),
                _ => {}
            }
        }

        // BUG FIX: status used to expose raw byte offsets, which are wrong
        // for any non-ASCII text. Count chars instead.
        let line_start = analysis.line_start(caret_line);
        let column = display[line_start..paint_caret].chars().count();
        let selection_len = display[selection.0..selection.1].chars().count();
        let caret = layout.caret_at(paint_caret);
        let node_rect = ui
            .cx
            .tree
            .get(node)
            .map(|node| node.rect)
            .unwrap_or_default();
        let caret_rect = Some(Rect::from_xywh(
            node_rect.origin.x + gutter_width + caret.x - scroll_x,
            node_rect.origin.y + caret.y - scroll_y,
            2.0,
            caret.height,
        ));

        let hover = ui.cx.input.pointer;
        let hover_byte = if response.hovered() && ui.cx.input.pointer_inside {
            let local_x = hover.x - node_rect.origin.x + scroll_x - gutter_width;
            let local_y = hover.y - node_rect.origin.y + scroll_y;
            Some(layout.byte_at_point(&display, local_x, local_y))
        } else {
            None
        };
        let hover_point = response
            .hovered()
            .then(|| kasumi_core::Point::new(hover.x, hover.y));

        let status = EditorStatus {
            line: caret_line + 1,
            column,
            line_count: analysis.line_count(),
            selection_len,
            changed: outcome.changed,
            byte: paint_caret,
            selection: (selection.0 != selection.1).then_some(selection),
            caret_rect,
            hover_byte,
            hover_point,
        };
        (response, status)
    }
}

fn editor_family(editor: &TextEditor) -> kasumi_core::FontFamily {
    if editor.monospace {
        kasumi_core::FontFamily::Monospace
    } else {
        kasumi_core::FontFamily::SansSerif
    }
}

fn text_style(editor: &TextEditor, size: f32, line_height: f32) -> TextStyle {
    TextStyle {
        family: editor_family(editor),
        size,
        line_height: Some(line_height),
        color: Color::WHITE,
        align: TextAlign::Start,
        wrap: if editor.wrap {
            TextWrap::Word
        } else {
            TextWrap::None
        },
        ..TextStyle::default()
    }
}

fn add_trailing_whitespace_spans(text: &str, spans: &mut Vec<kasumi_core::TextSpan>, color: Color) {
    let analysis = TextAnalysis::new(text);
    for line in 0..analysis.line_count() {
        let range = analysis.line_range(line);
        let line_text = &text[range.start..range.end];
        // BUG FIX: `line_range` typically includes the trailing newline, so
        // trimming spaces/tabs off the raw slice never matched. Strip the
        // newline first, then trim whitespace.
        let body = line_text.trim_end_matches(['\r', '\n']);
        let trimmed = body.trim_end_matches([' ', '\t']);
        if trimmed.len() < body.len() {
            spans.push(kasumi_core::TextSpan::new(
                range.start + trimmed.len(),
                range.start + body.len(),
                color,
            ));
        }
    }
}

fn queue_edit(ui: &mut Ui, id: Id, edit: kasumi_tree::TextEdit) {
    if let Some(node) = ui.cx.tree.node_by_id_mut(id) {
        node.text_input
            .get_or_insert_with(TextInputState::default)
            .pending
            .push(edit);
    }
    ui.request_repaint();
}

fn clamp(text: &str, mut byte: usize) -> usize {
    byte = byte.min(text.len());
    while byte > 0 && !text.is_char_boundary(byte) {
        byte -= 1;
    }
    byte
}

fn resolve_length(length: Length, parent: f32) -> f32 {
    length.resolve(Some(parent)).unwrap_or(parent)
}
