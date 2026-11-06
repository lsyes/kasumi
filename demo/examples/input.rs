//! `input` — raw keys, pointer state, focus and the event-driven loop.
//!
//! ```sh
//! cargo run -p kasumi-demo --example input
//! ```
//!
//! Widgets consume most input for you, but `Ui` exposes the frame's input state
//! directly for custom controls and shortcuts:
//!
//! - `ui.key_pressed(&Key::Character("k".into()))` — one-shot per key-down.
//! - `ui.key_modifiers(..)` — the modifiers that accompanied it.
//! - `ui.input().pointer` / `modifiers` / `viewport` — the current frame.
//! - `Response::{hovered, pressed, dragged, clicked, focused, context_clicked}`.
//!
//! Note that `Response` geometry is from the *previous* layout pass, because
//! layout for the current frame cannot run until `view` returns. Hit testing
//! against the previous rect is an explicit property of the immediate surface.
//!
//! The frame counter at the bottom proves the event-driven loop: pressing a
//! key produces a frame, then the window sleeps until the next input event.

use kasumi::App;
use kasumi::core::{Key, Modifiers};
use kasumi::prelude::*;
use kasumi_backend::run;

#[path = "shared/mod.rs"]
mod shared;

use shared::{code, heading, micro, note, options};

/// One entry in the shortcut log.
#[derive(Clone)]
struct Entry {
    key: String,
    detail: String,
}

fn push_log(log: &State<Vec<Entry>>, key: String, detail: String) {
    log.update(|lines| {
        lines.insert(0, Entry { key, detail });
        lines.truncate(9);
    });
}

/// A pointer-sensitive surface using `ui.clickable()` and `Response` flags.
fn hit_target(ui: &mut Ui) -> Response {
    let theme = ui.theme().clone();
    ui.column(|ui| {
        ui.clickable();
        ui.width_full()
            .height(170.0)
            .align_items(Align::Center)
            .justify(Justify::Center)
            .gap(6.0)
            .radius(theme.shape.medium)
            .border(1.0, theme.colors.outline_variant)
            .background(theme.colors.surface_container_low);
        ui.add(
            Label::new("Click, drag or right-click inside this box")
                .font_size(14.0)
                .color(theme.colors.on_surface),
        );
        micro(ui, "the full Response is available to the caller");
    })
}

struct Input;

