use kasumi_core::{Align, Corners, Edges, FlexDirection, Length, Style};

use crate::response::Response;
use crate::theme::Theme;
use crate::ui::{Ui, Widget};

/// Visual treatment for a [`Card`].
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum CardVariant {
    /// A raised surface. Kasumi approximates Material elevation with a tonal
    /// container and a hairline border because the paint layer has no shadows.
    #[default]
    Elevated,
    /// A filled container with no outline.
    Filled,
    /// A surface with a visible outline.
    Outlined,
}

/// Configuration for an M3-style card container.
///
/// Cards are ordinary column containers with themed padding, corner radius and
/// background. Use [`Ui::card`] to declare the content that belongs inside one.
#[derive(Debug, Clone)]
pub struct Card {
    pub variant: CardVariant,
    pub padding: Edges,
    pub gap: f32,
    pub width: Length,
    pub height: Length,
    pub max_width: Length,
    pub max_height: Length,
    pub radius: Option<f32>,
    /// Overrides the variant's container color.
    pub background: Option<kasumi_core::Color>,
    /// Overrides the variant's outline.
    pub border: Option<(f32, kasumi_core::Color)>,
    pub clickable: bool,
}

impl Default for Card {
    fn default() -> Self {
        Self {
            variant: CardVariant::Elevated,
            padding: Edges::all(16.0),
            gap: 12.0,
            width: Length::Auto,
            height: Length::Auto,
            max_width: Length::Auto,
            max_height: Length::Auto,
            radius: None,
            background: None,
            border: None,
            clickable: false,
        }
    }
}

impl Card {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn elevated() -> Self {
        Self {
            variant: CardVariant::Elevated,
            ..Self::default()
        }
    }

    pub fn filled() -> Self {
        Self {
            variant: CardVariant::Filled,
            ..Self::default()
        }
    }

    pub fn outlined() -> Self {
        Self {
            variant: CardVariant::Outlined,
            ..Self::default()
        }
    }

    pub fn variant(mut self, variant: CardVariant) -> Self {
        self.variant = variant;
        self
    }

    pub fn padding(mut self, padding: f32) -> Self {
        self.padding = Edges::all(padding.max(0.0));
        self
    }

    pub fn padding_xy(mut self, x: f32, y: f32) -> Self {
        self.padding = Edges::xy(x.max(0.0), y.max(0.0));
        self
    }

    pub fn gap(mut self, gap: f32) -> Self {
        self.gap = gap.max(0.0);
        self
    }

    pub fn width(mut self, width: impl Into<Length>) -> Self {
        self.width = width.into();
        self
    }

    pub fn height(mut self, height: impl Into<Length>) -> Self {
        self.height = height.into();
        self
    }

    pub fn max_width(mut self, width: impl Into<Length>) -> Self {
        self.max_width = width.into();
        self
    }

    pub fn max_height(mut self, height: impl Into<Length>) -> Self {
        self.max_height = height.into();
        self
    }

    pub fn radius(mut self, radius: f32) -> Self {
        self.radius = Some(radius.max(0.0));
        self
    }

    /// Overrides the container color selected by the variant.
    pub fn background(mut self, color: kasumi_core::Color) -> Self {
        self.background = Some(color);
        self
    }

    /// Overrides the outline selected by the variant.
    pub fn border(mut self, width: f32, color: kasumi_core::Color) -> Self {
        self.border = Some((width.max(0.0), color));
        self
    }

    /// Makes the card a keyboard-focusable click target.
    pub fn clickable(mut self) -> Self {
        self.clickable = true;
        self
    }

    pub(crate) fn style(&self, theme: &Theme) -> Style {
        // Material 3 elevates cards with a tonal container and a shadow. The
        // paint layer does not implement shadows yet, so elevated cards use
        // surface-container-low and stay flat; filled cards use the highest
        // container, and outlined cards use the plain surface plus an outline.
        let (mut background, mut border, mut border_width) = match self.variant {
            CardVariant::Elevated => (
                theme.colors.surface_container_low,
                kasumi_core::Color::TRANSPARENT,
                0.0,
            ),
            CardVariant::Filled => (
                theme.colors.surface_container_highest,
                kasumi_core::Color::TRANSPARENT,
                0.0,
            ),
            CardVariant::Outlined => (theme.colors.surface, theme.colors.outline_variant, 1.0),
        };
        if let Some(override_color) = self.background {
            background = override_color;
        }
        if let Some((width, color)) = self.border {
            border = color;
            border_width = width;
        }
        Style {
            direction: FlexDirection::Column,
            align_items: Align::Stretch,
            width: self.width,
            height: self.height,
            max_width: self.max_width,
            max_height: self.max_height,
            padding: self.padding,
            gap: self.gap,
            background,
            border_color: border,
            border_width,
            radius: Corners::all(self.radius.unwrap_or(theme.shape.medium)),
            focus_ring: theme.colors.primary,
            state_layer: if self.clickable {
                theme.colors.primary
            } else {
                kasumi_core::Color::TRANSPARENT
            },
            ..Style::default()
        }
    }
}

impl Widget for Card {
    type Output = Response;

    /// Declares an empty card shell. Prefer [`Ui::card`] to attach children.
    fn ui(self, ui: &mut Ui) -> Response {
        let id = ui.next_id("card");
        let style = self.style(ui.theme());
        let node = ui.adopt(id, kasumi_tree::NodeKind::Container, style);
        if self.clickable {
            ui.set_sense_on(node, kasumi_tree::Sense::click());
        }
        ui.response_of(id)
    }
}
