//! Pure editing operations for the multi-line [`TextEditor`](super::TextEditor).
//!
//! Every function works on a `String` plus a [`TextInputState`] and returns
//! whether the value changed. Keeping this separate from the widget means the
//! behaviour is unit-testable without a `Ui`, a font database or a GPU.

use kasumi_paint::TextAnalysis;
use kasumi_tree::{TextEditSnapshot, TextInputState, TextMotion};

use crate::runtime::ClipboardAction;
use crate::ui::Ui;

/// How many undo checkpoints to retain.
const UNDO_LIMIT: usize = 256;

/// Editor-specific configuration resolved by the widget each frame.
#[derive(Debug, Clone)]
pub struct EditorConfig {
    pub tab_width: usize,
    pub insert_spaces: bool,
    pub auto_indent: bool,
    pub line_comment: Option<String>,
    pub family: kasumi_core::FontFamily,
    pub font_size: f32,
    pub line_height: f32,
    pub viewport_height: f32,
    pub gutter_width: f32,
    pub viewport_width: f32,
    pub wrap: bool,
}

impl Default for EditorConfig {
    fn default() -> Self {
        Self {
            tab_width: 4,
            insert_spaces: true,
            auto_indent: true,
            line_comment: None,
            family: kasumi_core::FontFamily::Monospace,
            font_size: 14.0,
            line_height: 20.0,
            viewport_height: 400.0,
            gutter_width: 0.0,
            viewport_width: 600.0,
            wrap: false,
        }
    }
}

/// Result of applying the queued edits.
pub struct EditOutcome {
    pub changed: bool,
    /// Request that the widget keep the caret visible (scroll follow).
    pub follow_caret: bool,
}

fn clamp(text: &str, mut byte: usize) -> usize {
    byte = byte.min(text.len());
    while byte > 0 && !text.is_char_boundary(byte) {
        byte -= 1;
    }
    byte
}

pub fn push_undo(state: &mut TextInputState) {
    // `last_value` holds the pre-edit snapshot when the widget calls us.
    if let Some(text) = state.last_value.take() {
        state.undo.push(TextEditSnapshot {
            text,
            anchor: state.anchor,
            cursor: state.cursor,
        });
        if state.undo.len() > UNDO_LIMIT {
            state.undo.remove(0);
        }
        state.redo.clear();
    }
}

fn snapshot(value: &str, state: &mut TextInputState) {
    push_undo(state);
    state.last_value = Some(value.to_owned());
}

fn selected(value: &str, state: &TextInputState) -> (usize, usize) {
    let a = clamp(value, state.anchor.min(state.cursor));
    let b = clamp(value, state.anchor.max(state.cursor));
    (a, b)
}

fn delete_selection(value: &mut String, state: &mut TextInputState) -> bool {
    let (start, end) = selected(value, state);
    if start == end {
        return false;
    }
    value.replace_range(start..end, "");
    state.anchor = start;
    state.cursor = start;
    true
}

/// Inserts `text`, replacing the selection. Newlines are preserved; control
/// characters other than `\n` and `\t` are dropped.
pub fn insert_text(value: &mut String, state: &mut TextInputState, text: &str) -> bool {
    let cleaned: String = text
        .replace("\r\n", "\n")
        .replace('\r', "\n")
        .chars()
        .filter(|ch| !ch.is_control() || matches!(ch, '\n' | '\t'))
        .collect();
    if cleaned.is_empty() {
        return false;
    }
    snapshot(value, state);
    delete_selection(value, state);
    let at = clamp(value, state.cursor);
    value.insert_str(at, &cleaned);
    state.cursor = at + cleaned.len();
    state.anchor = state.cursor;
    state.preedit.clear();
    state.preedit_cursor = None;
    true
}

/// Inserts a newline with the current line's indentation, and one extra level
/// after an opening bracket.
pub fn insert_newline(
    value: &mut String,
    state: &mut TextInputState,
    config: &EditorConfig,
) -> bool {
    snapshot(value, state);
    delete_selection(value, state);
    let at = clamp(value, state.cursor);
    let analysis = TextAnalysis::new(value);
    let indent_range = analysis.indent_range(value, at);
    let mut indent = if config.auto_indent {
        value[indent_range.clone()].to_owned()
    } else {
        String::new()
    };
    // Only copy indentation when the caret is after the leading whitespace.
    if at > indent_range.end {
        // A line ending with an opening brace/bracket gets one extra level.
        let before = value[..at].trim_end();
        if before.ends_with(['{', '(', '[']) {
            indent.push_str(&indent_unit(config));
        }
    }
    let text = format!("\n{indent}");
    value.insert_str(at, &text);
    state.cursor = at + 1 + indent.len();
    state.anchor = state.cursor;
    true
}

