use kasumi_core::{
    Align, Color, Corners, Edges, FlexDirection, Id, Justify, Length, Style, TextStyle, child_id,
};
use kasumi_tree::{NodeKind, Sense};

use crate::response::Response;
use crate::ui::{Ui, Widget};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum ButtonVariant {
    #[default]
    Secondary,
    Primary,
    Subtle,
}

/// Builder for a text button.
#[derive(Debug, Clone)]
pub struct Button {
    text: String,
    variant: ButtonVariant,
    min_width: Option<f32>,
    padding: Option<(f32, f32)>,
    font_size: Option<f32>,
    full_width: bool,
    menu_item: bool,
    enabled: bool,
}

impl Button {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            variant: ButtonVariant::Secondary,
            min_width: None,
            padding: None,
            font_size: None,
            full_width: false,
            menu_item: false,
            enabled: true,
        }
    }

    pub fn primary(mut self) -> Self {
        self.variant = ButtonVariant::Primary;
        self
    }

    pub fn subtle(mut self) -> Self {
        self.variant = ButtonVariant::Subtle;
        self
    }

    pub fn variant(mut self, variant: ButtonVariant) -> Self {
        self.variant = variant;
        self
    }

    pub fn min_width(mut self, width: f32) -> Self {
        self.min_width = Some(width);
        self
    }

    pub fn padding_xy(mut self, x: f32, y: f32) -> Self {
        self.padding = Some((x, y));
        self
    }

    pub fn font_size(mut self, size: f32) -> Self {
        self.font_size = Some(size);
        self
    }

    pub fn full_width(mut self) -> Self {
        self.full_width = true;
        self
    }

    /// Makes the button a menu row with a persistent hover-gray base.
    pub fn menu_item(mut self) -> Self {
        self.variant = ButtonVariant::Subtle;
        self.menu_item = true;
        self.full_width = true;
        self
    }

    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }
}

impl Widget for Button {
    type Output = Response;

    fn ui(self, ui: &mut Ui) -> Response {
        let theme = ui.theme().clone();
        let id = ui.next_id("button");

        let (background, border, text_color) = match (self.variant, self.enabled) {
            (ButtonVariant::Primary, true) => (theme.accent, theme.accent, theme.on_accent),
            (ButtonVariant::Primary, false) => (
                theme.accent.fade(0.45),
                theme.accent.fade(0.35),
                theme.on_accent.fade(0.6),
            ),
            (ButtonVariant::Secondary, true) => (theme.surface, theme.border, theme.text),
            (ButtonVariant::Secondary, false) => (
                theme.surface.fade(0.5),
                theme.border.fade(0.5),
                theme.text_subtle,
            ),
            (ButtonVariant::Subtle, true) if self.menu_item => {
                (theme.panel, Color::TRANSPARENT, theme.text)
            }
            (ButtonVariant::Subtle, true) => (theme.panel, Color::TRANSPARENT, theme.text_muted),
            (ButtonVariant::Subtle, false) => {
                (theme.panel.fade(0.5), Color::TRANSPARENT, theme.text_subtle)
            }
        };

        let (pad_x, pad_y) = self.padding.unwrap_or((16.0, 9.0));
        let font_size = self.font_size.unwrap_or(theme.font_size);
        let min_height = (font_size * 1.32 + pad_y * 2.0).max(30.0);
        let horizontal_parent = matches!(
            ui.style.direction,
            FlexDirection::Row | FlexDirection::RowReverse
        );
        let align_self = if self.full_width {
            Align::Stretch
        } else if horizontal_parent {
            match ui.style.align_items {
                Align::Start => Align::Start,
                Align::Center => Align::Center,
                Align::End => Align::End,
                _ => Align::Auto,
            }
        } else {
            Align::Start
        };
        let (pad_x, pad_y) = if self.menu_item {
            (11.0, 7.0)
        } else {
            (pad_x, pad_y)
        };
        let style = Style {
            direction: FlexDirection::Row,
            align_items: Align::Center,
            justify: Justify::Center,
            padding: Edges::xy(pad_x, pad_y),
            background,
            border_color: border,
            border_width: if self.variant == ButtonVariant::Secondary {
                1.0
            } else {
                0.0
            },
            radius: Corners::all(theme.radius_medium),
            cursor: if self.enabled {
                kasumi_core::CursorIcon::Pointer
            } else {
                kasumi_core::CursorIcon::Default
            },
            focus_ring: theme.focus_ring,
            align_self,
            min_width: self.min_width.map(Length::Px).unwrap_or(Length::Auto),
            min_height: Length::Px(min_height),
            width: if self.full_width {
                Length::Percent(100.0)
            } else {
                Length::Auto
            },
            ..Style::default()
        };

        let node = ui.adopt(id, NodeKind::Button, style);
        ui.set_sense_on(
            node,
            if self.enabled {
                Sense::click()
            } else {
                Sense::NONE
            },
        );

        let child_id: Id = child_id(id, 0);
        ui.register_node_id(child_id, "button label");
        let text_style = TextStyle {
            size: font_size,
            color: text_color,
            weight: kasumi_core::FontWeight::MEDIUM,
            align: kasumi_core::TextAlign::Center,
            ..TextStyle::default()
        };
        let child_style = Style {
            text: text_style.clone(),
            align_self: Align::Center,
            ..Style::default()
        };
        let child = ui.cx.tree.ensure(child_id, NodeKind::Text);
        ui.declare_child(node, child);
        ui.set_style_on(child, child_style);
        ui.set_text_on(child, self.text, text_style);

        ui.response_of(id)
    }
}
