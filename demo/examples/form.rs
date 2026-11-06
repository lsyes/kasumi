//! `form` — text inputs, validation, checkboxes, radios, combo boxes, dialogs.
//!
//! ```sh
//! cargo run -p kasumi-demo --example form
//! ```
//!
//! Every control is bound to a `State<T>` you own. There is no two-way binding
//! magic and no per-widget controller object: the widget writes the state and
//! reports `changed()` for the frame it happened in, and you read the state to
//! validate or submit.
//!
//! The submit path below is deliberately boring: validate on every frame from
//! the current values, and only enable the button when the form is valid. That
//! is the whole pattern.

use kasumi::App;
use kasumi::prelude::*;
use kasumi_backend::run;

#[path = "shared/mod.rs"]
mod shared;

use shared::{code, heading, micro, note, options};

/// A validation problem attached to one field.
struct Problem {
    field: &'static str,
    message: String,
}

fn validate(name: &str, email: &str, plan: usize, accepted: bool) -> Vec<Problem> {
    let mut problems = Vec::new();
    if name.trim().len() < 2 {
        problems.push(Problem {
            field: "name",
            message: "Name needs at least 2 characters.".to_owned(),
        });
    }
    if !email.contains('@') || !email.split('@').nth(1).is_some_and(|d| d.contains('.')) {
        problems.push(Problem {
            field: "email",
            message: "Email needs a local part and a dotted domain.".to_owned(),
        });
    }
    if plan > 2 {
        problems.push(Problem {
            field: "plan",
            message: "Unknown plan.".to_owned(),
        });
    }
    if !accepted {
        problems.push(Problem {
            field: "terms",
            message: "Accept the terms to continue.".to_owned(),
        });
    }
    problems
}

/// A labelled field wrapper. The framework has no `FormField` widget: a form is
/// a column of a micro label plus the control, which keeps each control free to
/// own its own layout.
fn field(ui: &mut Ui, label: &str, error: Option<&str>, body: impl FnOnce(&mut Ui)) {
    let theme = ui.theme().clone();
    ui.column(|ui| {
        ui.width_full().gap(5.0);
        ui.row(|ui| {
            ui.width_full().gap(6.0).align_items(Align::Center);
            micro(ui, label);
            if let Some(error) = error {
                ui.spacer();
                ui.add(
                    Label::new(error)
                        .font_size(10.0)
                        .color(theme.colors.error)
                        .wrap(TextWrap::None),
                );
            }
        });
        body(ui);
    });
}

struct Form;

