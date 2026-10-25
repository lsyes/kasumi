use kasumi_core::{FlexDirection, Length};

/// Configuration for a scrollable viewport.
#[derive(Debug, Clone, Copy)]
pub struct ScrollView {
    pub horizontal: bool,
    pub vertical: bool,
    pub smooth: bool,
    pub line_step: f32,
    pub width: Length,
    pub height: Length,
    pub grow: f32,
}

impl ScrollView {
    pub fn vertical() -> Self {
        Self {
            horizontal: false,
            vertical: true,
            smooth: true,
            line_step: 52.0,
            width: Length::Percent(100.0),
            height: Length::Auto,
            grow: 0.0,
        }
    }

    pub fn horizontal() -> Self {
        Self {
            horizontal: true,
            vertical: false,
            smooth: true,
            line_step: 52.0,
            width: Length::Auto,
            height: Length::Percent(100.0),
            grow: 0.0,
        }
    }

    pub fn both() -> Self {
        Self {
            horizontal: true,
            vertical: true,
            smooth: true,
            line_step: 52.0,
            width: Length::Percent(100.0),
            height: Length::Percent(100.0),
            grow: 0.0,
        }
    }

    pub fn smooth(mut self, smooth: bool) -> Self {
        self.smooth = smooth;
        self
    }

    pub fn line_step(mut self, step: f32) -> Self {
        self.line_step = step.max(1.0);
        self
    }

    pub fn size(mut self, width: impl Into<Length>, height: impl Into<Length>) -> Self {
        self.width = width.into();
        self.height = height.into();
        self
    }

    /// Lets the viewport consume remaining space in the parent flex layout.
    pub fn grow(mut self, grow: f32) -> Self {
        self.grow = grow.max(0.0);
        self
    }

    pub fn direction(self) -> FlexDirection {
        if self.horizontal && !self.vertical {
            FlexDirection::Row
        } else {
            FlexDirection::Column
        }
    }
}
