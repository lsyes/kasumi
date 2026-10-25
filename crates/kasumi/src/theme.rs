//! Default palette and metrics.
//!
//! The theme is plain data. Widgets copy the values they need into their node
//! styles, so changing a theme only affects nodes declared after the change.

use kasumi_core::{Color, TextStyle};

#[derive(Debug, Clone, PartialEq)]
pub struct Theme {
    pub background: Color,
    pub panel: Color,
    pub surface: Color,
    pub surface_hover: Color,
    pub scrollbar_rail: Color,
    pub scrollbar_thumb: Color,
    pub scrollbar_thumb_hover: Color,
    pub border: Color,
    pub border_strong: Color,
    pub text: Color,
    pub text_muted: Color,
    pub text_subtle: Color,
    pub accent: Color,
    pub accent_hover: Color,
    pub on_accent: Color,
    pub focus_ring: Color,
    pub danger: Color,
    pub success: Color,

    pub font_size: f32,
    pub font_size_small: f32,
    pub font_size_large: f32,
    pub radius_small: f32,
    pub radius_medium: f32,
    pub radius_large: f32,

    pub spacing: f32,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            // macOS light palette: a quiet neutral window, white cards, and
            // the system blue for selection and primary actions.
            background: Color::hex("#f5f5f7"),
            panel: Color::hex("#ffffff"),
            surface: Color::hex("#f7f7f9"),
            surface_hover: Color::hex("#ececf0"),
            scrollbar_rail: Color::hex("#e6e6ea"),
            scrollbar_thumb: Color::hex("#b9b9c0"),
            scrollbar_thumb_hover: Color::hex("#8c8c95"),
            border: Color::hex("#d8d8de"),
            border_strong: Color::hex("#b8b8c0"),
            text: Color::hex("#1d1d1f"),
            text_muted: Color::hex("#66666d"),
            text_subtle: Color::hex("#8b8b92"),
            accent: Color::hex("#007aff"),
            accent_hover: Color::hex("#0a84ff"),
            on_accent: Color::hex("#ffffff"),
            focus_ring: Color::hex("#007aff"),
            danger: Color::hex("#ff3b30"),
            success: Color::hex("#248a3d"),
            font_size: 14.0,
            font_size_small: 12.0,
            font_size_large: 18.0,
            radius_small: 7.0,
            radius_medium: 10.0,
            radius_large: 16.0,
            spacing: 12.0,
        }
    }
}

impl Theme {
    pub fn text_style(&self) -> TextStyle {
        TextStyle::default().size(self.font_size).color(self.text)
    }

    pub fn heading_style(&self, size: f32) -> TextStyle {
        TextStyle::default()
            .size(size)
            .color(self.text)
            .weight(kasumi_core::FontWeight::SEMIBOLD)
    }
}
