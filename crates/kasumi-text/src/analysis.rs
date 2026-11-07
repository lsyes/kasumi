//! Pure-text analysis for editor front-ends.
//!
//! Shaping tells us where glyphs are; an editor also needs to know where the
//! *boundaries* are. This module works on UTF-8 strings only (no font, no
//! layout) so it can be shared by any widget that edits text: UAX #29
//! grapheme and word boundaries, logical lines, indentation, tab stops and
//! bracket pairs.
//!
//! [`TextAnalysis`] precomputes the boundary tables once for a document. The
//! editor rebuilds it when the value changes; the free functions are the
//! single-shot equivalents used by tests and small callers.

use std::collections::HashMap;

use unicode_segmentation::UnicodeSegmentation;

/// Which kind of bracket a byte offset carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BracketKind {
    Paren,
    Square,
    Curly,
    Angle,
}

/// A matched bracket pair, as byte offsets into the analysed text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BracketMatch {
    pub open: usize,
    pub close: usize,
    pub kind: BracketKind,
}

/// Precomputed boundaries for one immutable snapshot of a string.
///
/// All offsets are UTF-8 byte offsets into the text passed to [`Self::new`].
/// Methods clamp out-of-range inputs instead of panicking, which keeps event
/// handlers safe even when a State write and a queued edit disagree for a
/// frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextAnalysis {
    len: usize,
    graphemes: Vec<usize>,
    words: Vec<usize>,
    line_starts: Vec<usize>,
    brackets: HashMap<usize, BracketMatch>,
}

fn clamp(text: &str, byte: usize) -> usize {
    let mut byte = byte.min(text.len());
    while byte > 0 && !text.is_char_boundary(byte) {
        byte -= 1;
    }
    byte
}

fn bracket_kind(ch: char) -> Option<(BracketKind, bool)> {
    match ch {
        '(' => Some((BracketKind::Paren, true)),
        ')' => Some((BracketKind::Paren, false)),
        '[' => Some((BracketKind::Square, true)),
        ']' => Some((BracketKind::Square, false)),
        '{' => Some((BracketKind::Curly, true)),
        '}' => Some((BracketKind::Curly, false)),
        '<' => Some((BracketKind::Angle, true)),
        '>' => Some((BracketKind::Angle, false)),
        _ => None,
    }
}

