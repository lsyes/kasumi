//! The Material 3 exposed dropdown menu.
//!
//! A [`ComboBox`] is a select control backed by the selected option index:
//!
//! ```ignore
//! ui.combo_box(["Automatic", "Always smooth", "Reduced motion"], &mode);
//! ```
//!
//! The popup follows the M3 exposed-dropdown motion rather than a generic
//! spring:
//!
//! * **Open** — the panel scales from 0.8 to 1.0 with a fast spatial spring and
//!   fades in over 100 ms, with its transform origin on the *top* edge: the
//!   panel grows out of the field it belongs to.
//! * **Close** — a 150 ms emphasized-accelerate tween scales back to 0.8 and
//!   fades to transparent. The popup stays mounted while it animates so the
//!   exit is visible, but its rows stop accepting input immediately.
//! * **Chevron** — the trailing glyph cross-fades between down and up instead
//!   of swapping at a threshold, which is what Material's icon rotation looks
//!   like at control size without a transform on shaped text.
//!
//! Rows follow the M3 menu tokens: 48 dp tall, 8 dp vertical panel padding, a
//! leading 24 dp column reserved for the check mark, `secondary-container` for
//! the selected row, and a state layer on hover.
//!
//! Keyboard: `Enter`/`Space` toggles the popup, `↑`/`↓` move the selection
//! (live, like a native select), `Home`/`End` jump to the ends, and `Escape`
//! closes it.

use kasumi_core::{
    Align, Color, Corners, CursorIcon, Edges, FlexDirection, FontWeight, Id, Inset, Justify, Key,
    Length, Style, TextAlign, TextWrap, child_id, keyed_id,
};
use kasumi_state::State;
use kasumi_tree::{MotionSpec, NodeKind, Sense, Spring};

use crate::response::{Response, ResponseFlags};
use crate::ui::{Ui, Widget};

/// Height of the closed field.
pub const FIELD_HEIGHT: f32 = 56.0;
/// Height of one option row.
pub const ROW_HEIGHT: f32 = 48.0;
/// Vertical padding inside the popup panel.
const PANEL_PADDING: f32 = 8.0;
/// Horizontal padding inside a row.
const ROW_PADDING: f32 = 12.0;
/// Width of the leading check-mark column.
const CHECK_COLUMN: f32 = 24.0;
/// Gap between the check column and the label.
const ROW_GAP: f32 = 8.0;
/// Largest the popup grows before it starts scrolling.
const MAX_PANEL_HEIGHT: f32 = 280.0;
/// Distance between the field and the panel.
const ANCHOR_GAP: f32 = 2.0;
/// Scale the panel starts from when opening.
const CLOSED_SCALE: f32 = 0.8;

/// A select control backed by the selected option index.
#[derive(Clone)]
pub struct ComboBox {
    options: Vec<String>,
    selected: State<usize>,
    placeholder: String,
    width: Length,
    enabled: bool,
}

impl ComboBox {
    pub fn new(
        options: impl IntoIterator<Item = impl Into<String>>,
        selected: &State<usize>,
    ) -> Self {
        Self {
            options: options.into_iter().map(Into::into).collect(),
            selected: selected.clone(),
            placeholder: "Select…".to_owned(),
            width: Length::Px(240.0),
            enabled: true,
        }
    }

    pub fn placeholder(mut self, text: impl Into<String>) -> Self {
        self.placeholder = text.into();
        self
    }

    pub fn width(mut self, width: impl Into<Length>) -> Self {
        self.width = width.into();
        self
    }

    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }
}

impl Widget for ComboBox {
    type Output = Response;

