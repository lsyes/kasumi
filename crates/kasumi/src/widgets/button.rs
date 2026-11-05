use kasumi_core::{
    Align, Color, Corners, Edges, FlexDirection, Id, Justify, Length, Style, TextAlign, TextWrap,
    child_id,
};
use kasumi_tree::{NodeKind, Sense};

use crate::response::Response;
use crate::ui::{Ui, Widget};

/// Material 3 button emphasis.
///
/// `Primary`, `Secondary` and `Subtle` are retained as aliases for the earlier
/// API and map to filled, outlined and text buttons respectively.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum ButtonVariant {
    Filled,
    Tonal,
    #[default]
    Outlined,
    Text,
    Elevated,
    Primary,
    Secondary,
    Subtle,
}

impl ButtonVariant {
    fn material(self) -> Self {
        match self {
            Self::Primary => Self::Filled,
            Self::Secondary => Self::Outlined,
            Self::Subtle => Self::Text,
            other => other,
        }
    }
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
    align_start: bool,
    enabled: bool,
}

impl Button {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            variant: ButtonVariant::Outlined,
            min_width: None,
            padding: None,
            font_size: None,
            full_width: false,
            menu_item: false,
            align_start: false,
            enabled: true,
        }
    }

    /// High-emphasis filled button.
    pub fn primary(mut self) -> Self {
        self.variant = ButtonVariant::Filled;
        self
    }

    pub fn filled(mut self) -> Self {
        self.variant = ButtonVariant::Filled;
        self
    }

    /// Medium-emphasis tonal button using the secondary container role.
    pub fn tonal(mut self) -> Self {
        self.variant = ButtonVariant::Tonal;
        self
    }

    /// Low-emphasis outlined button.
    pub fn outlined(mut self) -> Self {
        self.variant = ButtonVariant::Outlined;
        self
    }

    /// Lowest-emphasis text button.
    pub fn text(mut self) -> Self {
        self.variant = ButtonVariant::Text;
        self
    }

    pub fn elevated(mut self) -> Self {
        self.variant = ButtonVariant::Elevated;
        self
    }

    /// Alias for [`Button::text`].
    pub fn subtle(mut self) -> Self {
        self.variant = ButtonVariant::Text;
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

    /// Makes the button a full-width menu row with start-aligned text.
    pub fn menu_item(mut self) -> Self {
        self.variant = ButtonVariant::Text;
        self.menu_item = true;
        self.full_width = true;
        self
    }

    /// Aligns the label to the leading edge; used by menu and list rows.
    pub fn align_start(mut self) -> Self {
        self.align_start = true;
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
        let colors = &theme.colors;
        let id = ui.next_id("button");
        let variant = self.variant.material();

        let (background, foreground, border, layer, focus_ring) = if !self.enabled {
            match variant {
                ButtonVariant::Filled | ButtonVariant::Tonal | ButtonVariant::Elevated => (
                    colors.on_surface.fade(theme.state.disabled_container),
                    colors.on_surface.fade(theme.state.disabled_content),
                    Color::TRANSPARENT,
                    Color::TRANSPARENT,
                    colors.on_surface.fade(0.38),
                ),
                _ => (
                    Color::TRANSPARENT,
                    colors.on_surface.fade(theme.state.disabled_content),
                    colors.on_surface.fade(theme.state.disabled_container),
                    Color::TRANSPARENT,
                    colors.on_surface.fade(0.38),
                ),
            }
        } else if self.menu_item {
            (
                Color::TRANSPARENT,
                colors.on_surface,
                Color::TRANSPARENT,
                colors.on_surface,
                colors.primary,
            )
        } else {
            match variant {
                ButtonVariant::Filled => (
                    colors.primary,
                    colors.on_primary,
                    Color::TRANSPARENT,
                    colors.on_primary,
                    colors.on_primary,
                ),
                ButtonVariant::Tonal => (
                    colors.secondary_container,
                    colors.on_secondary_container,
                    Color::TRANSPARENT,
                    colors.on_secondary_container,
                    colors.secondary,
                ),
                ButtonVariant::Outlined => (
                    Color::TRANSPARENT,
                    colors.primary,
                    colors.outline,
                    colors.primary,
                    colors.primary,
                ),
                ButtonVariant::Text => (
                    Color::TRANSPARENT,
                    colors.primary,
                    Color::TRANSPARENT,
                    colors.primary,
                    colors.primary,
                ),
                ButtonVariant::Elevated => (
                    colors.surface_container_low,
                    colors.primary,
                    Color::TRANSPARENT,
                    colors.primary,
                    colors.primary,
                ),
                // Normalized by `material`; kept for exhaustiveness.
                ButtonVariant::Primary | ButtonVariant::Secondary | ButtonVariant::Subtle => {
                    unreachable!("button variant is normalized")
                }
            }
        };

        let (default_pad_x, default_pad_y) = if self.menu_item {
            (12.0, 10.0)
        } else if variant == ButtonVariant::Text {
            (12.0, 10.0)
        } else {
            (24.0, 10.0)
        };
        let (pad_x, pad_y) = self.padding.unwrap_or((default_pad_x, default_pad_y));
        let mut text_style = theme.typography.label_large.clone();
        text_style.color = foreground;
        text_style.wrap = TextWrap::None;
        let start_aligned = self.menu_item || self.align_start;
        text_style.align = if start_aligned {
            TextAlign::Start
        } else {
            TextAlign::Center
        };
        if let Some(size) = self.font_size {
            if let Some(line_height) = text_style.line_height {
                text_style.line_height = Some(line_height * size / text_style.size.max(1.0));
            }
            text_style.size = size;
        }
        let line_height = text_style.line_height.unwrap_or(text_style.size * 1.3);
        let min_height = (line_height + pad_y * 2.0).max(40.0);

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
        let style = Style {
            direction: FlexDirection::Row,
            align_items: Align::Center,
            justify: if start_aligned {
                Justify::Start
            } else {
                Justify::Center
            },
            padding: Edges::xy(pad_x, pad_y),
            background,
            border_color: border,
            border_width: if border.a > 0.0 { 1.0 } else { 0.0 },
            radius: Corners::all(if self.menu_item {
                theme.shape.extra_small
            } else {
                theme.shape.full
            }),
            cursor: if self.enabled {
                kasumi_core::CursorIcon::Pointer
            } else {
                kasumi_core::CursorIcon::Default
            },
            focus_ring,
            state_layer: layer,
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
        ui.cx.tree.set_enabled(node, self.enabled);
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
        let text_style = text_style;
        let child_style = Style {
            text: text_style.clone(),
            align_self: Align::Center,
            min_width: Length::Px(0.0),
            ..Style::default()
        };
        let child = ui.cx.tree.ensure(child_id, NodeKind::Text);
        ui.declare_child(node, child);
        ui.set_style_on(child, child_style);
        ui.set_text_on(child, self.text, text_style);

        ui.response_of(id)
    }
}