fn indent_unit(config: &EditorConfig) -> String {
    if config.insert_spaces {
        " ".repeat(config.tab_width.max(1))
    } else {
        "\t".to_owned()
    }
}

fn line_bounds(value: &str, byte: usize) -> (usize, usize) {
    let analysis = TextAnalysis::new(value);
    let line = analysis.line_of_byte(byte);
    (analysis.line_start(line), analysis.line_end(line))
}

fn selection_line_bounds(value: &str, state: &TextInputState) -> (usize, usize, usize, usize) {
    let (start, end) = selected(value, state);
    let (line_start, _) = line_bounds(value, start);
    // A selection ending exactly at a line start belongs to the previous line:
    // the last selected byte is the newline itself, and an editor must not
    // indent the line after the selection.
    let mut end_byte = if end > start { end - 1 } else { end };
    if end > start && end_byte > 0 && value[..end_byte].ends_with('\n') && end < value.len() {
        end_byte -= 1;
    }
    let (_, line_end) = line_bounds(value, end_byte);
    (start, end, line_start, line_end)
}

/// Rewrites a multi-line block, adjusting selection endpoints by the actual
/// per-line byte delta rather than a single global offset.
///
/// `transform` receives the line's text and returns its replacement. Old byte
/// offsets are remapped through the rebuilt block: an endpoint keeps its
/// (line, within-line) position, clamped to the new line's length, so a caret
/// at the end of the last line stays at the end after every line changed.
fn rewrite_lines<F>(
    value: &mut String,
    state: &mut TextInputState,
    line_start: usize,
    line_end: usize,
    mut transform: F,
) -> bool
where
    F: FnMut(&str) -> Option<String>,
{
    let block = value[line_start..line_end].to_owned();
    let old_lines: Vec<&str> = block.split('\n').collect();
    let mut new_lines: Vec<String> = Vec::with_capacity(old_lines.len());
    let mut changed = false;
    for line in &old_lines {
        let replacement = transform(line);
        let new_line = replacement.unwrap_or_else(|| (*line).to_owned());
        changed |= new_line != *line;
        new_lines.push(new_line);
    }
    if !changed {
        return false;
    }
    let result = new_lines.join("\n");
    // Old/new line boundaries in absolute byte offsets.
    let old_bounds: Vec<(usize, usize)> = {
        let mut bounds = Vec::with_capacity(old_lines.len());
        let mut cursor = line_start;
        for line in &old_lines {
            bounds.push((cursor, cursor + line.len()));
            cursor += line.len() + 1;
        }
        bounds
    };
    let new_bounds: Vec<(usize, usize)> = {
        let mut bounds = Vec::with_capacity(new_lines.len());
        let mut cursor = line_start;
        for line in &new_lines {
            bounds.push((cursor, cursor + line.len()));
            cursor += line.len() + 1;
        }
        bounds
    };
    let remap = |byte: usize| -> usize {
        if byte <= line_start {
            return line_start;
        }
        for (index, &(old_start, old_end)) in old_bounds.iter().enumerate() {
            // A byte at a line start belongs to that line, not to the
            // previous line's trailing newline, so the caret keeps its
            // (line, column) position after the rewrite.
            if byte >= old_start && byte <= old_end {
                let within = byte - old_start;
                let (new_start, new_end) = new_bounds[index];
                if within >= old_end - old_start {
                    return new_end;
                }
                return new_start + within;
            }
            if byte == old_end + 1 && index + 1 < old_bounds.len() {
                let (new_start, _) = new_bounds[index + 1];
                return new_start;
            }
        }
        // Past the block (or on the newline that ends the last line): clamp.
        new_bounds
            .last()
            .map(|&(_, new_end)| new_end)
            .unwrap_or(line_start)
    };
    snapshot(value, state);
    value.replace_range(line_start..line_end, &result);
    state.anchor = clamp(value, remap(state.anchor));
    state.cursor = clamp(value, remap(state.cursor));
    true
}

