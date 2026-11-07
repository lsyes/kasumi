use kasumi_core::{
    Align, Color, FontFamily, FontWeight, Style, TextAlign, TextSpan, TextStyle, TextWrap,
};
use kasumi_tree::NodeKind;

use crate::response::Response;
use crate::ui::{Ui, Widget};

/// A text builder. Unset properties inherit from the active theme.
#[derive(Debug, Clone)]
pub struct Label {
    text: String,
    font_size: Option<f32>,
    color: Option<kasumi_core::Color>,
    weight: Option<FontWeight>,
    family: Option<FontFamily>,
    align: Option<TextAlign>,
    wrap: Option<TextWrap>,
    line_height: Option<f32>,
    width: Option<kasumi_core::Length>,
    italic: bool,
    base: Option<TextStyle>,
    spans: Vec<TextSpan>,
}

impl Label {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            font_size: None,
            color: None,
            weight: None,
            family: None,
            align: None,
            wrap: None,
            line_height: None,
            width: None,
            italic: false,
            base: None,
            spans: Vec::new(),
        }
    }

    /// Starts from a complete type-scale style. Explicit label overrides are
    /// applied on top.
    pub fn style(mut self, style: TextStyle) -> Self {
        self.base = Some(style);
        self
    }

    pub fn font_size(mut self, size: f32) -> Self {
        self.font_size = Some(size);
        self
    }

    /// Alias for [`Label::font_size`].
    pub fn size(self, size: f32) -> Self {
        self.font_size(size)
    }

    pub fn color(mut self, color: kasumi_core::Color) -> Self {
        self.color = Some(color);
        self
    }

    pub fn weight(mut self, weight: FontWeight) -> Self {
        self.weight = Some(weight);
        self
    }

    pub fn bold(self) -> Self {
        self.weight(FontWeight::BOLD)
    }

    pub fn semibold(self) -> Self {
        self.weight(FontWeight::SEMIBOLD)
    }

    pub fn family(mut self, family: impl Into<FontFamily>) -> Self {
        self.family = Some(family.into());
        self
    }

    pub fn align(mut self, align: TextAlign) -> Self {
        self.align = Some(align);
        self
    }

    pub fn wrap(mut self, wrap: TextWrap) -> Self {
        self.wrap = Some(wrap);
        self
    }

    pub fn line_height(mut self, line_height: f32) -> Self {
        self.line_height = Some(line_height);
        self
    }

    /// Gives the text node an explicit width. This is how paragraphs opt into
    /// wrapping: the layout pass receives the same width the paint pass uses.
    pub fn width(mut self, width: impl Into<kasumi_core::Length>) -> Self {
        self.width = Some(width.into());
        self
    }

    pub fn italic(mut self) -> Self {
        self.italic = true;
        self
    }

    /// Colours byte ranges of the label without splitting it into several
    /// nodes. Word wrapping, bidi and shaping still see one paragraph.
    pub fn spans(mut self, spans: impl IntoIterator<Item = TextSpan>) -> Self {
        self.spans = spans.into_iter().collect();
        self
    }

    /// Convenience for a single coloured range.
    pub fn highlight(self, start: usize, end: usize, color: Color) -> Self {
        self.spans([TextSpan::new(start, end, color)])
    }

    pub fn text_style(&self, ui: &Ui) -> TextStyle {
        let mut style = self.base.clone().unwrap_or_else(|| ui.theme().text_style());
        if let Some(size) = self.font_size {
            // The M3 roles bundle a line height with their size. When a caller
            // overrides only the size, scale that line height instead of
            // keeping the original role's 20 px (or 64 px) line box.
            if self.line_height.is_none()
                && let Some(line_height) = style.line_height
            {
                style.line_height = Some(line_height * size / style.size.max(1.0));
            }
            style.size = size;
        }
        if let Some(color) = self.color {
            style.color = color;
        }
        if let Some(weight) = self.weight {
            style.weight = weight;
        }
        if let Some(family) = &self.family {
            style.family = family.clone();
        }
        if let Some(align) = self.align {
            style.align = align;
        }
        if let Some(wrap) = self.wrap {
            style.wrap = wrap;
        }
        if let Some(line_height) = self.line_height {
            style.line_height = Some(line_height);
        }
        style.italic = self.italic;
        style
    }
}

impl Widget for Label {
    type Output = Response;

    fn ui(self, ui: &mut Ui) -> Response {
        let id = ui.next_id("label");
        let text_style = self.text_style(ui);
        let style = Style {
            text: text_style.clone(),
            align_self: Align::Auto,
            width: self.width.unwrap_or(kasumi_core::Length::Auto),
            min_height: kasumi_core::Length::Px(
                text_style.line_height.unwrap_or(text_style.size * 1.32),
            ),
            ..Style::default()
        };
        let node = ui.adopt(id, NodeKind::Text, style);
        if self.spans.is_empty() {
            ui.set_text_on(node, self.text, text_style);
        } else {
            ui.set_text_on_with_spans(node, self.text, text_style, self.spans);
        }
        ui.response_of(id)
    }
}
