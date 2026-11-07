//! Window descriptions and cross-window requests.
//!
//! A backend owns the actual platform windows. The runtime knows only these
//! value types, which lets applications inspect the current window and ask the
//! backend to open, close, focus or rename windows without depending on winit.

use kasumi_core::{Point, Size, WindowKey};

use crate::Theme;

/// A snapshot of one live window.
#[derive(Debug, Clone, PartialEq)]
pub struct WindowInfo {
    pub key: WindowKey,
    pub title: String,
    /// Logical size in points.
    pub size: Size,
    pub scale_factor: f32,
    /// True for the window that was opened by the single-window entry point.
    pub is_primary: bool,
    pub focused: bool,
}

impl Default for WindowInfo {
    fn default() -> Self {
        Self {
            key: WindowKey::MAIN,
            title: "Kasumi".to_owned(),
            size: Size::new(800.0, 600.0),
            scale_factor: 1.0,
            is_primary: true,
            focused: true,
        }
    }
}

impl WindowInfo {
    pub fn new(key: WindowKey, title: impl Into<String>, size: Size) -> Self {
        Self {
            key,
            title: title.into(),
            size,
            scale_factor: 1.0,
            is_primary: false,
            focused: true,
        }
    }

    pub fn is_primary(&self) -> bool {
        self.is_primary
    }

    pub fn is_main(&self) -> bool {
        self.is_primary
    }
}

/// A request to open another window.
///
/// The backend assigns the actual [`WindowKey`]; inspect [`crate::Ui::windows`]
/// on a later frame to discover it. `theme: None` inherits the default theme
/// of the running application.
#[derive(Debug, Clone)]
pub struct WindowRequest {
    pub title: String,
    pub size: Size,
    pub min_size: Option<Size>,
    pub theme: Option<Theme>,
    pub vsync: bool,
}

impl Default for WindowRequest {
    fn default() -> Self {
        Self {
            title: "Kasumi".to_owned(),
            size: Size::new(960.0, 680.0),
            min_size: Some(Size::new(320.0, 240.0)),
            theme: None,
            vsync: true,
        }
    }
}

impl WindowRequest {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            ..Self::default()
        }
    }

    pub fn size(mut self, width: f32, height: f32) -> Self {
        self.size = Size::new(width.max(1.0), height.max(1.0));
        self
    }

    pub fn min_size(mut self, width: f32, height: f32) -> Self {
        self.min_size = Some(Size::new(width.max(1.0), height.max(1.0)));
        self
    }

    pub fn theme(mut self, theme: Theme) -> Self {
        self.theme = Some(theme);
        self
    }

    pub fn vsync(mut self, vsync: bool) -> Self {
        self.vsync = vsync;
        self
    }
}

/// A command emitted by `Ui` during `view` and executed by the backend after
/// the frame completes.
#[derive(Debug, Clone)]
pub enum WindowAction {
    Open(WindowRequest),
    Close(WindowKey),
    Focus(WindowKey),
    SetTitle { window: WindowKey, title: String },
    /// Move a window. X11 honors this for override-redirect popups; Wayland
    /// compositors may ignore it for ordinary toplevels, which is why the IME
    /// backend uses a layer-shell surface instead.
    SetPosition { window: WindowKey, position: Point },
}
