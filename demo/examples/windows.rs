//! `windows` — multi-window applications with a shared `App`.
//!
//! ```sh
//! cargo run -p kasumi-demo --example windows
//! ```
//!
//! The model is: **one `App`, many windows**. The backend owns the platform
//! windows and gives each one its own `Runtime`, GPU surface, `StateStore` and
//! resource store. The framework calls the same `view` for every window, so the
//! application distinguishes them with `ui.window()`.
//!
//! Consequences worth knowing before designing around this:
//!
//! - `State<T>` is per window. The same call site in two windows yields two
//!   independent values. To share data across windows, keep it in `self`, which
//!   *is* shared (usually behind an `Arc<Mutex<_>>`).
//! - Window commands are applied after the frame that emitted them. A window
//!   opened during frame N first appears to other windows on frame N+1.
//! - Closing the current window ends its frame loop after presentation; the rest
//!   of that `view` still runs.
//!
//! ```text
//!                     ┌──────────────┐
//!      &mut self ────▶│  App::view   │──── declares ────┐
//!                     └──────────────┘                  │
//!         ▲                  ▲                          ▼
//!         │                  │            window 1: Runtime + StateStore
//!         │                  │            window 2: Runtime + StateStore
//!      shared            per-window  ◀─── window 3: Runtime + StateStore
//! ```

use std::sync::{Arc, Mutex};

use kasumi::App;
use kasumi::prelude::*;
use kasumi_backend::run;

#[path = "shared/mod.rs"]
mod shared;

use shared::{code, heading, micro, note, options};

/// Data every window shares, kept off the per-window state store.
#[derive(Default)]
struct Shared {
    notes: Vec<String>,
    opened: u32,
}

struct Windows {
    shared: Arc<Mutex<Shared>>,
}

impl Windows {
    fn new() -> Self {
        Self {
            shared: Arc::new(Mutex::new(Shared {
                notes: vec!["Windows share the App, not the StateStore.".to_owned()],
                opened: 1,
            })),
        }
    }
}

