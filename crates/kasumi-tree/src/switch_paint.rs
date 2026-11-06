//! Paint data for the Material 3 switch.
//!
//! The widget resolves the animated geometry and colors once per frame and
//! stores them on the node; the paint layer only replays them, which keeps the
//! theme crate out of `kasumi-paint` and keeps `kasumi-tree` dependency-free.

use kasumi_core::Color;

/// Resolved switch geometry and colors for one frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SwitchPaint {
    /// Track size in logical pixels.
    pub track_size: (f32, f32),
    /// Thumb diameter.
    pub thumb: f32,
    /// Thumb centre in track-local coordinates.
    pub thumb_center: (f32, f32),
    pub track: Color,
    pub track_border: Color,
    pub thumb_color: Color,
    pub state_layer: Color,
    pub icon_color: Color,
    /// 0 for off, 1 for on; drives the icon glyph.
    pub progress: f32,
    /// Whether the switch draws its check/close icons.
    pub icons: bool,
}
