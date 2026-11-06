//! `search` — the Material search bar and the full-window search view.
//!
//! ```sh
//! cargo run -p kasumi-demo --example search
//! ```
//!
//! `SearchBar` is a pill-shaped field with a leading magnifier and an inline
//! results popup. Typing filters with a case-insensitive substring match over
//! both the title and the description; ↑/↓ move the highlight, Enter activates
//! it, Escape closes the popup and then the field.
//!
//! `SearchBar::view(true)` renders the same data as a full-window Material
//! *search view*: a surface-container-high sheet over the page with the field
//! docked at the top. The view is a root overlay that swallows clicks behind
//! it, so it behaves like a modal without being a dialog.

use kasumi::App;
use kasumi::prelude::*;
use kasumi_backend::run;

#[path = "shared/mod.rs"]
mod shared;

use shared::{code, heading, micro, note, options};

/// The catalogue both search surfaces index.
const ENTRIES: [(&str, &str, &str); 16] = [
    ("Counter", "state · a counter bound to State<i32>", "◉"),
    ("Layout", "flex · grid, stack and wrapping", "▤"),
    ("Typography", "type · the M3 scale and fallback", "Aa"),
    ("Form", "input · validation and dialogs", "✎"),
    ("Controls", "buttons · switches, sliders, progress", "⚙"),
    ("Lists", "rows · Material list and virtualization", "▦"),
    ("Menus", "popup · nested submenus and context menus", "≡"),
    ("Navigation", "drawer · rail and collapsible groups", "☰"),
    ("Async", "resource · loaders on a worker thread", "⇅"),
    ("Media", "image · SVG, QR and Markdown", "▣"),
    ("Animation", "motion · tweens, springs and presence", "◐"),
    (
        "Windows",
        "multi-window · shared App, per-window state",
        "▢",
    ),
    ("Tray", "tray · StatusNotifierItem menu", "⊞"),
    ("Canvas", "paint · custom drawing with primitives", "◫"),
    ("Tables", "grid · header, rows and zebra striping", "▥"),
    ("Theme lab", "color · dynamic schemes from a seed", "◆"),
];

fn results() -> Vec<SearchResult> {
    ENTRIES
        .iter()
        .map(|(title, description, icon)| {
            SearchResult::new(*title)
                .description(*description)
                .icon(*icon)
        })
        .collect()
}

struct Search;

impl App for Search {
    fn view(&mut self, ui: &mut Ui) {
        let theme = ui.theme().clone();
        let query = ui.state(String::new);
        let view_open = ui.state(|| false);
        let activated = ui.state(|| None::<String>);
        let scoped_query = ui.state(String::new);

        shared::page(
            ui,
            "Search",
            "SearchBar · inline popup · full-window search view",
            "search",
            |ui| {
                ui.card(
                    Card::elevated()
                        .padding(theme.spacing.lg)
                        .gap(theme.spacing.md),
                    |ui| {
                        heading(ui, "Search bar", "SearchBar::new");
                        note(
                            ui,
                            "Click the pill and start typing. The panel filters as you type, \
                             ↑/↓ move the highlight, Enter activates and Escape closes the panel \
                             before it releases focus.",
                        );
                        ui.row(|ui| {
                            ui.width_full().gap(10.0).align_items(Align::Center);
                            let outcome = ui.search(
                                &SearchBar::new(&query)
                                    .placeholder("Search the gallery")
                                    .width(Length::Percent(100.0))
                                    .results(results()),
                            );
                            if let Some(title) = outcome.activated {
                                query.set(String::new());
                                activated.set(Some(title));
                            }
                        });
                        ui.row(|ui| {
                            ui.width_full().gap(10.0).align_items(Align::Center);
                            micro(ui, "last activation");
                            code(
                                ui,
                                activated.get().unwrap_or_else(|| "nothing yet".to_owned()),
                            );
                            ui.spacer();
                            micro(ui, format!("{} entries indexed", ENTRIES.len()));
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
                                heading(ui, "Search view", "SearchBar::view(true)");
                                note(
                                    ui,
                                    "The same catalogue as a modal search view: a \
                                     surface-container-high sheet covering the page, with the \
                                     field docked at the top and results as a full-height list.",
                                );
                                if ui
                                    .add(Button::new("Open search view").filled().full_width())
                                    .clicked()
                                {
                                    view_open.set(true);
                                }
                                note(
                                    ui,
                                    "Escape closes the view and returns focus to the page. \
                                     The sheet swallows clicks behind it without being a dialog.",
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
                                heading(ui, "A second, independent field", "one State per bar");
                                note(
                                    ui,
                                    "Search bars are not singletons. Each one owns a query state \
                                     and a retained popup session, so several can coexist on one \
                                     page without fighting over focus.",
                                );
                                ui.search(
                                    &SearchBar::new(&scoped_query)
                                        .placeholder("Filter widgets…")
                                        .clear_button(true)
                                        .width(Length::Percent(100.0))
                                        .results(
                                            results()
                                                .into_iter()
                                                .filter(|result| {
                                                    result
                                                        .description
                                                        .as_deref()
                                                        .is_some_and(|d| d.contains('·'))
                                                })
                                                .collect::<Vec<_>>(),
                                        ),
                                );
                                code(ui, format!("query = {:?}", scoped_query.get()));
                            },
                        );
                    });
                });

                ui.card(
                    Card::filled()
                        .padding(theme.spacing.lg)
                        .gap(theme.spacing.md),
                    |ui| {
                        heading(ui, "Keyboard and matching", "SearchResponse");
                        note(
                            ui,
                            "`SearchBar::results` takes any `IntoIterator<Item = SearchResult>`. \
                             Filtering is the widget's job; you supply the whole catalogue and \
                             receive the activated title.",
                        );
                        ui.grid(2, |ui| {
                            ui.width_full().gap(12.0);
                            for (key, detail) in [
                                ("↑ / ↓", "move the highlighted result"),
                                ("Enter", "activate the highlighted result"),
                                ("Escape", "close the panel, then the field"),
                                ("click outside", "dismiss the panel"),
                                (
                                    "typing",
                                    "case-insensitive substring of title or description",
                                ),
                                ("clear button", "shown when the query is non-empty"),
                            ] {
                                ui.row(|ui| {
                                    ui.width_full().gap(10.0);
                                    ui.column(|ui| {
                                        ui.width(96.0).flex_shrink(0.0);
                                        code(ui, key);
                                    });
                                    ui.add(
                                        Label::new(detail)
                                            .font_size(11.5)
                                            .color(theme.colors.on_surface_variant),
                                    );
                                });
                            }
                        });
                    },
                );
            },
        );

        // The full-window search view is a root overlay declared after the page.
        if view_open.get() {
            let outcome = ui.search(
                &SearchBar::new(&query)
                    .placeholder("Search the gallery")
                    .view(true)
                    .width(Length::Percent(100.0))
                    .results(results()),
            );
            if let Some(title) = outcome.activated {
                view_open.set(false);
                query.set(String::new());
                activated.set(Some(title));
            }
            // Clicking anywhere outside the sheet dismisses the view.
            if ui.key_pressed(&kasumi::core::Key::Escape) {
                view_open.set(false);
            }
        }
    }
}

fn main() -> anyhow::Result<()> {
    run(Search, options("Kasumi — search", 1120.0, 860.0))
}
