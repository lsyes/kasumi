use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use kasumi::App;
use kasumi::prelude::*;
use kasumi_backend::{WindowOptions, run};

struct Demo;

#[derive(Clone)]
struct Handles {
    dark: State<bool>,
    page: State<u8>,
    count: State<u32>,
    rows: State<Vec<u32>>,
    next_row: State<u32>,
    target: State<f32>,
    name: State<String>,
    greeting: State<String>,
    notifications: State<bool>,
    plan: State<usize>,
    combo: State<usize>,
    volume: State<f32>,
    progress: State<f32>,
    dialog_open: State<bool>,
    project_name: State<String>,
}

impl App for Demo {
    fn view(&mut self, ui: &mut Ui) {
        view(ui);
    }
}

fn view(ui: &mut Ui) {
    if !ui.window().is_primary() {
        secondary_window(ui);
        return;
    }
    let dark = ui.state(|| std::env::var_os("KASUMI_DEMO_DARK").is_some());
    let target_mode = if dark.get() {
        ThemeMode::Dark
    } else {
        ThemeMode::Light
    };
    if ui.theme().mode != target_mode {
        ui.set_theme(match target_mode {
            ThemeMode::Light => Theme::light(),
            ThemeMode::Dark => Theme::dark(),
        });
    }
    let theme = ui.theme().clone();
    let initial_page = std::env::var("KASUMI_DEMO_PAGE")
        .ok()
        .and_then(|value| value.parse::<u8>().ok())
        .unwrap_or(0);
    let handles = Handles {
        dark,
        page: ui.state(move || initial_page.min(6)),
        count: ui.state(|| 0u32),
        rows: ui.state(|| vec![1u32, 2, 3]),
        next_row: ui.state(|| 4u32),
        target: ui.state(move || {
            std::env::var("KASUMI_DEMO_TARGET")
                .ok()
                .and_then(|value| value.parse::<f32>().ok())
                .unwrap_or(0.35)
        }),
        name: ui.state(|| "Kasumi".to_owned()),
        greeting: ui.state(String::new),
        notifications: ui.state(|| true),
        plan: ui.state(|| 0usize),
        combo: ui.state(|| 0usize),
        volume: ui.state(|| 0.58),
        progress: ui.state(|| 0.64),
        dialog_open: ui.state(|| false),
        project_name: ui.state(|| "Kasumi".to_owned()),
    };

    ui.size_full().background(theme.colors.background);
    ui.row(|ui| {
        ui.size_full().align_items(Align::Stretch);
        sidebar(ui, &theme, &handles);

        ui.column(|ui| {
            ui.grow(1.0)
                .height_full()
                .min_width(0.0)
                .background(theme.colors.background);

            ui.row(|ui| {
                ui.width_full()
                    .padding_edges(Edges {
                        left: 32.0,
                        right: 32.0,
                        top: 28.0,
                        bottom: 16.0,
                    })
                    .background(theme.colors.background);
                header(ui, &theme);
            });
            ui.scroll(ScrollView::vertical().grow(1.0).smooth(true), |ui| {
                ui.width_full()
                    .padding_edges(Edges {
                        left: 32.0,
                        right: 32.0,
                        top: 4.0,
                        bottom: 18.0,
                    })
                    .gap(18.0);
                match handles.page.get() {
                    0 => ui.keyed("overview", |ui| overview(ui, &theme, &handles)),
                    1 => ui.keyed("type", |ui| typography(ui, &theme)),
                    2 => ui.keyed("layout", |ui| layout_page(ui, &theme)),
                    3 => ui.keyed("form", |ui| form_page(ui, &theme, &handles)),
                    4 => ui.keyed("controls", |ui| controls_page(ui, &theme, &handles)),
                    5 => ui.keyed("surfaces", |ui| surfaces_page(ui, &theme, &handles)),
                    _ => ui.keyed("async", |ui| async_page(ui, &theme)),
                }
            });
            ui.row(|ui| {
                ui.width_full()
                    .padding_edges(Edges {
                        left: 32.0,
                        right: 32.0,
                        top: 10.0,
                        bottom: 16.0,
                    })
                    .align_items(Align::Center)
                    .border(0.0, Color::TRANSPARENT)
                    .background(theme.colors.background);
                ui.add(
                    Label::new("Tab cycles focus · Enter activates · Esc clears focus")
                        .font_size(11.0)
                        .color(theme.colors.outline)
                        .family(FontFamily::Monospace),
                );
                ui.spacer();
                ui.add(
                    Label::new("render on demand")
                        .font_size(11.0)
                        .color(theme.colors.outline)
                        .family(FontFamily::Monospace),
                );
            });
        });
    });
}

fn page_keys() -> [(&'static str, &'static str); 7] {
    [
        ("overview", "Overview"),
        ("typography", "Typography"),
        ("layout", "Layout"),
        ("form", "Form"),
        ("controls", "Controls"),
        ("surfaces", "Surfaces"),
        ("async", "Async"),
    ]
}

fn page_index(key: &str) -> Option<u8> {
    page_keys()
        .iter()
        .position(|(candidate, _)| *candidate == key)
        .map(|index| index as u8)
}

fn sidebar(ui: &mut Ui, theme: &Theme, handles: &Handles) {
    let current = page_keys()
        .get(handles.page.get() as usize)
        .map(|(key, _)| *key)
        .unwrap_or("overview");
    let navigation = NavigationDrawer::new()
        .header(NavHeader::new("kasumi").subtitle(format!("0.2.0 · {}", kasumi_backend::RENDERER)))
        .item(NavItem::new("overview", "Overview").icon("◉"))
        .section("Surface", |section| {
            section
                .item(NavItem::new("typography", "Typography").icon("Aa"))
                .item(NavItem::new("layout", "Layout").icon("▤"))
        })
        .section("Components", |section| {
            section
                .item(NavItem::new("form", "Form").icon("✎"))
                .item(NavItem::new("controls", "Controls").icon("⚙"))
                .item(NavItem::new("surfaces", "Surfaces").icon("▢"))
                .item(NavItem::new("async", "Async").icon("⏳"))
        })
        .footer(
            NavItem::new(
                "theme",
                if handles.dark.get() {
                    "Light theme"
                } else {
                    "Dark theme"
                },
            )
            .icon("◐"),
        )
        .selected(current);
    let response = ui.navigation(&navigation);
    if let Some(key) = response.selected {
        if key == "theme" {
            handles.dark.update(|dark| *dark = !*dark);
        } else if let Some(index) = page_index(&key) {
            handles.page.set(index);
        }
    }
    let _ = theme;
}

