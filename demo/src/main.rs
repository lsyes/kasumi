use std::time::Duration;

use kasumi::App;
use kasumi::prelude::*;
use kasumi_backend::{WindowOptions, run};

struct Demo;

#[derive(Clone)]
struct Handles {
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
}

impl App for Demo {
    fn view(&mut self, ui: &mut Ui) {
        view(ui);
    }
}

fn view(ui: &mut Ui) {
    let theme = ui.theme().clone();
    let initial_page = std::env::var("KASUMI_DEMO_PAGE")
        .ok()
        .and_then(|value| value.parse::<u8>().ok())
        .unwrap_or(0);
    let handles = Handles {
        page: ui.state(move || initial_page.min(4)),
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
    };

    ui.size_full().background(theme.background);
    ui.row(|ui| {
        ui.size_full().align_items(Align::Stretch);
        sidebar(ui, &theme, &handles);

        ui.column(|ui| {
            ui.grow(1.0)
                .height_full()
                .min_width(0.0)
                .background(theme.background);

            ui.row(|ui| {
                ui.width_full()
                    .padding_edges(Edges {
                        left: 32.0,
                        right: 32.0,
                        top: 28.0,
                        bottom: 16.0,
                    })
                    .background(theme.background);
                header(ui, &theme);
            });
            ui.scroll(
                ScrollView::vertical()
                    .grow(1.0)
                    .line_step(44.0)
                    .smooth(true),
                |ui| {
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
                        _ => ui.keyed("controls", |ui| controls_page(ui, &theme, &handles)),
                    }
                },
            );
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
                    .background(theme.background);
                ui.add(
                    Label::new("Tab cycles focus · Enter activates · Esc clears focus")
                        .font_size(11.0)
                        .color(theme.text_subtle)
                        .family(FontFamily::Monospace),
                );
                ui.spacer();
                ui.add(
                    Label::new("render on demand")
                        .font_size(11.0)
                        .color(theme.text_subtle)
                        .family(FontFamily::Monospace),
                );
            });
        });
    });
}

fn sidebar(ui: &mut Ui, theme: &Theme, handles: &Handles) {
    ui.column(|ui| {
        ui.width(240.0)
            .height_full()
            .padding(20.0)
            .gap(16.0)
            .background(theme.panel);

        ui.row(|ui| {
            ui.width_full().align_items(Align::Center).gap(10.0);
            ui.column(|ui| {
                ui.width(34.0)
                    .height(34.0)
                    .radius(10.0)
                    .background(theme.accent);
            });
            ui.column(|ui| {
                ui.gap(0.0);
                ui.add(
                    Label::new("kasumi")
                        .font_size(19.0)
                        .semibold()
                        .color(theme.text),
                );
                ui.add(
                    Label::new("0.1.0 · skeleton")
                        .font_size(10.0)
                        .color(theme.text_subtle)
                        .family(FontFamily::Monospace),
                );
            });
        });

        ui.column(|ui| {
            ui.width_full().gap(5.0).margin(4.0);
            for (index, title) in ["Overview", "Typography", "Layout", "Form", "Controls"]
                .iter()
                .enumerate()
            {
                let active = handles.page.get() == index as u8;
                let mut button = Button::new(*title).full_width().padding_xy(12.0, 10.0);
                button = if active {
                    button.primary()
                } else {
                    button.subtle()
                };
                if ui.add(button).clicked() {
                    handles.page.set(index as u8);
                }
            }
        });

        ui.spacer();

        ui.column(|ui| {
            ui.width_full().gap(4.0);
            ui.add(
                Label::new("RETAINED CORE")
                    .font_size(9.0)
                    .color(theme.text_subtle)
                    .family(FontFamily::Monospace),
            );
            ui.add(
                Label::new("taffy · cosmic-text")
                    .font_size(11.0)
                    .color(theme.text_muted),
            );
            ui.add(
                Label::new("OpenGL 3.3 · swash glyphs")
                    .font_size(11.0)
                    .color(theme.text_muted),
            );
        });
    });
}

