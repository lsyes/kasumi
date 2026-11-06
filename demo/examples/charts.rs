//! `charts` — data visualization using only layout primitives.
//!
//! ```sh
//! cargo run -p kasumi-demo --example charts
//! ```
//!
//! Kasumi has no canvas or path API yet, but a chart does not need one:
//! rectangles, rounded corners and color roles are enough for bars,
//! sparklines, a heatmap and a concentric donut. Everything here is an
//! ordinary container declared in `view`, so it participates in layout,
//! scrolling and theming like any other widget.
//!
//! The data is generated deterministically from a `State<u32>` seed, which also
//! shows that a chart can be driven by state rather than an external timer.

use kasumi::App;
use kasumi::prelude::*;
use kasumi_backend::run;

#[path = "shared/mod.rs"]
mod shared;

use shared::{code, heading, micro, note, options};

/// A deterministic pseudo-random series so the charts stay stable per seed.
fn series(seed: u32, count: usize) -> Vec<f32> {
    (0..count)
        .map(|index| {
            let x = (index as u32)
                .wrapping_mul(73_891)
                .wrapping_add(seed * 9_337);
            let wave = ((index as f32) * 0.42).sin() * 0.28 + 0.5;
            let noise = (x % 1000) as f32 / 1000.0;
            (wave * 0.65 + noise * 0.35).clamp(0.05, 0.98)
        })
        .collect()
}

fn index_of_max(values: &[f32]) -> usize {
    values
        .iter()
        .enumerate()
        .max_by(|a, b| a.1.total_cmp(b.1))
        .map(|(index, _)| index)
        .unwrap_or(0)
}

fn palette(theme: &Theme) -> [Color; 7] {
    [
        theme.colors.primary,
        theme.colors.secondary,
        theme.colors.tertiary,
        theme.colors.error,
        theme.colors.success,
        theme.colors.primary_container,
        theme.colors.tertiary_container,
    ]
}

/// A vertical bar chart with a baseline label row.
fn bar_chart(ui: &mut Ui, values: &[f32], labels: &[&str], colors: &[Color; 7]) {
    let theme = ui.theme().clone();
    ui.column(|ui| {
        ui.width_full().gap(6.0);
        ui.row(|ui| {
            ui.width_full()
                .height(180.0)
                .gap(6.0)
                .align_items(Align::End);
            for (index, value) in values.iter().enumerate() {
                ui.column(|ui| {
                    ui.grow(1.0).basis(0.0).height_full().justify(Justify::End);
                    ui.column(|ui| {
                        ui.width_full()
                            .height_full()
                            .background(theme.colors.surface_container_high.fade(0.45))
                            .radius(theme.shape.extra_small);
                    });
                    ui.column(|ui| {
                        ui.absolute(0.0, (1.0 - value) * 180.0)
                            .width_full()
                            .height(value * 180.0)
                            .radius(theme.shape.extra_small)
                            .background(colors[index % colors.len()]);
                    });
                });
            }
        });
        ui.row(|ui| {
            ui.width_full().gap(6.0);
            for label in labels {
                ui.column(|ui| {
                    ui.grow(1.0).basis(0.0).align_items(Align::Center);
                    micro(ui, *label);
                });
            }
        });
    });
}

/// A sparkline: one thin rect per sample, each as tall as its value.
fn sparkline(ui: &mut Ui, values: &[f32], color: Color, height: f32) {
    let theme = ui.theme().clone();
    ui.row(|ui| {
        ui.width_full()
            .height(height)
            .gap(1.0)
            .align_items(Align::End)
            .background(theme.colors.surface_container_high.fade(0.4))
            .radius(theme.shape.extra_small);
        for value in values {
            ui.column(|ui| {
                ui.grow(1.0)
                    .basis(0.0)
                    .height((value * height).max(2.0))
                    .radius(1.0)
                    .background(color);
            });
        }
    });
}

/// A donut built from concentric filled circles: each layer is a disk whose
/// diameter shrinks by its share of the total, so the visible band for entry i
/// is the ring between disk i and disk i+1. The centre is punched out with a
/// disk in the container color. No path API is required — only `ui.stack`,
/// absolute placement and corner radii.
fn donut(ui: &mut Ui, values: &[f32], colors: &[Color; 7]) {
    let theme = ui.theme().clone();
    let total: f32 = values.iter().sum::<f32>().max(f32::EPSILON);
    let side = 148.0;
    let inner = 52.0; // diameter of the hole
    let band = (side - inner) / 2.0;

    ui.stack(|ui| {
        ui.width(side).height(side);
        // Draw from the outside in. `remaining` is the diameter of the disk
        // still visible after all larger entries have been painted.
        let mut remaining = side;
        for (index, value) in values.iter().enumerate() {
            let next = remaining - (value / total) * band * 2.0;
            let inset = (side - remaining) / 2.0;
            ui.column(|ui| {
                ui.absolute(inset, inset)
                    .width(remaining)
                    .height(remaining)
                    .radius(remaining / 2.0)
                    .background(colors[index % colors.len()]);
            });
            remaining = next.max(inner);
        }
        // Punch the hole.
        let hole_inset = (side - inner) / 2.0;
        ui.column(|ui| {
            ui.absolute(hole_inset, hole_inset)
                .width(inner)
                .height(inner)
                .radius(inner / 2.0)
                .background(theme.colors.surface_container_highest);
        });
    });
}