impl App for Windows {
    fn view(&mut self, ui: &mut Ui) {
        let theme = ui.theme().clone();
        let window = ui.window().clone();

        // Per-window state. The same source line produces a different slot in
        // each window, which is the point.
        let local_count = ui.state(|| 0u32);
        let draft = ui.state(String::new);

        // The inspector window gets a different layout.
        if !window.is_primary() {
            ui.size_full().background(theme.colors.background);
            ui.column(|ui| {
                ui.size_full()
                    .padding(24.0)
                    .gap(14.0)
                    .background(theme.colors.surface);
                ui.add(
                    Label::new("SECONDARY WINDOW")
                        .font_size(10.0)
                        .color(theme.colors.primary)
                        .family(FontFamily::Monospace),
                );
                ui.add(
                    Label::new("Inspector")
                        .style(theme.typography.headline_small.clone())
                        .color(theme.colors.on_surface),
                );
                note(
                    ui,
                    "This view is declared by the same `App::view`, dispatched on \
                     `ui.window().is_primary()`.",
                );
                ui.divider();
                for (label, value) in [
                    ("key", format!("{}", window.key)),
                    ("title", window.title.clone()),
                    (
                        "size",
                        format!(
                            "{:.0} × {:.0} @ {:.2}×",
                            window.size.width, window.size.height, window.scale_factor
                        ),
                    ),
                    ("focused", window.focused.to_string()),
                    ("windows", ui.windows().len().to_string()),
                ] {
                    ui.row(|ui| {
                        ui.width_full().gap(10.0);
                        ui.column(|ui| {
                            ui.width(74.0).flex_shrink(0.0);
                            micro(ui, label);
                        });
                        ui.column(|ui| {
                            ui.grow(1.0);
                            code(ui, value);
                        });
                    });
                }
                ui.divider();

                micro(ui, "per-window state");
                ui.row(|ui| {
                    ui.width_full().gap(8.0).align_items(Align::Center);
                    if ui.add(Button::new("−").tonal()).clicked() {
                        local_count.update(|value| *value = value.saturating_sub(1));
                    }
                    ui.add(
                        Label::new(local_count.get().to_string())
                            .font_size(28.0)
                            .color(theme.colors.on_surface)
                            .semibold()
                            .family(FontFamily::Monospace),
                    );
                    if ui.add(Button::new("+").primary()).clicked() {
                        local_count.update(|value| *value += 1);
                    }
                    ui.spacer();
                    micro(ui, "resets when this window closes");
                });

                ui.spacer();
                ui.row(|ui| {
                    ui.width_full().justify(Justify::End).gap(8.0);
                    if ui.add(Button::new("Close").primary()).clicked() {
                        ui.close_window(window.key);
                    }
                });
            });
            return;
        }

        // ---------------------------------------------------------------
        // Primary window
        // ---------------------------------------------------------------
        let shared_notes = self
            .shared
            .lock()
            .map(|shared| (shared.notes.clone(), shared.opened))
            .unwrap_or_default();

        shared::page(
            ui,
            "Multiple windows",
            "one App · one Runtime per window · shared data via self",
            "windows",
            |ui| {
                // Three metric tiles. No widget for this: a tinted column with
                // three labels is the whole thing, and keeping it inline shows
                // how little a "stat card" actually is.
                ui.row(|ui| {
                    ui.width_full().gap(12.0);
                    let live = ui.windows().len();
                    for (kicker, value, detail) in [
                        (
                            "PER-WINDOW STATE",
                            local_count.get().to_string(),
                            "independent StateStore",
                        ),
                        ("LIVE WINDOWS", live.to_string(), "ui.windows()"),
                        (
                            "SHARED NOTES",
                            shared_notes.0.len().to_string(),
                            "Arc<Mutex<_>> on self",
                        ),
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
                                    .font_size(22.0)
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

                ui.row(|ui| {
                    ui.width_full().gap(16.0).align_items(Align::Stretch);

                    ui.column(|ui| {
                        ui.grow(1.0).basis(0.0);
                        ui.card(
                            Card::elevated()
                                .padding(theme.spacing.lg)
                                .gap(theme.spacing.md),
                            |ui| {
                                heading(ui, "Open and manage windows", "Ui::open_window");
                                note(
                                    ui,
                                    "The request is queued and applied after this frame presents, \
                                     so a new window shows up in `ui.windows()` on the next \
                                     frame rather than during this one.",
                                );
                                ui.wrap_row(|ui| {
                                    ui.width_full().gap(8.0);
                                    if ui.add(Button::new("Open inspector").primary()).clicked() {
                                        ui.open_window(
                                            WindowRequest::new("Kasumi — Inspector")
                                                .size(520.0, 620.0)
                                                .min_size(320.0, 280.0)
                                                .theme(theme.clone()),
                                        );
                                        if let Ok(mut shared) = self.shared.lock() {
                                            shared.opened += 1;
                                        }
                                    }
                                    if ui.add(Button::new("Open as dark").tonal()).clicked() {
                                        ui.open_window(
                                            WindowRequest::new("Kasumi — Dark inspector")
                                                .size(420.0, 520.0)
                                                .theme(Theme::dark()),
                                        );
                                    }
                                });
                                ui.divider();
                                for candidate in ui.windows().to_vec() {
                                    ui.row(|ui| {
                                        ui.width_full().gap(8.0).align_items(Align::Center);
                                        ui.add(
                                            Label::new(format!(
                                                "{} {:<10} {}",
                                                if candidate.is_primary() { "●" } else { "○" },
                                                format!("{}", candidate.key),
                                                candidate.title
                                            ))
                                            .font_size(11.0)
                                            .color(theme.colors.on_surface_variant)
                                            .family(FontFamily::Monospace),
                                        );
                                        ui.spacer();
                                        if candidate.focused {
                                            micro(ui, "focused");
                                        }
                                        if !candidate.is_primary() {
                                            if ui
                                                .add(
                                                    Button::new("Focus")
                                                        .subtle()
                                                        .font_size(10.0)
                                                        .padding_xy(8.0, 4.0),
                                                )
                                                .clicked()
                                            {
                                                ui.focus_window(candidate.key);
                                            }
                                            if ui
                                                .add(
                                                    Button::new("Close")
                                                        .subtle()
                                                        .font_size(10.0)
                                                        .padding_xy(8.0, 4.0),
                                                )
                                                .clicked()
                                            {
                                                ui.close_window(candidate.key);
                                            }
                                        }
                                    });
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
                                heading(ui, "Shared data", "self + Arc<Mutex<_>>");
                                note(
                                    ui,
                                    "Everything the windows must agree on lives in the `App`, \
                                     which is shared. Mutating it here is visible in every other \
                                     window on its next frame.",
                                );
                                ui.row(|ui| {
                                    ui.width_full().gap(8.0).align_items(Align::Center);
                                    ui.add(
                                        TextInput::new(&draft)
                                            .placeholder("A note for every window")
                                            .width(Length::Percent(100.0)),
                                    );
                                    if ui
                                        .add(
                                            Button::new("Add")
                                                .primary()
                                                .enabled(!draft.get().trim().is_empty()),
                                        )
                                        .clicked()
                                    {
                                        let text = draft.get();
                                        if let Ok(mut shared) = self.shared.lock() {
                                            shared.notes.push(text);
                                        }
                                        draft.set(String::new());
                                        ui.request_repaint();
                                    }
                                });
                                for line in &shared_notes.0 {
                                    ui.row(|ui| {
                                        ui.width_full().gap(6.0);
                                        micro(ui, "·");
                                        ui.add(
                                            Label::new(line)
                                                .font_size(11.5)
                                                .color(theme.colors.on_surface_variant),
                                        );
                                    });
                                }
                                if shared_notes.0.is_empty() {
                                    micro(ui, "no shared notes yet");
                                }
                            },
                        );
                    });
                });

                ui.card(
                    Card::outlined()
                        .padding(theme.spacing.lg)
                        .gap(theme.spacing.md),
                    |ui| {
                        heading(ui, "What is shared and what is not", "architecture");
                        ui.grid(2, |ui| {
                            ui.width_full().gap(14.0);
                            for (title, lines) in [
                                (
                                    "Shared",
                                    vec![
                                        "The `App` value behind `&mut self`.",
                                        "The text engine and font database.",
                                        "Window commands queued through `Ui`.",
                                    ],
                                ),
                                (
                                    "Per window",
                                    vec![
                                        "`StateStore` — every `State<T>`.",
                                        "The `ResourceStore` and its cache.",
                                        "Theme, layout engine and GPU surface.",
                                    ],
                                ),
                            ] {
                                ui.column(|ui| {
                                    ui.gap(5.0);
                                    code(ui, title);
                                    for line in lines {
                                        ui.add(
                                            Label::new(format!("· {line}"))
                                                .font_size(11.0)
                                                .color(theme.colors.on_surface_variant),
                                        );
                                    }
                                });
                            }
                        });
                    },
                );
            },
        );
    }
}

fn main() -> anyhow::Result<()> {
    run(
        Windows::new(),
        options("Kasumi — multiple windows", 1120.0, 860.0),
    )
}
