//! Pluggable syntax highlighting for [`TextEditor`](super::TextEditor).
//!
//! A highlighter turns a source string into a list of [`TextSpan`]s (byte
//! ranges + colours). It is a plain trait so applications can bring tree-sitter,
//! a hand-written lexer, or a server-side token stream without the framework
//! taking a dependency on any of them.
//!
//! [`SyntaxHighlighter`] is a small, dependency-free tokenizer that covers the
//! common syntax families: line/block comments, strings, numbers, keywords,
//! types, attributes, operators and brackets. It is deliberately not a parser;
//! it is enough to make a code editor readable and to demonstrate the API.

use kasumi_core::{Color, TextSpan};

/// Colour palette for a syntax theme.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SyntaxColors {
    pub plain: Color,
    pub keyword: Color,
    pub type_name: Color,
    pub string: Color,
    pub number: Color,
    pub comment: Color,
    pub operator: Color,
    pub punctuation: Color,
    pub attribute: Color,
    pub function: Color,
    pub variable: Color,
}

impl SyntaxColors {
    /// A palette that works on a dark editor surface.
    pub fn dark() -> Self {
        Self {
            plain: Color::rgb8(0xd6, 0xde, 0xe8),
            keyword: Color::rgb8(0xc7, 0x92, 0xea),
            type_name: Color::rgb8(0x82, 0xaa, 0xff),
            string: Color::rgb8(0xc3, 0xe8, 0x8d),
            number: Color::rgb8(0xf7, 0x8c, 0x6c),
            comment: Color::rgb8(0x6a, 0x76, 0x88),
            operator: Color::rgb8(0x89, 0xdd, 0xff),
            punctuation: Color::rgb8(0x9a, 0xa5, 0xb4),
            attribute: Color::rgb8(0xff, 0xcb, 0x6b),
            function: Color::rgb8(0x82, 0xaa, 0xff),
            variable: Color::rgb8(0xd6, 0xde, 0xe8),
        }
    }

    /// A palette that works on a light editor surface.
    pub fn light() -> Self {
        Self {
            plain: Color::rgb8(0x24, 0x29, 0x2f),
            keyword: Color::rgb8(0x8a, 0x3f, 0xbc),
            type_name: Color::rgb8(0x1c, 0x5c, 0xa8),
            string: Color::rgb8(0x2e, 0x7d, 0x32),
            number: Color::rgb8(0xb7, 0x50, 0x00),
            comment: Color::rgb8(0x78, 0x82, 0x8c),
            operator: Color::rgb8(0x00, 0x6b, 0x83),
            punctuation: Color::rgb8(0x50, 0x58, 0x60),
            attribute: Color::rgb8(0x9a, 0x67, 0x00),
            function: Color::rgb8(0x1c, 0x5c, 0xa8),
            variable: Color::rgb8(0x24, 0x29, 0x2f),
        }
    }
}

/// Token categories used by the built-in highlighter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenKind {
    Plain,
    Keyword,
    Type,
    String,
    Number,
    Comment,
    Operator,
    Punctuation,
    Attribute,
    Function,
    Variable,
}

impl TokenKind {
    pub fn color(self, colors: &SyntaxColors) -> Color {
        match self {
            Self::Plain | Self::Variable => colors.plain,
            Self::Keyword => colors.keyword,
            Self::Type => colors.type_name,
            Self::String => colors.string,
            Self::Number => colors.number,
            Self::Comment => colors.comment,
            Self::Operator => colors.operator,
            Self::Punctuation => colors.punctuation,
            Self::Attribute => colors.attribute,
            Self::Function => colors.function,
        }
    }
}

/// A syntax token: a byte range and a category.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Token {
    pub start: usize,
    pub end: usize,
    pub kind: TokenKind,
}