/// Indents or outdents every line touched by the selection.
pub fn indent(
    value: &mut String,
    state: &mut TextInputState,
    config: &EditorConfig,
    outdent: bool,
) -> bool {
    let (_, _, line_start, line_end) = selection_line_bounds(value, state);
    let unit = indent_unit(config);
    rewrite_lines(value, state, line_start, line_end, |line| {
        if outdent {
            let trimmed = if let Some(rest) = line.strip_prefix(&unit) {
                rest
            } else if let Some(rest) = line.strip_prefix('\t') {
                rest
            } else {
                let spaces = line.chars().take_while(|ch| *ch == ' ').count();
                &line[spaces.min(line.len())..]
            };
            Some(trimmed.to_owned())
        } else {
            Some(format!("{unit}{line}"))
        }
    })
}

/// Inserts an opening bracket/quote and its closer, leaving the caret between.
/// When a selection exists, wraps it instead.
pub fn insert_pair(value: &mut String, state: &mut TextInputState, opening: char) -> bool {
    let closing = match opening {
        '(' => ')',
        '[' => ']',
        '{' => '}',
        '"' => '"',
        '\'' => '\'',
        '`' => '`',
        _ => return false,
    };
    snapshot(value, state);
    let (start, end) = selected(value, state);
    let selected_text = value[start..end].to_owned();
    let text = format!("{opening}{selected_text}{closing}");
    value.replace_range(start..end, &text);
    state.cursor = start + 1 + selected_text.len();
    state.anchor = if selected_text.is_empty() {
        state.cursor
    } else {
        start + 1
    };
    true
}

fn backspace(
    value: &mut String,
    state: &mut TextInputState,
    by_word: bool,
    analysis: &TextAnalysis,
) -> bool {
    if delete_selection(value, state) {
        return true;
    }
    let at = clamp(value, state.cursor);
    if at == 0 {
        return false;
    }
    let previous = if by_word {
        analysis.prev_word(at)
    } else {
        analysis.prev_grapheme(at)
    };
    if previous == at {
        return false;
    }
    snapshot(value, state);
    value.replace_range(previous..at, "");
    state.anchor = previous;
    state.cursor = previous;
    true
}

fn delete_forward(
    value: &mut String,
    state: &mut TextInputState,
    by_word: bool,
    analysis: &TextAnalysis,
) -> bool {
    if delete_selection(value, state) {
        return true;
    }
    let at = clamp(value, state.cursor);
    if at == value.len() {
        return false;
    }
    let next = if by_word {
        analysis.next_word(at)
    } else {
        analysis.next_grapheme(at)
    };
    if next == at {
        return false;
    }
    snapshot(value, state);
    value.replace_range(at..next, "");
    true
}

/// Deletes the current logical line (including its newline).
pub fn delete_line(value: &mut String, state: &mut TextInputState) -> bool {
    if value.is_empty() {
        return false;
    }
    let analysis = TextAnalysis::new(value);
    let line = analysis.line_of_byte(state.cursor);
    let start = analysis.line_start(line);
    let end = analysis
        .line_starts()
        .get(line + 1)
        .copied()
        .unwrap_or(value.len());
    snapshot(value, state);
    value.replace_range(start..end, "");
    state.anchor = start.min(value.len());
    state.cursor = state.anchor;
    true
}

/// Duplicates the current line, or the selected lines, below themselves.
pub fn duplicate_line(value: &mut String, state: &mut TextInputState) -> bool {
    let (start, end, line_start, line_end) = selection_line_bounds(value, state);
    let block = value[line_start..line_end].to_owned();
    let insertion = format!("\n{block}");
    snapshot(value, state);
    value.insert_str(line_end, &insertion);
    let offset = insertion.len();
    state.anchor = start + offset;
    state.cursor = end + offset;
    true
}