fn header(ui: &mut Ui, theme: &Theme) {
    ui.row(|ui| {
        ui.width_full().align_items(Align::End).gap(16.0);
        ui.column(|ui| {
            ui.gap(4.0);
            let mut kicker = theme.typography.label_medium.clone();
            kicker.color = theme.colors.primary;
            kicker.family = FontFamily::Monospace;
            ui.add(Label::new("IMMEDIATE SURFACE / RETAINED CORE").style(kicker));
            let mut title = theme.typography.headline_medium.clone();
            title.color = theme.colors.on_surface;
            ui.add(Label::new("A desktop UI skeleton with no handles in sight").style(title));
        });
        ui.spacer();
        ui.add(
            Label::new("view(ui) rebuilds the description;\nthe tree, layout and state persist.")
                .font_size(11.0)
                .color(theme.colors.outline)
                .align(TextAlign::End),
        );
    });
}

fn overview(ui: &mut Ui, theme: &Theme, handles: &Handles) {
    ui.column(|ui| {
        ui.width_full().gap(18.0);
        ui.row(|ui| {
            ui.width_full().gap(14.0);
            stat(ui, theme, "SURFACE", "immediate", "plain fn view(ui)");
            stat(ui, theme, "TREE", "retained", "generational arena");
            stat(ui, theme, "TEXT", "shaped", "kasumi-text + swash");
        });

        ui.row(|ui| {
            ui.width_full().align_items(Align::Stretch).gap(18.0);
            ui.column(|ui| {
                ui.grow(1.55).basis(0.0).gap(18.0);
                counter_card(ui, theme, handles);
                rows_card(ui, theme, handles);
            });
            ui.column(|ui| {
                ui.grow(1.0).basis(0.0).gap(18.0);
                animation_card(ui, theme, handles);
                note_card(ui, theme);
            });
        });
    });
}

fn stat(ui: &mut Ui, theme: &Theme, kicker: &str, value: &str, detail: &str) {
    ui.column(|ui| {
        ui.grow(1.0)
            .basis(0.0)
            .padding(16.0)
            .gap(5.0)
            .background(theme.colors.surface_container_low)
            .radius(theme.shape.medium)
            .border(1.0, theme.colors.outline_variant);
        ui.add(
            Label::new(kicker)
                .font_size(9.0)
                .color(theme.colors.outline)
                .family(FontFamily::Monospace),
        );
        ui.add(
            Label::new(value)
                .font_size(20.0)
                .color(theme.colors.on_surface)
                .weight(FontWeight::SEMIBOLD),
        );
        ui.add(
            Label::new(detail)
                .font_size(11.0)
                .color(theme.colors.on_surface_variant),
        );
    });
}

fn card(ui: &mut Ui, theme: &Theme, title: &str, kicker: &str, body: impl FnOnce(&mut Ui)) {
    ui.card(
        Card::elevated()
            .padding(theme.spacing.lg)
            .gap(theme.spacing.md),
        |ui| {
            ui.row(|ui| {
                ui.width_full().align_items(Align::Center).gap(8.0);
                let mut title_style = theme.typography.title_medium.clone();
                title_style.color = theme.colors.on_surface;
                ui.add(Label::new(title).style(title_style).wrap(TextWrap::None));
                ui.spacer();
                ui.add(
                    Label::new(kicker)
                        .font_size(9.0)
                        .color(theme.colors.outline)
                        .family(FontFamily::Monospace)
                        .wrap(TextWrap::None),
                );
            });
            body(ui);
        },
    );
}

fn counter_card(ui: &mut Ui, theme: &Theme, handles: &Handles) {
    card(ui, theme, "State counter", "State<u32>", |ui| {
        ui.row(|ui| {
            ui.width_full().align_items(Align::Center).gap(10.0);
            ui.add(
                Label::new(handles.count.get().to_string())
                    .font_size(48.0)
                    .color(theme.colors.on_surface)
                    .weight(FontWeight::SEMIBOLD),
            );
            ui.spacer();
            if ui
                .add(Button::new("-").min_width(42.0).padding_xy(0.0, 10.0))
                .clicked()
            {
                handles
                    .count
                    .update(|count| *count = count.saturating_sub(1));
            }
            if ui.add(Button::new("Reset").subtle()).clicked() {
                handles.count.set(0);
            }
            if ui
                .add(
                    Button::new("+")
                        .primary()
                        .min_width(42.0)
                        .padding_xy(0.0, 10.0),
                )
                .clicked()
            {
                handles.count.update(|count| *count += 1);
            }
        });
        ui.add(
            Label::new("State handles can leave the frame; they never borrow the Ui.")
                .font_size(11.0)
                .color(theme.colors.on_surface_variant),
        );
    });
}

fn rows_card(ui: &mut Ui, theme: &Theme, handles: &Handles) {
    card(ui, theme, "Keyed rows", "ui.keyed", |ui| {
        let rows = handles.rows.get();
        if rows.is_empty() {
            ui.add(
                Label::new("No rows. Add one to exercise reconciliation.")
                    .font_size(12.0)
                    .color(theme.colors.outline),
            );
        }
        for id in rows {
            ui.keyed(id, |ui| {
                ui.row(|ui| {
                    ui.width_full()
                        .height(34.0)
                        .padding_xy(10.0, 0.0)
                        .gap(10.0)
                        .align_items(Align::Center)
                        .background(theme.colors.surface_container)
                        .radius(theme.shape.small);
                    ui.add(
                        Label::new(format!("row {id:02}"))
                            .font_size(12.0)
                            .color(theme.colors.on_surface_variant)
                            .family(FontFamily::Monospace),
                    );
                    ui.spacer();
                    if ui
                        .add(
                            Button::new("remove")
                                .subtle()
                                .font_size(11.0)
                                .padding_xy(10.0, 5.0),
                        )
                        .clicked()
                    {
                        handles.rows.update(|rows| rows.retain(|row| *row != id));
                    }
                });
            });
        }
        ui.row(|ui| {
            ui.width_full().align_items(Align::Center).gap(8.0);
            if ui.add(Button::new("Add row").subtle()).clicked() {
                let id = handles.next_row.get();
                handles.next_row.set(id + 1);
                handles.rows.update(|rows| rows.push(id));
            }
            ui.spacer();
            ui.add(
                Label::new(format!("{} rows", handles.rows.get().len()))
                    .font_size(10.0)
                    .color(theme.colors.outline)
                    .family(FontFamily::Monospace),
            );
        });
    });
}