fn header(ui: &mut Ui, theme: &Theme) {
    ui.row(|ui| {
        ui.width_full().align_items(Align::End).gap(16.0);
        ui.column(|ui| {
            ui.gap(4.0);
            ui.add(
                Label::new("IMMEDIATE SURFACE / RETAINED CORE")
                    .font_size(10.0)
                    .color(theme.accent)
                    .family(FontFamily::Monospace)
                    .weight(FontWeight::SEMIBOLD),
            );
            ui.add(
                Label::new("A desktop UI skeleton with no handles in sight")
                    .font_size(28.0)
                    .color(theme.text)
                    .weight(FontWeight::SEMIBOLD),
            );
        });
        ui.spacer();
        ui.add(
            Label::new("view(ui) rebuilds the description;\nthe tree, layout and state persist.")
                .font_size(11.0)
                .color(theme.text_subtle)
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
            stat(ui, theme, "TEXT", "shaped", "cosmic-text + swash");
        });

        ui.row(|ui| {
            ui.width_full().align_items(Align::Stretch).gap(18.0);
            ui.column(|ui| {
                ui.grow(1.55).gap(18.0);
                counter_card(ui, theme, handles);
                rows_card(ui, theme, handles);
            });
            ui.column(|ui| {
                ui.grow(1.0).gap(18.0);
                animation_card(ui, theme, handles);
                note_card(ui, theme);
            });
        });
    });
}

fn stat(ui: &mut Ui, theme: &Theme, kicker: &str, value: &str, detail: &str) {
    ui.column(|ui| {
        ui.grow(1.0)
            .padding(16.0)
            .gap(5.0)
            .background(theme.panel)
            .radius(14.0)
            .border(1.0, theme.border);
        ui.add(
            Label::new(kicker)
                .font_size(9.0)
                .color(theme.text_subtle)
                .family(FontFamily::Monospace),
        );
        ui.add(
            Label::new(value)
                .font_size(20.0)
                .color(theme.text)
                .weight(FontWeight::SEMIBOLD),
        );
        ui.add(Label::new(detail).font_size(11.0).color(theme.text_muted));
    });
}

fn card(ui: &mut Ui, theme: &Theme, title: &str, kicker: &str, body: impl FnOnce(&mut Ui)) {
    ui.column(|ui| {
        ui.width_full()
            .padding(18.0)
            .gap(12.0)
            .background(theme.panel)
            .radius(16.0)
            .border(1.0, theme.border);
        ui.row(|ui| {
            ui.width_full().align_items(Align::Center).gap(8.0);
            ui.add(
                Label::new(title)
                    .font_size(15.0)
                    .color(theme.text)
                    .weight(FontWeight::SEMIBOLD),
            );
            ui.spacer();
            ui.add(
                Label::new(kicker)
                    .font_size(9.0)
                    .color(theme.text_subtle)
                    .family(FontFamily::Monospace),
            );
        });
        body(ui);
    });
}