/// Moves the selected lines one line up or down, preserving the selection.
pub fn move_lines(value: &mut String, state: &mut TextInputState, up: bool) -> bool {
    let (start, end, line_start, _line_end) = selection_line_bounds(value, state);
    let analysis = TextAnalysis::new(value);
    let first_line = analysis.line_of_byte(line_start);
    let last_line = analysis.line_of_byte(if end > start { end - 1 } else { end });
    let line_count = analysis.line_count();
    if (up && first_line == 0) || (!up && last_line + 1 >= line_count) {
        return false;
    }
    snapshot(value, state);
    let trailing_newline = value.ends_with('\n');
    let mut lines: Vec<String> = value.split('\n').map(str::to_owned).collect();
    if trailing_newline {
        lines.pop();
    }
    // Remove the selected block, then reinsert it one line earlier/later.
    let block: Vec<String> = lines.drain(first_line..=last_line).collect();
    let new_first = if up { first_line - 1 } else { first_line + 1 };
    lines.splice(new_first..new_first, block.iter().cloned());
    // The selection shifts by exactly the length of one swapped neighbour.
    let neighbour_len = if up {
        lines[last_line].len() + 1
    } else {
        lines[first_line].len() + 1
    };
    let delta = if up {
        -(neighbour_len as isize)
    } else {
        neighbour_len as isize
    };
    *value = lines.join("\n");
    if trailing_newline {
        value.push('\n');
    }
    state.anchor = clamp(value, ((start as isize) + delta).max(0) as usize);
    state.cursor = clamp(value, ((end as isize) + delta).max(0) as usize);
    true
}

/// Toggles `//` (or the language's line comment) on all selected lines.
pub fn toggle_comment(
    value: &mut String,
    state: &mut TextInputState,
    config: &EditorConfig,
) -> bool {
    let Some(prefix) = config.line_comment.clone() else {
        return false;
    };
    let (_, _, line_start, line_end) = selection_line_bounds(value, state);
    let block = &value[line_start..line_end];
    let all_commented = block
        .split('\n')
        .filter(|line| !line.trim().is_empty())
        .all(|line| line.trim_start().starts_with(&prefix));
    rewrite_lines(value, state, line_start, line_end, |line| {
        if line.trim().is_empty() {
            return Some(line.to_owned());
        }
        let indent_len = line.len() - line.trim_start().len();
        let (indent, rest) = line.split_at(indent_len);
        if all_commented {
            // Remove the prefix and the single space that follows it.
            let rest = rest.strip_prefix(&prefix).unwrap_or(rest);
            let rest = rest.strip_prefix(' ').unwrap_or(rest);
            Some(format!("{indent}{rest}"))
        } else {
            Some(format!("{indent}{prefix} {rest}"))
        }
    })
}

fn undo(value: &mut String, state: &mut TextInputState) -> bool {
    let Some(previous) = state.undo.pop() else {
        return false;
    };
    let current = std::mem::take(value);
    state.redo.push(TextEditSnapshot {
        text: current,
        anchor: state.anchor,
        cursor: state.cursor,
    });
    *value = previous.text;
    state.anchor = clamp(value, previous.anchor);
    state.cursor = clamp(value, previous.cursor);
    state.last_value = Some(value.clone());
    state.preedit.clear();
    true
}

fn redo(value: &mut String, state: &mut TextInputState) -> bool {
    let Some(next) = state.redo.pop() else {
        return false;
    };
    let current = std::mem::take(value);
    state.undo.push(TextEditSnapshot {
        text: current,
        anchor: state.anchor,
        cursor: state.cursor,
    });
    *value = next.text;
    state.anchor = clamp(value, next.anchor);
    state.cursor = clamp(value, next.cursor);
    state.last_value = Some(value.clone());
    state.preedit.clear();
    true
}

