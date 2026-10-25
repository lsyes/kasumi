//! Text style types. Shaping and rasterization live in `kasumi-paint` and the
//! backend respectively; this module only describes what the user asked for.

use std::fmt;

use crate::color::Color;

/// Identifies a loaded font face. Assigned by `kasumi-paint` when a glyph run
/// is shaped, then handed to the renderer for rasterization.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FontFaceId(pub u64);

/// A font family request. Generic families are resolved by the platform font
/// database; `Named` is an exact family name.
#[derive(Debug, Default, Clone, PartialEq, Eq, Hash)]
pub enum FontFamily {
    #[default]
    SansSerif,
    Serif,
    Monospace,
    Named(String),
}

impl From<&str> for FontFamily {
    fn from(value: &str) -> Self {
        Self::Named(value.to_owned())
    }
}

impl From<String> for FontFamily {
    fn from(value: String) -> Self {
        Self::Named(value)
    }
}

impl fmt::Display for FontFamily {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SansSerif => f.write_str("sans-serif"),
            Self::Serif => f.write_str("serif"),
            Self::Monospace => f.write_str("monospace"),
            Self::Named(name) => f.write_str(name),
        }
    }
}

/// OpenType weight, 100..=900. Normal is 400.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FontWeight(pub u16);

impl FontWeight {
    pub const THIN: Self = Self(100);
    pub const LIGHT: Self = Self(300);
    pub const NORMAL: Self = Self(400);
    pub const MEDIUM: Self = Self(500);
    pub const SEMIBOLD: Self = Self(600);
    pub const BOLD: Self = Self(700);
    pub const BLACK: Self = Self(900);
}

impl Default for FontWeight {
    fn default() -> Self {
        Self::NORMAL
    }
}

/// Horizontal alignment of laid-out text inside its box.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TextAlign {
    #[default]
    Start,
    Center,
    End,
}

/// Wrapping policy.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TextWrap {
    /// Layout on one line and allow horizontal overflow.
    None,
    /// Break at word boundaries when the available width is exceeded.
    #[default]
    Word,
}

/// Everything shaping needs to turn a string into positioned glyphs.
#[derive(Debug, Clone, PartialEq)]
pub struct TextStyle {
    pub family: FontFamily,
    pub weight: FontWeight,
    pub italic: bool,
    pub size: f32,
    /// `None` means a line height derived from the font metrics.
    pub line_height: Option<f32>,
    pub letter_spacing: f32,
    pub color: Color,
    pub align: TextAlign,
    pub wrap: TextWrap,
}

impl Default for TextStyle {
    fn default() -> Self {
        Self {
            family: FontFamily::SansSerif,
            weight: FontWeight::NORMAL,
            italic: false,
            size: 14.0,
            line_height: None,
            letter_spacing: 0.0,
            color: Color::WHITE,
            align: TextAlign::Start,
            wrap: TextWrap::Word,
        }
    }
}

impl TextStyle {
    pub fn size(mut self, size: f32) -> Self {
        self.size = size;
        self
    }

    pub fn color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }

    pub fn weight(mut self, weight: FontWeight) -> Self {
        self.weight = weight;
        self
    }

    pub fn family(mut self, family: impl Into<FontFamily>) -> Self {
        self.family = family.into();
        self
    }

    pub fn line_height(mut self, line_height: f32) -> Self {
        self.line_height = Some(line_height);
        self
    }

    pub fn align(mut self, align: TextAlign) -> Self {
        self.align = align;
        self
    }

    pub fn wrap(mut self, wrap: TextWrap) -> Self {
        self.wrap = wrap;
        self
    }

    pub fn italic(mut self) -> Self {
        self.italic = true;
        self
    }
}

/// Content stored on a text node. Kept intentionally small; shaping output is
/// cached separately by the paint layer.
#[derive(Debug, Clone, PartialEq)]
pub struct TextContent {
    pub text: String,
    pub style: TextStyle,
}

impl TextContent {
    pub fn new(text: impl Into<String>, style: TextStyle) -> Self {
        Self {
            text: text.into(),
            style,
        }
    }
}
