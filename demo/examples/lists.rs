//! `lists` — Material lists, 10k-row virtualization and a hand-built list.
//!
//! ```sh
//! cargo run -p kasumi-demo --example lists
//! ```
//!
//! Three levels of abstraction, from highest to lowest:
//!
//! | API | Declares | Use when |
//! |---|---|---|
//! | `List` | rows, headers, dividers | the content is known and short |
//! | `List::virtualize(true)` | only visible rows | the content is long and uniform |
//! | `ListView` | only visible rows, fixed height | you control the row layout |
//! | `ScrollView::virtual_content` + `Ui::virtual_scroll` | only what you declare | rows vary in height |
//!
//! All of them share the same retained scroll state, the same eased wheel
//! animation and the same scrollbar.

use std::sync::OnceLock;

use kasumi::App;
use kasumi::prelude::*;
use kasumi_backend::run;

#[path = "shared/mod.rs"]
mod shared;

use shared::{code, heading, micro, note, options};

/// The 10,000-row list is built once. Rebuilding it every frame would spend
/// more time formatting row titles than rendering the scene.
static BIG_LIST: OnceLock<List> = OnceLock::new();

fn big_list() -> &'static List {
    BIG_LIST.get_or_init(|| {
        let mut list = List::new().virtualize(true).overscan(320.0);
        for index in 0..10_000 {
            list =
                list.row(ListRow::new(format!("virtual row {:05}", index + 1)).leading_icon("·"));
        }
        list.size(Length::Percent(100.0), 240.0)
    })
}

/// Titles for the hand-built `ListView`, generated once.
fn palette() -> &'static [String] {
    static NAMES: OnceLock<Vec<String>> = OnceLock::new();
    NAMES.get_or_init(|| {
        (0..2_000)
            .map(|index| {
                let hue = index * 37 % 360;
                format!("swatch {index:04} · hue {hue:03}°")
            })
            .collect()
    })
}

struct Lists;