impl App for Form {
    fn view(&mut self, ui: &mut Ui) {
        let theme = ui.theme().clone();

        let name = ui.state(|| "Kasumi".to_owned());
        let email = ui.state(|| String::new());
        let bio = ui.state(|| String::new());
        let plan = ui.state(|| 0usize);
        let newsletter = ui.state(|| true);
        let accepted = ui.state(|| false);
        let submitted = ui.state(|| 0u32);
        let confirm_open = ui.state(|| false);

        let name_value = name.get();
        let email_value = email.get();
        let problems = validate(&name_value, &email_value, plan.get(), accepted.get());
        let valid = problems.is_empty();
        let error_for = |field: &str| {
            problems
                .iter()
                .find(|problem| problem.field == field)
                .map(|problem| problem.message.as_str())
        };

        shared::page(
            ui,
            "Forms",
            "State<T> bound controls · live validation · modal confirmation",
            "form",
            |ui| {
                ui.row(|ui| {
                    ui.width_full().gap(16.0).align_items(Align::Stretch);

                    // -------------------------------------------------------
                    // Left column: the form itself
                    // -------------------------------------------------------
                    ui.column(|ui| {
                        ui.grow(1.35).basis(0.0);
                        ui.card(
                            Card::elevated()
                                .padding(theme.spacing.lg)
                                .gap(theme.spacing.md),
                            |ui| {
                                heading(ui, "Create an account", "TextInput · State<String>");
                                note(
                                    ui,
                                    "Type in the fields. Validation runs from the current values \
                                     every frame, so the error text and the submit button update \
                                     as you type.",
                                );

                                field(ui, "NAME", error_for("name"), |ui| {
                                    ui.add(
                                        TextInput::new(&name)
                                            .placeholder("Your name")
                                            .width(Length::Percent(100.0)),
                                    );
                                });

                                field(ui, "EMAIL", error_for("email"), |ui| {
                                    ui.add(
                                        TextInput::new(&email)
                                            .placeholder("you@example.com")
                                            .width(Length::Percent(100.0)),
                                    );
                                });

                                field(ui, "BIO", None, |ui| {
                                    ui.add(
                                        TextInput::new(&bio)
                                            .placeholder(
                                                "Single line; the engine has no \
                                                          multiline field yet",
                                            )
                                            .width(Length::Percent(100.0)),
                                    );
                                });
                                code(
                                    ui,
                                    format!("{} characters typed", bio.with(|b| b.chars().count())),
                                );

                                field(ui, "PLAN", None, |ui| {
                                    ui.add(
                                        ComboBox::new(["Free", "Team", "Studio"], &plan)
                                            .width(Length::Percent(100.0)),
                                    );
                                });

                                ui.divider();

                                ui.add(Checkbox::new("Send me the release notes", &newsletter));
                                ui.add(Checkbox::new("I accept the terms of service", &accepted));

                                ui.divider();

                                ui.row(|ui| {
                                    ui.width_full().gap(8.0).align_items(Align::Center);
                                    if ui
                                        .add(Button::new("Create account").primary().enabled(valid))
                                        .clicked()
                                    {
                                        confirm_open.set(true);
                                    }
                                    if ui.add(Button::new("Reset").subtle()).clicked() {
                                        name.set(String::new());
                                        email.set(String::new());
                                        bio.set(String::new());
                                        plan.set(0);
                                        accepted.set(false);
                                        newsletter.set(true);
                                    }
                                    ui.spacer();
                                    if valid {
                                        micro(ui, "valid");
                                    } else {
                                        ui.add(
                                            Label::new(format!("{} problem(s)", problems.len()))
                                                .font_size(9.0)
                                                .color(theme.colors.error)
                                                .family(FontFamily::Monospace),
                                        );
                                    }
                                });
                            },
                        );
                    });

                    // -------------------------------------------------------
                    // Right column: state inspector + notes
                    // -------------------------------------------------------
                    ui.column(|ui| {
                        ui.grow(1.0).basis(0.0).gap(16.0);

                        ui.card(
                            Card::filled()
                                .padding(theme.spacing.lg)
                                .gap(theme.spacing.md),
                            |ui| {
                                heading(ui, "Live state", "reading State<T>");
                                note(
                                    ui,
                                    "What the form would submit right now. Nothing here is a \
                                     special form object; these are the same handles the \
                                     controls write to.",
                                );
                                for (key, value) in [
                                    ("name", name_value.clone()),
                                    ("email", email_value.clone()),
                                    (
                                        "plan",
                                        format!(
                                            "{}",
                                            ["Free", "Team", "Studio"][plan.get().min(2)]
                                        ),
                                    ),
                                    ("newsletter", newsletter.get().to_string()),
                                    ("accepted", accepted.get().to_string()),
                                    (
                                        "submitted",
                                        format!("{} times · frame {}", submitted.get(), ui.frame()),
                                    ),
                                ] {
                                    ui.row(|ui| {
                                        ui.width_full().gap(8.0);
                                        ui.column(|ui| {
                                            ui.width(84.0).flex_shrink(0.0);
                                            micro(ui, key);
                                        });
                                        ui.column(|ui| {
                                            ui.grow(1.0);
                                            code(
                                                ui,
                                                if value.is_empty() {
                                                    "—".to_owned()
                                                } else {
                                                    value
                                                },
                                            );
                                        });
                                    });
                                }
                            },
                        );

                        ui.card(
                            Card::outlined()
                                .padding(theme.spacing.lg)
                                .gap(theme.spacing.md),
                            |ui| {
                                heading(ui, "Keyboard behaviour", "built into the controls");
                                for line in [
                                    "Click a field to focus it; Ctrl/Cmd+A selects all.",
                                    "Shift+arrows extend a selection; Ctrl+Z undoes.",
                                    "Tab and Shift+Tab move focus between controls.",
                                    "Space toggles a focused checkbox.",
                                    "Enter submits nothing automatically — wire it yourself.",
                                    "Esc clears focus unless a dialog is open.",
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
                });
            },
        );

        // The dialog is a root overlay: it paints above the page, blocks input
        // to it, and closes on Escape or a scrim click.
        let summary_name = name_value.clone();
        let summary_email = email_value.clone();
        let chosen_plan = ["Free", "Team", "Studio"][plan.get().min(2)];
        if ui
            .dialog(
                Dialog::new(&confirm_open)
                    .title("Confirm account")
                    .description("This is a modal dialog rendered as a root overlay.")
                    .width(460.0),
                |ui| {
                    for (label, value) in [
                        ("Name", summary_name.as_str()),
                        ("Email", summary_email.as_str()),
                        ("Plan", chosen_plan),
                    ] {
                        ui.row(|ui| {
                            ui.width_full().gap(10.0).align_items(Align::Center);
                            ui.column(|ui| {
                                ui.width(70.0).flex_shrink(0.0);
                                micro(ui, label);
                            });
                            code(ui, value);
                        });
                    }
                    ui.divider();
                    ui.row(|ui| {
                        ui.width_full().justify(Justify::End).gap(8.0);
                        if ui.add(Button::new("Cancel").subtle()).clicked() {
                            confirm_open.set(false);
                        }
                        if ui.add(Button::new("Confirm").primary()).clicked() {
                            confirm_open.set(false);
                            submitted.update(|count| *count += 1);
                        }
                    });
                },
            )
            .is_some()
        {
            // A click on the scrim already closed the dialog by the time this
            // runs; the returned response is the dialog card's, not the scrim's.
        }
    }
}

fn main() -> anyhow::Result<()> {
    run(Form, options("Kasumi — form", 1120.0, 860.0))
}