fn animation_card(ui: &mut Ui, theme: &Theme, handles: &Handles) {
    card(ui, theme, "Motion system", "tweens · springs", |ui| {
        let target = handles.target.get();
        let value = ui.animate_with("chart", target, MotionSpec::spring(Spring::new(0.38, 0.76)));
        ui.row(|ui| {
            ui.width_full().align_items(Align::End);
            ui.add(
                Label::new(format!("{:.0}%", value * 100.0))
                    .font_size(32.0)
                    .color(theme.colors.on_surface)
                    .weight(FontWeight::SEMIBOLD),
            );
            ui.spacer();
            ui.add(
                Label::new("spring · 380ms")
                    .font_size(10.0)
                    .color(theme.colors.outline)
                    .family(FontFamily::Monospace),
            );
        });
        ui.column(|ui| {
            ui.width_full()
                .height(10.0)
                .radius(theme.shape.extra_small)
                .background(theme.colors.surface_container_high);
            ui.row(|ui| {
                ui.width(Length::Percent(value * 100.0))
                    .height_full()
                    .radius(theme.shape.extra_small)
                    .background(theme.colors.primary);
            });
        });
        ui.row(|ui| {
            ui.width_full().align_items(Align::Center).gap(8.0);
            if ui.add(Button::new("Advance").primary()).clicked() {
                handles.target.update(|target| {
                    *target = if *target >= 0.999 {
                        0.0
                    } else {
                        (*target + 0.2).min(1.0)
                    };
                });
            }
            if ui.add(Button::new("Reset").subtle()).clicked() {
                handles.target.set(0.0);
            }
        });
    });
}

fn note_card(ui: &mut Ui, theme: &Theme) {
    card(ui, theme, "What persists?", "node / state", |ui| {
        ui.add(
            Label::new("Nodes are reconciled by id and recycled at frame end. State survives the tree on a TTL; the interpolator lives on the node.")
                .font_size(12.0)
                .color(theme.colors.on_surface_variant),
        );
        ui.column(|ui| {
            ui.width_full()
                .padding(10.0)
                .background(theme.colors.background)
                .radius(theme.shape.small);
            ui.add(
                Label::new("fn view(ui: &mut Ui) {\n    let n = ui.state(|| 0i32);\n    ui.button(\"+\");\n}")
                    .font_size(10.5)
                    .color(theme.colors.on_surface_variant)
                    .family(FontFamily::Monospace),
            );
        });
    });
}

fn hex(color: Color) -> String {
    let [r, g, b, _] = color.to_rgba8();
    format!("#{r:02X}{g:02X}{b:02X}")
}