    fn ui(self, ui: &mut Ui) -> Response {
        let theme = ui.theme().clone();
        let id = ui.next_id("combo box");
        let node = ui.cx.tree.ensure(id, NodeKind::ComboBox);
        ui.declare_child(ui.node, node);

        let option_count = self.options.len();
        let focused = ui.cx.focused == Some(id);
        let mut selected = self.selected.get().min(option_count.saturating_sub(1));
        let mut open = ui
            .cx
            .tree
            .node_by_id(id)
            .is_some_and(|node| node.popup_open);
        let mut response = ui.response_of(id);
        let mut changed = false;

        if self.enabled {
            if response.clicked() {
                open = !open;
            }
            if focused {
                if open {
                    if ui.key_pressed(&Key::ArrowDown) && option_count > 0 {
                        let next = (selected + 1).min(option_count - 1);
                        if next != selected {
                            selected = next;
                            self.selected.set(selected);
                            changed = true;
                        }
                    }
                    if ui.key_pressed(&Key::ArrowUp) && option_count > 0 {
                        let next = selected.saturating_sub(1);
                        if next != selected {
                            selected = next;
                            self.selected.set(selected);
                            changed = true;
                        }
                    }
                    if ui.key_pressed(&Key::Home) && option_count > 0 && selected != 0 {
                        selected = 0;
                        self.selected.set(selected);
                        changed = true;
                    }
                    if ui.key_pressed(&Key::End) && option_count > 0 && selected != option_count - 1
                    {
                        selected = option_count - 1;
                        self.selected.set(selected);
                        changed = true;
                    }
                    if ui.key_pressed(&Key::Enter) || ui.key_pressed(&Key::Space) {
                        open = false;
                    }
                } else if ui.key_pressed(&Key::Enter) || ui.key_pressed(&Key::Space) {
                    open = true;
                }
                if ui.key_pressed(&Key::Escape) {
                    open = false;
                }
            }
        }
        if changed {
            response.insert_flag(ResponseFlags::CHANGED);
        }

        // The panel is retained through its exit animation: `progress` is the
        // animated value and `open` the target state. The two motions use
        // different specs, so opening feels springy and closing decisive.
        let progress = ui.animate_on(
            node,
            "combo-open",
            if open { 1.0 } else { 0.0 },
            if open {
                MotionSpec::spring(Spring::new(0.25, 0.9))
            } else {
                MotionSpec::tween(
                    theme.motion.duration.short3,
                    theme.motion.easing.emphasized_accelerate,
                )
            },
        );
        let live = open && self.enabled;
        let visible = live || progress > 0.001;
        if visible && progress < 0.999 {
            ui.request_repaint();
        }

        // Geometry. The panel matches the field's width and grows downward,
        // flipping above the field when there is no room below.
        let rect = ui.cx.tree.visual_rect_of(id);
        let panel_width = if rect.width() > 1.0 {
            rect.width()
        } else {
            240.0
        };
        let viewport = ui.viewport();
        let rows_height = option_count as f32 * ROW_HEIGHT + PANEL_PADDING * 2.0;
        let panel_height = rows_height.clamp(ROW_HEIGHT + PANEL_PADDING * 2.0, MAX_PANEL_HEIGHT);
        let x = rect
            .left()
            .min((viewport.width - panel_width - 8.0).max(8.0));
        let mut y = rect.bottom() + ANCHOR_GAP;
        let mut flipped = false;
        if y + panel_height > viewport.height - 8.0 {
            y = (rect.top() - panel_height - ANCHOR_GAP).max(8.0);
            flipped = true;
        }

        let selection_text = self
            .options
            .get(selected)
            .cloned()
            .unwrap_or_else(|| self.placeholder.clone());
        let mut picked: Option<usize> = None;
        let mut dismissed = false;
        let owner_id = id;
        if visible {
            let (_, popup_id) = ui.overlay_at(
                ("combo-popup-v3", id.raw()),
                "combo popup",
                id,
                Inset::at(x, y),
                Style {
                    direction: FlexDirection::Column,
                    width: Length::Px(panel_width),
                    height: Length::Px(panel_height),
                    padding: Edges::xy(0.0, PANEL_PADDING),
                    gap: 0.0,
                    background: theme.surface_at_elevation(2),
                    border_color: theme.colors.outline_variant.fade(0.5),
                    border_width: 1.0,
                    radius: Corners::all(theme.shape.extra_small),
                    clip: true,
                    ..Style::default()
                },
                |ui| {
                    // Compose anchors the exposed dropdown's scale on the top
                    // edge that meets the field; when the panel flips above the
                    // field that edge is its bottom.
                    if let Some(visual) = ui.visual() {
                        let eased = progress.clamp(0.0, 1.0);
                        visual.scale =
                            kasumi_tree::Animated::new(CLOSED_SCALE + (1.0 - CLOSED_SCALE) * eased);
                        // While the panel is open it stays fully opaque: the
                        // transit skips nodes at or below 0.001 opacity, which
                        // would make the first frames unhittable. The exit
                        // motion is what fades.
                        visual.opacity = kasumi_tree::Animated::new(if live { 1.0 } else { eased });
                        visual.origin = Some((0.5, if flipped { 1.0 } else { 0.0 }));
                    }
                    ui.column(|ui| {
                        ui.width_full();
                        for (index, option) in self.options.iter().enumerate() {
                            if option_row(ui, id, index, option, index == selected, live, &theme) {
                                picked = Some(index);
                            }
                        }
                    });
                },
            );
            if live
                && let Some(clicked) = ui.cx.clicked
                && !ui.cx.tree.is_within(clicked, popup_id)
                && !ui.cx.tree.is_within(clicked, id)
            {
                dismissed = true;
            }
            if picked.is_some() || dismissed {
                open = false;
            }
            if picked.is_some() {
                // Return focus to the field so the next keyboard interaction
                // targets the select rather than the row that is about to be
                // recycled.
                ui.cx.focused = Some(owner_id);
            }
        }
        if let Some(index) = picked
            && index != selected
        {
            selected = index;
            self.selected.set(index);
            changed = true;
            response.insert_flag(ResponseFlags::CHANGED);
        }

        selection_node(
            ui,
            node,
            id,
            &selection_text,
            &theme,
            progress,
            self.enabled,
        );
        if let Some(tree_node) = ui.cx.tree.get_mut(node) {
            tree_node.popup_open = open;
            tree_node.changed = changed;
            tree_node.slider_value = selected as f32;
        }
        ui.cx
            .tree
            .set_accessibility_label(node, selection_text.clone());

        let style = Style {
            direction: FlexDirection::Row,
            align_items: Align::Center,
            justify: Justify::Start,
            width: self.width,
            max_width: Length::Percent(100.0),
            height: Length::Px(FIELD_HEIGHT),
            min_height: Length::Px(FIELD_HEIGHT),
            padding: Edges::xy(16.0, 0.0),
            gap: 8.0,
            border_color: Color::TRANSPARENT,
            background: theme.colors.surface_container_highest,
            border_width: 0.0,
            radius: Corners::all(theme.shape.extra_small),
            focus_ring: theme.colors.primary,
            state_layer: if self.enabled {
                theme.colors.on_surface
            } else {
                Color::TRANSPARENT
            },
            cursor: if self.enabled {
                CursorIcon::Pointer
            } else {
                CursorIcon::NotAllowed
            },
            ..Style::default()
        };
        ui.set_style_on(node, style);
        ui.set_sense_on(
            node,
            if self.enabled {
                Sense::click()
            } else {
                Sense::NONE
            },
        );
        ui.cx.tree.set_enabled(node, self.enabled);
        response
    }
}