fn counter_card(ui: &mut Ui, theme: &Theme, handles: &Handles) {
    card(ui, theme, "State counter", "State<u32>", |ui| {
        ui.row(|ui| {
            ui.width_full().align_items(Align::Center).gap(10.0);
            ui.add(
                Label::new(handles.count.get().to_string())
                    .font_size(48.0)
                    .color(theme.text)
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
                .color(theme.text_muted),
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
                    .color(theme.text_subtle),
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
                        .background(theme.surface)
                        .radius(9.0);
                    ui.add(
                        Label::new(format!("row {id:02}"))
                            .font_size(12.0)
                            .color(theme.text_muted)
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
                    .color(theme.text_subtle)
                    .family(FontFamily::Monospace),
            );
        });
    });
}

fn animation_card(ui: &mut Ui, theme: &Theme, handles: &Handles) {
    card(ui, theme, "Node animation", "Ui::animate", |ui| {
        let target = handles.target.get();
        let value = ui.animate(target, Duration::from_millis(500));
        ui.row(|ui| {
            ui.width_full().align_items(Align::End);
            ui.add(
                Label::new(format!("{:.0}%", value * 100.0))
                    .font_size(32.0)
                    .color(theme.text)
                    .weight(FontWeight::SEMIBOLD),
            );
            ui.spacer();
            ui.add(
                Label::new("smoothstep")
                    .font_size(10.0)
                    .color(theme.text_subtle)
                    .family(FontFamily::Monospace),
            );
        });
        ui.column(|ui| {
            ui.width_full()
                .height(10.0)
                .radius(5.0)
                .background(theme.surface_hover);
            ui.row(|ui| {
                ui.width(Length::Percent(value * 100.0))
                    .height_full()
                    .radius(5.0)
                    .background(theme.accent);
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
                .color(theme.text_muted),
        );
        ui.column(|ui| {
            ui.width_full()
                .padding(10.0)
                .background(theme.background)
                .radius(9.0);
            ui.add(
                Label::new("fn view(ui: &mut Ui) {\n    let n = ui.state(|| 0i32);\n    ui.button(\"+\");\n}")
                    .font_size(10.5)
                    .color(theme.text_muted)
                    .family(FontFamily::Monospace),
            );
        });
    });
}

fn typography(ui: &mut Ui, theme: &Theme) {
    ui.column(|ui| {
        ui.width_full().gap(18.0);
        card(ui, theme, "Scale", "Inter / Noto Sans", |ui| {
            ui.add(
                Label::new("Immediate surface.")
                    .font_size(44.0)
                    .color(theme.text)
                    .weight(FontWeight::SEMIBOLD),
            );
            ui.add(
                Label::new("Retained core.")
                    .font_size(44.0)
                    .color(theme.accent)
                    .weight(FontWeight::LIGHT),
            );
            ui.add(
                Label::new("Every frame rebuilds a thin description. Shaping is cached by text, attributes and available width; glyphs land in one atlas and every primitive shares one instanced draw call.")
                    .font_size(14.0)
                    .color(theme.text_muted)
                    .line_height(22.0)
                    .width(Length::Percent(78.0)),
            );
        });

        ui.row(|ui| {
            ui.width_full().gap(18.0).align_items(Align::Stretch);
            ui.column(|ui| {
                ui.grow(1.0);
                card(ui, theme, "CJK fallback", "fontdb fallback", |ui| {
                    ui.add(
                        Label::new("即时为面，保留为骨。")
                            .font_size(24.0)
                            .color(theme.text),
                    );
                    ui.add(
                        Label::new("字体回退、行高与换行由文本层处理，用户代码只写字符串。")
                            .font_size(13.0)
                            .color(theme.text_muted),
                    );
                });
            });
            ui.column(|ui| {
                ui.grow(1.0);
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
                                .color(theme.text)
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
                    .color(theme.success)
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
                        .background(theme.background)
                        .radius(9.0);
                    for index in 0..3 {
                        ui.column(|ui| {
                            ui.width(48.0)
                                .height(26.0)
                                .radius(6.0)
                                .background(if index == 0 {
                                    theme.accent
                                } else {
                                    theme.surface_hover
                                });
                        });
                    }
                });
            }
        });

        ui.row(|ui| {
            ui.width_full().gap(18.0).align_items(Align::Stretch);
            ui.column(|ui| {
                ui.grow(1.0);
                card(ui, theme, "Column gap", "flex", |ui| {
                    for index in 0..4 {
                        ui.row(|ui| {
                            ui.width_full().height(22.0).radius(5.0).background(
                                if index % 2 == 0 {
                                    theme.surface
                                } else {
                                    theme.surface_hover
                                },
                            );
                        });
                    }
                });
            });
            ui.column(|ui| {
                ui.grow(1.0);
                card(ui, theme, "Alignment", "align_items", |ui| {
                    for align in [Align::Start, Align::Center, Align::End] {
                        ui.row(|ui| {
                            ui.width_full()
                                .height(38.0)
                                .align_items(align)
                                .background(theme.background)
                                .radius(7.0);
                            ui.row(|ui| {
                                ui.width(70.0)
                                    .height(16.0)
                                    .radius(4.0)
                                    .background(theme.accent);
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
            ui.grow(1.25);
            card(ui, theme, "Text input", "State<String>", |ui| {
                ui.add(Label::new("Name").font_size(11.0).color(theme.text_muted));
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
                            .color(theme.text_subtle)
                            .family(FontFamily::Monospace),
                    );
                });

                let greeting = handles.greeting.get();
                if greeting.is_empty() {
                    ui.add(
                        Label::new("Click the field and type. The value is a State<String>.")
                            .font_size(12.0)
                            .color(theme.text_muted),
                    );
                } else {
                    ui.add(
                        Label::new(greeting)
                            .font_size(24.0)
                            .color(theme.accent)
                            .weight(FontWeight::SEMIBOLD),
                    );
                }
            });
        });

        ui.column(|ui| {
            ui.grow(1.0);
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
                            .color(theme.text_muted),
                    );
                }
                ui.spacer();
                ui.add(Button::new("Disabled").enabled(false));
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
                ui.grow(1.15).gap(18.0);
                card(ui, theme, "Rich text editing", "selection + clipboard", |ui| {
                    ui.add(
                        Label::new("Mouse drag to select · Ctrl+A/C/X/V · Ctrl+Z / Ctrl+Shift+Z")
                            .font_size(10.5)
                            .color(theme.text_subtle)
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
                            .color(theme.text_subtle)
                            .family(FontFamily::Monospace),
                        );
                    });
                });

                card(ui, theme, "Smooth view scrolling", "wheel + spring", |ui| {
                    ui.add(
                        Label::new("Scroll the list. Wheel input accumulates into a critically damped spring; dragging the scrollbar jumps directly.")
                            .font_size(11.0)
                            .color(theme.text_muted),
                    );
                    ui.scroll(
                        ScrollView::vertical()
                            .size(Length::Percent(100.0), 196.0)
                            .smooth(true),
                        |ui| {
                            ui.width_full().gap(7.0);
                            for index in 0..18 {
                                ui.row(|ui| {
                                    ui.width_full()
                                        .height(34.0)
                                        .padding_xy(10.0, 0.0)
                                        .align_items(Align::Center)
                                        .background(if index % 2 == 0 {
                                            theme.surface
                                        } else {
                                            theme.panel
                                        })
                                        .radius(7.0);
                                    ui.add(
                                        Label::new(format!("scrollable row {:02}", index + 1))
                                            .font_size(11.0)
                                            .color(theme.text_muted)
                                            .family(FontFamily::Monospace),
                                    );
                                    ui.spacer();
                                    ui.add(
                                        Label::new(if index % 3 == 0 { "cached" } else { "retained" })
                                            .font_size(9.0)
                                            .color(theme.text_subtle),
                                    );
                                });
                            }
                        },
                    );
                });
            });

            ui.column(|ui| {
                ui.grow(1.0).gap(18.0);
                card(ui, theme, "Choices", "checkbox · radio · combo", |ui| {
                    ui.checkbox("Enable notifications", &handles.notifications);
                    ui.divider();
                    ui.add(
                        Label::new("Plan")
                            .font_size(10.0)
                            .color(theme.text_subtle)
                            .family(FontFamily::Monospace),
                    );
                    ui.row(|ui| {
                        ui.width_full().gap(16.0);
                        ui.radio("Free", 0usize, &handles.plan);
                        ui.radio("Team", 1usize, &handles.plan);
                        ui.radio("Studio", 2usize, &handles.plan);
                    });
                    ui.combo_box(
                        ["Automatic", "Always smooth", "Reduced motion"],
                        &handles.combo,
                    );
                });

                card(ui, theme, "Values", "slider · progress", |ui| {
                    ui.row(|ui| {
                        ui.width_full().align_items(Align::Center);
                        ui.add(
                            Label::new("Volume")
                                .font_size(12.0)
                                .color(theme.text_muted),
                        );
                        ui.spacer();
                        ui.add(
                            Label::new(format!("{:.0}%", handles.volume.get() * 100.0))
                                .font_size(11.0)
                                .color(theme.accent)
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
                            .color(theme.text_muted),
                    );
                    ui.progress(&handles.progress);
                });

                card(ui, theme, "Context menu", "right click", |ui| {
                    let trigger = ui.add(
                        Button::new("Right click for actions")
                            .subtle()
                            .full_width(),
                    );
                    ui.tooltip(&trigger, "The menu and tooltip both use root overlays.");
                    ui.context_menu(&trigger, |ui| {
                        if ui.add(Button::new("Reset volume").subtle().full_width()).clicked() {
                            handles.volume.set(0.5);
                        }
                        if ui.add(Button::new("Complete progress").subtle().full_width()).clicked() {
                            handles.progress.set(1.0);
                        }
                        ui.divider();
                        if ui.add(Button::new("Toggle notifications").subtle().full_width()).clicked() {
                            handles.notifications.update(|value| *value = !*value);
                        }
                    });
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
                        .background(theme.panel)
                        .border(1.0, theme.border)
                        .radius(10.0);
                    ui.add(
                        Label::new(title)
                            .font_size(9.0)
                            .color(theme.accent)
                            .family(FontFamily::Monospace),
                    );
                    ui.add(
                        Label::new(value)
                            .font_size(15.0)
                            .color(theme.text)
                            .semibold(),
                    );
                    ui.add(Label::new(detail).font_size(10.0).color(theme.text_subtle));
                });
            }
        });

        card(ui, theme, "Media and Markdown", "image · SVG · QR · markdown", |ui| {
            ui.row(|ui| {
                ui.width_full().gap(16.0).align_items(Align::Center);
                ui.add(
                    Image::from_rgba8(64, 64, gradient_pixels())
                        .fit(ImageFit::Cover)
                        .size(108.0, 108.0)
                        .radius(12.0),
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
                        .colors(theme.text, theme.panel),
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
