//! `custom_widget` — implementing `Widget` and composing containers.
//!
//! ```sh
//! cargo run -p kasumi-demo --example custom_widget
//! ```
//!
//! `Widget` is a two-method trait: an associated `Output` and a `ui(self, ui)`
//! method. That is the *entire* extension point. Every built-in control is an
//! ordinary type implementing it, so a custom widget has the same standing as a
//! `Button`.
//!
//! Three patterns are shown below:
//!
//! - A widget that returns a `Response` by declaring a clickable container with
//!   `ui.clickable()`. The caller keeps full layout control and still gets
//!   `hovered() / pressed() / clicked() / focused()`.
//! - A widget that returns a *custom* output type (`Rating` returns the chosen
//!   value), which is useful when a control is more than a click.
//! - A widget built purely from other widgets, with no retained state of its
//!   own (`MetricCard`), because `State<T>` handles persistence.

use kasumi::App;
use kasumi::prelude::*;
use kasumi_backend::run;

#[path = "shared/mod.rs"]
mod shared;

use shared::{code, heading, micro, note, options};

/// A selectable chip. `ui.clickable()` at the start of the closure marks the
/// container as a click target; the `Response` returned by `ui.row` then
/// carries the interaction flags from the previous frame.
struct Chip {
    label: String,
    selected: bool,
}

impl Widget for Chip {
    type Output = Response;

    fn ui(self, ui: &mut Ui) -> Response {
        let theme = ui.theme().clone();
        ui.row(|ui| {
            ui.clickable();
            ui.height(34.0)
                .padding_xy(14.0, 0.0)
                .align_items(Align::Center)
                .justify(Justify::Center)
                .radius(theme.shape.full)
                .border(1.0, theme.colors.outline_variant)
                .background(if self.selected {
                    theme.colors.secondary_container
                } else {
                    Color::TRANSPARENT
                });
            ui.add(
                Label::new(self.label)
                    .font_size(12.0)
                    .color(if self.selected {
                        theme.colors.on_secondary_container
                    } else {
                        theme.colors.on_surface_variant
                    })
                    .wrap(TextWrap::None),
            );
        })
    }
}

/// A row of five stars. The output is not a `Response`: the widget reports the
/// value it is showing, leaving the click handling to the caller.
struct Rating {
    value: usize,
    max: usize,
}

impl Widget for Rating {
    type Output = (usize, Option<usize>);

    fn ui(self, ui: &mut Ui) -> (usize, Option<usize>) {
        let theme = ui.theme().clone();
        let mut clicked = None;
        ui.row(|ui| {
            ui.gap(4.0);
            for index in 1..=self.max {
                let star = ui.row(|ui| {
                    ui.clickable();
                    ui.width(38.0)
                        .height(38.0)
                        .align_items(Align::Center)
                        .justify(Justify::Center)
                        .radius(theme.shape.extra_small)
                        .background(if index <= self.value {
                            theme.colors.primary_container
                        } else {
                            Color::TRANSPARENT
                        });
                    ui.add(
                        Label::new(if index <= self.value { "★" } else { "☆" })
                            .font_size(20.0)
                            .color(if index <= self.value {
                                theme.colors.primary
                            } else {
                                theme.colors.outline
                            }),
                    );
                });
                if star.clicked() {
                    clicked = Some(index);
                }
                ui.tooltip(&star, format!("Rate {index} of {}", self.max));
            }
        });
        (self.value, clicked)
    }
}

/// A card built entirely from other widgets, with no `State` of its own.
struct MetricCard {
    label: String,
    value: String,
    detail: String,
    fraction: f32,
}

impl Widget for MetricCard {
    type Output = ();

    fn ui(self, ui: &mut Ui) {
        let theme = ui.theme().clone();
        ui.card(
            Card::elevated()
                .padding(theme.spacing.lg)
                .gap(theme.spacing.md),
            |ui| {
                ui.row(|ui| {
                    ui.width_full().align_items(Align::End);
                    ui.column(|ui| {
                        ui.gap(2.0);
                        micro(ui, &self.label);
                        ui.add(
                            Label::new(self.value)
                                .font_size(28.0)
                                .color(theme.colors.on_surface)
                                .semibold(),
                        );
                    });
                    ui.spacer();
                    micro(ui, &self.detail);
                });
                ui.column(|ui| {
                    ui.width_full()
                        .height(8.0)
                        .radius(4.0)
                        .background(theme.colors.surface_container_high);
                    ui.row(|ui| {
                        ui.width(Length::Percent(self.fraction.clamp(0.0, 1.0) * 100.0))
                            .height_full()
                            .radius(4.0)
                            .background(theme.colors.primary);
                    });
                });
            },
        );
    }
}

struct CustomWidget;

