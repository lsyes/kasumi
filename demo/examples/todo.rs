//! `todo` — a small but complete application: add, filter, toggle, delete.
//!
//! ```sh
//! cargo run -p kasumi-demo --example todo
//! ```
//!
//! This is the example to copy when starting a real app. It exercises the
//! whole loop without any custom widgets:
//!
//! - a `State<Vec<Todo>>` as the single source of truth,
//! - `ui.keyed(id, ..)` so a reordered row keeps its own state,
//! - a `State<String>` for the input,
//! - a `State<Filter>` for the view,
//! - derived counts computed from the current value each frame.
//!
//! There is no controller, no model object and no callback queue. `view` reads
//! the state, declares the UI and writes the state when a `Response` says the
//! user did something.

use kasumi::App;
use kasumi::prelude::*;
use kasumi_backend::run;

#[path = "shared/mod.rs"]
mod shared;

use shared::{code, heading, micro, note, options};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Filter {
    All,
    Active,
    Done,
}

impl Filter {
    fn label(self) -> &'static str {
        match self {
            Filter::All => "All",
            Filter::Active => "Active",
            Filter::Done => "Done",
        }
    }
}

#[derive(Clone)]
struct Todo {
    id: u32,
    title: String,
    done: bool,
}

struct TodoApp;

