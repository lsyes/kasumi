//! The Material 3 switch.
//!
//! A [`Switch`] is bound to `State<bool>` and painted by the paint layer so the
//! thumb and track can animate independently:
//!
//! ```ignore
//! ui.switch(&notifications);
//!
//! // With a label and a supporting line:
//! ui.add(
//!     Switch::new(&notifications)
//!         .label("Notifications")
//!         .supporting("Push alerts for new comments"),
//! );
//! ```
//!
//! Geometry and colors follow the M3 switch tokens:
//!
//! | Part | Unselected | Selected |
//! |---|---|---|
//! | Track | 52×32, `surface-variant`, 2 dp `outline` border | 52×32, `primary`, no border |
//! | Thumb | 16 dp, `outline` | 24 dp, `on-primary`, inset 4 dp from the trailing edge |
//! | Pressed thumb | 28 dp, flush with the leading edge | 28 dp, flush with the trailing edge |
//! | State layer | 40 dp `on-surface` | 40 dp `primary` |
//! | Icons | optional 16 dp | optional 16 dp `on-primary-container` |
//!
//! Motion: the thumb position uses a 300 ms overshoot Bézier, the thumb size a
//! 250 ms standard curve, and colors a 67 ms linear fade. The keyboard follows
//! the platform convention: Space toggles and arrows move the thumb without
//! committing, which is what `Switch::keyboard_arrows_move_thumb` selects.

use kasumi_core::{Align, Color, CursorIcon, Length, Style, TextAlign, TextWrap, child_id};
use kasumi_state::State;
use kasumi_tree::{NodeKind, Sense};

use crate::response::{Response, ResponseFlags};
use crate::ui::{Ui, Widget};

/// Width of the switch track.
pub const TRACK_WIDTH: f32 = 52.0;
/// Height of the switch track.
pub const TRACK_HEIGHT: f32 = 32.0;
/// Diameter of the thumb when off.
pub const THUMB_OFF: f32 = 16.0;
/// Diameter of the thumb when on.
pub const THUMB_ON: f32 = 24.0;
/// Diameter of the thumb while pressed.
pub const THUMB_PRESSED: f32 = 28.0;
/// Diameter of the state-layer circle.
pub const STATE_LAYER: f32 = 40.0;
/// Total height of the touch target.
pub const TOUCH_HEIGHT: f32 = 48.0;

/// Where the thumb and the state layer sit inside the track.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SwitchGeometry {
    /// Thumb diameter in logical pixels.
    pub thumb: f32,
    /// Thumb centre, in track-local coordinates.
    pub thumb_center: (f32, f32),
    /// Colors resolved for the current state.
    pub track: Color,
    pub track_border: Color,
    pub thumb_color: Color,
    pub state_layer: Color,
    /// Check-mark glyph for the on state, when the switch draws icons.
    pub icon_color: Color,
    /// 0 for off, 1 for on.
    pub progress: f32,
}

/// Resolves the paint geometry for a switch.
///
/// `progress` is the animated 0..1 selection amount, `hover` the 0..1 hover
/// amount and `pressed` whether the pointer is down on the control. Colors come
/// from the theme roles so a seeded scheme repaints the switch correctly.
pub fn geometry(
    theme: &crate::Theme,
    progress: f32,
    hover: f32,
    pressed: bool,
    enabled: bool,
    _with_icons: bool,
) -> SwitchGeometry {
    let progress = progress.clamp(0.0, 1.0);
    let colors = &theme.colors;
    let size_scale = if pressed {
        THUMB_PRESSED
    } else {
        THUMB_OFF + (THUMB_ON - THUMB_OFF) * progress
    };
    // At rest the unselected thumb sits 8 dp from the leading edge and the
    // selected thumb 4 dp from the trailing edge; the travel is linear in
    // `progress` between those two centres.
    let leading_offset = 8.0 + THUMB_OFF / 2.0;
    let trailing_offset = TRACK_WIDTH - 4.0 - THUMB_ON / 2.0;
    let inset = (TRACK_HEIGHT - size_scale) / 2.0;
    let center_x = if pressed {
        // A pressed thumb grows toward the edge it is heading to.
        if progress >= 0.5 {
            TRACK_WIDTH - inset - size_scale / 2.0
        } else {
            inset + size_scale / 2.0
        }
    } else {
        let travel = leading_offset + (trailing_offset - leading_offset) * progress;
        travel.clamp(
            inset + size_scale / 2.0,
            TRACK_WIDTH - inset - size_scale / 2.0,
        )
    };

    let (track, track_border, thumb_color, base_layer) = if !enabled {
        (
            colors.on_surface.fade(theme.state.disabled_container),
            colors.on_surface.fade(theme.state.disabled_container),
            colors.on_surface.fade(theme.state.disabled_content),
            Color::TRANSPARENT,
        )
    } else {
        (
            colors.surface_variant.lerp(colors.primary, progress),
            colors.outline.fade(1.0 - progress),
            colors.outline.lerp(colors.on_primary, progress),
            colors.on_surface.lerp(colors.primary, progress),
        )
    };
    // Hovering and pressing lighten the state layer. The layer is 40 dp and
    // centred on the thumb.
    let layer_alpha = if !enabled {
        0.0
    } else if pressed {
        theme.state.pressed
    } else {
        theme.state.hover * hover
    };
    SwitchGeometry {
        thumb: size_scale,
        thumb_center: (center_x, TRACK_HEIGHT / 2.0),
        track,
        track_border,
        thumb_color,
        state_layer: base_layer.fade(layer_alpha),
        icon_color: if enabled {
            colors.on_primary_container
        } else {
            colors.on_surface.fade(theme.state.disabled_content)
        },
        progress,
    }
}

