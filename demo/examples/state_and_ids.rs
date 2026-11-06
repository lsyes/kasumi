//! `state_and_ids` — state handles, keys, conditional branches and TTL.
//!
//! ```sh
//! cargo run -p kasumi-demo --example state_and_ids
//! ```
//!
//! This is the example to read when a value inexplicably resets.
//!
//! Kasumi derives element ids from the *position* of a declaration in the tree:
//! a child id is `mix(parent_scope, sibling_index)`. `ui.state(..)` uses a
//! similar positional slot. That makes the common case free, but it has one
//! consequence: adding, removing or reordering siblings shifts the slots of
//! everything after them.
//!
//! Use `ui.keyed(key, |ui| ..)` for lists and conditional branches. It replaces
//! the sibling-derived scope with a hash of the key, so a row keeps its state
//! when the rows around it move.
//!
//! State slots are evicted after 600 frames in which their declaring call did
//! not run. `ui.state_persistent(..)` opts out of that. Reading a value is *not*
//! what keeps it alive.

use kasumi::App;
use kasumi::prelude::*;
use kasumi_backend::run;

#[path = "shared/mod.rs"]
mod shared;

use shared::{code, heading, micro, note, options};

struct StateAndIds;

impl App for StateAndIds {
    fn view(&mut self, ui: &mut Ui) {
        let theme = ui.theme().clone();
        let show = ui.state(|| true);
        let rows = ui.state(|| vec![2u32, 0u32, 1u32]);
        let next_id = ui.state(|| 3u32);
        let branch_slider = ui.state(|| 0.35f32);
        let persistent = ui.state_persistent(|| "persistent".to_owned());
        let log = ui.state(Vec::<String>::new);

        shared::page(
            ui,
            "State and identity",
            "ui.state · state_persistent · keyed · TTL eviction",
            "state_and_ids",
            |ui| {
                ui.card(
                    Card::elevated()
                        .padding(theme.spacing.lg)
                        .gap(theme.spacing.md),
                    |ui| {
                        heading(ui, "Positional slots", "ui.state");
                        note(
                            ui,
                            "`ui.state(..)` is identified by its call site. Each call below has \
                             its own slot; writing one never affects the others. The handles are \
                             cheap to clone and can be captured by closures.",
                        );
                        let a = ui.state(|| 0i32);
                        let b = ui.state(|| 100i32);
                        let doubled = a.get() * 2;
                        ui.row(|ui| {
                            ui.width_full().gap(10.0).align_items(Align::Center);
                            if ui.add(Button::new("a += 1").primary()).clicked() {
                                a.update(|value| *value += 1);
                            }
                            if ui.add(Button::new("b += 10").tonal()).clicked() {
                                b.update(|value| *value += 10);
                            }
                            ui.spacer();
                            code(
                                ui,
                                format!("a = {} · b = {} · 2a = {}", a.get(), b.get(), doubled),
                            );
                        });
                        note(
                            ui,
                            "A `State<T>` write requests a repaint. The runtime also schedules \
                             one follow-up frame so widgets declared earlier in the same frame \
                             observe the new value.",
                        );
                    },
                );

                ui.row(|ui| {
                    ui.width_full().gap(16.0).align_items(Align::Stretch);

                    // ------------------------------------------------------
                    // Keyed rows
                    // ------------------------------------------------------
                    ui.column(|ui| {
                        ui.grow(1.4).basis(0.0);
                        ui.card(
                            Card::elevated()
                                .padding(theme.spacing.lg)
                                .gap(theme.spacing.md),
                            |ui| {
                                heading(ui, "Keyed rows", "ui.keyed(id, ..)");
                                note(
                                    ui,
                                    "Each row owns a counter declared inside `ui.keyed(id, ..)`. \
                                     Increment a few, then reorder or remove a row: the counters \
                                     stay with their ids, not with their positions.",
                                );
                                let ids = rows.get();
                                for id in &ids {
                                    ui.keyed(*id, |ui| {
                                        let count = ui.state(|| 0u32);
                                        ui.row(|ui| {
                                            ui.width_full()
                                                .height(38.0)
                                                .padding_xy(10.0, 0.0)
                                                .gap(8.0)
                                                .align_items(Align::Center)
                                                .background(theme.colors.surface_container)
                                                .radius(theme.shape.small);
                                            code(ui, format!("row {id:02}"));
                                            ui.add(
                                                Label::new(count.get().to_string())
                                                    .font_size(13.0)
                                                    .color(theme.colors.on_surface)
                                                    .family(FontFamily::Monospace),
                                            );
                                            ui.spacer();
                                            if ui
                                                .add(
                                                    Button::new("+")
                                                        .font_size(11.0)
                                                        .padding_xy(9.0, 3.0),
                                                )
                                                .clicked()
                                            {
                                                count.update(|value| *value += 1);
                                            }
                                            if ui
                                                .add(
                                                    Button::new("remove")
                                                        .subtle()
                                                        .font_size(10.0)
                                                        .padding_xy(8.0, 3.0),
                                                )
                                                .clicked()
                                            {
                                                rows.update(|list| {
                                                    list.retain(|candidate| candidate != id)
                                                });
                                            }
                                        });
                                    });
                                }
                                if ids.is_empty() {
                                    note(ui, "No rows left. Add one to try again.");
                                }
                                ui.divider();
                                ui.row(|ui| {
                                    ui.width_full().gap(8.0).align_items(Align::Center);
                                    if ui.add(Button::new("Add row").tonal()).clicked() {
                                        let id = next_id.get();
                                        next_id.set(id + 1);
                                        rows.update(|list| list.push(id));
                                    }
                                    if ui.add(Button::new("Rotate").subtle()).clicked() {
                                        rows.update(|list| {
                                            if !list.is_empty() {
                                                let first = list.remove(0);
                                                list.push(first);
                                            }
                                        });
                                    }
                                    if ui.add(Button::new("Reverse").subtle()).clicked() {
                                        rows.update(|list| list.reverse());
                                    }
                                    ui.spacer();
                                    micro(ui, format!("{} rows", ids.len()));
                                });
                            },
                        );
                    });

                    // ------------------------------------------------------
                    // Conditional branches & TTL
                    // ------------------------------------------------------
                    ui.column(|ui| {
                        ui.grow(1.0).basis(0.0).gap(16.0);
                        ui.card(
                            Card::outlined()
                                .padding(theme.spacing.lg)
                                .gap(theme.spacing.md),
                            |ui| {
                                heading(ui, "Conditional branches", "one key per branch");
                                note(
                                    ui,
                                    "Two branches declare different widgets at the same position. \
                                     Wrapping each in `ui.keyed` keeps their ids separate — \
                                     without it, debug builds panic on duplicate ids.",
                                );
                                ui.add(Switch::new(&show).label("Show the first branch"));
                                if show.get() {
                                    ui.keyed("branch-a", |ui| {
                                        ui.row(|ui| {
                                            ui.width_full()
                                                .padding(12.0)
                                                .gap(8.0)
                                                .align_items(Align::Center)
                                                .radius(theme.shape.small)
                                                .background(theme.colors.primary_container);
                                            ui.add(
                                                Label::new("Branch A declares a button here")
                                                    .font_size(12.0)
                                                    .color(theme.colors.on_primary_container),
                                            );
                                            let _ = ui.add(Button::new("A").subtle());
                                        });
                                    });
                                } else {
                                    ui.keyed("branch-b", |ui| {
                                        ui.row(|ui| {
                                            ui.width_full()
                                                .padding(12.0)
                                                .gap(8.0)
                                                .align_items(Align::Center)
                                                .radius(theme.shape.small)
                                                .background(theme.colors.tertiary_container);
                                            ui.add(
                                                Label::new("Branch B declares a slider instead")
                                                    .font_size(12.0)
                                                    .color(theme.colors.on_tertiary_container),
                                            );
                                            let _ = ui.add(
                                                Slider::new(&branch_slider, 0.0, 1.0).width(120.0),
                                            );
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
                                heading(ui, "Persistence", "state_persistent");
                                note(
                                    ui,
                                    "Normal state is evicted after 600 frames in which its \
                                     declaring call did not run. `state_persistent` opts out. \
                                     Lift anything that must survive a long-hidden subtree into \
                                     one.",
                                );
                                ui.row(|ui| {
                                    ui.width_full().gap(8.0).align_items(Align::Center);
                                    micro(ui, "persistent");
                                    ui.spacer();
                                    code(ui, persistent.get());
                                });
                                ui.row(|ui| {
                                    ui.width_full().gap(8.0).align_items(Align::Center);
                                    if ui
                                        .add(Button::new("Change").subtle().full_width())
                                        .clicked()
                                    {
                                        persistent.set(format!("edited at frame {}", ui.frame()));
                                        log.update(|lines| {
                                            lines.insert(0, "persistent state written".to_owned());
                                            lines.truncate(4);
                                        });
                                    }
                                });
                            },
                        );
                    });
                });

                ui.card(
                    Card::elevated()
                        .padding(theme.spacing.lg)
                        .gap(theme.spacing.md),
                    |ui| {
                        heading(ui, "Rules of thumb", "reconciliation");
                        let rules = [
                            ("Static children", "positional ids are fine"),
                            ("Reordered or filtered lists", "ui.keyed(stable_id, ..)"),
                            ("Conditional widget trees", "one key per branch"),
                            ("Values that must survive hiding", "ui.state_persistent(..)"),
                            (
                                "Two widgets producing the same id",
                                "debug builds panic by design",
                            ),
                            ("Reading a value", "does not keep its slot alive"),
                        ];
                        for (case, rule) in rules {
                            ui.row(|ui| {
                                ui.width_full().gap(12.0).align_items(Align::Center);
                                ui.column(|ui| {
                                    ui.width(250.0).flex_shrink(0.0);
                                    ui.add(
                                        Label::new(case)
                                            .font_size(11.5)
                                            .color(theme.colors.on_surface_variant),
                                    );
                                });
                                code(ui, rule);
                            });
                        }
                    },
                );
            },
        );
    }
}

fn main() -> anyhow::Result<()> {
    run(
        shared::QuitOnEscape { app: StateAndIds },
        options("Kasumi — state and identity", 1120.0, 880.0),
    )
}