struct Charts;

impl App for Charts {
    fn view(&mut self, ui: &mut Ui) {
        let theme = ui.theme().clone();
        let seed = ui.state(|| 0u32);
        let columns = ui.state(|| 20.0f32);

        shared::page(
            ui,
            "Charts",
            "rects + color roles · no canvas API required",
            "charts",
            |ui| {
                let values = series(seed.get(), 7);
                let long = series(seed.get() + 50, 48);
                let labels = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
                let colors = palette(&theme);

                ui.row(|ui| {
                    ui.width_full().gap(10.0).align_items(Align::Center);
                    if ui.add(Button::new("Next dataset").primary()).clicked() {
                        seed.update(|value| *value += 1);
                    }
                    if ui.add(Button::new("Reset").subtle()).clicked() {
                        seed.set(0);
                    }
                    ui.spacer();
                    code(ui, format!("seed {} · deterministic series", seed.get()));
                });

                ui.row(|ui| {
                    ui.width_full().gap(16.0).align_items(Align::Stretch);

                    ui.column(|ui| {
                        ui.grow(1.2).basis(0.0);
                        ui.card(
                            Card::elevated()
                                .padding(theme.spacing.lg)
                                .gap(theme.spacing.md),
                            |ui| {
                                heading(ui, "Bar chart", "align_items(Align::End)");
                                note(
                                    ui,
                                    "Bars are plain containers aligned to the bottom of a row; \
                                     each bar takes a different color role.",
                                );
                                bar_chart(ui, &values, &labels, &colors);
                                let peak = index_of_max(&values);
                                code(
                                    ui,
                                    format!("max {} = {:.0}%", labels[peak], values[peak] * 100.0),
                                );
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
                                heading(ui, "Sparklines", "one rect per sample");
                                note(
                                    ui,
                                    "A 48-sample series in a row of 1 px-gapped columns. Since \
                                     each rect is a node, the whole chart is still one draw call.",
                                );
                                sparkline(ui, &long, theme.colors.primary, 60.0);
                                sparkline(
                                    ui,
                                    &series(seed.get() + 7, 48),
                                    theme.colors.tertiary,
                                    60.0,
                                );
                                sparkline(
                                    ui,
                                    &series(seed.get() + 21, 48),
                                    theme.colors.error,
                                    60.0,
                                );
                            },
                        );
                    });
                });

                ui.row(|ui| {
                    ui.width_full().gap(16.0).align_items(Align::Stretch);

                    ui.column(|ui| {
                        ui.grow(1.0).basis(0.0);
                        ui.card(
                            Card::elevated()
                                .padding(theme.spacing.lg)
                                .gap(theme.spacing.md),
                            |ui| {
                                heading(ui, "Heatmap", "Color::lerp");
                                note(
                                    ui,
                                    "A grid of cells colored by interpolating between two theme \
                                     roles. `Color::lerp` is linear in the sRGB values rather \
                                     than in linear light.",
                                );
                                ui.add(
                                    Slider::new(&columns, 4.0, 32.0)
                                        .step(1.0)
                                        .label("Columns")
                                        .width(Length::Percent(100.0)),
                                );
                                let count = columns.get() as usize;
                                ui.grid(count, |ui| {
                                    ui.width_full().gap(2.0);
                                    for cell in 0..count * 8 {
                                        let factor = series(seed.get() + cell as u32, 1)[0];
                                        ui.column(|ui| {
                                            ui.height(18.0).radius(2.0).background(
                                                theme
                                                    .colors
                                                    .surface_container_high
                                                    .lerp(theme.colors.primary, factor),
                                            );
                                        });
                                    }
                                });
                            },
                        );
                    });

                    ui.column(|ui| {
                        ui.grow(1.0).basis(0.0);
                        ui.card(
                            Card::filled()
                                .padding(theme.spacing.lg)
                                .gap(theme.spacing.md),
                            |ui| {
                                heading(ui, "Proportion", "concentric rings");
                                note(
                                    ui,
                                    "A donut without arcs: concentric disks whose diameters \
                                     shrink by each entry's share, with the centre punched out. \
                                     A useful illustration of what `ui.stack` plus absolute \
                                     placement gives you.",
                                );
                                let total: f32 = values.iter().sum();
                                ui.row(|ui| {
                                    ui.width_full().gap(18.0).align_items(Align::Center);
                                    donut(ui, &values, &colors);
                                    ui.column(|ui| {
                                        ui.grow(1.0).gap(4.0);
                                        for (index, label) in labels.iter().enumerate() {
                                            ui.row(|ui| {
                                                ui.width_full().gap(8.0).align_items(Align::Center);
                                                ui.column(|ui| {
                                                    ui.width(10.0)
                                                        .height(10.0)
                                                        .radius(5.0)
                                                        .background(colors[index % 7]);
                                                });
                                                micro(ui, *label);
                                                ui.spacer();
                                                code(
                                                    ui,
                                                    format!(
                                                        "{:.0}%",
                                                        values[index] / total * 100.0
                                                    ),
                                                );
                                            });
                                        }
                                    });
                                });
                            },
                        );
                    });
                });
            },
        );
    }
}

fn main() -> anyhow::Result<()> {
    run(
        shared::QuitOnEscape { app: Charts },
        options("Kasumi — charts", 1120.0, 900.0),
    )
}
