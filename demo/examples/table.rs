//! `table` — a data table with a sticky header and virtualized rows.
//!
//! ```sh
//! cargo run -p kasumi-demo --example table
//! ```
//!
//! The framework has no `Table` widget. A table is a grid of rows, and that is
//! exactly how to build one: a header row, then a `ListView` whose row closure
//! declares a row of fixed-width columns. Because `ListView` virtualizes, a
//! 50,000-row table costs the same as a 20-row one.
//!
//! Sorting works by rebuilding an index vector. The rows themselves are pure
//! data in a `OnceLock`, so sorting never allocates strings: the index order
//! is the only state that changes.

use std::sync::OnceLock;

use kasumi::App;
use kasumi::prelude::*;
use kasumi_backend::run;

#[path = "shared/mod.rs"]
mod shared;

use shared::{code, heading, micro, note, options};

const COLUMNS: [(&str, f32); 5] = [
    ("ID", 70.0),
    ("Name", 220.0),
    ("Role", 150.0),
    ("Score", 100.0),
    ("Status", 110.0),
];

#[derive(Clone)]
struct Record {
    id: u32,
    name: String,
    role: String,
    score: f32,
    status: &'static str,
}

static RECORDS: OnceLock<Vec<Record>> = OnceLock::new();

fn records() -> &'static [Record] {
    RECORDS.get_or_init(|| {
        let roles = ["Engineer", "Designer", "Analyst", "Manager", "Support"];
        let statuses = ["Active", "Active", "Review", "Active", "Paused"];
        let first = [
            "Aoi", "Ren", "Mika", "Sora", "Yuki", "Haru", "Nao", "Rin", "Kai", "Mei",
        ];
        let last = [
            "Tanaka",
            "Sato",
            "Suzuki",
            "Watanabe",
            "Ito",
            "Yamamoto",
            "Nakamura",
            "Kobayashi",
            "Kato",
            "Yoshida",
        ];
        (0..50_000)
            .map(|index| Record {
                id: 10_000 + index as u32,
                name: format!(
                    "{} {}",
                    first[index as usize % first.len()],
                    last[(index as usize / first.len()) % last.len()]
                ),
                role: roles[index as usize % roles.len()].to_owned(),
                score: series(index as u32, 1)[0],
                status: statuses[index as usize % statuses.len()],
            })
            .collect()
    })
}

fn series(seed: u32, count: usize) -> Vec<f32> {
    (0..count)
        .map(|index| {
            let x = (seed * 7_381 + index as u32 * 911) % 1000;
            x as f32 / 1000.0
        })
        .collect()
}

/// One row in a uniform grid. Declared inside `ListView`, so it only exists
/// while the row is visible.
fn table_row(ui: &mut Ui, record: &Record) {
    let theme = ui.theme().clone();
    let score_color = if record.score >= 0.8 {
        theme.colors.success
    } else if record.score >= 0.5 {
        theme.colors.primary
    } else {
        theme.colors.error
    };
    ui.row(|ui| {
        ui.width_full().height_full().align_items(Align::Center);
        for (column, (_, width)) in COLUMNS.iter().enumerate() {
            ui.row(|ui| {
                ui.width(*width)
                    .height_full()
                    .flex_shrink(0.0)
                    .padding_xy(10.0, 0.0)
                    .align_items(Align::Center);
                let text = match column {
                    0 => format!("{}", record.id),
                    1 => record.name.clone(),
                    2 => record.role.clone(),
                    3 => format!("{:.2}", record.score),
                    _ => record.status.to_owned(),
                };
                ui.add(
                    Label::new(text)
                        .font_size(11.5)
                        .color(if column == 3 {
                            score_color
                        } else if column == 0 {
                            theme.colors.outline
                        } else {
                            theme.colors.on_surface
                        })
                        .family(if column == 0 {
                            FontFamily::Monospace
                        } else {
                            FontFamily::SansSerif
                        })
                        .wrap(TextWrap::None),
                );
            });
        }
    });
}

struct Table;