impl TextAnalysis {
    pub fn new(text: &str) -> Self {
        let mut graphemes: Vec<usize> = std::iter::once(0)
            .chain(text.grapheme_indices(true).map(|(index, _)| index))
            .chain(std::iter::once(text.len()))
            .collect();
        let mut words: Vec<usize> = std::iter::once(0)
            .chain(text.split_word_bound_indices().map(|(index, _)| index))
            .chain(std::iter::once(text.len()))
            .collect();
        graphemes.dedup();
        words.dedup();
        let line_starts: Vec<usize> = std::iter::once(0)
            .chain(
                text.char_indices()
                    .filter(|(_, ch)| *ch == '\n')
                    .map(|(index, ch)| index + ch.len_utf8()),
            )
            .collect();

        // A single stack pass matches brackets. Strings and line comments are
        // skipped so quotes inside source code do not confuse navigation.
        let mut stack: Vec<(usize, BracketKind)> = Vec::new();
        let mut brackets = HashMap::new();
        let bytes = text.as_bytes();
        let mut index = 0usize;
        let mut quote: Option<u8> = None;
        while index < bytes.len() {
            let byte = bytes[index];
            if let Some(delimiter) = quote {
                if byte == b'\\' {
                    index += 1;
                } else if byte == delimiter {
                    quote = None;
                }
                index += 1;
                continue;
            }
            if byte == b'/' && bytes.get(index + 1) == Some(&b'/') {
                while index < bytes.len() && bytes[index] != b'\n' {
                    index += 1;
                }
                continue;
            }
            if matches!(byte, b'"' | b'\'' | b'`') {
                quote = Some(byte);
                index += 1;
                continue;
            }
            let ch = text[index..].chars().next().unwrap();
            if let Some((kind, opening)) = bracket_kind(ch) {
                if opening {
                    stack.push((index, kind));
                } else if let Some(position) = stack.iter().rposition(|(_, k)| *k == kind) {
                    // Discard unclosed openers above the match; mismatched
                    // nesting is not a pair and must not stay on the stack.
                    let (open, _) = stack.remove(position);
                    stack.truncate(position);
                    let pair = BracketMatch {
                        open,
                        close: index,
                        kind,
                    };
                    brackets.insert(open, pair);
                    brackets.insert(index, pair);
                }
            }
            index += ch.len_utf8();
        }

        Self {
            len: text.len(),
            graphemes,
            words,
            line_starts,
            brackets,
        }
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Number of logical (source) lines.
    pub fn line_count(&self) -> usize {
        self.line_starts.len()
    }

    /// Logical line index containing `byte`.
    pub fn line_of_byte(&self, byte: usize) -> usize {
        let byte = byte.min(self.len);
        match self.line_starts.binary_search(&byte) {
            Ok(index) => index,
            Err(index) => index.saturating_sub(1),
        }
    }

    /// Byte offset of the first character of a logical line.
    pub fn line_start(&self, line: usize) -> usize {
        self.line_starts
            .get(line)
            .copied()
            .unwrap_or(self.line_starts.last().copied().unwrap_or(0))
    }

    /// All logical line start offsets (including 0).
    pub fn line_starts(&self) -> &[usize] {
        &self.line_starts
    }

    /// Byte offset just past the last character of a logical line, excluding
    /// the trailing newline (but including a trailing `\r` in CRLF input).
    pub fn line_end(&self, line: usize) -> usize {
        self.line_starts
            .get(line + 1)
            .copied()
            .map(|next| next.saturating_sub(1))
            .unwrap_or(self.len)
    }

    pub fn line_range(&self, line: usize) -> std::ops::Range<usize> {
        self.line_start(line)..self.line_end(line)
    }

    /// Byte offset of the start of the next line, or the same offset when on
    /// the last line.
    pub fn next_line_start(&self, byte: usize) -> usize {
        let line = self.line_of_byte(byte);
        self.line_start((line + 1).min(self.line_count().saturating_sub(1)))
    }

    /// Byte offset of the start of the previous line, or 0 on the first line.
    pub fn previous_line_start(&self, byte: usize) -> usize {
        let line = self.line_of_byte(byte);
        self.line_start(line.saturating_sub(1))
    }

    /// Leading whitespace of the line containing `byte`, as a byte range.
    pub fn indent_range(&self, text: &str, byte: usize) -> std::ops::Range<usize> {
        let start = self.line_start(self.line_of_byte(byte));
        let end = self.line_end(self.line_of_byte(byte));
        let mut cursor = start;
        for ch in text[start.min(text.len())..end.min(text.len())].chars() {
            if ch == ' ' || ch == '\t' {
                cursor += ch.len_utf8();
            } else {
                break;
            }
        }
        start..cursor
    }

    /// Column width of a line's indentation, expanding tabs to `tab_width`.
    pub fn indent_columns(&self, text: &str, byte: usize, tab_width: usize) -> usize {
        let range = self.indent_range(text, byte);
        text[range.clone()]
            .chars()
            .map(|ch| if ch == '\t' { tab_width } else { 1 })
            .sum()
    }

    /// Grapheme boundary at or before `byte`.
    pub fn prev_grapheme(&self, byte: usize) -> usize {
        let byte = byte.min(self.len);
        match self.graphemes.binary_search(&byte) {
            Ok(index) => self.graphemes[index],
            Err(index) => self.graphemes[index.saturating_sub(1)],
        }
    }

    /// Grapheme boundary strictly after `byte`.
    pub fn next_grapheme(&self, byte: usize) -> usize {
        let byte = byte.min(self.len);
        match self.graphemes.binary_search(&byte) {
            Ok(index) => self.graphemes[(index + 1).min(self.graphemes.len() - 1)],
            Err(index) => self.graphemes[index.min(self.graphemes.len() - 1)],
        }
    }

    /// Full grapheme cluster containing `byte`.
    pub fn grapheme_range_at(&self, byte: usize) -> std::ops::Range<usize> {
        let start = self.prev_grapheme(byte);
        start..self.next_grapheme(start)
    }

    /// Word boundary at or before `byte`.
    pub fn prev_word(&self, byte: usize) -> usize {
        let byte = byte.min(self.len);
        match self.words.binary_search(&byte) {
            Ok(index) => self.words[index],
            Err(index) => self.words[index.saturating_sub(1)],
        }
    }

    /// Word boundary strictly after `byte`.
    pub fn next_word(&self, byte: usize) -> usize {
        let byte = byte.min(self.len);
        match self.words.binary_search(&byte) {
            Ok(index) => self.words[(index + 1).min(self.words.len() - 1)],
            Err(index) => self.words[index.min(self.words.len() - 1)],
        }
    }

    /// Word (UAX #29) containing `byte`, with leading/trailing whitespace
    /// trimmed. Double-click selection uses this.
    pub fn word_range_at(&self, text: &str, byte: usize) -> std::ops::Range<usize> {
        let mut start = self.prev_word(byte);
        let mut end = self.next_word(start);
        let slice = &text[start.min(text.len())..end.min(text.len())];
        let leading: String = slice
            .chars()
            .take_while(|ch| ch.is_whitespace())
            .collect();
        start = (start + leading.len()).min(end);
        let trailing: String = text[start.min(text.len())..end.min(text.len())]
            .chars()
            .rev()
            .take_while(|ch| ch.is_whitespace())
            .collect();
        end = (end - trailing.len()).max(start);
        start..end
    }

    /// Bracket pair containing `byte`, if the byte itself is a bracket.
    pub fn bracket_at(&self, byte: usize) -> Option<BracketMatch> {
        self.brackets.get(&byte).copied()
    }

    /// The matching bracket for `byte`, or `None` when unmatched.
    pub fn matching_bracket(&self, byte: usize) -> Option<usize> {
        let pair = self.bracket_at(byte)?;
        Some(if pair.open == byte { pair.close } else { pair.open })
    }

    /// All matched bracket pairs, unordered. Useful for a "matching bracket"
    /// highlight pass.
    pub fn brackets(&self) -> impl Iterator<Item = BracketMatch> + '_ {
        self.brackets.values().copied()
    }
}