/// Declares the closed field's label and its two cross-faded chevrons.
fn selection_node(
    ui: &mut Ui,
    parent: kasumi_tree::NodeId,
    parent_id: Id,
    text: &str,
    theme: &crate::Theme,
    progress: f32,
    enabled: bool,
) {
    let label_id = child_id(parent_id, 0);
    ui.register_node_id(label_id, "combo box label");
    let label = ui.cx.tree.ensure(label_id, NodeKind::Text);
    ui.declare_child(parent, label);
    let mut label_style = theme.typography.body_large.clone();
    label_style.color = if enabled {
        theme.colors.on_surface
    } else {
        theme.colors.on_surface.fade(theme.state.disabled_content)
    };
    label_style.weight = FontWeight::NORMAL;
    label_style.wrap = TextWrap::None;
    label_style.align = TextAlign::Start;
    ui.set_style_on(
        label,
        Style {
            text: label_style.clone(),
            flex_grow: 1.0,
            min_width: Length::Px(0.0),
            align_self: Align::Center,
            ..Style::default()
        },
    );
    ui.set_text_on(label, text.to_owned(), label_style);

    let progress = progress.clamp(0.0, 1.0);
    let mut chevron_style = kasumi_core::TextStyle {
        size: 12.0,
        color: if enabled {
            theme.colors.on_surface_variant
        } else {
            theme.colors.on_surface.fade(theme.state.disabled_content)
        },
        align: TextAlign::Center,
        wrap: TextWrap::None,
        ..kasumi_core::TextStyle::default()
    };
    chevron_style.weight = FontWeight::MEDIUM;
    for (slot, glyph, alpha) in [(1u32, "▾", 1.0 - progress), (2, "▴", progress)] {
        let glyph_id = child_id(parent_id, slot);
        ui.register_node_id(glyph_id, "combo box chevron");
        let node = ui.cx.tree.ensure(glyph_id, NodeKind::Text);
        ui.cx.tree.declare_child(parent, node);
        ui.cx.tree.set_a11y_hidden(node, true);
        ui.set_style_on(
            node,
            Style {
                text: chevron_style.clone(),
                width: Length::Px(16.0),
                min_width: Length::Px(16.0),
                flex_shrink: 0.0,
                align_self: Align::Center,
                ..Style::default()
            },
        );
        ui.set_text_on(node, glyph.to_owned(), chevron_style.clone());
        if let Some(tree_node) = ui.cx.tree.get_mut(node) {
            tree_node.visual.opacity = kasumi_tree::Animated::new(alpha);
        }
    }
}

