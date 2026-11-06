//! `controls` — buttons, switches, sliders, progress, checkboxes and radios.
//!
//! ```sh
//! cargo run -p kasumi-demo --example controls
//! ```
//!
//! All controls follow Material 3 tokens: the five button emphasis levels, the
//! 52×32 switch with its overshoot thumb, 48 dp touch targets on checkbox and
//! radio rows, and a slider whose track fills with a spring.
//!
//! Each block below is independent. Read one, delete the rest.

use kasumi::App;
use kasumi::prelude::*;
use kasumi_backend::run;

#[path = "shared/mod.rs"]
mod shared;

use shared::{code, heading, micro, note, options};

/// The five Material button emphasis levels, side by side.
fn emphasis_row(ui: &mut Ui) {
    ui.wrap_row(|ui| {
        ui.width_full().gap(8.0);
        ui.add(Button::new("Filled").filled());
        ui.add(Button::new("Tonal").tonal());
        ui.add(Button::new("Outlined").outlined());
        ui.add(Button::new("Text").text());
        ui.add(Button::new("Elevated").elevated());
    });
}

struct Controls;

impl App for Controls {
    fn view(&mut self, ui: &mut Ui) {
        let theme = ui.theme().clone();

        let notifications = ui.state(|| true);
        let analytics = ui.state(|| false);
        let plan = ui.state(|| 1usize);
        let volume = ui.state(|| 0.58f32);
        let brightness = ui.state(|| 0.30f32);
        let progress = ui.state(|| 0.64f32);
        let manual = ui.state(|| false);

        shared::page(
            ui,
            "Controls",
            "M3 buttons · switch · slider · progress · checkbox · radio",
            "controls",
            |ui| {
                // ----------------------------------------------------------
                // Buttons
                // ----------------------------------------------------------
                ui.card(
                    Card::elevated()
                        .padding(theme.spacing.lg)
                        .gap(theme.spacing.md),
                    |ui| {
                        heading(ui, "Button emphasis", "ButtonVariant");
                        note(
                            ui,
                            "Five levels, in the order Material uses them: filled for the one \
                             primary action, tonal for secondary, outlined for alternatives, \
                             text for low emphasis and elevated for surfaces over imagery.",
                        );
                        emphasis_row(ui);
                        ui.divider();
                        ui.row(|ui| {
                            ui.width_full().gap(8.0).align_items(Align::Center);
                            ui.add(Button::new("Small").font_size(11.0).padding_xy(10.0, 5.0));
                            ui.add(Button::new("Default"));
                            ui.add(Button::new("Large").font_size(16.0).padding_xy(26.0, 12.0));
                            ui.add(Button::new("Wide").min_width(140.0));
                            ui.spacer();
                            ui.add(Button::new("Disabled filled").filled().enabled(false));
                            ui.add(Button::new("Disabled text").text().enabled(false));
                        });
                        code(
                            ui,
                            "font_size · padding_xy · min_width · full_width · enabled",
                        );
                    },
                );

                ui.row(|ui| {
                    ui.width_full().gap(16.0).align_items(Align::Stretch);

                    // ------------------------------------------------------
                    // Switch / checkbox / radio
                    // ------------------------------------------------------
                    ui.column(|ui| {
                        ui.grow(1.0).basis(0.0);
                        ui.card(
                            Card::elevated()
                                .padding(theme.spacing.lg)
                                .gap(theme.spacing.md),
                            |ui| {
                                heading(ui, "Toggles", "Switch · Checkbox · Radio");
                                note(
                                    ui,
                                    "A switch is for a setting that applies immediately; a \
                                     checkbox is for a choice inside a form. They have \
                                     different semantics even when they look alike.",
                                );
                                ui.add(
                                    Switch::new(&notifications)
                                        .label("Notifications")
                                        .supporting("Push alerts for new comments"),
                                );
                                ui.add(
                                    Switch::new(&analytics)
                                        .label("Share analytics")
                                        .supporting("Anonymous usage counters"),
                                );
                                ui.divider();
                                ui.add(Checkbox::new("Remember this device", &notifications));
                                ui.add(
                                    Checkbox::new("Disabled checkbox", &notifications)
                                        .enabled(false),
                                );
                                ui.divider();
                                micro(ui, "radio group");
                                ui.row(|ui| {
                                    ui.width_full().gap(18.0);
                                    ui.radio("Free", 0usize, &plan);
                                    ui.radio("Team", 1usize, &plan);
                                    ui.radio("Studio", 2usize, &plan);
                                });
                                code(
                                    ui,
                                    format!(
                                        "plan = {}",
                                        ["Free", "Team", "Studio"][plan.get().min(2)]
                                    ),
                                );
                            },
                        );
                    });

                    // ------------------------------------------------------
                    // Slider / progress
                    // ------------------------------------------------------
                    ui.column(|ui| {
                        ui.grow(1.0).basis(0.0);
                        ui.card(
                            Card::elevated()
                                .padding(theme.spacing.lg)
                                .gap(theme.spacing.md),
                            |ui| {
                                heading(ui, "Continuous values", "Slider · ProgressBar");
                                note(
                                    ui,
                                    "A slider writes a `State<f32>` on press, drag and keyboard. \
                                     Arrow keys move by one step when it has focus; Home and \
                                     End jump to the ends.",
                                );

                                ui.row(|ui| {
                                    ui.width_full().align_items(Align::Center);
                                    micro(ui, "volume");
                                    ui.spacer();
                                    code(ui, format!("{:.0}%", volume.get() * 100.0));
                                });
                                ui.add(
                                    Slider::new(&volume, 0.0, 1.0)
                                        .step(0.01)
                                        .label("Volume")
                                        .width(Length::Percent(100.0)),
                                );

                                ui.row(|ui| {
                                    ui.width_full().align_items(Align::Center);
                                    micro(ui, "brightness");
                                    ui.spacer();
                                    code(ui, format!("{:.0}%", brightness.get() * 100.0));
                                });
                                ui.add(
                                    Slider::new(&brightness, 0.0, 1.0)
                                        .label("Brightness")
                                        .width(Length::Percent(100.0)),
                                );

                                ui.divider();

                                ui.row(|ui| {
                                    ui.width_full().align_items(Align::Center);
                                    micro(ui, "build progress");
                                    ui.spacer();
                                    code(ui, format!("{:.0}%", progress.get() * 100.0));
                                });
                                ui.progress(&progress);
                                ui.row(|ui| {
                                    ui.width_full().gap(8.0).align_items(Align::Center);
                                    if ui.add(Button::new("Advance").tonal()).clicked() {
                                        progress.update(|value| {
                                            *value = if *value >= 0.999 {
                                                0.0
                                            } else {
                                                (*value + 0.125).min(1.0)
                                            };
                                        });
                                    }
                                    if ui.add(Button::new("Complete").subtle()).clicked() {
                                        progress.set(1.0);
                                    }
                                    ui.spacer();
                                    ui.add(
                                        Switch::new(&manual)
                                            .label("Auto")
                                            .keyboard_arrows_move_thumb(false),
                                    );
                                });
                                if manual.get() {
                                    // A running animation requests its own frames, so the
                                    // progress advances without any external timer.
                                    progress.update(|value| *value = (*value + 0.004) % 1.0);
                                }
                            },
                        );
                    });
                });

                // ----------------------------------------------------------
                // Disabled matrix
                // ----------------------------------------------------------
                ui.card(
                    Card::outlined()
                        .padding(theme.spacing.lg)
                        .gap(theme.spacing.md),
                    |ui| {
                        heading(ui, "Disabled states", "enabled(false)");
                        note(
                            ui,
                            "Disabled controls keep their geometry but drop the state layer and \
                             use the M3 disabled-content opacity. They are removed from tab \
                             order, so keyboard users never land on them.",
                        );
                        ui.row(|ui| {
                            ui.width_full().gap(8.0);
                            ui.add(Button::new("Filled").filled().enabled(false));
                            ui.add(Button::new("Tonal").tonal().enabled(false));
                            ui.add(Button::new("Outlined").enabled(false));
                            ui.add(Button::new("Text").text().enabled(false));
                        });
                        ui.add(Switch::new(&notifications).label("Switch").enabled(false));
                        ui.add(Slider::new(&volume, 0.0, 1.0).enabled(false).width(320.0));
                        ui.add(
                            ComboBox::new(["One", "Two"], &plan)
                                .enabled(false)
                                .width(220.0),
                        );
                    },
                );
            },
        );
    }
}

fn main() -> anyhow::Result<()> {
    run(
        shared::QuitOnEscape { app: Controls },
        options("Kasumi — controls", 1120.0, 900.0),
    )
}