/// Produces colour spans for one snapshot of a document.
///
/// Implementations are called once per frame while the editor is visible, so
/// they should be cheap and deterministic. Return spans sorted by `start`;
/// the editor clamps them to the current text length.
pub trait Highlighter: Send + Sync {
    fn highlight(&self, text: &str) -> Vec<TextSpan>;

    /// Optional language identifier shown in the editor status bar.
    fn language(&self) -> &str {
        "text"
    }

    /// Line-comment prefix used by the Toggle Comment command.
    fn line_comment(&self) -> Option<&str> {
        None
    }

    /// Bracket/quote pairs used for auto-closing.
    fn auto_pairs(&self) -> &[(char, char)] {
        &[]
    }
}

/// A no-op highlighter for plain text.
#[derive(Debug, Default, Clone, Copy)]
pub struct PlainHighlighter;

impl Highlighter for PlainHighlighter {
    fn highlight(&self, _text: &str) -> Vec<TextSpan> {
        Vec::new()
    }

    fn language(&self) -> &str {
        "plain text"
    }
}

/// Which language dialect the built-in lexer should assume.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum Language {
    #[default]
    Rust,
    C,
    Python,
    Json,
    Shell,
}

impl Language {
    fn keywords(self) -> &'static [&'static str] {
        match self {
            Self::Rust => &[
                "as", "async", "await", "break", "const", "continue", "crate", "dyn", "else",
                "enum", "extern", "false", "fn", "for", "if", "impl", "in", "let", "loop", "match",
                "mod", "move", "mut", "pub", "ref", "return", "self", "Self", "static", "struct",
                "super", "trait", "true", "type", "unsafe", "use", "where", "while",
            ],
            Self::C => &[
                "auto", "break", "case", "char", "const", "continue", "default", "do", "double",
                "else", "enum", "extern", "float", "for", "goto", "if", "inline", "int", "long",
                "register", "restrict", "return", "short", "signed", "sizeof", "static", "struct",
                "switch", "typedef", "union", "unsigned", "void", "volatile", "while",
            ],
            Self::Python => &[
                "and", "as", "assert", "async", "await", "break", "class", "continue", "def",
                "del", "elif", "else", "except", "False", "finally", "for", "from", "global", "if",
                "import", "in", "is", "lambda", "None", "nonlocal", "not", "or", "pass", "raise",
                "return", "True", "try", "while", "with", "yield",
            ],
            Self::Json => &["true", "false", "null"],
            Self::Shell => &[
                "case", "do", "done", "elif", "else", "esac", "fi", "for", "function", "if", "in",
                "local", "return", "then", "until", "while",
            ],
        }
    }

    fn types(self) -> &'static [&'static str] {
        match self {
            Self::Rust => &[
                "bool", "char", "f32", "f64", "i8", "i16", "i32", "i64", "i128", "isize", "str",
                "u8", "u16", "u32", "u64", "u128", "usize", "String", "Vec", "Option", "Result",
                "Box", "Arc", "Rc", "Self",
            ],
            Self::C => &[
                "bool", "char", "double", "float", "int", "long", "short", "size_t", "uint8_t",
                "uint32_t", "void",
            ],
            Self::Python => &[
                "bool", "bytes", "dict", "float", "int", "list", "object", "set", "str", "tuple",
            ],
            Self::Json => &[],
            Self::Shell => &[],
        }
    }

    fn line_comment(self) -> Option<&'static str> {
        match self {
            Self::Rust | Self::C => Some("//"),
            Self::Python | Self::Shell => Some("#"),
            Self::Json => None,
        }
    }

    fn block_comments(self) -> Option<(&'static str, &'static str)> {
        match self {
            Self::Rust | Self::C => Some(("/*", "*/")),
            _ => None,
        }
    }
}

/// The built-in lexer. It is intentionally allocation-light: one pass over
/// the bytes, with keywords checked against sorted static slices.
#[derive(Debug, Clone, Copy)]
pub struct SyntaxHighlighter {
    pub language: Language,
    pub colors: SyntaxColors,
}