impl App for Input {
    fn view(&mut self, ui: &mut Ui) {
        let theme = ui.theme().clone();
        let log = ui.state(Vec::<Entry>::new);
        let text = ui.state(String::new);
        let toggles = ui.state(|| 0u32);

        // Global shortcuts, independent of focus. `key_modifiers` returns the
        // modifiers that accompanied the key-down this frame.
        let shortcuts = [("k", "K"), ("s", "S"), ("d", "D")];
        for (character, label) in shortcuts {
            let key = Key::Character(character.to_owned());
            if ui.key_pressed(&key) {
                let mods = ui.key_modifiers(&key).unwrap_or_default();
                let prefix = if mods.ctrl && mods.shift {
                    "Ctrl+Shift+"
                } else if mods.ctrl {
                    "Ctrl+"
                } else if mods.shift {
                    "Shift+"
                } else {
                    ""
                };
                push_log(
                    &log,
                    format!("{prefix}{label}"),
                    format!(
                        "key_pressed · ctrl={} shift={} alt={}",
                        mods.ctrl, mods.shift, mods.alt
                    ),
                );
            }
        }

        // Named keys are separate from printable text, so a shortcut works
        // regardless of the keyboard layout.
        for key in [
            Key::Enter,
            Key::Space,
            Key::Escape,
            Key::ArrowUp,
            Key::ArrowDown,
        ] {
            if ui.key_pressed(&key) {
                push_log(
                    &log,
                    format!("{key:?}"),
                    "named key · layout independent".to_owned(),
                );
            }
        }

        shared::page(
            ui,
            "Input and events",
            "key_pressed · key_modifiers · Response flags · event-driven frames",
            "input",
            |ui| {
                ui.row(|ui| {
                    ui.width_full().gap(16.0).align_items(Align::Stretch);

                    ui.column(|ui| {
                        ui.grow(1.2).basis(0.0);
                        ui.card(
                            Card::elevated()
                                .padding(theme.spacing.lg)
                                .gap(theme.spacing.md),
                            |ui| {
                                heading(ui, "Raw pointer state", "ui.input()");
                                note(
                                    ui,
                                    "The values below are a snapshot for this frame. Move the \
                                     pointer and hold a modifier to see them change.",
                                );
                                // Copy the frame's input snapshot out: `ui.input()`
                                // borrows `Ui` immutably, so it cannot be held across
                                // a widget declaration that needs `&mut Ui`.
                                let (pointer, viewport, scale_factor, pointer_inside, modifiers) = {
                                    let input = ui.input();
                                    (
                                        input.pointer,
                                        input.viewport,
                                        input.scale_factor,
                                        input.pointer_inside,
                                        input.modifiers,
                                    )
                                };
                                let snapshot = format!(
                                    "pointer ({:.0}, {:.0}) · viewport {:.0}×{:.0} · scale {:.2} · inside {}",
                                    pointer.x,
                                    pointer.y,
                                    viewport.width,
                                    viewport.height,
                                    scale_factor,
                                    pointer_inside
                                );
                                code(ui, snapshot);
                                let Modifiers { shift, ctrl, alt, logo } = modifiers;
                                ui.row(|ui| {
                                    ui.width_full().gap(8.0);
                                    for (label, active) in [
                                        ("shift", shift),
                                        ("ctrl", ctrl),
                                        ("alt", alt),
                                        ("logo", logo),
                                    ] {
                                        ui.row(|ui| {
                                            ui.padding_xy(9.0, 4.0)
                                                .radius(theme.shape.extra_small)
                                                .background(if active {
                                                    theme.colors.primary_container
                                                } else {
                                                    theme.colors.surface_container_high
                                                });
                                            micro(ui, label);
                                        });
                                    }
                                });
                                ui.divider();

                                let target = hit_target(ui);
                                if target.clicked() {
                                    push_log(&log, "clicked".to_owned(), "Response::clicked".to_owned());
                                    toggles.update(|value| *value += 1);
                                }
                                if target.dragged() {
                                    push_log(&log, "dragged".to_owned(), "Response::dragged".to_owned());
                                }
                                if target.context_clicked() {
                                    push_log(
                                        &log,
                                        "context".to_owned(),
                                        "Response::context_clicked".to_owned(),
                                    );
                                }
                                code(
                                    ui,
                                    format!(
                                        "hover={} press={} focus={} clicks={}",
                                        target.hovered(),
                                        target.pressed(),
                                        target.focused(),
                                        toggles.get()
                                    ),
                                );
                                note(
                                    ui,
                                    "The rect behind a `Response` comes from the previous layout \
                                     pass. That one-frame delay is why hit testing is resolved \
                                     before `view` runs.",
                                );
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
                                heading(ui, "Shortcuts", "Key::Character");
                                note(
                                    ui,
                                    "Try K, S, D with and without Ctrl/Shift, plus arrows, \
                                     Enter, Space and F1. Printable keys are matched by their \
                                     physical character, not by the text they produce.",
                                );
                                let entries = log.get();
                                if entries.is_empty() {
                                    ui.add(
                                        Label::new("No keys yet.")
                                            .font_size(12.0)
                                            .color(theme.colors.outline),
                                    );
                                }
                                for entry in &entries {
                                    ui.row(|ui| {
                                        ui.width_full().gap(8.0).align_items(Align::Center);
                                        ui.column(|ui| {
                                            ui.width(104.0).flex_shrink(0.0);
                                            code(ui, &entry.key);
                                        });
                                        ui.add(
                                            Label::new(&entry.detail)
                                                .font_size(10.0)
                                                .color(theme.colors.on_surface_variant),
                                        );
                                    });
                                }
                                ui.divider();
                                ui.row(|ui| {
                                    ui.width_full().align_items(Align::Center);
                                    micro(ui, "frame");
                                    ui.spacer();
                                    code(ui, ui.frame().to_string());
                                });
                                note(
                                    ui,
                                    "The frame counter stops when the window is idle: no timer, no \
                                     polling, no frames. Any input event wakes the loop.",
                                );
                            },
                        );
                    });
                });

                ui.card(
                    Card::outlined()
                        .padding(theme.spacing.lg)
                        .gap(theme.spacing.md),
                    |ui| {
                        heading(ui, "Focus and tab order", "Sense::click()");
                        note(
                            ui,
                            "Focusable controls are discovered from their `Sense`. Tab and \
                             Shift+Tab cycle them, Enter or Space activates a focused button, \
                             and Escape clears focus. Disabled controls leave tab order.",
                        );
                        ui.wrap_row(|ui| {
                            ui.width_full().gap(8.0);
                            ui.add(Button::new("First").tonal());
                            ui.add(Button::new("Second").outlined());
                            ui.add(Button::new("Disabled").filled().enabled(false));
                            ui.add(
                                TextInput::new(&text)
                                    .placeholder("A text field")
                                    .width(220.0),
                            );
                            ui.add(Button::new("Last").subtle());
                        });
                    },
                );
            },
        );
    }
}

fn main() -> anyhow::Result<()> {
    run(
        shared::QuitOnEscape { app: Input },
        options("Kasumi — input", 1120.0, 860.0),
    )
}