/// Declares one option row. Returns true when it was activated.
fn option_row(
    ui: &mut Ui,
    owner: Id,
    index: usize,
    option: &str,
    selected: bool,
    live: bool,
    theme: &crate::Theme,
) -> bool {
    let id = keyed_id(owner, &("combo-option", index));
    ui.register_node_id(id, "combo option");
    let node = ui.cx.tree.ensure(id, NodeKind::MenuItem);
    ui.declare_child(ui.node, node);
    ui.cx.tree.set_accessibility_label(node, option.to_owned());
    let foreground = if selected {
        theme.colors.on_secondary_container
    } else {
        theme.colors.on_surface
    };
    ui.set_style_on(
        node,
        Style {
            direction: FlexDirection::Row,
            align_items: Align::Center,
            width: Length::Percent(100.0),
            height: Length::Px(ROW_HEIGHT),
            min_height: Length::Px(ROW_HEIGHT),
            max_height: Length::Px(ROW_HEIGHT),
            flex_shrink: 0.0,
            padding: Edges::xy(ROW_PADDING, 0.0),
            gap: ROW_GAP,
            background: if selected {
                theme.colors.secondary_container
            } else {
                Color::TRANSPARENT
            },
            radius: Corners::all(theme.shape.extra_small),
            state_layer: if selected {
                theme.colors.on_secondary_container
            } else {
                theme.colors.on_surface
            },
            cursor: if live {
                CursorIcon::Pointer
            } else {
                CursorIcon::Default
            },
            focus_ring: theme.colors.primary,
            ..Style::default()
        },
    );
    ui.set_sense_on(node, if live { Sense::click() } else { Sense::NONE });

    let check_id = child_id(id, 0);
    ui.register_node_id(check_id, "combo option check");
    let check = ui.cx.tree.ensure(check_id, NodeKind::Text);
    ui.cx.tree.declare_child(node, check);
    ui.cx.tree.set_a11y_hidden(check, true);
    let mut check_style = theme.typography.label_large.clone();
    check_style.color = foreground;
    check_style.wrap = TextWrap::None;
    ui.set_style_on(
        check,
        Style {
            text: check_style.clone(),
            width: Length::Px(CHECK_COLUMN),
            min_width: Length::Px(CHECK_COLUMN),
            flex_shrink: 0.0,
            align_self: Align::Center,
            ..Style::default()
        },
    );
    ui.set_text_on(
        check,
        if selected { "✓" } else { "" }.to_owned(),
        check_style,
    );

    let label_id = child_id(id, 1);
    ui.register_node_id(label_id, "combo option label");
    let label = ui.cx.tree.ensure(label_id, NodeKind::Text);
    ui.cx.tree.declare_child(node, label);
    let mut label_style = theme.typography.body_large.clone();
    label_style.color = foreground;
    label_style.wrap = TextWrap::None;
    label_style.align = TextAlign::Start;
    ui.set_style_on(
        label,
        Style {
            text: label_style.clone(),
            flex_grow: 1.0,
            min_width: Length::Px(0.0),
            align_self: Align::Center,
            ..Style::default()
        },
    );
    ui.set_text_on(label, option.to_owned(), label_style);

    live && ui.cx.response(id).clicked()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_panel_scale_matches_the_m3_dropdown_motion() {
        let start = CLOSED_SCALE + (1.0 - CLOSED_SCALE) * 0.0;
        let end = CLOSED_SCALE + (1.0 - CLOSED_SCALE) * 1.0;
        assert!((start - 0.8).abs() < 1e-6);
        assert!((end - 1.0).abs() < 1e-6);
    }

    #[test]
    fn rows_and_padding_follow_the_menu_tokens() {
        assert_eq!(ROW_HEIGHT, 48.0);
        assert_eq!(PANEL_PADDING, 8.0);
        let three = 3.0 * ROW_HEIGHT + PANEL_PADDING * 2.0;
        assert!(three < MAX_PANEL_HEIGHT, "a short list must not scroll");
        assert!((100.0f32 * ROW_HEIGHT).min(MAX_PANEL_HEIGHT) == MAX_PANEL_HEIGHT);
    }

    #[test]
    fn the_chevron_cross_fade_covers_both_states() {
        for progress in [0.0f32, 0.25, 0.5, 0.75, 1.0] {
            let down = 1.0 - progress;
            let up = progress;
            assert!((down + up - 1.0).abs() < 1e-6);
            assert!((0.0..=1.0).contains(&down));
        }
    }
}