impl App for TodoApp {
    fn view(&mut self, ui: &mut Ui) {
        let theme = ui.theme().clone();
        let todos = ui.state(|| {
            vec![
                Todo {
                    id: 0,
                    title: "Read the README".to_owned(),
                    done: true,
                },
                Todo {
                    id: 1,
                    title: "Run cargo run -p kasumi-demo --example todo".to_owned(),
                    done: false,
                },
                Todo {
                    id: 2,
                    title: "Delete the retained tree (mentally)".to_owned(),
                    done: false,
                },
            ]
        });
        let next_id = ui.state(|| 3u32);
        let draft = ui.state(String::new);
        let filter = ui.state(|| Filter::All);

        let all = todos.get();
        let done = all.iter().filter(|todo| todo.done).count();
        let active = all.len() - done;

        shared::page(
            ui,
            "Todo",
            "a whole application in one view function",
            "todo",
            |ui| {
                ui.row(|ui| {
                    ui.width_full().gap(12.0);
                    for (kicker, value, detail) in [
                        ("TOTAL", all.len().to_string(), "items"),
                        ("ACTIVE", active.to_string(), "left to do"),
                        ("DONE", done.to_string(), "completed"),
                    ] {
                        ui.column(|ui| {
                            ui.grow(1.0)
                                .basis(0.0)
                                .padding(14.0)
                                .gap(4.0)
                                .background(theme.colors.surface_container_low)
                                .border(1.0, theme.colors.outline_variant)
                                .radius(theme.shape.medium);
                            micro(ui, kicker);
                            ui.add(
                                Label::new(value)
                                    .font_size(24.0)
                                    .color(theme.colors.on_surface)
                                    .semibold(),
                            );
                            ui.add(
                                Label::new(detail)
                                    .font_size(10.5)
                                    .color(theme.colors.on_surface_variant),
                            );
                        });
                    }
                });

                ui.card(
                    Card::elevated()
                        .padding(theme.spacing.lg)
                        .gap(theme.spacing.md),
                    |ui| {
                        heading(ui, "Add a task", "State<Vec<Todo>>");
                        ui.row(|ui| {
                            ui.width_full().gap(8.0).align_items(Align::Center);
                            ui.add(
                                TextInput::new(&draft)
                                    .placeholder("What needs doing?")
                                    .width(Length::Percent(100.0)),
                            );
                            let can_add = !draft.get().trim().is_empty();
                            if ui
                                .add(Button::new("Add").primary().enabled(can_add))
                                .clicked()
                                || (can_add && ui.key_pressed(&kasumi::core::Key::Enter))
                            {
                                let id = next_id.get();
                                next_id.set(id + 1);
                                todos.update(|list| {
                                    list.push(Todo {
                                        id,
                                        title: draft.get().trim().to_owned(),
                                        done: false,
                                    });
                                });
                                draft.set(String::new());
                            }
                        });
                        note(
                            ui,
                            "Press Enter or click Add. The new row appears on the next frame \
                             because a state write requests a repaint.",
                        );
                    },
                );

                ui.card(
                    Card::outlined()
                        .padding(theme.spacing.lg)
                        .gap(theme.spacing.md),
                    |ui| {
                        ui.row(|ui| {
                            ui.width_full().gap(8.0).align_items(Align::Center);
                            heading(ui, "Tasks", "ui.keyed(id, ..)");
                            ui.spacer();
                            for candidate in [Filter::All, Filter::Active, Filter::Done] {
                                let active = filter.get() == candidate;
                                let mut button = Button::new(candidate.label())
                                    .font_size(10.5)
                                    .padding_xy(10.0, 5.0);
                                button = if active {
                                    button.tonal()
                                } else {
                                    button.subtle()
                                };
                                if ui.add(button).clicked() {
                                    filter.set(candidate);
                                }
                            }
                        });

                        let shown: Vec<Todo> = all
                            .iter()
                            .filter(|todo| match filter.get() {
                                Filter::All => true,
                                Filter::Active => !todo.done,
                                Filter::Done => todo.done,
                            })
                            .cloned()
                            .collect();

                        if shown.is_empty() {
                            ui.column(|ui| {
                                ui.width_full()
                                    .height(90.0)
                                    .align_items(Align::Center)
                                    .justify(Justify::Center)
                                    .radius(theme.shape.small)
                                    .background(theme.colors.surface_container_low);
                                ui.add(
                                    Label::new("Nothing here — change the filter or add a task.")
                                        .font_size(12.0)
                                        .color(theme.colors.outline),
                                );
                            });
                        }

                        for todo in &shown {
                            // `ui.keyed` gives this row its own state scope, so a
                            // `State<bool>` declared here survives reordering.
                            // The list in `todos` stays the single source of
                            // truth: we seed the checkbox from it and write the
                            // click back into it.
                            let done = todo.done;
                            let id = todo.id;
                            let title = todo.title.clone();
                            ui.keyed(id, |ui| {
                                let checked = ui.state(move || done);
                                if checked.get() != done {
                                    checked.set(done);
                                }
                                ui.row(|ui| {
                                    ui.width_full()
                                        .height(46.0)
                                        .padding_xy(12.0, 0.0)
                                        .gap(10.0)
                                        .align_items(Align::Center)
                                        .background(if done {
                                            theme.colors.surface_container_low
                                        } else {
                                            theme.colors.surface_container
                                        })
                                        .radius(theme.shape.small);
                                    if ui.add(Checkbox::new("", &checked)).clicked() {
                                        let next = checked.get();
                                        todos.update(|list| {
                                            if let Some(item) =
                                                list.iter_mut().find(|item| item.id == id)
                                            {
                                                item.done = next;
                                            }
                                        });
                                    }
                                    ui.add(
                                        Label::new(&title)
                                            .font_size(13.0)
                                            .color(if done {
                                                theme.colors.outline
                                            } else {
                                                theme.colors.on_surface
                                            })
                                            .wrap(TextWrap::None),
                                    );
                                    ui.spacer();
                                    if ui
                                        .add(
                                            Button::new("Delete")
                                                .subtle()
                                                .font_size(10.0)
                                                .padding_xy(8.0, 4.0),
                                        )
                                        .clicked()
                                    {
                                        todos.update(|list| list.retain(|item| item.id != id));
                                    }
                                });
                            });
                        }
                    },
                );

                ui.card(
                    Card::filled()
                        .padding(theme.spacing.lg)
                        .gap(theme.spacing.md),
                    |ui| {
                        heading(ui, "What persists", "reconciliation");
                        note(
                            ui,
                            "The list, the draft and the filter are three `State` handles. The \
                             rows themselves are recreated every frame and matched by their \
                             keyed id; deleting a row recycles its node at frame end.",
                        );
                        code(
                            ui,
                            format!(
                                "{} total · {} active · {} done · filter {}",
                                all.len(),
                                active,
                                done,
                                filter.get().label()
                            ),
                        );
                    },
                );
            },
        );
    }
}

fn main() -> anyhow::Result<()> {
    run(
        shared::QuitOnEscape { app: TodoApp },
        options("Kasumi — todo", 960.0, 820.0),
    )
}
