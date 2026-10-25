use kasumi_core::{Align, FontFamily, FontWeight, Style, TextAlign, TextStyle, TextWrap};
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
        }
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

    pub fn text_style(&self, ui: &Ui) -> TextStyle {
        let mut style = ui.theme().text_style();
        if let Some(size) = self.font_size {
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
            min_height: kasumi_core::Length::Px(text_style.size * 1.32),
            ..Style::default()
        };
        let node = ui.adopt(id, NodeKind::Text, style);
        ui.set_text_on(node, self.text, text_style);
        ui.response_of(id)
    }
}
