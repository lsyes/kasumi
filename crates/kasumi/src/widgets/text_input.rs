use kasumi_core::{
    Align, Corners, CursorIcon, Edges, FontFamily, Id, Length, Style, TextStyle, TextWrap,
};
use kasumi_paint::TextEngine;
use kasumi_state::State;
use kasumi_tree::{NodeKind, Sense, TextEdit, TextEditSnapshot, TextInputState, TextMotion};

use crate::response::Response;
use crate::runtime::ClipboardAction;
use crate::ui::{Ui, Widget};

/// A single-line editor bound to `State<String>`.
///
/// The editor supports mouse and keyboard selection, word movement,
/// horizontal scrolling, undo/redo, IME pre-edit, and the standard clipboard
/// operations. The authoritative value remains the application's `State`.
#[derive(Clone)]
pub struct TextInput {
    value: State<String>,
    placeholder: String,
    width: Length,
    height: f32,
    font_size: Option<f32>,
    enabled: bool,
    context_menu: bool,
}

impl TextInput {
    pub fn new(value: &State<String>) -> Self {
        Self {
            value: value.clone(),
            placeholder: String::new(),
            width: Length::Px(280.0),
            height: 40.0,
            font_size: None,
            enabled: true,
            context_menu: true,
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

    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Enables the built-in Cut/Copy/Paste/Select All menu. Disable this when
    /// supplying a custom menu with `Ui::context_menu`.
    pub fn context_menu(mut self, enabled: bool) -> Self {
        self.context_menu = enabled;
        self
    }
}

impl Widget for TextInput {
    type Output = Response;

    fn ui(self, ui: &mut Ui) -> Response {
        let theme = ui.theme().clone();
        let id: Id = ui.next_id("text input");
        let node = ui.cx.tree.ensure(id, NodeKind::TextInput);
        ui.declare_child(ui.node, node);

        let focused = ui.cx.focused == Some(id) && self.enabled;
        let mut value = self.value.get();
        let mut editor = ui
            .cx
            .tree
            .get_mut(node)
            .and_then(|node| node.text_input.take())
            .unwrap_or_default();
        if !focused {
            editor.preedit.clear();
            editor.preedit_cursor = None;
        }
        if editor.last_value.as_deref() != Some(value.as_str()) {
            editor.anchor = clamp_boundary(&value, editor.anchor);
            editor.cursor = clamp_boundary(&value, editor.cursor);
            editor.preedit.clear();
        }

        let edits = std::mem::take(&mut editor.pending);
        let mut changed = false;
        for edit in edits {
            if !self.enabled {
                editor.preedit.clear();
                editor.preedit_cursor = None;
                break;
            }
            match edit {
                TextEdit::Insert(text) => {
                    changed |= insert_text(&mut value, &mut editor, &text);
                }
                TextEdit::Backspace => {
                    changed |= backspace(&mut value, &mut editor, false);
                }
                TextEdit::BackspaceWord => {
                    changed |= backspace(&mut value, &mut editor, true);
                }
                TextEdit::Delete => {
                    changed |= delete_forward(&mut value, &mut editor, false);
                }
                TextEdit::DeleteWord => {
                    changed |= delete_forward(&mut value, &mut editor, true);
                }
                TextEdit::MoveCaret {
                    motion,
                    extend,
                    by_word,
                } => move_caret(&value, &mut editor, motion, extend, by_word),
                TextEdit::SelectAll => {
                    editor.anchor = 0;
                    editor.cursor = value.len();
                }
                TextEdit::PointerDown { x, extend } => {
                    pointer_select(
                        ui.texts(),
                        &value,
                        &mut editor,
                        x,
                        extend,
                        self.font_size.unwrap_or(theme.font_size),
                    );
                }
                TextEdit::PointerDrag { x } => {
                    pointer_select(
                        ui.texts(),
                        &value,
                        &mut editor,
                        x,
                        true,
                        self.font_size.unwrap_or(theme.font_size),
                    );
                }
                TextEdit::PointerUp => {}
                TextEdit::SelectWord { x } => {
                    select_word(
                        ui.texts(),
                        &value,
                        &mut editor,
                        x,
                        self.font_size.unwrap_or(theme.font_size),
                    );
                }
                TextEdit::Paste(text) => {
                    changed |= paste_text(&mut value, &mut editor, &text);
                }
                TextEdit::Copy => {
                    if let Some(text) = selected_text(&value, &editor)
                        && !text.is_empty()
                    {
                        ui.cx.clipboard_action = Some(ClipboardAction::SetText(text));
                    }
                }
                TextEdit::Cut => {
                    if let Some(text) = selected_text(&value, &editor)
                        && !text.is_empty()
                    {
                        ui.cx.clipboard_action = Some(ClipboardAction::SetText(text));
                        push_undo(
                            &value,
                            editor.anchor,
                            editor.cursor,
                            &mut editor.undo,
                            &mut editor.redo,
                        );
                        delete_selection(&mut value, &mut editor);
                        changed = true;
                    }
                }
                TextEdit::Undo => {
                    changed |= undo(&mut value, &mut editor);
                }
                TextEdit::Redo => {
                    changed |= redo(&mut value, &mut editor);
                }
                TextEdit::SetPreedit { text, cursor } => {
                    editor.preedit = text;
                    editor.preedit_cursor = cursor;
                }
            }
        }

        editor.anchor = clamp_boundary(&value, editor.anchor);
        editor.cursor = clamp_boundary(&value, editor.cursor);
        if changed {
            self.value.set(value.clone());
        }

        let style_text = TextStyle {
            size: self.font_size.unwrap_or(theme.font_size),
            color: if value.is_empty() && !focused {
                theme.text_subtle
            } else {
                theme.text
            },
            family: FontFamily::SansSerif,
            wrap: TextWrap::None,
            ..TextStyle::default()
        };
        let content_rect_width =
            resolve_length(self.width, ui.cx.input.viewport.width) - style_padding_horizontal();
        let mut display = if value.is_empty() && !focused {
            self.placeholder.clone()
        } else {
            value.clone()
        };
        if focused && !editor.preedit.is_empty() {
            let start = clamp_boundary(&value, editor.anchor.min(editor.cursor));
            let end = clamp_boundary(&value, editor.anchor.max(editor.cursor));
            display.replace_range(start..end, "");
            display.insert_str(start, &editor.preedit);
            let preedit_cursor = editor
                .preedit_cursor
                .map(|(cursor, _)| cursor)
                .map(|cursor| clamp_boundary(&editor.preedit, cursor))
                .unwrap_or(editor.preedit.len());
            editor.paint_cursor = start + preedit_cursor;
            editor.paint_anchor = start;
        } else {
            editor.paint_cursor = editor.cursor;
            editor.paint_anchor = editor.anchor;
        }

        if focused && !display.is_empty() {
            let layout = ui.texts().layout(&display, &style_text, None);
            let caret = layout.caret_x(editor.paint_cursor);
            editor.scroll_x = scroll_to_caret(
                editor.scroll_x,
                caret,
                content_rect_width,
                layout.size.width,
            );
        } else if !focused {
            editor.scroll_x = 0.0;
        }

        let style = Style {
            width: self.width,
            height: Length::Px(self.height),
            min_height: Length::Px(self.height),
            padding: Edges::xy(12.0, 0.0),
            align_items: Align::Center,
            background: theme.surface,
            border_color: if focused {
                theme.border_strong
            } else {
                theme.border
            },
            border_width: 1.0,
            radius: Corners::all(theme.radius_medium),
            cursor: if self.enabled {
                CursorIcon::Text
            } else {
                CursorIcon::NotAllowed
            },
            focus_ring: theme.focus_ring,
            align_self: Align::Start,
            clip: true,
            text: style_text.clone(),
            ..Style::default()
        };

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
        ui.set_text_on(node, display, style_text);
        if let Some(node) = ui.cx.tree.get_mut(node) {
            node.text_input = Some(editor);
            node.changed = changed;
        }

        let mut response = ui.response_of(id);
        if changed {
            response.insert_flag(crate::response::ResponseFlags::CHANGED);
        }
        if self.context_menu {
            ui.context_menu(&response, |ui| {
                let full = |button: crate::Button| button.menu_item();
                if ui.add(full(crate::Button::new("Cut"))).clicked() {
                    queue_edit(ui, id, TextEdit::Cut);
                }
                if ui.add(full(crate::Button::new("Copy"))).clicked() {
                    queue_edit(ui, id, TextEdit::Copy);
                }
                if ui.add(full(crate::Button::new("Paste"))).clicked() {
                    ui.cx.clipboard_action = Some(ClipboardAction::ReadText);
                    ui.cx.clipboard_target = Some(id);
                    ui.request_repaint();
                }
                ui.divider();
                if ui.add(full(crate::Button::new("Select all"))).clicked() {
                    queue_edit(ui, id, TextEdit::SelectAll);
                }
                if ui.add(full(crate::Button::new("Undo"))).clicked() {
                    queue_edit(ui, id, TextEdit::Undo);
                }
                if ui.add(full(crate::Button::new("Redo"))).clicked() {
                    queue_edit(ui, id, TextEdit::Redo);
                }
            });
        }
        response
    }
}

fn queue_edit(ui: &mut Ui, id: Id, edit: TextEdit) {
    if let Some(node) = ui.cx.tree.node_by_id_mut(id) {
        node.text_input
            .get_or_insert_with(TextInputState::default)
            .pending
            .push(edit);
    }
    ui.request_repaint();
}

fn push_undo(
    value: &str,
    anchor: usize,
    cursor: usize,
    undo: &mut Vec<TextEditSnapshot>,
    redo: &mut Vec<TextEditSnapshot>,
) {
    undo.push(TextEditSnapshot {
        text: value.to_owned(),
        anchor,
        cursor,
    });
    if undo.len() > 128 {
        undo.remove(0);
    }
    redo.clear();
}

fn selected_text(value: &str, editor: &TextInputState) -> Option<String> {
    let start = clamp_boundary(value, editor.anchor.min(editor.cursor));
    let end = clamp_boundary(value, editor.anchor.max(editor.cursor));
    (start != end).then(|| value[start..end].to_owned())
}

fn delete_selection(value: &mut String, editor: &mut TextInputState) -> bool {
    let start = clamp_boundary(value, editor.anchor.min(editor.cursor));
    let end = clamp_boundary(value, editor.anchor.max(editor.cursor));
    if start == end {
        return false;
    }
    value.replace_range(start..end, "");
    editor.anchor = start;
    editor.cursor = start;
    true
}

fn normalize_insert(text: &str) -> String {
    text.chars()
        .filter_map(|ch| match ch {
            '\n' | '\r' | '\t' => Some(' '),
            ch if ch.is_control() => None,
            ch => Some(ch),
        })
        .collect()
}

/// Single-line paste normalizes line endings before collapsing whitespace, so
/// CRLF contributes one separator rather than two.
fn normalize_paste(text: &str) -> String {
    text.replace("\r\n", "\n")
        .replace('\r', "\n")
        .chars()
        .filter_map(|ch| match ch {
            '\n' | '\t' => Some(' '),
            ch if ch.is_control() => None,
            ch => Some(ch),
        })
        .collect()
}

fn paste_text(value: &mut String, editor: &mut TextInputState, text: &str) -> bool {
    insert_text(value, editor, &normalize_paste(text))
}

fn insert_text(value: &mut String, editor: &mut TextInputState, text: &str) -> bool {
    let text = normalize_insert(text);
    if text.is_empty() {
        return false;
    }
    push_undo(
        value,
        editor.anchor,
        editor.cursor,
        &mut editor.undo,
        &mut editor.redo,
    );
    delete_selection(value, editor);
    editor.cursor = clamp_boundary(value, editor.cursor);
    value.insert_str(editor.cursor, &text);
    editor.cursor += text.len();
    editor.anchor = editor.cursor;
    editor.preedit.clear();
    editor.preedit_cursor = None;
    true
}

fn backspace(value: &mut String, editor: &mut TextInputState, by_word: bool) -> bool {
    if editor.anchor != editor.cursor {
        push_undo(
            value,
            editor.anchor,
            editor.cursor,
            &mut editor.undo,
            &mut editor.redo,
        );
        return delete_selection(value, editor);
    }
    let cursor = clamp_boundary(value, editor.cursor);
    let previous = if by_word {
        previous_word_boundary(value, cursor)
    } else {
        previous_boundary(value, cursor)
    };
    if previous == cursor {
        return false;
    }
    push_undo(
        value,
        editor.anchor,
        editor.cursor,
        &mut editor.undo,
        &mut editor.redo,
    );
    value.replace_range(previous..cursor, "");
    editor.anchor = previous;
    editor.cursor = previous;
    true
}

fn delete_forward(value: &mut String, editor: &mut TextInputState, by_word: bool) -> bool {
    if editor.anchor != editor.cursor {
        push_undo(
            value,
            editor.anchor,
            editor.cursor,
            &mut editor.undo,
            &mut editor.redo,
        );
        return delete_selection(value, editor);
    }
    let cursor = clamp_boundary(value, editor.cursor);
    let next = if by_word {
        next_word_boundary(value, cursor)
    } else {
        next_boundary(value, cursor)
    };
    if next == cursor {
        return false;
    }
    push_undo(
        value,
        editor.anchor,
        editor.cursor,
        &mut editor.undo,
        &mut editor.redo,
    );
    value.replace_range(cursor..next, "");
    editor.anchor = cursor;
    editor.cursor = cursor;
    true
}

fn move_caret(
    value: &str,
    editor: &mut TextInputState,
    motion: TextMotion,
    extend: bool,
    by_word: bool,
) {
    if !extend && editor.anchor != editor.cursor {
        let target = match motion {
            TextMotion::Left => editor.anchor.min(editor.cursor),
            TextMotion::Right => editor.anchor.max(editor.cursor),
            TextMotion::Home => 0,
            TextMotion::End => value.len(),
        };
        editor.cursor = target;
        editor.anchor = target;
        return;
    }
    let current = clamp_boundary(value, editor.cursor);
    editor.cursor = match motion {
        TextMotion::Left if by_word => previous_word_boundary(value, current),
        TextMotion::Right if by_word => next_word_boundary(value, current),
        TextMotion::Left => previous_boundary(value, current),
        TextMotion::Right => next_boundary(value, current),
        TextMotion::Home => 0,
        TextMotion::End => value.len(),
    };
    if !extend {
        editor.anchor = editor.cursor;
    }
}

fn pointer_select(
    texts: &TextEngine,
    value: &str,
    editor: &mut TextInputState,
    x: f32,
    extend: bool,
    font_size: f32,
) {
    let style = TextStyle {
        size: font_size,
        wrap: TextWrap::None,
        ..TextStyle::default()
    };
    let layout = texts.layout(value, &style, None);
    let byte = layout.byte_at_x(value, x + editor.scroll_x);
    editor.cursor = clamp_boundary(value, byte);
    if !extend {
        editor.anchor = editor.cursor;
    }
}

fn select_word(
    texts: &TextEngine,
    value: &str,
    editor: &mut TextInputState,
    x: f32,
    font_size: f32,
) {
    let style = TextStyle {
        size: font_size,
        wrap: TextWrap::None,
        ..TextStyle::default()
    };
    let layout = texts.layout(value, &style, None);
    let byte = layout.byte_at_x(value, x + editor.scroll_x);
    let byte = clamp_boundary(value, byte);
    let mut start = byte;
    let mut end = byte;
    while let Some((index, ch)) = value[..start].char_indices().next_back() {
        if ch.is_whitespace() {
            break;
        }
        start = index;
    }
    while let Some(ch) = value[end..].chars().next() {
        if ch.is_whitespace() {
            break;
        }
        end += ch.len_utf8();
    }
    editor.anchor = start;
    editor.cursor = end;
}

fn undo(value: &mut String, editor: &mut TextInputState) -> bool {
    let Some(previous) = editor.undo.pop() else {
        return false;
    };
    editor.redo.push(TextEditSnapshot {
        text: std::mem::take(value),
        anchor: editor.anchor,
        cursor: editor.cursor,
    });
    *value = previous.text;
    editor.anchor = clamp_boundary(value, previous.anchor);
    editor.cursor = clamp_boundary(value, previous.cursor);
    editor.preedit.clear();
    true
}

fn redo(value: &mut String, editor: &mut TextInputState) -> bool {
    let Some(next) = editor.redo.pop() else {
        return false;
    };
    editor.undo.push(TextEditSnapshot {
        text: std::mem::take(value),
        anchor: editor.anchor,
        cursor: editor.cursor,
    });
    *value = next.text;
    editor.anchor = clamp_boundary(value, next.anchor);
    editor.cursor = clamp_boundary(value, next.cursor);
    editor.preedit.clear();
    true
}

fn clamp_boundary(text: &str, mut byte: usize) -> usize {
    byte = byte.min(text.len());
    while byte > 0 && !text.is_char_boundary(byte) {
        byte -= 1;
    }
    byte
}

fn previous_boundary(text: &str, byte: usize) -> usize {
    let byte = clamp_boundary(text, byte);
    text[..byte]
        .char_indices()
        .next_back()
        .map(|(index, _)| index)
        .unwrap_or(0)
}

fn next_boundary(text: &str, byte: usize) -> usize {
    let byte = clamp_boundary(text, byte);
    text[byte..]
        .char_indices()
        .nth(1)
        .map(|(index, _)| byte + index)
        .unwrap_or(text.len())
}

fn previous_word_boundary(text: &str, byte: usize) -> usize {
    let mut index = clamp_boundary(text, byte);
    while let Some((previous, ch)) = text[..index].char_indices().next_back() {
        if ch.is_whitespace() {
            index = previous;
        } else {
            break;
        }
    }
    let mut in_word = false;
    while let Some((previous, ch)) = text[..index].char_indices().next_back() {
        if ch.is_whitespace() {
            break;
        }
        in_word = true;
        index = previous;
    }
    if !in_word { 0 } else { index }
}

fn next_word_boundary(text: &str, byte: usize) -> usize {
    let mut index = clamp_boundary(text, byte);
    while let Some(ch) = text[index..].chars().next() {
        if ch.is_whitespace() {
            index += ch.len_utf8();
        } else {
            break;
        }
    }
    while let Some(ch) = text[index..].chars().next() {
        if ch.is_whitespace() {
            break;
        }
        index += ch.len_utf8();
    }
    index
}

fn scroll_to_caret(current: f32, caret: f32, viewport: f32, text_width: f32) -> f32 {
    let viewport = viewport.max(1.0);
    const MARGIN: f32 = 4.0;
    let mut next = current;
    if caret - MARGIN < next {
        next = caret - MARGIN;
    } else if caret + MARGIN - next > viewport {
        next = caret + MARGIN - viewport;
    }
    next.clamp(0.0, (text_width - viewport + MARGIN).max(0.0))
}

fn style_padding_horizontal() -> f32 {
    24.0
}

fn resolve_length(length: Length, parent: f32) -> f32 {
    length.resolve(Some(parent)).unwrap_or(280.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paste_normalization_collapses_crlf_to_one_space() {
        assert_eq!(normalize_paste("a\r\nb\r c\td"), "a b  c d");
    }

    #[test]
    fn paste_never_replaces_more_than_one_selected_span() {
        let mut value = "hello world".to_owned();
        let mut editor = TextInputState {
            anchor: 6,
            cursor: 11,
            ..TextInputState::default()
        };
        assert!(paste_text(&mut value, &mut editor, "name\r\nnext"));
        assert_eq!(value, "hello name next");
        assert_eq!(editor.anchor, editor.cursor);
    }

    #[test]
    fn scroll_to_caret_keeps_a_margin() {
        assert_eq!(scroll_to_caret(0.0, 3.0, 100.0, 200.0), 0.0);
        assert!(scroll_to_caret(0.0, 105.0, 100.0, 200.0) >= 9.0);
        assert_eq!(scroll_to_caret(100.0, 0.0, 100.0, 200.0), 0.0);
    }
}
