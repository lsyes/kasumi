//! `dialogs` — modal dialogs, scrims, Escape handling and nested confirmation.
//!
//! ```sh
//! cargo run -p kasumi-demo --example dialogs
//! ```
//!
//! `Ui::dialog` declares a root overlay with three properties that are easy to
//! get wrong by hand:
//!
//! 1. **It blocks the page.** The scrim is a full-window node that swallows
//!    pointer and wheel input, so controls underneath cannot be clicked even
//!    though they are still in the tree.
//! 2. **It paints above everything.** Overlays are declared as children of the
//!    root after the page body, so they win the z-order without a z-index.
//! 3. **It dismisses itself.** With `dismissible(true)`, Escape and a click on
//!    the scrim both write `false` to the dialog's state.
//!
//! Focus trapping is deliberately *not* included. Tabbing can still reach page
//! controls; manage focus yourself when a flow needs a hard trap.

use kasumi::App;
use kasumi::prelude::*;
use kasumi_backend::run;

#[path = "shared/mod.rs"]
mod shared;

use shared::{code, heading, micro, note, options};

struct Dialogs;

impl App for Dialogs {
    fn view(&mut self, ui: &mut Ui) {
        let theme = ui.theme().clone();

        let simple = ui.state(|| false);
        let form = ui.state(|| false);
        let blocking = ui.state(|| false);
        let project = ui.state(|| "Kasumi".to_owned());
        let draft = ui.state(|| "Kasumi".to_owned());
        let deleted = ui.state(|| 0u32);
        let events = ui.state(Vec::<String>::new);

        // Dialogs are closures that borrow `Ui`, so the only way information
        // leaves one is by writing a `State` the page can read next frame.
        let log = |line: String| {
            events.update(|lines| {
                lines.insert(0, line);
                lines.truncate(6);
            });
        };

        shared::page(
            ui,
            "Dialogs",
            "Dialog · scrim blocking · Escape · confirmations",
            "dialogs",
            |ui| {
                ui.row(|ui| {
                    ui.width_full().gap(16.0).align_items(Align::Stretch);

                    ui.column(|ui| {
                        ui.grow(1.0).basis(0.0).gap(16.0);

                        ui.card(
                            Card::elevated()
                                .padding(theme.spacing.lg)
                                .gap(theme.spacing.md),
                            |ui| {
                                heading(ui, "Three flavours", "Dialog::new");
                                note(
                                    ui,
                                    "Each button opens a dialog with different configuration. \
                                     Notice that the page underneath stays laid out and visible \
                                     through the scrim, but is not interactive.",
                                );
                                ui.wrap_row(|ui| {
                                    ui.width_full().gap(8.0);
                                    if ui.add(Button::new("Simple").primary()).clicked() {
                                        simple.set(true);
                                    }
                                    if ui.add(Button::new("With a form").tonal()).clicked() {
                                        form.set(true);
                                        draft.set(project.get());
                                    }
                                    if ui
                                        .add(Button::new("Non-dismissible").outlined())
                                        .clicked()
                                    {
                                        blocking.set(true);
                                    }
                                });
                                ui.divider();
                                micro(ui, "current project");
                                code(ui, project.get());
                                micro(ui, format!("deleted {} time(s)", deleted.get()));
                            },
                        );

                        ui.card(
                            Card::outlined()
                                .padding(theme.spacing.lg)
                                .gap(theme.spacing.md),
                            |ui| {
                                heading(ui, "How dismissal works", "Dialog::dismissible");
                                for line in [
                                    "Escape writes false when dismissible(true), which is the default.",
                                    "A click on the scrim does the same.",
                                    "Buttons inside the body close the dialog explicitly.",
                                    "dismissible(false) leaves only the explicit buttons — note the",
                                    "runtime also clears focus on Escape, so a hard trap needs care.",
                                ] {
                                    ui.add(
                                        Label::new(format!("· {line}"))
                                            .font_size(11.5)
                                            .color(theme.colors.on_surface_variant),
                                    );
                                }
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
                                heading(ui, "Event log", "frame-local");
                                note(
                                    ui,
                                    "A rolling list kept in a `State<Vec<String>>`. It survives \
                                     frames and is updated from inside dialog bodies, which is \
                                     the only way information leaves a dialog closure.",
                                );
                                let lines = events.get();
                                if lines.is_empty() {
                                    ui.add(
                                        Label::new("No dialog interaction yet.")
                                            .font_size(12.0)
                                            .color(theme.colors.outline),
                                    );
                                }
                                for (index, line) in lines.iter().enumerate() {
                                    ui.row(|ui| {
                                        ui.width_full().gap(8.0).align_items(Align::Center);
                                        ui.column(|ui| {
                                            ui.width(22.0).flex_shrink(0.0);
                                            micro(ui, format!("{:02}", index));
                                        });
                                        ui.add(
                                            Label::new(line)
                                                .font_size(11.5)
                                                .color(theme.colors.on_surface_variant),
                                        );
                                    });
                                }
                            },
                        );
                    });
                });
            },
        );

        // ---------------------------------------------------------------
        // Simple dialog
        // ---------------------------------------------------------------
        // `ui.dialog` returns `Some(response)` while it is open and `None`
        // while closed. The dialog itself writes `false` to `simple` on an
        // Escape or a scrim click; the buttons below close it explicitly.
        let _ = ui.dialog(
            Dialog::new(&simple)
                .title("Discard changes?")
                .description(
                    "The scrim blocks the page and Escape closes this dialog. \
                         Nothing outside remembers that it was open.",
                )
                .width(440.0),
            |ui| {
                ui.row(|ui| {
                    ui.width_full().justify(Justify::End).gap(8.0);
                    if ui.add(Button::new("Keep editing").subtle()).clicked() {
                        simple.set(false);
                        log("simple · kept editing".to_owned());
                    }
                    if ui.add(Button::new("Discard").primary()).clicked() {
                        simple.set(false);
                        log("simple · discarded".to_owned());
                    }
                });
            },
        );

        // ---------------------------------------------------------------
        // Dialog with a form
        // ---------------------------------------------------------------
        if ui
            .dialog(
                Dialog::new(&form)
                    .title("Rename project")
                    .description("The body is ordinary immediate-mode content.")
                    .width(460.0),
                |ui| {
                    micro(ui, "project name");
                    ui.add(
                        TextInput::new(&draft)
                            .placeholder("Project name")
                            .width(Length::Percent(100.0)),
                    );
                    ui.add(Checkbox::new("Also rename the remote", &simple));
                    ui.divider();
                    ui.row(|ui| {
                        ui.width_full().justify(Justify::End).gap(8.0);
                        if ui.add(Button::new("Cancel").subtle()).clicked() {
                            form.set(false);
                            log("rename · cancelled".to_owned());
                        }
                        if ui
                            .add(
                                Button::new("Save")
                                    .primary()
                                    .enabled(!draft.get().trim().is_empty()),
                            )
                            .clicked()
                        {
                            let next = draft.get();
                            project.set(next.clone());
                            form.set(false);
                            log(format!("rename · saved \"{next}\""));
                        }
                    });
                },
            )
            .is_some()
        {}

        // ---------------------------------------------------------------
        // Non-dismissible dialog
        // ---------------------------------------------------------------
        ui.dialog(
            Dialog::new(&blocking)
                .title("Confirm deletion")
                .description(
                    "This one has dismissible(false): Escape and the scrim do nothing. \
                     The only ways out are the two buttons.",
                )
                .width(440.0)
                .dismissible(false),
            |ui| {
                ui.row(|ui| {
                    ui.width_full().justify(Justify::End).gap(8.0);
                    if ui.add(Button::new("Cancel").subtle()).clicked() {
                        blocking.set(false);
                        log("delete · cancelled".to_owned());
                    }
                    if ui.add(Button::new("Delete").primary()).clicked() {
                        blocking.set(false);
                        deleted.update(|count| *count += 1);
                        log("delete · confirmed".to_owned());
                    }
                });
            },
        );
    }
}

fn main() -> anyhow::Result<()> {
    run(
        shared::QuitOnEscape { app: Dialogs },
        options("Kasumi — dialogs", 1080.0, 840.0),
    )
}