impl App for Lists {
    fn view(&mut self, ui: &mut Ui) {
        let theme = ui.theme().clone();
        let selected = ui.state(|| 0usize);

        shared::page(
            ui,
            "Lists and scrolling",
            "List · virtualization · ListView · ScrollView",
            "lists",
            |ui| {
                ui.row(|ui| {
                    ui.width_full().gap(16.0).align_items(Align::Stretch);

                    // -------------------------------------------------------
                    // Material list
                    // -------------------------------------------------------
                    ui.column(|ui| {
                        ui.grow(1.0).basis(0.0);
                        ui.card(
                            Card::elevated()
                                .padding(theme.spacing.lg)
                                .gap(theme.spacing.md),
                            |ui| {
                                heading(ui, "Material list", "List · ListRow");
                                note(
                                    ui,
                                    "Rows follow the M3 tokens: 56, 72 and 88 dp for one, two \
                                     and three lines, a 16 dp inset, leading icons or avatars, \
                                     and a secondary-container selection.",
                                );
                                let list = List::new()
                                    .header("This week")
                                    .row(
                                        ListRow::new("Design review")
                                            .supporting("Today, 09:30 · 4 comments")
                                            .leading_avatar("DR")
                                            .trailing("09:30")
                                            .selected(selected.get() == 0),
                                    )
                                    .divider()
                                    .row(
                                        ListRow::new("Deploy pipeline")
                                            .supporting("main · 3 commits ahead")
                                            .leading_icon("↑")
                                            .trailing_icon("›")
                                            .selected(selected.get() == 1),
                                    )
                                    .row(
                                        ListRow::new("Notifications")
                                            .leading_icon("◔")
                                            .trailing_icon("›")
                                            .selected(selected.get() == 2),
                                    )
                                    .row(
                                        ListRow::new("Release notes")
                                            .supporting(
                                                "Every milestone is a commit: the menu, the \
                                                 drawer, the list and the rest of the M3 \
                                                 surface land separately.",
                                            )
                                            .height(ListRowHeight::ThreeLine)
                                            .leading_icon("▤")
                                            .selected(selected.get() == 3),
                                    )
                                    .divider()
                                    .row(
                                        ListRow::new("Disabled row")
                                            .supporting("enabled(false)")
                                            .leading_icon("✗")
                                            .enabled(false),
                                    );
                                if let Some(index) = ui.list(&list).clicked {
                                    selected.set(index);
                                }
                                code(ui, format!("selected row = {}", selected.get()));
                            },
                        );
                    });

                    // -------------------------------------------------------
                    // ListView
                    // -------------------------------------------------------
                    ui.column(|ui| {
                        ui.grow(1.0).basis(0.0);
                        ui.card(
                            Card::outlined()
                                .padding(theme.spacing.lg)
                                .gap(theme.spacing.md),
                            |ui| {
                                heading(ui, "ListView", "fixed-height rows");
                                note(
                                    ui,
                                    "`ListView` declares only the rows intersecting the visible \
                                     window plus an overscan. Row identity comes from the index, \
                                     so scrolling does not recreate widgets that stay visible.",
                                );
                                let names = palette();
                                let stripe = theme.colors.surface_container_low.fade(0.5);
                                ui.add(
                                    ListView::new(names.len(), 34.0, |ui, index| {
                                        ui.row(|ui| {
                                            ui.size_full()
                                                .padding_xy(12.0, 0.0)
                                                .align_items(Align::Center)
                                                .gap(10.0)
                                                .background(if index % 2 == 0 {
                                                    Color::TRANSPARENT
                                                } else {
                                                    stripe
                                                });
                                            ui.column(|ui| {
                                                ui.width(10.0).height(10.0).radius(5.0).background(
                                                    Color::rgb8(
                                                        (index * 13 % 256) as u8,
                                                        (index * 29 % 256) as u8,
                                                        (index * 47 % 256) as u8,
                                                    ),
                                                );
                                            });
                                            ui.add(
                                                Label::new(&names[index])
                                                    .font_size(12.0)
                                                    .color(ui.theme().colors.on_surface),
                                            );
                                        });
                                    })
                                    .size(Length::Percent(100.0), 280.0),
                                );
                                code(ui, format!("{} rows · 34 px each", names.len()));
                            },
                        );
                    });
                });

                // -----------------------------------------------------------
                // Virtualized Material list
                // -----------------------------------------------------------
                ui.card(
                    Card::elevated()
                        .padding(theme.spacing.lg)
                        .gap(theme.spacing.md),
                    |ui| {
                        heading(ui, "10,000 rows", "List::virtualize");
                        note(
                            ui,
                            "The list value is built once in a `OnceLock` and reused every \
                             frame. Virtualization keeps the node count flat no matter how far \
                             you scroll, which is what makes 10k rows affordable.",
                        );
                        ui.list(big_list());
                        ui.row(|ui| {
                            ui.width_full().gap(8.0).align_items(Align::Center);
                            micro(ui, format!("{} rows", big_list().row_count()));
                            ui.spacer();
                            micro(
                                ui,
                                format!("{:.0} px of virtual content", big_list().content_height()),
                            );
                        });
                    },
                );

                // -----------------------------------------------------------
                // Raw scroll views
                // -----------------------------------------------------------
                ui.row(|ui| {
                    ui.width_full().gap(16.0).align_items(Align::Stretch);

                    ui.column(|ui| {
                        ui.grow(1.0).basis(0.0);
                        ui.card(
                            Card::filled()
                                .padding(theme.spacing.lg)
                                .gap(theme.spacing.md),
                            |ui| {
                                heading(ui, "Horizontal scroll", "ScrollView::horizontal");
                                note(
                                    ui,
                                    "Wheel and trackpad deltas are normalized into Chromium's \
                                     eased animation, so a horizontal strip scrolls with the \
                                     same feel as a vertical one.",
                                );
                                ui.scroll(
                                    ScrollView::horizontal()
                                        .size(Length::Percent(100.0), 96.0)
                                        .scrollbars(false),
                                    |ui| {
                                        ui.row(|ui| {
                                            ui.height_full().gap(10.0).padding_xy(2.0, 0.0);
                                            for index in 0..14 {
                                                ui.column(|ui| {
                                                    ui.width(96.0)
                                                        .height(92.0)
                                                        .align_items(Align::Center)
                                                        .justify(Justify::Center)
                                                        .gap(4.0)
                                                        .radius(theme.shape.small)
                                                        .background(theme.colors
                                                            .surface_container_high);
                                                    ui.add(
                                                        Label::new(format!("{index:02}"))
                                                            .font_size(20.0)
                                                            .color(theme.colors.on_surface)
                                                            .weight(FontWeight::SEMIBOLD),
                                                    );
                                                    micro(ui, "card");
                                                });
                                            }
                                        });
                                    },
                                );
                                micro(ui, "shift+wheel or drag the bar · hide_scrollbars() removes the rail");
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
                                heading(ui, "Bidirectional", "ScrollView::both");
                                note(
                                    ui,
                                    "Both axes at once. The content keeps its natural size and \
                                     simply overflows; the viewport is the only thing that clips.",
                                );
                                ui.scroll(
                                    ScrollView::both()
                                        .size(Length::Percent(100.0), 180.0),
                                    |ui| {
                                        ui.grid(24, |ui| {
                                            ui.gap(4.0).padding(2.0);
                                            for index in 0..24 * 14 {
                                                let factor = (index % 24) as f32 / 23.0;
                                                let tint = (index / 24) as f32 / 13.0;
                                                ui.column(|ui| {
                                                    ui.width(30.0).height(30.0).radius(4.0).background(
                                                        theme
                                                            .colors
                                                            .primary
                                                            .lerp(theme.colors.tertiary, factor)
                                                            .fade(0.25 + tint * 0.75),
                                                    );
                                                });
                                            }
                                        });
                                    },
                                );
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
        shared::QuitOnEscape { app: Lists },
        options("Kasumi — lists", 1180.0, 900.0),
    )
}