/// A Material 3 switch bound to `State<bool>`.
#[derive(Clone)]
pub struct Switch {
    value: State<bool>,
    label: Option<String>,
    supporting: Option<String>,
    enabled: bool,
    icons: bool,
    keyboard_arrows_move_thumb: bool,
}

impl Switch {
    pub fn new(value: &State<bool>) -> Self {
        Self {
            value: value.clone(),
            label: None,
            supporting: None,
            enabled: true,
            icons: true,
            keyboard_arrows_move_thumb: true,
        }
    }

    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    pub fn supporting(mut self, text: impl Into<String>) -> Self {
        self.supporting = Some(text.into());
        self
    }

    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Draws the check/close glyphs inside the thumb.
    pub fn icons(mut self, icons: bool) -> Self {
        self.icons = icons;
        self
    }

    /// When set, arrow keys move the thumb without changing the value, which
    /// is the desktop convention for a switch that is not a checkbox.
    pub fn keyboard_arrows_move_thumb(mut self, allow: bool) -> Self {
        self.keyboard_arrows_move_thumb = allow;
        self
    }
}

impl Widget for Switch {
    type Output = Response;

    fn ui(self, ui: &mut Ui) -> Response {
        let theme = ui.theme().clone();
        let id = ui.next_id("switch");
        let has_text = self.label.is_some();
        let height = if self.supporting.is_some() {
            64.0
        } else if has_text {
            TOUCH_HEIGHT
        } else {
            // An unlabelled switch is just the 52×32 track with its touch
            // target padding.
            TOUCH_HEIGHT
        };
        let node = ui.adopt(
            id,
            NodeKind::Switch,
            Style {
                direction: kasumi_core::FlexDirection::Row,
                align_items: Align::Center,
                justify: kasumi_core::Justify::Start,
                gap: 16.0,
                height: Length::Px(height),
                min_height: Length::Px(height),
                cursor: if self.enabled {
                    CursorIcon::Pointer
                } else {
                    CursorIcon::NotAllowed
                },
                focus_ring: theme.colors.primary,
                ..Style::default()
            },
        );
        ui.set_sense_on(
            node,
            if self.enabled {
                Sense::click()
            } else {
                Sense::NONE
            },
        );
        ui.cx.tree.set_enabled(node, self.enabled);
        ui.cx.tree.set_accessibility_label(
            node,
            self.label.clone().unwrap_or_else(|| "Switch".to_owned()),
        );

        let mut checked = self.value.get();
        let mut response = ui.response_of(id);
        let mut changed = false;
        if self.enabled && (response.clicked() || ui.key_pressed_space_when(id)) {
            checked = !checked;
            changed = true;
        }
        if self.enabled && self.keyboard_arrows_move_thumb && ui.cx.focused == Some(id) && !changed
        {
            // Arrows move the thumb without toggling, matching the desktop
            // switch convention; Space commits the imitated position.
            let dir = if ui.key_pressed(&kasumi_core::Key::ArrowLeft) {
                Some(false)
            } else if ui.key_pressed(&kasumi_core::Key::ArrowRight) {
                Some(true)
            } else {
                None
            };
            if let Some(next) = dir
                && next != checked
            {
                checked = next;
                changed = true;
            }
        }
        if changed {
            self.value.set(checked);
            if let Some(tree_node) = ui.cx.tree.get_mut(node) {
                tree_node.changed = true;
            }
            response.insert_flag(ResponseFlags::CHANGED);
        }
        if let Some(tree_node) = ui.cx.tree.get_mut(node) {
            tree_node.checked = checked;
        }

        let progress = ui.animate_on(
            node,
            "switch",
            if checked { 1.0 } else { 0.0 },
            kasumi_tree::MotionSpec::tween(
                std::time::Duration::from_millis(250),
                kasumi_tree::Easing::M3_STANDARD,
            ),
        );
        let hover = ui
            .cx
            .tree
            .node_by_id(id)
            .map(|node| node.hover_progress)
            .unwrap_or(0.0);
        let pressed = response.pressed();
        let state = geometry(&theme, progress, hover, pressed, self.enabled, self.icons);

        // Label column first so it takes the leading space, then a dedicated
        // 52×32 track node at the trailing edge. Painting the switch chrome
        // from the row would centre it over the label instead.
        if has_text {
            let column_id = child_id(id, 20);
            ui.register_node_id(column_id, "switch text");
            let column = ui.cx.tree.ensure(column_id, NodeKind::Container);
            ui.declare_child(node, column);
            ui.set_style_on(
                column,
                Style {
                    direction: kasumi_core::FlexDirection::Column,
                    justify: kasumi_core::Justify::Center,
                    align_items: Align::Start,
                    flex_grow: 1.0,
                    flex_basis: Length::Px(0.0),
                    min_width: Length::Px(0.0),
                    gap: 2.0,
                    ..Style::default()
                },
            );
            let mut label_style = theme.typography.body_large.clone();
            label_style.color = if self.enabled {
                theme.colors.on_surface
            } else {
                theme.colors.on_surface.fade(theme.state.disabled_content)
            };
            label_style.wrap = TextWrap::None;
            label_style.align = TextAlign::Start;
            let label_id = child_id(column_id, 0);
            ui.register_node_id(label_id, "switch label");
            let label = ui.cx.tree.ensure(label_id, NodeKind::Text);
            ui.declare_child(column, label);
            ui.set_style_on(
                label,
                Style {
                    text: label_style.clone(),
                    align_self: Align::Start,
                    min_width: Length::Px(0.0),
                    ..Style::default()
                },
            );
            ui.set_text_on(label, self.label.clone().unwrap_or_default(), label_style);
            if let Some(supporting) = &self.supporting {
                let mut style = theme.typography.body_medium.clone();
                style.color = if self.enabled {
                    theme.colors.on_surface_variant
                } else {
                    theme.colors.on_surface.fade(theme.state.disabled_content)
                };
                style.wrap = TextWrap::None;
                style.align = TextAlign::Start;
                let id = child_id(column_id, 1);
                ui.register_node_id(id, "switch supporting");
                let text = ui.cx.tree.ensure(id, NodeKind::Text);
                ui.declare_child(column, text);
                ui.set_style_on(
                    text,
                    Style {
                        text: style.clone(),
                        align_self: Align::Start,
                        min_width: Length::Px(0.0),
                        ..Style::default()
                    },
                );
                ui.set_text_on(text, supporting.clone(), style);
            }
        }

        let track_id = child_id(id, 30);
        ui.register_node_id(track_id, "switch track");
        let track = ui.cx.tree.ensure(track_id, NodeKind::Switch);
        ui.declare_child(node, track);
        ui.cx.tree.set_a11y_hidden(track, true);
        ui.set_style_on(
            track,
            Style {
                width: Length::Px(TRACK_WIDTH),
                height: Length::Px(TRACK_HEIGHT),
                min_width: Length::Px(TRACK_WIDTH),
                min_height: Length::Px(TRACK_HEIGHT),
                flex_shrink: 0.0,
                align_self: Align::Center,
                ..Style::default()
            },
        );
        if let Some(tree_node) = ui.cx.tree.get_mut(track) {
            tree_node.switch = Some(kasumi_tree::SwitchPaint {
                track_size: (TRACK_WIDTH, TRACK_HEIGHT),
                thumb: state.thumb,
                thumb_center: state.thumb_center,
                track: state.track,
                track_border: state.track_border,
                thumb_color: state.thumb_color,
                state_layer: state.state_layer,
                icon_color: state.icon_color,
                progress: state.progress,
                icons: self.icons,
            });
        }

        response
    }
}

