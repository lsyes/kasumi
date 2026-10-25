use kasumi_core::{Color, Corners, FlexDirection, Length, Style};
use kasumi_state::State;
use kasumi_tree::NodeKind;

use crate::response::Response;
use crate::ui::{Ui, Widget};

/// A horizontal or vertical rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DividerOrientation {
    Horizontal,
    Vertical,
}

#[derive(Debug, Clone, Copy)]
pub struct Divider {
    orientation: DividerOrientation,
    thickness: f32,
    color: Option<Color>,
}

impl Default for Divider {
    fn default() -> Self {
        Self {
            orientation: DividerOrientation::Horizontal,
            thickness: 1.0,
            color: None,
        }
    }
}

impl Divider {
    pub fn horizontal() -> Self {
        Self::default()
    }

    pub fn vertical() -> Self {
        Self {
            orientation: DividerOrientation::Vertical,
            ..Self::default()
        }
    }

    pub fn thickness(mut self, thickness: f32) -> Self {
        self.thickness = thickness.max(0.5);
        self
    }

    pub fn color(mut self, color: Color) -> Self {
        self.color = Some(color);
        self
    }
}

impl Widget for Divider {
    type Output = Response;

    fn ui(self, ui: &mut Ui) -> Response {
        let id = ui.next_id("divider");
        let color = self.color.unwrap_or(ui.theme().border);
        let style = match self.orientation {
            DividerOrientation::Horizontal => Style {
                width: Length::Percent(100.0),
                height: Length::Px(self.thickness),
                min_height: Length::Px(self.thickness),
                background: color,
                ..Style::default()
            },
            DividerOrientation::Vertical => Style {
                width: Length::Px(self.thickness),
                min_width: Length::Px(self.thickness),
                height: Length::Percent(100.0),
                background: color,
                ..Style::default()
            },
        };
        ui.adopt(id, NodeKind::Container, style);
        ui.response_of(id)
    }
}

/// A determinate progress bar for a value in the inclusive `0.0..=1.0` range.
#[derive(Clone)]
pub struct ProgressBar {
    value: State<f32>,
    width: Length,
    height: f32,
}

impl ProgressBar {
    pub fn new(value: &State<f32>) -> Self {
        Self {
            value: value.clone(),
            width: Length::Percent(100.0),
            height: 8.0,
        }
    }

    pub fn width(mut self, width: impl Into<Length>) -> Self {
        self.width = width.into();
        self
    }

    pub fn height(mut self, height: f32) -> Self {
        self.height = height.max(2.0);
        self
    }
}

impl Widget for ProgressBar {
    type Output = Response;

    fn ui(self, ui: &mut Ui) -> Response {
        let theme = ui.theme().clone();
        let id = ui.next_id("progress bar");
        let node = ui.adopt(
            id,
            NodeKind::ProgressBar,
            Style {
                direction: FlexDirection::Row,
                width: self.width,
                height: Length::Px(self.height),
                radius: Corners::all(self.height * 0.5),
                background: theme.surface_hover,
                border_color: theme.accent,
                ..Style::default()
            },
        );
        if let Some(node) = ui.cx.tree.get_mut(node) {
            node.slider_value = self.value.get().clamp(0.0, 1.0);
            node.slider_min = 0.0;
            node.slider_max = 1.0;
        }
        ui.cx.tree.set_accessibility_label(node, "Progress bar");
        ui.response_of(id)
    }
}
