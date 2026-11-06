//! `typography` — the M3 type scale, weights, families, wrapping and fallback.
//!
//! ```sh
//! cargo run -p kasumi-demo --example typography
//! ```
//!
//! Text styling has three levels:
//!
//! - `Label::new(..)` inherits the theme's body style and you override pieces.
//! - `Label::style(role.clone())` applies a complete M3 role (size, line height,
//!   weight and tracking together) and then layers overrides on top.
//! - `TextStyle` is the plain-data struct both end up producing.
//!
//! Wrapping is opt-in for a reason: a label's wrap width is the width of its
//! last measure pass, so a paragraph needs an explicit `width(..)`. Use the
//! width slider below to see the difference between unbounded and constrained
//! text.

use kasumi::App;
use kasumi::core::TextStyle;
use kasumi::prelude::*;
use kasumi_backend::run;

#[path = "shared/mod.rs"]
mod shared;

use shared::{code, heading, micro, note, options};

const PARAGRAPH: &str = "Kasumi shapes text with swash and breaks lines with UAX #14. \
Glyphs are packed into one RGBA atlas, so a paragraph of mixed scripts is still a single \
instanced draw call. A label's wrap width is the width used by its last measure pass, \
which is why paragraphs take an explicit width.";

struct Typography;

impl App for Typography {
    fn view(&mut self, ui: &mut Ui) {
        let theme = ui.theme().clone();
        let paragraph_width = ui.state(|| 520.0f32);
        let weight = ui.state(|| 3.0f32);

        shared::page(
            ui,
            "Typography",
            "15 type roles · weights 100–900 · CJK and emoji fallback",
            "typography",
            |ui| {
                ui.card(
                    Card::elevated()
                        .padding(theme.spacing.lg)
                        .gap(theme.spacing.md),
                    |ui| {
                        heading(ui, "The M3 type scale", "theme.typography");
                    note(
                        ui,
                        "Material 3 defines role pairs of size/line-height with a weight and \
                         tracking token. `Label::style` applies the whole role at once.",
                    );
                    let roles: [(&str, TextStyle, &str); 6] = [
                        (
                            "display large · 57/64 · 400",
                            theme.typography.display_large.clone(),
                            "Immediate",
                        ),
                        (
                            "headline medium · 28/36 · 400",
                            theme.typography.headline_medium.clone(),
                            "Retained core",
                        ),
                        (
                            "title large · 22/28 · 400",
                            theme.typography.title_large.clone(),
                            "A title for content",
                        ),
                        (
                            "body large · 16/24 · 400",
                            theme.typography.body_large.clone(),
                            "Body copy is the workhorse of the scale and keeps a 24 px line box.",
                        ),
                        (
                            "label large · 14/20 · 500",
                            theme.typography.label_large.clone(),
                            "BUTTON LABEL",
                        ),
                        (
                            "body small · 12/16 · 400",
                            theme.typography.body_small.clone(),
                            "Supporting caption text.",
                        ),
                    ];
                    for (name, role, sample) in roles {
                        ui.row(|ui| {
                            ui.width_full().align_items(Align::End).gap(20.0);
                            ui.column(|ui| {
                                ui.width(210.0).flex_shrink(0.0);
                                micro(ui, name);
                            });
                            ui.column(|ui| {
                                ui.grow(1.0);
                                let mut role = role;
                                role.color = theme.colors.on_surface;
                                ui.add(Label::new(sample).style(role));
                            });
                        });
                        ui.divider();
                    }
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
                                heading(ui, "Weights", "FontWeight");
                                let weights = [
                                    ("Thin 100", FontWeight::THIN),
                                    ("Light 300", FontWeight::LIGHT),
                                    ("Regular 400", FontWeight::NORMAL),
                                    ("Medium 500", FontWeight::MEDIUM),
                                    ("Semibold 600", FontWeight::SEMIBOLD),
                                    ("Bold 700", FontWeight::BOLD),
                                    ("Black 900", FontWeight::BLACK),
                                ];
                                let selected =
                                    (weight.get().round() as usize).min(weights.len() - 1);
                                for (index, (label, value)) in weights.into_iter().enumerate() {
                                    ui.row(|ui| {
                                        ui.width_full().align_items(Align::Center).gap(10.0);
                                        ui.column(|ui| {
                                            ui.width(110.0).flex_shrink(0.0);
                                            micro(ui, label);
                                        });
                                        ui.add(
                                            Label::new("Immediate surface")
                                                .font_size(18.0)
                                                .weight(value)
                                                .color(if index == selected {
                                                    theme.colors.primary
                                                } else {
                                                    theme.colors.on_surface
                                                }),
                                        );
                                    });
                                }
                                ui.add(
                                    Slider::new(&weight, 0.0, 6.0)
                                        .step(1.0)
                                        .label("Weight index")
                                        .width(Length::Percent(100.0)),
                                );
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
                                heading(ui, "Families", "FontFamily");
                                for (family, sample) in [
                                    (FontFamily::SansSerif, "Sans serif — the platform default"),
                                    (FontFamily::Serif, "Serif — for long-form reading"),
                                    (FontFamily::Monospace, "Monospace — code and identifiers"),
                                    (
                                        FontFamily::Named("DejaVu Sans".to_owned()),
                                        "Named(\"DejaVu Sans\") — resolved through fontdb",
                                    ),
                                ] {
                                    ui.add(
                                        Label::new(sample)
                                            .font_size(14.0)
                                            .family(family.clone())
                                            .color(theme.colors.on_surface),
                                    );
                                    let name = family.to_string();
                                    micro(ui, format!("family: {name}"));
                                    ui.divider();
                                }
                            },
                        );
                    });
                });

                ui.card(
                    Card::elevated()
                        .padding(theme.spacing.lg)
                        .gap(theme.spacing.md),
                    |ui| {
                        heading(ui, "Wrap width", "Label::width");
                        note(
                            ui,
                            "Drag the slider. A paragraph needs an explicit width because its wrap \
                         width is the width of the last measure pass; `TextWrap::None` opts out.",
                        );
                        ui.add(
                            Slider::new(&paragraph_width, 180.0, 900.0)
                                .step(10.0)
                                .label("Paragraph width")
                                .width(Length::Percent(100.0)),
                        );
                        ui.row(|ui| {
                            ui.width_full().gap(20.0).align_items(Align::Start);
                            ui.column(|ui| {
                                ui.width(paragraph_width.get())
                                    .gap(6.0)
                                    .padding(14.0)
                                    .background(theme.colors.surface_container_low)
                                    .radius(theme.shape.small);
                                micro(ui, "TextWrap::Word (default)");
                                ui.add(
                                    Label::new(PARAGRAPH)
                                        .font_size(13.0)
                                        .color(theme.colors.on_surface),
                                );
                            });
                            ui.column(|ui| {
                                ui.grow(1.0).basis(0.0).gap(6.0);
                                micro(ui, "TextWrap::None");
                                ui.clip();
                                ui.add(
                                    Label::new("Single line, allowed to overflow the box")
                                        .font_size(13.0)
                                        .color(theme.colors.on_surface_variant)
                                        .wrap(TextWrap::None),
                                );
                                code(
                                    ui,
                                    format!("resolved width = {:.0} px", paragraph_width.get()),
                                );
                            });
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
                                heading(ui, "Fallback and scripts", "fontdb · per-cluster");
                            note(
                                ui,
                                "`FontFamily::SansSerif` is resolved per cluster, so one label can \
                                 mix Latin, CJK and emoji without the caller doing anything.",
                            );
                            ui.add(
                                Label::new("即时为面，保留为骨。")
                                    .font_size(26.0)
                                    .color(theme.colors.on_surface),
                            );
                            ui.add(
                                Label::new("中文、日本語、한국어 fall back to Noto CJK.")
                                    .font_size(13.0)
                                    .color(theme.colors.on_surface_variant),
                            );
                            ui.add(
                                Label::new("Emoji fallback: ✈ ● ▲ ✓ — and colour glyphs where the platform has them.")
                                    .font_size(13.0)
                                    .color(theme.colors.on_surface_variant),
                            );
                            ui.add(
                                Label::new("مرحبا — bidi reordering keeps right-to-left runs intact.")
                                    .font_size(15.0)
                                    .color(theme.colors.on_surface),
                            );
                        });
                    });

                    ui.column(|ui| {
                        ui.grow(1.0).basis(0.0);
                        ui.card(
                            Card::elevated()
                                .padding(theme.spacing.lg)
                                .gap(theme.spacing.md),
                            |ui| {
                                heading(ui, "Styling details", "label builder");
                            for line in [
                                "font_size   — pixels; scales the role's line height too",
                                "weight      — OpenType 100..=900",
                                "italic()    — selects an italic or oblique face",
                                "line_height — explicit override in pixels",
                                "align       — Start / Center / End inside the box",
                                "color       — any palette role",
                            ] {
                                code(ui, format!("· {line}"));
                            }
                            ui.divider();
                            ui.add(
                                Label::new("Italic sample: the quick brown fox")
                                    .font_size(15.0)
                                    .italic()
                                    .color(theme.colors.on_surface),
                            );
                            ui.add(
                                Label::new("Line height 32 on body text")
                                    .font_size(15.0)
                                    .line_height(32.0)
                                    .color(theme.colors.primary),
                            );
                            ui.add(
                                Label::new("Centered inside the box")
                                    .font_size(15.0)
                                    .align(TextAlign::Center)
                                    .width(Length::Percent(100.0))
                                    .color(theme.colors.tertiary),
                            );
                        });
                    });
                });

                ui.card(
                    Card::elevated()
                        .padding(theme.spacing.lg)
                        .gap(theme.spacing.md),
                    |ui| {
                        heading(ui, "Measuring before you paint", "ui.texts().measure");
                        note(
                            ui,
                            "`ui.texts()` exposes the engine. Measuring here decides how wide the \
                         chip should be before any node exists, which is how a custom widget \
                         sizes itself to its content.",
                        );
                        let style = theme.typography.label_large.clone();
                        let samples = ["Short", "A somewhat longer label", "中文标签"];
                        ui.wrap_row(|ui| {
                            ui.width_full().gap(8.0);
                            for sample in samples {
                                let size = ui.texts().measure(sample, &style, None);
                                ui.row(|ui| {
                                    ui.height(34.0)
                                        .padding_xy(14.0, 0.0)
                                        .align_items(Align::Center)
                                        .justify(Justify::Center)
                                        .radius(theme.shape.full)
                                        .background(theme.colors.secondary_container);
                                    ui.add(
                                        Label::new(sample)
                                            .font_size(13.0)
                                            .color(theme.colors.on_secondary_container),
                                    );
                                    micro(ui, format!(" {:.0}×{:.0}", size.width, size.height));
                                });
                            }
                        });
                    },
                );
            },
        );
    }
}

fn main() -> anyhow::Result<()> {
    run(
        shared::QuitOnEscape { app: Typography },
        options("Kasumi — typography", 1120.0, 880.0),
    )
}
