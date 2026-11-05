//! Modal dialog configuration.
//!
//! The rendering lives in [`crate::Ui::dialog`]: it needs the root overlay
//! layer, scrim hit testing and keyboard dismissal, none of which belong in a
//! declarative config type.

use kasumi_core::{Color, Size};
use kasumi_state::State;

/// A modal dialog controlled by an external open state.
///
/// ```ignore
/// let open = ui.state(|| false);
/// if ui.add(Button::new("Rename")).clicked() {
///     open.set(true);
/// }
/// ui.dialog(
///     Dialog::new(&open).title("Rename file").width(420.0),
///     |ui| {
///         ui.add(TextInput::new(&name));
///         ui.row(|ui| {
///             if ui.add(Button::new("Cancel").subtle()).clicked() {
///                 open.set(false);
///             }
///             if ui.add(Button::new("Save").primary()).clicked() {
///                 save();
///                 open.set(false);
///             }
///         });
///     },
/// );
/// ```
#[derive(Debug, Clone)]
pub struct Dialog {
    pub open: State<bool>,
    pub title: Option<String>,
    pub description: Option<String>,
    /// Preferred width in logical pixels, clamped to the current viewport.
    pub width: f32,
    /// Allow Escape and scrim clicks to close the dialog.
    pub dismissible: bool,
    pub scrim: Option<Color>,
    pub padding: f32,
}

impl Dialog {
    pub fn new(open: &State<bool>) -> Self {
        Self {
            open: open.clone(),
            title: None,
            description: None,
            width: 480.0,
            dismissible: true,
            scrim: None,
            padding: 24.0,
        }
    }

    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    pub fn width(mut self, width: f32) -> Self {
        self.width = width.max(240.0);
        self
    }

    pub fn dismissible(mut self, dismissible: bool) -> Self {
        self.dismissible = dismissible;
        self
    }

    pub fn scrim(mut self, color: Color) -> Self {
        self.scrim = Some(color);
        self
    }

    pub fn padding(mut self, padding: f32) -> Self {
        self.padding = padding.max(0.0);
        self
    }

    /// Clamps the preferred width to the window, leaving a 24 px margin on
    /// each side for narrow windows.
    pub fn resolved_width(&self, viewport: Size) -> f32 {
        self.width.min((viewport.width - 48.0).max(200.0))
    }
}