impl SyntaxHighlighter {
    pub fn new(language: Language, colors: SyntaxColors) -> Self {
        Self { language, colors }
    }

    pub fn rust(colors: SyntaxColors) -> Self {
        Self::new(Language::Rust, colors)
    }

    /// Tokenizes without allocating a `TextSpan` per token; callers that want
    /// to post-process tokens (folding, semantic highlighting) can use this.
    pub fn tokenize(&self, text: &str) -> Vec<Token> {
        let bytes = text.as_bytes();
        let mut tokens = Vec::new();
        let mut index = 0usize;
        let mut previous_word_was_def = false;
        while index < bytes.len() {
            let byte = bytes[index];
            // Whitespace.
            if byte.is_ascii_whitespace() {
                index += 1;
                continue;
            }
            // Line comments.
            if let Some(prefix) = self.language.line_comment()
                && text[index..].starts_with(prefix)
            {
                let end = text[index..]
                    .find('\n')
                    .map(|offset| index + offset)
                    .unwrap_or(text.len());
                tokens.push(Token {
                    start: index,
                    end,
                    kind: TokenKind::Comment,
                });
                index = end;
                continue;
            }
            // Block comments.
            if let Some((open, close)) = self.language.block_comments()
                && text[index..].starts_with(open)
            {
                let rest = &text[index + open.len()..];
                let end = rest
                    .find(close)
                    .map(|offset| index + open.len() + offset + close.len())
                    .unwrap_or(text.len());
                tokens.push(Token {
                    start: index,
                    end,
                    kind: TokenKind::Comment,
                });
                index = end;
                continue;
            }
            // Strings (with escapes), including raw strings r#"…"#.
            if byte == b'"' || byte == b'\'' || byte == b'`' {
                let delimiter = byte;
                let mut end = index + 1;
                while end < bytes.len() {
                    if bytes[end] == b'\\' {
                        end += 2;
                        continue;
                    }
                    if bytes[end] == delimiter {
                        end += 1;
                        break;
                    }
                    end += 1;
                }
                end = end.min(bytes.len());
                tokens.push(Token {
                    start: index,
                    end,
                    kind: TokenKind::String,
                });
                index = end;
                continue;
            }
            // Numbers.
            if byte.is_ascii_digit()
                || (byte == b'.' && bytes.get(index + 1).is_some_and(u8::is_ascii_digit))
            {
                let mut end = index;
                while end < bytes.len()
                    && (bytes[end].is_ascii_alphanumeric()
                        || matches!(bytes[end], b'_' | b'.' | b'x' | b'X' | b'o' | b'b'))
                {
                    end += 1;
                }
                tokens.push(Token {
                    start: index,
                    end,
                    kind: TokenKind::Number,
                });
                index = end;
                continue;
            }
            // Identifiers, keywords, types, attributes and function names.
            if byte.is_ascii_alphabetic() || byte == b'_' {
                let mut end = index;
                while end < bytes.len()
                    && (bytes[end].is_ascii_alphanumeric() || bytes[end] == b'_')
                {
                    end += 1;
                }
                let word = &text[index..end];
                let kind = if self.language.keywords().contains(&word) {
                    TokenKind::Keyword
                } else if self.language.types().contains(&word) {
                    TokenKind::Type
                } else if previous_word_was_def || bytes.get(end) == Some(&b'(') {
                    TokenKind::Function
                } else {
                    TokenKind::Variable
                };
                previous_word_was_def = matches!(word, "fn" | "def" | "function");
                tokens.push(Token {
                    start: index,
                    end,
                    kind,
                });
                index = end;
                continue;
            }
            // Attributes / preprocessor / decorators.
            if matches!(byte, b'#' | b'@') {
                let mut end = index;
                while end < bytes.len()
                    && (bytes[end].is_ascii_alphanumeric()
                        || matches!(bytes[end], b'_' | b'[' | b']' | b'!' | b'#' | b'@'))
                {
                    end += 1;
                }
                tokens.push(Token {
                    start: index,
                    end: end.max(index + 1),
                    kind: TokenKind::Attribute,
                });
                index = end;
                continue;
            }
            // Operators and punctuation.
            let kind = if b"+-*/%=!<>&|^~?:".contains(&byte) {
                TokenKind::Operator
            } else {
                TokenKind::Punctuation
            };
            let ch_len = text[index..]
                .chars()
                .next()
                .map(char::len_utf8)
                .unwrap_or(1);
            tokens.push(Token {
                start: index,
                end: (index + ch_len).min(bytes.len()),
                kind,
            });
            index += ch_len;
        }
        tokens
    }
}