/// Applies caret motion. `layout` provides visual-line geometry; `texts` is
/// used for the column-preserving vertical motion.
pub fn move_caret(
    ui: &Ui,
    value: &str,
    state: &mut TextInputState,
    motion: TextMotion,
    extend: bool,
    by_word: bool,
    config: &EditorConfig,
) {
    let analysis = TextAnalysis::new(value);
    let layout = hit_layout(ui, value, config);
    let current = clamp(value, state.cursor);
    let mut next = current;
    match motion {
        TextMotion::Left => {
            next = if by_word {
                analysis.prev_word(current)
            } else {
                analysis.prev_grapheme(current)
            };
            state.goal_column = 0.0;
        }
        TextMotion::Right => {
            next = if by_word {
                analysis.next_word(current)
            } else {
                analysis.next_grapheme(current)
            };
            state.goal_column = 0.0;
        }
        TextMotion::Home => {
            let (start, _) = line_bounds(value, current);
            let indent = analysis.indent_range(value, current).end;
            next = if current > indent { indent } else { start };
            state.goal_column = 0.0;
        }
        TextMotion::End => {
            let (_, end) = line_bounds(value, current);
            next = end;
            state.goal_column = 0.0;
        }
        TextMotion::DocumentStart => {
            next = 0;
            state.goal_column = 0.0;
        }
        TextMotion::DocumentEnd => {
            next = value.len();
            state.goal_column = 0.0;
        }
        TextMotion::Up | TextMotion::Down | TextMotion::PageUp | TextMotion::PageDown => {
            let caret = layout.caret_at(current);
            if state.goal_column <= 0.0 {
                state.goal_column = caret.x;
            }
            let line_delta: isize = match motion {
                TextMotion::Up => -1,
                TextMotion::Down => 1,
                TextMotion::PageUp => -((config.viewport_height / config.line_height) as isize),
                _ => (config.viewport_height / config.line_height) as isize,
            };
            let target_line = (caret.line as isize + line_delta)
                .clamp(0, layout.lines.len().saturating_sub(1) as isize)
                as usize;
            let line = &layout.lines[target_line];
            next = layout.byte_at_point(value, state.goal_column, line.top + line.height * 0.5);
            // byte_at_point caps at the line end; that is the desired behaviour.
        }
        TextMotion::MatchingBracket => {
            if let Some(matching) = find_nearby_bracket(&analysis, value, current) {
                next = matching;
            }
            state.goal_column = 0.0;
        }
    }
    state.cursor = clamp(value, next);
    if !extend {
        state.anchor = state.cursor;
    }
}

fn find_nearby_bracket(analysis: &TextAnalysis, text: &str, byte: usize) -> Option<usize> {
    if byte < text.len()
        && let Some(matching) = analysis.matching_bracket(byte)
    {
        return Some(matching);
    }
    if byte > 0 {
        let mut previous = byte - 1;
        while previous > 0 && !text.is_char_boundary(previous) {
            previous -= 1;
        }
        return analysis.matching_bracket(previous);
    }
    None
}

fn editor_style(config: &EditorConfig) -> kasumi_core::TextStyle {
    kasumi_core::TextStyle {
        family: config.family.clone(),
        size: config.font_size,
        line_height: Some(config.line_height),
        wrap: if config.wrap {
            kasumi_core::TextWrap::Word
        } else {
            kasumi_core::TextWrap::None
        },
        ..kasumi_core::TextStyle::default()
    }
}