impl App for CustomWidget {
    fn view(&mut self, ui: &mut Ui) {
        let theme = ui.theme().clone();
        let filter = ui.state(|| 0usize);
        let rating = ui.state(|| 3usize);
        let last_action = ui.state(|| "nothing yet".to_owned());

        shared::page(
            ui,
            "Custom widgets",
            "impl Widget · clickable containers · custom outputs",
            "custom_widget",
            |ui| {
                ui.card(
                    Card::elevated()
                        .padding(theme.spacing.lg)
                        .gap(theme.spacing.md),
                    |ui| {
                        heading(ui, "The Widget trait", "impl Widget for MyControl");
                        note(
                            ui,
                            "`ui.add(widget)` is the builder counterpart to `ui.button(..)`: it \
                             calls `widget.ui(self)` and returns whatever `Output` the trait \
                             declares. There is no registry and no downcast — the type is known \
                             at the call site.",
                        );
                        ui.column(|ui| {
                            ui.width_full()
                                .padding(12.0)
                                .background(theme.colors.background)
                                .radius(theme.shape.small);
                            code(
                                ui,
                                "impl Widget for Chip {\n    type Output = Response;\n    fn ui(self, ui: &mut Ui) -> Response { ... }\n}",
                            );
                        });
                    },
                );

                ui.row(|ui| {
                    ui.width_full().gap(16.0).align_items(Align::Stretch);

                    ui.column(|ui| {
                        ui.grow(1.0).basis(0.0);
                        ui.card(
                            Card::elevated()
                                .padding(theme.spacing.lg)
                                .gap(theme.spacing.md),
                            |ui| {
                                heading(ui, "Selectable chips", "ui.clickable()");
                                note(
                                    ui,
                                    "Each chip is a `ui.row` with `clickable()` called first. \
                                     The container gets a pointer cursor, a focus ring and a state \
                                     layer, while the caller keeps full layout control.",
                                );
                                ui.wrap_row(|ui| {
                                    ui.width_full().gap(8.0);
                                    for (index, label) in
                                        ["All", "Open", "Closed", "Draft", "Archived"]
                                            .iter()
                                            .enumerate()
                                    {
                                        let chip = ui.add(Chip {
                                            label: (*label).to_owned(),
                                            selected: filter.get() == index,
                                        });
                                        if chip.clicked() {
                                            filter.set(index);
                                            last_action.set(format!("chip: {label}"));
                                        }
                                        ui.tooltip(
                                            &chip,
                                            format!(
                                                "hovered={} pressed={} focused={}",
                                                chip.hovered(),
                                                chip.pressed(),
                                                chip.focused()
                                            ),
                                        );
                                    }
                                });
                                code(ui, format!("selected = {}", filter.get()));
                            },
                        );
                    });

                    ui.column(|ui| {
                        ui.grow(1.0).basis(0.0);
                        ui.card(
                            Card::outlined()
                                .padding(theme.spacing.lg)
                                .gap(theme.spacing.md),
                            |ui| {
                                heading(
                                    ui,
                                    "A custom output",
                                    "Widget::Output = (usize, Option<usize>)",
                                );
                                note(
                                    ui,
                                    "A widget can return anything. `Rating` returns its current \
                                     value and which star was clicked, so the caller decides \
                                     whether a star is a command or a value.",
                                );
                                let (value, clicked) = ui.add(Rating {
                                    value: rating.get(),
                                    max: 5,
                                });
                                if let Some(next) = clicked {
                                    rating.set(next);
                                    last_action.set(format!("rating: {next}"));
                                }
                                ui.row(|ui| {
                                    ui.width_full().gap(8.0).align_items(Align::Center);
                                    code(ui, format!("value = {value}"));
                                    ui.spacer();
                                    if ui.add(Button::new("Reset").subtle()).clicked() {
                                        rating.set(0);
                                    }
                                });
                            },
                        );
                    });
                });

                ui.card(
                    Card::filled()
                        .padding(theme.spacing.lg)
                        .gap(theme.spacing.md),
                    |ui| {
                        heading(ui, "Composing widgets", "no state of its own");
                        note(
                            ui,
                            "`MetricCard` returns `()` and is built entirely from `Card`, `Label` \
                             and two containers. Persistence belongs to `State<T>`, not to a \
                             widget object, so most custom widgets are just functions with a \
                             nicer call site.",
                        );
                        ui.row(|ui| {
                            ui.width_full().gap(12.0);
                            ui.add(MetricCard {
                                label: "FRAMES".to_owned(),
                                value: ui.frame().to_string(),
                                detail: "rendered".to_owned(),
                                fraction: (ui.frame() % 120) as f32 / 120.0,
                            });
                            ui.add(MetricCard {
                                label: "RATING".to_owned(),
                                value: format!("{}/5", rating.get()),
                                detail: "current".to_owned(),
                                fraction: rating.get() as f32 / 5.0,
                            });
                            ui.add(MetricCard {
                                label: "FILTER".to_owned(),
                                value: filter.get().to_string(),
                                detail: "index".to_owned(),
                                fraction: filter.get() as f32 / 4.0,
                            });
                        });
                        code(ui, format!("last action: {}", last_action.get()));
                    },
                );
            },
        );
    }
}

fn main() -> anyhow::Result<()> {
    run(
        shared::QuitOnEscape { app: CustomWidget },
        options("Kasumi — custom widgets", 1120.0, 880.0),
    )
}
