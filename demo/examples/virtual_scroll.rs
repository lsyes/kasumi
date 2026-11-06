//! `virtual_scroll` — the lowest-level virtualization primitive.
//!
//! ```sh
//! cargo run -p kasumi-demo --example virtual_scroll
//! ```
//!
//! `ListView` handles fixed-height rows. When rows vary in height — a timeline,
//! a chat log, a document with headings — use `Ui::virtual_scroll` directly:
//!
//! 1. Declare the full content extent with `ScrollView::virtual_content(..)`.
//! 2. The closure receives a `ScrollWindow` with the retained offset, the
//!    viewport measured on the previous frame, and an overscan margin.
//! 3. Declare only the children that intersect that window, positioned with
//!    `ui.absolute(0.0, y)`. Wrap each in `ui.keyed(id, ..)` so it keeps its
//!    identity while scrolling.
//!
//! This example uses variable row heights and shows the visible index range,
//! the number of declared rows and the total node count from frame stats, so
//! the win is measurable.

use kasumi::App;
use kasumi::prelude::*;
use kasumi_backend::run;

#[path = "shared/mod.rs"]
mod shared;

use shared::{code, heading, micro, note, options};

const ROWS: usize = 4_000;

/// A deterministic, variable height for each row: 44, 62, 88 or 36 px.
fn row_height(index: usize) -> f32 {
    match index % 4 {
        0 => 44.0,
        1 => 62.0,
        2 => 88.0,
        _ => 36.0,
    }
}

/// Prefix sum of row heights, built once. The index range for a scroll window
/// is then a binary search rather than a full scan.
fn offsets() -> &'static Vec<f32> {
    static OFFSETS: std::sync::OnceLock<Vec<f32>> = std::sync::OnceLock::new();
    OFFSETS.get_or_init(|| {
        let mut out = Vec::with_capacity(ROWS + 1);
        out.push(0.0);
        for index in 0..ROWS {
            out.push(out[index] + row_height(index));
        }
        out
    })
}

/// First and last row intersecting `[top, bottom)`, with overscan.
fn visible_range(top: f32, bottom: f32) -> std::ops::Range<usize> {
    let offsets = offsets();
    let start = offsets.partition_point(|value| *value < top);
    let end = offsets.partition_point(|value| *value < bottom);
    let start = start.saturating_sub(1).min(ROWS);
    let end = (end + 2).min(ROWS);
    start..end.max(start)
}

struct VirtualScroll;

impl App for VirtualScroll {
    fn view(&mut self, ui: &mut Ui) {
        let theme = ui.theme().clone();
        let total_height = *offsets().last().unwrap_or(&0.0);
        let declared = ui.state(|| 0usize);
        let range_label = ui.state(|| "—".to_owned());

        shared::page(
            ui,
            "Virtual scroll",
            "ScrollView::virtual_content · Ui::virtual_scroll",
            "virtual_scroll",
            |ui| {
                ui.card(
                    Card::elevated()
                        .padding(theme.spacing.lg)
                        .gap(theme.spacing.md),
                    |ui| {
                        heading(ui, "Variable-height rows", "4,000 rows");
                        note(
                            ui,
                            "Each row has a different height (44 / 62 / 88 / 36 px). That is why \
                             `ListView` is not enough here: the closure decides what to declare \
                             from a prefix-sum table of row offsets.",
                        );
                        ui.row(|ui| {
                            ui.width_full().gap(10.0).align_items(Align::Center);
                            micro(ui, format!("total height {total_height:.0} px"));
                            ui.spacer();
                            code(ui, range_label.get());
                        });
                    },
                );

                ui.card(
                    Card::outlined()
                        .padding(theme.spacing.lg)
                        .gap(theme.spacing.md),
                    |ui| {
                        heading(ui, "Declared window", "ScrollWindow");
                        let mut count = 0usize;
                        let mut label = "—".to_owned();
                        let view = ScrollView::vertical()
                            .size(Length::Percent(100.0), 420.0)
                            .overscan(120.0)
                            .virtual_content(Size::new(0.0, total_height));
                        ui.virtual_scroll(view, |ui, window| {
                            let top = (window.offset.y - window.overscan).max(0.0);
                            let bottom = window.offset.y + window.viewport.height + window.overscan;
                            let range = visible_range(top, bottom);
                            label = format!(
                                "rows {}..{} · offset {:.0} · viewport {:.0}",
                                range.start, range.end, window.offset.y, window.viewport.height
                            );
                            for index in range.clone() {
                                count += 1;
                                let y = offsets()[index];
                                let height = row_height(index);
                                ui.keyed(index, |ui| {
                                    ui.column(|ui| {
                                        ui.absolute(0.0, y)
                                            .width(Length::Percent(100.0))
                                            .height(height - 4.0)
                                            .padding_xy(12.0, 0.0)
                                            .gap(2.0)
                                            .justify(Justify::Center)
                                            .radius(theme.shape.extra_small)
                                            .background(if index % 2 == 0 {
                                                theme.colors.surface_container_low
                                            } else {
                                                theme.colors.surface_container
                                            });
                                        ui.add(
                                            Label::new(format!("row {index:05}"))
                                                .font_size(12.0)
                                                .color(theme.colors.on_surface)
                                                .family(FontFamily::Monospace),
                                        );
                                        if height >= 62.0 {
                                            ui.add(
                                                Label::new(format!(
                                                    "y = {y:.0} · height = {height:.0} · \
                                                     only visible rows are declared"
                                                ))
                                                .font_size(10.0)
                                                .color(theme.colors.on_surface_variant),
                                            );
                                        }
                                    });
                                });
                            }
                        });
                        if declared.get() != count {
                            declared.set(count);
                        }
                        if range_label.get() != label {
                            range_label.set(label);
                        }
                        ui.row(|ui| {
                            ui.width_full().gap(10.0).align_items(Align::Center);
                            micro(ui, format!("{} rows declared this frame", declared.get()));
                            ui.spacer();
                            code(ui, format!("of {ROWS} total"));
                        });
                    },
                );

                ui.card(
                    Card::filled()
                        .padding(theme.spacing.lg)
                        .gap(theme.spacing.md),
                    |ui| {
                        heading(ui, "Why the overscan matters", "one-frame latency");
                        note(
                            ui,
                            "The viewport size comes from the previous layout pass. Rendering \
                             extra rows above and below hides that one-frame delay on fast \
                             scrolls. The scroll offset itself is retained on the node, so no \
                             application state is needed to remember where the user is.",
                        );
                        for line in [
                            "virtual_content(Size) declares the full extent",
                            "row_range(row_height, count) for fixed-height rows",
                            "absolute(0, y) positions each declared row",
                            "keyed(index, ..) preserves identity while scrolling",
                        ] {
                            code(ui, format!("· {line}"));
                        }
                    },
                );
            },
        );
    }
}

fn main() -> anyhow::Result<()> {
    run(
        shared::QuitOnEscape { app: VirtualScroll },
        options("Kasumi — virtual scroll", 1080.0, 820.0),
    )
}