/// Applies every queued edit. `paint_string` is the string the editor will
/// render (IME pre-edit is spliced in by the widget afterwards).
pub fn apply_edits(
    ui: &mut Ui,
    value: &mut String,
    state: &mut TextInputState,
    config: &EditorConfig,
    enabled: bool,
) -> EditOutcome {
    let edits = std::mem::take(&mut state.pending);
    if !enabled || edits.is_empty() {
        return EditOutcome {
            changed: false,
            follow_caret: false,
        };
    }
    // The widget stores the pre-edit value here; the first mutation pushes it
    // onto the undo stack. Keep it in sync when no mutation happens.
    state.last_value = Some(value.clone());
    let mut changed = false;
    let mut follow_caret = false;
    for edit in edits {
        use kasumi_tree::TextEdit;
        match edit {
            TextEdit::Insert(text) => {
                changed |= insert_text(value, state, &text);
                follow_caret = true;
            }
            TextEdit::NewLine => {
                changed |= insert_newline(value, state, config);
                follow_caret = true;
            }
            TextEdit::InsertPair { opening } => {
                changed |= insert_pair(value, state, opening);
                follow_caret = true;
            }
            TextEdit::Backspace => {
                let analysis = TextAnalysis::new(value);
                changed |= backspace(value, state, false, &analysis);
                follow_caret = true;
            }
            TextEdit::BackspaceWord => {
                let analysis = TextAnalysis::new(value);
                changed |= backspace(value, state, true, &analysis);
                follow_caret = true;
            }
            TextEdit::Delete => {
                let analysis = TextAnalysis::new(value);
                changed |= delete_forward(value, state, false, &analysis);
                follow_caret = true;
            }
            TextEdit::DeleteWord => {
                let analysis = TextAnalysis::new(value);
                changed |= delete_forward(value, state, true, &analysis);
                follow_caret = true;
            }
            TextEdit::DeleteLine => {
                changed |= delete_line(value, state);
                follow_caret = true;
            }
            TextEdit::DuplicateLine => {
                changed |= duplicate_line(value, state);
                follow_caret = true;
            }
            TextEdit::MoveLines { up } => {
                changed |= move_lines(value, state, up);
                follow_caret = true;
            }
            TextEdit::ToggleComment => {
                changed |= toggle_comment(value, state, config);
                follow_caret = true;
            }
            TextEdit::Indent { outdent } => {
                changed |= indent(value, state, config, outdent);
                follow_caret = true;
            }
            TextEdit::MoveCaret {
                motion,
                extend,
                by_word,
            } => {
                move_caret(ui, value, state, motion, extend, by_word, config);
                follow_caret = true;
            }
            TextEdit::SelectAll => {
                state.anchor = 0;
                state.cursor = value.len();
                follow_caret = true;
            }
            TextEdit::SetSelection { anchor, cursor } => {
                state.anchor = clamp(value, anchor);
                state.cursor = clamp(value, cursor);
                state.goal_column = 0.0;
                state.preedit.clear();
                state.preedit_cursor = None;
                follow_caret = true;
            }
            TextEdit::SelectLine { y } => {
                select_line(ui, value, state, y, config);
                follow_caret = true;
            }
            TextEdit::SelectWord { x, y } => {
                select_word(ui, value, state, x, y, config);
                follow_caret = true;
            }
            TextEdit::PointerDown { x, y, extend } => {
                select_point(ui, value, state, x, y, extend, config);
                follow_caret = true;
            }
            TextEdit::PointerDrag { x, y } => {
                select_point(ui, value, state, x, y, true, config);
                follow_caret = true;
            }
            TextEdit::PointerUp => {}
            TextEdit::Paste(text) => {
                changed |= insert_text(value, state, &text);
                follow_caret = true;
            }
            TextEdit::Copy => {
                let (start, end) = selected(value, state);
                if end > start {
                    ui.cx.clipboard_action =
                        Some(ClipboardAction::SetText(value[start..end].to_owned()));
                }
            }
            TextEdit::Cut => {
                let (start, end) = selected(value, state);
                if end > start {
                    ui.cx.clipboard_action =
                        Some(ClipboardAction::SetText(value[start..end].to_owned()));
                    push_undo(state);
                    delete_selection(value, state);
                    changed = true;
                    follow_caret = true;
                }
            }
            TextEdit::Undo => {
                changed |= undo(value, state);
                follow_caret = true;
            }
            TextEdit::Redo => {
                changed |= redo(value, state);
                follow_caret = true;
            }
            TextEdit::SetPreedit { text, cursor } => {
                state.preedit = text;
                state.preedit_cursor = cursor;
            }
        }
    }
    state.anchor = clamp(value, state.anchor);
    state.cursor = clamp(value, state.cursor);
    EditOutcome {
        changed,
        follow_caret,
    }
}

fn hit_layout(
    ui: &Ui,
    value: &str,
    config: &EditorConfig,
) -> std::sync::Arc<kasumi_paint::TextLayout> {
    ui.texts().layout(
        value,
        &editor_style(config),
        Some((config.viewport_width - config.gutter_width).max(1.0)),
    )
}

fn select_point(
    ui: &Ui,
    value: &str,
    state: &mut TextInputState,
    x: f32,
    y: f32,
    extend: bool,
    config: &EditorConfig,
) {
    let layout = hit_layout(ui, value, config);
    let byte = layout.byte_at_point(value, x + state.scroll_x, y + state.scroll_y);
    state.cursor = clamp(value, byte);
    if !extend {
        state.anchor = state.cursor;
        state.goal_column = 0.0;
    }
}

fn select_word(
    ui: &Ui,
    value: &str,
    state: &mut TextInputState,
    x: f32,
    y: f32,
    config: &EditorConfig,
) {
    let layout = hit_layout(ui, value, config);
    let byte = clamp(
        value,
        layout.byte_at_point(value, x + state.scroll_x, y + state.scroll_y),
    );
    let analysis = TextAnalysis::new(value);
    let range = analysis.word_range_at(value, byte);
    state.anchor = range.start;
    state.cursor = range.end;
    state.goal_column = 0.0;
}

