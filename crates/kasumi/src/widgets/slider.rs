use kasumi_core::{Color, CursorIcon, Key, Length, Style};
use kasumi_state::State;
use kasumi_tree::{NodeKind, Sense};

use crate::response::{Response, ResponseFlags};
use crate::ui::{Ui, Widget};

/// A horizontal value slider bound to `State<f32>`.
#[derive(Clone)]
pub struct Slider {
    value: State<f32>,
    min: f32,
    max: f32,
    step: f32,
    width: Length,
    height: f32,
    enabled: bool,
    label: Option<String>,
}

impl Slider {
    pub fn new(value: &State<f32>, min: f32, max: f32) -> Self {
        let (min, max) = if min <= max { (min, max) } else { (max, min) };
        Self {
            value: value.clone(),
            min,
            max,
            step: 0.0,
            width: Length::Px(220.0),
            height: 32.0,
            enabled: true,
            label: None,
        }
    }

    pub fn step(mut self, step: f32) -> Self {
        self.step = step.max(0.0);
        self
    }

    pub fn width(mut self, width: impl Into<Length>) -> Self {
        self.width = width.into();
        self
    }

    pub fn height(mut self, height: f32) -> Self {
        self.height = height.max(20.0);
        self
    }

    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }
}

impl Widget for Slider {
    type Output = Response;

    fn ui(self, ui: &mut Ui) -> Response {
        let theme = ui.theme().clone();
        let id = ui.next_id("slider");
        let node = ui.cx.tree.ensure(id, NodeKind::Slider);
        ui.declare_child(ui.node, node);

        let focused = ui.cx.focused == Some(id);
        let mut value = self.value.get().clamp(self.min, self.max);
        let mut response = ui.response_of(id);
        let mut changed = false;
        if self.enabled {
            if response.pressed() {
                let rect = ui.cx.tree.visual_rect_of(id);
                let x = ui.input().pointer.x - rect.left() - 9.0;
                let ratio = (x / (rect.width() - 18.0).max(1.0)).clamp(0.0, 1.0);
                changed = set_value(
                    &self.value,
                    &mut value,
                    self.min + ratio * (self.max - self.min),
                    self.step,
                    self.min,
                    self.max,
                );
            } else if focused {
                let delta = (self.max - self.min)
                    / if self.step > 0.0 {
                        (self.max - self.min) / self.step
                    } else {
                        100.0
                    };
                if ui.key_pressed(&Key::ArrowLeft) || ui.key_pressed(&Key::ArrowDown) {
                    let next = value - delta;
                    changed =
                        set_value(&self.value, &mut value, next, self.step, self.min, self.max);
                } else if ui.key_pressed(&Key::ArrowRight) || ui.key_pressed(&Key::ArrowUp) {
                    let next = value + delta;
                    changed =
                        set_value(&self.value, &mut value, next, self.step, self.min, self.max);
                } else if ui.key_pressed(&Key::Home) {
                    changed = set_value(
                        &self.value,
                        &mut value,
                        self.min,
                        self.step,
                        self.min,
                        self.max,
                    );
                } else if ui.key_pressed(&Key::End) {
                    changed = set_value(
                        &self.value,
                        &mut value,
                        self.max,
                        self.step,
                        self.min,
                        self.max,
                    );
                }
            }
        }

        let style = Style {
            width: self.width,
            height: Length::Px(self.height),
            min_height: Length::Px(self.height),
            background: Color::TRANSPARENT,
            border_color: theme.accent,
            focus_ring: theme.focus_ring,
            cursor: if self.enabled {
                if response.pressed() {
                    CursorIcon::Grabbing
                } else {
                    CursorIcon::Grab
                }
            } else {
                CursorIcon::NotAllowed
            },
            ..Style::default()
        };
        ui.set_style_on(node, style);
        ui.set_sense_on(
            node,
            if self.enabled {
                Sense::click().union(Sense::DRAG)
            } else {
                Sense::NONE
            },
        );
        if let Some(node) = ui.cx.tree.get_mut(node) {
            node.slider_value = value;
            node.slider_min = self.min;
            node.slider_max = self.max;
            node.changed = changed;
        }
        ui.cx.tree.set_accessibility_label(
            node,
            self.label
                .clone()
                .unwrap_or_else(|| format!("Slider {value:.2}")),
        );
        if changed {
            response.insert_flag(ResponseFlags::CHANGED);
        }
        response
    }
}

fn set_value(
    state: &State<f32>,
    value: &mut f32,
    next: f32,
    step: f32,
    min: f32,
    max: f32,
) -> bool {
    let next = if step > 0.0 {
        (min + ((next - min) / step).round() * step).clamp(min, max)
    } else {
        next.clamp(min, max)
    };
    if (next - *value).abs() <= f32::EPSILON {
        return false;
    }
    *value = next;
    state.set(next);
    true
}
