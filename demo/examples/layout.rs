//! `layout` — flex, grid, stack, wrap and absolute positioning.
//!
//! ```sh
//! cargo run -p kasumi-demo --example layout
//! ```
//!
//! Kasumi exposes CSS flexbox and grid through taffy. You rarely write a
//! `Style` by hand: the container helpers (`ui.row`, `ui.column`, `ui.grid`,
//! `ui.stack`, `ui.wrap_row`) pick the algorithm, and the chainable setters on
//! `Ui` write into the style of the *current* container.
//!
//! The mental model to hold on to: calling `ui.width(..)` inside a container
//! closure styles that container, not its parent and not the next sibling.
//!
//! The sliders at the bottom change live geometry. Layout is incremental, so
//! only the affected container re-measures.

use kasumi::App;
use kasumi::prelude::*;
use kasumi_backend::run;

#[path = "shared/mod.rs"]
mod shared;

use shared::{code, heading, micro, note, options};

struct Layout;

impl App for Layout {
    fn view(&mut self, ui: &mut Ui) {
        let theme = ui.theme().clone();
        let gap = ui.state(|| 12.0f32);
        let grow_left = ui.state(|| 1.0f32);
        let wrap = ui.state(|| true);

        shared::page(
            ui,
            "Flex, grid and stacking",
            "ui.row · ui.column · ui.grid · ui.stack · ui.wrap_row",
            "layout",
            |ui| {
                // ---------------------------------------------------------------
                // Justify
                // ---------------------------------------------------------------
                ui.card(
                    Card::elevated()
                        .padding(theme.spacing.lg)
                        .gap(theme.spacing.md),
                    |ui| {
                        heading(ui, "Main-axis distribution", "Justify");
                        note(
                            ui,
                            "The same three 52 px boxes under each `Justify` value. \
                             Free space is distributed by the container, not the children.",
                        );
                        for justify in [
                            Justify::Start,
                            Justify::Center,
                            Justify::End,
                            Justify::SpaceBetween,
                            Justify::SpaceAround,
                            Justify::SpaceEvenly,
                        ] {
                            ui.row(|ui| {
                                ui.width_full()
                                    .height(48.0)
                                    .padding_xy(10.0, 0.0)
                                    .justify(justify)
                                    .background(theme.colors.background)
                                    .radius(theme.shape.small);
                                for index in 0..3 {
                                    ui.column(|ui| {
                                        ui.width(52.0)
                                            .height(24.0)
                                            .radius(theme.shape.extra_small)
                                            .background(if index == 0 {
                                                theme.colors.primary
                                            } else {
                                                theme.colors.surface_container_high
                                            });
                                    });
                                }
                            });
                        }
                    },
                );

                // ---------------------------------------------------------------
                // Align + grow
                // ---------------------------------------------------------------
                ui.row(|ui| {
                    ui.width_full().gap(16.0).align_items(Align::Stretch);

                    ui.column(|ui| {
                        ui.grow(1.0).basis(0.0);
                        ui.card(
                            Card::elevated()
                                .padding(theme.spacing.lg)
                                .gap(theme.spacing.md),
                            |ui| {
                                heading(ui, "Cross-axis alignment", "Align");
                                for align in [Align::Start, Align::Center, Align::End] {
                                    ui.row(|ui| {
                                        ui.width_full()
                                            .height(44.0)
                                            .padding_xy(10.0, 0.0)
                                            .align_items(align)
                                            .background(theme.colors.background)
                                            .radius(theme.shape.extra_small);
                                        ui.column(|ui| {
                                            ui.width(78.0)
                                                .height(16.0)
                                                .radius(theme.shape.extra_small)
                                                .background(theme.colors.tertiary);
                                        });
                                        ui.column(|ui| {
                                            ui.width(78.0)
                                                .height(30.0)
                                                .radius(theme.shape.extra_small)
                                                .background(theme.colors.tertiary_container);
                                        });
                                    });
                                }
                            },
                        );
                    });

                    ui.column(|ui| {
                        ui.grow(1.0).basis(0.0);
                        ui.card(
                            Card::elevated()
                                .padding(theme.spacing.lg)
                                .gap(theme.spacing.md),
                            |ui| {
                                heading(ui, "Grow ratios", "flex_grow");
                                note(
                                    ui,
                                    "`grow(n).basis(0.0)` splits free space strictly by ratio. \
                                     Without `basis(0.0)` the content width is added first.",
                                );
                                let left = grow_left.get();
                                ui.row(|ui| {
                                    ui.width_full().height(40.0).gap(6.0);
                                    ui.column(|ui| {
                                        ui.grow(left)
                                            .basis(0.0)
                                            .height_full()
                                            .radius(theme.shape.extra_small)
                                            .background(theme.colors.primary);
                                    });
                                    ui.column(|ui| {
                                        ui.grow(1.0)
                                            .basis(0.0)
                                            .height_full()
                                            .radius(theme.shape.extra_small)
                                            .background(theme.colors.secondary_container);
                                    });
                                });
                                ui.add(
                                    Slider::new(&grow_left, 0.2, 4.0)
                                        .step(0.1)
                                        .label("Left grow")
                                        .width(Length::Percent(100.0)),
                                );
                                code(ui, format!("grow {:.1} : 1", left));
                            },
                        );
                    });
                });

                // ---------------------------------------------------------------
                // Grid
                // ---------------------------------------------------------------
                ui.card(
                    Card::elevated()
                        .padding(theme.spacing.lg)
                        .gap(theme.spacing.md),
                    |ui| {
                        heading(ui, "Grid tracks", "ui.grid_with");
                        note(
                            ui,
                            "`ui.grid(n, ..)` creates n equal fractions. `ui.grid_with` takes \
                             explicit tracks, so fixed gutters and mixed units are possible.",
                        );
                        ui.grid_with(
                            [
                                GridTrack::px(120.0),
                                GridTrack::fr(1.0),
                                GridTrack::percent(20.0),
                                GridTrack::fr(2.0),
                            ],
                            |ui| {
                                ui.width_full().gap(8.0);
                                for (index, label) in
                                    ["120px", "1fr", "20%", "2fr"].iter().enumerate()
                                {
                                    ui.column(|ui| {
                                        ui.height(38.0)
                                            .align_items(Align::Center)
                                            .justify(Justify::Center)
                                            .radius(theme.shape.extra_small)
                                            .background(if index % 2 == 0 {
                                                theme.colors.surface_container_high
                                            } else {
                                                theme.colors.surface_container
                                            });
                                        code(ui, *label);
                                    });
                                }
                            },
                        );
                        ui.grid(3, |ui| {
                            ui.width_full().gap(8.0);
                            ui.column(|ui| {
                                ui.grid_span(2, 1)
                                    .height(38.0)
                                    .radius(theme.shape.extra_small)
                                    .background(theme.colors.primary_container);
                                ui.row(|ui| {
                                    ui.size_full()
                                        .align_items(Align::Center)
                                        .justify(Justify::Center);
                                    code(ui, "grid_span(2, 1)");
                                });
                            });
                            ui.column(|ui| {
                                ui.height(38.0)
                                    .radius(theme.shape.extra_small)
                                    .background(theme.colors.tertiary_container);
                                ui.row(|ui| {
                                    ui.size_full()
                                        .align_items(Align::Center)
                                        .justify(Justify::Center);
                                    code(ui, "1 column");
                                });
                            });
                        });
                    },
                );

                // ---------------------------------------------------------------
                // Stack + wrap
                // ---------------------------------------------------------------
                ui.row(|ui| {
                    ui.width_full().gap(16.0).align_items(Align::Stretch);

                    ui.column(|ui| {
                        ui.grow(1.0).basis(0.0);
                        ui.card(
                            Card::elevated()
                                .padding(theme.spacing.lg)
                                .gap(theme.spacing.md),
                            |ui| {
                                heading(ui, "Stack", "ui.stack");
                                note(
                                    ui,
                                    "Every direct child of a stack shares one grid cell. \
                                     Use margin or absolute insets to place them.",
                                );
                                ui.stack(|ui| {
                                    ui.width_full().height(140.0);
                                    ui.column(|ui| {
                                        ui.size_full()
                                            .background(theme.colors.surface_container_high)
                                            .radius(theme.shape.medium);
                                    });
                                    ui.column(|ui| {
                                        ui.margin(24.0)
                                            .background(theme.colors.primary_container)
                                            .radius(theme.shape.small);
                                    });
                                    ui.column(|ui| {
                                        ui.absolute(40.0, 92.0)
                                            .width(200.0)
                                            .height(28.0)
                                            .align_items(Align::Center)
                                            .justify(Justify::Center)
                                            .radius(theme.shape.full)
                                            .background(theme.colors.primary);
                                        code(ui, "absolute(40, 92)");
                                    });
                                });
                            },
                        );
                    });

                    ui.column(|ui| {
                        ui.grow(1.0).basis(0.0);
                        ui.card(
                            Card::elevated()
                                .padding(theme.spacing.lg)
                                .gap(theme.spacing.md),
                            |ui| {
                                heading(ui, "Wrapping", "ui.wrap_row");
                                note(
                                    ui,
                                    "A wrapped row breaks at the container width. Toggle it to \
                                     compare against a clipped single line.",
                                );
                                ui.add(
                                    Switch::new(&wrap)
                                        .label("Wrap items")
                                        .supporting("ui.wrap(true)"),
                                );
                                ui.wrap_row(|ui| {
                                    ui.width_full().gap(6.0);
                                    ui.wrap(wrap.get());
                                    for index in 0..12 {
                                        ui.column(|ui| {
                                            ui.width(72.0 + (index % 3) as f32 * 18.0)
                                                .height(28.0)
                                                .align_items(Align::Center)
                                                .justify(Justify::Center)
                                                .radius(theme.shape.extra_small)
                                                .background(theme.colors.secondary_container);
                                            code(ui, format!("#{index}"));
                                        });
                                    }
                                });
                            },
                        );
                    });
                });

                // ---------------------------------------------------------------
                // Live gap
                // ---------------------------------------------------------------
                ui.card(
                    Card::elevated()
                        .padding(theme.spacing.lg)
                        .gap(theme.spacing.md),
                    |ui| {
                        heading(ui, "Live gap", "Style::gap");
                        note(
                            ui,
                            "Dragging the slider changes `gap` on eight children. Only the \
                             affected container re-measures; the rest of the page stays clean.",
                        );
                        ui.row(|ui| {
                            ui.width_full().height(44.0).gap(gap.get());
                            for index in 0..8 {
                                ui.column(|ui| {
                                    ui.grow(1.0)
                                        .basis(0.0)
                                        .height_full()
                                        .radius(theme.shape.extra_small)
                                        .background(if index % 2 == 0 {
                                            theme.colors.primary
                                        } else {
                                            theme.colors.primary_container
                                        });
                                });
                            }
                        });
                        ui.add(
                            Slider::new(&gap, 0.0, 32.0)
                                .step(1.0)
                                .label("Gap")
                                .width(Length::Percent(100.0)),
                        );
                        micro(ui, format!("gap = {:.0} px", gap.get()));
                    },
                );
            },
        );
    }
}

fn main() -> anyhow::Result<()> {
    run(
        shared::QuitOnEscape { app: Layout },
        options("Kasumi — layout", 1120.0, 860.0),
    )
}