impl Highlighter for SyntaxHighlighter {
    fn highlight(&self, text: &str) -> Vec<TextSpan> {
        self.tokenize(text)
            .into_iter()
            .filter(|token| token.end > token.start)
            .map(|token| TextSpan::new(token.start, token.end, token.kind.color(&self.colors)))
            .collect()
    }

    fn language(&self) -> &str {
        match self.language {
            Language::Rust => "Rust",
            Language::C => "C / C++",
            Language::Python => "Python",
            Language::Json => "JSON",
            Language::Shell => "Shell",
        }
    }

    fn line_comment(&self) -> Option<&str> {
        self.language.line_comment()
    }

    fn auto_pairs(&self) -> &[(char, char)] {
        &[('(', ')'), ('[', ']'), ('{', '}'), ('"', '"'), ('\'', '\'')]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rust_tokens_cover_the_main_categories() {
        let highlighter = SyntaxHighlighter::rust(SyntaxColors::dark());
        let text = "fn main() { let x: u32 = 42; // hi\n println!(\"ok\"); }";
        let tokens = highlighter.tokenize(text);
        assert!(tokens.iter().any(|t| t.kind == TokenKind::Keyword));
        assert!(tokens.iter().any(|t| t.kind == TokenKind::Type));
        assert!(tokens.iter().any(|t| t.kind == TokenKind::Number));
        assert!(tokens.iter().any(|t| t.kind == TokenKind::Comment));
        assert!(tokens.iter().any(|t| t.kind == TokenKind::String));
        assert!(tokens.iter().any(|t| t.kind == TokenKind::Function));
        for token in &tokens {
            assert!(token.end <= text.len() && token.end > token.start);
        }
    }

    #[test]
    fn block_comments_and_strings_do_not_leak() {
        let highlighter = SyntaxHighlighter::rust(SyntaxColors::dark());
        let text = "/* \"not a string\" */ let s = \"a /* b */\";";
        let tokens = highlighter.tokenize(text);
        let comment = tokens
            .iter()
            .find(|t| t.kind == TokenKind::Comment)
            .expect("block comment missing");
        assert_eq!(&text[comment.start..comment.end], "/* \"not a string\" */");
        let string = tokens
            .iter()
            .find(|t| t.kind == TokenKind::String)
            .expect("string missing");
        assert_eq!(&text[string.start..string.end], "\"a /* b */\"");
    }

    #[test]
    fn spans_are_sorted_and_clamped_to_text() {
        let highlighter = SyntaxHighlighter::rust(SyntaxColors::light());
        let spans = highlighter.highlight("let x = 1;");
        assert!(spans.windows(2).all(|pair| pair[0].start <= pair[1].start));
        assert!(spans.iter().all(|span| span.end <= "let x = 1;".len()));
    }

    #[test]
    fn python_uses_hash_comments() {
        let highlighter = SyntaxHighlighter::new(Language::Python, SyntaxColors::dark());
        let text = "# comment\ndef f(x): return x + 1";
        let tokens = highlighter.tokenize(text);
        assert_eq!(tokens.first().unwrap().kind, TokenKind::Comment);
        assert_eq!(highlighter.line_comment(), Some("#"));
    }
}
