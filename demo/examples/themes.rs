//! `themes` — dynamic color, scheme variants, contrast levels and role tokens.
//!
//! ```sh
//! cargo run -p kasumi-demo --example themes
//! ```
//!
//! A `Theme` is plain data. `Theme::from_seed_with` derives a full Material 3
//! scheme from one seed color using the HCT algorithm; `Theme::light()` and
//! `Theme::dark()` are the baseline schemes with no seed influence.
//!
//! Themes are per window, not per application: `Ui::set_theme` swaps the tokens
//! for the current window and schedules a follow-up frame so already-declared
//! widgets pick them up. Two open windows can therefore show different modes at
//! the same time.

use kasumi::App;
use kasumi::prelude::*;
use kasumi_backend::run;

#[path = "shared/mod.rs"]
mod shared;

use shared::{code, heading, micro, note, options};

const SEEDS: [(&str, &str); 7] = [
    ("Baseline", "#6750A4"),
    ("Mint", "#006B5E"),
    ("Iris", "#4F5BD5"),
    ("Ember", "#B3261E"),
    ("Moss", "#2E6B30"),
    ("Amber", "#8B6B00"),
    ("Ocean", "#005AC1"),
];

/// Every role shown as a swatch, with the foreground that belongs on it.
fn role_swatches(theme: &Theme) -> Vec<(&'static str, Color, Color)> {
    vec![
        ("primary", theme.colors.primary, theme.colors.on_primary),
        (
            "primary container",
            theme.colors.primary_container,
            theme.colors.on_primary_container,
        ),
        (
            "secondary",
            theme.colors.secondary,
            theme.colors.on_secondary,
        ),
        (
            "secondary container",
            theme.colors.secondary_container,
            theme.colors.on_secondary_container,
        ),
        ("tertiary", theme.colors.tertiary, theme.colors.on_tertiary),
        (
            "tertiary container",
            theme.colors.tertiary_container,
            theme.colors.on_tertiary_container,
        ),
        ("error", theme.colors.error, theme.colors.on_error),
        (
            "error container",
            theme.colors.error_container,
            theme.colors.on_error_container,
        ),
        ("success", theme.colors.success, theme.colors.on_success),
        (
            "success container",
            theme.colors.success_container,
            theme.colors.on_success_container,
        ),
        ("surface", theme.colors.surface, theme.colors.on_surface),
        (
            "surface container",
            theme.colors.surface_container,
            theme.colors.on_surface,
        ),
        (
            "surface container high",
            theme.colors.surface_container_high,
            theme.colors.on_surface,
        ),
        (
            "surface variant",
            theme.colors.surface_variant,
            theme.colors.on_surface_variant,
        ),
        (
            "inverse surface",
            theme.colors.inverse_surface,
            theme.colors.inverse_on_surface,
        ),
        (
            "outline variant",
            theme.colors.outline_variant,
            theme.colors.on_surface,
        ),
    ]
}

fn hex(color: Color) -> String {
    let [r, g, b, _] = color.to_rgba8();
    format!("#{r:02X}{g:02X}{b:02X}")
}

struct Themes;