fn select_line(ui: &Ui, value: &str, state: &mut TextInputState, y: f32, config: &EditorConfig) {
    let layout = hit_layout(ui, value, config);
    let byte = clamp(
        value,
        layout.byte_at_point(value, state.scroll_x, y + state.scroll_y),
    );
    let analysis = TextAnalysis::new(value);
    let line = analysis.line_of_byte(byte);
    state.anchor = analysis.line_start(line);
    state.cursor = analysis.line_end(line);
    state.goal_column = 0.0;
}

/// Keeps the caret inside the viewport by updating the retained scroll state.
pub fn follow_caret(
    ui: &Ui,
    value: &str,
    state: &mut TextInputState,
    config: &EditorConfig,
    caret_byte: usize,
) {
    let layout = hit_layout(ui, value, config);
    let caret = layout.caret_at(caret_byte);
    let viewport_w = (config.viewport_width - config.gutter_width).max(1.0);
    let viewport_h = config.viewport_height.max(1.0);
    const MARGIN: f32 = 4.0;
    // Horizontal.
    if caret.x - MARGIN < state.scroll_x {
        state.scroll_x = (caret.x - MARGIN).max(0.0);
    } else if caret.x + MARGIN - state.scroll_x > viewport_w {
        state.scroll_x = caret.x + MARGIN - viewport_w;
    }
    let max_x = (layout.size.width - viewport_w + MARGIN).max(0.0);
    state.scroll_x = state.scroll_x.clamp(0.0, max_x);
    // Vertical.
    if caret.y - MARGIN < state.scroll_y {
        state.scroll_y = (caret.y - MARGIN).max(0.0);
    } else if caret.y + caret.height + MARGIN - state.scroll_y > viewport_h {
        state.scroll_y = caret.y + caret.height + MARGIN - viewport_h;
    }
    let max_y = (layout.size.height - viewport_h + MARGIN).max(0.0);
    state.scroll_y = state.scroll_y.clamp(0.0, max_y);
    // Publish the content extent so wheel scrolling and scrollbars work.
    let _ = ui;
}

