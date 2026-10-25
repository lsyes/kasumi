use kasumi_core::{
    Align, Corners, CursorIcon, Edges, FlexDirection, FontWeight, Id, Inset, Justify, Key, Length,
    Style, TextAlign, TextStyle,
};
use kasumi_state::State;
use kasumi_tree::{NodeKind, Sense};

use crate::response::{Response, ResponseFlags};
use crate::ui::{Ui, Widget};

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

        let focused = ui.cx.focused == Some(id);
        let mut selected = self
            .selected
            .get()
            .min(self.options.len().saturating_sub(1));
        let mut open = ui.cx.tree.get(node).is_some_and(|node| node.popup_open);
        let mut response = ui.response_of(id);
        if self.enabled && response.clicked() {
            open = !open;
        }
        if focused && ui.key_pressed(&Key::Escape) {
            open = false;
        }
        if open && focused {
            if ui.key_pressed(&Key::ArrowDown) && !self.options.is_empty() {
                selected = (selected + 1).min(self.options.len() - 1);
                self.selected.set(selected);
                response.insert_flag(ResponseFlags::CHANGED);
            } else if ui.key_pressed(&Key::ArrowUp) && !self.options.is_empty() {
                selected = selected.saturating_sub(1);
                self.selected.set(selected);
                response.insert_flag(ResponseFlags::CHANGED);
            }
        }

        let rect = ui.cx.tree.visual_rect_of(id);
        let panel_width = if rect.width() > 1.0 {
            rect.width()
        } else {
            240.0
        };
        let panel_height = (self.options.len() as f32 * 34.0 + 12.0).clamp(42.0, 240.0);
        let viewport = ui.viewport();
        let x = rect
            .left()
            .min((viewport.width - panel_width - 8.0).max(8.0));
        let mut y = rect.bottom() + 4.0;
        if y + panel_height > viewport.height - 8.0 {
            y = (rect.top() - panel_height - 4.0).max(8.0);
        }

        let mut changed = response.flags().contains(ResponseFlags::CHANGED);
        if open && self.enabled {
            selection_node(ui, node, id, &self.options[selected], &theme);
            let (popup_response, popup_id) = ui.overlay_at(
                ("combo-popup", id.raw()),
                "combo popup",
                id,
                Inset::at(x, y),
                Style {
                    direction: FlexDirection::Column,
                    width: Length::Px(panel_width),
                    height: Length::Px(panel_height),
                    padding: Edges::all(6.0),
                    gap: 2.0,
                    background: theme.panel,
                    border_color: theme.border_strong,
                    border_width: 1.0,
                    radius: Corners::all(theme.radius_medium),
                    clip: true,
                    ..Style::default()
                },
                |ui| {
                    ui.scroll(crate::ScrollView::vertical().smooth(false), |ui| {
                        ui.width_full().gap(2.0);
                        for (index, option) in self.options.iter().enumerate() {
                            let mut button = crate::Button::new(option)
                                .full_width()
                                .padding_xy(10.0, 7.0)
                                .font_size(13.0);
                            if index == selected {
                                button = button.primary();
                            } else {
                                button = button.subtle();
                            }
                            if ui.add(button).clicked() && index != selected {
                                selected = index;
                                self.selected.set(index);
                                changed = true;
                                open = false;
                            }
                        }
                    });
                },
            );
            if let Some(clicked) = ui.cx.clicked {
                if ui.cx.tree.is_within(clicked, popup_id) {
                    if popup_response.clicked() || ui.cx.tree.is_within(clicked, popup_id) {
                        open = ui.cx.tree.is_within(clicked, id);
                    }
                } else if !ui.cx.tree.is_within(clicked, id) {
                    open = false;
                }
            }
            if ui.key_pressed(&Key::Escape) {
                open = false;
            }
        } else {
            selection_node(ui, node, id, &self.options[selected], &theme);
        }

        let style = Style {
            direction: FlexDirection::Row,
            align_items: Align::Center,
            justify: Justify::Start,
            width: self.width,
            height: Length::Px(38.0),
            min_height: Length::Px(38.0),
            padding: Edges::xy(11.0, 0.0),
            gap: 8.0,
            background: theme.surface,
            border_color: if open { theme.accent } else { theme.border },
            border_width: 1.0,
            radius: Corners::all(theme.radius_medium),
            focus_ring: theme.focus_ring,
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
        if let Some(node) = ui.cx.tree.get_mut(node) {
            node.popup_open = open;
            node.changed = changed;
        }
        ui.cx
            .tree
            .set_accessibility_label(node, self.options[selected].clone());
        if changed {
            response.insert_flag(ResponseFlags::CHANGED);
        }
        response
    }
}

fn selection_node(
    ui: &mut Ui,
    parent: kasumi_tree::NodeId,
    parent_id: Id,
    text: &str,
    theme: &crate::Theme,
) {
    let label_id = kasumi_core::child_id(parent_id, 0);
    ui.register_node_id(label_id, "combo box label");
    let label = ui.cx.tree.ensure(label_id, NodeKind::Text);
    ui.cx.tree.declare_child(parent, label);
    let label_style = TextStyle {
        size: theme.font_size,
        color: theme.text,
        weight: FontWeight::MEDIUM,
        ..TextStyle::default()
    };
    ui.set_style_on(
        label,
        Style {
            text: label_style.clone(),
            flex_grow: 1.0,
            align_self: Align::Center,
            ..Style::default()
        },
    );
    ui.set_text_on(label, text.to_owned(), label_style);

    let glyph_id = kasumi_core::child_id(parent_id, 1);
    ui.register_node_id(glyph_id, "combo box chevron");
    let glyph = ui.cx.tree.ensure(glyph_id, NodeKind::Text);
    ui.cx.tree.declare_child(parent, glyph);
    ui.cx.tree.set_a11y_hidden(glyph, true);
    let glyph_style = TextStyle {
        size: 11.0,
        color: theme.text_subtle,
        align: TextAlign::Center,
        ..TextStyle::default()
    };
    ui.set_style_on(
        glyph,
        Style {
            text: glyph_style.clone(),
            width: Length::Px(14.0),
            align_self: Align::Center,
            ..Style::default()
        },
    );
    ui.set_text_on(glyph, "▾".to_owned(), glyph_style);
}
