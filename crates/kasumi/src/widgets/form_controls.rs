use kasumi_core::{
    Align, Color, Corners, CursorIcon, Edges, FontWeight, Id, Justify, Length, Style, TextAlign,
    TextStyle,
};
use kasumi_state::State;
use kasumi_tree::{NodeKind, Sense};

use crate::response::{Response, ResponseFlags};
use crate::ui::{Ui, Widget};

#[derive(Clone)]
pub struct Checkbox {
    label: String,
    checked: State<bool>,
    enabled: bool,
}

impl Checkbox {
    pub fn new(label: impl Into<String>, checked: &State<bool>) -> Self {
        Self {
            label: label.into(),
            checked: checked.clone(),
            enabled: true,
        }
    }

    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }
}

impl Widget for Checkbox {
    type Output = Response;

    fn ui(self, ui: &mut Ui) -> Response {
        let theme = ui.theme().clone();
        let id = ui.next_id("checkbox");
        let mut checked = self.checked.get();
        let node = ui.adopt(
            id,
            NodeKind::Checkbox,
            Style {
                direction: kasumi_core::FlexDirection::Row,
                align_items: Align::Center,
                gap: 9.0,
                height: Length::Px(26.0),
                min_height: Length::Px(26.0),
                cursor: if self.enabled {
                    CursorIcon::Pointer
                } else {
                    CursorIcon::NotAllowed
                },
                focus_ring: theme.focus_ring,
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
        ui.cx.tree.set_accessibility_label(node, self.label.clone());

        let mut response = ui.response_of(id);
        if self.enabled && (response.clicked() || ui.key_pressed_space_when(id)) {
            checked = !checked;
            self.checked.set(checked);
            if let Some(node) = ui.cx.tree.get_mut(node) {
                node.changed = true;
            }
            response.insert_flag(ResponseFlags::CHANGED);
        }
        if let Some(node) = ui.cx.tree.get_mut(node) {
            node.checked = checked;
        }

        let indicator_id = kasumi_core::child_id(id, 0);
        ui.register_node_id(indicator_id, "checkbox indicator");
        let indicator = ui.cx.tree.ensure(indicator_id, NodeKind::Container);
        ui.cx.tree.declare_child(node, indicator);
        ui.cx.tree.set_a11y_hidden(indicator, true);
        ui.set_style_on(
            indicator,
            Style {
                width: Length::Px(18.0),
                height: Length::Px(18.0),
                direction: kasumi_core::FlexDirection::Row,
                align_items: Align::Center,
                justify: Justify::Center,
                background: if checked { theme.accent } else { theme.surface },
                border_color: if checked {
                    theme.accent
                } else {
                    theme.border_strong
                },
                border_width: 1.0,
                radius: Corners::all(5.0),
                ..Style::default()
            },
        );

        let check_id = kasumi_core::child_id(indicator_id, 0);
        ui.register_node_id(check_id, "checkbox check");
        let check = ui.cx.tree.ensure(check_id, NodeKind::Text);
        ui.cx.tree.declare_child(indicator, check);
        let check_style = TextStyle {
            size: 12.0,
            color: theme.on_accent,
            weight: FontWeight::BOLD,
            align: TextAlign::Center,
            ..TextStyle::default()
        };
        ui.set_style_on(
            check,
            Style {
                text: check_style.clone(),
                ..Style::default()
            },
        );
        ui.set_text_on(
            check,
            if checked { "✓" } else { "" }.to_owned(),
            check_style,
        );

        add_label(ui, node, id, 1, &self.label, theme.text, "checkbox label");
        response
    }
}

#[derive(Clone)]
pub struct Radio<T: Clone + PartialEq + 'static> {
    label: String,
    value: T,
    selected: State<T>,
    enabled: bool,
}

impl<T: Clone + PartialEq + 'static> Radio<T> {
    pub fn new(label: impl Into<String>, value: T, selected: &State<T>) -> Self {
        Self {
            label: label.into(),
            value,
            selected: selected.clone(),
            enabled: true,
        }
    }

    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }
}

impl<T: Clone + PartialEq + 'static> Widget for Radio<T> {
    type Output = Response;

    fn ui(self, ui: &mut Ui) -> Response {
        let theme = ui.theme().clone();
        let id = ui.next_id("radio");
        let mut selected = self.selected.with(|value| *value == self.value);
        let node = ui.adopt(
            id,
            NodeKind::Radio,
            Style {
                direction: kasumi_core::FlexDirection::Row,
                align_items: Align::Center,
                gap: 9.0,
                height: Length::Px(26.0),
                min_height: Length::Px(26.0),
                cursor: if self.enabled {
                    CursorIcon::Pointer
                } else {
                    CursorIcon::NotAllowed
                },
                focus_ring: theme.focus_ring,
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
        ui.cx.tree.set_accessibility_label(node, self.label.clone());

        let mut response = ui.response_of(id);
        if self.enabled && (response.clicked() || ui.key_pressed_space_when(id)) && !selected {
            self.selected.set(self.value.clone());
            selected = true;
            if let Some(node) = ui.cx.tree.get_mut(node) {
                node.changed = true;
            }
            response.insert_flag(ResponseFlags::CHANGED);
        }
        if let Some(node) = ui.cx.tree.get_mut(node) {
            node.checked = selected;
        }

        let indicator_id = kasumi_core::child_id(id, 0);
        ui.register_node_id(indicator_id, "radio indicator");
        let indicator = ui.cx.tree.ensure(indicator_id, NodeKind::Container);
        ui.cx.tree.declare_child(node, indicator);
        ui.cx.tree.set_a11y_hidden(indicator, true);
        ui.set_style_on(
            indicator,
            Style {
                width: Length::Px(18.0),
                height: Length::Px(18.0),
                direction: kasumi_core::FlexDirection::Row,
                align_items: Align::Center,
                justify: Justify::Center,
                background: if selected {
                    theme.accent
                } else {
                    theme.surface
                },
                border_color: if selected {
                    theme.accent
                } else {
                    theme.border_strong
                },
                border_width: 1.0,
                radius: Corners::all(9.0),
                ..Style::default()
            },
        );
        if selected {
            let dot_id = kasumi_core::child_id(indicator_id, 0);
            ui.register_node_id(dot_id, "radio dot");
            let dot = ui.cx.tree.ensure(dot_id, NodeKind::Container);
            ui.cx.tree.declare_child(indicator, dot);
            ui.set_style_on(
                dot,
                Style {
                    width: Length::Px(6.0),
                    height: Length::Px(6.0),
                    background: theme.on_accent,
                    radius: Corners::all(3.0),
                    ..Style::default()
                },
            );
        }

        add_label(ui, node, id, 1, &self.label, theme.text, "radio label");
        response
    }
}

fn add_label(
    ui: &mut Ui,
    parent: kasumi_tree::NodeId,
    parent_id: Id,
    index: u32,
    label: &str,
    color: Color,
    origin: &'static str,
) {
    let child_id = kasumi_core::child_id(parent_id, index);
    ui.register_node_id(child_id, origin);
    let child = ui.cx.tree.ensure(child_id, NodeKind::Text);
    ui.cx.tree.declare_child(parent, child);
    let style = TextStyle {
        color,
        size: 14.0,
        ..TextStyle::default()
    };
    ui.set_style_on(
        child,
        Style {
            text: style.clone(),
            align_self: Align::Center,
            padding: Edges::ZERO,
            ..Style::default()
        },
    );
    ui.set_text_on(child, label.to_owned(), style);
}