/// Byte offset of the start of the line containing `byte` (one-shot version).
pub fn line_start(text: &str, byte: usize) -> usize {
    let byte = clamp(text, byte);
    text[..byte].rfind('\n').map(|index| index + 1).unwrap_or(0)
}

/// Byte offset just past the last character of the line containing `byte`.
pub fn line_end(text: &str, byte: usize) -> usize {
    let byte = clamp(text, byte);
    text[byte..]
        .find('\n')
        .map(|index| byte + index)
        .unwrap_or(text.len())
}

/// Expands tabs into spaces using fixed `tab_width` columns. Line endings are
/// preserved. Used by editors that want a monospace, tab-aligned display while
/// keeping the authoritative value untouched.
pub fn expand_tabs(text: &str, tab_width: usize) -> String {
    let tab_width = tab_width.max(1);
    let mut out = String::with_capacity(text.len());
    let mut column = 0usize;
    for ch in text.chars() {
        match ch {
            '\t' => {
                let spaces = tab_width - column % tab_width;
                out.extend(std::iter::repeat_n(' ', spaces));
                column += spaces;
            }
            '\n' | '\r' => {
                out.push(ch);
                column = 0;
            }
            _ => {
                out.push(ch);
                column += 1;
            }
        }
    }
    out
}

/// Visual column of a byte offset, expanding tabs but not shaping glyphs.
pub fn column_of_byte(text: &str, byte: usize, tab_width: usize) -> usize {
    let byte = clamp(text, byte);
    let tab_width = tab_width.max(1);
    let mut column = 0usize;
    for (index, ch) in text.char_indices() {
        if index >= byte {
            break;
        }
        if ch == '\t' {
            column += tab_width - column % tab_width;
        } else {
            column += 1;
        }
    }
    column
}