impl App for Table {
    fn view(&mut self, ui: &mut Ui) {
        let theme = ui.theme().clone();
        let sort = ui.state(|| 0u32);
        let filter = ui.state(String::new);

        let rows = records();
        let needle = filter.get().to_lowercase();
        let mut indices: Vec<usize> = (0..rows.len())
            .filter(|index| {
                needle.is_empty()
                    || rows[*index].name.to_lowercase().contains(&needle)
                    || rows[*index].role.to_lowercase().contains(&needle)
            })
            .collect();

        match sort.get() {
            0 => {}
            1 => indices.sort_by(|a, b| rows[*a].name.cmp(&rows[*b].name)),
            2 => indices.sort_by(|a, b| rows[*a].role.cmp(&rows[*b].role)),
            _ => indices.sort_by(|a, b| rows[*b].score.total_cmp(&rows[*a].score)),
        }

        shared::page(
            ui,
            "Data table",
            "sticky header · 50,000 rows · ListView virtualization",
            "table",
            |ui| {
                ui.card(
                    Card::elevated()
                        .padding(theme.spacing.lg)
                        .gap(theme.spacing.md),
                    |ui| {
                        heading(ui, "People", "a table is a grid of rows");
                        note(
                            ui,
                            "There is no `Table` widget: a header row plus a `ListView` of \
                             fixed-width rows is the whole thing. Filtering and sorting rebuild \
                             an index vector, not the 50,000 records.",
                        );

                        ui.add(
                            TextInput::new(&filter)
                                .placeholder("Filter by name or role…")
                                .width(Length::Percent(100.0)),
                        );
                        ui.row(|ui| {
                            ui.width_full().gap(8.0).align_items(Align::Center);
                            micro(ui, "sort by");
                            for (index, label) in ["ID", "Name", "Role", "Score"].iter().enumerate()
                            {
                                let active = sort.get() == index as u32;
                                let mut button =
                                    Button::new(*label).font_size(10.5).padding_xy(10.0, 5.0);
                                button = if active {
                                    button.tonal()
                                } else {
                                    button.subtle()
                                };
                                if ui.add(button).clicked() {
                                    sort.set(index as u32);
                                }
                            }
                            ui.spacer();
                        });

                        ui.row(|ui| {
                            ui.width_full().gap(10.0).align_items(Align::Center);
                            micro(ui, format!("{} / {} rows", indices.len(), rows.len()));
                            ui.spacer();
                            code(ui, "virtualized · flat node count");
                        });

                        // Sticky header: a plain row outside the scroll view.
                        ui.row(|ui| {
                            ui.width_full()
                                .height(38.0)
                                .align_items(Align::Center)
                                .background(theme.colors.primary_container)
                                .radius(theme.shape.extra_small);
                            for (label, width) in COLUMNS {
                                ui.row(|ui| {
                                    ui.width(width)
                                        .height_full()
                                        .flex_shrink(0.0)
                                        .padding_xy(10.0, 0.0)
                                        .align_items(Align::Center);
                                    ui.add(
                                        Label::new(label)
                                            .font_size(10.0)
                                            .color(theme.colors.on_primary_container)
                                            .semibold()
                                            .wrap(TextWrap::None),
                                    );
                                });
                            }
                        });

                        let visible = indices.clone();
                        if visible.is_empty() {
                            ui.column(|ui| {
                                ui.width_full()
                                    .height(120.0)
                                    .align_items(Align::Center)
                                    .justify(Justify::Center)
                                    .radius(theme.shape.small)
                                    .background(theme.colors.surface_container_low);
                                ui.add(
                                    Label::new("No matching rows")
                                        .font_size(13.0)
                                        .color(theme.colors.outline),
                                );
                            });
                        } else {
                            ui.add(
                                ListView::new(visible.len(), 34.0, |ui, row| {
                                    ui.column(|ui| {
                                        ui.size_full().background(if row % 2 == 0 {
                                            Color::TRANSPARENT
                                        } else {
                                            theme.colors.surface_container_low.fade(0.55)
                                        });
                                        table_row(ui, &rows[visible[row]]);
                                    });
                                })
                                .size(Length::Percent(100.0), 360.0),
                            );
                        }
                    },
                );
            },
        );
    }
}

fn main() -> anyhow::Result<()> {
    run(
        shared::QuitOnEscape { app: Table },
        options("Kasumi — data table", 1180.0, 740.0),
    )
}