/// Byte offsets of the bracket pair next to the caret, for highlighting.
/// The bracket immediately before the caret counts as "next to" it, which is
/// what an editor wants after typing or after moving rightwards.
pub fn bracket_highlights(value: &str, caret: usize) -> Vec<usize> {
    let analysis = TextAnalysis::new(value);
    let before = if caret == 0 {
        0
    } else {
        let mut byte = caret - 1;
        while byte > 0 && !value.is_char_boundary(byte) {
            byte -= 1;
        }
        byte
    };
    for candidate in [caret, before] {
        if let Some(pair) = analysis.bracket_at(candidate) {
            return vec![pair.open, pair.close];
        }
    }
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(crate) fn state(cursor: usize) -> TextInputState {
        TextInputState {
            anchor: cursor,
            cursor,
            last_value: None,
            ..TextInputState::default()
        }
    }

    pub(crate) fn config() -> EditorConfig {
        EditorConfig {
            line_comment: Some("//".to_owned()),
            ..EditorConfig::default()
        }
    }

    #[test]
    fn insert_keeps_newlines_and_tabs() {
        let mut value = "a\nb".to_owned();
        let mut s = state(1);
        assert!(insert_text(&mut value, &mut s, "x\ny\tz"));
        assert_eq!(value, "ax\ny\tz\nb");
        assert_eq!(s.cursor, 6);
        assert_eq!(s.anchor, s.cursor);
    }

    #[test]
    fn insert_replaces_selection_and_records_undo() {
        let mut value = "hello world".to_owned();
        let mut s = state(0);
        s.cursor = 5;
        s.last_value = Some(value.clone());
        assert!(insert_text(&mut value, &mut s, ","));
        assert_eq!(value, ", world");
        assert_eq!(s.undo.len(), 1);
    }

    #[test]
    fn newline_copies_indentation_and_adds_a_level_after_brace() {
        let mut value = "fn main() {\n    let x = 1;".to_owned();
        let mut s = state(value.len());
        let cfg = config();
        assert!(insert_newline(&mut value, &mut s, &cfg));
        assert!(value.ends_with("\n    "));
        let mut value = "if x {".to_owned();
        let mut s = state(value.len());
        assert!(insert_newline(&mut value, &mut s, &cfg));
        assert!(value.ends_with("\n    "), "{value:?}");
    }

    #[test]
    fn indent_and_outdent_touch_all_selected_lines() {
        let mut value = "a\nb\nc".to_owned();
        let mut s = state(0);
        s.cursor = value.len();
        let cfg = config();
        assert!(indent(&mut value, &mut s, &cfg, false));
        assert_eq!(value, "    a\n    b\n    c");
        assert!(indent(&mut value, &mut s, &cfg, true));
        assert_eq!(value, "a\nb\nc");
    }

    #[test]
    fn insert_pair_wraps_a_selection() {
        let mut value = "call(arg)".to_owned();
        let mut s = state(5);
        s.cursor = 8;
        assert!(insert_pair(&mut value, &mut s, '('));
        assert_eq!(value, "call((arg))");
        assert_eq!(s.cursor, 9);
    }

    #[test]
    fn comment_toggle_is_reversible() {
        let mut value = "let a = 1;\n    let b = 2;".to_owned();
        let mut s = state(0);
        s.cursor = value.len();
        let cfg = config();
        assert!(toggle_comment(&mut value, &mut s, &cfg));
        assert_eq!(value, "// let a = 1;\n    // let b = 2;");
        assert!(toggle_comment(&mut value, &mut s, &cfg));
        assert_eq!(value, "let a = 1;\n    let b = 2;");
    }

    #[test]
    fn duplicate_and_delete_line() {
        let mut value = "one\ntwo\nthree".to_owned();
        let mut s = state(4);
        assert!(duplicate_line(&mut value, &mut s));
        assert_eq!(value, "one\ntwo\ntwo\nthree");
        assert!(delete_line(&mut value, &mut s));
        assert_eq!(value, "one\ntwo\nthree");
    }

    #[test]
    fn undo_restores_the_previous_text() {
        let mut value = "abc".to_owned();
        let mut s = state(3);
        s.last_value = Some(value.clone());
        insert_text(&mut value, &mut s, "d");
        assert!(undo(&mut value, &mut s));
        assert_eq!(value, "abc");
        assert!(redo(&mut value, &mut s));
        assert_eq!(value, "abcd");
    }

    #[test]
    fn bracket_highlights_find_the_matching_pair() {
        let text = "foo(bar)";
        assert_eq!(bracket_highlights(text, 3), vec![3, 7]);
        assert_eq!(bracket_highlights(text, 4), vec![3, 7]);
    }

    #[test]
    fn move_lines_swaps_with_the_neighbour() {
        let mut value = "a\nb\nc".to_owned();
        let mut s = state(2);
        assert!(move_lines(&mut value, &mut s, true));
        assert_eq!(value, "b\na\nc");
        assert!(move_lines(&mut value, &mut s, false));
        assert_eq!(value, "a\nb\nc");
    }

    #[test]
    fn backspace_and_delete_remove_one_grapheme() {
        let mut value = "abc".to_owned();
        let mut s = state(2);
        s.last_value = Some(value.clone());
        let analysis = TextAnalysis::new(&value);
        assert!(backspace(&mut value, &mut s, false, &analysis));
        assert_eq!(value, "ac");
        assert_eq!(s.cursor, 1);
        s.last_value = Some(value.clone());
        let analysis = TextAnalysis::new(&value);
        assert!(delete_forward(&mut value, &mut s, false, &analysis));
        assert_eq!(value, "a");
    }

    #[test]
    fn word_movement_removes_a_whole_word() {
        let mut value = "one two three".to_owned();
        let mut s = state(8); // start of "three", after the space
        s.last_value = Some(value.clone());
        let analysis = TextAnalysis::new(&value);
        assert!(backspace(&mut value, &mut s, true, &analysis));
        assert_eq!(value, "one twothree");
        assert_eq!(s.cursor, 7);
        s.last_value = Some(value.clone());
        let analysis = TextAnalysis::new(&value);
        assert!(backspace(&mut value, &mut s, true, &analysis));
        assert_eq!(value, "one three");
    }

    #[test]
    fn boundaries_are_strict_so_keys_are_never_no_ops() {
        let value = "abc";
        let analysis = TextAnalysis::new(value);
        // A degenerate "previous boundary == current byte" would make every
        // Left/Backspace a no-op.
        assert_eq!(analysis.prev_grapheme(2), 1);
        assert_eq!(analysis.next_grapheme(2), 3);
        // "abc" only: word boundaries are 0 and 3.
        assert_eq!(analysis.prev_word(3), 0);
        assert_eq!(analysis.next_word(0), 3);
    }
}