/// Byte offset of a visual column on one line, clamped to that line.
pub fn byte_at_column(text: &str, line_start_byte: usize, column: usize, tab_width: usize) -> usize {
    let tab_width = tab_width.max(1);
    let mut byte = line_start_byte.min(text.len());
    let mut current = 0usize;
    while current < column {
        let Some(ch) = text[byte..].chars().next() else {
            break;
        };
        if ch == '\n' {
            break;
        }
        let width = if ch == '\t' {
            tab_width - current % tab_width
        } else {
            1
        };
        if current + width > column {
            break;
        }
        current += width;
        byte += ch.len_utf8();
    }
    byte
}

/// Previous grapheme boundary (one-shot version).
pub fn prev_grapheme_boundary(text: &str, byte: usize) -> usize {
    TextAnalysis::new(text).prev_grapheme(byte)
}

/// Next grapheme boundary (one-shot version).
pub fn next_grapheme_boundary(text: &str, byte: usize) -> usize {
    TextAnalysis::new(text).next_grapheme(byte)
}

/// Word range containing `byte` (one-shot version).
pub fn word_range_at(text: &str, byte: usize) -> std::ops::Range<usize> {
    TextAnalysis::new(text).word_range_at(text, byte)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_track_crlf_and_final_line() {
        let analysis = TextAnalysis::new("a\r\nb\n");
        assert_eq!(analysis.line_count(), 3);
        assert_eq!(analysis.line_range(0), 0..2); // "a\r"
        assert_eq!(analysis.line_range(1), 3..4); // "b"
        assert_eq!(analysis.line_range(2), 5..5); // trailing empty line
        assert_eq!(analysis.line_of_byte(3), 1);
    }

    #[test]
    fn graphemes_keep_combining_marks_and_emoji_zwj() {
        let text = "a\u{301}e\u{301}👨‍👩‍👧x";
        let analysis = TextAnalysis::new(text);
        assert_eq!(analysis.next_grapheme(0), "a\u{301}".len());
        assert_eq!(analysis.prev_grapheme(1), 0);
        let family = "a\u{301}e\u{301}".len();
        assert_eq!(analysis.grapheme_range_at(family).end - family, "👨‍👩‍👧".len());
    }

    #[test]
    fn word_ranges_trim_whitespace_but_keep_punctuation() {
        let analysis = TextAnalysis::new("let value = foo_bar + 42;");
        let text = "let value = foo_bar + 42;";
        assert_eq!(&text[analysis.word_range_at(text, 6)], "value");
        let range = analysis.word_range_at(text, 15);
        assert_eq!(&text[range.clone()], "foo_bar");
        assert_eq!(range.start, 12);
    }

    #[test]
    fn brackets_nest_and_ignore_strings() {
        let text = "f(\"(not)\", [x], {y})";
        let analysis = TextAnalysis::new(text);
        let close = text.rfind(')').unwrap();
        assert_eq!(analysis.matching_bracket(1), Some(close));
        assert_eq!(analysis.matching_bracket(close), Some(1));
        assert_eq!(analysis.matching_bracket(text.find('[').unwrap()), Some(text.find(']').unwrap()));
        assert_eq!(analysis.matching_bracket(text.find('(').unwrap() + 1), None);
    }

    #[test]
    fn tabs_expand_to_fixed_stops() {
        assert_eq!(expand_tabs("a\tb", 4), "a   b");
        assert_eq!(expand_tabs("\t\tx", 4), "        x");
        assert_eq!(column_of_byte("a\tb", 2, 4), 4);
        assert_eq!(byte_at_column("a\tb", 0, 4, 4), 2);
    }

    #[test]
    fn indent_uses_tab_width() {
        let analysis = TextAnalysis::new("\t\tfn main() {}");
        assert_eq!(analysis.indent_range("\t\tfn main() {}", 0), 0..2);
        assert_eq!(analysis.indent_columns("\t\tfn main() {}", 0, 4), 8);
    }

    // Small helper so the assertion above reads like a slice check.
}