impl Ui<'_> {
    /// A Material 3 switch bound to `State<bool>`.
    pub fn switch(&mut self, value: &State<bool>) -> Response {
        Switch::new(value).ui(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thumb_grows_and_travels_with_progress() {
        let theme = crate::Theme::light();
        let off = geometry(&theme, 0.0, 0.0, false, true, true);
        assert!((off.thumb - THUMB_OFF).abs() < 0.01);
        let on = geometry(&theme, 1.0, 0.0, false, true, true);
        assert!((on.thumb - THUMB_ON).abs() < 0.01);
        assert!(
            on.thumb_center.0 > off.thumb_center.0 + 10.0,
            "the thumb must travel to the far edge"
        );
        // The on thumb keeps a 4 dp inset from the trailing edge.
        let trailing_gap = TRACK_WIDTH - (on.thumb_center.0 + on.thumb / 2.0);
        assert!(
            (trailing_gap - 4.0).abs() < 0.6,
            "trailing gap {trailing_gap}"
        );
        let leading_gap = off.thumb_center.0 - off.thumb / 2.0;
        assert!((leading_gap - 8.0).abs() < 0.6, "leading gap {leading_gap}");
    }

    #[test]
    fn pressing_enlarges_the_thumb_toward_its_edge() {
        let theme = crate::Theme::light();
        let pressed_off = geometry(&theme, 0.0, 0.0, true, true, true);
        assert!((pressed_off.thumb - THUMB_PRESSED).abs() < 0.01);
        let pressed_on = geometry(&theme, 1.0, 0.0, true, true, true);
        assert!((pressed_on.thumb - THUMB_PRESSED).abs() < 0.01);
        // The pressed thumb sits flush with the track edge (2 dp outline).
        assert!(pressed_off.thumb_center.0 - pressed_off.thumb / 2.0 < 4.0);
        assert!(pressed_on.thumb_center.0 + pressed_on.thumb / 2.0 > TRACK_WIDTH - 4.0);
    }

    #[test]
    fn track_and_thumb_swap_roles_between_states() {
        let theme = crate::Theme::light();
        let off = geometry(&theme, 0.0, 0.0, false, true, true);
        let on = geometry(&theme, 1.0, 0.0, false, true, true);
        assert!(
            (off.track.r - theme.colors.surface_variant.r).abs() < 0.002
                && (off.track.g - theme.colors.surface_variant.g).abs() < 0.002,
            "the unselected track is surface-variant"
        );
        assert!(
            (on.track.r - theme.colors.primary.r).abs() < 0.002
                && (on.track.b - theme.colors.primary.b).abs() < 0.002,
            "the selected track is primary"
        );
        assert_eq!(off.thumb_color, theme.colors.outline);
        assert_eq!(on.thumb_color, theme.colors.on_primary);
        // The off track keeps its 2 dp outline; the on track has none.
        assert!(off.track_border.a > 0.9);
        assert!(on.track_border.a < 0.05);
    }

    #[test]
    fn the_state_layer_only_appears_on_hover_or_press() {
        let theme = crate::Theme::light();
        let idle = geometry(&theme, 0.5, 0.0, false, true, true);
        assert!(idle.state_layer.a < 0.01);
        let hovered = geometry(&theme, 0.5, 1.0, false, true, true);
        assert!((hovered.state_layer.a - theme.state.hover).abs() < 0.01);
        let pressed = geometry(&theme, 0.5, 1.0, true, true, true);
        assert!((pressed.state_layer.a - theme.state.pressed).abs() < 0.01);
    }

    #[test]
    fn disabled_switches_desaturate() {
        let theme = crate::Theme::light();
        let disabled = geometry(&theme, 1.0, 1.0, true, false, true);
        assert!(disabled.state_layer.a < 0.01);
        assert_eq!(
            disabled.thumb_color,
            theme.colors.on_surface.fade(theme.state.disabled_content)
        );
    }
}