impl App for Themes {
    fn view(&mut self, ui: &mut Ui) {
        let dark = ui.state(|| false);
        let seed = ui.state(|| 0usize);
        let variant = ui.state(|| 1usize);
        let contrast = ui.state(|| 1usize);

        // Resolve the wanted theme and install it before declaring anything.
        // `set_theme` also requests the follow-up frame that makes the change
        // visible for nodes declared earlier in this same frame.
        let mode = if dark.get() {
            ThemeMode::Dark
        } else {
            ThemeMode::Light
        };
        let seed_index = seed.get().min(SEEDS.len() - 1);
        let wanted = if seed_index == 0 {
            match mode {
                ThemeMode::Light => Theme::light(),
                ThemeMode::Dark => Theme::dark(),
            }
        } else {
            Theme::from_seed_with(
                Color::hex(SEEDS[seed_index].1),
                mode,
                DynamicColorConfig::new()
                    .variant(SchemeVariant::ALL[variant.get().min(SchemeVariant::ALL.len() - 1)])
                    .contrast(ContrastLevel::ALL[contrast.get().min(ContrastLevel::ALL.len() - 1)]),
            )
        };
        if ui.theme() != &wanted {
            ui.set_theme(wanted);
        }
        let theme = ui.theme().clone();

        shared::page(
            ui,
            "Dynamic color",
            "HCT seeds · scheme variants · contrast levels",
            "themes",
            |ui| {
                ui.card(
                    Card::elevated()
                        .padding(theme.spacing.lg)
                        .gap(theme.spacing.md),
                    |ui| {
                        heading(ui, "Seed, variant, contrast", "Theme::from_seed_with");
                        note(
                            ui,
                            "Every role on this page is derived from one seed by the Material \
                             algorithm: HCT tonal palettes, per-role contrast curves, and the \
                             chosen strategy. Pick a seed to repaint the entire window.",
                        );

                        // Seed swatches.
                        ui.wrap_row(|ui| {
                            ui.width_full().gap(8.0);
                            for (index, (name, value)) in SEEDS.iter().enumerate() {
                                let active = seed_index == index;
                                let mut button =
                                    Button::new(*name).font_size(11.0).padding_xy(12.0, 6.0);
                                button = if active {
                                    button.filled()
                                } else {
                                    button.outlined()
                                };
                                if ui.add(button).clicked() {
                                    seed.set(index);
                                }
                                ui.column(|ui| {
                                    ui.width(14.0)
                                        .height(14.0)
                                        .radius(7.0)
                                        .border(1.0, theme.colors.outline_variant)
                                        .background(Color::hex(*value));
                                });
                            }
                        });

                        ui.divider();

                        // Variant chips.
                        ui.row(|ui| {
                            ui.width_full().gap(14.0).align_items(Align::Center);
                            ui.column(|ui| {
                                ui.width(64.0).flex_shrink(0.0);
                                micro(ui, "variant");
                            });
                            ui.wrap_row(|ui| {
                                ui.gap(6.0);
                                for (index, candidate) in SchemeVariant::ALL.iter().enumerate() {
                                    let active =
                                        variant.get().min(SchemeVariant::ALL.len() - 1) == index;
                                    let mut button = Button::new(candidate.label())
                                        .font_size(10.0)
                                        .padding_xy(9.0, 4.0);
                                    button = if active {
                                        button.tonal()
                                    } else {
                                        button.subtle()
                                    };
                                    if ui.add(button).clicked() {
                                        variant.set(index);
                                    }
                                }
                            });
                        });

                        // Contrast chips.
                        ui.row(|ui| {
                            ui.width_full().gap(14.0).align_items(Align::Center);
                            ui.column(|ui| {
                                ui.width(64.0).flex_shrink(0.0);
                                micro(ui, "contrast");
                            });
                            ui.wrap_row(|ui| {
                                ui.gap(6.0);
                                for (index, candidate) in ContrastLevel::ALL.iter().enumerate() {
                                    let active =
                                        contrast.get().min(ContrastLevel::ALL.len() - 1) == index;
                                    let mut button = Button::new(candidate.label())
                                        .font_size(10.0)
                                        .padding_xy(9.0, 4.0);
                                    button = if active {
                                        button.tonal()
                                    } else {
                                        button.subtle()
                                    };
                                    if ui.add(button).clicked() {
                                        contrast.set(index);
                                    }
                                }
                            });
                        });

                        ui.row(|ui| {
                            ui.width_full().gap(10.0).align_items(Align::Center);
                            ui.add(
                                Switch::new(&dark)
                                    .label("Dark mode")
                                    .supporting("Theme per window · Ui::set_theme"),
                            );
                            ui.spacer();
                            let summary = if seed_index == 0 {
                                "baseline scheme".to_owned()
                            } else {
                                let v = SchemeVariant::ALL
                                    [variant.get().min(SchemeVariant::ALL.len() - 1)];
                                let c = ContrastLevel::ALL
                                    [contrast.get().min(ContrastLevel::ALL.len() - 1)];
                                format!(
                                    "seed {} · {} · {}",
                                    SEEDS[seed_index].1,
                                    v.label(),
                                    c.label()
                                )
                            };
                            code(ui, summary);
                        });
                    },
                );

                ui.card(
                    Card::elevated()
                        .padding(theme.spacing.lg)
                        .gap(theme.spacing.md),
                    |ui| {
                        heading(ui, "Color roles", "theme.colors.*");
                        note(
                            ui,
                            "The full role set that the rest of the framework reads from. \
                             A widget never hard-codes a color; it names a role.",
                        );
                        ui.grid(4, |ui| {
                            ui.width_full().gap(10.0);
                            for (name, fill, text) in role_swatches(&theme) {
                                ui.column(|ui| {
                                    ui.width_full()
                                        .height(64.0)
                                        .padding(8.0)
                                        .justify(Justify::End)
                                        .background(fill)
                                        .radius(theme.shape.small)
                                        .border(1.0, theme.colors.outline_variant.fade(0.4));
                                    ui.add(
                                        Label::new(format!("{name}\n{}", hex(fill)))
                                            .font_size(9.0)
                                            .line_height(12.0)
                                            .color(text)
                                            .family(FontFamily::Monospace)
                                            .wrap(TextWrap::None),
                                    );
                                });
                            }
                        });
                    },
                );

                ui.row(|ui| {
                    ui.width_full().gap(16.0).align_items(Align::Stretch);

                    ui.column(|ui| {
                        ui.grow(1.0).basis(0.0);
                        ui.card(
                            Card::filled()
                                .padding(theme.spacing.lg)
                                .gap(theme.spacing.md),
                            |ui| {
                                heading(ui, "Surfaces by elevation", "surface_at_elevation");
                                note(
                                    ui,
                                    "Elevation is tonal, not shadow-based: the paint layer has no \
                                     blurred shadow primitive yet, so a card changes roles \
                                     instead of casting a shadow.",
                                );
                                for level in 0..=5u8 {
                                    ui.row(|ui| {
                                        ui.width_full()
                                            .height(30.0)
                                            .padding_xy(10.0, 0.0)
                                            .align_items(Align::Center)
                                            .radius(theme.shape.extra_small)
                                            .background(theme.surface_at_elevation(level));
                                        micro(ui, format!("level {level}"));
                                        ui.spacer();
                                        code(ui, hex(theme.surface_at_elevation(level)));
                                    });
                                }
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
                                heading(ui, "Other token scales", "shape · spacing · motion");
                                for (label, value) in [
                                    ("shape.small", theme.shape.small),
                                    ("shape.medium", theme.shape.medium),
                                    ("shape.large", theme.shape.large),
                                    ("shape.full", theme.shape.full),
                                ] {
                                    ui.row(|ui| {
                                        ui.width_full().align_items(Align::Center).gap(10.0);
                                        ui.column(|ui| {
                                            ui.width(38.0)
                                                .height(38.0)
                                                .radius(value)
                                                .background(theme.colors.secondary_container);
                                        });
                                        code(ui, format!("{label:<16} {value:.0}"));
                                    });
                                }
                                ui.divider();
                                for (label, value) in [
                                    ("spacing.xs", theme.spacing.xs),
                                    ("spacing.sm", theme.spacing.sm),
                                    ("spacing.md", theme.spacing.md),
                                    ("spacing.lg", theme.spacing.lg),
                                    ("spacing.xl", theme.spacing.xl),
                                ] {
                                    ui.row(|ui| {
                                        ui.width_full().align_items(Align::Center).gap(8.0);
                                        micro(ui, label);
                                        ui.spacer();
                                        ui.column(|ui| {
                                            ui.width(value * 3.0)
                                                .height(10.0)
                                                .radius(5.0)
                                                .background(theme.colors.primary);
                                        });
                                        code(ui, format!("{value:.0}"));
                                    });
                                }
                            },
                        );
                    });
                });
            },
        );
    }
}

fn main() -> anyhow::Result<()> {
    run(Themes, options("Kasumi — dynamic color", 1120.0, 900.0))
}