fn typography(ui: &mut Ui, theme: &Theme) {
    ui.column(|ui| {
        ui.width_full().gap(18.0);
        card(ui, theme, "M3 type scale", "15 roles · Roboto metrics", |ui| {
            ui.add(
                Label::new("Display/headline/title/body/label, with the official size, line height, weight and tracking tokens. Label::style applies a complete role.")
                    .font_size(12.0)
                    .color(theme.colors.on_surface_variant),
            );
            for (name, style, sample) in [
                (
                    "display large · 57/64",
                    theme.typography.display_large.clone(),
                    "Immediate",
                ),
                (
                    "headline medium · 28/36",
                    theme.typography.headline_medium.clone(),
                    "Retained core",
                ),
                (
                    "title large · 22/28",
                    theme.typography.title_large.clone(),
                    "A title for content",
                ),
                (
                    "body large · 16/24",
                    theme.typography.body_large.clone(),
                    "Body copy is the workhorse of the scale; it keeps a 24 px line height.",
                ),
                (
                    "label large · 14/20",
                    theme.typography.label_large.clone(),
                    "BUTTON LABEL",
                ),
                (
                    "body small · 12/16",
                    theme.typography.body_small.clone(),
                    "Supporting caption text.",
                ),
            ] {
                ui.row(|ui| {
                    ui.width_full().align_items(Align::End).gap(16.0);
                    ui.column(|ui| {
                        ui.width(190.0);
                        ui.add(
                            Label::new(name)
                                .font_size(10.0)
                                .color(theme.colors.outline)
                                .family(FontFamily::Monospace)
                                .wrap(TextWrap::None),
                        );
                    });
                    ui.column(|ui| {
                        ui.grow(1.0);
                        let mut style = style;
                        style.color = theme.colors.on_surface;
                        ui.add(Label::new(sample).style(style));
                    });
                });
                ui.divider();
            }
        });

        card(ui, theme, "Color roles", "md.sys.color.*", |ui| {
            let swatches = [
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
                (
                    "tertiary",
                    theme.colors.tertiary,
                    theme.colors.on_tertiary,
                ),
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
                    "surface container highest",
                    theme.colors.surface_container_highest,
                    theme.colors.on_surface,
                ),
                (
                    "inverse surface",
                    theme.colors.inverse_surface,
                    theme.colors.inverse_on_surface,
                ),
                ("outline", theme.colors.outline, theme.colors.surface),
                (
                    "outline variant",
                    theme.colors.outline_variant,
                    theme.colors.on_surface,
                ),
                ("success (extension)", theme.colors.success, theme.colors.on_success),
            ];
            ui.grid(4, |ui| {
                ui.width_full().gap(10.0);
                for (name, fill, text) in swatches {
                    ui.column(|ui| {
                        ui.width_full()
                            .height(62.0)
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
        });

        ui.row(|ui| {
            ui.width_full().gap(18.0).align_items(Align::Stretch);
            ui.column(|ui| {
                ui.grow(1.0).basis(0.0);
                card(ui, theme, "CJK fallback", "fontdb fallback", |ui| {
                    ui.add(
                        Label::new("即时为面，保留为骨。")
                            .font_size(24.0)
                            .color(theme.colors.on_surface),
                    );
                    ui.add(
                        Label::new("字体回退、行高与换行由文本层处理，用户代码只写字符串。")
                            .font_size(13.0)
                            .color(theme.colors.on_surface_variant),
                    );
                });
            });
            ui.column(|ui| {
                ui.grow(1.0).basis(0.0);
                card(ui, theme, "Weights", "100 – 900", |ui| {
                    for (label, weight) in [
                        ("Light 300", FontWeight::LIGHT),
                        ("Regular 400", FontWeight::NORMAL),
                        ("Medium 500", FontWeight::MEDIUM),
                        ("Semibold 600", FontWeight::SEMIBOLD),
                        ("Bold 700", FontWeight::BOLD),
                    ] {
                        ui.add(
                            Label::new(label)
                                .font_size(16.0)
                                .color(theme.colors.on_surface)
                                .weight(weight),
                        );
                    }
                });
            });
        });

        card(ui, theme, "Monospace", "FontFamily::Monospace", |ui| {
            ui.add(
                Label::new("let clicks = ui.button(\"OK\").clicked();\nstate.update(|count| *count += clicks as u32);")
                    .font_size(13.0)
                    .color(theme.colors.success)
                    .family(FontFamily::Monospace)
                    .line_height(20.0),
            );
        });
    });
}

fn layout_page(ui: &mut Ui, theme: &Theme) {
    ui.column(|ui| {
        ui.width_full().gap(18.0);
        card(ui, theme, "Row distribution", "justify", |ui| {
            for justify in [
                Justify::Start,
                Justify::Center,
                Justify::SpaceBetween,
                Justify::SpaceAround,
            ] {
                ui.row(|ui| {
                    ui.width_full()
                        .height(44.0)
                        .padding_xy(8.0, 0.0)
                        .gap(8.0)
                        .align_items(Align::Center)
                        .justify(justify)
                        .background(theme.colors.background)
                        .radius(theme.shape.small);
                    for index in 0..3 {
                        ui.column(|ui| {
                            ui.width(48.0)
                                .height(26.0)
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
        });

        ui.row(|ui| {
            ui.width_full().gap(18.0).align_items(Align::Stretch);
            ui.column(|ui| {
                ui.grow(1.0).basis(0.0);
                card(ui, theme, "Column gap", "flex", |ui| {
                    for index in 0..4 {
                        ui.row(|ui| {
                            ui.width_full()
                                .height(22.0)
                                .radius(theme.shape.extra_small)
                                .background(if index % 2 == 0 {
                                    theme.colors.surface_container
                                } else {
                                    theme.colors.surface_container_high
                                });
                        });
                    }
                });
            });
            ui.column(|ui| {
                ui.grow(1.0).basis(0.0);
                card(ui, theme, "Alignment", "align_items", |ui| {
                    for align in [Align::Start, Align::Center, Align::End] {
                        ui.row(|ui| {
                            ui.width_full()
                                .height(38.0)
                                .align_items(align)
                                .background(theme.colors.background)
                                .radius(theme.shape.extra_small);
                            ui.row(|ui| {
                                ui.width(70.0)
                                    .height(16.0)
                                    .radius(theme.shape.extra_small)
                                    .background(theme.colors.primary);
                            });
                        });
                    }
                });
            });
        });
    });
}

fn form_page(ui: &mut Ui, theme: &Theme, handles: &Handles) {
    ui.row(|ui| {
        ui.width_full().gap(18.0).align_items(Align::Stretch);
        ui.column(|ui| {
            ui.grow(1.25).basis(0.0);
            card(ui, theme, "Text input", "State<String>", |ui| {
                ui.add(
                    Label::new("Name")
                        .font_size(11.0)
                        .color(theme.colors.on_surface_variant),
                );
                let response = ui.add(
                    TextInput::new(&handles.name)
                        .placeholder("Type your name")
                        .width(Length::Percent(100.0)),
                );
                if response.changed() {
                    let name = handles.name.get();
                    if !name.is_empty() {
                        handles.greeting.set(format!("Hello, {name}!"));
                    }
                }

                ui.row(|ui| {
                    ui.width_full().align_items(Align::Center).gap(8.0);
                    if ui.add(Button::new("Say hello").primary()).clicked() {
                        let name = handles.name.get();
                        if !name.is_empty() {
                            handles.greeting.set(format!("Hello, {name}!"));
                        }
                    }
                    if ui.add(Button::new("Clear").subtle()).clicked() {
                        handles.name.set(String::new());
                        handles.greeting.set(String::new());
                    }
                    ui.spacer();
                    ui.add(
                        Label::new("single line · append at end")
                            .font_size(10.0)
                            .color(theme.colors.outline)
                            .family(FontFamily::Monospace),
                    );
                });

                let greeting = handles.greeting.get();
                if greeting.is_empty() {
                    ui.add(
                        Label::new("Click the field and type. The value is a State<String>.")
                            .font_size(12.0)
                            .color(theme.colors.on_surface_variant),
                    );
                } else {
                    ui.add(
                        Label::new(greeting)
                            .font_size(24.0)
                            .color(theme.colors.primary)
                            .weight(FontWeight::SEMIBOLD),
                    );
                }
            });
        });

        ui.column(|ui| {
            ui.grow(1.0).basis(0.0);
            card(ui, theme, "Focus and keys", "no Rc<RefCell>", |ui| {
                for line in [
                    "Click the field to focus it.",
                    "Type to append; Backspace removes the last character.",
                    "Tab and Shift+Tab cycle focusable nodes.",
                    "Esc clears focus.",
                    "Enter activates a focused button.",
                ] {
                    ui.add(
                        Label::new(format!("· {line}"))
                            .font_size(12.0)
                            .color(theme.colors.on_surface_variant),
                    );
                }
                ui.spacer();
                ui.add(Button::new("Disabled").enabled(false));
            });
        });
    });
}

fn secondary_window(ui: &mut Ui) {
    let theme = ui.theme().clone();
    let window = ui.window().clone();
    let primary = ui
        .windows()
        .iter()
        .find(|candidate| candidate.is_primary())
        .map(|candidate| candidate.key);
    ui.column(|ui| {
        ui.size_full()
            .padding(24.0)
            .gap(16.0)
            .background(theme.colors.background);
        ui.add(
            Label::new("SECONDARY WINDOW")
                .font_size(10.0)
                .color(theme.colors.primary)
                .family(FontFamily::Monospace)
                .weight(FontWeight::SEMIBOLD),
        );
        ui.add(
            Label::new("Inspector")
                .font_size(30.0)
                .color(theme.colors.on_surface)
                .semibold(),
        );
        ui.add(
            Label::new(format!(
                "{} · {:.0} × {:.0} · scale {:.2}",
                window.key,
                window.size.width,
                window.size.height,
                window.scale_factor
            ))
            .font_size(11.0)
            .color(theme.colors.outline)
            .family(FontFamily::Monospace),
        );

        ui.card(Card::outlined().gap(10.0), |ui| {
            ui.add(Label::new("Independent runtime").font_size(16.0).semibold());
            ui.add(
                Label::new("Every window owns a retained tree, layout engine, state store and GPU surface. Widgets read the window they are rendering through ui.window().")
                    .font_size(12.0)
                    .color(theme.colors.on_surface_variant),
            );
        });

        ui.card(Card::filled().gap(10.0), |ui| {
            ui.add(Label::new("Live windows").font_size(16.0).semibold());
            for candidate in ui.windows().to_vec() {
                ui.row(|ui| {
                    ui.width_full().align_items(Align::Center).gap(8.0);
                    ui.add(
                        Label::new(format!("{}  {}", candidate.key, candidate.title))
                            .font_size(11.0)
                            .color(if candidate.focused {
                                theme.colors.on_surface
                            } else {
                                theme.colors.on_surface_variant
                            })
                            .family(FontFamily::Monospace),
                    );
                    ui.spacer();
                    ui.add(
                        Label::new(if candidate.is_primary() {
                            "primary"
                        } else {
                            "secondary"
                        })
                        .font_size(9.0)
                        .color(theme.colors.outline),
                    );
                });
            }
        });

        ui.spacer();
        ui.row(|ui| {
            ui.width_full().gap(8.0);
            if ui.add(Button::new("Toggle theme").tonal()).clicked() {
                ui.set_theme(theme.toggled());
            }
            if let Some(primary) = primary
                && ui.add(Button::new("Focus primary").subtle()).clicked()
            {
                ui.focus_window(primary);
            }
            ui.spacer();
            if ui.add(Button::new("Close window").primary()).clicked() {
                let key = window.key;
                ui.close_window(key);
            }
        });
    });
}

fn surfaces_page(ui: &mut Ui, theme: &Theme, handles: &Handles) {
    ui.column(|ui| {
        ui.width_full().gap(18.0);
        ui.row(|ui| {
            ui.width_full().gap(14.0).align_items(Align::Stretch);
            for (variant, title, detail) in [
                (
                    CardVariant::Elevated,
                    "Elevated",
                    "Tonal surface plus a hairline border. The paint layer has no shadows yet, so elevation is expressed with surface roles.",
                ),
                (
                    CardVariant::Filled,
                    "Filled",
                    "A quiet container for dense dashboards. No outline, 12 px corners and the theme's surface token.",
                ),
                (
                    CardVariant::Outlined,
                    "Outlined",
                    "The lowest-emphasis treatment. Useful when the page itself is already a surface.",
                ),
            ] {
                ui.column(|ui| {
                    ui.grow(1.0).basis(0.0);
                    ui.card(
                        Card::new()
                            .variant(variant)
                            .padding(16.0)
                            .gap(8.0)
                            .width(Length::Percent(100.0)),
                        |ui| {
                            ui.add(
                                Label::new(title)
                                    .font_size(16.0)
                                    .semibold()
                                    .color(theme.colors.on_surface),
                            );
                            ui.add(
                                Label::new(detail)
                                    .font_size(11.0)
                                    .color(theme.colors.on_surface_variant),
                            );
                            ui.add(
                                Label::new("Card")
                                    .font_size(9.0)
                                    .color(theme.colors.primary)
                                    .family(FontFamily::Monospace),
                            );
                        },
                    );
                });
            }
        });

        ui.row(|ui| {
            ui.width_full().gap(18.0).align_items(Align::Stretch);
            ui.column(|ui| {
                ui.grow(1.0).basis(0.0);
                ui.card(Card::new().gap(12.0), |ui| {
                    ui.add(Label::new("Modal dialog").font_size(17.0).semibold());
                    ui.add(
                        Label::new("A root overlay with a full-window scrim. Escape and a scrim click close it; page controls stay unreachable while it is open.")
                            .font_size(12.0)
                            .color(theme.colors.on_surface_variant),
                    );
                    ui.add(
                        Label::new(format!("Project: {}", handles.project_name.get()))
                            .font_size(13.0)
                            .color(theme.colors.primary)
                            .family(FontFamily::Monospace),
                    );
                    if ui.add(Button::new("Rename project").primary()).clicked() {
                        handles.dialog_open.set(true);
                    }
                });
            });
            ui.column(|ui| {
                ui.grow(1.0).basis(0.0);
                ui.card(Card::outlined().gap(12.0), |ui| {
                    ui.add(Label::new("Multiple windows").font_size(17.0).semibold());
                    ui.add(
                        Label::new("Open another platform window. Each one gets its own Runtime and StateStore, and the application object is shared.")
                            .font_size(12.0)
                            .color(theme.colors.on_surface_variant),
                    );
                    let windows = ui.windows().to_vec();
                    for candidate in &windows {
                        ui.row(|ui| {
                            ui.width_full().align_items(Align::Center).gap(8.0);
                            ui.add(
                                Label::new(format!(
                                    "{} {}{}",
                                    if candidate.is_primary() { "●" } else { "○" },
                                    candidate.key,
                                    if candidate.focused { " · focused" } else { "" }
                                ))
                                .font_size(10.5)
                                .color(theme.colors.on_surface_variant)
                                .family(FontFamily::Monospace),
                            );
                            ui.spacer();
                            if !candidate.is_primary() {
                                if ui
                                    .add(
                                        Button::new("Focus")
                                            .subtle()
                                            .font_size(10.0)
                                            .padding_xy(8.0, 4.0),
                                    )
                                    .clicked()
                                {
                                    ui.focus_window(candidate.key);
                                }
                                if ui
                                    .add(
                                        Button::new("Close")
                                            .subtle()
                                            .font_size(10.0)
                                            .padding_xy(8.0, 4.0),
                                    )
                                    .clicked()
                                {
                                    ui.close_window(candidate.key);
                                }
                            }
                        });
                    }
                    if ui
                        .add(Button::new("Open inspector window").primary())
                        .clicked()
                    {
                        ui.open_window(
                            WindowRequest::new("Kasumi — Inspector")
                                .size(560.0, 700.0)
                                .min_size(360.0, 320.0)
                                .theme(theme.clone()),
                        );
                    }
                });
            });
        });

        ui.card(Card::filled().gap(12.0), |ui| {
            ui.add(Label::new("Current window").font_size(17.0).semibold());
            let window = ui.window().clone();
            ui.add(
                Label::new(format!(
                    "{} · {} · {:.0} × {:.0} @ {:.2}× · {} live window(s)",
                    window.key,
                    window.title,
                    window.size.width,
                    window.size.height,
                    window.scale_factor,
                    ui.windows().len()
                ))
                .font_size(11.0)
                .color(theme.colors.on_surface_variant)
                .family(FontFamily::Monospace),
            );
        });

        ui.card(Card::outlined().gap(theme.spacing.md), |ui| {
            ui.add(Label::new("Buttons, shape and elevation").font_size(17.0).semibold());
            ui.add(
                Label::new("M3 emphasis levels map to filled, tonal, outlined, text and elevated buttons. Shape and elevation tokens are available on the theme for custom surfaces.")
                    .font_size(12.0)
                    .color(theme.colors.on_surface_variant),
            );
            ui.wrap_row(|ui| {
                ui.gap(8.0);
                ui.add(Button::new("Filled").filled());
                ui.add(Button::new("Tonal").tonal());
                ui.add(Button::new("Outlined").outlined());
                ui.add(Button::new("Text").text());
                ui.add(Button::new("Elevated").elevated());
                ui.add(Button::new("Disabled").filled().enabled(false));
            });
            ui.row(|ui| {
                ui.width_full().gap(10.0).align_items(Align::End);
                for (label, radius) in [
                    ("extra small", theme.shape.extra_small),
                    ("small", theme.shape.small),
                    ("medium", theme.shape.medium),
                    ("large", theme.shape.large),
                    ("extra large", theme.shape.extra_large),
                ] {
                    ui.column(|ui| {
                        ui.gap(4.0);
                        // The swatch is its own child. Styling the outer
                        // column would paint its background behind the label
                        // and make the caption look like overflowing text.
                        ui.column(|ui| {
                            ui.width(74.0)
                                .height(44.0)
                                .background(theme.colors.secondary_container)
                                .radius(radius);
                        });
                        ui.add(
                            Label::new(format!("{label}\n{radius:.0} dp"))
                                .font_size(9.0)
                                .line_height(12.0)
                                .color(theme.colors.outline)
                                .family(FontFamily::Monospace)
                                .wrap(TextWrap::None),
                        );
                    });
                }
                ui.spacer();
                ui.column(|ui| {
                    ui.gap(4.0);
                    ui.add(
                        Label::new("elevation")
                            .font_size(9.0)
                            .color(theme.colors.outline)
                            .family(FontFamily::Monospace),
                    );
                    ui.row(|ui| {
                        ui.gap(4.0);
                        for level in 0..=5u8 {
                            ui.column(|ui| {
                                ui.width(26.0)
                                    .height(44.0)
                                    .radius(theme.shape.small)
                                    .background(theme.surface_at_elevation(level))
                                    .border(1.0, theme.colors.outline_variant.fade(0.5));
                            });
                        }
                    });
                });
            });
        });

        let _ = ui.dialog(
            Dialog::new(&handles.dialog_open)
                .title("Rename project")
                .description("The dialog body is ordinary immediate-mode content. Background state remains untouched while the scrim is up.")
                .width(460.0),
            |ui| {
                ui.add(
                    Label::new("Project name")
                        .font_size(11.0)
                        .color(theme.colors.on_surface_variant),
                );
                ui.add(
                    TextInput::new(&handles.project_name)
                        .placeholder("Project name")
                        .width(Length::Percent(100.0)),
                );
                ui.row(|ui| {
                    ui.width_full().justify(Justify::End).gap(8.0);
                    if ui.add(Button::new("Cancel").subtle()).clicked() {
                        handles.dialog_open.set(false);
                    }
                    if ui.add(Button::new("Save").primary()).clicked() {
                        handles.dialog_open.set(false);
                    }
                });
            },
        );
    });
}

#[derive(Clone)]
struct AsyncSample {
    request: usize,
    rows: usize,
    latency_ms: u64,
}

static ASYNC_REQUESTS: AtomicUsize = AtomicUsize::new(0);
static FUTURE_REQUESTS: AtomicUsize = AtomicUsize::new(0);

fn load_async_sample() -> Result<AsyncSample, String> {
    let request = ASYNC_REQUESTS.fetch_add(1, Ordering::Relaxed) + 1;
    std::thread::sleep(Duration::from_millis(650));
    if request % 4 == 0 {
        return Err(format!("simulated network failure on request {request}"));
    }
    Ok(AsyncSample {
        request,
        rows: 2_400 + request * 37,
        latency_ms: 28 + (request as u64 * 7) % 41,
    })
}

fn async_page(ui: &mut Ui, theme: &Theme) {
    ui.column(|ui| {
        ui.width_full().gap(18.0);
        ui.add(
            Label::new("Load data off the UI thread. The runtime receives a wake event when a worker finishes; idle windows stay asleep until then.")
                .font_size(12.0)
                .color(theme.colors.on_surface_variant),
        );

        ui.card(Card::new().gap(12.0), |ui| {
            ui.row(|ui| {
                ui.width_full().align_items(Align::Center).gap(8.0);
                ui.add(Label::new("Blocking loader").font_size(17.0).semibold());
                ui.spacer();
                ui.add(
                    Label::new("thread pool · Resource<T>")
                        .font_size(9.0)
                        .color(theme.colors.outline)
                        .family(FontFamily::Monospace),
                );
            });
            ui.add(
                Label::new("The loader runs once per cache key on a worker thread. Reload keeps the old value visible while the next request is in flight. Every fourth request fails on purpose to exercise the error state.")
                    .font_size(12.0)
                    .color(theme.colors.on_surface_variant),
            );

            let sample = ui.resource_keyed("demo::async-sample", load_async_sample);
            let status = sample.status();
            match status {
                ResourceStatus::Loading if sample.has_value() => {
                    let stale = sample.get().unwrap();
                    ui.add(
                        Label::new(format!(
                            "Refreshing… showing request #{} from cache",
                            stale.request
                        ))
                        .font_size(13.0)
                        .color(theme.colors.outline)
                        .family(FontFamily::Monospace),
                    );
                }
                ResourceStatus::Loading => {
                    ui.add(
                        Label::new("Loading on a background worker…")
                            .font_size(18.0)
                            .color(theme.colors.primary)
                            .semibold(),
                    );
                }
                ResourceStatus::Failed => {
                    ui.add(
                        Label::new(format!(
                            "Load failed: {}",
                            sample
                                .error()
                                .map(|error| error.message().to_owned())
                                .unwrap_or_default()
                        ))
                        .font_size(13.0)
                        .color(theme.colors.error),
                    );
                    if let Some(stale) = sample.get() {
                        ui.add(
                            Label::new(format!(
                                "Last good value: request #{} · {} rows · {} ms",
                                stale.request, stale.rows, stale.latency_ms
                            ))
                            .font_size(11.0)
                            .color(theme.colors.outline)
                            .family(FontFamily::Monospace),
                        );
                    }
                }
                ResourceStatus::Ready => {
                    if let Some(sample) = sample.get() {
                        ui.add(
                            Label::new(format!("Request #{} complete", sample.request))
                                .font_size(24.0)
                                .color(theme.colors.on_surface)
                                .semibold(),
                        );
                        ui.add(
                            Label::new(format!(
                                "{} rows · {} ms · cached by key",
                                sample.rows, sample.latency_ms
                            ))
                            .font_size(11.0)
                            .color(theme.colors.success)
                            .family(FontFamily::Monospace),
                        );
                    }
                }
                ResourceStatus::Idle => {
                    ui.add(
                        Label::new("Idle. Press Load to start a request.")
                            .font_size(13.0)
                            .color(theme.colors.outline),
                    );
                }
            }

            ui.row(|ui| {
                ui.width_full().align_items(Align::Center).gap(8.0);
                if ui.add(Button::new("Reload").primary()).clicked() {
                    sample.reload();
                }
                if ui.add(Button::new("Cancel").subtle()).clicked() {
                    sample.cancel();
                }
                ui.spacer();
                ui.add(
                    Label::new(format!("{status:?} · cached by key"))
                        .font_size(9.0)
                        .color(theme.colors.outline)
                        .family(FontFamily::Monospace),
                );
            });
        });

        ui.row(|ui| {
            ui.width_full().gap(18.0).align_items(Align::Stretch);
            ui.column(|ui| {
                ui.grow(1.0).basis(0.0);
                ui.card(Card::outlined().gap(10.0), |ui| {
                    ui.add(Label::new("Future resource").font_size(17.0).semibold());
                    ui.add(
                        Label::new("resource_future_keyed drives an async future on the same executor. The closure is a factory, so reload creates a fresh future.")
                            .font_size(12.0)
                            .color(theme.colors.on_surface_variant),
                    );
                    let future = ui.resource_future_keyed("demo::async-future", || async {
                        let request = FUTURE_REQUESTS.fetch_add(1, Ordering::Relaxed) + 1;
                        std::thread::sleep(Duration::from_millis(420));
                        Ok::<_, String>(format!(
                            "future #{request} resolved {}",
                            if request == 1 { "once" } else { "again" }
                        ))
                    });
                    let status = future.status();
                    match (status, future.get()) {
                        (ResourceStatus::Ready, Some(value)) => {
                            ui.add(
                                Label::new(value)
                                    .font_size(14.0)
                                    .color(theme.colors.primary)
                                    .family(FontFamily::Monospace),
                            );
                        }
                        (ResourceStatus::Loading, _) => {
                            ui.add(
                                Label::new("Polling the future…")
                                    .font_size(13.0)
                                    .color(theme.colors.outline),
                            );
                        }
                        (ResourceStatus::Failed, _) => {
                            ui.add(
                                Label::new("Future rejected")
                                    .font_size(13.0)
                                    .color(theme.colors.error),
                            );
                        }
                        _ => {
                            ui.add(
                                Label::new("No future result yet")
                                    .font_size(13.0)
                                    .color(theme.colors.outline),
                            );
                        }
                    }
                    if ui
                        .add(Button::new("Run future again").subtle().full_width())
                        .clicked()
                    {
                        future.reload();
                    }
                });
            });
            ui.column(|ui| {
                ui.grow(1.0).basis(0.0);
                ui.card(Card::filled().gap(10.0), |ui| {
                    ui.add(Label::new("Cache lifecycle").font_size(17.0).semibold());
                    for line in [
                        "Cached handles live on the runtime and survive frames.",
                        "Unused slots expire after the resource TTL (600 frames by default).",
                        "A reload keeps the previous value while status is Loading.",
                        "Completion wakes the winit event loop through an EventLoopProxy.",
                        "Values and loaders must be Send + Sync + 'static.",
                    ] {
                        ui.add(
                            Label::new(format!("· {line}"))
                                .font_size(12.0)
                                .color(theme.colors.on_surface_variant),
                        );
                    }
                });
            });
        });
    });
}

fn controls_page(ui: &mut Ui, theme: &Theme, handles: &Handles) {
    ui.column(|ui| {
        ui.width_full().gap(18.0);
        ui.row(|ui| {
            ui.width_full().gap(18.0).align_items(Align::Stretch);
            ui.column(|ui| {
                ui.grow(1.15).basis(0.0).gap(18.0);
                card(ui, theme, "Rich text editing", "selection + clipboard", |ui| {
                    ui.add(
                        Label::new("Mouse drag to select · Ctrl+A/C/X/V · Ctrl+Z / Ctrl+Shift+Z")
                            .font_size(10.5)
                            .color(theme.colors.outline)
                            .family(FontFamily::Monospace),
                    );
                    ui.add(
                        TextInput::new(&handles.name)
                            .placeholder("Drag across this text, then paste")
                            .width(Length::Percent(100.0)),
                    );
                    ui.row(|ui| {
                        ui.width_full().align_items(Align::Center).gap(10.0);
                        ui.add(
                            Button::new("Paste-ready")
                                .subtle()
                                .font_size(11.0)
                                .padding_xy(10.0, 5.0),
                        );
                        ui.spacer();
                        ui.add(
                            Label::new(format!(
                                "caret/selection retained · {} chars",
                                handles.name.with(|name| name.chars().count())
                            ))
                            .font_size(10.0)
                            .color(theme.colors.outline)
                            .family(FontFamily::Monospace),
                        );
                    });
                });

                card(ui, theme, "Material lists", "1 / 2 / 3-line · virtualized", |ui| {
                    ui.add(
                        Label::new(
                            "Rows follow the M3 list tokens: 56, 72 and 88 dp, a 16 dp inset, \
                             leading icons and avatars, and a secondary-container selection.",
                        )
                        .font_size(11.0)
                        .color(theme.colors.on_surface_variant),
                    );
                    let list = List::new()
                        .header("This week")
                        .row(
                            ListRow::new("Design review")
                                .supporting("Today, 09:30 · 4 comments")
                                .leading_avatar("DR")
                                .trailing("09:30"),
                        )
                        .divider()
                        .row(
                            ListRow::new("Deploy pipeline")
                                .supporting("main · 3 commits ahead")
                                .leading_icon("↑")
                                .trailing_icon("›"),
                        )
                        .row(ListRow::new("Notifications").leading_icon("◔").trailing_icon("›"))
                        .row(
                            ListRow::new("Release notes")
                                .supporting(
                                    "Every milestone is a commit: the menu, the drawer, the list \
                                     and the rest of the M3 surface land separately.",
                                )
                                .height(ListRowHeight::ThreeLine)
                                .leading_icon("▤"),
                        )
                        .divider()
                        .header("Virtualized")
                        .virtualize(true)
                        .overscan(90.0)
                        .size(Length::Percent(100.0), 300.0);
                    if let Some(index) = ui.list(&list).clicked {
                        handles.combo.set(index.min(3));
                    }
                    ui.add(
                        Label::new("10,000 rows · List::virtualize")
                            .font_size(9.0)
                            .color(theme.colors.outline)
                            .family(FontFamily::Monospace),
                    );
                    ui.list(virtualized_list());
                });
            });

            ui.column(|ui| {
                ui.grow(1.0).basis(0.0).gap(18.0);
                card(ui, theme, "Choices", "checkbox · radio · combo", |ui| {
                    ui.checkbox("Enable notifications", &handles.notifications);
                    ui.divider();
                    ui.add(
                        Label::new("Plan")
                            .font_size(10.0)
                            .color(theme.colors.outline)
                            .family(FontFamily::Monospace),
                    );
                    ui.row(|ui| {
                        ui.width_full().gap(16.0);
                        ui.radio("Free", 0usize, &handles.plan);
                        ui.radio("Team", 1usize, &handles.plan);
                        ui.radio("Studio", 2usize, &handles.plan);
                    });
                    ui.add(
                        ComboBox::new(
                            ["Automatic", "Always smooth", "Reduced motion"],
                            &handles.combo,
                        )
                        .width(Length::Percent(100.0)),
                    );
                });

                card(ui, theme, "Values", "slider · progress", |ui| {
                    ui.row(|ui| {
                        ui.width_full().align_items(Align::Center);
                        ui.add(
                            Label::new("Volume")
                                .font_size(12.0)
                                .color(theme.colors.on_surface_variant),
                        );
                        ui.spacer();
                        ui.add(
                            Label::new(format!("{:.0}%", handles.volume.get() * 100.0))
                                .font_size(11.0)
                                .color(theme.colors.primary)
                                .family(FontFamily::Monospace),
                        );
                    });
                    ui.add(
                        Slider::new(&handles.volume, 0.0, 1.0)
                            .step(0.01)
                            .label("Volume")
                            .width(Length::Percent(100.0)),
                    );
                    ui.add(
                        Label::new("Build progress")
                            .font_size(12.0)
                            .color(theme.colors.on_surface_variant),
                    );
                    ui.progress(&handles.progress);
                });

                card(ui, theme, "Menus and submenus", "right click · nested", |ui| {
                    ui.add(
                        Label::new(
                            "Hover a parent row for 400 ms or press → to cascade; \
                             ← closes one level and Esc closes the menu.",
                        )
                        .font_size(10.5)
                        .color(theme.colors.outline),
                    );
                    let trigger = ui.add(
                        Button::new("Right click for actions")
                            .subtle()
                            .full_width(),
                    );
                    ui.tooltip(&trigger, "The menu and tooltip both use root overlays.");
                    if let Some(menu) = ui.context_menu(
                        &trigger,
                        Menu::new()
                            .item(MenuItem::new("Reset volume").shortcut("R"))
                            .item(MenuItem::new("Complete progress"))
                            .separator()
                            .submenu(
                                "Notifications",
                                Menu::new().item(MenuItem::new("Toggle").shortcut("N")),
                            )
                            .submenu(
                                "Advanced",
                                Menu::new()
                                    .item(MenuItem::new("Reset everything"))
                                    .submenu(
                                        "Danger zone",
                                        Menu::new().item(MenuItem::new("Delete project")),
                                    ),
                            ),
                    ) {
                        match menu.path.as_slice() {
                            [0] => handles.volume.set(0.5),
                            [1] => handles.progress.set(1.0),
                            [3, 0] => handles.notifications.update(|value| *value = !*value),
                            [4, 0] => {
                                handles.volume.set(0.5);
                                handles.progress.set(0.0);
                                handles.notifications.set(true);
                            }
                            _ => {}
                        }
                    }
                });
            });
        });

        ui.grid(3, |ui| {
            ui.width_full().gap(12.0);
            for (title, value, detail) in [
                ("GRID", "3 columns", "taffy grid tracks"),
                ("STACK", "overlay", "shared grid cell"),
                ("FLEX", "row / column", "wrap and center"),
            ] {
                ui.column(|ui| {
                    ui.width_full()
                        .padding(12.0)
                        .gap(3.0)
                        .background(theme.colors.surface_container_low)
                        .border(1.0, theme.colors.outline_variant)
                        .radius(theme.shape.medium);
                    ui.add(
                        Label::new(title)
                            .font_size(9.0)
                            .color(theme.colors.primary)
                            .family(FontFamily::Monospace),
                    );
                    ui.add(
                        Label::new(value)
                            .font_size(15.0)
                            .color(theme.colors.on_surface)
                            .semibold(),
                    );
                    ui.add(Label::new(detail).font_size(10.0).color(theme.colors.outline));
                });
            }
        });

        card(ui, theme, "Media and Markdown", "image · SVG · QR · markdown", |ui| {
            ui.row(|ui| {
                ui.width_full().gap(16.0).align_items(Align::Center);
                ui.add(
                    Image::from_shared(gradient_image())
                        .fit(ImageFit::Cover)
                        .size(108.0, 108.0)
                        .radius(theme.shape.medium),
                );
                ui.add(
                    Svg::new(
                        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 200 200"><defs><linearGradient id="g" x1="0" y1="0" x2="1" y2="1"><stop stop-color="#7c9cff"/><stop offset="1" stop-color="#74d6a0"/></linearGradient></defs><rect width="200" height="200" rx="34" fill="#151923"/><circle cx="100" cy="100" r="62" fill="url(#g)"/><path d="M64 104l24 24 50-58" fill="none" stroke="#0f1117" stroke-width="16" stroke-linecap="round" stroke-linejoin="round"/></svg>"##,
                    )
                    .expect("embedded SVG")
                    .render_size(256, 256)
                    .size(108.0, 108.0),
                );
                ui.add(
                    QrCode::new("https://github.com/lsyes/kasumi")
                        .expect("QR payload")
                        .size(168)
                        .colors(theme.colors.on_surface, theme.colors.surface_container_low),
                );
                ui.column(|ui| {
                    ui.grow(1.0);
                    ui.add(
                        Markdown::new(
                            "### Markdown\n\
                             Rendered with **semantic blocks**, *inline emphasis*, `code`, lists and quotes.\n\n\
                             - keyboard-first editing\n\
                             - crisp rasterized SVG and QR\n\n\
                             > Media stays in the same draw list as text.",
                        )
                        .font_size(12.0),
                    );
                });
            });
        });
    });
}

/// The 10,000-row list is built once. Rebuilding it every frame would spend
/// more time formatting row titles than rendering the scene.
static VIRTUALIZED_LIST: OnceLock<List> = OnceLock::new();

fn virtualized_list() -> &'static List {
    VIRTUALIZED_LIST.get_or_init(|| {
        let mut list = List::new().virtualize(true).overscan(90.0);
        for index in 0..10_000 {
            list =
                list.row(ListRow::new(format!("virtual row {:05}", index + 1)).leading_icon("·"));
        }
        list.size(Length::Percent(100.0), 168.0)
    })
}

static GRADIENT_IMAGE: OnceLock<Arc<ImageData>> = OnceLock::new();

fn gradient_image() -> Arc<ImageData> {
    Arc::clone(
        GRADIENT_IMAGE.get_or_init(|| Arc::new(ImageData::from_rgba8(64, 64, gradient_pixels()))),
    )
}

fn gradient_pixels() -> Vec<u8> {
    let mut pixels = Vec::with_capacity(64 * 64 * 4);
    for y in 0..64 {
        for x in 0..64 {
            let t = (x + y) as f32 / 126.0;
            pixels.extend_from_slice(&[
                (124.0 + (116.0 - 124.0) * t) as u8,
                (156.0 + (214.0 - 156.0) * t) as u8,
                255,
                255,
            ]);
        }
    }
    pixels
}

fn main() -> anyhow::Result<()> {
    let options = WindowOptions {
        title: "Kasumi — immediate surface, retained core".to_owned(),
        logical_size: Size::new(1180.0, 800.0),
        ..WindowOptions::default()
    };
    run(Demo, options)
}
